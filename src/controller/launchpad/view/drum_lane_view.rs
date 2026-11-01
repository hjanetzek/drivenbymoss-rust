// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum sequencer views with 4 and 8 lanes of the Launchpad (former `Drum4View` and
//! `Drum8View`, which are identical except for the number of lanes).

use super::sequencer_common::{drum_scroll_states, handle_repeat_combination};
use crate::daw::clip::NotePosition;
use crate::daw::events::ModelEvent;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::sequencer::abstract_drum_lane_view::{DrumLaneBase, DrumLaneHooks};
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The Launchpad specific behaviour of the drum lane sequencers.
#[derive(Default)]
struct Hooks {
    note_edit_position: Option<NotePosition>,
}

impl DrumLaneHooks for Hooks {
    fn handle_sequencer_area(&mut self, base: &mut DrumLaneBase, ctx: &mut Ctx, velocity: i32, down_velocity: i32, note_position: &NotePosition) {
        // Toggle the note on up, so we can intercept the long presses
        if velocity != 0 {
            self.note_edit_position = None;
            return;
        }
        // Note: If the length of the note was changed this method will not be called since
        // button up was consumed! Therefore, always call edit note
        match self.note_edit_position {
            Some(position) => base.drum.seq.edit_note(ctx, &position, false),
            None => base.handle_sequencer_area(ctx, velocity, down_velocity, note_position),
        }
    }

    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumLaneBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32, accent_velocity: i32) -> bool {
        handle_repeat_combination(&base.drum.seq, ctx, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity, accent_velocity)
    }
}

/// The drum sequencer view with 4 lanes.
pub type Drum4View = DrumLaneView;
/// The drum sequencer view with 8 lanes.
pub type Drum8View = DrumLaneView;

/// A drum sequencer view with several lanes (sounds).
pub struct DrumLaneView {
    base: DrumLaneBase,
    hooks: Hooks,
}

impl DrumLaneView {
    /// Create the Drum 4 view (former `Drum4View`).
    pub fn new_drum4(_ctx: &Ctx) -> Self {
        DrumLaneView { base: DrumLaneBase::drum4(true), hooks: Hooks::default() }
    }

    /// Create the Drum 8 view (former `Drum8View`).
    pub fn new_drum8(_ctx: &Ctx) -> Self {
        DrumLaneView { base: DrumLaneBase::drum8(true), hooks: Hooks::default() }
    }

    /// The number of lanes.
    pub fn lanes(&self) -> i32 {
        self.base.lanes
    }
}

impl View for DrumLaneView {
    fn name(&self) -> &str {
        self.base.seq().name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        self.base.seq().button_color_id(ctx, button)
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.base.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut self.hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.seq().is_active() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let (sound, step) = self.base.sound_and_step(ctx, index);
        // Remember the long pressed note to use it either for editing or for changing the
        // length of the note on pad release
        self.hooks.note_edit_position = Some(NotePosition::new(ctx.config.midi_edit_channel(), step, sound));
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.seq_mut().on_button(ctx, button, event);
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        self.base.update_control_surface(ctx);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.base.update_note_mapping(ctx);
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.base.on_model_event(ctx, event);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(drum_scroll_states(self.base.seq(), ctx))
    }

    fn on_sequencer_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq_mut().on_left(ctx, event);
    }

    fn on_sequencer_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq_mut().on_right(ctx, event);
    }

    crate::impl_transpose_view!();

    crate::impl_as_any!();
}

impl TransposeView for DrumLaneView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_down(ctx, event);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_up(ctx, event);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        self.base.drum.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.base.drum.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.base.drum.reset_octave(ctx);
    }
}
