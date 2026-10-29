// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Callback of the 8 virtual faders of the Launchpad surface (former `VirtualFaderViewCallback`).

use crate::framework::ctx::Ctx;
use crate::framework::virtual_fader::VirtualFaderCallback;

/// Gets/sets the value of a virtual fader from/to the active view, if it is a fader view (see
/// [`crate::framework::feature::View::as_virtual_fader_view`]).
pub struct VirtualFaderViewCallback {
    index: usize,
}

impl VirtualFaderViewCallback {
    pub fn new(index: usize) -> Self {
        VirtualFaderViewCallback { index }
    }
}

impl VirtualFaderCallback for VirtualFaderViewCallback {
    fn value(&self, ctx: &Ctx) -> i32 {
        let Some(view) = ctx.active_view() else {
            return 0;
        };
        let Ok(view) = view.try_borrow() else {
            return 0;
        };
        view.as_virtual_fader_view().map(|v| v.fader_value(ctx, self.index)).unwrap_or(0)
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
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
