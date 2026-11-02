// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for global commands (former `GlobalHandler`).

use super::{is_button_pressed, to_midi_value, unsupported};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::controller::generic::surface::{is_shift_pressed, set_shift_pressed, update_relative_knob_sensitivity};
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    match command {
        FlexiCommand::GlobalToggleAudioEngine => to_midi_value(ctx.model.application().is_engine_active()),
        FlexiCommand::GlobalShiftButton => to_midi_value(is_shift_pressed(ctx)),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);
    match command {
        FlexiCommand::GlobalUndo => {
            if is_button_pressed {
                ctx.model.application_mut().undo();
            }
        }
        FlexiCommand::GlobalRedo => {
            if is_button_pressed {
                ctx.model.application_mut().redo();
            }
        }
        FlexiCommand::GlobalPreviousProject => {
            if is_button_pressed {
                ctx.model.project_mut().previous();
            }
        }
        FlexiCommand::GlobalNextProject => {
            if is_button_pressed {
                ctx.model.project_mut().next();
            }
        }
        FlexiCommand::GlobalToggleAudioEngine => {
            if is_button_pressed {
                ctx.model.application_mut().toggle_engine_active();
            }
        }
        FlexiCommand::GlobalShiftButton => {
            set_shift_pressed(ctx, is_button_pressed);
            // Former ToggleKnobSpeedCommand (executed with DOWN or UP, never LONG)
            let is_slow = is_shift_pressed(ctx);
            ctx.surface.set_knob_sensitivity_is_slow(is_slow);
            update_relative_knob_sensitivity(ctx);
        }
        _ => unsupported(ctx, command),
    }
}
