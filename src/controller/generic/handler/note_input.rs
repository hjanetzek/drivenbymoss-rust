// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler (former `NoteInputHandler`). STUB

use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(_ctx: &Ctx, _command: FlexiCommand) -> i32 {
    -1
}

/// Former `handle`.
pub fn handle(_ctx: &mut Ctx, _command: FlexiCommand, _knob_mode: KnobMode, _value: MidiValue) {}
