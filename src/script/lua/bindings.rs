// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! What a Lua controller script sees (doc/LUA_API.md is generated from the registrations here,
//! see [`api_reference`]).
//!
//! - `ctx`: the context of the call, valid only during that call.
//! - Handles (`ctx.model:track(0)`, `ctx.surface`, …): small values (a bank and an index) which
//!   are resolved through the context of the running call each time they are used. They can be
//!   kept between calls but not used outside of a call.
//! - Indices are 0-based as in the Rust and Java versions (not 1-based as usual in Lua); the
//!   IDs are the names of the Rust enum variants (`"Row11"`, `"Fader1"`, `"Send"`).
//! - Built-in modes, commands, parameter providers and parameters are values which hold the
//!   Rust object (`ctx:create_mode`, `ctx:command`, `ctx:provider`, `ctx:parameter`).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use mlua::{AnyUserData, Function, Lua, Table, UserData, UserDataFields, UserDataMethods, UserDataRef, Value};

use super::adapters::{light_sender, light_supplier, task, LuaContinuous, LuaMode, LuaParameter, LuaTrigger, LuaView};
use super::{Engine, EngineData};
use crate::config::Configuration;
use crate::daw::bank::BankItem;
use crate::daw::channel::Channel;
use crate::daw::clip::{CursorClip, NotePosition, StepState};
use crate::daw::color::{ColorEx, DawColor};
use crate::daw::constants::{DeviceId, Resolution};
use crate::daw::device::SpecificDevice;
use crate::daw::events::{ModelEvent, ParameterBankId};
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::ColorManager;
use crate::framework::command::SharedTrigger;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::{Ctx, ModeManagerId};
use crate::framework::feature::{Mode, ModeManager, View};
use crate::framework::ids::{ButtonId, ContinuousId, Modes};
use crate::framework::mode::layer;
use crate::framework::parameterprovider::{parameter_bank, parameter_bank_mut, ParamRef, SharedProvider};
use crate::framework::surface::{ButtonAction, LightSender, LightSupplier};
use crate::framework::value_changer::ValueChanger;
use crate::framework::view::control_only_view::ControlOnlyView;
use crate::scale::Scales;
use crate::script::builtin::{self, Command, CommandOptions, ModeOptions, ProviderSpec};
use crate::script::ctx_stack::{CtxStack, ScriptError};
use crate::script::names;

// ------------------------------------------------------------------------------------------
// The API reference

/// One documented function of the API.
struct ApiEntry {
    ty: &'static str,
    name: &'static str,
    signature: &'static str,
    doc: &'static str,
}

thread_local! {
    /// Collects the documentation of the bindings while [`api_reference`] registers them.
    static API_DOC: RefCell<Option<Vec<ApiEntry>>> = const { RefCell::new(None) };
}

fn document(ty: &'static str, name: &'static str, signature: &'static str, doc: &'static str) {
    API_DOC.with(|d| {
        if let Some(entries) = d.borrow_mut().as_mut() {
            entries.push(ApiEntry { ty, name, signature, doc });
        }
    });
}

/// Register a method and document it.
macro_rules! method {
    ($m:ident, $ty:expr, $name:literal, $sig:literal, $doc:literal, $f:expr) => {{
        document($ty, $name, $sig, $doc);
        $m.add_method($name, $f);
    }};
}

/// Register a field (read-only) and document it.
macro_rules! field {
    ($f:ident, $ty:expr, $name:literal, $sig:literal, $doc:literal, $g:expr) => {{
        document($ty, $name, $sig, $doc);
        $f.add_field_method_get($name, $g);
    }};
}

/// The types of the API in the order of the reference, with their description.
const TYPES: &[(&str, &str)] = &[
    ("dbm", "Global functions (the table `dbm`)."),
    ("Ctx", "The context of a call (`ctx`). Valid only during the call it is passed to."),
    ("Model", "The DAW model (`ctx.model`)."),
    ("TrackBank", "A track bank (`ctx.model:track_bank()`, `:current_track_bank()`)."),
    ("Track", "A track of the current page of a track bank."),
    ("Layer", "A layer (or drum pad) of the current page of the first instrument."),
    ("SendBank", "The sends of a track or layer."),
    ("SceneBank", "The scene bank (`ctx.model:scene_bank()`)."),
    ("Scene", "A scene of the current page."),
    ("Device", "A device: the cursor device, a specific device or the drum device."),
    ("DrumPad", "A drum pad of the current page of a device."),
    ("ParameterBank", "A parameter bank (`ctx.model:parameter_bank(name)`)."),
    ("Clip", "A cursor clip of the given size (`ctx.model:note_clip(steps, rows)`). A step is `(channel, step, note)`."),
    ("Transport", "The transport (`ctx.model:transport()`)."),
    ("Application", "The application (`ctx.model:application()`)."),
    ("Scales", "The scales (`ctx.model:scales()`)."),
    ("ValueChanger", "The value changer of the model (`ctx.model:value_changer()`)."),
    ("Surface", "The control surface (`ctx.surface`)."),
    ("Host", "The host (`ctx.host`)."),
    ("Config", "The configuration (`ctx.config`)."),
    ("Mode", "A built-in mode (`ctx:create_mode`), as the `base` of a mode table or registered directly."),
    ("View", "A built-in view (`ctx:create_view`)."),
    ("Command", "A built-in command (`ctx:command`)."),
    ("Provider", "A parameter provider (`ctx:provider`)."),
    ("Parameter", "A parameter (`ctx:parameter`)."),
];

/// The reference of the API as Markdown, generated from the registrations.
pub fn api_reference() -> String {
    API_DOC.with(|d| *d.borrow_mut() = Some(Vec::new()));
    let collect = || -> mlua::Result<()> {
        let lua = Lua::new_with(mlua::StdLib::NONE, mlua::LuaOptions::new())?;
        register_globals(&lua)?;
        // Each userdata type registers its methods when the first value is created
        lua.create_userdata(CtxUd { serial: 0 })?;
        lua.create_userdata(ModelUd)?;
        lua.create_userdata(TrackBankUd { current: false })?;
        lua.create_userdata(TrackUd { current: false, index: 0 })?;
        lua.create_userdata(LayerUd { index: 0 })?;
        lua.create_userdata(SendBankUd { owner: Owner::Layer(0) })?;
        lua.create_userdata(SceneBankUd)?;
        lua.create_userdata(SceneUd { index: 0 })?;
        lua.create_userdata(DeviceUd { which: Which::Cursor })?;
        lua.create_userdata(DrumPadUd { which: Which::Cursor, index: 0 })?;
        lua.create_userdata(ParameterBankUd { bank: ParameterBankId::CursorDevice })?;
        lua.create_userdata(ClipUd { cols: 0, rows: 0 })?;
        lua.create_userdata(TransportUd)?;
        lua.create_userdata(ApplicationUd)?;
        lua.create_userdata(ScalesUd)?;
        lua.create_userdata(ValueChangerUd)?;
        lua.create_userdata(SurfaceUd)?;
        lua.create_userdata(HostUd)?;
        lua.create_userdata(ConfigUd)?;
        lua.create_userdata(ModeUd(Rc::new(RefCell::new(Box::new(ControlOnlyMode)))))?;
        lua.create_userdata(ViewUd(Rc::new(RefCell::new(Box::new(ControlOnlyView)))))?;
        lua.create_userdata(CommandUd(builtin::create_command("Play", CommandOptions::default()).map_err(se)?))?;
        lua.create_userdata(ProviderUd(builtin::empty_provider()))?;
        lua.create_userdata(ParameterUd(ParamRef::Empty))?;
        Ok(())
    };
    let result = collect();
    let entries = API_DOC.with(|d| d.borrow_mut().take()).unwrap_or_default();
    if let Err(error) = result {
        return format!("Error: {error}\n");
    }
    let mut text = String::from(
        "# The Lua controller API\n\n\
Generated from the bindings (`src/script/lua/bindings.rs`) by `cargo test --lib -- --ignored write_lua_api` \
(the test `lua_api_is_up_to_date` checks it). See doc/LUA_PLAN.md for the design and \
`controllers/launch_control_xl.lua` for an example.\n\n\
## Conventions\n\n\
- A script returns a table: `info` (`vendor`, `model`, `uuid`, `class_name`, `midi_inputs`, `midi_outputs`), `discovery` \
(a function of the OS name `windows`/`linux`/`mac` or a table by OS, a list of `{inputs = {…}, outputs = {…}}`), `settings` \
(names of generic settings or `{name, \"global\"|\"document\"}`), `value_changer` (`{\"twos_complement\", upper_bound, step}`), \
`colors` (`{indices = {[color id] = index}, rgb = {[index] = {r, g, b} | DAW color name}, fill = {r, g, b}}`), `scales` \
(`{start_note, end_note, cols, rows}`), `model` (a table or a function of `{master_tracks_included = bool}`: `flat_track_list`, \
`full_flat_track_list`, `num_tracks`, `num_scenes`, `num_sends`, `num_devices_in_bank`, `num_params`, `num_drum_pad_layers`, \
`devices = {\"Eq\", \"FirstInstrument\"}`, `track_bank_indication`), `midi_input_filters`, and the functions `create(ctx)`, \
`startup(ctx)`, `handle_changes(ctx)`, `flush(ctx)` (default `ctx:flush_surface()`), `exit(ctx)` (default \
`ctx:turn_off_lights()`), `handle_sysex(ctx, hex)`, `handle_midi(ctx, status, data1, data2)`.\n\
- Indices are 0-based, as in the Rust and Java versions.\n\
- IDs are the names of the Rust enum variants: buttons (`\"Row11\"`), continuous controls (`\"Fader1\"`), modes (`\"Send\"`), \
views; parameter banks `CursorDevice`, `FirstInstrument`, `Eq`, `DrumDevice`, `CursorTrack`, `Project`; button events \
`\"down\"`, `\"up\"`, `\"long\"`; bind types `\"cc\"`, `\"note\"`, `\"pitchbend\"`, `\"program_change\"`.\n\
- `ctx` is read-only in lights, colors and values (functions which change something raise an error there).\n\
- A mode table has any of the functions of the `Mode` trait (`on_activate(self, ctx)`, `on_button(self, ctx, row, index, event)`, \
`button_color(self, ctx, button)`, `knob_value(self, ctx, index)`, …), an optional `name`, an optional `base` (a built-in \
mode) and an optional `forward` list: the methods without a function go to the base (default: all).\n\
- An error in a script function is logged with the script and the line; the call has no effect, and a function which fails \
three times in a row is disabled.\n\n",
    );
    for (ty, description) in TYPES {
        let functions: Vec<&ApiEntry> = entries.iter().filter(|e| e.ty == *ty).collect();
        text.push_str(&format!("## {ty}\n\n{description}\n\n"));
        for entry in functions {
            let prefix = match *ty {
                "dbm" => "dbm.",
                // A field
                _ if entry.signature.starts_with(" ->") => ".",
                _ => ":",
            };
            text.push_str(&format!("- `{prefix}{}{}`: {}\n", entry.name, entry.signature, entry.doc));
        }
        text.push('\n');
    }
    text.push_str(&format!("{} functions and fields.\n", entries.len()));
    text
}


// ------------------------------------------------------------------------------------------
// Helpers

pub(crate) fn rt(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

pub(crate) fn se(error: ScriptError) -> mlua::Error {
    mlua::Error::runtime(error.0)
}

fn stack(lua: &Lua) -> mlua::Result<Rc<CtxStack>> {
    lua.app_data_ref::<EngineData>().map(|d| d.stack.clone()).ok_or_else(|| rt("not a controller state"))
}

fn engine(lua: &Lua) -> mlua::Result<Weak<Engine>> {
    lua.app_data_ref::<EngineData>().map(|d| d.engine.borrow().clone()).ok_or_else(|| rt("not a controller state"))
}

/// Read the context of the running call.
fn read<R>(lua: &Lua, f: impl FnOnce(&Ctx) -> R) -> mlua::Result<R> {
    stack(lua)?.with(f).map_err(se)
}

/// Change the context of the running call.
fn write<R>(lua: &Lua, f: impl FnOnce(&mut Ctx) -> R) -> mlua::Result<R> {
    stack(lua)?.with_mut(f).map_err(se)
}

/// A 0-based index.
fn index(value: i64) -> mlua::Result<usize> {
    if (0..=100_000).contains(&value) {
        Ok(value as usize)
    } else {
        Err(rt(format!("invalid index {value}")))
    }
}

fn button(name: &str) -> mlua::Result<ButtonId> {
    names::button(name).map_err(se)
}

fn continuous(name: &str) -> mlua::Result<ContinuousId> {
    names::continuous(name).map_err(se)
}

fn mode_id(name: &str) -> mlua::Result<Modes> {
    names::mode(name).map_err(se)
}

/// Check the ID of a mode manager.
fn manager(ctx: &Ctx, manager: Option<i64>) -> mlua::Result<ModeManagerId> {
    let manager = manager.unwrap_or(0);
    if manager < 0 || manager as usize > ctx.extra_modes.len() {
        return Err(rt(format!("unknown mode manager {manager}")));
    }
    Ok(manager as ModeManagerId)
}

/// A mode switch (de-)activates the active mode, the new one and (on restore) the previous or
/// default one; none of them may run, otherwise the switch would fail (as in Rust, where it is
/// logged): the script gets an error.
fn check_modes_free(modes: &ModeManager, target: Option<Modes>) -> mlua::Result<()> {
    let ids = [modes.active_id(), modes.active_id_ignore_temporary(), target, modes.previous_id(), modes.default_id];
    for id in ids.into_iter().flatten() {
        if let Some(mode) = modes.get(id) {
            if mode.try_borrow_mut().is_err() {
                return Err(rt(format!("the mode {} is running; a mode cannot switch its own mode manager (use ctx:schedule)", id.name())));
            }
        }
    }
    Ok(())
}

fn mode_name(id: Option<Modes>) -> Option<&'static str> {
    id.map(Modes::name)
}

/// A step position.
fn position(channel: i32, step: i32, note: i32) -> NotePosition {
    NotePosition::new(channel, step, note)
}

fn step_state_name(state: StepState) -> &'static str {
    match state {
        StepState::Off => "off",
        StepState::Start => "start",
        StepState::Continue => "continue",
    }
}

/// A Lua function or a built-in command as a button command.
fn trigger_command(lua: &Lua, value: Value, what: String) -> mlua::Result<Option<SharedTrigger>> {
    match value {
        Value::Nil => Ok(None),
        Value::Function(function) => Ok(Some(Rc::new(RefCell::new(LuaTrigger { engine: engine(lua)?, function, what })))),
        Value::UserData(ud) => ud.borrow::<CommandUd>()?.0.trigger().map(Some).ok_or_else(|| rt("not a button command")),
        _ => Err(rt("a command is a function or a built-in command")),
    }
}

// ------------------------------------------------------------------------------------------
// Global functions

/// The global table `dbm`.
pub fn register_globals(lua: &Lua) -> mlua::Result<()> {
    let dbm = lua.create_table()?;
    let ty = "dbm";
    let add = |name: &'static str, signature: &'static str, doc: &'static str, f: Function| -> mlua::Result<()> {
        document(ty, name, signature, doc);
        dbm.raw_set(name, f)
    };
    add(
        "button",
        "(first, offset) -> string",
        "The button `offset` positions after `first` (former `ButtonID.get`), e.g. `dbm.button(\"Row11\", 2)` is `\"Row13\"`.",
        lua.create_function(|_, (first, offset): (String, i64)| {
            let first = button(&first)?;
            ButtonId::from_ordinal(first.ordinal() + index(offset)?).map(|b| b.name()).ok_or_else(|| rt("button out of range"))
        })?,
    )?;
    add(
        "continuous",
        "(first, offset) -> string",
        "The continuous control `offset` positions after `first`, e.g. `dbm.continuous(\"Fader1\", 7)`.",
        lua.create_function(|_, (first, offset): (String, i64)| {
            let first = continuous(&first)?;
            ContinuousId::from_ordinal(first.ordinal() + index(offset)?).map(|b| b.name()).ok_or_else(|| rt("control out of range"))
        })?,
    )?;
    add(
        "button_index",
        "(button, first, count) -> integer|nil",
        "The offset of `button` from `first` if it is one of the `count` buttons from there.",
        lua.create_function(|_, (b, first, count): (String, String, i64)| {
            let (b, first) = (button(&b)?, button(&first)?);
            let (o, f) = (b.ordinal() as i64, first.ordinal() as i64);
            Ok((o >= f && o < f + count).then_some(o - f))
        })?,
    )?;
    add(
        "resolution",
        "(index) -> number, string",
        "The value (in beats) and the name of a sequencer resolution (0-7, former `Resolution`).",
        lua.create_function(|_, i: i64| {
            let i = index(i)?.min(Resolution::ALL.len().saturating_sub(1));
            Ok((Resolution::value_at(i), Resolution::name_at(i)))
        })?,
    )?;
    add(
        "format_note",
        "(note, octave_offset) -> string",
        "The name and octave of a MIDI note (former `Scales.formatNoteAndOctave`).",
        lua.create_function(|_, (note, offset): (i32, i32)| Ok(Scales::format_note_and_octave(note, offset)))?,
    )?;
    add(
        "eq_parameter_index",
        "(kind, band) -> integer",
        "The index of a parameter of an equalizer band in the parameter bank `Eq`: kind `\"gain\"`, `\"q\"` or `\"frequency\"`.",
        lua.create_function(|_, (kind, band): (String, i64)| {
            let band = index(band)?;
            match kind.as_str() {
                "gain" => Ok(SpecificDevice::gain_parameter_index(band)),
                "q" => Ok(SpecificDevice::q_parameter_index(band)),
                "frequency" => Ok(SpecificDevice::frequency_parameter_index(band)),
                _ => Err(rt(format!("unknown equalizer parameter '{kind}'"))),
            }
        })?,
    )?;
    add(
        "from_hex",
        "(text) -> {integer}",
        "The bytes of a hex string (as `handle_sysex` gets it).",
        lua.create_function(|_, text: String| Ok(crate::midi::from_hex_str(&text).into_iter().map(i64::from).collect::<Vec<_>>()))?,
    )?;
    lua.globals().raw_set("dbm", dbm)?;
    Ok(())
}

// ------------------------------------------------------------------------------------------
// The context

/// The `ctx` of a call: the serial of its frame on the context stack.
pub struct CtxUd {
    pub serial: u64,
}

impl CtxUd {
    fn check(&self, lua: &Lua) -> mlua::Result<()> {
        if stack(lua)?.is_valid(self.serial) {
            Ok(())
        } else {
            Err(rt("ctx is used outside of the call it was passed to"))
        }
    }
}

/// The handles without state (the model, the surface, …) are created once per Lua state.
fn handle<T: UserData + 'static>(lua: &Lua, key: &str, make: impl FnOnce() -> T) -> mlua::Result<AnyUserData> {
    if let Some(ud) = lua.named_registry_value::<Option<AnyUserData>>(key)? {
        return Ok(ud);
    }
    let ud = lua.create_userdata(make())?;
    lua.set_named_registry_value(key, &ud)?;
    Ok(ud)
}

/// Options of the `ctx:create_mode` table.
fn mode_options(ctx: &Ctx, options: Option<Table>) -> mlua::Result<ModeOptions> {
    let mut result = ModeOptions::default();
    let Some(options) = options else {
        return Ok(result);
    };
    let _ = ctx;
    result.name = options.get("name")?;
    if let Some(controls) = options.get::<Option<Table>>("controls")? {
        for name in controls.sequence_values::<String>() {
            result.controls.push(continuous(&name?)?);
        }
    }
    result.is_absolute = options.get("absolute")?;
    if let Some(bank) = options.get::<Option<String>>("bank")? {
        result.bank = Some(builtin::mode_bank(&bank).map_err(se)?);
    }
    if let Some(provider) = options.get::<Option<UserDataRef<ProviderUd>>>("provider")? {
        result.provider = Some(provider.0.clone());
    }
    if let Some(clip) = options.get::<Option<Table>>("clip")? {
        result.clip = Some((index(clip.get(1)?)?, index(clip.get(2)?)?));
    }
    result.send_index = options.get::<Option<i32>>("send")?.unwrap_or(0);
    Ok(result)
}

impl UserData for CtxUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        let ty = "Ctx";
        field!(fields, ty, "model", " -> Model", "The DAW model.", |lua, this| {
            this.check(lua)?;
            handle(lua, "dbm.model", || ModelUd)
        });
        field!(fields, ty, "surface", " -> Surface", "The control surface.", |lua, this| {
            this.check(lua)?;
            handle(lua, "dbm.surface", || SurfaceUd)
        });
        field!(fields, ty, "host", " -> Host", "The host.", |lua, this| {
            this.check(lua)?;
            handle(lua, "dbm.host", || HostUd)
        });
        field!(fields, ty, "config", " -> Config", "The configuration.", |lua, this| {
            this.check(lua)?;
            handle(lua, "dbm.config", || ConfigUd)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Ctx";
        method!(methods, ty, "is_valid", "() -> bool", "True during the call which got this ctx.", |lua, this, ()| Ok(stack(lua)?.is_valid(this.serial)));
        method!(methods, ty, "add_mode_manager", "() -> integer", "Add a mode manager (0 is the main one).", |lua, this, ()| {
            this.check(lua)?;
            write(lua, |ctx| ctx.add_mode_manager() as i64)
        });
        method!(
            methods,
            ty,
            "create_mode",
            "(kind, options) -> Mode",
            "Create a built-in mode: `Parameter`, `Sequencer`, `Dummy`, `MasterAndFXVolume`, `Track`, `TrackSelect`, `TrackMute`, `TrackSolo`, `TrackRecArm`, `TrackStopClip`, `TrackVolume`, `TrackPan`, `TrackSend`, `LayerMute`, `LayerSolo`. Options: `name`, `controls` (list of IDs), `absolute` (default true), `bank` (`none`, `track`, a parameter bank, `drum_pads:FirstInstrument|Drum|Cursor`), `provider`, `clip = {steps, rows}`, `send`.",
            |lua, this, (kind, options): (String, Option<Table>)| {
                this.check(lua)?;
                let options = read(lua, |ctx| mode_options(ctx, options))??;
                let mode = write(lua, |ctx| builtin::create_mode(ctx, &kind, options))?.map_err(se)?;
                Ok(ModeUd(Rc::new(RefCell::new(mode))))
            }
        );
        method!(
            methods,
            ty,
            "register_mode_in",
            "(manager, id, mode)",
            "Register a mode (a table or a built-in mode) in a mode manager.",
            |lua, this, (m, id, mode): (i64, String, Value)| {
                this.check(lua)?;
                let id = mode_id(&id)?;
                let mode = LuaMode::from_value(lua, engine(lua)?, mode)?;
                write(lua, |ctx| -> mlua::Result<()> {
                    let m = manager(ctx, Some(m))?;
                    ctx.register_mode_in(m, id, mode);
                    Ok(())
                })?
            }
        );
        method!(methods, ty, "register_mode", "(id, mode)", "Register a mode in the main mode manager.", |lua, this, (id, mode): (String, Value)| {
            this.check(lua)?;
            let id = mode_id(&id)?;
            let mode = LuaMode::from_value(lua, engine(lua)?, mode)?;
            write(lua, |ctx| ctx.register_mode(id, mode))
        });
        method!(methods, ty, "set_active_mode_in", "(manager, id)", "Activate a mode.", |lua, this, (m, id): (i64, String)| {
            this.check(lua)?;
            let id = mode_id(&id)?;
            write(lua, |ctx| -> mlua::Result<()> {
                let m = manager(ctx, Some(m))?;
                check_modes_free(ctx.mode_manager(m), Some(id))?;
                ctx.set_active_mode_in(m, id);
                Ok(())
            })?
        });
        method!(methods, ty, "set_active_mode", "(id)", "Activate a mode of the main mode manager.", |lua, this, id: String| {
            this.check(lua)?;
            let id = mode_id(&id)?;
            write(lua, |ctx| -> mlua::Result<()> {
                check_modes_free(&ctx.modes, Some(id))?;
                ctx.set_active_mode(id);
                Ok(())
            })?
        });
        method!(methods, ty, "set_temporary_mode_in", "(manager, id)", "Activate a mode temporarily (see `restore_mode_in`).", |lua, this, (m, id): (i64, String)| {
            this.check(lua)?;
            let id = mode_id(&id)?;
            write(lua, |ctx| -> mlua::Result<()> {
                let m = manager(ctx, Some(m))?;
                check_modes_free(ctx.mode_manager(m), Some(id))?;
                ctx.set_temporary_mode_in(m, id);
                Ok(())
            })?
        });
        method!(methods, ty, "restore_mode_in", "(manager)", "End the temporary mode (or go back to the previous one).", |lua, this, m: i64| {
            this.check(lua)?;
            write(lua, |ctx| -> mlua::Result<()> {
                let m = manager(ctx, Some(m))?;
                check_modes_free(ctx.mode_manager(m), None)?;
                ctx.restore_mode_in(m);
                Ok(())
            })?
        });
        method!(methods, ty, "active_mode_id", "(manager = 0) -> string|nil", "The ID of the active mode (the temporary one if set).", |lua, this, m: Option<i64>| {
            this.check(lua)?;
            read(lua, |ctx| -> mlua::Result<Option<&'static str>> { Ok(mode_name(ctx.mode_manager(manager(ctx, m)?).active_id())) })?
        });
        method!(
            methods,
            ty,
            "active_mode_id_ignore_temporary",
            "(manager = 0) -> string|nil",
            "The ID of the active mode, not the temporary one.",
            |lua, this, m: Option<i64>| {
                this.check(lua)?;
                read(lua, |ctx| -> mlua::Result<Option<&'static str>> { Ok(mode_name(ctx.mode_manager(manager(ctx, m)?).active_id_ignore_temporary())) })?
            }
        );
        method!(methods, ty, "is_active_mode", "(id, manager = 0) -> bool", "True if the mode is active (or temporary).", |lua, this, (id, m): (String, Option<i64>)| {
            this.check(lua)?;
            let id = mode_id(&id)?;
            read(lua, |ctx| -> mlua::Result<bool> { Ok(ctx.mode_manager(manager(ctx, m)?).is_active(id)) })?
        });
        method!(
            methods,
            ty,
            "mode_button_color",
            "(manager, button) -> integer",
            "The color of a button from the active mode of the manager (0 without an active mode).",
            |lua, this, (m, b): (i64, String)| {
                this.check(lua)?;
                let b = button(&b)?;
                read(lua, |ctx| -> mlua::Result<i32> {
                    let Some(mode) = ctx.mode_manager(manager(ctx, Some(m))?).active() else {
                        return Ok(0);
                    };
                    let mode = mode.try_borrow().map_err(|_| rt("the active mode is running (re-entrant call)"))?;
                    Ok(mode.button_color(ctx, b))
                })?
            }
        );
        method!(
            methods,
            ty,
            "mode_knob_value",
            "(manager, index) -> integer",
            "The value of a knob from the active mode of the manager (0 without an active mode).",
            |lua, this, (m, i): (i64, i64)| {
                this.check(lua)?;
                let i = index(i)?;
                read(lua, |ctx| -> mlua::Result<i32> {
                    let Some(mode) = ctx.mode_manager(manager(ctx, Some(m))?).active() else {
                        return Ok(0);
                    };
                    let mode = mode.try_borrow().map_err(|_| rt("the active mode is running (re-entrant call)"))?;
                    Ok(mode.knob_value(ctx, i))
                })?
            }
        );
        method!(methods, ty, "create_view", "(kind) -> View", "Create a built-in view: `ControlOnly`.", |lua, this, kind: String| {
            this.check(lua)?;
            match kind.as_str() {
                "ControlOnly" => Ok(ViewUd(Rc::new(RefCell::new(Box::new(ControlOnlyView))))),
                _ => Err(rt(format!("unknown built-in view '{kind}', one of ControlOnly"))),
            }
        });
        method!(methods, ty, "register_view", "(id, view)", "Register a view (a table or a built-in view).", |lua, this, (id, view): (String, Value)| {
            this.check(lua)?;
            let id = names::view(&id).map_err(se)?;
            let view = LuaView::from_value(engine(lua)?, view)?;
            write(lua, |ctx| ctx.register_view(id, view))
        });
        method!(methods, ty, "set_active_view", "(id)", "Activate a view.", |lua, this, id: String| {
            this.check(lua)?;
            let id = names::view(&id).map_err(se)?;
            write(lua, |ctx| -> mlua::Result<()> {
                for view in [ctx.views.active(), ctx.views.get(id)].into_iter().flatten() {
                    if view.try_borrow_mut().is_err() {
                        return Err(rt("the view is running; a view cannot switch the views (use ctx:schedule)"));
                    }
                }
                ctx.set_active_view(id);
                Ok(())
            })?
        });
        method!(methods, ty, "active_view_id", "() -> string|nil", "The ID of the active view.", |lua, this, ()| {
            this.check(lua)?;
            read(lua, |ctx| ctx.views.active_id().map(|v| v.name()))
        });
        method!(
            methods,
            ty,
            "command",
            "(kind, options) -> Command",
            "Create a built-in command: `ButtonRowMode` (`row`, `index`, `manager`), `KnobRowMode` (`index`, `manager`), `ModeCursor`/`Cursor` (`direction` left/right/up/down, `notify`), `ModeSelect` (`mode`, `manager`, `toggle`, `notify`, `temporary`), `New`, `Tempo` (`increase`), `Wind` (`forward`), `Play`.",
            |lua, this, (kind, options): (String, Option<Table>)| {
                this.check(lua)?;
                let mut o = CommandOptions::default();
                if let Some(t) = options {
                    o.row = index(t.get::<Option<i64>>("row")?.unwrap_or(0))?;
                    o.index = index(t.get::<Option<i64>>("index")?.unwrap_or(0))?;
                    if let Some(m) = t.get::<Option<i64>>("manager")? {
                        o.manager = Some(read(lua, |ctx| manager(ctx, Some(m)))??);
                    }
                    if let Some(d) = t.get::<Option<String>>("direction")? {
                        o.direction = Some(names::direction(&d).map_err(se)?);
                    }
                    o.notify = t.get::<Option<bool>>("notify")?.unwrap_or(false);
                    o.increase = t.get::<Option<bool>>("increase")?.or(t.get::<Option<bool>>("forward")?).unwrap_or(false);
                    if let Some(m) = t.get::<Option<String>>("mode")? {
                        o.mode = Some(mode_id(&m)?);
                    }
                    o.toggle = t.get::<Option<bool>>("toggle")?.unwrap_or(false);
                    o.temporary = t.get::<Option<bool>>("temporary")?.unwrap_or(false);
                }
                Ok(CommandUd(builtin::create_command(&kind, o).map_err(se)?))
            }
        );
        method!(
            methods,
            ty,
            "provider",
            "(kind, ...) -> Provider",
            "Create a parameter provider: `volume`, `pan`, `send` (index, offset = 0) of the tracks; `bank` (name) of a parameter bank; `layer_volume`, `layer_pan`, `layer_send` (index) of the first instrument; `combined` ({providers}); `fixed` ({parameters}); `empty` (size).",
            |lua, this, (kind, a, b): (String, Value, Option<i64>)| {
                this.check(lua)?;
                let spec = match kind.as_str() {
                    "volume" => ProviderSpec::Volume,
                    "pan" => ProviderSpec::Pan,
                    "send" => ProviderSpec::Send(lua.unpack::<i32>(a)?, index(b.unwrap_or(0))?),
                    "bank" => ProviderSpec::Bank(names::parameter_bank(&lua.unpack::<String>(a)?).map_err(se)?),
                    "layer_volume" => ProviderSpec::LayerVolume,
                    "layer_pan" => ProviderSpec::LayerPan,
                    "layer_send" => ProviderSpec::LayerSend(index(lua.unpack::<i64>(a)?)?),
                    "combined" => {
                        let list: Table = lua.unpack(a)?;
                        let providers: mlua::Result<Vec<SharedProvider>> = list.sequence_values::<UserDataRef<ProviderUd>>().map(|p| p.map(|p| p.0.clone())).collect();
                        ProviderSpec::Combined(providers?)
                    }
                    "fixed" => {
                        let list: Table = lua.unpack(a)?;
                        let parameters: mlua::Result<Vec<ParamRef>> = list.sequence_values::<UserDataRef<ParameterUd>>().map(|p| p.map(|p| p.0.clone())).collect();
                        ProviderSpec::Fixed(parameters?)
                    }
                    "empty" => ProviderSpec::Empty(index(lua.unpack::<i64>(a)?)?),
                    _ => return Err(rt(format!("unknown parameter provider '{kind}'"))),
                };
                read(lua, |ctx| ProviderUd(builtin::create_provider(ctx, spec)))
            }
        );
        method!(
            methods,
            ty,
            "parameter",
            "(kind, ...) -> Parameter",
            "Create a parameter: `empty`; `bank_item` (bank, index); `bank_position` (bank, position); `track_volume`, `track_pan` (index); `track_send` (index, send); `selected_track_volume`, `selected_track_pan`; `selected_track_send` (send); `master_volume`, `master_pan`, `metronome_volume`, `crossfader`; `eq_band_type` (band); `custom` (a table with `does_exist`, `value`, `set_normalized_value`, `change_value` and optionally `name`, `displayed_value`, `set_value`, `reset_value`, `touch_value`, each `function (self, ctx, …)`).",
            |lua, this, (kind, a, b): (String, Value, Option<i64>)| {
                this.check(lua)?;
                let int = |v: Value| -> mlua::Result<usize> { index(lua.unpack::<i64>(v)?) };
                let bank = |v: Value| -> mlua::Result<ParameterBankId> { names::parameter_bank(&lua.unpack::<String>(v)?).map_err(se) };
                let b = || index(b.unwrap_or(0));
                let parameter = match kind.as_str() {
                    "empty" => ParamRef::Empty,
                    "bank_item" => ParamRef::BankItem { bank: bank(a)?, index: b()? },
                    "bank_position" => ParamRef::BankPosition { bank: bank(a)?, position: b()? },
                    "track_volume" => ParamRef::TrackVolume { index: int(a)? },
                    "track_pan" => ParamRef::TrackPan { index: int(a)? },
                    "track_send" => ParamRef::TrackSend { index: int(a)?, send: b()? },
                    "selected_track_volume" => ParamRef::SelectedTrackVolume,
                    "selected_track_pan" => ParamRef::SelectedTrackPan,
                    "selected_track_send" => ParamRef::SelectedTrackSend { send: int(a)? },
                    "master_volume" => ParamRef::MasterVolume,
                    "master_pan" => ParamRef::MasterPan,
                    "metronome_volume" => ParamRef::MetronomeVolume,
                    "crossfader" => ParamRef::Crossfader,
                    "eq_band_type" => ParamRef::EqBandType { band: int(a)? },
                    "custom" => ParamRef::Custom(Rc::new(LuaParameter::new(engine(lua)?, lua.unpack::<Table>(a)?)?)),
                    _ => return Err(rt(format!("unknown parameter '{kind}'"))),
                };
                Ok(ParameterUd(parameter))
            }
        );
        method!(methods, ty, "color_index", "(color_id) -> integer", "The color index of a registered color ID (an error if it is not registered).", |lua, this, id: String| {
            this.check(lua)?;
            read(lua, |ctx| if ctx.colors.has_color_index(&id) { Ok(ctx.colors.color_index(&id)) } else { Err(rt(format!("the color '{id}' is not registered"))) })?
        });
        method!(methods, ty, "notify", "(text)", "Show a notification (on the display or by the host).", |lua, this, text: String| {
            this.check(lua)?;
            write(lua, |ctx| ctx.notify(&text))
        });
        method!(methods, ty, "schedule", "(delay_ms, function (ctx))", "Run a function later (also to switch the mode or view from inside it).", |lua, this, (delay, f): (i64, Function)| {
            this.check(lua)?;
            let task = task(engine(lua)?, f);
            write(lua, |ctx| ctx.schedule(delay.max(0) as u64, task))
        });
        method!(
            methods,
            ty,
            "is_button_combination",
            "(button) -> bool",
            "True if the button is pressed; then it is marked as used (its release does not execute its command).",
            |lua, this, b: String| {
                this.check(lua)?;
                let b = button(&b)?;
                write(lua, |ctx| ctx.is_button_combination(b))
            }
        );
        method!(methods, ty, "flush_surface", "()", "Update the surface: the active view, then the lights (the default `flush`).", |lua, this, ()| {
            this.check(lua)?;
            write(lua, |ctx| ctx.flush_surface())
        });
        method!(methods, ty, "turn_off_lights", "()", "Turn off all lights (the default `exit`).", |lua, this, ()| {
            this.check(lua)?;
            write(lua, |ctx| ctx.turn_off_lights())
        });
        method!(
            methods,
            ty,
            "notify_selected",
            "(what, ...)",
            "The notifications of the selection (former `MVHelper`): `track`, `layer`, `device`, `parameter_page`, `device_and_parameter_page`, `sends` (track index), `layer_sends` (layer index), `scene_page`, `edit_page` (steps, rows), `scale`.",
            |lua, this, (what, a, b): (String, Option<i64>, Option<i64>)| {
                this.check(lua)?;
                let a = index(a.unwrap_or(0))?;
                let b = index(b.unwrap_or(0))?;
                write(lua, |ctx| -> mlua::Result<()> {
                    match what.as_str() {
                        "track" => mv_helper::notify_selected_track(ctx),
                        "layer" => mv_helper::notify_selected_layer(ctx),
                        "device" => mv_helper::notify_selected_device(ctx),
                        "parameter_page" => mv_helper::notify_selected_parameter_page(ctx),
                        "device_and_parameter_page" => mv_helper::notify_selected_device_and_parameter_page(ctx),
                        "sends" => mv_helper::notify_selected_sends(ctx, a),
                        "layer_sends" => mv_helper::notify_selected_layer_sends(ctx, a),
                        "scene_page" => mv_helper::notify_scene_page(ctx),
                        "edit_page" => mv_helper::notify_edit_page(ctx, (a, b)),
                        "scale" => mv_helper::notify_scale(ctx),
                        _ => return Err(rt(format!("unknown notification '{what}'"))),
                    }
                    Ok(())
                })?
            }
        );
    }
}

// ------------------------------------------------------------------------------------------
// The model

pub struct ModelUd;

impl UserData for ModelUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Model";
        method!(methods, ty, "track_bank", "() -> TrackBank", "The track bank (former `getTrackBank`).", |_, _, ()| Ok(TrackBankUd { current: false }));
        method!(methods, ty, "current_track_bank", "() -> TrackBank", "The current track bank (instrument/audio or effect tracks).", |_, _, ()| Ok(TrackBankUd { current: true }));
        method!(methods, ty, "track", "(index) -> Track", "A track of the track bank.", |_, _, i: i64| Ok(TrackUd { current: false, index: index(i)? }));
        method!(methods, ty, "layer", "(index) -> Layer", "A layer (or drum pad) of the first instrument.", |_, _, i: i64| Ok(LayerUd { index: index(i)? }));
        method!(methods, ty, "scene_bank", "() -> SceneBank", "The scene bank.", |_, _, ()| Ok(SceneBankUd));
        method!(methods, ty, "scene", "(index) -> Scene", "A scene of the scene bank.", |_, _, i: i64| Ok(SceneUd { index: index(i)? }));
        method!(methods, ty, "transport", "() -> Transport", "The transport.", |_, _, ()| Ok(TransportUd));
        method!(methods, ty, "application", "() -> Application", "The application.", |_, _, ()| Ok(ApplicationUd));
        method!(methods, ty, "scales", "() -> Scales", "The scales.", |_, _, ()| Ok(ScalesUd));
        method!(methods, ty, "value_changer", "() -> ValueChanger", "The value changer.", |_, _, ()| Ok(ValueChangerUd));
        method!(methods, ty, "cursor_device", "() -> Device", "The cursor device.", |_, _, ()| Ok(DeviceUd { which: Which::Cursor }));
        method!(methods, ty, "device", "(name) -> Device", "A specific device: `FirstInstrument`, `Eq`, `NiKomplete`.", |_, _, name: String| {
            Ok(DeviceUd { which: Which::Specific(names::device(&name).map_err(se)?) })
        });
        method!(methods, ty, "drum_device", "() -> Device", "The (main) drum device.", |_, _, ()| Ok(DeviceUd { which: Which::Drum }));
        method!(methods, ty, "parameter_bank", "(name) -> ParameterBank", "A parameter bank.", |_, _, name: String| Ok(ParameterBankUd {
            bank: names::parameter_bank(&name).map_err(se)?
        }));
        method!(
            methods,
            ty,
            "note_clip",
            "(steps, rows) -> Clip|nil",
            "The cursor clip of the size; created if it does not exist (nil in a read-only call if it does not exist).",
            |lua, _, (cols, rows): (i64, i64)| {
                let (cols, rows) = (index(cols)?, index(rows)?);
                if stack(lua)?.is_mutable() {
                    write(lua, |ctx| {
                        ctx.model.note_clip(cols, rows);
                    })?;
                    return Ok(Some(ClipUd { cols, rows }));
                }
                read(lua, |ctx| ctx.model.note_clip_ref(cols, rows).map(|_| ClipUd { cols, rows }))
            }
        );
        method!(methods, ty, "is_launcher_overdub", "() -> bool", "True if launcher overdub is on.", |lua, _, ()| read(lua, |ctx| ctx.model.is_launcher_overdub()));
        method!(methods, ty, "toggle_launcher_overdub", "()", "Toggle launcher overdub.", |lua, _, ()| write(lua, |ctx| ctx.model.toggle_launcher_overdub()));
        method!(methods, ty, "can_selected_track_hold_notes", "() -> bool", "True if the selected track can hold notes.", |lua, _, ()| read(
            lua,
            |ctx| ctx.model.can_selected_track_hold_notes()
        ));
        method!(
            methods,
            ty,
            "take_events",
            "() -> {event}",
            "The events of the model since the last call (former observers): tables with `type` (`TrackSelectionChanged` with `index`, `is_selected`; `TrackBankPageChanged`; `SceneBankPageChanged`; `SendBankPageChanged` with `track_position`; `TrackNote` with `track_index`, `note`, `velocity`; `ParameterValueChanged` with `bank`, `page`, `index`; `ParameterPageChanged` with `bank`; `CurrentTrackBankChanged`).",
            |lua, _, ()| {
                let events = write(lua, |ctx| ctx.model.take_events())?;
                let list = lua.create_table()?;
                for event in events {
                    list.raw_push(event_table(lua, &event)?)?;
                }
                Ok(list)
            }
        );
    }
}

fn bank_name(bank: ParameterBankId) -> &'static str {
    names::PARAMETER_BANKS.iter().find(|(_, b)| *b == bank).map_or("", |(n, _)| *n)
}

fn event_table(lua: &Lua, event: &ModelEvent) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    match event {
        ModelEvent::TrackSelectionChanged { index, is_selected } => {
            t.set("type", "TrackSelectionChanged")?;
            t.set("index", *index)?;
            t.set("is_selected", *is_selected)?;
        }
        ModelEvent::TrackBankPageChanged => t.set("type", "TrackBankPageChanged")?,
        ModelEvent::SceneBankPageChanged => t.set("type", "SceneBankPageChanged")?,
        ModelEvent::SendBankPageChanged { track_position } => {
            t.set("type", "SendBankPageChanged")?;
            t.set("track_position", *track_position)?;
        }
        ModelEvent::TrackNote { track_index, note, velocity } => {
            t.set("type", "TrackNote")?;
            t.set("track_index", *track_index)?;
            t.set("note", *note)?;
            t.set("velocity", *velocity)?;
        }
        ModelEvent::ParameterValueChanged { bank, page, index } => {
            t.set("type", "ParameterValueChanged")?;
            t.set("bank", bank_name(*bank))?;
            t.set("page", *page)?;
            t.set("index", *index)?;
        }
        ModelEvent::ParameterPageChanged { bank } => {
            t.set("type", "ParameterPageChanged")?;
            t.set("bank", bank_name(*bank))?;
        }
        ModelEvent::CurrentTrackBankChanged => t.set("type", "CurrentTrackBankChanged")?,
    }
    Ok(t)
}

// Track banks and tracks

pub struct TrackBankUd {
    current: bool,
}

impl UserData for TrackBankUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "TrackBank";
        method!(methods, ty, "item", "(index) -> Track", "A track of the current page.", |_, this, i: i64| Ok(TrackUd { current: this.current, index: index(i)? }));
        method!(methods, ty, "selected_index", "() -> integer|nil", "The index of the selected track in the page.", |lua, this, ()| read(lua, |ctx| {
            let bank = if this.current { ctx.model.current_track_bank() } else { ctx.model.track_bank() };
            (0..bank.page_size()).find(|i| bank.item(*i).is_selected())
        }));
        method!(methods, ty, "set_skip_disabled_items", "(skip)", "Exclude the deactivated tracks.", |lua, this, skip: bool| write(lua, |ctx| {
            let bank = if this.current { ctx.model.current_track_bank_mut() } else { ctx.model.track_bank_mut() };
            bank.set_skip_disabled_items(skip)
        }));
        method!(methods, ty, "set_indication", "(enable)", "Show the bank in the DAW.", |lua, this, enable: bool| write(lua, |ctx| {
            let bank = if this.current { ctx.model.current_track_bank_mut() } else { ctx.model.track_bank_mut() };
            bank.set_indication(enable)
        }));
        method!(methods, ty, "clear_mute", "()", "Unmute all tracks.", |lua, this, ()| write(lua, |ctx| {
            let bank = if this.current { ctx.model.current_track_bank_mut() } else { ctx.model.track_bank_mut() };
            bank.clear_mute()
        }));
        method!(methods, ty, "clear_solo", "()", "Unsolo all tracks.", |lua, this, ()| write(lua, |ctx| {
            let bank = if this.current { ctx.model.current_track_bank_mut() } else { ctx.model.track_bank_mut() };
            bank.clear_solo()
        }));
    }
}

pub struct TrackUd {
    current: bool,
    index: usize,
}

impl TrackUd {
    fn read<R>(&self, lua: &Lua, f: impl FnOnce(&crate::daw::track::Track) -> R) -> mlua::Result<R> {
        read(lua, |ctx| {
            let bank = if self.current { ctx.model.current_track_bank() } else { ctx.model.track_bank() };
            f(bank.item(self.index))
        })
    }

    fn write<R>(&self, lua: &Lua, f: impl FnOnce(&mut crate::daw::track::Track) -> R) -> mlua::Result<R> {
        write(lua, |ctx| {
            let bank = if self.current { ctx.model.current_track_bank_mut() } else { ctx.model.track_bank_mut() };
            f(bank.item_mut(self.index))
        })
    }
}

impl UserData for TrackUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Track";
        method!(methods, ty, "exists", "() -> bool", "True if the track exists.", |lua, this, ()| this.read(lua, |t| t.does_exist()));
        method!(methods, ty, "is_selected", "() -> bool", "True if the track is selected.", |lua, this, ()| this.read(lua, |t| t.is_selected()));
        method!(methods, ty, "select", "()", "Select the track.", |lua, this, ()| this.write(lua, |t| t.select()));
        method!(methods, ty, "name", "() -> string", "The name.", |lua, this, ()| this.read(lua, |t| t.name().to_string()));
        method!(methods, ty, "position", "() -> integer", "The absolute position (-1 if it does not exist).", |lua, this, ()| this.read(lua, |t| t.position()));
        method!(methods, ty, "is_group", "() -> bool", "True for a group track.", |lua, this, ()| this.read(lua, |t| t.is_group()));
        method!(methods, ty, "toggle_group_expanded", "()", "Expand or collapse the group.", |lua, this, ()| this.write(lua, |t| t.toggle_group_expanded()));
        method!(methods, ty, "is_mute", "() -> bool", "True if muted.", |lua, this, ()| this.read(lua, |t| t.is_mute()));
        method!(methods, ty, "is_solo", "() -> bool", "True if soloed.", |lua, this, ()| this.read(lua, |t| t.is_solo()));
        method!(methods, ty, "is_rec_arm", "() -> bool", "True if armed for recording.", |lua, this, ()| this.read(lua, |t| t.is_rec_arm()));
        method!(methods, ty, "can_hold_notes", "() -> bool", "True for an instrument track.", |lua, this, ()| this.read(lua, |t| t.can_hold_notes()));
        method!(methods, ty, "volume", "() -> integer", "The volume (in the range of the value changer).", |lua, this, ()| this.read(lua, |t| t.volume()));
        method!(methods, ty, "pan", "() -> integer", "The panorama (in the range of the value changer).", |lua, this, ()| this.read(lua, |t| t.pan()));
        method!(methods, ty, "add_equalizer_device", "()", "Add an equalizer device to the track.", |lua, this, ()| this.write(lua, |t| t.add_equalizer_device()));
        method!(methods, ty, "send_bank", "() -> SendBank", "The sends of the track.", |_, this, ()| Ok(SendBankUd { owner: Owner::Track(this.current, this.index) }));
    }
}

/// A layer (or drum pad) of the first instrument (`framework::mode::layer::layer`).
pub struct LayerUd {
    index: usize,
}

impl LayerUd {
    fn read<R: Default>(&self, lua: &Lua, f: impl FnOnce(&Channel) -> R) -> mlua::Result<R> {
        read(lua, |ctx| layer::layer(ctx, self.index).map(f).unwrap_or_default())
    }
}

impl UserData for LayerUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Layer";
        method!(methods, ty, "exists", "() -> bool", "True if the layer exists.", |lua, this, ()| this.read(lua, |l| l.does_exist()));
        method!(methods, ty, "is_selected", "() -> bool", "True if the layer is selected.", |lua, this, ()| this.read(lua, |l| l.is_selected()));
        method!(methods, ty, "name", "() -> string", "The name.", |lua, this, ()| this.read(lua, |l| l.name().to_string()));
        method!(methods, ty, "is_mute", "() -> bool", "True if muted.", |lua, this, ()| this.read(lua, |l| l.is_mute()));
        method!(methods, ty, "is_solo", "() -> bool", "True if soloed.", |lua, this, ()| this.read(lua, |l| l.is_solo()));
        method!(methods, ty, "volume", "() -> integer", "The volume.", |lua, this, ()| this.read(lua, |l| l.volume()));
        method!(methods, ty, "pan", "() -> integer", "The panorama.", |lua, this, ()| this.read(lua, |l| l.pan()));
        method!(methods, ty, "send_bank", "() -> SendBank", "The sends of the layer.", |_, this, ()| Ok(SendBankUd { owner: Owner::Layer(this.index) }));
    }
}

#[derive(Clone, Copy)]
enum Owner {
    Track(bool, usize),
    Layer(usize),
}

pub struct SendBankUd {
    owner: Owner,
}

impl SendBankUd {
    fn read<R: Default>(&self, lua: &Lua, f: impl FnOnce(&crate::daw::channel::SendBank) -> R) -> mlua::Result<R> {
        read(lua, |ctx| match self.owner {
            Owner::Track(current, index) => {
                let bank = if current { ctx.model.current_track_bank() } else { ctx.model.track_bank() };
                f(bank.item(index).send_bank())
            }
            Owner::Layer(index) => layer::layer(ctx, index).map(|l| f(l.send_bank())).unwrap_or_default(),
        })
    }

    fn write(&self, lua: &Lua, f: impl FnOnce(&mut crate::daw::channel::SendBank)) -> mlua::Result<()> {
        write(lua, |ctx| match self.owner {
            Owner::Track(current, index) => {
                let bank = if current { ctx.model.current_track_bank_mut() } else { ctx.model.track_bank_mut() };
                f(bank.item_mut(index).send_bank_mut())
            }
            Owner::Layer(index) => {
                if let Some(l) = layer::layer_mut(ctx, index) {
                    f(l.send_bank_mut())
                }
            }
        })
    }
}

impl UserData for SendBankUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "SendBank";
        method!(methods, ty, "value", "(index) -> integer", "The value of the send at the index of the page.", |lua, this, i: i64| {
            let i = index(i)?;
            this.read(lua, |b| b.item(i).value())
        });
        method!(methods, ty, "exists", "(index) -> bool", "True if the send at the index of the page exists.", |lua, this, i: i64| {
            let i = index(i)?;
            this.read(lua, |b| b.item(i).does_exist())
        });
        method!(methods, ty, "select_next_page", "()", "Scroll to the next page.", |lua, this, ()| this.write(lua, |b| b.select_next_page()));
        method!(methods, ty, "select_previous_page", "()", "Scroll to the previous page.", |lua, this, ()| this.write(lua, |b| b.select_previous_page()));
        method!(methods, ty, "can_scroll_page_forwards", "() -> bool", "True if there is a next page.", |lua, this, ()| this.read(lua, |b| b.can_scroll_page_forwards()));
        method!(methods, ty, "can_scroll_page_backwards", "() -> bool", "True if there is a previous page.", |lua, this, ()| this.read(lua, |b| b.can_scroll_page_backwards()));
    }
}

// Scenes

pub struct SceneBankUd;

impl UserData for SceneBankUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "SceneBank";
        method!(methods, ty, "item", "(index) -> Scene", "A scene of the current page.", |_, _, i: i64| Ok(SceneUd { index: index(i)? }));
        method!(methods, ty, "select_next_page", "()", "Scroll to the next page.", |lua, _, ()| write(lua, |ctx| ctx.model.scene_bank_mut().select_next_page()));
        method!(methods, ty, "select_previous_page", "()", "Scroll to the previous page.", |lua, _, ()| write(lua, |ctx| ctx.model.scene_bank_mut().select_previous_page()));
        method!(methods, ty, "can_scroll_page_forwards", "() -> bool", "True if there is a next page.", |lua, _, ()| read(lua, |ctx| ctx.model.scene_bank().can_scroll_page_forwards()));
        method!(methods, ty, "can_scroll_page_backwards", "() -> bool", "True if there is a previous page.", |lua, _, ()| read(lua, |ctx| ctx.model.scene_bank().can_scroll_page_backwards()));
    }
}

pub struct SceneUd {
    index: usize,
}

impl UserData for SceneUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Scene";
        method!(methods, ty, "exists", "() -> bool", "True if the scene exists.", |lua, this, ()| read(lua, |ctx| ctx.model.scene_bank().item(this.index).does_exist()));
        method!(methods, ty, "is_selected", "() -> bool", "True if the scene is selected.", |lua, this, ()| read(lua, |ctx| ctx.model.scene_bank().item(this.index).is_selected()));
        method!(methods, ty, "name", "() -> string", "The name.", |lua, this, ()| read(lua, |ctx| ctx.model.scene_bank().item(this.index).name()));
        method!(methods, ty, "select", "()", "Select the scene.", |lua, this, ()| write(lua, |ctx| ctx.model.scene_bank_mut().item_mut(this.index).select()));
        method!(methods, ty, "launch", "(is_pressed, is_alternative)", "Launch the scene (on press and release).", |lua, this, (pressed, alternative): (bool, bool)| write(
            lua,
            |ctx| ctx.model.scene_bank_mut().item_mut(this.index).launch(pressed, alternative)
        ));
    }
}

// Devices

#[derive(Clone, Copy)]
enum Which {
    Cursor,
    Specific(DeviceId),
    Drum,
}

fn device_of(ctx: &Ctx, which: Which) -> Option<&SpecificDevice> {
    match which {
        Which::Cursor => Some(ctx.model.cursor_device()),
        Which::Specific(id) => ctx.model.specific_device(id),
        Which::Drum => ctx.model.drum_device(),
    }
}

fn device_of_mut(ctx: &mut Ctx, which: Which) -> Option<&mut SpecificDevice> {
    match which {
        Which::Cursor => Some(ctx.model.cursor_device_mut()),
        Which::Specific(id) => ctx.model.specific_device_mut(id),
        Which::Drum => ctx.model.drum_device_mut(),
    }
}

pub struct DeviceUd {
    which: Which,
}

impl DeviceUd {
    fn read<R: Default>(&self, lua: &Lua, f: impl FnOnce(&SpecificDevice) -> R) -> mlua::Result<R> {
        read(lua, |ctx| device_of(ctx, self.which).map(f).unwrap_or_default())
    }

    fn write(&self, lua: &Lua, f: impl FnOnce(&mut SpecificDevice)) -> mlua::Result<()> {
        write(lua, |ctx| {
            if let Some(device) = device_of_mut(ctx, self.which) {
                f(device)
            }
        })
    }
}

impl UserData for DeviceUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Device";
        method!(methods, ty, "exists", "() -> bool", "True if the device exists.", |lua, this, ()| this.read(lua, |d| d.does_exist()));
        method!(methods, ty, "name", "() -> string", "The name.", |lua, this, ()| this.read(lua, |d| d.name().to_string()));
        method!(methods, ty, "select_next", "()", "Select the next device.", |lua, this, ()| this.write(lua, |d| d.select_next()));
        method!(methods, ty, "select_previous", "()", "Select the previous device.", |lua, this, ()| this.write(lua, |d| d.select_previous()));
        method!(methods, ty, "can_select_next", "() -> bool", "True if there is a next device.", |lua, this, ()| this.read(lua, |d| d.can_select_next()));
        method!(methods, ty, "can_select_previous", "() -> bool", "True if there is a previous device.", |lua, this, ()| this.read(lua, |d| d.can_select_previous()));
        method!(methods, ty, "is_window_open", "() -> bool", "True if the plug-in window is open.", |lua, this, ()| this.read(lua, |d| d.is_window_open()));
        method!(methods, ty, "toggle_window_open", "()", "Open or close the plug-in window.", |lua, this, ()| this.write(lua, |d| d.toggle_window_open()));
        method!(
            methods,
            ty,
            "eq_band_type",
            "(band) -> string",
            "The type of an equalizer band: `Off`, `Lowcut`, `Lowshelf`, `Bell`, `Highcut`, `Highshelf`, `Notch`.",
            |lua, this, band: i64| {
                let band = index(band)?;
                read(lua, |ctx| device_of(ctx, this.which).map_or("Off".to_string(), |d| format!("{:?}", d.type_id(band))))
            }
        );
        method!(methods, ty, "drum_pad", "(index) -> DrumPad", "A drum pad of the current page.", |_, this, i: i64| Ok(DrumPadUd { which: this.which, index: index(i)? }));
        method!(methods, ty, "has_soloed_pads", "() -> bool", "True if a drum pad is soloed.", |lua, this, ()| this.read(lua, |d| d.drum_pad_bank().has_soloed_pads()));
        method!(
            methods,
            ty,
            "scroll_drum_pads_to",
            "(position, adjust_page)",
            "Scroll the drum pad bank to the position.",
            |lua, this, (position, adjust): (i64, bool)| {
                let position = index(position)?;
                this.write(lua, |d| d.drum_pad_bank_mut().scroll_to(position, adjust))
            }
        );
    }
}

pub struct DrumPadUd {
    which: Which,
    index: usize,
}

impl DrumPadUd {
    fn read<R: Default>(&self, lua: &Lua, f: impl FnOnce(&Channel) -> R) -> mlua::Result<R> {
        read(lua, |ctx| device_of(ctx, self.which).map(|d| f(d.drum_pad_bank().item(self.index))).unwrap_or_default())
    }
}

impl UserData for DrumPadUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "DrumPad";
        method!(methods, ty, "exists", "() -> bool", "True if the pad exists.", |lua, this, ()| this.read(lua, |p| p.does_exist()));
        method!(methods, ty, "is_activated", "() -> bool", "True if the pad is activated.", |lua, this, ()| this.read(lua, |p| p.is_activated()));
        method!(methods, ty, "is_mute", "() -> bool", "True if muted.", |lua, this, ()| this.read(lua, |p| p.is_mute()));
        method!(methods, ty, "is_solo", "() -> bool", "True if soloed.", |lua, this, ()| this.read(lua, |p| p.is_solo()));
        method!(methods, ty, "name", "() -> string", "The name.", |lua, this, ()| this.read(lua, |p| p.name().to_string()));
    }
}

pub struct ParameterBankUd {
    bank: ParameterBankId,
}

impl UserData for ParameterBankUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "ParameterBank";
        method!(methods, ty, "exists", "() -> bool", "True if the bank exists in the model.", |lua, this, ()| read(lua, |ctx| parameter_bank(&ctx.model, this.bank).is_some()));
        method!(methods, ty, "value", "(index) -> integer", "The value of the parameter at the index of the page (0 if there is none).", |lua, this, i: i64| {
            let i = index(i)?;
            read(lua, |ctx| parameter_bank(&ctx.model, this.bank).map_or(0, |b| b.item(i).value()))
        });
        method!(methods, ty, "page_count", "() -> integer", "The number of parameter pages.", |lua, this, ()| read(lua, |ctx| parameter_bank(&ctx.model, this.bank)
            .map_or(0, |b| b.page_bank().item_count())));
        method!(methods, ty, "selected_page", "() -> integer", "The index of the selected page.", |lua, this, ()| read(lua, |ctx| parameter_bank(&ctx.model, this.bank)
            .map_or(-1, |b| b.page_bank().selected_item_index() as i64)));
        method!(methods, ty, "select_page", "(index)", "Select a parameter page.", |lua, this, i: i64| {
            let i = index(i)?;
            write(lua, |ctx| {
                if let Some(bank) = parameter_bank_mut(&mut ctx.model, this.bank) {
                    bank.select_page(i);
                }
            })
        });
    }
}

// Clips

pub struct ClipUd {
    cols: usize,
    rows: usize,
}

impl ClipUd {
    fn read<R: Default>(&self, lua: &Lua, f: impl FnOnce(&CursorClip) -> R) -> mlua::Result<R> {
        read(lua, |ctx| ctx.model.note_clip_ref(self.cols, self.rows).map(f).unwrap_or_default())
    }

    fn write<R>(&self, lua: &Lua, f: impl FnOnce(&mut CursorClip) -> R) -> mlua::Result<R> {
        write(lua, |ctx| f(ctx.model.note_clip(self.cols, self.rows)))
    }
}

impl UserData for ClipUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Clip";
        method!(methods, ty, "exists", "() -> bool", "True if the clip exists in the DAW.", |lua, this, ()| this.read(lua, |c| c.does_exist()));
        method!(methods, ty, "is_host_grid", "() -> bool", "True if the step grid comes from the host (all note attributes).", |lua, this, ()| this
            .read(lua, |c| c.is_host_grid()));
        method!(methods, ty, "num_steps", "() -> integer", "The number of steps of a page.", |lua, this, ()| this.read(lua, |c| c.num_steps()));
        method!(methods, ty, "edit_page", "() -> integer", "The edited page.", |lua, this, ()| this.read(lua, |c| c.edit_page()));
        method!(methods, ty, "current_step", "() -> integer", "The playing step (-1 if not playing).", |lua, this, ()| read(lua, |ctx| ctx
            .model
            .note_clip_ref(this.cols, this.rows)
            .map_or(-1, |c| c.current_step())));
        method!(methods, ty, "loop_length", "() -> number", "The loop length in beats.", |lua, this, ()| this.read(lua, |c| c.loop_length()));
        method!(methods, ty, "set_loop_length", "(beats)", "Set the loop length.", |lua, this, beats: f64| this.write(lua, |c| c.set_loop_length(beats)));
        method!(methods, ty, "set_step_length", "(beats)", "Set the length of a step.", |lua, this, beats: f64| this.write(lua, |c| c.set_step_length(beats)));
        method!(
            methods,
            ty,
            "step",
            "(channel, step, note) -> table",
            "The step: `state` (`off`, `start`, `continue`), `muted`, `velocity`, `chance`, `repeat_count`, `velocity_spread`, `pan`, `formatted_repeat_count`.",
            |lua, this, (channel, step, note): (i32, i32, i32)| {
                let info = this.read(lua, |c| c.step(&position(channel, step, note)).clone())?;
                let t = lua.create_table()?;
                t.set("state", step_state_name(info.state()))?;
                t.set("muted", info.is_muted())?;
                t.set("velocity", info.velocity())?;
                t.set("chance", info.chance())?;
                t.set("repeat_count", info.repeat_count())?;
                t.set("velocity_spread", info.velocity_spread())?;
                t.set("pan", info.pan())?;
                t.set("formatted_repeat_count", info.formatted_repeat_count())?;
                Ok(t)
            }
        );
        method!(methods, ty, "highest_row", "(channel, step) -> integer", "The highest note of the step (-1 if none).", |lua, this, (channel, step): (i32, i32)| read(
            lua,
            |ctx| ctx.model.note_clip_ref(this.cols, this.rows).map_or(-1, |c| c.highest_row(channel, step))
        ));
        method!(methods, ty, "toggle_step", "(channel, step, note, velocity)", "Toggle the note of a step.", |lua, this, (ch, s, n, v): (i32, i32, i32, i32)| this.write(
            lua,
            |c| c.toggle_step(&position(ch, s, n), v)
        ));
        method!(methods, ty, "clear_step", "(channel, step, note)", "Remove the note of a step.", |lua, this, (ch, s, n): (i32, i32, i32)| this.write(lua, |c| c
            .clear_step(&position(ch, s, n))));
        method!(methods, ty, "move_step_y", "(channel, step, note, new_note)", "Move the note of a step to another note.", |lua, this, (ch, s, n, new): (i32, i32, i32, i32)| this.write(
            lua,
            |c| c.move_step_y(&position(ch, s, n), new)
        ));
        method!(methods, ty, "scroll_steps_page_forward", "()", "Edit the next page.", |lua, this, ()| this.write(lua, |c| c.scroll_steps_page_forward()));
        method!(methods, ty, "scroll_steps_page_backwards", "()", "Edit the previous page.", |lua, this, ()| this.write(lua, |c| c.scroll_steps_page_backwards()));
        method!(methods, ty, "update_step_velocity", "(channel, step, note, value)", "Set the velocity (0..1).", |lua, this, (ch, s, n, v): (i32, i32, i32, f64)| this.write(
            lua,
            |c| c.update_step_velocity(&position(ch, s, n), v)
        ));
        method!(methods, ty, "update_step_mute_state", "(channel, step, note, muted)", "Mute or unmute the note.", |lua, this, (ch, s, n, v): (i32, i32, i32, bool)| this.write(
            lua,
            |c| c.update_step_mute_state(&position(ch, s, n), v)
        ));
        method!(methods, ty, "update_step_chance", "(channel, step, note, value)", "Set the chance (0..1).", |lua, this, (ch, s, n, v): (i32, i32, i32, f64)| this.write(
            lua,
            |c| c.update_step_chance(&position(ch, s, n), v)
        ));
        method!(methods, ty, "update_step_repeat_count", "(channel, step, note, count)", "Set the repeat count.", |lua, this, (ch, s, n, v): (i32, i32, i32, i32)| this.write(
            lua,
            |c| c.update_step_repeat_count(&position(ch, s, n), v)
        ));
        method!(
            methods,
            ty,
            "update_step_velocity_spread",
            "(channel, step, note, value)",
            "Set the velocity spread (0..1).",
            |lua, this, (ch, s, n, v): (i32, i32, i32, f64)| this.write(lua, |c| c.update_step_velocity_spread(&position(ch, s, n), v))
        );
        method!(methods, ty, "update_step_pan", "(channel, step, note, value)", "Set the panorama (-1..1).", |lua, this, (ch, s, n, v): (i32, i32, i32, f64)| this.write(
            lua,
            |c| c.update_step_pan(&position(ch, s, n), v)
        ));
        method!(methods, ty, "change_step_chance", "(channel, step, note, control)", "Change the chance with a relative value.", |lua, this, (ch, s, n, v): (i32, i32, i32, i32)| this
            .write(lua, |c| c.change_step_chance(&position(ch, s, n), v)));
        method!(
            methods,
            ty,
            "change_step_repeat_count",
            "(channel, step, note, control)",
            "Change the repeat count with a relative value.",
            |lua, this, (ch, s, n, v): (i32, i32, i32, i32)| this.write(lua, |c| c.change_step_repeat_count(&position(ch, s, n), v))
        );
        method!(
            methods,
            ty,
            "change_step_velocity_spread",
            "(channel, step, note, control)",
            "Change the velocity spread with a relative value.",
            |lua, this, (ch, s, n, v): (i32, i32, i32, i32)| this.write(lua, |c| c.change_step_velocity_spread(&position(ch, s, n), v))
        );
        method!(methods, ty, "change_step_pan", "(channel, step, note, control)", "Change the panorama with a relative value.", |lua, this, (ch, s, n, v): (i32, i32, i32, i32)| this
            .write(lua, |c| c.change_step_pan(&position(ch, s, n), v)));
    }
}

// Transport, application, scales, value changer

pub struct TransportUd;

impl UserData for TransportUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Transport";
        method!(methods, ty, "is_playing", "() -> bool", "True while playing.", |lua, _, ()| read(lua, |ctx| ctx.model.transport().is_playing()));
        method!(methods, ty, "is_recording", "() -> bool", "True while recording.", |lua, _, ()| read(lua, |ctx| ctx.model.transport().is_recording()));
        method!(methods, ty, "is_loop", "() -> bool", "True if the loop is on.", |lua, _, ()| read(lua, |ctx| ctx.model.transport().is_loop()));
        method!(methods, ty, "is_metronome_on", "() -> bool", "True if the metronome is on.", |lua, _, ()| read(lua, |ctx| ctx.model.transport().is_metronome_on()));
        method!(methods, ty, "is_writing_arranger_automation", "() -> bool", "True if arranger automation is written.", |lua, _, ()| read(lua, |ctx| ctx
            .model
            .transport()
            .is_writing_arranger_automation()));
        method!(methods, ty, "is_writing_clip_launcher_automation", "() -> bool", "True if clip launcher automation is written.", |lua, _, ()| read(lua, |ctx| ctx
            .model
            .transport()
            .is_writing_clip_launcher_automation()));
        method!(methods, ty, "start_recording", "()", "Start recording.", |lua, _, ()| write(lua, |ctx| ctx.model.transport_mut().start_recording()));
        method!(methods, ty, "toggle_loop", "()", "Toggle the loop.", |lua, _, ()| write(lua, |ctx| ctx.model.transport_mut().toggle_loop()));
        method!(methods, ty, "toggle_metronome", "()", "Toggle the metronome.", |lua, _, ()| write(lua, |ctx| ctx.model.transport_mut().toggle_metronome()));
        method!(methods, ty, "toggle_write_arranger_automation", "()", "Toggle writing arranger automation.", |lua, _, ()| write(lua, |ctx| ctx
            .model
            .transport_mut()
            .toggle_write_arranger_automation()));
        method!(methods, ty, "toggle_write_clip_launcher_automation", "()", "Toggle writing clip launcher automation.", |lua, _, ()| write(lua, |ctx| ctx
            .model
            .transport_mut()
            .toggle_write_clip_launcher_automation()));
    }
}

pub struct ApplicationUd;

impl UserData for ApplicationUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Application";
        method!(methods, ty, "undo", "()", "Undo.", |lua, _, ()| write(lua, |ctx| ctx.model.application_mut().undo()));
        method!(methods, ty, "redo", "()", "Redo.", |lua, _, ()| write(lua, |ctx| ctx.model.application_mut().redo()));
        method!(methods, ty, "can_undo", "() -> bool", "True if there is something to undo.", |lua, _, ()| read(lua, |ctx| ctx.model.application().can_undo()));
        method!(methods, ty, "can_redo", "() -> bool", "True if there is something to redo.", |lua, _, ()| read(lua, |ctx| ctx.model.application().can_redo()));
    }
}

pub struct ScalesUd;

impl UserData for ScalesUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Scales";
        method!(methods, ty, "drum_offset", "() -> integer", "The first note of the drum pads.", |lua, _, ()| read(lua, |ctx| ctx.model.scales().drum_offset()));
        method!(methods, ty, "set_drum_default_offset", "(offset)", "Set the default drum offset.", |lua, _, offset: i32| write(lua, |ctx| ctx
            .model
            .scales_mut()
            .set_drum_default_offset(offset)));
        method!(methods, ty, "inc_drum_octave", "()", "One drum octave up.", |lua, _, ()| write(lua, |ctx| ctx.model.scales_mut().inc_drum_octave()));
        method!(methods, ty, "dec_drum_octave", "()", "One drum octave down.", |lua, _, ()| write(lua, |ctx| ctx.model.scales_mut().dec_drum_octave()));
        method!(methods, ty, "can_scroll_drum_octave_up", "() -> bool", "True if the drum octave can go up.", |lua, _, ()| read(lua, |ctx| ctx
            .model
            .scales()
            .can_scroll_drum_octave_up()));
        method!(methods, ty, "can_scroll_drum_octave_down", "() -> bool", "True if the drum octave can go down.", |lua, _, ()| read(lua, |ctx| ctx
            .model
            .scales()
            .can_scroll_drum_octave_down()));
        method!(methods, ty, "drum_range_text", "() -> string", "The range of the drum pads as text.", |lua, _, ()| read(lua, |ctx| ctx.model.scales().drum_range_text()));
        method!(methods, ty, "next_scale", "()", "Select the next scale.", |lua, _, ()| write(lua, |ctx| ctx.model.scales_mut().next_scale()));
        method!(methods, ty, "prev_scale", "()", "Select the previous scale.", |lua, _, ()| write(lua, |ctx| ctx.model.scales_mut().prev_scale()));
        method!(methods, ty, "has_next_scale", "() -> bool", "True if there is a next scale.", |lua, _, ()| read(lua, |ctx| ctx.model.scales().has_next_scale()));
        method!(methods, ty, "has_prev_scale", "() -> bool", "True if there is a previous scale.", |lua, _, ()| read(lua, |ctx| ctx.model.scales().has_prev_scale()));
        method!(methods, ty, "scale_name", "() -> string", "The name of the scale.", |lua, _, ()| read(lua, |ctx| ctx.model.scales().scale().name().to_string()));
        method!(methods, ty, "scale_offset", "() -> integer", "The offset of the scale base (0-11).", |lua, _, ()| read(lua, |ctx| ctx.model.scales().scale_offset()));
        method!(methods, ty, "is_chromatic", "() -> bool", "True if chromatic.", |lua, _, ()| read(lua, |ctx| ctx.model.scales().is_chromatic()));
        method!(methods, ty, "set_chromatic", "(chromatic)", "Set chromatic.", |lua, _, chromatic: bool| write(lua, |ctx| ctx.model.scales_mut().set_chromatic(chromatic)));
        method!(methods, ty, "set_scale_by_name", "(name)", "Select a scale by name.", |lua, _, name: String| write(lua, |ctx| ctx.model.scales_mut().set_scale_by_name(&name)));
        method!(methods, ty, "set_scale_offset_by_name", "(name)", "Select the scale base by name.", |lua, _, name: String| write(lua, |ctx| ctx
            .model
            .scales_mut()
            .set_scale_offset_by_name(&name)));
        method!(methods, ty, "is_in_scale", "(note_in_octave) -> bool", "True if the note (0-11) is in the scale.", |lua, _, note: i32| read(lua, |ctx| ctx
            .model
            .scales()
            .is_in_scale(note)));
        method!(methods, ty, "to_note_in_octave", "(note) -> integer", "The note in the octave (0-11) relative to the scale base.", |lua, _, note: i32| read(lua, |ctx| ctx
            .model
            .scales()
            .to_note_in_octave(note)));
        method!(methods, ty, "nearest_note_in_scale", "(note) -> integer", "The nearest note of the scale.", |lua, _, note: i32| read(lua, |ctx| ctx
            .model
            .scales()
            .nearest_note_in_scale(note)));
    }
}

pub struct ValueChangerUd;

impl UserData for ValueChangerUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "ValueChanger";
        method!(methods, ty, "to_normalized", "(value) -> number", "A value in the range of the value changer as 0..1.", |lua, _, value: f64| read(lua, |ctx| ctx
            .model
            .value_changer()
            .to_normalized_value(value)));
        method!(methods, ty, "from_normalized", "(value) -> integer", "A value 0..1 in the range of the value changer.", |lua, _, value: f64| read(lua, |ctx| ctx
            .model
            .value_changer()
            .from_normalized_value(value)));
        method!(methods, ty, "is_increase", "(control) -> bool", "True if the relative control value increases.", |lua, _, control: i32| read(lua, |ctx| ctx
            .model
            .value_changer()
            .is_increase(control)));
        method!(methods, ty, "upper_bound", "() -> integer", "The upper bound of the values.", |lua, _, ()| read(lua, |ctx| ctx.model.value_changer().upper_bound()));
    }
}

// ------------------------------------------------------------------------------------------
// The surface, the host and the configuration

pub struct SurfaceUd;

/// The `midi = {type, channel, control}` option.
fn midi_option(t: &Table) -> mlua::Result<Option<(crate::midi::BindType, i32, i32)>> {
    let Some(midi) = t.get::<Option<Table>>("midi")? else {
        return Ok(None);
    };
    Ok(Some((names::bind_type(&midi.get::<String>(1)?).map_err(se)?, midi.get(2)?, midi.get(3)?)))
}

impl UserData for SurfaceUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Surface";
        method!(
            methods,
            ty,
            "button",
            "(id, {label, command, light, sender, midi})",
            "Create a button: `command` is a function (ctx, event, velocity) or a built-in command; `light` a function (ctx) -> state; `sender` a function (ctx, state) which sends it; `midi = {type, channel, control}` binds it (then the default sender sends the state as the value of the control, and without `light` the state is 1 while pressed).",
            |lua, _, (id, options): (String, Table)| {
                let id = button(&id)?;
                let label = options.get::<Option<String>>("label")?.unwrap_or_else(|| id.name().to_string());
                let command = trigger_command(lua, options.get("command")?, format!("command of button {}", id.name()))?;
                let weak = engine(lua)?;
                let light: Option<Function> = options.get("light")?;
                let sender: Option<Function> = options.get("sender")?;
                let midi = midi_option(&options)?;
                if light.is_some() && sender.is_none() && midi.is_none() {
                    return Err(rt("a button with a light needs a sender or a MIDI binding"));
                }
                write(lua, |ctx| {
                    let button = ctx.surface.create_button(id, &label);
                    if let Some(command) = command {
                        button.action = ButtonAction::Command(command);
                    }
                    if let Some((bind_type, channel, control)) = midi {
                        ctx.surface.bind_button(id, bind_type, channel, control, -1);
                    }
                    if light.is_none() && midi.is_none() {
                        return;
                    }
                    let supplier: LightSupplier = match light {
                        Some(f) => light_supplier(weak.clone(), f, format!("light of button {}", id.name())),
                        None => Rc::new(move |ctx: &Ctx| if ctx.surface.is_pressed(id) { 1 } else { 0 }),
                    };
                    let sender: LightSender = match (sender, midi) {
                        (Some(f), _) => light_sender(weak, f, format!("sender of button {}", id.name())),
                        (None, Some((bind_type, channel, control))) => Rc::new(move |ctx: &mut Ctx, state| ctx.surface.set_trigger(bind_type, channel, control, state)),
                        (None, None) => return,
                    };
                    ctx.surface.create_light(None, Some(id), supplier, sender);
                })
            }
        );
        method!(
            methods,
            ty,
            "light",
            "({supplier, sender, button})",
            "Create a light: `supplier` (ctx) -> state, `sender` (ctx, state); optionally the button it belongs to.",
            |lua, _, options: Table| {
                let weak = engine(lua)?;
                let supplier = light_supplier(weak.clone(), options.get("supplier")?, "light".to_string());
                let sender = light_sender(weak, options.get("sender")?, "light sender".to_string());
                let id = match options.get::<Option<String>>("button")? {
                    Some(name) => Some(button(&name)?),
                    None => None,
                };
                write(lua, |ctx| {
                    ctx.surface.create_light(None, id, supplier, sender);
                })
            }
        );
        method!(
            methods,
            ty,
            "continuous",
            "(id, {label, kind, command, midi})",
            "Create a knob or fader: `kind` is `fader`, `absolute_knob` or `relative_knob`; `command` a function (ctx, value) or a built-in command; `midi = {type, channel, control}` binds it.",
            |lua, _, (id, options): (String, Table)| {
                let id = continuous(&id)?;
                let label = options.get::<Option<String>>("label")?.unwrap_or_else(|| id.name().to_string());
                let kind = names::continuous_kind(&options.get::<Option<String>>("kind")?.unwrap_or_else(|| "absolute_knob".to_string())).map_err(se)?;
                let command: Option<Rc<RefCell<dyn crate::framework::command::ContinuousCommand>>> = match options.get::<Value>("command")? {
                    Value::Nil => None,
                    Value::Function(function) => Some(Rc::new(RefCell::new(LuaContinuous { engine: engine(lua)?, function, what: format!("command of {}", id.name()) }))),
                    Value::UserData(ud) => Some(ud.borrow::<CommandUd>()?.0.continuous().ok_or_else(|| rt("not a continuous command"))?),
                    _ => return Err(rt("a command is a function or a built-in command")),
                };
                let midi = midi_option(&options)?;
                write(lua, |ctx| {
                    let control = ctx.surface.create_continuous(id, &label, kind);
                    control.command = command;
                    if let Some((bind_type, channel, number)) = midi {
                        ctx.surface.bind_continuous(id, bind_type, channel, number);
                    }
                })
            }
        );
        method!(methods, ty, "bind_button", "(id, type, channel, control)", "Bind a button to a MIDI message.", |lua, _, (id, bind_type, channel, control): (String, String, i32, i32)| {
            let id = button(&id)?;
            let bind_type = names::bind_type(&bind_type).map_err(se)?;
            write(lua, |ctx| ctx.surface.bind_button(id, bind_type, channel, control, -1))
        });
        method!(
            methods,
            ty,
            "bind_continuous",
            "(id, type, channel, control)",
            "Bind a knob or fader to a MIDI message.",
            |lua, _, (id, bind_type, channel, control): (String, String, i32, i32)| {
                let id = continuous(&id)?;
                let bind_type = names::bind_type(&bind_type).map_err(se)?;
                write(lua, |ctx| ctx.surface.bind_continuous(id, bind_type, channel, control))
            }
        );
        method!(methods, ty, "clear_cache", "()", "Forget the sent light states (all are sent again).", |lua, _, ()| write(lua, |ctx| ctx.surface.clear_cache()));
        method!(methods, ty, "is_pressed", "(id) -> bool", "True if the button is pressed.", |lua, _, id: String| {
            let id = button(&id)?;
            read(lua, |ctx| ctx.surface.is_pressed(id))
        });
        method!(methods, ty, "is_shift_pressed", "() -> bool", "True if Shift is pressed.", |lua, _, ()| read(lua, |ctx| ctx.surface.is_shift_pressed()));
        method!(methods, ty, "send_sysex", "(bytes)", "Send a system exclusive message (a list of bytes or a hex string).", |lua, _, data: Value| {
            let bytes: Vec<u8> = match data {
                Value::String(s) => crate::midi::from_hex_str(&s.to_str()?),
                Value::Table(t) => t.sequence_values::<u8>().collect::<mlua::Result<_>>()?,
                _ => return Err(rt("sysex data is a list of bytes or a hex string")),
            };
            read(lua, |ctx| ctx.surface.output.send_sysex(&bytes))
        });
        method!(methods, ty, "send_note", "(channel, note, velocity)", "Send a note.", |lua, _, (channel, note, velocity): (i32, i32, i32)| read(lua, |ctx| ctx
            .surface
            .output
            .send_note_ex(channel, note, velocity)));
        method!(methods, ty, "send_cc", "(channel, cc, value)", "Send a control change.", |lua, _, (channel, cc, value): (i32, i32, i32)| read(lua, |ctx| ctx
            .surface
            .output
            .send_cc_ex(channel, cc, value)));
    }
}

pub struct HostUd;

impl UserData for HostUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Host";
        method!(methods, ty, "println", "(text)", "Log a message.", |lua, _, text: String| read(lua, |ctx| ctx.host.println(&text)));
        method!(methods, ty, "error", "(text)", "Log an error.", |lua, _, text: String| read(lua, |ctx| ctx.host.error(&text)));
        method!(methods, ty, "show_notification", "(text)", "Show a notification in the DAW.", |lua, _, text: String| read(lua, |ctx| ctx.host.show_notification(&text)));
    }
}

pub struct ConfigUd;

impl UserData for ConfigUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Config";
        method!(methods, ty, "midi_edit_channel", "() -> integer", "The MIDI channel for editing notes (0-15).", |lua, _, ()| read(lua, |ctx| ctx.config.midi_edit_channel()));
        method!(methods, ty, "is_turn_off_empty_drum_pads", "() -> bool", "Turn off empty drum pads.", |lua, _, ()| read(lua, |ctx| ctx.config.is_turn_off_empty_drum_pads()));
        method!(methods, ty, "are_deactivated_items_excluded", "() -> bool", "Exclude the deactivated items.", |lua, _, ()| read(lua, |ctx| ctx.config.are_deactivated_items_excluded()));
        method!(methods, ty, "are_master_tracks_included", "() -> bool", "Include the master tracks.", |lua, _, ()| read(lua, |ctx| ctx.config.are_master_tracks_included()));
        method!(methods, ty, "scale", "() -> string", "The scale setting.", |lua, _, ()| read(lua, |ctx| ctx.config.scale().to_string()));
        method!(methods, ty, "scale_base", "() -> string", "The scale base setting.", |lua, _, ()| read(lua, |ctx| ctx.config.scale_base().to_string()));
        method!(methods, ty, "is_scale_in_key", "() -> bool", "The in key setting.", |lua, _, ()| read(lua, |ctx| ctx.config.is_scale_in_key()));
        method!(methods, ty, "set_scale", "(name)", "Store the scale setting.", |lua, _, name: String| write(lua, |ctx| ctx.config.set_scale(&name)));
        method!(
            methods,
            ty,
            "take_changed_settings",
            "() -> {string}",
            "The settings which changed since the last call, by the names of their constants (`SCALES_SCALE`, …; controller-specific settings as numbers).",
            |lua, _, ()| {
                let ids = write(lua, |ctx| ctx.config.take_changed_settings())?;
                let list = lua.create_table()?;
                for id in ids {
                    match names::setting_name(id) {
                        Some(name) => list.raw_push(name)?,
                        None => list.raw_push(id.0 as i64)?,
                    }
                }
                Ok(list)
            }
        );
    }
}

// ------------------------------------------------------------------------------------------
// Built-in modes, views, commands, providers, parameters

/// A built-in mode (the base of a mode table, or registered directly).
pub struct ModeUd(pub Rc<RefCell<Box<dyn Mode>>>);

/// A mode without anything (for the API reference).
struct ControlOnlyMode;

impl Mode for ControlOnlyMode {
    fn name(&self) -> &str {
        "None"
    }

    crate::impl_as_any!();
}

impl ModeUd {
    fn with_mut<R>(&self, lua: &Lua, ctx: &CtxUd, f: impl FnOnce(&mut dyn Mode, &mut Ctx) -> R) -> mlua::Result<R> {
        ctx.check(lua)?;
        let mut mode = self.0.try_borrow_mut().map_err(|_| rt("the mode is running (re-entrant call)"))?;
        write(lua, |c| f(mode.as_mut(), c))
    }

    fn with<R>(&self, lua: &Lua, ctx: &CtxUd, f: impl FnOnce(&dyn Mode, &Ctx) -> R) -> mlua::Result<R> {
        ctx.check(lua)?;
        let mode = self.0.try_borrow().map_err(|_| rt("the mode is running (re-entrant call)"))?;
        read(lua, |c| f(mode.as_ref(), c))
    }

    fn parameter_mode<R>(&self, lua: &Lua, ctx: &CtxUd, f: impl FnOnce(&mut builtin::ParameterMode, &mut Ctx) -> R) -> mlua::Result<R> {
        self.with_mut(lua, ctx, |mode, c| mode.as_any_mut().downcast_mut::<builtin::ParameterMode>().map(|m| f(m, c)))?.ok_or_else(|| rt("not a Parameter mode"))
    }

    fn sequencer_mode<R>(&self, f: impl FnOnce(&mut builtin::SequencerMode) -> R) -> mlua::Result<R> {
        let mut mode = self.0.try_borrow_mut().map_err(|_| rt("the mode is running (re-entrant call)"))?;
        mode.as_any_mut().downcast_mut::<builtin::SequencerMode>().map(f).ok_or_else(|| rt("not a Sequencer mode"))
    }
}

type Ctx_ = UserDataRef<CtxUd>;

impl UserData for ModeUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Mode";
        method!(methods, ty, "name", "() -> string", "The name.", |_, this, ()| Ok(this.0.try_borrow().map(|m| m.name().to_string()).unwrap_or_default()));
        method!(methods, ty, "on_activate", "(ctx)", "Activate.", |lua, this, ctx: Ctx_| this.with_mut(lua, &ctx, |m, c| m.on_activate(c)));
        method!(methods, ty, "on_deactivate", "(ctx)", "Deactivate.", |lua, this, ctx: Ctx_| this.with_mut(lua, &ctx, |m, c| m.on_deactivate(c)));
        method!(methods, ty, "knob_value", "(ctx, index) -> integer", "The value of a knob.", |lua, this, (ctx, i): (Ctx_, i64)| {
            let i = index(i)?;
            this.with(lua, &ctx, |m, c| m.knob_value(c, i))
        });
        method!(methods, ty, "on_knob_value", "(ctx, index, value)", "A knob was moved.", |lua, this, (ctx, i, v): (Ctx_, i64, i32)| {
            let i = index(i)?;
            this.with_mut(lua, &ctx, |m, c| m.on_knob_value(c, i, v))
        });
        method!(methods, ty, "button_color", "(ctx, button) -> integer", "The color of a button.", |lua, this, (ctx, b): (Ctx_, String)| {
            let b = button(&b)?;
            this.with(lua, &ctx, |m, c| m.button_color(c, b))
        });
        method!(methods, ty, "on_button", "(ctx, row, index, event)", "A button of a row.", |lua, this, (ctx, row, i, event): (Ctx_, i64, i64, String)| {
            let (row, i) = (index(row)?, index(i)?);
            let event = names::button_event(&event).map_err(se)?;
            this.with_mut(lua, &ctx, |m, c| m.on_button(c, row, i, event))
        });
        method!(methods, ty, "select_item", "(ctx, index)", "Select an item of the bank.", |lua, this, (ctx, i): (Ctx_, i64)| {
            let i = index(i)?;
            this.with_mut(lua, &ctx, |m, c| m.select_item(c, i))
        });
        method!(methods, ty, "select_previous_item", "(ctx)", "Select the previous item.", |lua, this, ctx: Ctx_| this.with_mut(lua, &ctx, |m, c| m.select_previous_item(c)));
        method!(methods, ty, "select_next_item", "(ctx)", "Select the next item.", |lua, this, ctx: Ctx_| this.with_mut(lua, &ctx, |m, c| m.select_next_item(c)));
        method!(methods, ty, "select_previous_item_page", "(ctx)", "Select the previous page.", |lua, this, ctx: Ctx_| this.with_mut(lua, &ctx, |m, c| m
            .select_previous_item_page(c)));
        method!(methods, ty, "select_next_item_page", "(ctx)", "Select the next page.", |lua, this, ctx: Ctx_| this.with_mut(lua, &ctx, |m, c| m.select_next_item_page(c)));
        method!(methods, ty, "has_previous_item", "(ctx) -> bool", "True if there is a previous item.", |lua, this, ctx: Ctx_| this.with(lua, &ctx, |m, c| m.has_previous_item(c)));
        method!(methods, ty, "has_next_item", "(ctx) -> bool", "True if there is a next item.", |lua, this, ctx: Ctx_| this.with(lua, &ctx, |m, c| m.has_next_item(c)));
        method!(methods, ty, "has_previous_item_page", "(ctx) -> bool", "True if there is a previous page.", |lua, this, ctx: Ctx_| this.with(lua, &ctx, |m, c| m
            .has_previous_item_page(c)));
        method!(methods, ty, "has_next_item_page", "(ctx) -> bool", "True if there is a next page.", |lua, this, ctx: Ctx_| this.with(lua, &ctx, |m, c| m.has_next_item_page(c)));
        method!(
            methods,
            ty,
            "set_parameter_provider",
            "(ctx, provider)",
            "`Parameter`: set the provider of the controls (former `setParameterProvider`).",
            |lua, this, (ctx, p): (Ctx_, UserDataRef<ProviderUd>)| {
                let p = p.0.clone();
                this.parameter_mode(lua, &ctx, |m, c| m.base.set_parameter_provider(c, p))
            }
        );
        method!(
            methods,
            ty,
            "set_button_parameter_provider",
            "(ctx, button, provider)",
            "`Parameter`: the provider while the button is pressed.",
            |lua, this, (ctx, b, p): (Ctx_, String, UserDataRef<ProviderUd>)| {
                let b = button(&b)?;
                let p = p.0.clone();
                this.parameter_mode(lua, &ctx, |m, c| m.base.set_button_parameter_provider(c, b, p))
            }
        );
        method!(methods, ty, "bind_controls", "(ctx)", "`Parameter`: bind the controls to the parameters of the provider.", |lua, this, ctx: Ctx_| this.parameter_mode(
            lua,
            &ctx,
            |m, c| m.base.bind_controls(c)
        ));
        method!(methods, ty, "selected_resolution_index", "() -> integer", "`Sequencer`: the index of the step resolution.", |_, this, ()| this.sequencer_mode(|m| m
            .base
            .selected_resolution_index()));
        method!(methods, ty, "set_selected_resolution_index", "(index)", "`Sequencer`: set the index of the step resolution.", |_, this, i: i64| {
            let i = index(i)?;
            this.sequencer_mode(|m| m.base.set_selected_resolution_index(i))
        });
    }
}

/// A built-in view.
pub struct ViewUd(pub Rc<RefCell<Box<dyn View>>>);

impl UserData for ViewUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "View";
        method!(methods, ty, "name", "() -> string", "The name.", |_, this, ()| Ok(this.0.try_borrow().map(|v| v.name().to_string()).unwrap_or_default()));
    }
}

/// A built-in command.
pub struct CommandUd(pub Command);

impl UserData for CommandUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Command";
        method!(methods, ty, "execute", "(ctx, event, velocity)", "Execute (with Shift the shifted function).", |lua, this, (ctx, event, velocity): (Ctx_, String, Option<i32>)| {
            ctx.check(lua)?;
            let event = names::button_event(&event).map_err(se)?;
            write(lua, |c| this.0.with_trigger(|t| t.execute(c, event, velocity.unwrap_or(127))))?.map_err(se)
        });
        method!(methods, ty, "execute_normal", "(ctx, event)", "Execute the normal function.", |lua, this, (ctx, event): (Ctx_, String)| {
            ctx.check(lua)?;
            let event = names::button_event(&event).map_err(se)?;
            write(lua, |c| this.0.with_trigger(|t| t.execute_normal(c, event)))?.map_err(se)
        });
        method!(methods, ty, "execute_value", "(ctx, value)", "Execute a continuous command.", |lua, this, (ctx, value): (Ctx_, i32)| {
            ctx.check(lua)?;
            let command = this.0.continuous().ok_or_else(|| rt("not a continuous command"))?;
            let mut command = command.try_borrow_mut().map_err(|_| rt("the command is running (re-entrant call)"))?;
            write(lua, |c| command.execute(c, value))
        });
        method!(methods, ty, "can_scroll", "() -> bool", "`ModeCursor`/`Cursor`: true if it can scroll (for its light).", |lua, this, ()| match &this.0 {
            Command::Cursor(c) => {
                let command = c.try_borrow().map_err(|_| rt("the command is running (re-entrant call)"))?;
                read(lua, |ctx| command.can_scroll(ctx))
            }
            _ => Err(rt("not a cursor command")),
        });
        method!(
            methods,
            ty,
            "create_clip",
            "(ctx, enable_overdub)",
            "`New`: create a new clip on the cursor track (former `NewCommand.handleExecute`).",
            |lua, this, (ctx, overdub): (Ctx_, bool)| {
                ctx.check(lua)?;
                match &this.0 {
                    Command::New(c) => {
                        let command = c.try_borrow().map_err(|_| rt("the command is running (re-entrant call)"))?;
                        write(lua, |ctx| command.handle_execute(ctx, overdub))
                    }
                    _ => Err(rt("not a New command")),
                }
            }
        );
    }
}

/// A parameter provider.
pub struct ProviderUd(pub SharedProvider);

impl UserData for ProviderUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Provider";
        method!(methods, ty, "size", "() -> integer", "The number of parameters.", |_, this, ()| Ok(this.0.size()));
    }
}

/// A parameter.
pub struct ParameterUd(pub ParamRef);

impl UserData for ParameterUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let ty = "Parameter";
        method!(methods, ty, "value", "() -> integer", "The value.", |lua, this, ()| read(lua, |ctx| this.0.value(ctx)));
        method!(methods, ty, "exists", "() -> bool", "True if the parameter exists.", |lua, this, ()| read(lua, |ctx| this.0.does_exist(ctx)));
    }
}

// ------------------------------------------------------------------------------------------
// The setup tables of the script

/// `value_changer = {"twos_complement", upper_bound, step}` (default 128, 1).
pub fn value_changer(value: Value) -> mlua::Result<ValueChanger> {
    let Value::Table(t) = value else {
        return Ok(ValueChanger::twos_complement(128, 1));
    };
    let kind = t.get::<Option<String>>(1)?.unwrap_or_else(|| "twos_complement".to_string());
    let upper = t.get::<Option<i32>>(2)?.unwrap_or(128);
    let step = t.get::<Option<i32>>(3)?.unwrap_or(1);
    match kind.as_str() {
        "twos_complement" => Ok(ValueChanger::twos_complement(upper, step)),
        _ => Err(rt(format!("unknown value changer '{kind}'"))),
    }
}

fn rgb(value: Value) -> mlua::Result<(f64, f64, f64)> {
    match value {
        Value::Table(t) => {
            let color = ColorEx::from_rgb(t.get(1)?, t.get(2)?, t.get(3)?);
            Ok((color.red(), color.green(), color.blue()))
        }
        Value::String(s) => {
            let name = s.to_str()?.to_string();
            let color = DawColor::ALL.iter().find(|c| format!("{c:?}") == name).ok_or_else(|| rt(format!("unknown DAW color '{name}'")))?.color();
            Ok((color.red(), color.green(), color.blue()))
        }
        _ => Err(rt("a color is {r, g, b} or the name of a DAW color")),
    }
}

/// `colors = {indices = {[id] = index}, rgb = {[index] = {r, g, b}}, fill = {r, g, b}}`.
pub fn color_manager(value: Value) -> mlua::Result<ColorManager> {
    let mut colors = ColorManager::new();
    let Value::Table(t) = value else {
        return Ok(colors);
    };
    if let Some(fill) = t.get::<Option<Value>>("fill")? {
        let fill = rgb(fill)?;
        for i in 0..128 {
            colors.register_color(i, fill);
        }
    }
    if let Some(indices) = t.get::<Option<Table>>("indices")? {
        for pair in indices.pairs::<String, i32>() {
            let (key, index) = pair?;
            if colors.has_color_index(&key) {
                return Err(rt(format!("the color {key} is registered twice")));
            }
            colors.register_color_index(&key, index);
        }
    }
    if let Some(list) = t.get::<Option<Table>>("rgb")? {
        for pair in list.pairs::<i32, Value>() {
            let (index, color) = pair?;
            if !(0..=127).contains(&index) {
                return Err(rt(format!("color index {index} out of range 0..127")));
            }
            colors.register_color(index, rgb(color)?);
        }
    }
    Ok(colors)
}

/// `scales = {start_note, end_note, cols, rows}` (default 36, 100, 8, 8).
pub fn scales(value_changer: &ValueChanger, value: Value) -> mlua::Result<Scales> {
    let (start, end, cols, rows) = match value {
        Value::Table(t) => (t.get::<Option<i32>>(1)?.unwrap_or(36), t.get::<Option<i32>>(2)?.unwrap_or(100), t.get::<Option<i64>>(3)?.unwrap_or(8), t.get::<Option<i64>>(4)?.unwrap_or(8)),
        _ => (36, 100, 8, 8),
    };
    Ok(Scales::new(value_changer, start, end, index(cols)? as i32, index(rows)? as i32))
}

/// The model setup from `model` (a table or a function of the configuration values).
pub fn model_setup(lua: &Lua, value: Value, config: &Configuration) -> mlua::Result<ModelSetup> {
    let mut setup = ModelSetup::new();
    let t = match value {
        Value::Nil => return Ok(setup),
        Value::Table(t) => t,
        Value::Function(f) => {
            let values = lua.create_table()?;
            values.set("master_tracks_included", config.are_master_tracks_included())?;
            f.call::<Table>(values)?
        }
        _ => return Err(rt("model is a table or a function")),
    };
    for pair in t.pairs::<String, Value>() {
        let (key, value) = pair?;
        let int = || -> mlua::Result<i64> { lua.unpack::<i64>(value.clone()) };
        let flag = || -> mlua::Result<bool> { lua.unpack::<bool>(value.clone()) };
        match key.as_str() {
            "flat_track_list" => setup.set_has_flat_track_list(flag()?),
            "full_flat_track_list" => setup.set_has_full_flat_track_list(flag()?),
            "num_tracks" => setup.set_num_tracks(index(int()?)?),
            "num_scenes" => setup.set_num_scenes(index(int()?)?),
            "num_sends" => setup.set_num_sends(index(int()?)?),
            "num_params" => setup.set_num_params(int()? as i32),
            "num_devices_in_bank" => setup.set_num_devices_in_bank(int()? as i32),
            "num_drum_pad_layers" => setup.set_num_drum_pad_layers(int()? as i32),
            "devices" => {
                let list: Table = lua.unpack(value.clone())?;
                for name in list.sequence_values::<String>() {
                    setup.enable_device(names::device(&name?).map_err(se)?);
                }
            }
            "drum_device" => setup.enable_main_drum_device(flag()?),
            // Applied after the model is created
            "track_bank_indication" => {}
            _ => return Err(rt(format!("unknown model option '{key}'"))),
        }
    }
    Ok(setup)
}
