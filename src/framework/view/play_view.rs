// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Abstract implementation for a play grid (former `AbstractPlayView`), the chord grid (former
//! `AbstractChordView`) and the piano grid drawing (former `PianoViewHelper`).
//!
//! The concrete view owns a [`PlayViewBase`] (or [`ChordViewBase`]) and forwards the
//! [`crate::framework::feature::View`] and
//! [`crate::framework::view::transpose_view::TransposeView`] functions to it. The scale
//! matrix (former overridable `getScaleMatrix`/`getMapping`) is handed in by the concrete view.

use crate::daw::events::ModelEvent;
use crate::daw::track::Track;
use crate::framework::chord_identifier;
use crate::framework::color_ids::abstract_play_view::{COLOR_OFF, COLOR_PLAY, COLOR_RECORD};
use crate::framework::ctx::Ctx;
use crate::framework::view::base::{replace_octave_color_with_track_color, ViewBase};
use crate::midi::{ButtonEvent, CMD_NOTE_ON};
use crate::scale::{NoteMatrix, Scales};

/// The state of a play view (former `AbstractPlayView`).
pub struct PlayViewBase {
    pub base: ViewBase,
    pub use_track_color: bool,
    /// The number of notes to block from the bottom.
    pub block_notes: i32,
    default_velocity: [i32; 128],
    previous_chord: String,
}

impl PlayViewBase {
    /// Constructor. `use_track_color`: true to use the color of the current track for coloring
    /// the octaves.
    pub fn new(name: &str, use_track_color: bool) -> Self {
        PlayViewBase {
            base: ViewBase::new(name),
            use_track_color,
            block_notes: 0,
            default_velocity: std::array::from_fn(|i| i as i32),
            previous_chord: String::new(),
        }
    }

    /// Blocks the number of notes from the bottom and shifts the rest up by this number.
    pub fn set_blocked_notes(&mut self, block_notes: i32) {
        self.block_notes = block_notes;
    }

    /// Replaces the track bank observers (selection: clear the pressed keys; notes: mark the
    /// played notes).
    pub fn on_model_event(&mut self, ctx: &Ctx, event: &ModelEvent) {
        match event {
            ModelEvent::TrackSelectionChanged { .. } => self.base.key_manager.clear_pressed_keys(),
            ModelEvent::TrackNote { track_index, note, velocity } => {
                let selected = ctx.model.current_track_bank().selected_index().map(|i| i as i32);
                self.base.key_manager.call(*track_index, *note, *velocity, selected);
            }
            _ => {}
        }
    }

    /// Draw the grid (former `drawGrid`/`drawLightGuide`).
    pub fn draw_grid(&self, ctx: &mut Ctx) {
        let is_keyboard_enabled = ctx.model.can_selected_track_hold_notes();
        let is_recording = ctx.model.has_recording_state();
        let start_note = ctx.model.scales().start_note();
        let end_note = ctx.model.scales().end_note();
        for i in start_note..end_note {
            let color_id = self.grid_color(ctx, is_keyboard_enabled, is_recording, i);
            let color = ctx.colors.color_index(color_id);
            ctx.surface.grid().light(i, color);
        }
    }

    /// Get the color ID for a pad.
    pub fn grid_color(&self, ctx: &Ctx, is_keyboard_enabled: bool, is_recording: bool, note: i32) -> &'static str {
        if !is_keyboard_enabled {
            return COLOR_OFF;
        }
        let key_manager = &self.base.key_manager;
        if key_manager.is_key_pressed(note) {
            return if is_recording { COLOR_RECORD } else { COLOR_PLAY };
        }
        if ctx.config.is_turn_off_scale_pads() && key_manager.color(ctx.model.scales(), note) == Scales::SCALE_COLOR_NOTE {
            return Scales::SCALE_COLOR_OFF;
        }
        let track: Option<&Track> = if self.use_track_color { Some(ctx.model.cursor_track()) } else { None };
        self.base.pad_color(ctx, note, track)
    }

    /// A pad was played: mark the keys and let the hardware note pass to the DAW.
    pub fn on_grid_note(&mut self, ctx: &mut Ctx, key: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        // Mark selected notes immediately for better performance
        let note = self.base.key_manager.map(key);
        if note != -1 {
            self.base.key_manager.set_all_keys_pressed(note, velocity);
        }
        // Note: the Java hook `playNote` is empty for all views used here
    }

    pub fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent, scale_matrix: ScaleMatrixFn) {
        if event != ButtonEvent::Down {
            return;
        }
        self.base.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().dec_octave();
        self.update_note_mapping(ctx, scale_matrix(ctx));
        let text = ctx.model.scales().range_text();
        ctx.notify(&text);
    }

    pub fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent, scale_matrix: ScaleMatrixFn) {
        if event != ButtonEvent::Down {
            return;
        }
        self.base.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().inc_octave();
        self.update_note_mapping(ctx, scale_matrix(ctx));
        let text = ctx.model.scales().range_text();
        ctx.notify(&text);
    }

    pub fn reset_octave(&mut self, ctx: &mut Ctx, scale_matrix: ScaleMatrixFn) {
        self.base.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().reset_octave();
        self.update_note_mapping(ctx, scale_matrix(ctx));
    }

    pub fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        ctx.model.scales().octave() > -Scales::OCTAVE_RANGE
    }

    pub fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        ctx.model.scales().octave() < Scales::OCTAVE_RANGE
    }

    /// Activation: update the note mapping and the velocity table.
    pub fn on_activate(&mut self, ctx: &mut Ctx, scale_matrix: NoteMatrix) {
        self.update_note_mapping(ctx, scale_matrix);
        self.init_max_velocity(ctx);
    }

    /// Apply the note mapping (Java delays this by 100ms, here it is applied immediately).
    /// `scale_matrix` is the matrix of the view (former `getScaleMatrix`), the blocked notes are
    /// applied to it.
    pub fn update_note_mapping(&mut self, ctx: &mut Ctx, scale_matrix: NoteMatrix) {
        let mapping = self.mapping(ctx, scale_matrix);
        self.base.update_note_mapping(ctx, mapping);
    }

    /// Get the matrix to apply to the mapping table (former `getMapping`): empty if the
    /// selected track cannot play notes, otherwise the scale matrix with the blocked notes.
    pub fn mapping(&self, ctx: &Ctx, scale_matrix: NoteMatrix) -> NoteMatrix {
        if !ctx.model.can_selected_track_hold_notes() {
            return Scales::empty_matrix();
        }
        let mut note_matrix = scale_matrix;
        if self.block_notes > 0 {
            let start_note = ctx.model.scales().start_note() as usize;
            let end_note = ctx.model.scales().end_note() as usize;
            let block = self.block_notes as usize;
            let length = end_note - start_note - block;
            note_matrix.copy_within(start_note..start_note + length, start_note + block);
            note_matrix[start_note..start_note + 8].fill(-1);
        }
        note_matrix
    }

    /// Updates the velocity translation table based on the fixed accent value.
    pub fn init_max_velocity(&self, ctx: &mut Ctx) {
        if ctx.config.is_accent_active() {
            let mut max_velocity = [ctx.config.fixed_accent_value().clamp(0, 127); 128];
            max_velocity[0] = 0;
            ctx.surface.set_velocity_translation_table(&max_velocity);
        } else {
            ctx.surface.set_velocity_translation_table(&self.default_velocity);
        }
    }

    /// Notify the name of the played chord (former `AbstractView.displayChord`). `chord` is the
    /// result of the former `getChordName`.
    pub fn display_chord(&mut self, ctx: &mut Ctx, chord: Option<String>) {
        if !ctx.config.is_show_played_chords() {
            return;
        }
        if let Some(chord) = chord {
            if self.previous_chord != chord {
                ctx.notify(&chord);
                self.previous_chord = chord;
                return;
            }
        }
        self.previous_chord.clear();
    }

    /// The name of the chord of the pressed keys (former `getChordName`).
    pub fn chord_name(&self) -> Option<String> {
        chord_identifier::identify_chord(&self.base.key_manager.midi_notes_from_pressed_keys())
    }

    /// Update the display of the active mode and show the played chord (former
    /// `AbstractView.updateControlSurface`).
    pub fn update_control_surface(&mut self, ctx: &mut Ctx, chord: Option<String>) {
        ctx.update_active_mode_display();
        self.display_chord(ctx, chord);
    }
}

/// Calculates the scale matrix of a view (former `getScaleMatrix`).
pub type ScaleMatrixFn = fn(&Ctx) -> NoteMatrix;

/// The scale matrix of the play view (former `AbstractPlayView.getScaleMatrix`).
pub fn play_scale_matrix(ctx: &Ctx) -> NoteMatrix {
    ctx.model.scales().note_matrix()
}

/// The intervals of the chords for the rows of the chord view.
const CHORD_INTERVALS: [&[i32]; 8] = [
    &[3, 5],     // Triad
    &[5, 8],     // Dyad - Powerchord
    &[2, 5],     // Suspended second (Sus2)
    &[4, 5],     // Suspended fourth (Sus4)
    &[3, 5, 6],  // Add sixth (6)
    &[3, 5, 7],  // Major seventh chord (maj7)
    &[3, 5, 9],  // Add ninth (9)
    &[3, 5, 11], // Add eleventh (11)
];

/// Abstract implementation for a chord grid (former `AbstractChordView`). Each row of the grid
/// plays a different chord type on the note of the pad.
pub struct ChordViewBase {
    pub play: PlayViewBase,
    current_chord: Vec<i32>,
}

impl ChordViewBase {
    pub fn new(name: &str, use_track_color: bool) -> Self {
        ChordViewBase { play: PlayViewBase::new(name, use_track_color), current_chord: Vec::new() }
    }

    /// The scale matrix of the chord view (former `getScaleMatrix`).
    pub fn scale_matrix(ctx: &Ctx) -> NoteMatrix {
        let scales = ctx.model.scales();
        scales.note_matrix_from(scales.active_chord_matrix())
    }

    /// A pad was played: send the additional chord notes to the DAW.
    pub fn on_grid_note(&mut self, ctx: &mut Ctx, key: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }

        self.play.on_grid_note(ctx, key, velocity);

        let cols = ctx.surface.grid_ref().cols();
        let index = key - 36;
        let row = (index / cols).clamp(0, CHORD_INTERVALS.len() as i32 - 1) as usize;

        let note = self.play.base.key_manager.map(key);
        if note == -1 {
            // Java calculates the chord of -1 which results in notes outside of the MIDI range
            self.current_chord.clear();
            return;
        }

        let chord = ctx.model.scales().chord(note, CHORD_INTERVALS[row]);
        // Send additional chord notes to the DAW
        let channel = ctx.config.midi_edit_channel();
        let mut vel = 0;
        if velocity > 0 {
            vel = if ctx.config.is_accent_active() { ctx.config.fixed_accent_value() } else { velocity };
        }
        for &element in &chord {
            if (0..128).contains(&element) {
                ctx.surface.input.send_raw_midi_event(CMD_NOTE_ON + channel, element, vel);
            }
        }

        self.current_chord.clear();
        if velocity > 0 {
            self.current_chord.push(note);
            self.current_chord.extend(chord);
        }
    }

    /// The name of the played chord (former `getChordName`).
    pub fn chord_name(&self) -> Option<String> {
        chord_identifier::identify_chord(&self.current_chord)
    }
}

/// Draw the piano view grid (former `PianoViewHelper.drawGrid`).
pub fn draw_piano_grid(ctx: &mut Ctx, key_manager: &crate::scale::KeyManager, start_pad: i32, rows: i32, columns: i32) {
    if !ctx.model.can_selected_track_hold_notes() {
        ctx.surface.grid().turn_off();
        return;
    }

    let is_recording = ctx.model.has_recording_state();
    let track = ctx.model.cursor_track();
    let play_key_color = ctx.colors.color_index(if is_recording { COLOR_RECORD } else { COLOR_PLAY });
    let white_key_color = ctx.colors.color_index(Scales::SCALE_COLOR_NOTE);
    let black_key_color = ctx.colors.color_index(replace_octave_color_with_track_color(Some(track), Scales::SCALE_COLOR_OCTAVE));
    let off_key_color = ctx.colors.color_index(Scales::SCALE_COLOR_OFF);

    let grid = ctx.surface.grid();
    for row in 0..rows {
        for column in 0..columns {
            let n = start_pad + columns * row + column;
            let color = if row % 2 == 0 {
                // White keys
                if key_manager.is_key_pressed(n) {
                    play_key_color
                } else {
                    white_key_color
                }
            } else {
                // Black keys
                let octave_column = column % 7;
                if octave_column == 0 || octave_column == 3 || octave_column == 7 {
                    off_key_color
                } else if key_manager.is_key_pressed(n) {
                    play_key_color
                } else {
                    black_key_color
                }
            };
            grid.light_blink(n, color, -1, false);
        }
    }
}
