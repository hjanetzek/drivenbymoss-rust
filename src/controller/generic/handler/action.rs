// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for the assignable action commands (former `ActionHandler`).

use super::is_button_pressed;
use crate::config::generic_flexi::assignable_action;
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(_ctx: &Ctx, _command: FlexiCommand) -> i32 {
    -1
}

/// Former `handle`: execute the action of the "Action 1-8" setting.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    if !is_button_pressed(knob_mode, value) {
        return;
    }
    let Some(index) = command.index_in(FlexiCommand::Action1, 8) else {
        return;
    };
    let action_id = assignable_action(&ctx.config, index);
    if !action_id.trim().is_empty() {
        ctx.model.application_mut().invoke_action(&action_id);
    }
}
