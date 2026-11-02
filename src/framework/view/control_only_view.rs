// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A view for controllers without a pad grid (former `ControlOnlyView`): it only updates the
//! display of the active mode.

use crate::framework::ctx::Ctx;
use crate::framework::feature::View;

/// A view which does not use a grid (former `ControlOnlyView`).
#[derive(Debug, Default)]
pub struct ControlOnlyView;

impl View for ControlOnlyView {
    fn name(&self) -> &str {
        "Control"
    }

    fn draw_grid(&mut self, _ctx: &mut Ctx) {
        // Intentionally empty
    }

    fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {
        // Intentionally empty
    }

    crate::impl_as_any!();
}
