// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for groove commands (former `GrooveHandler`).

use super::{is_button_pressed, set_or_change, to_midi_value, toggle_parameter, unsupported, value_changers};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::constants::GrooveParameterId;
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let groove = ctx.model.groove();
    match command {
        FlexiCommand::GrooveActive => to_midi_value(groove.parameter(GrooveParameterId::Enabled).value() > 0),
        FlexiCommand::GrooveShuffleAmount => groove.parameter(GrooveParameterId::ShuffleAmount).value(),
        FlexiCommand::GrooveAccentAmount => groove.parameter(GrooveParameterId::AccentAmount).value(),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let changers = value_changers(ctx, knob_mode, value);
    match command {
        FlexiCommand::GrooveActive => {
            if is_button_pressed(knob_mode, value) {
                let upper_bound = ctx.model.value_changer().upper_bound();
                toggle_parameter(upper_bound, ctx.model.groove_mut().parameter_mut(GrooveParameterId::Enabled));
            }
        }
        FlexiCommand::GrooveShuffleAmount => {
            set_or_change(ctx.model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount), &changers, knob_mode, value);
        }
        FlexiCommand::GrooveAccentAmount => {
            set_or_change(ctx.model.groove_mut().parameter_mut(GrooveParameterId::AccentAmount), &changers, knob_mode, value);
        }
        _ => unsupported(ctx, command),
    }
}
