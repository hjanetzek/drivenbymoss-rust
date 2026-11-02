// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for the mode commands (former `ModesHandler`): the knobs and buttons control the
//! active mode.

use super::{is_absolute, is_button_pressed, relative_value_changer, unsupported};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::setup::MODE_IDS;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::controller::generic::surface::activate_mode;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::Modes;
use crate::framework::mode::device::ParameterMode;
use crate::framework::mode::track::{TrackMode, TrackPanMode, TrackSendMode, TrackVolumeMode};

/// The delay before the browser mode is activated after the preset browser was started.
const BROWSER_MODE_DELAY_MS: u64 = 500;

/// The mode select commands and their modes.
const SELECT_MODE_COMMANDS: [(FlexiCommand, Modes); 12] = [
    (FlexiCommand::ModesSelectModeTrack, Modes::Track),
    (FlexiCommand::ModesSelectModeVolume, Modes::Volume),
    (FlexiCommand::ModesSelectModePan, Modes::Pan),
    (FlexiCommand::ModesSelectModeSend1, Modes::Send1),
    (FlexiCommand::ModesSelectModeSend2, Modes::Send2),
    (FlexiCommand::ModesSelectModeSend3, Modes::Send3),
    (FlexiCommand::ModesSelectModeSend4, Modes::Send4),
    (FlexiCommand::ModesSelectModeSend5, Modes::Send5),
    (FlexiCommand::ModesSelectModeSend6, Modes::Send6),
    (FlexiCommand::ModesSelectModeSend7, Modes::Send7),
    (FlexiCommand::ModesSelectModeSend8, Modes::Send8),
    (FlexiCommand::ModesSelectModeDevice, Modes::DeviceParams),
];

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let Some(index) = command.index_in(FlexiCommand::ModesKnob1, 8) else {
        return -1;
    };
    ctx.active_mode().and_then(|mode| mode.try_borrow().ok().map(|mode| mode.knob_value(ctx, index))).unwrap_or(-1)
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let Some(mode) = ctx.active_mode() else {
        return;
    };
    let is_button_pressed = is_button_pressed(knob_mode, value);

    if let Some(index) = command.index_in(FlexiCommand::ModesKnob1, 8) {
        change_mode_value(ctx, knob_mode, index, value);
        return;
    }
    if !is_button_pressed {
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::ModesButton1, 8) {
        if let Ok(mut mode) = mode.try_borrow_mut() {
            mode.select_item(ctx, index);
        }
        mv_helper::notify_selected_item_of_active_mode(ctx);
        return;
    }
    if let Some((_, mode_id)) = SELECT_MODE_COMMANDS.iter().find(|(c, _)| *c == command) {
        activate_mode(ctx, *mode_id);
        return;
    }

    match command {
        FlexiCommand::ModesNextItem | FlexiCommand::ModesPrevItem | FlexiCommand::ModesNextPage | FlexiCommand::ModesPrevPage => {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                match command {
                    FlexiCommand::ModesNextItem => mode.select_next_item(ctx),
                    FlexiCommand::ModesPrevItem => mode.select_previous_item(ctx),
                    FlexiCommand::ModesNextPage => mode.select_next_item_page(ctx),
                    _ => mode.select_previous_item_page(ctx),
                }
            }
            mv_helper::notify_selected_item_of_active_mode(ctx);
        }
        FlexiCommand::ModesSelectModeNext => select_next_mode(ctx, true),
        FlexiCommand::ModesSelectModePrev => select_next_mode(ctx, false),
        FlexiCommand::ModesBrowsePresets => {
            ctx.model.browse_cursor_device_presets();
            ctx.schedule(BROWSER_MODE_DELAY_MS, |ctx| activate_mode(ctx, Modes::Browser));
        }
        _ => unsupported(ctx, command),
    }
}

/// Set the absolute state of a mode (former `AbstractParameterMode.setAbsolute`).
fn set_absolute(mode: &mut dyn Mode, is_absolute: bool) {
    let mode = mode.as_any_mut();
    if let Some(mode) = mode.downcast_mut::<TrackMode>() {
        mode.set_absolute(is_absolute);
    } else if let Some(mode) = mode.downcast_mut::<TrackVolumeMode>() {
        mode.set_absolute(is_absolute);
    } else if let Some(mode) = mode.downcast_mut::<TrackPanMode>() {
        mode.set_absolute(is_absolute);
    } else if let Some(mode) = mode.downcast_mut::<TrackSendMode>() {
        mode.set_absolute(is_absolute);
    } else if let Some(mode) = mode.downcast_mut::<ParameterMode>() {
        mode.base.is_absolute = is_absolute;
    }
}

/// Former `changeModeValue`.
fn change_mode_value(ctx: &mut Ctx, knob_mode: KnobMode, knob_index: usize, value: MidiValue) {
    let Some(mode) = ctx.active_mode() else {
        return;
    };
    let Ok(mut mode) = mode.try_borrow_mut() else {
        return;
    };
    let absolute = is_absolute(knob_mode);
    set_absolute(&mut *mode, absolute);
    if absolute {
        mode.on_knob_value(ctx, knob_index, value.upscaled());
    } else {
        // Re-encode the selected relative type to the default relative type
        let decoded = relative_value_changer(ctx, knob_mode).decode(value.value());
        let control = ctx.model.value_changer().encode(decoded);
        mode.on_knob_value(ctx, knob_index, control);
    }
}

/// Select the next or previous mode; a send mode is skipped if its send does not exist (former
/// `selectNextMode` and `selectPreviousMode`).
fn select_next_mode(ctx: &mut Ctx, is_next: bool) {
    let count = MODE_IDS.len() as i32;
    let active = ctx.modes.active_id();
    let mut index = active.and_then(|id| MODE_IDS.iter().position(|m| *m == id)).map_or(-1, |i| i as i32);
    let mut new_mode;
    loop {
        index += if is_next { 1 } else { -1 };
        if index < 0 || index >= count {
            index = if is_next { 0 } else { count - 1 };
        }
        new_mode = MODE_IDS[index as usize];
        let send = MODE_IDS[3..11].iter().position(|m| *m == new_mode);
        match send {
            Some(send_index) if !ctx.model.track_bank().can_edit_send(send_index) => continue,
            _ => break,
        }
    }
    activate_mode(ctx, new_mode);
}
