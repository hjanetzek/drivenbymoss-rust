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
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, update_control_surface, on_sequencer_left,
        on_sequencer_right);
    // AbstractRaindropsView
    crate::forward_view!(base => on_activate, draw_grid, on_grid_note, update_note_mapping);

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        resolution_button_color(&self.base.seq, ctx, button, 0)
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.on_button(ctx, button, event);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(note_sequencer_scroll_states(&self.base.seq, ctx))
    }

    crate::impl_transpose_view!();

    crate::impl_as_any!();
}

impl TransposeView for RaindropsView {
    // AbstractRaindropsView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
