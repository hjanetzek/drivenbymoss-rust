// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The sequencer view of the Launchpad (former `SequencerView`).

use super::sequencer_common::{handle_repeat_combination, note_sequencer_scroll_states, resolution_button_color};
use crate::controller::launchpad::colors::LAUNCHPAD_COLOR_BLACK;
use crate::daw::clip::NotePosition;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::sequencer::abstract_note_sequencer_view::{NoteSequencerBase, NoteSequencerHooks};
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The Launchpad specific behaviour of the note sequencer.
#[derive(Default)]
struct Hooks {
    note_edit_position: Option<NotePosition>,
}

impl NoteSequencerHooks for Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32) -> bool {
        handle_repeat_combination(&base.seq, ctx, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity)
    }

    fn handle_sequencer_area(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, index: i32, x: i32, y: i32, velocity: i32) {
        // Toggle the note on up, so we can intercept the long presses
        if velocity != 0 {
            self.note_edit_position = None;
            return;
        }
        // Note: If the length of the note was changed this method will not be called since
        // button up was consumed! Therefore, always call edit note
        match self.note_edit_position {
            Some(position) => base.seq.edit_note(ctx, &position, false),
            None => base.handle_sequencer_area(self, ctx, index, x, y, velocity),
        }
    }
}

/// The sequencer view.
pub struct SequencerView {
    base: NoteSequencerBase,
    hooks: Hooks,
}

impl SequencerView {
    pub fn new(_ctx: &Ctx) -> Self {
        SequencerView { base: NoteSequencerBase::new("Sequencer", true), hooks: Hooks::default() }
    }

    /// The MIDI note which is edited in the given row (0 = bottom) of the sequencer.
    pub fn note_of_row(&self, row: i32) -> i32 {
        self.base.seq.view.key_manager.map(row)
    }
}

impl View for SequencerView {
    fn name(&self) -> &str {
        self.base.seq.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        resolution_button_color(&self.base.seq, ctx, button, LAUNCHPAD_COLOR_BLACK)
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.base.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut self.hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.seq.is_active() {
            return;
        }
        let index = note - 36;
        let y = index / 8;
        if y >= self.base.seq.num_sequencer_rows {
            return;
        }
        // Remember the long pressed note to use it either for editing or for changing the
        // length of the note on pad release
        self.hooks.note_edit_position = Some(NotePosition::new(ctx.config.midi_edit_channel(), index % 8, self.base.seq.view.key_manager.map(y)));
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.seq.on_button(ctx, button, event);
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

impl TransposeView for SequencerView {
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
