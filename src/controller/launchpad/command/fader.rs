// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Forwards the value of a fader to the active fader view (former `FaderCommand`). Not bound by
//! the setup (as in Java), the virtual faders are drawn on the pad grid.

use crate::framework::command::ContinuousCommand;
use crate::framework::ctx::Ctx;

/// Sets the value of a fader of the active view, if it is a fader view.
pub struct FaderCommand {
    index: usize,
}

impl FaderCommand {
    pub fn new(index: usize) -> Self {
        FaderCommand { index }
    }
}

impl ContinuousCommand for FaderCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        let Some(view) = ctx.active_view() else {
            return;
        };
        let Ok(view) = view.try_borrow() else {
            return;
        };
        if let Some(fader_view) = view.as_virtual_fader_view() {
            fader_view.on_value_knob(ctx, self.index, value);
        }
    }
}
