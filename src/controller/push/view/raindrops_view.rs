// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The raindrops sequencer view of the Push (former `RaindropsView`).

use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::view::sequencer::abstract_raindrops_view::RaindropsBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The name of the raindrops view.
pub const NAME_RAINDROPS: &str = "Raindrops";

/// The raindrops view.
pub struct RaindropsView {
    pub base: RaindropsBase,
}

impl Default for RaindropsView {
    fn default() -> Self {
        Self::new()
    }
}

impl RaindropsView {
    pub fn new() -> Self {
        RaindropsView { base: RaindropsBase::new(NAME_RAINDROPS, true) }
    }

    pub fn clip_key(&self) -> ClipKey {
        self.base.seq.clip_key()
    }
}

impl View for RaindropsView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, update_control_surface, button_color_id, on_sequencer_left,
        on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractRaindropsView
    crate::forward_view!(base => on_activate, draw_grid, on_grid_note, update_note_mapping);

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.on_button(ctx, button, event);
    }

    fn sequencer_clip_key(&self) -> Option<ClipKey> {
        Some(self.base.seq.clip_key())
    }

    fn sequencer_edit_note(&mut self, ctx: &mut Ctx, note_position: &crate::daw::clip::NotePosition, add_note: bool) {
        self.base.seq.edit_note(ctx, note_position, add_note);
    }

    crate::impl_transpose_view!();


    crate::impl_as_any!();
}

impl TransposeView for RaindropsView {
    // AbstractRaindropsView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
