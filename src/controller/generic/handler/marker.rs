// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for marker commands (former `MarkerHandler`).

use super::{is_button_pressed, unsupported};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(_ctx: &Ctx, _command: FlexiCommand) -> i32 {
    -1
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    if !is_button_pressed(knob_mode, value) {
        return;
    }
    let marker_bank = ctx.model.marker_bank_mut();
    if let Some(index) = command.index_in(FlexiCommand::Marker1LaunchMarker, 8) {
        marker_bank.item_mut(index).launch(true);
        return;
    }
    match command {
        FlexiCommand::MarkerSelectPreviousBank => marker_bank.select_previous_page(),
        FlexiCommand::MarkerSelectNextBank => marker_bank.select_next_page(),
        _ => unsupported(ctx, command),
    }
}
