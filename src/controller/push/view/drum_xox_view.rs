// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The XoX drum sequencer view of the Push (former `DrumXoXView`).

use super::session_view::push_session_colors;
use super::sequencer_view::handle_push_step_combinations;
use crate::daw::clip::NotePosition;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::view::sequencer::abstract_drum_view::{DrumBase, DrumHooks};
use crate::framework::view::sequencer::abstract_drum_xox_view::DrumXoXBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The name of the XoX drum view.
pub const NAME_DRUM_XOX: &str = "Drum XoX";

struct Hooks;

impl DrumHooks for Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        handle_push_step_combinations(&mut base.seq, ctx, note_position, velocity, true) || base.handle_sequencer_area_button_combinations(ctx, note_position, velocity)
    }
}

/// The XoX drum view.
pub struct DrumXoXView {
    pub base: DrumXoXBase,
}

impl DrumXoXView {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = DrumXoXBase::new(NAME_DRUM_XOX, 8);
        let (colors, bird_has_content, bird_selected) = push_session_colors(ctx);
        base.session.clip_colors = colors;
        base.session.bird_color_has_content = bird_has_content;
        base.session.bird_color_selected = bird_selected;
        DrumXoXView { base }
    }

    pub fn clip_key(&self) -> ClipKey {
        self.base.drum.seq.clip_key()
    }
}

impl View for DrumXoXView {
    // AbstractSequencerView
    crate::forward_view!(base.drum.seq => name, button_color_id, on_sequencer_left, on_sequencer_right,
        sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractDrumView
    crate::forward_view!(base.drum => on_deactivate, update_control_surface, on_model_event);
    // AbstractDrumXoXView
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut Hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.drum.seq.is_active() {
            return;
        }
        let num_columns = self.base.drum.num_columns;
        let index = note - ctx.surface.grid_ref().start_note();
        let x = index % num_columns;
        let y = index / num_columns;
        // Sequencer steps
        if y < self.base.num_step_rows {
            ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, index.max(0) as usize));
            let offset_y = ctx.model.scales().drum_offset();
            let step = (self.base.num_step_rows - 1 - y) * num_columns + x;
            let note_position = NotePosition::new(ctx.config.midi_edit_channel(), step, offset_y + self.base.drum.selected_pad);
            self.base.drum.seq.edit_note(ctx, &note_position, false);
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.drum.seq.on_button(ctx, button, event);
    }

    fn session_alternative(&self) -> Option<&dyn crate::daw::clip::SessionAlternative> {
        Some(self)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn crate::daw::clip::SessionAlternative> {
        Some(self)
    }

    fn sequencer_clip_key(&self) -> Option<ClipKey> {
        Some(self.base.drum.seq.clip_key())
    }

    fn sequencer_edit_note(&mut self, ctx: &mut Ctx, note_position: &crate::daw::clip::NotePosition, add_note: bool) {
        self.base.drum.seq.edit_note(ctx, note_position, add_note);
    }

    crate::impl_transpose_view!();


    crate::impl_as_any!();
}

impl crate::daw::clip::SessionAlternative for DrumXoXView {
    fn was_alternate_interaction_used(&self) -> bool {
        self.base.was_alternate_interaction_used()
    }

    fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.base.set_alternate_interaction_used(was_used);
    }
}

impl TransposeView for DrumXoXView {
    // The note mapping of the XoX view after the octave change (Java: the overridden
    // getDrumMatrix is used by updateNoteMapping)
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.drum.on_octave_down(ctx, event);
        self.base.update_note_mapping(ctx);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.drum.on_octave_up(ctx, event);
        self.base.update_note_mapping(ctx);
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.base.drum.reset_octave(ctx);
        self.base.update_note_mapping(ctx);
    }

    // AbstractDrumView
    crate::forward_transpose_view!(base.drum => is_octave_up_button_on, is_octave_down_button_on);
}
