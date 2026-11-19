// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Forwards the touch of a knob to the active mode (former `KnobRowTouchModeCommand`).

use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// Calls `Mode::on_knob_touch` of the active mode for the knob with the index.
pub struct KnobRowTouchModeCommand {
    index: usize,
}

impl KnobRowTouchModeCommand {
    pub fn new(index: usize) -> Self {
        KnobRowTouchModeCommand { index }
    }
}

impl TriggerCommand for KnobRowTouchModeCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Long {
            return;
        }
        if let Some(mode) = ctx.active_mode() {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.on_knob_touch(ctx, self.index, event == ButtonEvent::Down);
            }
        }
    }
}
