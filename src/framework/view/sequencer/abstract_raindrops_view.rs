// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The raindrops sequencer: each column is a note, the row selects the distance of the repeated
//! notes (former `AbstractRaindropsView`).

use super::abstract_sequencer_view::SequencerBase;
use crate::daw::clip::{NotePosition, StepState};
use crate::daw::constants::Resolution;
use crate::framework::color_ids::abstract_sequencer_view::*;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

pub const NUM_DISPLAY_COLS: i32 = 8;
pub const NUM_OCTAVE: i32 = 12;
pub const START_KEY: i32 = 36;
/// 32 = biggest number of measures in Fixed Length.
pub const MAX_STEPS: i32 = 1536; // floor (32 * 4 / Resolution.RES_1_32T.getValue ())

/// The state of a raindrops sequencer view.
pub struct RaindropsBase {
    pub seq: SequencerBase,
    pub num_display_cols: i32,
    pub num_display_rows: i32,
    pub ongoing_resolution_change: bool,
    pub offset_y: i32,
}

impl RaindropsBase {
    pub fn new(name: &str, use_daw_colors: bool) -> Self {
        Self::with_columns(name, use_daw_colors, NUM_DISPLAY_COLS)
    }

    pub fn with_columns(name: &str, use_daw_colors: bool, num_display_cols: i32) -> Self {
        RaindropsBase {
            seq: SequencerBase::new(name, 128, MAX_STEPS as usize, 128, use_daw_colors),
            num_display_cols,
            num_display_rows: 8,
            ongoing_resolution_change: false,
            offset_y: START_KEY,
        }
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.seq.clip(ctx);
        self.update_note_mapping(ctx);
    }

    pub fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.seq.view.clear_note_mapping(ctx);
        self.update_scale(ctx);
    }

    pub fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !self.seq.is_active() || velocity == 0 {
            return;
        }
        let grid = ctx.surface.grid_ref();
        let index = note - grid.start_note();
        let num_columns = grid.cols();
        let x = index % num_columns;
        let y = index / num_columns;
        let step_size = if y == 0 { 1 } else { 2 * y };

        let resolution_length = Resolution::value_at(self.seq.resolution_index(ctx));
        let loop_length = self.seq.clip(ctx).loop_length();
        let length = (loop_length / resolution_length).floor() as i32;
        let row = self.seq.view.key_manager.map(x);
        let distance = self.note_distance(ctx, row, length);
        let edit_midi_channel = ctx.config.midi_edit_channel();
        let vel = if ctx.config.is_accent_active() { ctx.config.fixed_accent_value() } else { velocity };
        let clip = self.seq.clip(ctx);
        clip.clear_row(edit_midi_channel, row);
        if distance == -1 || distance != (if y == 0 { 1 } else { y * 2 }) {
            let offset = clip.current_step() % step_size;
            if offset < 0 {
                return;
            }
            let mut note_position = NotePosition::new(edit_midi_channel, 0, 0);
            let mut i = offset;
            while i < length {
                // Only support 32 measures at 1/32t
                if i < MAX_STEPS {
                    note_position.step = i;
                    note_position.note = row;
                    clip.set_step(&note_position, vel, resolution_length);
                }
                i += step_size;
            }
        }
    }

    pub fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !self.seq.is_active() {
            ctx.surface.grid().turn_off();
            return;
        }
        if self.ongoing_resolution_change {
            return;
        }
        self.seq.clip(ctx);
        let colors = self.grid_colors(ctx);
        let Ctx { surface, colors: color_manager, .. } = ctx;
        let grid = surface.grid();
        for (x, y, color) in colors {
            grid.light_ex_id(color_manager, x, y, color);
        }
    }

    fn grid_colors(&self, ctx: &Ctx) -> Vec<(i32, i32, &'static str)> {
        let mut result = Vec::new();
        let Some(clip) = self.seq.clip_ref(ctx) else {
            return result;
        };
        let cursor_track = if self.seq.use_daw_colors { Some(ctx.model.cursor_track()) } else { None };
        let length = (clip.loop_length() / Resolution::value_at(self.seq.resolution_index(ctx))).floor() as i32;
        let step = clip.current_step();
        for x in 0..self.num_display_cols {
            let mapped_key = self.seq.view.key_manager.map(x);
            if mapped_key == -1 {
                continue;
            }
            let left = self.note_distance_to_the_left(ctx, mapped_key, step, length);
            let right = self.note_distance_to_the_right(ctx, mapped_key, step, length);
            let is_on = left >= 0 && right >= 0;
            let sum = left + right;
            let distance = if sum == 0 { 0 } else { (sum + 1) / 2 };

            for y in 0..self.num_display_rows {
                let mut color_id = if y == 0 { self.seq.view.pad_color(ctx, x, cursor_track) } else { COLOR_NO_CONTENT };
                if is_on {
                    if y == distance {
                        color_id = COLOR_CONTENT;
                    }
                    if left <= distance && y == left || left > distance && y == sum - left {
                        color_id = COLOR_STEP_HILITE_NO_CONTENT;
                    }
                } else if ctx.config.is_turn_off_scale_pads() && color_id == Scales::SCALE_COLOR_NOTE {
                    color_id = Scales::SCALE_COLOR_OFF;
                }
                result.push((x, self.num_display_rows - 1 - y, color_id));
            }
        }
        result
    }

    pub fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent) {
        self.ongoing_resolution_change = true;
        self.seq.on_button(ctx, button, event);
        self.ongoing_resolution_change = false;
    }

    pub fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down || !self.seq.is_active() {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            self.seq.clip(ctx).transpose(-1);
            return;
        }
        if ctx.surface.is_select_pressed() {
            self.seq.clip(ctx).transpose(-12);
            return;
        }
        self.offset_y = (self.offset_y - NUM_OCTAVE).max(0);
        self.update_scale(ctx);
        self.notify_range(ctx);
    }

    pub fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down || !self.seq.is_active() {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            self.seq.clip(ctx).transpose(1);
            return;
        }
        if ctx.surface.is_select_pressed() {
            self.seq.clip(ctx).transpose(12);
            return;
        }
        let num_rows = self.seq.clip(ctx).num_rows() as i32;
        self.offset_y = (num_rows - NUM_OCTAVE).min(self.offset_y + NUM_OCTAVE);
        self.update_scale(ctx);
        self.notify_range(ctx);
    }

    fn notify_range(&self, ctx: &mut Ctx) {
        let km = &self.seq.view.key_manager;
        let text = Scales::sequencer_range_text(km.map(0), km.map(7));
        ctx.schedule(10, move |ctx| ctx.notify(&text));
    }

    /// Currently, not used.
    pub fn reset_octave(&mut self, _ctx: &mut Ctx) {}

    pub fn is_octave_up_button_on(&self) -> bool {
        self.seq.is_active()
    }

    pub fn is_octave_down_button_on(&self) -> bool {
        self.seq.is_active()
    }

    /// The distance between the first two notes of the row or -1.
    pub fn note_distance(&self, ctx: &mut Ctx, row: i32, length: i32) -> i32 {
        if row < 0 {
            return -1;
        }
        let channel = ctx.config.midi_edit_channel();
        let clip = self.seq.clip(ctx);
        let mut note_position = NotePosition::new(channel, 0, row);
        let mut step = 0;
        while step < length {
            note_position.step = step;
            if clip.step(&note_position).state() != StepState::Off {
                break;
            }
            step += 1;
        }
        if step >= length {
            return -1;
        }
        for step2 in step + 1..length {
            note_position.step = step2;
            if clip.step(&note_position).state() != StepState::Off {
                return step2 - step;
            }
        }
        -1
    }

    /// The distance to the next note to the right (wrapping around) or -1.
    pub fn note_distance_to_the_right(&self, ctx: &Ctx, row: i32, start: i32, length: i32) -> i32 {
        if row < 0 || start < 0 || start >= length {
            return -1;
        }
        let Some(clip) = self.seq.clip_ref(ctx) else {
            return -1;
        };
        let mut step = start;
        let mut counter = 0;
        let mut note_position = NotePosition::new(ctx.config.midi_edit_channel(), 0, row);
        loop {
            note_position.step = step;
            if clip.step(&note_position).state() != StepState::Off {
                return counter;
            }
            step += 1;
            counter += 1;
            if step >= length {
                step = 0;
            }
            if step == start {
                return -1;
            }
        }
    }

    /// The distance to the next note to the left (wrapping around) or -1.
    pub fn note_distance_to_the_left(&self, ctx: &Ctx, row: i32, start: i32, length: i32) -> i32 {
        if row < 0 || start < 0 || start >= length {
            return -1;
        }
        let Some(clip) = self.seq.clip_ref(ctx) else {
            return -1;
        };
        let s = if start == 0 { length - 1 } else { start - 1 };
        let mut step = s;
        let mut counter = 0;
        let mut note_position = NotePosition::new(ctx.config.midi_edit_channel(), 0, row);
        loop {
            note_position.step = step;
            if clip.step(&note_position).state() != StepState::Off {
                return counter;
            }
            step -= 1;
            counter += 1;
            if step < 0 {
                step = length - 1;
            }
            if step == s {
                return -1;
            }
        }
    }

    pub fn update_scale(&mut self, ctx: &mut Ctx) {
        if ctx.model.can_selected_track_hold_notes() {
            let matrix = ctx.model.scales().sequencer_matrix(self.num_display_cols as usize, self.offset_y);
            self.seq.set_sequencer_matrix(&matrix);
        } else {
            self.seq.view.clear_note_mapping(ctx);
        }
    }
}
