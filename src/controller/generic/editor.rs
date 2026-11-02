// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The slot editor: the settings of the selected slot and the MIDI learn (the setting observers
//! of the former `GenericFlexiConfiguration.init`).
//!
//! The settings of the selected slot ("Selected Slot - ..." categories) edit the command slot
//! which is selected with the "Selected:" setting. Hosts which show the settings in their own UI
//! (Bitwig) can therefore edit the mapping.
//!
//! Differences to Java: the Java setting observers are called synchronously, also when a setting
//! is set by the editor itself, and the editor uses flags (`doNotFire`, `commandIsUpdating`) to
//! ignore some of these calls. Here the changes are handled in `handle_changes`, the editor
//! applies its own changes directly to the slot and ignores their change notifications (see
//! [`FlexiState::editor_updates`]). Selecting a slot only shows its values, it does not change
//! the slot (in Java the observers of the type and number settings may reset the 14 bit
//! resolution of the newly selected slot since they still see the number of the previous one).

use super::command::FlexiCommand;
use super::slot::{KnobMode, TYPE_CC, TYPE_MMC, TYPE_PITCH_BEND};
use super::surface::{clear_note_map, flexi, flexi_mut, FlexiState};
use crate::config::generic_flexi::{self as cfg, NUMBER_NAMES, OPTIONS_RESOLUTION, OPTIONS_TYPE};
use crate::config::values::{lookup_index, ON_OFF_OPTIONS};
use crate::config::SettingId;
use crate::framework::ctx::Ctx;

/// Set a setting of the editor from the editor itself: its change notification is ignored.
fn editor_set(ctx: &mut Ctx, id: SettingId, value: &str) {
    if ctx.config.set_value(id, value) {
        flexi_mut(ctx).editor_updates.push((id, value.to_string()));
    }
}

/// True if the change of the setting was done by the editor itself (and forget it).
fn is_editor_update(ctx: &mut Ctx, id: SettingId) -> bool {
    let value = ctx.config.value(id).unwrap_or_default();
    let updates = &mut flexi_mut(ctx).editor_updates;
    let found = updates.iter().any(|(update_id, update_value)| *update_id == id && *update_value == value);
    updates.retain(|(update_id, _)| *update_id != id);
    found
}

fn value_of(ctx: &Ctx, id: SettingId) -> String {
    ctx.config.value(id).unwrap_or_default()
}

fn selected_slot_type(ctx: &Ctx) -> i32 {
    lookup_index(&OPTIONS_TYPE, &value_of(ctx, cfg::SLOT_TYPE)) as i32 - 1
}

fn selected_number(ctx: &Ctx) -> i32 {
    lookup_index(&NUMBER_NAMES, &value_of(ctx, cfg::SLOT_NUMBER)) as i32
}

fn selected_state(ctx: &mut Ctx) -> (&mut FlexiState, usize) {
    let state = flexi_mut(ctx);
    let index = state.selected_slot;
    (state, index)
}

/// Force the resolution of the selected slot (and the setting) to 7 or 14 bit.
fn force_resolution(ctx: &mut Ctx, is_high_res: bool) {
    editor_set(ctx, cfg::SLOT_RESOLUTION, OPTIONS_RESOLUTION[usize::from(is_high_res)]);
    let (state, index) = selected_state(ctx);
    state.slots[index].is_high_res = is_high_res;
}

/// Handle the change of a setting of the editor. Returns false if the setting is not one of the
/// editor.
pub fn handle_setting_change(ctx: &mut Ctx, id: SettingId) -> bool {
    let is_editor_setting = id == cfg::SLOT_SELECTED
        || (cfg::SLOT_TYPE.0..=cfg::SLOT_SEND_VALUE_WHEN_RECEIVED.0).contains(&id.0)
        || cfg::function_setting_category(id).is_some();
    if !is_editor_setting {
        return false;
    }
    if is_editor_update(ctx, id) {
        return true;
    }

    let value = value_of(ctx, id);
    match id {
        cfg::SLOT_SELECTED => select_slot(ctx, &value),

        cfg::SLOT_TYPE => {
            let slot_type = lookup_index(&OPTIONS_TYPE, &value) as i32 - 1;
            let (state, index) = selected_state(ctx);
            state.slots[index].slot_type = slot_type;
            // High resolution is only true for pitchbend as the default
            let number = selected_number(ctx);
            if slot_type != TYPE_PITCH_BEND && (slot_type != TYPE_CC || number >= 32) {
                force_resolution(ctx, false);
            }
            clear_note_map(ctx);
        }

        cfg::SLOT_NUMBER => {
            let number = lookup_index(&NUMBER_NAMES, &value) as i32;
            let (state, index) = selected_state(ctx);
            state.slots[index].number = number;
            // Switch resolution setting to low for CC >= 32
            if selected_slot_type(ctx) == TYPE_CC && number >= 32 {
                force_resolution(ctx, false);
            }
            clear_note_map(ctx);
        }

        cfg::SLOT_MIDI_CHANNEL => {
            let channel = lookup_index(&cfg::controller_channels(), &value) as i32;
            let (state, index) = selected_state(ctx);
            state.slots[index].midi_channel = channel;
            clear_note_map(ctx);
        }

        cfg::SLOT_RESOLUTION => {
            let is_high_res = value == OPTIONS_RESOLUTION[1];
            let (state, index) = selected_state(ctx);
            state.slots[index].is_high_res = is_high_res;
            // High resolution can only be set for CC < 32 and pitchbend (fixed to high res)
            let slot_type = selected_slot_type(ctx);
            if is_high_res {
                let number = selected_number(ctx);
                if (slot_type != TYPE_CC || number >= 32) && slot_type != TYPE_PITCH_BEND {
                    force_resolution(ctx, false);
                }
            } else if slot_type == TYPE_PITCH_BEND {
                force_resolution(ctx, true);
            }
        }

        cfg::SLOT_KNOB_MODE => {
            let (state, index) = selected_state(ctx);
            state.slots[index].knob_mode = KnobMode::lookup_by_label(&value);
            fix_knob_mode(ctx);
        }

        cfg::SLOT_FUNCTION_LAYER => {
            let layer = lookup_index(&cfg::FUNCTION_LAYERS, &value) as i32 - 1;
            let (state, index) = selected_state(ctx);
            state.slots[index].function_layer = layer;
        }

        cfg::SLOT_SEND_VALUE => {
            let (state, index) = selected_state(ctx);
            state.slots[index].send_value = lookup_index(&ON_OFF_OPTIONS, &value) > 0;
        }

        cfg::SLOT_SEND_VALUE_WHEN_RECEIVED => {
            let (state, index) = selected_state(ctx);
            state.slots[index].send_value_when_received = lookup_index(&ON_OFF_OPTIONS, &value) > 0;
        }

        _ => handle_function_change(ctx, &value),
    }
    true
}

/// Handles changing the function selection by the user (former `handleFunctionChange`).
fn handle_function_change(ctx: &mut Ctx, value: &str) {
    let new_command = FlexiCommand::lookup_by_name(value);
    let (state, index) = selected_state(ctx);
    let old_command = state.slots[index].command;
    state.slots[index].command = new_command;

    fix_knob_mode(ctx);
    // Former notifyCommandObserver
    super::setup::update_indication(ctx);

    // Only one category can have a command
    if let Some(old_category) = old_command.category() {
        if Some(old_category) != new_command.category() {
            editor_set(ctx, cfg::function_setting(old_category), FlexiCommand::Off.name());
        }
    }
}

/// Always set the knob mode to absolute for trigger commands (former `fixKnobMode`).
fn fix_knob_mode(ctx: &mut Ctx) {
    let state = flexi(ctx);
    let slot = &state.slots[state.selected_slot];
    if !slot.command.is_trigger() {
        return;
    }
    if !slot.knob_mode.is_absolute() || slot.slot_type == TYPE_MMC {
        editor_set(ctx, cfg::SLOT_KNOB_MODE, KnobMode::Absolute.label());
        let (state, index) = selected_state(ctx);
        state.slots[index].knob_mode = KnobMode::Absolute;
    }
}

/// Select a slot and show its values in the editor settings (former `selectSlot`). The value is
/// the number of the slot (1-300).
pub fn select_slot(ctx: &mut Ctx, value: &str) {
    let Some(index) = value.trim().parse::<usize>().ok().filter(|i| (1..=cfg::NUM_SLOTS).contains(i)).map(|i| i - 1) else {
        return;
    };
    flexi_mut(ctx).selected_slot = index;
    editor_set(ctx, cfg::SLOT_SELECTED, &(index + 1).to_string());
    let slot = flexi(ctx).slots[index].clone();

    let channels = cfg::controller_channels();
    editor_set(ctx, cfg::SLOT_TYPE, OPTIONS_TYPE[(slot.slot_type + 1).clamp(0, 5) as usize]);
    editor_set(ctx, cfg::SLOT_NUMBER, NUMBER_NAMES[slot.number.clamp(0, 127) as usize]);
    editor_set(ctx, cfg::SLOT_MIDI_CHANNEL, &channels[slot.midi_channel.clamp(0, 16) as usize]);
    editor_set(ctx, cfg::SLOT_RESOLUTION, OPTIONS_RESOLUTION[usize::from(slot.is_high_res)]);
    editor_set(ctx, cfg::SLOT_KNOB_MODE, slot.knob_mode.label());
    editor_set(ctx, cfg::SLOT_FUNCTION_LAYER, cfg::FUNCTION_LAYERS[(slot.function_layer + 1).clamp(0, 10) as usize]);
    editor_set(ctx, cfg::SLOT_SEND_VALUE, ON_OFF_OPTIONS[usize::from(slot.send_value)]);
    editor_set(ctx, cfg::SLOT_SEND_VALUE_WHEN_RECEIVED, ON_OFF_OPTIONS[usize::from(slot.send_value_when_received)]);

    // Former setCommand
    let category = slot.command.category();
    for c in super::command::CommandCategory::ALL {
        let name = if category == Some(c) { slot.command.name() } else { FlexiCommand::Off.name() };
        editor_set(ctx, cfg::function_setting(c), name);
    }
}

/// Apply the values of the last received MIDI message to the selected slot (the former "Set"
/// button of the MIDI learn section).
pub fn learn_set(ctx: &mut Ctx) {
    let learn = flexi(ctx).learn.clone();
    let Some(type_value) = learn.type_value else {
        return;
    };
    let mut number_value = learn.number_value;
    let mut resolution = learn.resolution;

    ctx.config.set_value(cfg::SLOT_TYPE, &type_value);
    ctx.config.set_value(cfg::SLOT_MIDI_CHANNEL, &learn.midi_channel_value);

    // CC? For 14-bit values only set CCs below 32
    if type_value == OPTIONS_TYPE[1] && resolution {
        let number = lookup_index(&NUMBER_NAMES, &number_value);
        if (32..64).contains(&number) {
            number_value = NUMBER_NAMES[number - 32].to_string();
        } else if number >= 64 {
            resolution = false;
        }
    }

    ctx.config.set_value(cfg::SLOT_RESOLUTION, OPTIONS_RESOLUTION[usize::from(resolution)]);
    ctx.config.set_value(cfg::SLOT_NUMBER, &number_value);
}
