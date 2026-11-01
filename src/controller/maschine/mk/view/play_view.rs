// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The play view (former `PlayView`): the pads play the notes of the scale, alternatively they
//! show the 16 sequencer steps of the last played note or the scale settings (shifted).

use std::cell::RefCell;
use std::rc::Rc;

use super::drum_view::{DrumView, CLIP_KEY, NUM_STEPS};
use crate::config::SettingId;
use crate::controller::maschine::core::colors::{COLOR_BLACK, COLOR_BLUE, COLOR_GREEN, COLOR_MAGENTA, COLOR_MAGENTA_LO, COLOR_ROSE, COLOR_SKIN};
use crate::daw::clip::{NotePosition, StepInfo, StepState};
use crate::daw::color::{ColorEx, DawColor};
use crate::daw::constants::Resolution;
use crate::daw::events::ModelEvent;
use crate::framework::color_ids::abstract_sequencer_view::{
    COLOR_CONTENT, COLOR_CONTENT_CONT, COLOR_NO_CONTENT, COLOR_STEP_HILITE_CONTENT, COLOR_STEP_HILITE_NO_CONTENT, COLOR_STEP_MUTED, COLOR_STEP_MUTED_CONT,
    COLOR_STEP_SELECTED,
};
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::view::play_view::{play_scale_matrix, HeldChords, PlayViewBase};
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

/// The play view.
pub struct PlayView {
    play: PlayViewBase,
    drum_view: Rc<RefCell<DrumView>>,
    aftertouch: Box<dyn AftertouchCommand>,
    num_columns: i32,
    is_shifted: bool,
    selected_note: i32,
    copy_note: Option<StepInfo>,
    is_chord_active: bool,
    held_chords: HeldChords,
}

impl PlayView {
    pub fn new(_ctx: &Ctx, drum_view: Rc<RefCell<DrumView>>) -> Self {
        PlayView {
            play: PlayViewBase::new("Play", true),
            drum_view,
            aftertouch: Box::new(AftertouchViewCommand::new()),
            num_columns: 4,
            is_shifted: false,
            selected_note: -1,
            copy_note: None,
            is_chord_active: false,
            held_chords: HeldChords::default(),
        }
    }

    pub fn toggle_chord_mode(&mut self) {
        self.is_chord_active = !self.is_chord_active;
    }

    pub fn is_chord_mode(&self) -> bool {
        self.is_chord_active
    }

    pub fn toggle_shifted(&mut self, ctx: &mut Ctx) {
        self.is_shifted = !self.is_shifted;
        self.update_note_mapping(ctx);
    }

    pub fn is_shifted(&self) -> bool {
        self.is_shifted
    }

    /// Draw the 16 sequencer steps of the selected note (former `drawSequencerSteps`).
    fn draw_sequencer_steps(&self, ctx: &mut Ctx, is_active: bool, note_row: i32) {
        let edit_notes = Self::edit_notes(ctx);
        let track_color = ctx.model.current_track_bank().selected_item().map(|t| t.color());
        let channel = ctx.config.midi_edit_channel();
        let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
        let step = clip.current_step();
        let mut lights = Vec::with_capacity(NUM_STEPS);
        let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % NUM_STEPS as i32 } else { -1 };
        let mut note_position = NotePosition::new(channel, 0, note_row);
        for col in 0..NUM_STEPS as i32 {
            note_position.step = col;
            let x = col % self.num_columns;
            let y = 3 - col / self.num_columns;
            if note_row == -1 {
                lights.push((x, y, COLOR_NO_CONTENT));
                continue;
            }
            let step_info = clip.step(&note_position);
            let hilite = col == hi_step;
            lights.push((x, y, Self::step_color(is_active, step_info, hilite, &note_position, &edit_notes, track_color)));
        }
        let Ctx { surface, colors, .. } = ctx;
        let grid = surface.grid();
        for (x, y, id) in lights {
            grid.light_ex_id(colors, x, y, id);
        }
    }

    /// Former `getStepColor`.
    fn step_color(is_active: bool, step_info: &StepInfo, hilite: bool, note_position: &NotePosition, edit_notes: &[NotePosition], track_color: Option<ColorEx>) -> &'static str {
        if !is_active {
            return COLOR_NO_CONTENT;
        }
        match step_info.state() {
            StepState::Start => {
                if hilite {
                    return COLOR_STEP_HILITE_CONTENT;
                }
                if edit_notes.contains(note_position) {
                    return COLOR_STEP_SELECTED;
                }
                if step_info.is_muted() {
                    return COLOR_STEP_MUTED;
                }
                track_color.map(|c| DawColor::color_id(ColorEx::darker(c))).unwrap_or(COLOR_CONTENT)
            }
            StepState::Continue => {
                if hilite {
                    return COLOR_STEP_HILITE_CONTENT;
                }
                if edit_notes.contains(note_position) {
                    return COLOR_STEP_SELECTED;
                }
                if step_info.is_muted() {
                    return COLOR_STEP_MUTED_CONT;
                }
                track_color.map(|c| DawColor::color_id(ColorEx::darker(c))).unwrap_or(COLOR_CONTENT_CONT)
            }
            StepState::Off => {
                if hilite {
                    COLOR_STEP_HILITE_NO_CONTENT
                } else {
                    COLOR_NO_CONTENT
                }
            }
        }
    }

    fn draw_shifted_grid(ctx: &mut Ctx) {
        let is_keyboard_enabled = ctx.model.can_selected_track_hold_notes();
        let is_chromatic = ctx.model.scales().is_chromatic();
        let on = |color: i32| if is_keyboard_enabled { color } else { COLOR_BLACK };
        let grid = ctx.surface.grid();
        grid.light(36, if is_chromatic { COLOR_MAGENTA } else { COLOR_MAGENTA_LO });
        let colors = [(37, 40, COLOR_BLACK), (40, 42, on(COLOR_BLUE)), (42, 44, COLOR_BLACK), (44, 46, on(COLOR_GREEN)), (46, 48, COLOR_BLACK), (48, 50, on(COLOR_ROSE)), (50, 52, on(COLOR_SKIN))];
        for (from, to, color) in colors {
            for i in from..to {
                grid.light(i, color);
            }
        }
    }

    /// Former `handleSequencerArea`.
    fn handle_sequencer_area(&mut self, ctx: &mut Ctx, index: i32, x: i32, y: i32, velocity: i32) {
        if self.selected_note == -1 || velocity != 0 {
            return;
        }
        let channel = ctx.config.midi_edit_channel();
        let step = self.num_columns * (3 - y) + x;
        let vel = if ctx.config.is_accent_active() { ctx.config.fixed_accent_value() } else { SequencerBase::pressed_velocity(ctx, index) };
        let note_position = NotePosition::new(channel, step, self.selected_note);
        if self.handle_sequencer_area_button_combinations(ctx, &note_position, vel) {
            return;
        }
        ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1).toggle_step(&note_position, vel);
    }

    /// Former `handleSequencerAreaButtonCombinations`.
    fn handle_sequencer_area_button_combinations(&mut self, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        if ctx.is_button_combination(ButtonId::Mute) {
            let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
            let note_step = clip.step(note_position);
            if note_step.state() == StepState::Start {
                let is_muted = note_step.is_muted();
                clip.update_step_mute_state(note_position, !is_muted);
            }
            return true;
        }

        if ctx.modes.is_active(Modes::Note) {
            ctx.host.show_notification(&format!("Note {} - Step {}", Scales::format_note_and_octave(note_position.note, -3), note_position.step + 1));
            Self::edit_note(ctx, note_position, true);
            return true;
        }

        if ctx.surface.button(ButtonId::Duplicate).is_some_and(|b| b.is_pressed()) {
            ctx.surface.set_trigger_consumed(ButtonId::Duplicate);
            let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
            let note_step = clip.step(note_position);
            if note_step.state() == StepState::Start {
                self.copy_note = Some(note_step.clone());
            } else if let Some(copy_note) = &self.copy_note {
                clip.set_step_from(note_position, copy_note);
            }
            return true;
        }

        let mut np = *note_position;
        let step = np.step;
        let note = np.note;
        for s in (0..step).rev() {
            np.step = s;
            let x = s % self.num_columns;
            let y = s / self.num_columns;
            let pad = y * self.num_columns + x;
            let id = ButtonId::get(ButtonId::Pad1, pad as usize);
            if ctx.surface.is_long_pressed(id) {
                ctx.surface.set_trigger_consumed(id);
                let length = step - s + 1;
                let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
                let duration = length as f64 * Resolution::value_at(Resolution::get_match(clip.step_length()));
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

    /// Get the notes of the note editor mode (former `getEditNotes`).
    fn edit_notes(ctx: &Ctx) -> Vec<NotePosition> {
        ctx.modes.get(Modes::Note).and_then(|mode| mode.try_borrow().ok().and_then(|m| m.note_editor().map(|e| e.notes().to_vec()))).unwrap_or_default()
    }

    /// Former `editNote`.
    fn edit_note(ctx: &mut Ctx, note_position: &NotePosition, add_note: bool) {
        if ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1).step(note_position).state() != StepState::Start {
            return;
        }
        let Some(mode) = ctx.modes.get(Modes::Note) else {
            return;
        };
        let has_editor = match mode.try_borrow_mut() {
            Ok(mut mode) => match mode.note_editor_mut() {
                Some(editor) => {
                    if add_note {
                        editor.add_note(CLIP_KEY, note_position);
                    } else {
                        editor.set_note(CLIP_KEY, note_position);
                    }
                    true
                }
                None => false,
            },
            Err(_) => false,
        };
        if has_editor {
            ctx.set_active_mode(Modes::Note);
        }
    }

    /// Former `update`: update the note mapping and store the scale settings.
    fn update(&mut self, ctx: &mut Ctx) {
        self.update_note_mapping(ctx);
        update_scale_properties(ctx);
    }
}

/// Store the settings of the scales in the configuration (former
/// `Scales.updateScaleProperties`).
pub fn update_scale_properties(ctx: &mut Ctx) {
    let scales = ctx.model.scales();
    let scale = scales.scale().name().to_string();
    let base = Scales::BASES[scales.scale_offset_index()].to_string();
    let layout = scales.scale_layout().name().to_string();
    let in_key = !scales.is_chromatic();
    ctx.config.set_scale(&scale);
    ctx.config.set_scale_base(&base);
    ctx.config.set_scale_in_key(in_key);
    ctx.config.set_scale_layout(&layout);
}

impl View for PlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.held_chords.clear();
        self.play.on_activate(ctx, play_scale_matrix(ctx));
        if self.is_shifted {
            self.play.base.clear_note_mapping(ctx);
        }
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let can_hold_notes = ctx.model.can_selected_track_hold_notes();
        let exists = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1).does_exist();
        if let Ok(mut drum_view) = self.drum_view.try_borrow_mut() {
            drum_view.base.seq.set_sequencer_active(can_hold_notes && exists);
        }
        let chord = self.play.chord_name();
        self.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        if self.is_shifted {
            self.play.base.clear_note_mapping(ctx);
        } else {
            self.play.update_note_mapping(ctx, play_scale_matrix(ctx));
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let (is_grid_editor, is_sequencer_visible) = match self.drum_view.try_borrow() {
            Ok(drum_view) => (drum_view.is_grid_editor(), drum_view.is_sequencer_visible()),
            Err(_) => (false, false),
        };
        if is_grid_editor {
            if let Ok(mut drum_view) = self.drum_view.try_borrow_mut() {
                drum_view.draw_grid(ctx);
            }
            return;
        }
        if is_sequencer_visible {
            let is_active = ctx.model.can_selected_track_hold_notes();
            self.draw_sequencer_steps(ctx, is_active, self.selected_note);
            return;
        }
        if self.is_shifted {
            Self::draw_shifted_grid(ctx);
            return;
        }
        self.play.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, key: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }

        let (is_grid_editor, is_sequencer_visible) = match self.drum_view.try_borrow() {
            Ok(drum_view) => (drum_view.is_grid_editor(), drum_view.is_sequencer_visible()),
            Err(_) => (false, false),
        };
        if is_grid_editor {
            if let Ok(mut drum_view) = self.drum_view.try_borrow_mut() {
                drum_view.on_grid_note(ctx, key, velocity);
            }
            return;
        }

        let index = key - 36;
        if is_sequencer_visible {
            let x = index % self.num_columns;
            let y = index / self.num_columns;
            self.handle_sequencer_area(ctx, index, x, 3 - y, velocity);
            return;
        }

        if !self.is_shifted {
            self.play.on_grid_note(ctx, key, velocity);
            let note = self.play.base.key_manager.map(key);
            if note != -1 {
                self.play.base.key_manager.set_all_keys_pressed(note, velocity);
                self.selected_note = note;
                // A held chord is also released after the chord mode was switched off
                if self.is_chord_active || (velocity == 0 && self.held_chords.is_held(key)) {
                    let third_chord = ctx.model.scales().third_chord(note);
                    if third_chord.len() < 2 {
                        return;
                    }
                    let channel = ctx.config.midi_edit_channel();
                    self.held_chords.play(ctx, key, note, &third_chord[..2], channel, velocity);
                }
            }
            return;
        }

        if velocity == 0 {
            return;
        }

        match index {
            0 => {
                ctx.model.scales_mut().toggle_chromatic();
                let is_chromatic = ctx.model.scales().is_chromatic();
                ctx.notify(&format!("Chromatic: {}", if is_chromatic { "On" } else { "Off" }));
                ctx.config.set_scale_in_key(!is_chromatic);
            }
            4 | 5 => {
                if index == 4 {
                    ctx.model.scales_mut().prev_scale_layout();
                } else {
                    ctx.model.scales_mut().next_scale_layout();
                }
                let name = ctx.model.scales().scale_layout().name();
                ctx.notify(name);
            }
            8 | 9 => {
                if index == 8 {
                    ctx.model.scales_mut().prev_scale_offset();
                } else {
                    ctx.model.scales_mut().next_scale_offset();
                }
                let name = Scales::BASES[ctx.model.scales().scale_offset_index()];
                ctx.notify(name);
            }
            12 | 13 => {
                if index == 12 {
                    ctx.model.scales_mut().prev_scale();
                } else {
                    ctx.model.scales_mut().next_scale();
                }
                let name = ctx.model.scales().scale().name();
                ctx.notify(name);
            }
            14 => self.on_octave_down(ctx, ButtonEvent::Down),
            15 => self.on_octave_up(ctx, ButtonEvent::Down),
            _ => {}
        }
        self.update(ctx);
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.play.on_model_event(ctx, event);
    }

    fn on_setting_changed(&mut self, ctx: &mut Ctx, id: SettingId) {
        if id == SettingId::ACTIVATE_FIXED_ACCENT || id == SettingId::FIXED_ACCENT_VALUE {
            self.play.init_max_velocity(ctx);
        }
    }

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    fn sequencer_resolution_index(&self, ctx: &Ctx) -> Option<usize> {
        self.drum_view.try_borrow().ok().map(|d| d.resolution_index(ctx))
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PlayView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.on_octave_down(ctx, event, play_scale_matrix);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.on_octave_up(ctx, event, play_scale_matrix);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        self.play.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.play.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.play.reset_octave(ctx, play_scale_matrix);
    }
}
