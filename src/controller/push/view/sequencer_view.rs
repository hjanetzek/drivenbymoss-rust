// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note sequencer view of the Push (former `SequencerView`).

use crate::daw::clip::NotePosition;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::view::sequencer::abstract_note_sequencer_view::{NoteSequencerBase, NoteSequencerHooks};
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The name of the sequencer view.
pub const NAME_SEQUENCER: &str = "Sequencer";

/// Shift + step: the note repeat of the step; Select + step: add the note to the edited notes.
/// Shared by the drum views (former overrides of `handleSequencerAreaButtonCombinations`).
/// `only_on_press` is true for the views which edit the note only for velocity > 0.
pub fn handle_push_step_combinations(seq: &mut SequencerBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32, only_on_press: bool) -> bool {
    let is_select_pressed = ctx.surface.is_select_pressed();
    if ctx.surface.is_shift_pressed() {
        if velocity > 0 {
            seq.handle_sequencer_area_repeat_operator(ctx, note_position, velocity, !is_select_pressed);
        }
        return true;
    }
    if is_select_pressed {
        ctx.surface.set_trigger_consumed(ButtonId::Select);
        if !only_on_press || velocity > 0 {
            seq.edit_note(ctx, note_position, true);
        }
        return true;
    }
    false
}

struct Hooks;

impl NoteSequencerHooks for Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32) -> bool {
        handle_push_step_combinations(&mut base.seq, ctx, note_position, velocity, false) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity)
    }
}

/// The sequencer view.
pub struct SequencerView {
    pub base: NoteSequencerBase,
}

impl Default for SequencerView {
    fn default() -> Self {
        Self::new()
    }
}

impl SequencerView {
    pub fn new() -> Self {
        SequencerView { base: NoteSequencerBase::new(NAME_SEQUENCER, true) }
    }

    pub fn clip_key(&self) -> ClipKey {
        self.base.seq.clip_key()
    }
}

impl View for SequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, update_control_surface, button_color_id, on_sequencer_left,
        on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractNoteSequencerView
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut Hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.seq.is_active() {
            return;
        }
        let index = note - 36;
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, index.max(0) as usize));
        let y = index / 8;
        if y >= self.base.seq.num_sequencer_rows {
            return;
        }
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), index % 8, self.base.seq.view.key_manager.map(y));
        self.base.seq.edit_note(ctx, &note_position, false);
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

impl TransposeView for SequencerView {
    // AbstractNoteSequencerView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
