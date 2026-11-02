// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Generic Flexi specific parts of the control surface (former `GenericFlexiControlSurface`)
//! and the controller specific state, which is stored in `ctx.ext` ([`FlexiState`]).
//!
//! All MIDI is handled by [`handle_midi`] and [`handle_sysex`]: the received messages are
//! matched against the command slots and the command of the matching slot is executed by its
//! handler (see `super::handler`). [`flush`] sends the values of the commands back to the device
//! (for the slots with "Send value to device").
//!
//! Differences to Java:
//! - The Shift state of the Flexi (`setShiftPressed`) is the pressed state of a virtual Shift
//!   button (it is not bound to MIDI), so that the generic code which checks
//!   `ctx.surface.is_shift_pressed()` sees it.
//! - The relative value changers are owned here. The two's complement one is a copy of the value
//!   changer of the model, which gets the knob sensitivity (the model's own value changer has no
//!   sensitivity in Rust). The handlers use it for all relative changes.
//! - The file dialogs of "Load" and "Save" do not exist (no UI), see `super::mapping`.

use std::rc::Rc;
use std::time::{Duration, Instant};

use super::command::{CommandCategory, FlexiCommand};
use super::handler;
use super::slot::{CommandSlot, KnobMode, MidiValue, ProgramBank, TYPE_CC, TYPE_MMC, TYPE_NOTE, TYPE_PITCH_BEND, TYPE_PROGRAM_CHANGE};
use crate::config::generic_flexi::{self as cfg, NUM_SLOTS, OPTIONS_TYPE};
use crate::framework::clip_launcher_navigator::{ClipLauncherNavigator, SharedNavigator};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::value_changer::{RelativeEncoding, ValueChanger};
use crate::midi::{from_hex_str, ButtonEvent, CMD_CC, CMD_NOTE_OFF, CMD_NOTE_ON, CMD_PITCHBEND, CMD_PROGRAM_CHANGE};

/// How long the handled command blocks the sending of values (former 400 ms task of
/// `handleCommand`).
pub const UPDATING_VALUE_DELAY_MS: u64 = 400;

/// The values of the last MIDI learn (former `learnTypeValue` etc.).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearnValues {
    pub type_value: Option<String>,
    pub number_value: String,
    pub midi_channel_value: String,
    pub resolution: bool,
}

/// The state of a Generic Flexi instance (parts of the former `GenericFlexiControlSurface`,
/// `GenericFlexiConfiguration` and `GenericFlexiControllerSetup`).
pub struct FlexiState {
    /// The command slots (former `GenericFlexiConfiguration.commandSlots`).
    pub slots: Vec<CommandSlot>,
    /// The index of the slot selected in the slot editor.
    pub selected_slot: usize,
    /// The cached key translation table (former `keyMap`), None if it needs to be recalculated.
    pub key_map: Option<Vec<i32>>,
    /// The keyboard channel read at startup: -1 = off, 0-15 the MIDI channel, 16 = omni (it
    /// requires a restart, as in Java).
    pub keyboard_channel: i32,

    /// The last values sent to the device per slot (former `valueCache`).
    pub value_cache: Vec<i32>,
    /// When the last CC was received.
    pub last_received: Option<Instant>,
    /// The number of the last received CC, -1 if none.
    pub last_cc_received: i32,
    /// The last received value of each CC.
    pub last_cc_values: [i32; 128],
    /// True while a command is handled (no values are sent back to the device).
    pub is_updating_value: bool,
    /// The active function layer (0-9).
    pub function_layer: i32,
    /// The layer to return to from a temporary layer.
    pub previous_function_layer: i32,

    /// The value changer for absolute 7 bit values (former `absoluteLowResValueChanger`).
    pub absolute_low_res: ValueChanger,
    /// The default value changer for 14 bit values and relative two's complement changes (a copy
    /// of the one of the model, with the knob sensitivity).
    pub default_changer: ValueChanger,
    pub signed_bit: ValueChanger,
    pub signed_bit2: ValueChanger,
    pub offset_binary: ValueChanger,
    /// The movement counters of the handlers (former `AbstractHandler.movementCounter`), one per
    /// command category (each category has its own handler).
    pub movement_counters: [i32; CommandCategory::ALL.len()],

    /// The last MIDI learn values.
    pub learn: LearnValues,
    /// The values of the editor settings which were set by the editor itself, their change
    /// notifications are ignored (replaces the `commandIsUpdating` flag of Java, which blocks the
    /// observers of the function settings while `setCommand` sets them).
    pub editor_updates: Vec<(crate::config::SettingId, String)>,

    /// The program banks of the programs file (former `GenericFlexiControllerSetup.banks`).
    pub banks: Vec<ProgramBank>,
    /// The first setting ID of the program bank settings.
    pub bank_settings: Vec<crate::config::SettingId>,

    /// The browser was active on the last check (replaces the browser observer).
    pub browser_active: bool,
    /// The clip launcher navigator of the model (former `IClipLauncherNavigator`).
    pub navigator: SharedNavigator,
}

impl FlexiState {
    pub fn new(keyboard_channel: i32, default_changer: ValueChanger) -> Self {
        FlexiState {
            slots: vec![CommandSlot::default(); NUM_SLOTS],
            selected_slot: 0,
            key_map: None,
            keyboard_channel,
            value_cache: vec![-1; NUM_SLOTS],
            last_received: None,
            last_cc_received: -1,
            last_cc_values: [0; 128],
            is_updating_value: false,
            function_layer: 0,
            previous_function_layer: 0,
            absolute_low_res: ValueChanger::twos_complement(128, 1),
            default_changer,
            signed_bit: ValueChanger::new(RelativeEncoding::SignedBit, 16384, 100),
            signed_bit2: ValueChanger::new(RelativeEncoding::SignedBit2, 16384, 100),
            offset_binary: ValueChanger::new(RelativeEncoding::OffsetBinary, 16384, 100),
            movement_counters: [0; CommandCategory::ALL.len()],
            learn: LearnValues::default(),
            editor_updates: Vec::new(),
            banks: Vec::new(),
            bank_settings: Vec::new(),
            browser_active: false,
            navigator: Rc::new(std::cell::RefCell::new(ClipLauncherNavigator::default())),
        }
    }

    /// Get all matching configured slots (their indices), if any (former `getSlotCommands`).
    pub fn slot_commands(&self, slot_type: i32, number: i32, midi_channel: i32) -> Vec<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| {
                slot.command != FlexiCommand::Off
                    && slot.slot_type == slot_type
                    && (slot_type == TYPE_PITCH_BEND || slot.number == number)
                    && (slot.midi_channel == midi_channel || slot.midi_channel == 16)
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Get a key translation map which blocks the notes that are mapped to a command from the
    /// keyboard note input (former `getNoteMap`).
    pub fn note_map(&mut self) -> Vec<i32> {
        if self.key_map.is_none() {
            let mut key_map: Vec<i32> = (0..128).collect();
            for slot in &self.slots {
                if slot.command == FlexiCommand::Off || slot.slot_type != TYPE_NOTE {
                    continue;
                }
                if slot.midi_channel == self.keyboard_channel || self.keyboard_channel == 16 {
                    if let Some(entry) = usize::try_from(slot.number).ok().and_then(|n| key_map.get_mut(n)) {
                        *entry = -1;
                    }
                }
            }
            self.key_map = Some(key_map);
        }
        self.key_map.clone().unwrap_or_default()
    }

    /// All commands which are used in a slot (former `getMappedCommands`).
    pub fn mapped_commands(&self) -> std::collections::HashSet<FlexiCommand> {
        self.slots.iter().map(|slot| slot.command).collect()
    }

    /// The value changer for absolute values (former `AbstractHandler.getAbsoluteValueChanger`).
    pub fn absolute_value_changer(&self, value: MidiValue) -> &ValueChanger {
        if value.is_high_res() {
            &self.default_changer
        } else {
            &self.absolute_low_res
        }
    }

    /// The value changer for relative values (former `AbstractHandler.getRelativeValueChanger`).
    pub fn relative_value_changer(&self, knob_mode: KnobMode) -> &ValueChanger {
        match knob_mode {
            KnobMode::RelativeSignedBit => &self.signed_bit,
            KnobMode::RelativeSignedBit2 => &self.signed_bit2,
            KnobMode::RelativeOffsetBinary => &self.offset_binary,
            _ => &self.default_changer,
        }
    }
}

/// The state of the Flexi.
pub fn flexi(ctx: &Ctx) -> &FlexiState {
    ctx.ext::<FlexiState>()
}

pub fn flexi_mut(ctx: &mut Ctx) -> &mut FlexiState {
    ctx.ext_mut::<FlexiState>()
}

// ------------------------------------------------------------------------------------------
// Shift, modes, key translation

/// Former `isShiftPressed`.
pub fn is_shift_pressed(ctx: &Ctx) -> bool {
    ctx.surface.is_shift_pressed()
}

/// Set the Shift button pressed state (former `setShiftPressed`).
pub fn set_shift_pressed(ctx: &mut Ctx, is_pressed: bool) {
    if let Some(button) = ctx.surface.button_mut(ButtonId::Shift) {
        button.state = Some(if is_pressed { ButtonEvent::Down } else { ButtonEvent::Up });
    }
}

/// Activate a new mode (former `activateMode`).
pub fn activate_mode(ctx: &mut Ctx, mode_id: Modes) {
    let Some(mode) = ctx.modes.get(mode_id) else {
        return;
    };
    let mode_name = match mode.try_borrow() {
        Ok(mode) => mode.name().to_string(),
        Err(_) => return,
    };
    if !ctx.modes.is_active(mode_id) {
        ctx.set_active_mode(mode_id);
        ctx.host.show_notification(&mode_name);
    }
    if mode_name != cfg::selected_mode_name(&ctx.config) {
        ctx.config.set_value(cfg::SELECTED_MODE, &mode_name);
    }
}

/// Update the key translation table (former `updateKeyTranslation`).
pub fn update_key_translation(ctx: &mut Ctx) {
    let table = flexi_mut(ctx).note_map();
    ctx.surface.set_key_translation_table(table);
}

/// Clear the note map (former `GenericFlexiConfiguration.clearNoteMap`), which notifies a slot
/// change.
pub fn clear_note_map(ctx: &mut Ctx) {
    flexi_mut(ctx).key_map = None;
    ctx.config.mark_changed(cfg::SLOT_CHANGE);
}

/// Set the knob sensitivity of the value changers (former `updateRelativeKnobSensitivity`).
pub fn update_relative_knob_sensitivity(ctx: &mut Ctx) {
    let sensitivity = if ctx.surface.is_knob_sensitivity_slow() { ctx.config.knob_sensitivity_slow() } else { ctx.config.knob_sensitivity_default() } as f64;
    let state = flexi_mut(ctx);
    state.absolute_low_res.set_sensitivity(sensitivity);
    state.signed_bit.set_sensitivity(sensitivity);
    // The signed bit 2 value changer is not updated in Java
    state.offset_binary.set_sensitivity(sensitivity);
    state.default_changer.set_sensitivity(sensitivity);
}

// ------------------------------------------------------------------------------------------
// MIDI

/// Store the values of a received MIDI message for the MIDI learn (former
/// `GenericFlexiConfiguration.setLearnValues`).
pub fn set_learn_values(ctx: &mut Ctx, slot_type: i32, number: i32, midi_channel: i32, is_high_res: bool) {
    let type_value = OPTIONS_TYPE[(slot_type + 1) as usize].to_string();
    let number_value = cfg::NUMBER_NAMES[number.clamp(0, 127) as usize].to_string();
    let midi_channel_value = (midi_channel + 1).to_string();
    let resolution = cfg::OPTIONS_RESOLUTION[usize::from(is_high_res)];
    ctx.config.set_value(cfg::LEARN_TYPE, &type_value);
    ctx.config.set_value(cfg::LEARN_NUMBER, &number_value);
    ctx.config.set_value(cfg::LEARN_MIDI_CHANNEL, &midi_channel_value);
    ctx.config.set_value(cfg::LEARN_RESOLUTION, resolution);
    flexi_mut(ctx).learn = LearnValues { type_value: Some(type_value), number_value, midi_channel_value, resolution: is_high_res };
}

/// Handle all short MIDI messages (former `handleMidi`).
pub fn handle_midi(ctx: &mut Ctx, status: i32, data1: i32, data2: i32) {
    let code = status & 0xF0;
    let channel = status & 0xF;
    match code {
        CMD_NOTE_OFF | CMD_NOTE_ON => {
            set_learn_values(ctx, TYPE_NOTE, data1, channel, false);
            let value = MidiValue::new(if code == CMD_NOTE_OFF { 0 } else { data2 }, false);
            let slots = flexi(ctx).slot_commands(TYPE_NOTE, data1, channel);
            let slot = process_function_layer(ctx, &slots, value);
            handle_command(ctx, slot, value);
        }
        CMD_PROGRAM_CHANGE => handle_program_change(ctx, channel, data1),
        CMD_CC => handle_cc(ctx, channel, data1, data2),
        CMD_PITCHBEND => {
            set_learn_values(ctx, TYPE_PITCH_BEND, data1, channel, true);
            let value = MidiValue::new(data1 + data2 * 128, true);
            let slots = flexi(ctx).slot_commands(TYPE_PITCH_BEND, data1, channel);
            let slot = process_function_layer(ctx, &slots, value);
            handle_command(ctx, slot, value);
        }
        // Not used
        _ => {}
    }
}

/// Switch the function layer if one of the slots is a layer switcher, otherwise return the first
/// slot of the active function layer (former `processFunctionLayer`).
fn process_function_layer(ctx: &mut Ctx, slots: &[usize], value: MidiValue) -> Option<usize> {
    // Check for layer switcher command
    let function_layer = flexi(ctx).function_layer;
    for &index in slots {
        let slot = &flexi(ctx).slots[index];
        if slot.command.index_in(FlexiCommand::FunctionLayer1, 19).is_some() {
            let fl = slot.function_layer;
            if fl < 0 || fl == function_layer {
                let (command, knob_mode) = (slot.command, slot.knob_mode);
                switch_function_layer(ctx, command, knob_mode, value);
                return None;
            }
        }
    }

    // Only accept commands for the active function layer
    slots.iter().copied().find(|&index| {
        let fl = flexi(ctx).slots[index].function_layer;
        fl < 0 || fl == function_layer
    })
}

/// Handle a CC, including 14 bit values (former `handleCC`).
fn handle_cc(ctx: &mut Ctx, channel: i32, data1: i32, data2: i32) {
    let now = ctx.host.now();
    let (data1, data2) = (data1.clamp(0, 127), data2.clamp(0, 127));

    // High resolution command? See MIDI 1.0 Detailed Specification 4.2, page 11
    let state = flexi_mut(ctx);
    let is_related = state.last_received.is_some_and(|last| now.saturating_duration_since(last) < Duration::from_millis(1000));
    let is_high_res = is_related && state.last_cc_received < 32 && state.last_cc_received + 32 == data1;
    state.last_cc_received = data1;
    state.last_received = Some(now);
    state.last_cc_values[data1 as usize] = data2;

    set_learn_values(ctx, TYPE_CC, data1, channel, is_high_res);

    let state = flexi(ctx);
    let mut value = 0;
    let mut is_high_res_value = false;
    let mut matched_slot = None;

    // Check for high resolution related setting
    if data1 < 32 {
        for index in state.slot_commands(TYPE_CC, data1, channel) {
            let slot = &state.slots[index];
            let fl = slot.function_layer;
            // Note: the operator precedence is the one of the Java version: a slot of all
            // layers is always treated as a high resolution slot
            if fl < 0 || fl == state.function_layer && slot.is_high_res {
                matched_slot = Some(index);
                value = data2 * 128 + state.last_cc_values[(data1 + 32) as usize];
                is_high_res_value = true;
            }
        }
    } else if data1 < 64 {
        let first_cc = data1 - 32;
        for index in state.slot_commands(TYPE_CC, first_cc, channel) {
            let slot = &state.slots[index];
            if slot.function_layer == state.function_layer && slot.is_high_res {
                matched_slot = Some(index);
                value = state.last_cc_values[first_cc as usize] * 128 + data2;
                is_high_res_value = true;
            }
        }
    }

    // No Hi-Res
    let slots = match matched_slot {
        Some(index) => vec![index],
        None => {
            let slots = state.slot_commands(TYPE_CC, data1, channel);
            if !slots.is_empty() {
                value = data2;
            }
            slots
        }
    };

    let midi_value = MidiValue::new(value, is_high_res_value);
    let slot = process_function_layer(ctx, &slots, midi_value);
    handle_command(ctx, slot, midi_value);
}

/// Handle the reception of a program change message (former `handleProgramChange`).
fn handle_program_change(ctx: &mut Ctx, channel: i32, data1: i32) {
    set_learn_values(ctx, TYPE_PROGRAM_CHANGE, data1, channel, false);
    let slots = flexi(ctx).slot_commands(TYPE_PROGRAM_CHANGE, data1, channel);
    let Some(slot) = process_function_layer(ctx, &slots, MidiValue::new(127, false)) else {
        return;
    };
    if flexi(ctx).slots[slot].command.is_trigger() {
        handle_command(ctx, Some(slot), MidiValue::new(127, false));
        handle_command(ctx, Some(slot), MidiValue::new(0, false));
    } else {
        // Note: there is no data2 value for PC
        handle_command(ctx, Some(slot), MidiValue::new(data1, false));
    }
}

/// Handle system exclusive for MMC commands (former `handleSysEx`).
pub fn handle_sysex(ctx: &mut Ctx, data: &str) {
    let data = from_hex_str(data);
    if data.len() != 6 || data[0] != 0xF0 || data[1] != 0x7F || data[3] != 0x06 || data[5] != 0xF7 {
        return;
    }

    // This is not (fully) correct but at least supports 16 device IDs (and 7F for ignore)
    let channel = (data[2] % 16) as i32;
    let number = data[4] as i32;

    set_learn_values(ctx, TYPE_MMC, number, channel, false);
    let slots = flexi(ctx).slot_commands(TYPE_MMC, number, channel);
    let Some(slot) = process_function_layer(ctx, &slots, MidiValue::new(127, false)) else {
        return;
    };
    handle_command(ctx, Some(slot), MidiValue::new(127, false));
    handle_command(ctx, Some(slot), MidiValue::new(0, false));
}

/// Execute the command of a slot (former `handleCommand`).
pub fn handle_command(ctx: &mut Ctx, slot: Option<usize>, value: MidiValue) {
    let Some(index) = slot else {
        return;
    };
    flexi_mut(ctx).is_updating_value = true;
    let (command, knob_mode) = {
        let slot = &flexi(ctx).slots[index];
        (slot.command, slot.knob_mode)
    };
    if !handler::has_handler(command) {
        // Note: as in Java the updating flag is not reset in this case
        ctx.host.error(&format!("No handler registered for command: {}", command.name()));
        return;
    }
    handler::handle(ctx, command, knob_mode, value);
    ctx.schedule(UPDATING_VALUE_DELAY_MS, |ctx| flexi_mut(ctx).is_updating_value = false);
}

/// Switch the function layer (former `switchFunctionLayer`).
fn switch_function_layer(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let state = flexi_mut(ctx);
    let old_layer = state.function_layer;

    if let Some(layer) = command.index_in(FlexiCommand::FunctionLayer1, 10) {
        if knob_mode == KnobMode::AbsoluteToggle || knob_mode == KnobMode::Absolute && value.is_positive() {
            state.function_layer = layer as i32;
        }
    } else if let Some(layer) = command.index_in(FlexiCommand::FunctionLayer2Temp, 9) {
        // Note: this is one off since there is no 'activate layer 1 temporarily'!
        let layer_index = layer as i32 + 1;
        if knob_mode == KnobMode::AbsoluteToggle && state.function_layer != layer_index || knob_mode == KnobMode::Absolute && value.is_positive() {
            state.previous_function_layer = old_layer;
            state.function_layer = layer_index;
        } else {
            state.function_layer = state.previous_function_layer;
        }
    } else {
        return;
    }

    let function_layer = state.function_layer;
    if old_layer != function_layer {
        ctx.host.show_notification(&format!("Functions Layer {}", function_layer + 1));
    }
}

// ------------------------------------------------------------------------------------------
// Flush

/// Send the values of all slots which have "Send value to device" enabled (former `flush`).
pub fn flush(ctx: &mut Ctx) {
    for index in 0..NUM_SLOTS {
        let slot = &flexi(ctx).slots[index];
        if slot.command != FlexiCommand::Off && slot.send_value {
            flush_value(ctx, index);
        }
    }
}

fn flush_value(ctx: &mut Ctx, index: usize) {
    let slot = flexi(ctx).slots[index].clone();
    if flexi(ctx).is_updating_value && !(slot.command.is_trigger() && slot.send_value_when_received) {
        return;
    }

    let value = command_value(ctx, slot.command);
    let state = flexi_mut(ctx);
    if state.value_cache[index] == value {
        return;
    }
    state.value_cache[index] = value;
    reflect_value(ctx, &slot, value);
}

/// Get the current value of a command, scaled down to 7 bit, -1 if it has none (former
/// `getCommandValue`).
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    if !handler::has_handler(command) {
        return -1;
    }
    let value = handler::command_value(ctx, command);
    // Scale down to 7-bit
    (value as f64 * 127.0 / 16383.0).round() as i32
}

/// Send back the current value of a command slot to the device (former `reflectValue`).
fn reflect_value(ctx: &mut Ctx, slot: &CommandSlot, value: i32) {
    if !(0..=127).contains(&value) {
        ctx.host.error(&format!("Attempt to reflect value out of range from slot command: {} Value: {value}", slot.command.name()));
        return;
    }

    // Cannot reflect "All" setting
    let midi_channel = slot.midi_channel;
    if midi_channel > 15 {
        return;
    }

    let output = &ctx.surface.output;
    match slot.slot_type {
        TYPE_NOTE => output.send_note_ex(midi_channel, slot.number, value),
        TYPE_CC => output.send_cc_ex(midi_channel, slot.number, value),
        TYPE_PITCH_BEND => output.send_pitchbend(midi_channel, 0, value),
        // Other types not supported
        _ => {}
    }
}
