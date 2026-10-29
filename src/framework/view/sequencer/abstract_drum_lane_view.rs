// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A drum sequencer with several lanes (sounds) at once (former `AbstractDrumLaneView`,
//! `AbstractDrum4View` and `AbstractDrum8View`).

use super::abstract_drum_view::{apply_lights, DrumBase, PadLight, GRID_COLUMNS};
use super::abstract_sequencer_view::SequencerBase;
use crate::daw::clip::{NotePosition, StepState};
use crate::daw::constants::Resolution;
use crate::daw::events::ModelEvent;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// Former `Views.NAME_DRUM4`.
pub const NAME_DRUM4: &str = "Drum 4";
/// Former `Views.NAME_DRUM8`.
pub const NAME_DRUM8: &str = "Drum 8";

/// The hooks of the drum lane sequencer, which controller specific views can override.
pub trait DrumLaneHooks {
    /// Former `handleSequencerArea`.
    fn handle_sequencer_area(&mut self, base: &mut DrumLaneBase, ctx: &mut Ctx, velocity: i32, down_velocity: i32, note_position: &NotePosition) {
        base.handle_sequencer_area(ctx, velocity, down_velocity, note_position);
    }

    /// Former `handleSequencerAreaButtonCombinations`.
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumLaneBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32, accent_velocity: i32) -> bool {
        base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity, accent_velocity)
    }
}

/// The default hooks.
pub struct DefaultDrumLaneHooks;

impl DrumLaneHooks for DefaultDrumLaneHooks {}

/// The state of a drum lane view.
pub struct DrumLaneBase {
    pub drum: DrumBase,
    pub lanes: i32,
}

impl DrumLaneBase {
    /// Constructor with 8 rows and 8 columns.
    pub fn new(name: &str, lanes: i32, clip_cols: usize, use_daw_colors: bool) -> Self {
        Self::with_layout(name, lanes, 8, GRID_COLUMNS, clip_cols, true, use_daw_colors)
    }

    /// Constructor.
    pub fn with_layout(name: &str, lanes: i32, num_rows: i32, num_columns: i32, clip_cols: usize, follow_selection: bool, use_daw_colors: bool) -> Self {
        // numSequencerLines is set to 1 since it is only used as an indicator to trigger the
        // clip creation
        let mut drum = DrumBase::with_layout(name, 1, 0, num_columns, 128, clip_cols, follow_selection, use_daw_colors);
        drum.all_rows = num_rows;
        drum.lane_mode = true;
        DrumLaneBase { drum, lanes }
    }

    /// Former `AbstractDrum4View`: 4 lanes with 16 steps.
    pub fn drum4(use_daw_colors: bool) -> Self {
        Self::new(NAME_DRUM4, 4, 16, use_daw_colors)
    }

    /// Former `AbstractDrum8View`: 8 lanes with 8 steps.
    pub fn drum8(use_daw_colors: bool) -> Self {
        Self::with_layout(NAME_DRUM8, 8, 8, GRID_COLUMNS, 8, true, use_daw_colors)
    }

    pub fn seq(&self) -> &SequencerBase {
        &self.drum.seq
    }

    pub fn seq_mut(&mut self) -> &mut SequencerBase {
        &mut self.drum.seq
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.drum.on_activate(ctx);
    }

    pub fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.drum.on_deactivate(ctx);
    }

    pub fn update_control_surface(&mut self, ctx: &mut Ctx) {
        self.drum.update_control_surface(ctx);
    }

    pub fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.drum.on_model_event(ctx, event);
    }

    /// Get the sound (MIDI note) and the step of a grid pad (index).
    pub fn sound_and_step(&self, ctx: &Ctx, index: i32) -> (i32, i32) {
        let num_columns = self.drum.num_columns;
        let x = index % num_columns;
        let y = index / num_columns;
        let sound = y % self.lanes + ctx.model.scales().drum_offset();
        let step = (self.drum.all_rows - 1 - y) / self.lanes * num_columns + x;
        (sound, step)
    }

    pub fn on_grid_note<H: DrumLaneHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !self.drum.seq.is_active() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let y = index / self.drum.num_columns;
        let (sound, step) = self.sound_and_step(ctx, index);
        let vel = SequencerBase::pad_velocity(ctx, ButtonId::get(ButtonId::Pad1, index as usize));
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), step, sound);
        if hooks.handle_sequencer_area_button_combinations(self, ctx, &note_position, y, velocity, vel) || self.drum.seq.handle_note_editor(ctx, &note_position, velocity) {
            return;
        }
        hooks.handle_sequencer_area(self, ctx, velocity, vel, &note_position);
    }

    /// The default handling of a pad press: toggle the step on release.
    pub fn handle_sequencer_area(&mut self, ctx: &mut Ctx, velocity: i32, down_velocity: i32, note_position: &NotePosition) {
        if velocity == 0 {
            self.drum.seq.clip(ctx).toggle_step(note_position, down_velocity);
        }
    }

    /// The default handling of button combinations.
    pub fn handle_sequencer_area_button_combinations(&mut self, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32, accent_velocity: i32) -> bool {
        // Handle note duplicate function
        if ctx.is_button_combination(ButtonId::Duplicate) {
            if velocity == 0 {
                let clip = self.drum.seq.clip(ctx);
                let note_step = clip.step(note_position);
                if note_step.state() == StepState::Start {
                    self.drum.copy_note = Some(note_step.clone());
                } else if let Some(copy_note) = &self.drum.copy_note {
                    clip.set_step_from(note_position, copy_note);
                }
            }
            return true;
        }

        if ctx.is_button_combination(ButtonId::Mute) {
            if velocity == 0 {
                let clip = self.drum.seq.clip(ctx);
                let step_info = clip.step(note_position);
                if step_info.state() == StepState::Start {
                    let is_muted = step_info.is_muted();
                    clip.update_step_mute_state(note_position, !is_muted);
                }
            }
            return true;
        }

        // Change length of a note or create a new one with a length
        let num_columns = self.drum.num_columns;
        let lane_offset = (self.drum.all_rows - row - 1) / self.lanes * num_columns;
        let offset = row * num_columns;
        let mut np = *note_position;
        let step = np.step;
        let note = np.note;
        for s in 0..step {
            let pad = offset + s;
            if !(0..64).contains(&pad) {
                continue;
            }
            let id = ButtonId::get(ButtonId::Pad1, pad as usize);
            if ctx.surface.is_long_pressed(id) {
                let start = s + lane_offset;
                ctx.surface.set_trigger_consumed(id);
                let length = step - start + 1;
                let duration = length as f64 * Resolution::value_at(self.drum.seq.resolution_index(ctx));
                np.step = start;
                let clip = self.drum.seq.clip(ctx);
                let state = if note < 0 { StepState::Off } else { clip.step(&np).state() };
                if state == StepState::Start {
                    clip.update_step_duration(&np, duration);
                } else {
                    clip.set_step(&np, accent_velocity, duration);
                }
                return true;
            }
        }
        false
    }

    pub fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !self.drum.seq.is_active() {
            ctx.surface.grid().turn_off();
            return;
        }
        self.drum.seq.clip(ctx);
        let lights = self.grid_lights(ctx);
        apply_lights(ctx, lights);
    }

    fn grid_lights(&self, ctx: &Ctx) -> Vec<(i32, i32, PadLight)> {
        let mut lights = Vec::new();
        let seq = &self.drum.seq;
        let Some(clip) = seq.clip_ref(ctx) else {
            return lights;
        };
        let clip_cols = seq.clip_cols as i32;
        let num_columns = self.drum.num_columns;
        let step = clip.current_step();

        // Paint the sequencer steps
        let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % clip_cols } else { -1 };
        let offset_y = ctx.model.scales().drum_offset();
        let edit_notes = seq.edit_notes(ctx);
        let mut position = NotePosition::new(ctx.config.midi_edit_channel(), 0, 0);
        let primary = self.drum.drum_device(ctx);
        for sound in 0..self.lanes {
            position.note = offset_y + sound;
            let drum_pad_color = primary.and_then(|p| self.drum.pad_color(p, sound));
            for col in 0..clip_cols {
                position.step = col;
                let step_info = clip.step(&position);
                let x = col % num_columns;
                let mut y = self.lanes - 1 - sound;
                if col >= num_columns {
                    y += self.lanes;
                }
                let color = seq.step_color(Some(step_info), col == hi_step, drum_pad_color, position.channel, col, position.note, &edit_notes);
                lights.push((x, y, PadLight::Id(color)));
            }
        }
        lights
    }

    pub fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.drum.update_note_mapping(ctx);
    }

    pub fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.drum.change_octave(ctx, event, false, self.lanes, true, true);
    }

    pub fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.drum.change_octave(ctx, event, true, self.lanes, true, true);
    }
}
