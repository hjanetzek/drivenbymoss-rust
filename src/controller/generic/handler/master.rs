// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for master track commands (former `MasterHandler`).

use super::{is_button_pressed, set_or_change, to_midi_value, unsupported, value_changers};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let master = ctx.model.master_track();
    match command {
        FlexiCommand::MasterSetVolume => master.volume(),
        FlexiCommand::MasterSetPanning => master.pan(),
        FlexiCommand::MasterToggleMute | FlexiCommand::MasterSetMute => to_midi_value(master.is_mute()),
        FlexiCommand::MasterToggleSolo | FlexiCommand::MasterSetSolo => to_midi_value(master.is_solo()),
        FlexiCommand::MasterToggleArm | FlexiCommand::MasterSetArm => to_midi_value(master.is_rec_arm()),
        FlexiCommand::MasterCrossfader => ctx.model.crossfade(),
        FlexiCommand::MasterSelect => to_midi_value(master.is_selected()),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);
    let changers = value_changers(ctx, knob_mode, value);
    match command {
        FlexiCommand::MasterSetVolume => set_or_change(ctx.model.master_track_mut().volume_parameter_mut(), &changers, knob_mode, value),
        FlexiCommand::MasterSetPanning => set_or_change(ctx.model.master_track_mut().pan_parameter_mut(), &changers, knob_mode, value),
        FlexiCommand::MasterToggleMute => {
            if is_button_pressed {
                ctx.model.master_track_mut().toggle_mute();
            }
        }
        FlexiCommand::MasterSetMute => {
            if is_button_pressed {
                ctx.model.master_track_mut().set_mute(value.is_positive());
            }
        }
        FlexiCommand::MasterToggleSolo => {
            if is_button_pressed {
                ctx.model.master_track_mut().toggle_solo();
            }
        }
        FlexiCommand::MasterSetSolo => {
            if is_button_pressed {
                ctx.model.master_track_mut().set_solo(value.is_positive());
            }
        }
        FlexiCommand::MasterToggleArm => {
            if is_button_pressed {
                ctx.model.master_track_mut().toggle_rec_arm();
            }
        }
        FlexiCommand::MasterSetArm => {
            if is_button_pressed {
                ctx.model.master_track_mut().set_rec_arm(value.is_positive());
            }
        }
        FlexiCommand::MasterCrossfader => {
            if let Some(parameter) = ctx.model.crossfade_parameter_mut() {
                set_or_change(parameter, &changers, knob_mode, value);
            }
        }
        FlexiCommand::MasterSelect => {
            if is_button_pressed {
                ctx.model.master_track_mut().select();
                mv_helper::notify_selected_track(ctx);
            }
        }
        _ => unsupported(ctx, command),
    }
}
