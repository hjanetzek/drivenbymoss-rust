// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for effect track commands (former `FxTrackHandler`).
//!
//! All commands work on the effect track bank. Reaper has no effect track bank (the model has
//! none, as `IModel.getEffectTrackBank` of the Java Reaper version returns null), so, as in
//! Java, all commands do nothing and have no value. The Java implementation of the commands is
//! not ported since it is never executed.

use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`: -1 without an effect track bank.
pub fn command_value(_ctx: &Ctx, _command: FlexiCommand) -> i32 {
    -1
}

/// Former `handle`: does nothing without an effect track bank.
pub fn handle(_ctx: &mut Ctx, _command: FlexiCommand, _knob_mode: KnobMode, _value: MidiValue) {}
