// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Electra One specific parts of the control surface (former
//! `AbstractElectraOneControlSurface`, `ElectraOneControlSurface` and
//! `ElectraOneMiniControlSurface`) and the controller state, which is stored in `ctx.ext`
//! ([`ElectraState`]).
//!
//! The device has two port pairs: Port 1 carries the controls (CCs on MIDI channel 16, the
//! surface's input and output), CTRL the SysEx API (the further MIDI input and output of the
//! instance). The conversation on CTRL:
//!
//! 1. `startup` asks for the device information (`02 7F`).
//! 2. The reply (`01 7F`, JSON) is checked for the firmware version and the model, then the
//!    controller subscribes to the page and pot events (`14 79`) and asks for the preset list
//!    (`02 04`).
//! 3. The preset list (`01 04`, JSON) has to contain the DrivenByMoss preset, which is selected
//!    (`09 08 bank slot`). The other presets are remembered by their names.
//! 4. The device reports the selected preset (`7E 02 bank slot`): the controller goes online with
//!    the DrivenByMoss preset, otherwise offline (Dummy mode, no MIDI to the pads).
//! 5. Online, a page switch on the device (`7E 06 page`) selects the mode of the page and a knob
//!    touch (`7E 0A pot control touched`) touches the parameter of the control and checks the
//!    knob touch combinations.
//!
//! The device splits long SysEx messages into chunks (Bitwig hands them over in chunks of 1024
//! bytes), which are collected until F7.
//!
//! Differences to Java:
//! - A JSON reply which cannot be parsed, a wrong firmware or a missing DrivenByMoss preset is
//!   logged as an error; Java throws a `FrameworkException` from the MIDI callback, which Bitwig
//!   logs. The handshake stops there in both.
//! - Going offline while offline does not forget the mode of the DrivenByMoss preset (Java
//!   remembers the Dummy mode and then selects the page -1, see doc/UPSTREAM_CANDIDATES.md).
//! - A pot touch of the Mini with a knob number above 8 is ignored (Java throws an
//!   `ArrayIndexOutOfBoundsException`).

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use super::command::TouchCombinationCommand;
use super::colors::format_color;
use super::definition::*;
use super::link::{self, ElectraLink};
use crate::config::electra as config;
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;
use crate::framework::ids::Modes;
use crate::json::{JsonValue, JsonWriter};
use crate::midi::{ButtonEvent, MidiOutput};

/// The specific type of the normal model (former `ElectraOneType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectraOneType {
    Mk1,
    Mk2,
}

/// The controller specific state.
pub struct ElectraState {
    pub variant: ElectraVariant,
    /// The output of the CTRL port (former `ctrlOutput`).
    pub ctrl_output: MidiOutput,
    /// Is the DrivenByMoss preset selected (former `isOnline`)? Shared with the trigger sender.
    pub online: Rc<Cell<bool>>,
    /// The chunks of a SysEx message which is not complete yet (former `sysexChunks`).
    sysex_chunks: Vec<Vec<u8>>,
    /// The other presets by their names: bank and slot (former `presetBanks`, `presetIndices`).
    presets: HashMap<String, (i32, i32)>,
    /// The bank and slot of the DrivenByMoss preset, -1 if not found yet.
    pub bank_index: i32,
    pub preset_index: i32,
    /// The emulated Shift button (former `isShiftPressed`).
    pub is_shift_pressed: bool,
    /// The mode which was active when the device went offline (former `activeMode`).
    pub active_mode: Option<Modes>,
    /// The touch state of each knob (former `knobStates`).
    pub knob_states: Vec<i32>,
    /// The model of the normal variant (former `electraOneType`).
    pub electra_one_type: ElectraOneType,
    /// The commands of the knob touch combinations (former `touchCombinationCommands`).
    pub touch_commands: Vec<TouchCombinationCommand>,
    /// The indices of the touch patterns which have a function (former `commandPatterns`), in
    /// the order of the patterns.
    pub command_patterns: Vec<usize>,
    /// The patterns assigned to Shift, with 2 for "don't care" (former `shiftPatterns`). As in
    /// Java they are collected but not used (see doc/UPSTREAM_CANDIDATES.md).
    pub shift_patterns: Vec<Vec<i32>>,
    /// The provider of the surface link (not part of Java, see `link`).
    pub link: ElectraLink,
}

impl ElectraState {
    pub fn new(variant: ElectraVariant, ctrl_output: MidiOutput, link: ElectraLink) -> Self {
        let touch_commands = (0..variant.touch_patterns().len()).map(|i| TouchCombinationCommand::new(variant, i)).collect();
        ElectraState {
            variant,
            ctrl_output,
            online: Rc::new(Cell::new(false)),
            sysex_chunks: Vec::new(),
            presets: HashMap::new(),
            bank_index: -1,
            preset_index: -1,
            is_shift_pressed: false,
            active_mode: None,
            knob_states: vec![0; variant.number_of_knobs()],
            electra_one_type: ElectraOneType::Mk1,
            touch_commands,
            command_patterns: Vec::new(),
            shift_patterns: Vec::new(),
            link,
        }
    }

    /// The bank and slot of another preset by its name.
    pub fn preset(&self, name: &str) -> Option<(i32, i32)> {
        self.presets.get(name).copied()
    }
}

pub fn state(ctx: &Ctx) -> &ElectraState {
    ctx.ext.downcast_ref::<ElectraState>().expect("Electra One state")
}

pub fn state_mut(ctx: &mut Ctx) -> &mut ElectraState {
    ctx.ext.downcast_mut::<ElectraState>().expect("Electra One state")
}

/// Former `isOnline`.
pub fn is_online(ctx: &Ctx) -> bool {
    state(ctx).online.get()
}

// ---------------------------------------------------------------------------------------------
// Sending

fn send_sysex(ctx: &Ctx, command: &[u8], content: &[u8]) {
    state(ctx).ctrl_output.send_sysex(&sysex(command, content));
}

/// Send a JSON or Lua text; the characters above 127 are replaced (former `sendText`).
fn send_text(ctx: &Ctx, command: &[u8], text: &str) {
    send_sysex(ctx, command, crate::config::values::fix_ascii(text).as_bytes());
}

/// Former `updateValue`: the value of a control, as a CC on MIDI channel 16 to Port 1.
pub fn update_value(ctx: &Ctx, midi_cc: i32, value: i32) {
    ctx.surface.output.send_cc_ex(ELECTRA_MIDI_CHANNEL, midi_cc, value);
}

/// Former `updateLabel`: the name, color and visibility of a control (`None` leaves it).
pub fn update_label(ctx: &Ctx, control_id: i32, name: Option<&str>, color: Option<ColorEx>, visible: Option<bool>) {
    let mut json = JsonWriter::new();
    json.begin_object();
    if let Some(name) = name {
        json.key("name").string(name);
    }
    if let Some(color) = color {
        json.key("color").string(&format_color(color));
    }
    if let Some(visible) = visible {
        json.key("visible").boolean(visible);
    }
    json.end_object();
    send_text(ctx, &control_command(control_id, SYSEX_RUNTIME_CONTROL_UPDATE), &json.finish());
}

/// Former `updateValueLabel`: the text of the (first) value of a control.
pub fn update_value_label(ctx: &Ctx, control_id: i32, label: &str) {
    let command = control_command(control_id, SYSEX_RUNTIME_VALUE_LABEL_UPDATE);
    send_text(ctx, &[command[0], command[1], command[2], command[3], 0], label);
}

/// Former `updateGroupLabel`: the label of a group, with the `sgt` function of the preset's Lua.
pub fn update_group_label(ctx: &Ctx, group_id: i32, label: &str) {
    send_text(ctx, &SYSEX_RUNTIME_EXECUTE_LUA, &set_group_title(group_id, label));
}

/// Former `setLoggingEnabled`: send the logging setting to the device.
pub fn set_logging_enabled(ctx: &Ctx) {
    let enabled = config::is_log_to_console_enabled(&ctx.config);
    send_sysex(ctx, &SYSEX_RUNTIME_ENABLE_LOGGER, &[enabled as u8, 0x00]);
}

/// Former `setRepaintEnabled`: pause or resume the repaint of the display.
pub fn set_repaint_enabled(ctx: &Ctx, enable: bool) {
    send_sysex(ctx, &SYSEX_RUNTIME_SET_REPAINT_ENABLED, &[enable as u8, 0x00]);
}

/// Former `requestDeviceInfo`.
pub fn request_device_info(ctx: &Ctx) {
    send_sysex(ctx, &SYSEX_INFO_DEVICE, &[]);
}

fn request_preset_list(ctx: &Ctx) {
    send_sysex(ctx, &SYSEX_INFO_PRESET_LIST, &[]);
}

/// Former `selectDrivenByMossPreset`.
pub fn select_driven_by_moss_preset(ctx: &Ctx) {
    let state = state(ctx);
    select_preset(ctx, state.bank_index, state.preset_index);
}

/// Former `selectPreset`: the bank (0-11) and the preset in it (0-11).
fn select_preset(ctx: &Ctx, bank: i32, preset: i32) {
    ctx.host.println(&format!("Selecting preset: {bank}-{preset}"));
    send_sysex(ctx, &SYSEX_RUNTIME_SWITCH_PRESET, &[(bank & 0x7F) as u8, (preset & 0x7F) as u8]);
}

/// Former `selectPage`: a page of the active preset (0-11).
pub fn select_page(ctx: &Ctx, page: i32) {
    ctx.host.println(&format!("Selecting page: {page}"));
    send_sysex(ctx, &SYSEX_RUNTIME_SWITCH_PAGE, &[(page & 0x7F) as u8]);
}

// ---------------------------------------------------------------------------------------------
// Shift and the preset of the cursor device

/// Former `updateShift`: the emulated Shift button, which also slows down the knobs.
pub fn update_shift(ctx: &mut Ctx, is_shift: bool) {
    state_mut(ctx).is_shift_pressed = is_shift;
    ctx.surface.shift_pressed_override = Some(is_shift);
    ctx.surface.set_knob_sensitivity_is_slow(is_shift);
}

/// Former `clearShift` (the Mini has a Shift button and keeps it).
fn clear_shift(ctx: &mut Ctx) {
    if state(ctx).variant == ElectraVariant::Normal {
        update_shift(ctx, false);
    }
}

/// Former `switchToSpecificDevicePreset`: select the preset with the name of the cursor device,
/// the device page if there is none. If the DrivenByMoss preset is not selected, select it.
pub fn switch_to_specific_device_preset(ctx: &mut Ctx) {
    if !is_online(ctx) {
        select_driven_by_moss_preset(ctx);
        return;
    }
    if ctx.modes.is_active(Modes::Dummy) || ctx.active_mode().is_none() {
        return;
    }
    // Former AbstractElectraOneBaseMode.getActiveDeviceName
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return;
    }
    let device_name = device.name().to_string();
    match state(ctx).preset(&device_name) {
        None => {
            ctx.host.println(&format!("No specific preset found for '{device_name}', switching to device mode."));
            select_page(ctx, 2);
        }
        Some((bank, slot)) => {
            ctx.host.println(&format!("Found preset found for '{device_name}' at {bank}-{slot}."));
            select_preset(ctx, bank, slot);
        }
    }
    // Prevent hanging states
    state_mut(ctx).knob_states.fill(0);
}

// ---------------------------------------------------------------------------------------------
// Receiving

/// Handle (a chunk of) a SysEx message from the CTRL port (former `handleSysEx`): the chunks are
/// collected until the one which ends with F7.
pub fn handle_ctrl_sysex(ctx: &mut Ctx, data: &[u8]) {
    // Only SysEx (Java sets only a SysEx callback on the CTRL input): a message which starts
    // with another status byte is ignored; a chunk after the first starts with a data byte
    if data.is_empty() || (data[0] & 0x80 != 0 && data[0] != 0xF0 && data[0] != 0xF7) {
        return;
    }
    let full = {
        let state = state_mut(ctx);
        let unsound = data[0] == 0xF0 && !state.sysex_chunks.is_empty();
        if unsound {
            state.sysex_chunks.clear();
        }
        state.sysex_chunks.push(data.to_vec());
        let full = (data[data.len() - 1] == 0xF7).then(|| state.sysex_chunks.concat());
        if full.is_some() {
            state.sysex_chunks.clear();
        }
        if unsound {
            ctx.host.error("Unsound sysex message without ending F7 received.");
        }
        full
    };
    if let Some(full) = full {
        process_sysex(ctx, &full);
    }
}

fn process_sysex(ctx: &mut Ctx, data: &[u8]) {
    if data.len() <= SUB_CMD_START_POS || data[..SYSEX_HEADER.len()] != SYSEX_HEADER {
        return;
    }
    // The messages of the surface link (row, selection, ACK/NACK; not part of Java)
    if link::handle_device_sysex(ctx, data) {
        return;
    }
    let sub_command = data[SUB_CMD_START_POS];
    match data[CMD_START_POS] {
        CMD_INFO => handle_sysex_commands_info(ctx, sub_command, data),
        CMD_CONTROLLER => handle_sysex_commands_controller(ctx, sub_command, data),
        CMD_SYSTEM_CALL if sub_command == SYSTEM_CALL_LOGGING => log_message(ctx, data, 6),
        _ => {}
    }
}

/// The byte at the position, 0 if the message is too short (Java would throw).
fn byte(data: &[u8], position: usize) -> i32 {
    data.get(position).copied().unwrap_or(0) as i32
}

/// Former `handleSysexCommandsController`.
fn handle_sysex_commands_controller(ctx: &mut Ctx, command: u8, data: &[u8]) {
    match command {
        // Take the extension on-/offline depending on which preset is selected
        EVENT_PRESET_SWITCH => handle_online_status(ctx, byte(data, SUB_CMD_START_POS + 1), byte(data, SUB_CMD_START_POS + 2)),

        // Change modes if the extension is online
        EVENT_PAGE_SWITCH => {
            if !is_online(ctx) {
                return;
            }
            let page = byte(data, SUB_CMD_START_POS + 1) as usize;
            if let Some(&mode) = state(ctx).variant.modes().get(page) {
                ctx.host.println(&format!("Switching to mode: {}", mode_name(mode)));
                ctx.set_active_mode(mode);
                link::on_page_switched(ctx);
            }
        }

        EVENT_POT_TOUCH => {
            let pot_id = byte(data, SUB_CMD_START_POS + 1);
            let control_id = (byte(data, SUB_CMD_START_POS + 3) << 7) + byte(data, SUB_CMD_START_POS + 2);
            if !(0..12).contains(&pot_id) {
                ctx.host.error(&format!("Touch event with knob ID outside of range: {pot_id}"));
                return;
            }
            let touched = byte(data, SUB_CMD_START_POS + 4);
            let Some(knob_state) = state_mut(ctx).knob_states.get_mut(pot_id as usize) else {
                // The Mini has only 8 knob states (Java throws)
                return;
            };
            *knob_state = touched;
            set_editing(ctx, control_id, touched > 0);
            match_states(ctx);
        }

        _ => {}
    }
}

/// The name of a mode as Java's `Modes.name ()`.
fn mode_name(mode: Modes) -> &'static str {
    match mode {
        Modes::Volume => "VOLUME",
        Modes::Send => "SEND",
        Modes::DeviceParams => "DEVICE_PARAMS",
        Modes::EqDeviceParams => "EQ_DEVICE_PARAMS",
        Modes::Transport => "TRANSPORT",
        Modes::Session => "SESSION",
        Modes::Project => "PROJECT",
        _ => "?",
    }
}

/// Former `AbstractElectraOneBaseMode.setEditing`: the knob of a control is touched; for the
/// active Electra mode (not the Dummy mode).
pub(crate) fn set_editing(ctx: &mut Ctx, control_id: i32, is_touched: bool) {
    if ctx.modes.is_active(Modes::Dummy) {
        return;
    }
    let Some(mode) = ctx.active_mode() else {
        return;
    };
    let (rows, columns) = state(ctx).variant.grid();
    let page_size = (rows * columns) as i32;
    let index = (control_id - 1) % page_size;
    if index < 0 || index >= page_size {
        ctx.host.error(&format!("Control ID out of bounds: {control_id}"));
        return;
    }
    if let Ok(mut mode) = mode.try_borrow_mut() {
        mode.on_knob_touch(ctx, index as usize, is_touched);
    };
}

/// Former `matchStates`: execute the function of the touched knob combination.
fn match_states(ctx: &mut Ctx) {
    let found = {
        let state = state(ctx);
        let patterns = state.variant.touch_patterns();
        state.command_patterns.iter().copied().find(|&index| patterns[index] == state.knob_states.as_slice())
    };
    let Some(index) = found else {
        clear_shift(ctx);
        return;
    };
    let mut commands = std::mem::take(&mut state_mut(ctx).touch_commands);
    if let Some(command) = commands.get_mut(index) {
        command.execute(ctx, ButtonEvent::Down);
        command.execute(ctx, ButtonEvent::Up);
    }
    state_mut(ctx).touch_commands = commands;
    // Prevent hanging states
    state_mut(ctx).knob_states.fill(0);
}

/// Former `updateTouchCombinationCommandsCache`: the patterns which have a function.
pub fn update_touch_combination_commands_cache(ctx: &mut Ctx) {
    let variant = state(ctx).variant;
    let patterns = variant.touch_patterns();
    let mut command_patterns = Vec::new();
    let mut shift_patterns = Vec::new();
    for (i, pattern) in patterns.iter().enumerate() {
        let setting = config::assignable(&ctx.config, i);
        if setting == config::ELECTRA_ONE_FUNC_OFF {
            continue;
        }
        if variant == ElectraVariant::Normal && setting == config::ELECTRA_ONE_FUNC_SHIFT_BUTTON {
            // Specific shift patterns which ignore all other touched knobs (state = 2)
            shift_patterns.push(pattern.iter().map(|&p| if p == 1 { 1 } else { 2 }).collect());
        } else {
            command_patterns.push(i);
        }
    }
    let state = state_mut(ctx);
    state.command_patterns = command_patterns;
    state.shift_patterns = shift_patterns;
}

/// Former `handleSysexCommandsInfo`.
fn handle_sysex_commands_info(ctx: &mut Ctx, command: u8, data: &[u8]) {
    if command != INFO_DEVICE && command != INFO_PRESET_LIST {
        return;
    }
    let Some(content) = content(ctx, data) else {
        return;
    };
    if command == INFO_DEVICE {
        if let Err(error) = check_firmware_result(ctx, &content) {
            ctx.host.error(&error);
            return;
        }
        // The surface link adds the control set events if the device shows rows (not in Java)
        let events = link::on_device_info(ctx, &content);
        send_sysex(ctx, &SYSEX_RUNTIME_SUBSCRIBE_EVENTS, &[events]);
        request_preset_list(ctx);
    } else {
        if let Err(error) = find_and_select_driven_by_moss_preset(ctx, &content) {
            ctx.host.error(&error);
            return;
        }
        select_driven_by_moss_preset(ctx);
    }
}

/// Former `checkFirmwareResult` of the two surfaces.
fn check_firmware_result(ctx: &mut Ctx, root: &JsonValue) -> Result<(), String> {
    let version = root.get("versionSeq").map(JsonValue::as_int).ok_or("The device information has no versionSeq.")?;
    let version_text = root.get("versionText").map(JsonValue::as_text).ok_or("The device information has no versionText.")?;
    let model = root.get("model").map(JsonValue::as_text);
    match state(ctx).variant {
        ElectraVariant::Normal => {
            if version < 300400300 {
                return Err(format!("Firmware must be at least 3.4.3 but is {version_text}"));
            }
            if let Some(model) = model {
                let electra_one_type = match model.as_str() {
                    "mk1" => ElectraOneType::Mk1,
                    "mk2" => ElectraOneType::Mk2,
                    _ => return Err(format!("This extension does only support the Electro One Mk1 and Mk2 controller models but found model: {model}")),
                };
                state_mut(ctx).electra_one_type = electra_one_type;
            }
            let model = match state(ctx).electra_one_type {
                ElectraOneType::Mk1 => "MK1",
                ElectraOneType::Mk2 => "MK2",
            };
            ctx.host.println(&format!("Model: {model} - Firmware: {version_text}"));
        }
        ElectraVariant::Mini => {
            if version < 500000400 {
                return Err(format!("Firmware must be at least 5.0.4 but is {version_text}"));
            }
            if let Some(model) = model {
                if model != "mini" {
                    return Err(format!("This extension does only support the Electro One Mini but found model: {model}"));
                }
            }
            ctx.host.println(&format!("Model: Mini - Firmware: {version_text}"));
        }
    }
    Ok(())
}

/// Former `findAndSelectDrivenByMossPreset`: find the DrivenByMoss preset in the preset list and
/// remember the others by their names.
fn find_and_select_driven_by_moss_preset(ctx: &mut Ctx, root: &JsonValue) -> Result<(), String> {
    let template_name = state(ctx).variant.template_name();
    let mut found = None;
    let mut others = Vec::new();
    for preset in root.get("presets").map(JsonValue::elements).unwrap_or_default() {
        let (Some(name), Some(bank), Some(slot)) = (preset.get("name"), preset.get("bankNumber"), preset.get("slot")) else {
            continue;
        };
        let name = name.as_text();
        let (bank, slot) = (bank.as_int() as i32, slot.as_int() as i32);
        if name == template_name {
            found = Some((bank, slot));
        } else {
            others.push((name, (bank, slot)));
        }
    }
    let electra = state_mut(ctx);
    electra.presets.extend(others);
    if let Some((bank, slot)) = found {
        electra.bank_index = bank;
        electra.preset_index = slot;
        ctx.host.println(&format!("{template_name} template at: {bank}-{slot}"));
    }
    if state(ctx).bank_index == -1 {
        return Err(format!("The {template_name} template is not installed on the Electra.One."));
    }
    Ok(())
}

/// Former `logMessage`: a log line of the device, if logging is enabled.
fn log_message(ctx: &Ctx, data: &[u8], content_start: usize) {
    if !config::is_log_to_console_enabled(&ctx.config) || data.len() <= content_start {
        return;
    }
    let text: String = data[content_start..data.len() - 1].iter().map(|&b| b as char).collect();
    ctx.host.println(&text);
}

/// Former `getContent`: the JSON of an information message.
fn content(ctx: &Ctx, data: &[u8]) -> Option<JsonValue> {
    let start = SUB_CMD_START_POS + 1;
    let end = data.len().saturating_sub(1).max(start);
    let text: String = data.get(start..end).unwrap_or_default().iter().map(|&b| b as char).collect();
    match JsonValue::parse(&text) {
        Ok(content) => Some(content),
        Err(error) => {
            ctx.host.error(&text);
            ctx.host.error(&format!("Could not parse JSON information: {error}"));
            None
        }
    }
}

/// Former `handleOnlineStatus`: online if the DrivenByMoss preset is selected.
fn handle_online_status(ctx: &mut Ctx, bank: i32, preset: i32) {
    let state = state(ctx);
    if state.bank_index == bank && state.preset_index == preset {
        set_online(ctx);
    } else {
        set_offline(ctx);
    }
}

/// Former `setOffline`.
fn set_offline(ctx: &mut Ctx) {
    ctx.host.println("Going offline...");
    let active = ctx.modes.active_id();
    let state = state_mut(ctx);
    state.online.set(false);
    // Java also remembers the Dummy mode if it is already offline
    if active != Some(Modes::Dummy) {
        state.active_mode = active;
    }
    ctx.set_active_mode(Modes::Dummy);
}

/// Former `setOnline`.
fn set_online(ctx: &mut Ctx) {
    ctx.host.println("Going online...");
    state(ctx).online.set(true);
    ctx.set_active_mode(Modes::Volume);
    let state = state(ctx);
    let mode = state.active_mode.unwrap_or(Modes::Volume);
    let page = state.variant.modes().iter().position(|m| *m == mode).map_or(-1, |p| p as i32);
    select_page(ctx, page);
    link::on_page_switched(ctx);
    link::request_initial_state(ctx);
}
