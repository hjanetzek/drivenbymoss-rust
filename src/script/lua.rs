// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Lua back-end of the controller scripts (Lua 5.4 through mlua, doc/LUA_PLAN.md §2–§3).
//!
//! - [`Engine`]: one Lua state per controller instance (plus one for reading the metadata of a
//!   script). Only the safe standard libraries are loaded (`string`, `table`, `math`, `utf8`);
//!   `require` loads modules from the folder of the script only. A memory limit and an
//!   instruction budget per call protect the host from a broken script.
//! - Every call into the script goes through [`Engine::call_mut`] or [`Engine::call_ref`]: the
//!   context is put on the [`CtxStack`] for the call, an error is logged with the script and the
//!   line (`Host::error`), the call has no effect, and a function which fails three times in a
//!   row is disabled. A panic is caught as well.
//! - [`bindings`]: what the script sees (`ctx`, the handles of the model and the surface).
//! - [`adapters`]: Lua tables as modes, views, parameters and commands.
//! - [`LuaControllerFactory`] and [`LuaSetup`]: a script as a controller.
//!
//! Differences to the plan: errors are logged with `Host::error` (not `println`), so that the
//! hosts and the conformance runner see them.

mod adapters;
mod bindings;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::{Rc, Weak};

use mlua::{AnyUserData, FromLuaMulti, Function, HookTriggers, IntoLuaMulti, Lua, LuaOptions, StdLib, Table, Value, VmState};

use super::ctx_stack::CtxStack;
use super::errors::{short_message, ErrorPolicy, MAX_ERRORS_IN_A_ROW};
use super::ScriptSource;
use crate::backend::Sender;
use crate::config::{Configuration, Scope};
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, ControllerSetup, InstanceParts, MidiPorts, OperatingSystem};
use crate::daw::model::Model;
use crate::framework::ctx::Ctx;
use crate::framework::surface::Surface;
use crate::host::Host;
use crate::midi::{MidiInput, MidiOutput};

pub(crate) use bindings::CtxUd;

/// The memory limit of a Lua state.
pub const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
/// The number of Lua instructions an outermost call may execute (against endless loops).
pub const INSTRUCTION_BUDGET: u64 = 20_000_000;
/// The hook is called every this many instructions.
const HOOK_INTERVAL: u32 = 1000;

/// Stored in the Lua state: what the bindings need.
pub(crate) struct EngineData {
    pub stack: Rc<CtxStack>,
    pub engine: RefCell<Weak<Engine>>,
}

/// A Lua state of a controller script.
pub(crate) struct Engine {
    lua: Lua,
    name: String,
    stack: Rc<CtxStack>,
    errors: RefCell<ErrorPolicy>,
    host: Option<Host>,
    budget: Rc<Cell<u64>>,
    /// The table which the script returned.
    controller: Table,
}

impl Engine {
    /// Create the Lua state and run the script. `host` receives the error messages (`None` for
    /// the metadata, they go to standard error then).
    fn new(source: &ScriptSource, host: Option<Host>) -> Result<Rc<Engine>, String> {
        let stack = Rc::new(CtxStack::default());
        let budget = Rc::new(Cell::new(0u64));
        let lua = Lua::new_with(StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8, LuaOptions::new()).map_err(|e| e.to_string())?;
        let setup = || -> mlua::Result<Table> {
            lua.set_memory_limit(MEMORY_LIMIT)?;
            let hook_budget = budget.clone();
            lua.set_hook(HookTriggers::new().every_nth_instruction(HOOK_INTERVAL), move |_, debug| {
                let used = hook_budget.get() + HOOK_INTERVAL as u64;
                hook_budget.set(used);
                if used > INSTRUCTION_BUDGET {
                    let source = debug.source().short_src.map(|s| s.to_string()).unwrap_or_default();
                    let line = debug.current_line().unwrap_or(0);
                    return Err(mlua::Error::runtime(format!("{source}:{line}: the instruction limit of {INSTRUCTION_BUDGET} per call is exceeded (endless loop?)")));
                }
                Ok(VmState::Continue)
            })?;
            lua.set_app_data(EngineData { stack: stack.clone(), engine: RefCell::new(Weak::new()) });
            sandbox(&lua, source, host.clone())?;
            bindings::register_globals(&lua)?;
            lua.load(source.text.as_str()).set_name(format!("@{}", source.name)).eval::<Table>()
        };
        let controller = setup().map_err(|e| format_error(&source.name, &e))?;
        budget.set(0);
        let engine = Rc::new(Engine { lua, name: source.name.clone(), stack, errors: RefCell::new(ErrorPolicy::default()), host, budget, controller });
        if let Some(data) = engine.lua.app_data_ref::<EngineData>() {
            *data.engine.borrow_mut() = Rc::downgrade(&engine);
        }
        Ok(engine)
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn controller(&self) -> &Table {
        &self.controller
    }

    /// A function of the controller table, if the script defines it.
    fn hook(&self, name: &str) -> Option<Function> {
        self.controller.get::<Option<Function>>(name).ok().flatten()
    }

    fn log_error(&self, message: &str) {
        match &self.host {
            Some(host) => host.error(message),
            None => eprintln!("[DrivenByMoss] {message}"),
        }
    }

    fn start_call(&self) {
        // The budget is per outermost call
        if self.stack.depth() <= 1 {
            self.budget.set(0);
        }
    }

    /// Handle the result of a call: count and log errors.
    fn finish<R>(&self, key: usize, what: &dyn Describe, result: std::thread::Result<mlua::Result<R>>) -> Option<R> {
        let error = match result {
            Ok(Ok(value)) => {
                self.errors.borrow_mut().succeeded(key);
                return Some(value);
            }
            Ok(Err(error)) => format_error(&self.name, &error),
            Err(panic) => {
                let message = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_default();
                format!("{}: panic in {}: {message}", self.name, what.describe())
            }
        };
        self.log_error(&format!("Lua: {error}"));
        if self.errors.borrow_mut().failed(key) {
            self.log_error(&format!("Lua: {}: {} is disabled after {MAX_ERRORS_IN_A_ROW} errors in a row", self.name, what.describe()));
        }
        None
    }

    /// Call a script function with a context which it may change. `args` gets the `ctx` value
    /// of the call. Returns `None` on an error (which is logged) or if the function is disabled.
    pub fn call_mut<A: IntoLuaMulti, R: FromLuaMulti>(&self, ctx: &mut Ctx, f: &Function, what: &dyn Describe, args: impl FnOnce(AnyUserData) -> A) -> Option<R> {
        let key = f.to_pointer() as usize;
        if self.errors.borrow().is_disabled(key) {
            return None;
        }
        let stack = self.stack.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            stack.enter_mut(ctx, |serial| {
                self.start_call();
                let ud = self.lua.create_userdata(CtxUd { serial })?;
                f.call::<R>(args(ud))
            })
        }));
        self.finish(key, what, result)
    }

    /// As [`Self::call_mut`] with a read-only context.
    pub fn call_ref<A: IntoLuaMulti, R: FromLuaMulti>(&self, ctx: &Ctx, f: &Function, what: &dyn Describe, args: impl FnOnce(AnyUserData) -> A) -> Option<R> {
        let key = f.to_pointer() as usize;
        if self.errors.borrow().is_disabled(key) {
            return None;
        }
        let stack = self.stack.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            stack.enter(ctx, |serial| {
                self.start_call();
                let ud = self.lua.create_userdata(CtxUd { serial })?;
                f.call::<R>(args(ud))
            })
        }));
        self.finish(key, what, result)
    }

    /// Call a function of the controller table with the context, if it is defined. Returns false
    /// if it is not defined.
    fn call_hook(&self, ctx: &mut Ctx, name: &str) -> bool {
        let Some(f) = self.hook(name) else {
            return false;
        };
        let _: Option<()> = self.call_mut(ctx, &f, &name, |c| c);
        true
    }
}

/// The name of a script function in the error messages, created only when there is an error.
pub(crate) trait Describe {
    fn describe(&self) -> String;
}

impl Describe for &str {
    fn describe(&self) -> String {
        self.to_string()
    }
}

impl Describe for String {
    fn describe(&self) -> String {
        self.clone()
    }
}

/// A name which is formatted when it is needed.
pub(crate) struct Lazy<F: Fn() -> String>(pub F);

impl<F: Fn() -> String> Describe for Lazy<F> {
    fn describe(&self) -> String {
        (self.0)()
    }
}

/// The message of a Lua error: the script, the line and the message, without the traceback.
/// For an error raised by the API (a callback error) the location is taken from the traceback.
fn format_error(name: &str, error: &mlua::Error) -> String {
    match error {
        mlua::Error::CallbackError { traceback, cause } => {
            let cause = root_cause(cause);
            let location = traceback.lines().map(str::trim).find(|line| line.starts_with(name)).and_then(|line| {
                let mut parts = line.splitn(3, ':');
                Some(format!("{}:{}", parts.next()?, parts.next()?))
            });
            match location {
                Some(location) => format!("{location}: {}", short_message(&cause)),
                None => format!("{name}: {}", short_message(&cause)),
            }
        }
        mlua::Error::RuntimeError(message) | mlua::Error::SyntaxError { message, .. } => short_message(message),
        mlua::Error::MemoryError(message) => format!("{name}: out of memory ({message})"),
        other => format!("{name}: {}", short_message(&other.to_string())),
    }
}

fn root_cause(error: &mlua::Error) -> String {
    match error {
        mlua::Error::CallbackError { cause, .. } => root_cause(cause),
        mlua::Error::RuntimeError(message) => message.clone(),
        other => other.to_string(),
    }
}

/// Remove the unsafe functions, redirect `print` to the host and limit `require` to the folder
/// of the script.
fn sandbox(lua: &Lua, source: &ScriptSource, host: Option<Host>) -> mlua::Result<()> {
    let globals = lua.globals();
    for name in ["dofile", "loadfile", "load", "require", "collectgarbage"] {
        globals.raw_set(name, Value::Nil)?;
    }
    if let Ok(string) = globals.get::<Table>("string") {
        string.raw_set("dump", Value::Nil)?;
    }
    let name = source.name.clone();
    globals.raw_set(
        "print",
        lua.create_function(move |lua, args: mlua::Variadic<Value>| {
            let text: Vec<String> = args.iter().map(|v| lua.coerce_string(v.clone()).ok().flatten().map(|s| s.to_string_lossy()).unwrap_or_else(|| format!("{v:?}"))).collect();
            let line = format!("{name}: {}", text.join("\t"));
            match &host {
                Some(host) => host.println(&line),
                None => println!("{line}"),
            }
            Ok(())
        })?,
    )?;
    let folder = source.folder.clone();
    let loaded = lua.create_table()?;
    globals.raw_set(
        "require",
        lua.create_function(move |lua, module: String| {
            if let Some(value) = loaded.raw_get::<Option<Value>>(module.as_str())? {
                return Ok(value);
            }
            if module.is_empty() || !module.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(mlua::Error::runtime(format!("require: invalid module name '{module}' (letters, digits and _ only)")));
            }
            let Some(folder) = &folder else {
                return Err(mlua::Error::runtime(format!("require: module '{module}' not found (a built-in script has no module folder)")));
            };
            let file = format!("{module}.lua");
            let text = std::fs::read_to_string(folder.join(&file)).map_err(|e| mlua::Error::runtime(format!("require: module '{module}': {e}")))?;
            let value: Value = lua.load(text).set_name(format!("@{file}")).eval()?;
            let value = if value.is_nil() { Value::Boolean(true) } else { value };
            loaded.raw_set(module.as_str(), value.clone())?;
            Ok(value)
        })?,
    )?;
    Ok(())
}

// ------------------------------------------------------------------------------------------
// The metadata of a script

/// The input and output port names of a device for the automatic detection.
type PortPair = (Vec<String>, Vec<String>);
/// The metadata of a script, `None` if it could not be loaded.
type MetaEntry = Option<Rc<ScriptMeta>>;

/// What the factory needs to know of a script without running a controller.
struct ScriptMeta {
    source: ScriptSource,
    info: ControllerDefinitionInfo,
    class_name: &'static str,
    discovery: Vec<(OperatingSystem, Vec<PortPair>)>,
    settings: Vec<(String, Scope)>,
}

thread_local! {
    static META_CACHE: RefCell<HashMap<(String, String), MetaEntry>> = RefCell::new(HashMap::new());
    static CLASS_NAMES: RefCell<HashMap<String, &'static str>> = RefCell::new(HashMap::new());
}

/// The class name as a static string (`ControllerFactory::java_class_name`); each distinct name
/// is allocated once.
fn intern(name: &str) -> &'static str {
    CLASS_NAMES.with(|names| *names.borrow_mut().entry(name.to_string()).or_insert_with(|| Box::leak(name.to_string().into_boxed_str())))
}

fn os_name(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Windows => "windows",
        OperatingSystem::Linux => "linux",
        OperatingSystem::Mac => "mac",
    }
}

fn string_list(value: Value) -> mlua::Result<Vec<String>> {
    match value {
        Value::Nil => Ok(Vec::new()),
        Value::String(s) => Ok(vec![s.to_str()?.to_string()]),
        Value::Table(t) => t.sequence_values::<String>().collect(),
        _ => Err(mlua::Error::runtime("expected a string or a list of strings")),
    }
}

fn read_meta(source: &ScriptSource) -> Result<ScriptMeta, String> {
    let engine = Engine::new(source, None)?;
    let read = || -> mlua::Result<ScriptMeta> {
        let table = engine.controller();
        let info: Table = table.get("info").map_err(|_| mlua::Error::runtime("the script returns no 'info' table"))?;
        let uuid: String = info.get("uuid")?;
        let class_name: String = info.get("class_name")?;
        let info = ControllerDefinitionInfo {
            uuid,
            vendor: info.get("vendor")?,
            hardware_model: info.get("model")?,
            num_midi_in_ports: info.get::<Option<usize>>("midi_inputs")?.unwrap_or(1),
            num_midi_out_ports: info.get::<Option<usize>>("midi_outputs")?.unwrap_or(1),
        };
        let mut discovery = Vec::new();
        let value: Value = table.get("discovery")?;
        for os in [OperatingSystem::Windows, OperatingSystem::Linux, OperatingSystem::Mac] {
            let pairs: Value = match &value {
                Value::Function(f) => f.call(os_name(os))?,
                Value::Table(t) => t.get(os_name(os))?,
                _ => Value::Nil,
            };
            let mut list = Vec::new();
            if let Value::Table(pairs) = pairs {
                for pair in pairs.sequence_values::<Table>() {
                    let pair = pair?;
                    list.push((string_list(pair.get("inputs")?)?, string_list(pair.get("outputs")?)?));
                }
            }
            discovery.push((os, list));
        }
        let mut settings = Vec::new();
        if let Some(list) = table.get::<Option<Table>>("settings")? {
            for entry in list.sequence_values::<Value>() {
                let (name, scope) = match entry? {
                    Value::String(s) => (s.to_str()?.to_string(), "global".to_string()),
                    Value::Table(t) => (t.get::<String>(1)?, t.get::<Option<String>>(2)?.unwrap_or_else(|| "global".to_string())),
                    _ => return Err(mlua::Error::runtime("a setting is a name or {name, scope}")),
                };
                if !super::names::SETTING_ACTIVATORS.iter().any(|(n, _)| *n == name) {
                    return Err(mlua::Error::runtime(format!("unknown setting '{name}'")));
                }
                settings.push((name, super::names::scope(&scope).map_err(|e| mlua::Error::runtime(e.0))?));
            }
        }
        Ok(ScriptMeta { source: source.clone(), info, class_name: intern(&class_name), discovery, settings })
    };
    read().map_err(|e| format_error(&source.name, &e))
}

fn meta(source: ScriptSource) -> Option<Rc<ScriptMeta>> {
    let key = (source.name.clone(), source.text.clone());
    if let Some(meta) = META_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return meta;
    }
    let meta = match read_meta(&source) {
        Ok(meta) => Some(Rc::new(meta)),
        Err(error) => {
            eprintln!("[DrivenByMoss] Lua controller script not loaded: {error}");
            None
        }
    };
    META_CACHE.with(|c| c.borrow_mut().insert(key, meta.clone()));
    meta
}

/// The factory of a script, `None` if the script cannot be loaded.
pub fn factory(source: ScriptSource) -> Option<Box<dyn ControllerFactory>> {
    meta(source).map(|meta| Box::new(LuaControllerFactory { meta }) as Box<dyn ControllerFactory>)
}

// ------------------------------------------------------------------------------------------
// The factory and the setup

/// A controller written in Lua (doc/LUA_PLAN.md §3, "Loading").
pub struct LuaControllerFactory {
    meta: Rc<ScriptMeta>,
}

impl ControllerFactory for LuaControllerFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        self.meta.info.clone()
    }

    fn java_class_name(&self) -> &'static str {
        self.meta.class_name
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        self.meta.discovery.iter().find(|(o, _)| *o == os).map(|(_, pairs)| pairs.clone()).unwrap_or_default()
    }

    fn create_configuration(&self) -> Configuration {
        let mut config = Configuration::new(&crate::config::values::REAPER_ARP_MODES);
        for (name, scope) in &self.meta.settings {
            // The names were checked when the metadata was read
            let _ = super::names::activate_setting(&mut config, name, *scope);
        }
        config
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        let engine = Engine::new(&self.meta.source, Some(host.clone()))?;
        let table = engine.controller().clone();
        let name = self.meta.source.name.clone();
        let error = |e: mlua::Error| format_error(&name, &e);

        let value_changer = bindings::value_changer(table.get("value_changer").map_err(error)?).map_err(error)?;
        let colors = bindings::color_manager(table.get("colors").map_err(error)?).map_err(error)?;
        let scales = bindings::scales(&value_changer, table.get("scales").map_err(error)?).map_err(error)?;
        let model_setup = bindings::model_setup(engine.lua(), table.get("model").map_err(error)?, &configuration).map_err(error)?;
        let model_options: Option<Table> = table.get("model").ok().and_then(|v: Value| v.as_table().cloned());

        let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
        if model_options.is_some_and(|t| t.get::<Option<bool>>("track_bank_indication").ok().flatten() == Some(true)) {
            model.track_bank_mut().set_indication(true);
        }

        let filters = string_list(table.get("midi_input_filters").map_err(error)?).map_err(error)?;
        let filters: Vec<&str> = filters.iter().map(String::as_str).collect();
        let output = MidiOutput::new(sender.clone(), Some(ports.output));
        let input = MidiInput::new(sender.clone(), Some(ports.input), &filters)?;
        let surface = Surface::new(output, input, None);

        let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
        ctx.ext = Box::new(LuaExt { engine: Rc::downgrade(&engine) });
        if let Some(f) = engine.hook("handle_sysex") {
            let weak = Rc::downgrade(&engine);
            ctx.sysex_handler = Some(Rc::new(move |ctx: &mut Ctx, data: &str| {
                if let Some(engine) = weak.upgrade() {
                    let data = data.to_string();
                    let _: Option<()> = engine.call_mut(ctx, &f, &"handle_sysex", |c| (c, data));
                }
            }));
        }
        if let Some(f) = engine.hook("handle_midi") {
            let weak = Rc::downgrade(&engine);
            ctx.midi_handler = Some(Rc::new(move |ctx: &mut Ctx, status, data1, data2| {
                if let Some(engine) = weak.upgrade() {
                    let _: Option<()> = engine.call_mut(ctx, &f, &"handle_midi", |c| (c, status, data1, data2));
                }
            }));
        }
        Ok(InstanceParts { ctx, setup: Box::new(LuaSetup { engine }) })
    }
}

/// Stored in `ctx.ext` of a script controller.
pub(crate) struct LuaExt {
    engine: Weak<Engine>,
}

/// The setup of a script controller: calls the functions of the controller table.
pub struct LuaSetup {
    engine: Rc<Engine>,
}

impl ControllerSetup for LuaSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        self.engine.call_hook(ctx, "create");
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        self.engine.call_hook(ctx, "startup");
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        if !self.engine.call_hook(ctx, "flush") {
            ctx.flush_surface();
        }
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        if !self.engine.call_hook(ctx, "handle_changes") {
            // Nobody observes them
            ctx.model.take_events();
            ctx.config.take_changed_settings();
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        if !self.engine.call_hook(ctx, "exit") {
            ctx.turn_off_lights();
        }
    }
}

/// See [`super::eval`].
pub fn eval(ctx: &mut Ctx, code: &str) -> Option<String> {
    let engine = ctx.ext.downcast_ref::<LuaExt>()?.engine.upgrade()?;
    let f = engine.lua.load(format!("local controller, ctx = ...\n{code}")).set_name("=eval").into_function().ok()?;
    let controller = engine.controller.clone();
    let value: Value = engine.call_mut(ctx, &f, &"eval", |c| (controller, c))?;
    match value {
        Value::Nil => Some("nil".to_string()),
        other => engine.lua.coerce_string(other.clone()).ok().flatten().map(|s| s.to_string_lossy()).or_else(|| Some(format!("{other:?}"))),
    }
}

/// See [`super::api_reference`].
pub fn api_reference() -> String {
    bindings::api_reference()
}

#[cfg(test)]
mod tests;
