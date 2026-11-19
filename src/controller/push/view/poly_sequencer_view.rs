// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The poly sequencer view of the Push (former `PolySequencerView`).

use crate::daw::clip::{NotePosition, StepState};
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::view::sequencer::abstract_poly_sequencer_view::{PolySequencerBase, PolySequencerHooks};
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

struct Hooks;

impl PolySequencerHooks for Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut PolySequencerBase, ctx: &mut Ctx, channel: i32, step: i32) -> bool {
        let is_select_pressed = ctx.surface.is_select_pressed();
        let mut note_position = NotePosition::new(channel, step, 0);

        // Change note repeat setting for step
        if ctx.surface.is_shift_pressed() {
            for row in 0..128 {
                note_position.note = row;
                if base.seq.clip(ctx).step(&note_position).state() == StepState::Start {
                    base.seq.handle_sequencer_area_repeat_operator(ctx, &note_position, 127, !is_select_pressed);
                }
            }
            return true;
        }

        // Add note to edit notes with SELECT
        if is_select_pressed {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            for row in 0..128 {
                note_position.note = row;
                if base.seq.clip(ctx).step(&note_position).state() == StepState::Start {
                    base.seq.edit_note(ctx, &note_position, true);
                }
            }
            return true;
        }

        base.handle_sequencer_area_button_combinations(ctx, channel, step)
    }
}

/// The poly sequencer view.
pub struct PolySequencerView {
    pub base: PolySequencerBase,
}

impl PolySequencerView {
    pub fn new(use_track_color: bool) -> Self {
        PolySequencerView { base: PolySequencerBase::new(use_track_color) }
    }

    pub fn clip_key(&self) -> ClipKey {
        self.base.seq.clip_key()
    }
}

impl View for PolySequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, button_color_id, update_control_surface, on_sequencer_left,
        on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractPolySequencerView
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping, on_model_event, on_grid_note_long_press);

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut Hooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.seq.on_button(ctx, button, event);
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

impl TransposeView for PolySequencerView {
    // AbstractPolySequencerView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
