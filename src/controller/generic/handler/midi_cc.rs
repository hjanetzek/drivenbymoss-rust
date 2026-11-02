// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for the MIDI CC commands (former `MidiCCHandler`): the value is sent as a CC with
//! the number of the command to the DAW (on MIDI channel 1).

use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::framework::ctx::Ctx;
use crate::midi::CMD_CC;

/// Former `getCommandValue`.
pub fn command_value(_ctx: &Ctx, _command: FlexiCommand) -> i32 {
    -1
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, _knob_mode: KnobMode, value: MidiValue) {
    let Some(cc) = command.index_in(FlexiCommand::MidiCc0, 128) else {
        super::unsupported(ctx, command);
        return;
    };
    let mut val = value.value();
    if value.is_high_res() {
        val %= 128;
    }
    ctx.surface.input.send_raw_midi_event(CMD_CC, cc as i32, val);
}
