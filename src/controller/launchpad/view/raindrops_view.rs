// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The raindrops sequencer view of the Launchpad (former `RaindropsView`).

use super::sequencer_common::{note_sequencer_scroll_states, resolution_button_color};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::sequencer::abstract_raindrops_view::RaindropsBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The raindrops sequencer view.
pub struct RaindropsView {
    base: RaindropsBase,
}

impl RaindropsView {
    pub fn new(_ctx: &Ctx) -> Self {
        RaindropsView { base: RaindropsBase::new("Raindrops", true) }
    }
}

impl View for RaindropsView {
    fn name(&self) -> &str {
        self.base.seq.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        resolution_button_color(&self.base.seq, ctx, button, 0)
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.base.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.on_button(ctx, button, event);
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        self.base.seq.update_control_surface(ctx);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.base.update_note_mapping(ctx);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(note_sequencer_scroll_states(&self.base.seq, ctx))
    }

    fn on_sequencer_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq.on_left(ctx, event);
    }

    fn on_sequencer_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq.on_right(ctx, event);
    }

    crate::impl_transpose_view!();

    crate::impl_as_any!();
}

impl TransposeView for RaindropsView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_down(ctx, event);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_up(ctx, event);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        self.base.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.base.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.base.reset_octave(ctx);
    }
}
