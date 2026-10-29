// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A sequencer which records all notes played in the lower part of the grid into a step (former
//! `AbstractPolySequencerView`).

use std::collections::HashMap;
use std::time::Instant;

use super::abstract_sequencer_view::{selected_track_index, SequencerBase};
use crate::daw::clip::{CursorClip, NotePosition, StepInfo, StepState};
use crate::daw::constants::Resolution;
use crate::daw::events::ModelEvent;
use crate::framework::color_ids::abstract_play_view;
use crate::framework::color_ids::abstract_sequencer_view::*;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::ViewBase;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

pub const GRID_COLUMNS: i32 = 8;
pub const GRID_ROWS: i32 = 8;
pub const NUM_SEQUENCER_LINES: i32 = 4;
/// Former `Views.NAME_POLY_SEQUENCER`.
pub const NAME_POLY_SEQUENCER: &str = "Poly Seq.";

/// The hooks of the poly sequencer, which controller specific views can override.
pub trait PolySequencerHooks {
    /// Former `handleSequencerAreaButtonCombinations`.
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut PolySequencerBase, ctx: &mut Ctx, channel: i32, step: i32) -> bool {
        base.handle_sequencer_area_button_combinations(ctx, channel, step)
    }

    /// Former `handleSequencerArea`.
    fn handle_sequencer_area(&mut self, base: &mut PolySequencerBase, ctx: &mut Ctx, x: i32, y: i32, velocity: i32)
    where
        Self: Sized,
    {
        base.handle_sequencer_area(self, ctx, x, y, velocity);
    }
}

/// The default hooks.
pub struct DefaultPolySequencerHooks;

impl PolySequencerHooks for DefaultPolySequencerHooks {}

/// The state of a poly sequencer view.
pub struct PolySequencerBase {
    pub seq: SequencerBase,
    pub sequencer_steps: i32,
    pub note_memory: HashMap<i32, i32>,
    pub copy_step: i32,
    pub num_columns: i32,
    pub num_rows: i32,
    last_pressed_note_time: Option<Instant>,
}

impl PolySequencerBase {
    /// Constructor for an 8x8 grid with 4 sequencer lines.
    pub fn new(use_daw_colors: bool) -> Self {
        Self::with_size(use_daw_colors, GRID_COLUMNS, GRID_ROWS, NUM_SEQUENCER_LINES)
    }

    pub fn with_size(use_daw_colors: bool, num_columns: i32, num_rows: i32, num_sequencer_rows: i32) -> Self {
        let sequencer_steps = num_sequencer_rows * num_columns;
        let mut seq = SequencerBase::new(NAME_POLY_SEQUENCER, 128, sequencer_steps as usize, num_sequencer_rows, use_daw_colors);
        seq.poly_steps = true;
        PolySequencerBase {
            seq,
            sequencer_steps,
            note_memory: HashMap::new(),
            copy_step: -1,
            num_columns,
            num_rows,
            last_pressed_note_time: None,
        }
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.seq.clip(ctx);
        self.update_note_mapping(ctx);
    }

    /// Handles the observed model events (track selection and played notes).
    pub fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        match event {
            ModelEvent::TrackSelectionChanged { .. } => self.seq.view.key_manager.clear_pressed_keys(),
            ModelEvent::TrackNote { track_index, note, velocity } => {
                let selected = selected_track_index(ctx);
                self.seq.view.key_manager.call(*track_index, *note, *velocity, selected);
            }
            _ => {}
        }
    }

    pub fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.change_octave(ctx, false, event);
    }

    pub fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.change_octave(ctx, true, event);
    }

    fn change_octave(&mut self, ctx: &mut Ctx, increase: bool, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            self.seq.clip(ctx).transpose(if increase { 1 } else { -1 });
            return;
        }
        if ctx.surface.is_select_pressed() {
            self.seq.clip(ctx).transpose(if increase { 12 } else { -12 });
            return;
        }
        let octave = ctx.model.scales().octave() + if increase { 1 } else { -1 };
        self.set_octave(ctx, octave);
    }

    pub fn reset_octave(&mut self, ctx: &mut Ctx) {
        let octave = ctx.model.scales().default_octave();
        self.set_octave(ctx, octave);
    }

    fn set_octave(&mut self, ctx: &mut Ctx, octave: i32) {
        self.seq.view.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().set_octave(octave);
        self.update_note_mapping(ctx);
        self.seq.clear_edit_notes(ctx);
        let text = ctx.model.scales().range_text();
        ctx.notify(&text);
    }

    pub fn is_octave_up_button_on(&self) -> bool {
        true
    }

    pub fn is_octave_down_button_on(&self) -> bool {
        true
    }

    pub fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.seq.is_active() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, index as usize));
        let x = index % self.num_columns;
        let y = index / self.num_columns;
        if y < self.num_rows - self.seq.num_sequencer_rows {
            return;
        }
        self.seq.clear_edit_notes(ctx);
        let step = self.num_columns * (self.num_rows - 1 - y) + x;
        self.seq.edit_notes_of_step(ctx, ctx.config.midi_edit_channel(), step);
    }

    pub fn on_grid_note<H: PolySequencerHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let x = index % self.num_columns;
        let y = index / self.num_columns;
        if y < self.num_rows - self.seq.num_sequencer_rows {
            self.handle_note_area(note, velocity);
            return;
        }
        // Only allow playing the notes if there is no clip
        if self.seq.is_active() {
            hooks.handle_sequencer_area(self, ctx, x, y, velocity);
        }
    }

    fn handle_note_area(&mut self, note: i32, velocity: i32) {
        // No pressed keys? Clear up the note memory for programming the sequencer...
        let now = Instant::now();
        if self.last_pressed_note_time.is_none_or(|t| now.duration_since(t).as_millis() > 500) {
            self.last_pressed_note_time = Some(now);
            self.note_memory.clear();
        }

        // Mark selected notes immediately for better performance
        let mapped_note = self.seq.view.key_manager.map(note);
        if mapped_note == -1 {
            return;
        }
        self.seq.view.key_manager.set_all_keys_pressed(mapped_note, velocity);
        if velocity > 0 {
            self.note_memory.insert(mapped_note, velocity);
        }
    }

    /// The default handling of a pad in the sequencer area.
    pub fn handle_sequencer_area<H: PolySequencerHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, x: i32, y: i32, velocity: i32) {
        if velocity != 0 {
            return;
        }
        let step = self.num_columns * (self.num_rows - 1 - y) + x;
        let channel = ctx.config.midi_edit_channel();
        let mut note_position = NotePosition::new(channel, step, 0);
        if hooks.handle_sequencer_area_button_combinations(self, ctx, channel, step) || self.seq.handle_note_editor(ctx, &note_position, 127) {
            return;
        }

        // Clear all notes on the step, if there is at least one
        let clip = self.seq.clip(ctx);
        if clip.has_column_data(channel, step) {
            clip.clear_column(channel, step);
            return;
        }

        // Create a step and fill it with the memorized notes
        let accent = if ctx.config.is_accent_active() { Some(ctx.config.fixed_accent_value()) } else { None };
        let mut notes_present = false;
        let clip = self.seq.clip(ctx);
        for row in 0..128 {
            note_position.note = row;
            if let Some(velocity) = self.note_memory.get(&row) {
                clip.toggle_step(&note_position, accent.unwrap_or(*velocity));
                notes_present = true;
            }
        }
        if !notes_present {
            ctx.notify("Play some notes in the lower part first.");
        }
    }

    /// The default handling of button combinations in the sequencer area.
    pub fn handle_sequencer_area_button_combinations(&mut self, ctx: &mut Ctx, channel: i32, step: i32) -> bool {
        let mut note_position = NotePosition::new(channel, step, 0);

        // Handle note duplicate function
        if ctx.is_button_combination(ButtonId::Duplicate) {
            let clip = self.seq.clip(ctx);
            if self.step(clip, channel, step).state() == StepState::Start {
                self.copy_step = step;
            } else if self.copy_step >= 0 {
                let mut copy_position = NotePosition::new(channel, self.copy_step, 0);
                for row in 0..128 {
                    copy_position.note = row;
                    let step_info = clip.step(&copy_position).clone();
                    if step_info.velocity() > 0.0 {
                        note_position.note = row;
                        clip.set_step_from(&note_position, &step_info);
                    }
                }
            }
            return true;
        }

        if ctx.is_button_combination(ButtonId::Mute) {
            let clip = self.seq.clip(ctx);
            for note in 0..128 {
                note_position.note = note;
                let step_info = clip.step(&note_position);
                if step_info.state() == StepState::Start {
                    let is_muted = step_info.is_muted();
                    clip.update_step_mute_state(&note_position, !is_muted);
                }
            }
            return true;
        }

        self.change_length_of_notes(ctx, &mut note_position)
    }

    /// Change the length of the notes or create new ones with a length (if a previous step is
    /// long pressed).
    pub fn change_length_of_notes(&mut self, ctx: &mut Ctx, note_position: &mut NotePosition) -> bool {
        let step = note_position.step;

        // Find the step before the current step which is long pressed
        for s in (0..step).rev() {
            note_position.step = s;
            let x = s % self.num_columns;
            let y = self.num_rows - 1 - s / self.num_columns;
            let pad = y * self.num_columns + x;
            let id = ButtonId::get(ButtonId::Pad1, pad as usize);
            if !ctx.surface.is_long_pressed(id) {
                continue;
            }
            ctx.surface.set_trigger_consumed(id);
            let length = step - s + 1;
            let duration = length as f64 * Resolution::value_at(self.seq.resolution_index(ctx));
            let accent = if ctx.config.is_accent_active() { Some(ctx.config.fixed_accent_value()) } else { None };
            let clip = self.seq.clip(ctx);

            // Create new note(s)
            if self.step(clip, note_position.channel, s).state() != StepState::Start {
                for row in 0..128 {
                    note_position.note = row;
                    if let Some(velocity) = self.note_memory.get(&row) {
                        clip.set_step(note_position, accent.unwrap_or(*velocity), duration);
                    }
                }
                return true;
            }

            // Change length of existing notes
            for row in 0..128 {
                note_position.note = row;
                if clip.step(note_position).state() == StepState::Start {
                    clip.update_step_duration(note_position, duration);
                }
            }
            return true;
        }
        false
    }

    pub fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !ctx.model.can_selected_track_hold_notes() {
            ctx.surface.grid().turn_off();
            return;
        }
        self.seq.clip(ctx);
        let (steps, notes) = self.grid_colors(ctx);
        let Ctx { surface, colors, .. } = ctx;
        let grid = surface.grid();
        for (x, y, color) in steps {
            grid.light_ex_id(colors, x, y, color);
        }
        for (note, color) in notes {
            grid.light_id(colors, note, color);
        }
    }

    #[allow(clippy::type_complexity)]
    fn grid_colors(&self, ctx: &Ctx) -> (Vec<(i32, i32, &'static str)>, Vec<(i32, &'static str)>) {
        let mut steps = Vec::new();
        let mut notes = Vec::new();
        let Some(clip) = self.seq.clip_ref(ctx) else {
            return (steps, notes);
        };
        let is_active = self.seq.is_active();
        let step = clip.current_step();
        let channel = ctx.config.midi_edit_channel();

        // Paint the sequencer steps
        let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % self.sequencer_steps } else { -1 };
        let edit_notes = self.seq.edit_notes(ctx);
        for col in 0..self.sequencer_steps {
            let step_info = self.step(clip, channel, col);
            let x = col % self.num_columns;
            let y = col / self.num_columns;
            let color = if is_active { Self::step_color(&step_info, col == hi_step, channel, col, &edit_notes) } else { COLOR_NO_CONTENT };
            steps.push((x, y, color));
        }

        // Paint the play part
        let is_recording = ctx.model.has_recording_state();
        let cursor_track = ctx.model.cursor_track();
        let start_note = ctx.model.scales().start_note();
        for i in start_note..start_note + self.sequencer_steps {
            notes.push((i, self.grid_color(ctx, true, is_recording, cursor_track, i)));
        }
        (steps, notes)
    }

    pub fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let scales = ctx.model.scales();
        let mut matrix = scales.note_matrix();
        for i in scales.start_note() + self.sequencer_steps..scales.end_note() {
            matrix[i as usize] = -1;
        }
        self.seq.view.update_note_mapping(ctx, matrix);
    }

    /// Get the combined step info of all notes of a step.
    pub fn step(&self, clip: &CursorClip, channel: i32, col: i32) -> StepInfo {
        let mut result = StepInfo::new();
        let mut is_muted = true;
        let mut note_position = NotePosition::new(channel, col, 0);
        for row in 0..128 {
            note_position.note = row;
            let step_info = clip.step(&note_position);
            let r = step_info.state();
            if r == StepState::Start {
                result.set_state(StepState::Start);
            } else if r == StepState::Continue && result.state() != StepState::Start {
                result.set_state(StepState::Continue);
            }
            if (r == StepState::Start || r == StepState::Continue) && !step_info.is_muted() {
                is_muted = false;
            }
        }
        result.set_muted(is_muted);
        result
    }

    /// Get the positions of all notes which start at the first step.
    pub fn step_note_positions(clip: &CursorClip) -> Vec<NotePosition> {
        let mut note_position = NotePosition::default();
        let mut positions = Vec::new();
        for row in 0..128 {
            note_position.note = row;
            if clip.step(&note_position).state() == StepState::Start {
                positions.push(note_position);
            }
        }
        positions
    }

    /// Get the color of a sequencer step.
    pub fn step_color(step_info: &StepInfo, hilite: bool, channel: i32, step: i32, edit_notes: &[NotePosition]) -> &'static str {
        match step_info.state() {
            // Note starts
            StepState::Start => {
                if hilite {
                    COLOR_STEP_HILITE_CONTENT
                } else if Self::is_chord_edit(channel, step, edit_notes) {
                    COLOR_STEP_SELECTED
                } else if step_info.is_muted() {
                    COLOR_STEP_MUTED
                } else {
                    COLOR_CONTENT
                }
            }
            // Note continues
            StepState::Continue => {
                if hilite {
                    COLOR_STEP_HILITE_CONTENT
                } else if Self::is_chord_edit(channel, step, edit_notes) {
                    COLOR_STEP_SELECTED
                } else if step_info.is_muted() {
                    COLOR_STEP_MUTED_CONT
                } else {
                    COLOR_CONTENT_CONT
                }
            }
            // Empty
            StepState::Off => {
                if hilite {
                    COLOR_STEP_HILITE_NO_CONTENT
                } else {
                    COLOR_NO_CONTENT
                }
            }
        }
    }

    /// Get the color of a pad of the play area.
    pub fn grid_color(&self, ctx: &Ctx, is_keyboard_enabled: bool, is_recording: bool, track: &crate::daw::track::Track, note: i32) -> &'static str {
        if !is_keyboard_enabled {
            return abstract_play_view::COLOR_OFF;
        }
        if self.seq.view.key_manager.is_key_pressed(note) {
            return if is_recording { abstract_play_view::COLOR_RECORD } else { abstract_play_view::COLOR_PLAY };
        }
        let pad_color = self.seq.view.pad_color(ctx, note, if self.seq.use_daw_colors { Some(track) } else { None });
        if ctx.config.is_turn_off_scale_pads() && pad_color == Scales::SCALE_COLOR_NOTE {
            return Scales::SCALE_COLOR_OFF;
        }
        pad_color
    }

    /// Is a note of the step edited?
    pub fn is_chord_edit(channel: i32, step: i32, edit_notes: &[NotePosition]) -> bool {
        edit_notes.iter().any(|n| n.channel == channel && n.step == step)
    }

    pub fn view_base(&self) -> &ViewBase {
        &self.seq.view
    }
}
