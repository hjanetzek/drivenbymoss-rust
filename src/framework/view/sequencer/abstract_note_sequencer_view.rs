// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A sequencer for notes in the scale (former `AbstractNoteSequencerView`).

use super::abstract_sequencer_view::SequencerBase;
use crate::daw::clip::{NotePosition, StepInfo, StepState};
use crate::daw::constants::Resolution;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

const OCTAVE: i32 = 12;

/// The hooks of the note sequencer, which controller specific views can override (former
/// overridable methods). The defaults call the generic implementation.
pub trait NoteSequencerHooks {
    /// Former `handleSequencerAreaButtonCombinations`.
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32) -> bool {
        base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity)
    }

    /// Former `handleSequencerArea`.
    fn handle_sequencer_area(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, index: i32, x: i32, y: i32, velocity: i32)
    where
        Self: Sized,
    {
        base.handle_sequencer_area(self, ctx, index, x, y, velocity);
    }
}

/// The default hooks.
pub struct DefaultNoteSequencerHooks;

impl NoteSequencerHooks for DefaultNoteSequencerHooks {}

/// The state of a note sequencer view.
pub struct NoteSequencerBase {
    pub seq: SequencerBase,
    pub num_display_rows: i32,
    pub num_display_cols: i32,
    pub start_key: i32,
    pub loop_pad_pressed: i32,
    pub offset_y: i32,
    pub copy_note: Option<StepInfo>,
}

impl NoteSequencerBase {
    /// Constructor with 8 columns and 7 sequencer rows.
    pub fn new(name: &str, use_daw_colors: bool) -> Self {
        Self::with_size(name, 8, 7, use_daw_colors)
    }

    /// Constructor.
    pub fn with_size(name: &str, num_display_cols: i32, num_sequencer_rows: i32, use_daw_colors: bool) -> Self {
        let start_key = 36;
        NoteSequencerBase {
            seq: SequencerBase::new(name, 128, num_display_cols as usize, num_sequencer_rows, use_daw_colors),
            num_display_rows: 8,
            num_display_cols,
            start_key,
            loop_pad_pressed: -1,
            offset_y: start_key,
            copy_note: None,
        }
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.seq.clip(ctx);
        // updateScale() followed by super.onActivate() which calls updateNoteMapping()
        self.update_note_mapping(ctx);
    }

    pub fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.seq.view.clear_note_mapping(ctx);
        self.update_scale(ctx);
    }

    pub fn on_grid_note<H: NoteSequencerHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !self.seq.is_active() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let x = index % self.num_display_cols;
        let y = index / self.num_display_cols;
        if y < self.seq.num_sequencer_rows {
            hooks.handle_sequencer_area(self, ctx, index, x, y, velocity);
            return;
        }
        // Clip length/loop area
        self.handle_loop_area(ctx, x, velocity);
    }

    /// The default handling of a pad in the sequencer area.
    pub fn handle_sequencer_area<H: NoteSequencerHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, index: i32, x: i32, y: i32, velocity: i32) {
        // Toggle the note on up, so we can intercept the long presses
        if velocity != 0 {
            return;
        }
        let mapped_y = self.seq.view.key_manager.map(y);
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), x, mapped_y);
        let vel = SequencerBase::pad_velocity(ctx, ButtonId::get(ButtonId::Pad1, index as usize));

        if hooks.handle_sequencer_area_button_combinations(self, ctx, &note_position, y, vel) || self.seq.handle_note_editor(ctx, &note_position, vel) {
            return;
        }
        if mapped_y >= 0 {
            self.seq.clip(ctx).toggle_step(&note_position, vel);
        }
    }

    /// The default handling of button combinations in the sequencer area.
    pub fn handle_sequencer_area_button_combinations(&mut self, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32) -> bool {
        // Handle note duplicate function
        if ctx.is_button_combination(ButtonId::Duplicate) {
            let clip = self.seq.clip(ctx);
            let note_step = clip.step(note_position);
            if note_step.state() == StepState::Start {
                self.copy_note = Some(note_step.clone());
            } else if let Some(copy_note) = &self.copy_note {
                clip.set_step_from(note_position, copy_note);
            }
            return true;
        }

        if ctx.is_button_combination(ButtonId::Mute) {
            let clip = self.seq.clip(ctx);
            let step_info = clip.step(note_position);
            if step_info.state() == StepState::Start {
                let is_muted = step_info.is_muted();
                clip.update_step_mute_state(note_position, !is_muted);
            }
            return true;
        }

        // Change length of a note or create a new one with a length
        let num_steps = self.seq.clip(ctx).num_steps() as i32;
        let offset = row * num_steps;
        let mut np = *note_position;
        let step = np.step;
        let note = np.note;
        for s in 0..step {
            let id = ButtonId::get(ButtonId::Pad1, (offset + s) as usize);
            if ctx.surface.is_long_pressed(id) {
                np.step = s;
                ctx.surface.set_trigger_consumed(id);
                let length = step - s + 1;
                let duration = length as f64 * Resolution::value_at(self.seq.resolution_index(ctx));
                let clip = self.seq.clip(ctx);
                let state = if note < 0 { StepState::Off } else { clip.step(&np).state() };
                if state == StepState::Start {
                    clip.update_step_duration(&np, duration);
                } else {
                    clip.set_step(&np, velocity, duration);
                }
                return true;
            }
        }
        false
    }

    /// Handle the pads of the clip length/loop area.
    pub fn handle_loop_area(&mut self, ctx: &mut Ctx, pad: i32, velocity: i32) {
        // Button pressed?
        if velocity > 0 {
            // Not yet a button pressed, store it
            if self.loop_pad_pressed == -1 {
                self.loop_pad_pressed = pad;
            }
            return;
        }
        if self.loop_pad_pressed == -1 {
            return;
        }

        let length_of_one_pad = self.seq.length_of_one_page(ctx, self.num_display_cols) as f64;
        let clip = self.seq.clip(ctx);
        if pad == self.loop_pad_pressed && pad != clip.edit_page() {
            // Only single pad pressed -> page selection
            clip.scroll_to_page(pad);
        } else {
            // Set a new loop between the 2 selected pads
            let start = self.loop_pad_pressed.min(pad);
            let end = self.loop_pad_pressed.max(pad) + 1;
            let new_start = start as f64 * length_of_one_pad;
            clip.set_loop_start(new_start);
            clip.set_loop_length((end - start) as f64 * length_of_one_pad);
            clip.set_play_range(new_start, end as f64 * length_of_one_pad);
        }
        self.loop_pad_pressed = -1;
    }

    pub fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !self.seq.is_active() {
            ctx.surface.grid().turn_off();
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

    /// Compute the colors of all pads (x, y, color ID).
    fn grid_colors(&self, ctx: &Ctx) -> Vec<(i32, i32, &'static str)> {
        let mut result = Vec::new();
        let Some(clip) = self.seq.clip_ref(ctx) else {
            return result;
        };

        // Steps with notes
        let step = clip.current_step();
        let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % self.num_display_cols } else { -1 };
        let edit_notes = self.seq.edit_notes(ctx);
        let mut note_position = NotePosition::new(ctx.config.midi_edit_channel(), 0, 0);
        for x in 0..self.num_display_cols {
            note_position.step = x;
            for y in 0..self.seq.num_sequencer_rows {
                let map = self.seq.view.key_manager.map(y);
                note_position.note = map;
                let step_info = if map < 0 { None } else { Some(clip.step(&note_position)) };
                let color = self.seq.step_color_with_pad(ctx, step_info, x == hi_step, note_position.channel, x, y, map, &edit_notes);
                result.push((x, self.num_display_rows - 1 - y, color));
            }
        }

        if self.num_display_rows - self.seq.num_sequencer_rows <= 0 {
            return result;
        }

        // Clip Pages on the top
        let length_of_one_pad = self.seq.length_of_one_page(ctx, self.num_display_cols) as f64;
        let loop_start = clip.loop_start();
        let loop_start_pad = (loop_start / length_of_one_pad).ceil() as i32;
        let loop_end_pad = ((loop_start + clip.loop_length()) / length_of_one_pad).ceil() as i32;
        let current_page = step / self.num_display_cols;
        for pad in 0..self.num_display_cols {
            result.push((pad, 0, SequencerBase::page_color(loop_start_pad, loop_end_pad, current_page, clip.edit_page(), pad)));
        }
        result
    }

    pub fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if !self.seq.is_active() || event != ButtonEvent::Down {
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
        let value = (self.offset_y - self.scroll_offset(ctx)).max(0);
        self.update_octave(ctx, value);
    }

    pub fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if !self.seq.is_active() || event != ButtonEvent::Down {
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
        let offset = self.scroll_offset(ctx);
        if self.offset_y + offset < self.seq.clip(ctx).num_rows() as i32 {
            self.update_octave(ctx, self.offset_y + offset);
        }
    }

    pub fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.update_octave(ctx, self.offset_y);
    }

    /// The number of semitones to scroll for an octave up/down.
    pub fn scroll_offset(&self, ctx: &Ctx) -> i32 {
        let scales = ctx.model.scales();
        // In chromatic mode all semi-notes are present
        if scales.is_chromatic() {
            return self.seq.num_sequencer_rows;
        }
        let scale = scales.scale();
        let lower = scale.index_in_scale(self.offset_y).max(0);
        let upper = lower + self.seq.num_sequencer_rows;
        let intervals = scale.intervals();
        let len = intervals.len() as i32;
        let lower_note = intervals[lower as usize];
        let upper_note = upper / len * OCTAVE + intervals[(upper % len) as usize];
        upper_note - lower_note
    }

    pub fn is_octave_up_button_on(&self, _ctx: &Ctx) -> bool {
        self.seq.is_active()
    }

    pub fn is_octave_down_button_on(&self, _ctx: &Ctx) -> bool {
        self.seq.is_active()
    }

    pub fn update_scale(&mut self, ctx: &mut Ctx) {
        if ctx.model.can_selected_track_hold_notes() {
            let matrix = ctx.model.scales().sequencer_matrix((self.seq.num_sequencer_rows + 1) as usize, self.offset_y);
            self.seq.set_sequencer_matrix(&matrix);
        } else {
            self.seq.view.clear_note_mapping(ctx);
        }
    }

    pub fn update_octave(&mut self, ctx: &mut Ctx, value: i32) {
        self.offset_y = value;
        self.update_scale(ctx);
        self.seq.clear_edit_notes(ctx);
        let text = Scales::sequencer_range_text(self.seq.view.key_manager.map(0), self.seq.view.key_manager.map(self.seq.num_sequencer_rows - 1));
        ctx.schedule(10, move |ctx| ctx.notify(&text));
    }
}
