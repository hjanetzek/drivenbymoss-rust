// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Lua tables and functions as modes, views, parameters, commands and lights.
//!
//! A [`LuaMode`] forwards each method of the [`Mode`] trait to the function of the same name in
//! its table. If the table has no such function, the call goes to the built-in base mode of the
//! table (`base`, see `ctx:create_mode`) if the method is in its `forward` list (default: all
//! methods), else the default of the trait applies. The table's functions get the table as
//! `self` and the `ctx` of the call. [`LuaView`] does the same for the [`View`] trait.
//!
//! The mode or view is borrowed while its methods run, as in Rust: a call from its script which
//! needs it again (e.g. `ctx:set_active_mode_in` on its own manager) is a Lua error. A deferred
//! action goes through `ctx:schedule(0, fn)`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use mlua::{Function, Lua, Table, Value};

use super::bindings::{ModeUd, ViewUd};
use super::{Engine, Lazy};
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{Mode, View, BUTTON_COLOR_OFF};
use crate::framework::ids::ButtonId;
use crate::framework::parameterprovider::CustomParameter;
use crate::framework::surface::{LightSender, LightSupplier};
use crate::midi::ButtonEvent;
use crate::script::names::button_event_name;

// ------------------------------------------------------------------------------------------
// Modes

/// The methods of the [`Mode`] trait which a table can implement (`is_any_knob_touched` has no
/// context and is forwarded to the base only).
pub const MODE_METHODS: [&str; 21] = [
    "on_activate",
    "on_deactivate",
    "button_color",
    "update_display",
    "on_knob_value",
    "knob_value",
    "knob_color",
    "on_knob_touch",
    "on_button",
    "select_item",
    "selected_item_name",
    "select_previous_item",
    "select_next_item",
    "select_previous_item_page",
    "select_next_item_page",
    "select_item_page",
    "has_previous_item",
    "has_next_item",
    "has_previous_item_page",
    "has_next_item_page",
    "is_any_knob_touched",
];

const ON_ACTIVATE: usize = 0;
const ON_DEACTIVATE: usize = 1;
const BUTTON_COLOR: usize = 2;
const UPDATE_DISPLAY: usize = 3;
const ON_KNOB_VALUE: usize = 4;
const KNOB_VALUE: usize = 5;
const KNOB_COLOR: usize = 6;
const ON_KNOB_TOUCH: usize = 7;
const ON_BUTTON: usize = 8;
const SELECT_ITEM: usize = 9;
const SELECTED_ITEM_NAME: usize = 10;
const SELECT_PREVIOUS_ITEM: usize = 11;
const SELECT_NEXT_ITEM: usize = 12;
const SELECT_PREVIOUS_ITEM_PAGE: usize = 13;
const SELECT_NEXT_ITEM_PAGE: usize = 14;
const SELECT_ITEM_PAGE: usize = 15;
const HAS_PREVIOUS_ITEM: usize = 16;
const HAS_NEXT_ITEM: usize = 17;
const HAS_PREVIOUS_ITEM_PAGE: usize = 18;
const HAS_NEXT_ITEM_PAGE: usize = 19;
const IS_ANY_KNOB_TOUCHED: usize = 20;

type SharedMode = Rc<RefCell<Box<dyn Mode>>>;

/// Read the functions of a table (also through its metatable, e.g. a class), the base and the
/// forward list.
fn read_table<const N: usize>(table: &Table, names: &[&str; N]) -> mlua::Result<([Option<Function>; N], u64)> {
    let mut methods: [Option<Function>; N] = std::array::from_fn(|_| None);
    for (i, name) in names.iter().enumerate() {
        methods[i] = table.get::<Option<Function>>(*name)?;
    }
    let forward = match table.get::<Value>("forward")? {
        Value::Nil => u64::MAX,
        Value::Table(list) => {
            let mut mask = 0u64;
            for name in list.sequence_values::<String>() {
                let name = name?;
                let index = names.iter().position(|n| *n == name).ok_or_else(|| mlua::Error::runtime(format!("forward: unknown method '{name}'")))?;
                mask |= 1 << index;
            }
            mask
        }
        _ => return Err(mlua::Error::runtime("forward must be a list of method names")),
    };
    Ok((methods, forward))
}

/// A mode written in Lua (or a built-in mode registered directly).
pub struct LuaMode {
    engine: Weak<Engine>,
    name: String,
    table: Option<Table>,
    methods: [Option<Function>; MODE_METHODS.len()],
    base: Option<SharedMode>,
    forward: u64,
}

impl LuaMode {
    /// From a table or a built-in mode (`ctx:create_mode`).
    pub fn from_value(lua: &Lua, engine: Weak<Engine>, value: Value) -> mlua::Result<LuaMode> {
        match value {
            Value::UserData(ud) => {
                let base = ud.borrow::<ModeUd>()?.0.clone();
                let name = base.try_borrow().map(|m| m.name().to_string()).unwrap_or_default();
                Ok(LuaMode { engine, name, table: None, methods: std::array::from_fn(|_| None), base: Some(base), forward: u64::MAX })
            }
            Value::Table(table) => {
                let (methods, forward) = read_table(&table, &MODE_METHODS)?;
                let base = match table.get::<Value>("base")? {
                    Value::Nil => None,
                    Value::UserData(ud) => Some(ud.borrow::<ModeUd>()?.0.clone()),
                    _ => return Err(mlua::Error::runtime("the base of a mode must be a built-in mode (ctx:create_mode)")),
                };
                let name = match table.get::<Option<String>>("name")? {
                    Some(name) => name,
                    None => base.as_ref().and_then(|b| b.try_borrow().ok().map(|m| m.name().to_string())).unwrap_or_else(|| "Lua Mode".to_string()),
                };
                let _ = lua;
                Ok(LuaMode { engine, name, table: Some(table), methods, base, forward })
            }
            _ => Err(mlua::Error::runtime("a mode is a table or a built-in mode (ctx:create_mode)")),
        }
    }

    fn function(&self, method: usize) -> Option<(Rc<Engine>, &Function)> {
        let f = self.methods[method].as_ref()?;
        Some((self.engine.upgrade()?, f))
    }

    fn what(&self, method: usize) -> String {
        format!("mode '{}' {}", self.name, MODE_METHODS[method])
    }

    fn table_value(&self) -> Value {
        self.table.clone().map_or(Value::Nil, Value::Table)
    }

    /// The base for a forwarded method (`None` if there is none or it does not forward it).
    fn base(&self, method: usize) -> Option<&SharedMode> {
        self.base.as_ref().filter(|_| self.forward & (1 << method) != 0)
    }

    fn with_base_mut(&self, ctx: &mut Ctx, method: usize, f: impl FnOnce(&mut dyn Mode, &mut Ctx)) {
        if let Some(base) = self.base(method) {
            match base.try_borrow_mut() {
                Ok(mut mode) => f(mode.as_mut(), ctx),
                Err(_) => ctx.host.error(&format!("Lua: the base of {} is in use (re-entrant call)", self.what(method))),
            }
        }
    }

    fn with_base<R>(&self, ctx: &Ctx, method: usize, default: R, f: impl FnOnce(&dyn Mode, &Ctx) -> R) -> R {
        match self.base(method) {
            Some(base) => match base.try_borrow() {
                Ok(mode) => f(mode.as_ref(), ctx),
                Err(_) => {
                    ctx.host.error(&format!("Lua: the base of {} is in use (re-entrant call)", self.what(method)));
                    default
                }
            },
            None => default,
        }
    }

    /// Call a method which changes the context: the table's function, else the base.
    fn dispatch_mut(&self, ctx: &mut Ctx, method: usize, args: impl FnOnce(Value, mlua::AnyUserData) -> mlua::MultiValue, forward: impl FnOnce(&mut dyn Mode, &mut Ctx)) {
        if let Some((engine, f)) = self.function(method) {
            let table = self.table_value();
            let _: Option<()> = engine.call_mut(ctx, f, &Lazy(|| self.what(method)), |c| args(table, c));
            return;
        }
        self.with_base_mut(ctx, method, forward);
    }

    /// Call a method which reads the context.
    fn dispatch_ref<R: mlua::FromLua>(
        &self,
        ctx: &Ctx,
        method: usize,
        default: R,
        args: impl FnOnce(Value, mlua::AnyUserData) -> mlua::MultiValue,
        forward: impl FnOnce(&dyn Mode, &Ctx) -> R,
    ) -> R {
        if let Some((engine, f)) = self.function(method) {
            let table = self.table_value();
            return engine.call_ref::<_, Option<R>>(ctx, f, &Lazy(|| self.what(method)), |c| args(table, c)).flatten().unwrap_or(default);
        }
        self.with_base(ctx, method, default, forward)
    }
}

/// The arguments of a call: `self`, `ctx` and the values.
macro_rules! args {
    ($($arg:expr),*) => {
        |table: Value, ctx: mlua::AnyUserData| {
            let mut values = mlua::MultiValue::new();
            values.push_back(table);
            values.push_back(Value::UserData(ctx));
            $(values.push_back(into_value($arg));)*
            values
        }
    };
}

/// Simple values as Lua values (no allocation in the Lua state needed, except strings which
/// are passed as light names).
pub(crate) trait SimpleValue {
    fn into_simple(self) -> Value;
}

impl SimpleValue for usize {
    fn into_simple(self) -> Value {
        Value::Integer(self as i64)
    }
}

impl SimpleValue for i32 {
    fn into_simple(self) -> Value {
        Value::Integer(self as i64)
    }
}

impl SimpleValue for f64 {
    fn into_simple(self) -> Value {
        Value::Number(self)
    }
}

impl SimpleValue for bool {
    fn into_simple(self) -> Value {
        Value::Boolean(self)
    }
}

impl SimpleValue for Value {
    fn into_simple(self) -> Value {
        self
    }
}

fn into_value(value: impl SimpleValue) -> Value {
    value.into_simple()
}

/// A string as a Lua value (created in the Lua state of the engine).
fn string(engine: &Weak<Engine>, text: &str) -> Value {
    engine.upgrade().and_then(|e| e.lua().create_string(text).ok()).map_or(Value::Nil, Value::String)
}

impl Mode for LuaMode {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.dispatch_mut(ctx, ON_ACTIVATE, args!(), |m, c| m.on_activate(c));
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.dispatch_mut(ctx, ON_DEACTIVATE, args!(), |m, c| m.on_deactivate(c));
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let name = string(&self.engine, button.name());
        let default = if self.methods[BUTTON_COLOR].is_none() && self.base(BUTTON_COLOR).is_none() {
            // The default of the trait
            ctx.colors.color_index(BUTTON_COLOR_OFF)
        } else {
            0
        };
        self.dispatch_ref(ctx, BUTTON_COLOR, default, args!(name), |m, c| m.button_color(c, button))
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        self.dispatch_mut(ctx, UPDATE_DISPLAY, args!(), |m, c| m.update_display(c));
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.dispatch_mut(ctx, ON_KNOB_VALUE, args!(index, value), |m, c| m.on_knob_value(c, index, value));
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.dispatch_ref(ctx, KNOB_VALUE, 0, args!(index), |m, c| m.knob_value(c, index))
    }

    fn knob_color(&self, ctx: &Ctx, index: usize) -> i32 {
        self.dispatch_ref(ctx, KNOB_COLOR, 0, args!(index), |m, c| m.knob_color(c, index))
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.dispatch_mut(ctx, ON_KNOB_TOUCH, args!(index, is_touched), |m, c| m.on_knob_touch(c, index, is_touched));
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base(IS_ANY_KNOB_TOUCHED).and_then(|b| b.try_borrow().ok().map(|m| m.is_any_knob_touched())).unwrap_or(false)
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        let name = string(&self.engine, button_event_name(event));
        self.dispatch_mut(ctx, ON_BUTTON, args!(row, index, name), |m, c| m.on_button(c, row, index, event));
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.dispatch_mut(ctx, SELECT_ITEM, args!(index), |m, c| m.select_item(c, index));
    }

    fn selected_item_name(&self, ctx: &Ctx) -> Option<String> {
        if let Some((engine, f)) = self.function(SELECTED_ITEM_NAME) {
            let table = self.table_value();
            return engine.call_ref::<_, Option<String>>(ctx, f, &Lazy(|| self.what(SELECTED_ITEM_NAME)), |c| (table, c)).flatten();
        }
        self.with_base(ctx, SELECTED_ITEM_NAME, None, |m, c| m.selected_item_name(c))
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.dispatch_mut(ctx, SELECT_PREVIOUS_ITEM, args!(), |m, c| m.select_previous_item(c));
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.dispatch_mut(ctx, SELECT_NEXT_ITEM, args!(), |m, c| m.select_next_item(c));
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.dispatch_mut(ctx, SELECT_PREVIOUS_ITEM_PAGE, args!(), |m, c| m.select_previous_item_page(c));
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.dispatch_mut(ctx, SELECT_NEXT_ITEM_PAGE, args!(), |m, c| m.select_next_item_page(c));
    }

    fn select_item_page(&mut self, ctx: &mut Ctx, page: usize) {
        self.dispatch_mut(ctx, SELECT_ITEM_PAGE, args!(page), |m, c| m.select_item_page(c, page));
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.dispatch_ref(ctx, HAS_PREVIOUS_ITEM, false, args!(), |m, c| m.has_previous_item(c))
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.dispatch_ref(ctx, HAS_NEXT_ITEM, false, args!(), |m, c| m.has_next_item(c))
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.dispatch_ref(ctx, HAS_PREVIOUS_ITEM_PAGE, false, args!(), |m, c| m.has_previous_item_page(c))
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.dispatch_ref(ctx, HAS_NEXT_ITEM_PAGE, false, args!(), |m, c| m.has_next_item_page(c))
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// Views

/// The methods of the [`View`] trait which a table can implement.
pub const VIEW_METHODS: [&str; 8] = ["on_activate", "on_deactivate", "button_color", "draw_grid", "on_grid_note", "on_grid_note_long_press", "on_button", "update_control_surface"];

const V_ON_ACTIVATE: usize = 0;
const V_ON_DEACTIVATE: usize = 1;
const V_BUTTON_COLOR: usize = 2;
const V_DRAW_GRID: usize = 3;
const V_ON_GRID_NOTE: usize = 4;
const V_ON_GRID_NOTE_LONG_PRESS: usize = 5;
const V_ON_BUTTON: usize = 6;
const V_UPDATE_CONTROL_SURFACE: usize = 7;

type SharedView = Rc<RefCell<Box<dyn View>>>;

/// A view written in Lua (or a built-in view registered directly).
pub struct LuaView {
    engine: Weak<Engine>,
    name: String,
    table: Option<Table>,
    methods: [Option<Function>; VIEW_METHODS.len()],
    base: Option<SharedView>,
    forward: u64,
}

impl LuaView {
    pub fn from_value(engine: Weak<Engine>, value: Value) -> mlua::Result<LuaView> {
        match value {
            Value::UserData(ud) => {
                let base = ud.borrow::<ViewUd>()?.0.clone();
                let name = base.try_borrow().map(|v| v.name().to_string()).unwrap_or_default();
                Ok(LuaView { engine, name, table: None, methods: std::array::from_fn(|_| None), base: Some(base), forward: u64::MAX })
            }
            Value::Table(table) => {
                let (methods, forward) = read_table(&table, &VIEW_METHODS)?;
                let base = match table.get::<Value>("base")? {
                    Value::Nil => None,
                    Value::UserData(ud) => Some(ud.borrow::<ViewUd>()?.0.clone()),
                    _ => return Err(mlua::Error::runtime("the base of a view must be a built-in view (ctx:create_view)")),
                };
                let name = table.get::<Option<String>>("name")?.unwrap_or_else(|| "Lua View".to_string());
                Ok(LuaView { engine, name, table: Some(table), methods, base, forward })
            }
            _ => Err(mlua::Error::runtime("a view is a table or a built-in view (ctx:create_view)")),
        }
    }

    fn what(&self, method: usize) -> String {
        format!("view '{}' {}", self.name, VIEW_METHODS[method])
    }

    fn base(&self, method: usize) -> Option<&SharedView> {
        self.base.as_ref().filter(|_| self.forward & (1 << method) != 0)
    }

    /// Call the table's function; returns false if there is none.
    fn call_mut(&self, ctx: &mut Ctx, method: usize, args: impl FnOnce(Value, mlua::AnyUserData) -> mlua::MultiValue) -> bool {
        let (Some(f), Some(engine)) = (self.methods[method].as_ref(), self.engine.upgrade()) else {
            return self.methods[method].is_some();
        };
        let table = self.table.clone().map_or(Value::Nil, Value::Table);
        let _: Option<()> = engine.call_mut(ctx, f, &Lazy(|| self.what(method)), |c| args(table, c));
        true
    }

    fn forward_mut(&self, ctx: &mut Ctx, method: usize, f: impl FnOnce(&mut dyn View, &mut Ctx)) -> bool {
        let Some(base) = self.base(method) else {
            return false;
        };
        match base.try_borrow_mut() {
            Ok(mut view) => f(view.as_mut(), ctx),
            Err(_) => ctx.host.error(&format!("Lua: the base of {} is in use (re-entrant call)", self.what(method))),
        }
        true
    }
}

impl View for LuaView {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        if !self.call_mut(ctx, V_ON_ACTIVATE, args!()) {
            self.forward_mut(ctx, V_ON_ACTIVATE, |v, c| v.on_activate(c));
        }
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        if !self.call_mut(ctx, V_ON_DEACTIVATE, args!()) {
            self.forward_mut(ctx, V_ON_DEACTIVATE, |v, c| v.on_deactivate(c));
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if let (Some(f), Some(engine)) = (self.methods[V_BUTTON_COLOR].as_ref(), self.engine.upgrade()) {
            let table = self.table.clone().map_or(Value::Nil, Value::Table);
            let name = string(&self.engine, button.name());
            return engine.call_ref::<_, Option<i32>>(ctx, f, &Lazy(|| self.what(V_BUTTON_COLOR)), |c| (table, c, name)).flatten().unwrap_or(0);
        }
        if let Some(base) = self.base(V_BUTTON_COLOR) {
            return base.try_borrow().map(|v| v.button_color(ctx, button)).unwrap_or(0);
        }
        ctx.colors.color_index(BUTTON_COLOR_OFF)
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !self.call_mut(ctx, V_DRAW_GRID, args!()) {
            self.forward_mut(ctx, V_DRAW_GRID, |v, c| v.draw_grid(c));
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !self.call_mut(ctx, V_ON_GRID_NOTE, args!(note, velocity)) {
            self.forward_mut(ctx, V_ON_GRID_NOTE, |v, c| v.on_grid_note(c, note, velocity));
        }
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.call_mut(ctx, V_ON_GRID_NOTE_LONG_PRESS, args!(note)) {
            self.forward_mut(ctx, V_ON_GRID_NOTE_LONG_PRESS, |v, c| v.on_grid_note_long_press(c, note));
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, velocity: i32) {
        let name = string(&self.engine, button.name());
        let event_name = string(&self.engine, button_event_name(event));
        if !self.call_mut(ctx, V_ON_BUTTON, args!(name, event_name, velocity)) {
            self.forward_mut(ctx, V_ON_BUTTON, |v, c| v.on_button(c, button, event, velocity));
        }
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        if !self.call_mut(ctx, V_UPDATE_CONTROL_SURFACE, args!()) && !self.forward_mut(ctx, V_UPDATE_CONTROL_SURFACE, |v, c| v.update_control_surface(c)) {
            // The default of the trait
            ctx.update_active_mode_display();
        }
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// Parameters

/// A parameter written in Lua (the former custom `IParameter` implementations, e.g. the note
/// attributes of the Launch Control XL). The table has the functions `does_exist`, `value`,
/// `set_normalized_value`, `change_value` and optionally `name`, `displayed_value`, `set_value`,
/// `reset_value`, `touch_value`.
pub struct LuaParameter {
    engine: Weak<Engine>,
    table: Table,
    methods: [Option<Function>; PARAMETER_METHODS.len()],
}

pub const PARAMETER_METHODS: [&str; 9] = ["does_exist", "name", "value", "displayed_value", "set_value", "set_normalized_value", "change_value", "reset_value", "touch_value"];

impl LuaParameter {
    pub fn new(engine: Weak<Engine>, table: Table) -> mlua::Result<Self> {
        let (methods, _) = read_table(&table, &PARAMETER_METHODS)?;
        for required in [0, 2, 5, 6] {
            if methods[required].is_none() {
                return Err(mlua::Error::runtime(format!("a parameter needs the function '{}'", PARAMETER_METHODS[required])));
            }
        }
        Ok(LuaParameter { engine, table, methods })
    }

    fn call_ref<R: mlua::FromLua>(&self, ctx: &Ctx, method: usize, default: R) -> R {
        let (Some(f), Some(engine)) = (self.methods[method].as_ref(), self.engine.upgrade()) else {
            return default;
        };
        let table = self.table.clone();
        engine.call_ref::<_, Option<R>>(ctx, f, &Lazy(|| format!("parameter {}", PARAMETER_METHODS[method])), |c| (table, c)).flatten().unwrap_or(default)
    }

    fn call_mut(&self, ctx: &mut Ctx, method: usize, arg: Value) -> bool {
        let (Some(f), Some(engine)) = (self.methods[method].as_ref(), self.engine.upgrade()) else {
            return self.methods[method].is_some();
        };
        let table = self.table.clone();
        let _: Option<()> = engine.call_mut(ctx, f, &Lazy(|| format!("parameter {}", PARAMETER_METHODS[method])), |c| (table, c, arg));
        true
    }
}

impl CustomParameter for LuaParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool {
        self.call_ref(ctx, 0, false)
    }

    fn name(&self, ctx: &Ctx) -> String {
        self.call_ref(ctx, 1, String::new())
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        self.call_ref(ctx, 2, 0)
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        self.call_ref(ctx, 3, String::new())
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        if !self.call_mut(ctx, 4, Value::Integer(value as i64)) {
            // The default of the trait
            let normalized = ctx.model.value_changer().to_normalized_value(value as f64);
            self.set_normalized_value(ctx, normalized);
        }
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        self.call_mut(ctx, 5, Value::Number(value));
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        self.call_mut(ctx, 6, Value::Integer(control as i64));
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        self.call_mut(ctx, 7, Value::Nil);
    }

    fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        self.call_mut(ctx, 8, Value::Boolean(is_touched));
    }
}

// ------------------------------------------------------------------------------------------
// Commands and lights

/// A button command written in Lua: `function (ctx, event, velocity)`.
pub struct LuaTrigger {
    pub engine: Weak<Engine>,
    pub function: Function,
    pub what: String,
}

impl TriggerCommand for LuaTrigger {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        let Some(engine) = self.engine.upgrade() else {
            return;
        };
        let name = string(&self.engine, button_event_name(event));
        let _: Option<()> = engine.call_mut(ctx, &self.function, &self.what, |c| (c, name, velocity));
    }
}

/// A command of a knob or fader written in Lua: `function (ctx, value)`.
pub struct LuaContinuous {
    pub engine: Weak<Engine>,
    pub function: Function,
    pub what: String,
}

impl ContinuousCommand for LuaContinuous {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if let Some(engine) = self.engine.upgrade() {
            let _: Option<()> = engine.call_mut(ctx, &self.function, &self.what, |c| (c, value));
        }
    }
}

/// The state of a light from Lua: `function (ctx) -> integer` (read-only context).
pub fn light_supplier(engine: Weak<Engine>, function: Function, what: String) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| {
        let Some(engine) = engine.upgrade() else {
            return 0;
        };
        engine.call_ref::<_, Option<i32>>(ctx, &function, &what, |c| c).flatten().unwrap_or(0)
    })
}

/// Send the state of a light from Lua: `function (ctx, value)`.
pub fn light_sender(engine: Weak<Engine>, function: Function, what: String) -> LightSender {
    Rc::new(move |ctx: &mut Ctx, value: i32| {
        if let Some(engine) = engine.upgrade() {
            let _: Option<()> = engine.call_mut(ctx, &function, &what, |c| (c, value));
        }
    })
}

/// A scheduled task from Lua: `function (ctx)`.
pub fn task(engine: Weak<Engine>, function: Function) -> impl FnOnce(&mut Ctx) + 'static {
    move |ctx: &mut Ctx| {
        if let Some(engine) = engine.upgrade() {
            let _: Option<()> = engine.call_mut(ctx, &function, &"scheduled task", |c| c);
        }
    }
}
