// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A view which does nothing (former `DummyView`).

use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::grid::GRID_OFF;
use crate::framework::ids::ButtonId;

/// A view which does nothing (former `DummyView`): it neither draws the grid nor handles its
/// notes, and all its buttons are off. For the pad modes which a device handles itself (e.g. the
/// chord and custom modes of the Launchkey Mk3), so that the view manager knows them.
#[derive(Debug, Clone)]
pub struct DummyView {
    name: String,
}

impl DummyView {
    pub fn new(name: &str) -> Self {
        DummyView { name: name.to_string() }
    }
}

impl View for DummyView {
    fn name(&self) -> &str {
        &self.name
    }

    fn draw_grid(&mut self, _ctx: &mut Ctx) {
        // Intentionally empty
    }

    fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {
        // Intentionally empty
    }

    fn button_color(&self, ctx: &Ctx, _button: ButtonId) -> i32 {
        ctx.colors.color_index(GRID_OFF)
    }

    crate::impl_as_any!();
}
