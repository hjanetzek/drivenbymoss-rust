// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A XoX drum sequencer (former `AbstractDrumXoXView`): the clips of the cursor track at the top,
//! 16 drum pads below them and the sequencer steps of the selected pad at the bottom.
//!
//! Differences to Java:
//! - The clip colors come from the session colors which the concrete view sets in
//!   [`DrumXoXBase::session`] (Java reads them from the registered session view).
//! - The 16 clips are the slots of the cursor track's slot bank (Java: `getSlotBank (16)`).
//! - `adjustPage`/`blockSelectKnob` (only used by the Fire) are not ported.

use super::abstract_drum_view::{apply_lights, DrumBase, DrumHooks, PadLight};
use super::abstract_sequencer_view::SequencerBase;
use crate::daw::bank::BankItem;
use crate::daw::clip::{NotePosition, StepInfo};
use crate::daw::color::ColorEx;
use crate::daw::constants::Resolution;
use crate::framework::color_ids::abstract_sequencer_view::*;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::framework::view::session_view::SessionViewBase;
use crate::scale::{NoteMatrix, Scales};

/// The drum pad of each note of the 64 pads, -1 for none (former `DRUM_MATRIX`).
#[rustfmt::skip]
pub const DRUM_MATRIX: [i32; 64] = [
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
];

/// The number of clips (former `NUM_CLIPS`).
pub const NUM_CLIPS: i32 = 16;

/// The previous coarser resolution (former `PREV_RESOLUTION`).
pub fn prev_resolution(resolution: Resolution) -> Resolution {
    match resolution {
        Resolution::Res1_4 | Resolution::Res1_4T | Resolution::Res1_8 | Resolution::Res1_8T => Resolution::Res1_4,
        Resolution::Res1_16 | Resolution::Res1_16T => Resolution::Res1_8,
        Resolution::Res1_32 | Resolution::Res1_32T => Resolution::Res1_16,
    }
}

/// The next finer resolution (former `NEXT_RESOLUTION`).
pub fn next_resolution(resolution: Resolution) -> Resolution {
    match resolution {
        Resolution::Res1_4 | Resolution::Res1_4T => Resolution::Res1_8,
        Resolution::Res1_8 | Resolution::Res1_8T => Resolution::Res1_16,
        Resolution::Res1_16 | Resolution::Res1_16T | Resolution::Res1_32 | Resolution::Res1_32T => Resolution::Res1_32,
    }
}

/// The state of a XoX drum sequencer.
pub struct DrumXoXBase {
    pub drum: DrumBase,
    /// Provides the colors of the clips (former session view of the surface).
    pub session: SessionViewBase,
    pub delete_button: ButtonId,
    pub stop_button: ButtonId,
    pub browse_button: ButtonId,
    pub edit_loop_trigger_button: ButtonId,
    /// The slot to paste (index in the slot bank of the cursor track).
    pub source_slot: Option<(i32, i32)>,
    pub source_notes: Vec<StepInfo>,
    pub drum_matrix: [i32; 64],
    pub num_step_rows: i32,
    pub num_drum_pad_rows: i32,
    pub num_clips_rows: i32,
    pub sequencer_offset: i32,
    pub clips_offset: i32,
    was_alternate_interaction_used: bool,
}

impl DrumXoXBase {
    /// Constructor with a clip of 32 steps.
    pub fn new(name: &str, num_columns: i32) -> Self {
        Self::with_clip_cols(name, num_columns, 32)
    }

    pub fn with_clip_cols(name: &str, num_columns: i32, clip_cols: i32) -> Self {
        let mut drum = DrumBase::with_layout(name, 32 / num_columns, 1, num_columns, 128, clip_cols as usize, true, true);
        drum.play_columns = 16;
        drum.all_rows = drum.sequencer_lines;
        // Pre-calculation for grid drawing
        let num_clips_rows = NUM_CLIPS / num_columns;
        let num_drum_pad_rows = drum.play_columns / num_columns;
        let num_step_rows = clip_cols / num_columns;
        DrumXoXBase {
            drum,
            session: SessionViewBase::new(8, 8, true),
            delete_button: ButtonId::Delete,
            stop_button: ButtonId::Stop,
            browse_button: ButtonId::Browse,
            edit_loop_trigger_button: ButtonId::FixedLength,
            source_slot: None,
            source_notes: Vec::new(),
            drum_matrix: DRUM_MATRIX,
            num_step_rows,
            num_drum_pad_rows,
            num_clips_rows,
            sequencer_offset: num_clips_rows + num_drum_pad_rows,
            clips_offset: num_step_rows + num_drum_pad_rows,
            was_alternate_interaction_used: false,
        }
    }

    pub fn seq(&self) -> &SequencerBase {
        &self.drum.seq
    }

    // -------------------------------------------------------------------------------------
    // Drawing

    pub fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !ctx.model.can_selected_track_hold_notes() {
            ctx.surface.grid().turn_off();
            return;
        }
        self.drum.seq.clip(ctx);
        let lights = self.grid_lights(ctx);
        apply_lights(ctx, lights);
    }

    fn grid_lights(&self, ctx: &Ctx) -> Vec<(i32, i32, PadLight)> {
        let mut lights = Vec::new();
        let Some(primary) = self.drum.drum_device(ctx) else {
            return lights;
        };
        self.draw_drum_pads(ctx, &mut lights);
        if self.drum.sequencer_lines > 0 {
            let is_active = self.drum.seq.is_active();
            self.draw_pages(ctx, &mut lights);
            let note_row = ctx.model.scales().drum_offset() + self.drum.selected_pad;
            let row_color = self.drum.pad_color(primary, self.drum.selected_pad);
            self.draw_sequencer_steps(ctx, is_active, note_row, row_color, &mut lights);
        }
        lights
    }

    /// Former `drawPages`: the clips area.
    fn draw_pages(&self, ctx: &Ctx, lights: &mut Vec<(i32, i32, PadLight)>) {
        let track = ctx.model.cursor_track();
        let is_rec_armed = track.is_rec_arm();
        let slot_bank = track.slot_bank();
        let num_columns = self.drum.num_columns;
        let count = (slot_bank.page_size() as i32).min(NUM_CLIPS);
        for x in 0..count {
            let light = self.session.pad_light(ctx, slot_bank.item(x as usize), is_rec_armed);
            // The blink colors are not supported by the pad lights
            lights.push((x % num_columns, x / num_columns, PadLight::Index(light.color)));
        }
    }

    /// Former `drawDrumPads`.
    fn draw_drum_pads(&self, ctx: &Ctx, lights: &mut Vec<(i32, i32, PadLight)>) {
        let Some(device) = self.drum.drum_device(ctx) else {
            return;
        };
        let bank = device.drum_pad_bank();
        let is_recording = ctx.model.has_recording_state();
        let num_columns = self.drum.num_columns;
        for x in 0..self.drum.play_columns {
            let y = self.num_clips_rows + self.num_drum_pad_rows - 1 - x / num_columns;
            lights.push((x % num_columns, y, PadLight::Id(self.drum.drum_pad_color(ctx, x, bank, is_recording))));
        }
    }

    /// Former `drawSequencerSteps`.
    fn draw_sequencer_steps(&self, ctx: &Ctx, is_active: bool, note_row: i32, row_color: Option<ColorEx>, lights: &mut Vec<(i32, i32, PadLight)>) {
        let Some(clip) = self.drum.seq.clip_ref(ctx) else {
            return;
        };
        let num_columns = self.drum.num_columns;
        let sequencer_steps = self.drum.sequencer_steps;

        // Draw loop length
        if self.is_edit_loop_range(ctx) {
            let mut number_of_active_steps = (clip.loop_length() / Resolution::value_at(self.drum.seq.resolution_index(ctx))).floor() as i32;
            number_of_active_steps -= clip.edit_page() * sequencer_steps;
            for col in 0..sequencer_steps {
                let is_fourth = (col + 1) % 4 == 0;
                let color = if col < number_of_active_steps {
                    if is_fourth {
                        COLOR_ACTIVE_PAGE
                    } else {
                        COLOR_PAGE
                    }
                } else if is_fourth {
                    COLOR_SELECTED_PAGE
                } else {
                    COLOR_NO_CONTENT
                };
                lights.push((col % num_columns, self.sequencer_offset + col / num_columns, PadLight::Id(color)));
            }
            return;
        }

        let step = clip.current_step();
        let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % sequencer_steps } else { -1 };
        let edit_notes = self.drum.seq.edit_notes(ctx);
        let mut note_position = NotePosition::new(ctx.config.midi_edit_channel(), 0, note_row);
        for col in 0..sequencer_steps {
            note_position.step = col;
            let step_info = clip.step(&note_position);
            let color = if is_active {
                self.drum.seq.step_color(Some(step_info), col == hi_step, row_color, note_position.channel, col, note_row, &edit_notes)
            } else {
                COLOR_NO_CONTENT
            };
            lights.push((col % num_columns, self.sequencer_offset + col / num_columns, PadLight::Id(color)));
        }
    }

    // -------------------------------------------------------------------------------------
    // Pads

    /// Former `onGridNote`.
    pub fn on_grid_note<H: DrumHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let num_columns = self.drum.num_columns;
        let index = note - ctx.surface.grid_ref().start_note();
        let x = index % num_columns;
        let y = index / num_columns;
        let offset_y = ctx.model.scales().drum_offset();

        // Sequencer steps
        if y < self.num_step_rows {
            if self.drum.seq.is_active() {
                self.handle_sequencer_area(hooks, ctx, index, x, y, offset_y, velocity);
            }
            return;
        }

        // Drum Pad row(s)
        if y < self.clips_offset {
            self.handle_note_area(ctx, (y - self.num_step_rows) * num_columns + x, offset_y, velocity);
            return;
        }

        // Clips area
        let row = ctx.surface.grid_ref().rows() - y - 1;
        self.handle_clip_row(ctx, index, row * num_columns + x, velocity != 0);
    }

    /// Former `handleNoteArea` (with the override of `handleNoteAreaButtonCombinations`).
    fn handle_note_area(&mut self, ctx: &mut Ctx, pad: i32, offset_y: i32, velocity: i32) {
        if velocity > 0 {
            self.drum.set_selected_pad(ctx, pad);
        } else if self.drum.selected_pad != pad {
            self.drum.selected_pad = pad;
            self.drum.seq.clear_edit_notes(ctx);
        }

        // Mark selected note
        let key = offset_y + self.drum.selected_pad;
        if (0..128).contains(&key) {
            self.drum.seq.view.key_manager.set_key_pressed(key, velocity);
        }
        if velocity == 0 {
            return;
        }

        if self.is_duplicate_trigger(ctx) {
            self.duplicate_steps(ctx);
            return;
        }
        let selected_pad = self.drum.selected_pad;
        self.drum.handle_note_area_button_combinations(ctx, selected_pad);
    }

    /// Former `handleClipRow`.
    pub fn handle_clip_row(&mut self, ctx: &mut Ctx, pad_index: i32, clip_index: i32, is_pressed: bool) {
        let is_alternate_function = self.is_alternate_function(ctx);
        if is_alternate_function {
            self.was_alternate_interaction_used = true;
        }
        let slot_index = clip_index.max(0) as usize;

        if is_pressed {
            if self.handle_clip_row_button_combinations(ctx, slot_index) {
                // Do not trigger on pad up if already handled
                ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, pad_index.max(0) as usize));
                return;
            }
            if ctx.config.is_select_clip_on_launch() {
                ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index).select();
            }
        }

        let track = ctx.model.cursor_track();
        if !track.is_rec_arm() || track.slot_bank().item(slot_index).has_content() {
            self.handle_clip_launch(ctx, slot_index, is_pressed);
            return;
        }
        let Some(track) = ctx.model.cursor_track_ref() else {
            return;
        };
        match ctx.config.action_for_rec_armed_pad() {
            0 => ctx.model.record_note_clip(track, slot_index),
            1 => {
                let length_in_beats = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
                ctx.model.create_note_clip(track, slot_index, length_in_beats, true);
            }
            // Do nothing
            _ => {}
        }
    }

    /// Former `handleClipLaunch`.
    pub fn handle_clip_launch(&mut self, ctx: &mut Ctx, slot_index: usize, is_pressed: bool) {
        let is_alternative = self.is_alternate_function(ctx);
        let select_pressed = ctx.surface.is_pressed(self.drum.button_select);
        let slot = ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index);
        if select_pressed {
            slot.select();
        } else {
            slot.launch(is_pressed, is_alternative);
        }
    }

    /// Former `handleSequencerArea`.
    #[allow(clippy::too_many_arguments)]
    pub fn handle_sequencer_area<H: DrumHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, index: i32, x: i32, y: i32, offset_y: i32, velocity: i32) {
        if self.is_edit_loop_range(ctx) {
            if velocity > 0 {
                return;
            }
            ctx.surface.set_trigger_consumed(self.edit_loop_trigger_button);
            let mut steps = (self.num_step_rows - 1 - y) * self.drum.num_columns + x + 1;
            let resolution = Resolution::value_at(self.drum.seq.resolution_index(ctx));
            let sequencer_steps = self.drum.sequencer_steps;
            let clip = self.drum.seq.clip(ctx);
            steps += clip.edit_page() * sequencer_steps;
            clip.set_loop_length(steps as f64 * resolution);
            return;
        }
        // Former handleNoteEditorMode: false by default
        self.drum.handle_sequencer_area(hooks, ctx, index, x, y, offset_y, velocity);
    }

    /// Former `handleClipRowButtonCombinations`.
    pub fn handle_clip_row_button_combinations(&mut self, ctx: &mut Ctx, slot_index: usize) -> bool {
        if !ctx.model.cursor_track().does_exist() {
            return true;
        }

        // Delete selected clip
        if self.drum.is_delete_trigger(ctx) {
            let slot = ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index);
            if slot.does_exist() {
                slot.remove();
            }
            return true;
        }

        // Duplicate selected clip
        if self.is_duplicate_trigger(ctx) {
            if let Some((source_track, source_position)) = self.source_slot.take() {
                ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index).paste(source_track, source_position);
            } else {
                let slot = ctx.model.cursor_track().slot_bank().item(slot_index);
                if slot.does_exist() && slot.has_content() {
                    self.source_slot = Some((slot.track_position(), slot.position()));
                }
            }
            self.source_notes.clear();
            return true;
        }

        // Stop clip
        if ctx.is_button_combination(self.stop_button) {
            ctx.model.cursor_track_mut().stop(false);
            return true;
        }

        // Browse for clips (only with a host browser)
        if ctx.is_button_combination(self.browse_button) {
            let slot = ctx.model.cursor_track().slot_bank().item(slot_index);
            let (name, track, position) = (slot.name().to_string(), slot.track_position(), slot.position());
            if let Some(browser) = ctx.model.browser_mut() {
                browser.replace_slot(&name, track, position);
            }
            return true;
        }
        false
    }

    /// Former `duplicateSteps`.
    pub fn duplicate_steps(&mut self, ctx: &mut Ctx) {
        self.source_slot = None;
        let drum_pad = ctx.model.scales().drum_offset() + self.drum.selected_pad;
        if !self.drum.seq.clip(ctx).does_exist() || drum_pad == -1 {
            return;
        }
        let sequencer_steps = self.drum.sequencer_steps;
        let mut note_position = NotePosition::new(ctx.config.midi_edit_channel(), 0, drum_pad);

        // Stores the active steps of the selected drum pad, if there are no saved steps
        if self.source_notes.is_empty() {
            let clip = self.drum.seq.clip(ctx);
            for step in 0..sequencer_steps {
                note_position.step = step;
                self.source_notes.push(clip.step(&note_position).clone());
            }
            return;
        }

        // Writes the active steps of the (newly) selected drum pad and clears them afterwards
        let notes = std::mem::take(&mut self.source_notes);
        let clip = self.drum.seq.clip(ctx);
        for (step, note_step) in notes.iter().enumerate() {
            note_position.step = step as i32;
            if note_step.velocity() == 0.0 {
                clip.clear_step(&note_position);
            } else {
                clip.set_step_from(&note_position, note_step);
            }
        }
    }

    /// Former `isDuplicateTrigger`.
    pub fn is_duplicate_trigger(&self, ctx: &mut Ctx) -> bool {
        ctx.is_button_combination(ButtonId::Duplicate)
    }

    /// Former `getDrumMatrix`.
    pub fn drum_matrix(&self, ctx: &Ctx) -> NoteMatrix {
        let mut note_map = Scales::empty_matrix();
        let drum_offset = ctx.model.scales().drum_offset();
        for note in Scales::DRUM_NOTE_START..Scales::DRUM_NOTE_END {
            let ns = self.drum_matrix[(note - Scales::DRUM_NOTE_START) as usize];
            let n = if ns == -1 { -1 } else { ns + drum_offset };
            note_map[note as usize] = if !(0..=127).contains(&n) { -1 } else { n };
        }
        note_map
    }

    /// Former `updateNoteMapping` with the matrix of [`Self::drum_matrix`].
    pub fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let matrix = if self.can_pads_be_turned_on(ctx) { self.drum_matrix(ctx) } else { Scales::empty_matrix() };
        self.drum.seq.view.update_note_mapping(ctx, matrix);
    }

    fn can_pads_be_turned_on(&self, ctx: &mut Ctx) -> bool {
        if !ctx.model.can_selected_track_hold_notes() {
            return false;
        }
        let drum = &self.drum;
        !(drum.is_select_trigger(ctx) || drum.is_delete_trigger(ctx) || drum.is_mute_trigger(ctx) || drum.is_solo_trigger(ctx) || drum.is_browse_trigger(ctx))
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.drum.on_activate(ctx);
        self.update_note_mapping(ctx);
    }

    /// Former `selectDrumPad`.
    pub fn select_drum_pad(&self, ctx: &mut Ctx, index: usize) {
        if let Some(device) = ctx.model.drum_device_mut() {
            device.drum_pad_bank_mut().select_pad(index);
        }
    }

    /// Former `isEditLoopRange`.
    pub fn is_edit_loop_range(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_pressed(self.edit_loop_trigger_button)
    }

    /// Former `isAlternateFunction`.
    pub fn is_alternate_function(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_shift_pressed()
    }

    /// Former `wasAlternateInteractionUsed` (of `ISessionAlternative`).
    pub fn was_alternate_interaction_used(&self) -> bool {
        self.was_alternate_interaction_used
    }

    /// Former `setAlternateInteractionUsed` (of `ISessionAlternative`).
    pub fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.was_alternate_interaction_used = was_used;
    }

    /// Former `rotateStepsLeft`.
    pub fn rotate_steps_left(&mut self, ctx: &mut Ctx) {
        let drum_pad = ctx.model.scales().drum_offset() + self.drum.selected_pad;
        if drum_pad == -1 {
            return;
        }
        let sequencer_steps = self.drum.sequencer_steps;
        let mut np = NotePosition::new(ctx.config.midi_edit_channel(), 0, drum_pad);
        let clip = self.drum.seq.clip(ctx);
        let first_step = clip.step(&np).clone();
        for step in 1..sequencer_steps {
            np.step = step;
            let note_step = clip.step(&np).clone();
            np.step = step - 1;
            Self::write_step(clip, &np, &note_step);
        }
        np.step = sequencer_steps - 1;
        Self::write_step(clip, &np, &first_step);
    }

    /// Former `rotateStepsRight`.
    pub fn rotate_steps_right(&mut self, ctx: &mut Ctx) {
        let drum_pad = ctx.model.scales().drum_offset() + self.drum.selected_pad;
        if drum_pad == -1 {
            return;
        }
        let sequencer_steps = self.drum.sequencer_steps;
        let mut np = NotePosition::new(ctx.config.midi_edit_channel(), sequencer_steps - 1, drum_pad);
        let clip = self.drum.seq.clip(ctx);
        let last_step = clip.step(&np).clone();
        for step in 0..sequencer_steps - 1 {
            np.step = step;
            let note_step = clip.step(&np).clone();
            np.step = step + 1;
            Self::write_step(clip, &np, &note_step);
        }
        np.step = 0;
        Self::write_step(clip, &np, &last_step);
    }

    fn write_step(clip: &crate::daw::clip::CursorClip, np: &NotePosition, step: &StepInfo) {
        if step.velocity() == 0.0 {
            clip.clear_step(np);
        } else {
            clip.set_step_from(np, step);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_of_8_columns() {
        let xox = DrumXoXBase::new("Drum XoX", 8);
        assert_eq!(xox.num_step_rows, 4);
        assert_eq!(xox.num_drum_pad_rows, 2);
        assert_eq!(xox.num_clips_rows, 2);
        assert_eq!(xox.sequencer_offset, 4);
        assert_eq!(xox.clips_offset, 6);
        assert_eq!(xox.drum.all_rows, 4);
        assert_eq!(xox.drum.sequencer_steps, 32);
    }

    #[test]
    fn resolutions() {
        assert_eq!(prev_resolution(Resolution::Res1_16), Resolution::Res1_8);
        assert_eq!(next_resolution(Resolution::Res1_32T), Resolution::Res1_32);
    }
}
