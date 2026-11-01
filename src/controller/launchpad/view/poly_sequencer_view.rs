// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The poly sequencer view of the Launchpad (former `PolySequencerView`).

use super::sequencer_common::{note_sequencer_scroll_states, up_down_combination};
use crate::daw::clip::{NotePosition, StepState};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::sequencer::abstract_poly_sequencer_view::{PolySequencerBase, PolySequencerHooks};
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The Launchpad specific behaviour of the poly sequencer.
#[derive(Default)]
struct Hooks {
    note_edit_position: Option<NotePosition>,
}

impl PolySequencerHooks for Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut PolySequencerBase, ctx: &mut Ctx, channel: i32, step: i32) -> bool {
        if let Some(is_up) = up_down_combination(ctx) {
            let mut note_position = NotePosition::new(channel, step, 0);
            for row in 0..128 {
                note_position.note = row;
                if base.seq.clip(ctx).step(&note_position).state() != StepState::Off {
                    base.seq.handle_sequencer_area_repeat_operator(ctx, &note_position, 127, is_up);
                }
            }
            return true;
        }
        base.handle_sequencer_area_button_combinations(ctx, channel, step)
    }

    fn handle_sequencer_area(&mut self, base: &mut PolySequencerBase, ctx: &mut Ctx, x: i32, y: i32, velocity: i32) {
        // Toggle the note on up, so we can intercept the long presses
        if velocity != 0 {
            self.note_edit_position = None;
            return;
        }

        // Note: If the length of the note was changed this method will not be called since
        // button up was consumed! Therefore, always call edit note
        if let Some(mut position) = self.note_edit_position {
            base.seq.clear_edit_notes(ctx);
            for row in 0..128 {
                position.note = row;
                if base.seq.clip(ctx).step(&position).state() == StepState::Start {
                    base.seq.edit_note(ctx, &position, true);
                }
            }
            self.note_edit_position = Some(position);
            return;
        }

        base.handle_sequencer_area(self, ctx, x, y, velocity);
    }
}

/// The poly sequencer view.
pub struct PolySequencerView {
    base: PolySequencerBase,
    hooks: Hooks,
}

impl PolySequencerView {
    pub fn new(_ctx: &Ctx, use_track_color: bool) -> Self {
        PolySequencerView { base: PolySequencerBase::new(use_track_color), hooks: Hooks::default() }
    }
}

impl View for PolySequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, button_color_id, update_control_surface,
        on_sequencer_left, on_sequencer_right);
    // AbstractPolySequencerView
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping, on_model_event);

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut self.hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.seq.is_active() {
            return;
        }
        let index = note - 36;
        let x = index % self.base.num_columns;
        let y = index / self.base.num_columns;
        if y < self.base.num_rows - self.base.seq.num_sequencer_rows {
            return;
        }
        let step = self.base.num_columns * (self.base.num_rows - 1 - y) + x;
        self.hooks.note_edit_position = Some(NotePosition::new(ctx.config.midi_edit_channel(), step, 0));
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.seq.on_button(ctx, button, event);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(note_sequencer_scroll_states(&self.base.seq, ctx))
    }

    crate::impl_transpose_view!();

    crate::impl_as_any!();
}

impl TransposeView for PolySequencerView {
    // AbstractPolySequencerView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
