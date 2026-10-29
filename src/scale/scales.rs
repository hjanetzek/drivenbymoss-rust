// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Helper for applying scales to a pad grid (former `Scales`).

use std::fmt;

use super::{ChordGrid, Orientation, Scale, ScaleGrid, ScaleLayout};
use crate::framework::value_changer::ValueChanger;

/// A note matrix: maps each of the 128 MIDI notes of the pad grid to the MIDI note to play
/// (-1 means "no note").
pub type NoteMatrix = [i32; 128];

/// A hook which translates a note matrix to the note layout of a specific controller grid (former
/// overrides of `Scales.translateMatrixToGrid` in e.g. `LaunchpadScales`, `FireScales`,
/// `APCScales`, `APCminiScales`, `SLMkIIIScales`).
pub type GridTranslator = Box<dyn Fn(&NoteMatrix) -> NoteMatrix>;

/// The scale settings which are stored in the configuration (former parameters of
/// `Scales.updateScaleProperties (Configuration)`). Compare it with the configuration values and
/// write the changed ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaleProperties {
    /// The name of the scale (configuration setting "Scale").
    pub scale: &'static str,
    /// The name of the base note (configuration setting "Base", one of [`Scales::BASES`]).
    pub scale_base: &'static str,
    /// True if not chromatic (configuration setting "In Key").
    pub scale_in_key: bool,
    /// The name of the scale layout (configuration setting "Layout").
    pub scale_layout: &'static str,
}

/// Helper for applying scales to a rows x columns pad grid. There are different layouts available
/// including transpositions by octaves. Furthermore, specific matrices are supported for a drum, a
/// piano and a sequencer layout.
///
/// Controllers whose pads do not send the MIDI notes `start_note..end_note` in a row-major order
/// can install a [`GridTranslator`] with [`Scales::set_grid_translator`] (or
/// [`Scales::with_grid_translator`]) which is applied by [`Scales::translate_matrix_to_grid`].
/// [`Scales::table_grid_translator`] and [`Scales::offset_grid_translator`] create the typical
/// translators.
pub struct Scales {
    selected_scale: Scale,
    scale_offset: usize,
    scale_layout: ScaleLayout,
    orientation: Orientation,
    chromatic_on: bool,
    scale_shift: i32,
    semitone_shift: i32,
    octave: i32,
    default_octave: i32,
    drum_offset: i32,
    drum_default_offset: i32,
    piano_octave: i32,
    start_note: i32,
    end_note: i32,
    num_columns: i32,
    num_rows: i32,

    drum_matrix: Vec<i32>,
    drum_note_start: i32,
    drum_note_end: i32,

    scale_grids: Vec<ScaleGrid>,
    chord_grids: Vec<ChordGrid>,
    value_changer: ValueChanger,
    grid_translator: Option<GridTranslator>,
}

impl fmt::Debug for Scales {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scales")
            .field("scale", &self.selected_scale)
            .field("scale_offset", &self.scale_offset)
            .field("scale_layout", &self.scale_layout)
            .field("chromatic", &self.chromatic_on)
            .field("octave", &self.octave)
            .field("drum_offset", &self.drum_offset)
            .field("piano_octave", &self.piano_octave)
            .field("start_note", &self.start_note)
            .field("end_note", &self.end_note)
            .field("num_columns", &self.num_columns)
            .field("num_rows", &self.num_rows)
            .field("has_grid_translator", &self.grid_translator.is_some())
            .finish()
    }
}

/// The drum grid matrix.
#[rustfmt::skip]
const DRUM_MATRIX: [i32; 64] = [
     0,  1,  2,  3, -1, -1, -1, -1,
     4,  5,  6,  7, -1, -1, -1, -1,
     8,  9, 10, 11, -1, -1, -1, -1,
    12, 13, 14, 15, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
];

/// The matrix for the piano layout.
#[rustfmt::skip]
const PIANO_MATRIX: [[i32; 7]; 2] = [
    [  0,  2,  4,  5,  7,  9, 11 ],
    [ -1,  1,  3, -1,  6,  8, 10 ],
];

const DRUM_DEFAULT_OFFSET: i32 = 16;

impl Scales {
    /// The lowest drum note.
    pub const DRUM_NOTE_LOWER: i32 = 4;
    /// The highest drum note.
    pub const DRUM_NOTE_UPPER: i32 = 100;

    /// The names of notes.
    pub const NOTE_NAMES: [&'static str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

    /// The names of the base notes (circle of fifths order, index = scale offset index).
    pub const BASES: [&'static str; 12] = ["C", "G", "D", "A", "E", "B", "F", "Bb", "Eb", "Ab", "Db", "Gb"];

    /// The semi-tone offsets of the base notes (same order as [`Scales::BASES`]).
    pub const OFFSETS: [i32; 12] = [0, 7, 2, 9, 4, 11, 5, 10, 3, 8, 1, 6];

    /// The MIDI note at which the drum grid starts.
    pub const DRUM_NOTE_START: i32 = 36;
    /// The MIDI note at which the drum grid ends.
    pub const DRUM_NOTE_END: i32 = 100;

    /// Color index when the pad is turned off.
    pub const SCALE_COLOR_OFF: &'static str = "SCALE_COLOR_OFF";
    /// Color index when the pad is the base note of the scale.
    pub const SCALE_COLOR_OCTAVE: &'static str = "SCALE_COLOR_OCTAVE";
    /// Color index when the pad is a note of the scale.
    pub const SCALE_COLOR_NOTE: &'static str = "SCALE_COLOR_NOTE";
    /// Color index when the pad is a note which is not part of the scale.
    pub const SCALE_COLOR_OUT_OF_SCALE: &'static str = "SCALE_COLOR_OUT_OF_SCALE";

    /// Number of octaves which can be transposed up and down.
    pub const OCTAVE_RANGE: i32 = 4;
    /// Number of octaves which can be transposed up and down in the piano layout.
    pub const PIANO_OCTAVE_RANGE: i32 = 3;
    /// Number of upper drum octave limit.
    pub const DRUM_OCTAVE_UPPER: i32 = 4;
    /// Number of lower drum octave limit.
    pub const DRUM_OCTAVE_LOWER: i32 = -2;

    /// Constructor with a default octave of 0.
    ///
    /// * `value_changer` - used for [`Scales::change_scale`]; a clone is stored
    /// * `start_note` - the first MIDI note of the pad grid
    /// * `end_note` - the last MIDI note of the pad grid + 1
    /// * `num_columns`, `num_rows` - the size of the pad grid
    pub fn new(value_changer: &ValueChanger, start_note: i32, end_note: i32, num_columns: i32, num_rows: i32) -> Self {
        Self::with_default_octave(value_changer, start_note, end_note, num_columns, num_rows, 0)
    }

    /// Constructor.
    ///
    /// * `value_changer` - used for [`Scales::change_scale`]; a clone is stored
    /// * `start_note` - the first MIDI note of the pad grid
    /// * `end_note` - the last MIDI note of the pad grid + 1
    /// * `num_columns`, `num_rows` - the size of the pad grid
    /// * `default_octave` - the default octave
    pub fn with_default_octave(value_changer: &ValueChanger, start_note: i32, end_note: i32, num_columns: i32, num_rows: i32, default_octave: i32) -> Self {
        let mut scales = Scales {
            selected_scale: Scale::Major,
            scale_offset: 0,
            scale_layout: ScaleLayout::FourthUp,
            orientation: Orientation::OrientUp,
            chromatic_on: false,
            scale_shift: 3,
            semitone_shift: 5,
            octave: default_octave,
            default_octave,
            drum_offset: Self::DRUM_NOTE_START,
            drum_default_offset: DRUM_DEFAULT_OFFSET,
            piano_octave: 0,
            start_note,
            end_note,
            num_columns,
            num_rows,
            drum_matrix: DRUM_MATRIX.to_vec(),
            drum_note_start: Self::DRUM_NOTE_START,
            drum_note_end: Self::DRUM_NOTE_END,
            scale_grids: Vec::new(),
            chord_grids: Vec::new(),
            value_changer: value_changer.clone(),
            grid_translator: None,
        };
        scales.generate_matrices();
        scales
    }

    /// Builder style variant of [`Scales::set_grid_translator`].
    pub fn with_grid_translator(mut self, translator: GridTranslator) -> Self {
        self.grid_translator = Some(translator);
        self
    }

    /// Set (or remove) the hook which is used by [`Scales::translate_matrix_to_grid`].
    pub fn set_grid_translator(&mut self, translator: Option<GridTranslator>) {
        self.grid_translator = translator;
    }

    /// Creates a translator which moves the entry `first_note + i` of a matrix to the index
    /// `table[i]` of an otherwise empty matrix (e.g. `LaunchpadScales` with first note 36 and
    /// `LaunchpadPadGrid.TRANSLATE_MATRIX`, `FireScales` with `TRANSLATE_16x4_MATRIX`).
    pub fn table_grid_translator(first_note: i32, table: Vec<i32>) -> GridTranslator {
        Box::new(move |matrix: &NoteMatrix| {
            let mut grid_matrix = Self::empty_matrix();
            for (i, &target) in table.iter().enumerate() {
                grid_matrix[target as usize] = matrix[first_note as usize + i];
            }
            grid_matrix
        })
    }

    /// Creates a translator which moves the entries `first_note..end_note` of a matrix to the
    /// indices `0..end_note-first_note` of an otherwise empty matrix (e.g. `APCminiScales` with
    /// 36..100, `APCScales` with 36..76).
    pub fn offset_grid_translator(first_note: i32, end_note: i32) -> GridTranslator {
        Box::new(move |matrix: &NoteMatrix| {
            let mut grid_matrix = Self::empty_matrix();
            for i in first_note..end_note {
                grid_matrix[(i - first_note) as usize] = matrix[i as usize];
            }
            grid_matrix
        })
    }

    /// Get the currently selected scale.
    pub fn scale(&self) -> Scale {
        self.selected_scale
    }

    /// Set the scale.
    pub fn set_scale(&mut self, scale: Scale) {
        self.selected_scale = scale;
    }

    /// Set the scale by its name. Unknown names are ignored (Java would set `null`).
    pub fn set_scale_by_name(&mut self, scale_name: &str) {
        if let Some(scale) = Scale::by_name(scale_name) {
            self.selected_scale = scale;
        }
    }

    /// Change the scale value with a relative knob value.
    pub fn change_scale(&mut self, control: i32) {
        let index = self.value_changer.change_value_with(control, self.selected_scale.ordinal() as i32, -100.0, Scale::ALL.len() as i32);
        self.selected_scale = Scale::ALL[index as usize];
    }

    /// Returns true if there is a previous scale to select.
    pub fn has_prev_scale(&self) -> bool {
        self.selected_scale.ordinal() > 0
    }

    /// Returns true if there is a next scale to select.
    pub fn has_next_scale(&self) -> bool {
        self.selected_scale.ordinal() < Scale::ALL.len() - 1
    }

    /// Select the previous scale.
    pub fn prev_scale(&mut self) {
        self.selected_scale = Scale::ALL[self.selected_scale.ordinal().saturating_sub(1)];
    }

    /// Select the next scale.
    pub fn next_scale(&mut self) {
        self.selected_scale = Scale::ALL[(self.selected_scale.ordinal() + 1).min(Scale::ALL.len() - 1)];
    }

    /// Returns true if there is a previous scale offset to select.
    pub fn has_prev_scale_offset(&self) -> bool {
        self.scale_offset > 0
    }

    /// Returns true if there is a next scale offset to select.
    pub fn has_next_scale_offset(&self) -> bool {
        self.scale_offset < Self::OFFSETS.len() - 1
    }

    /// Select the previous scale offset.
    pub fn prev_scale_offset(&mut self) {
        self.set_scale_offset_by_index(self.scale_offset as i32 - 1);
    }

    /// Select the next scale offset.
    pub fn next_scale_offset(&mut self) {
        self.set_scale_offset_by_index(self.scale_offset as i32 + 1);
    }

    /// Get the base note offset (semitones, 0-11) to use for the current scale.
    pub fn scale_offset(&self) -> i32 {
        Self::OFFSETS[self.scale_offset]
    }

    /// Get the index of the base note (offset) to use for the current scale (index into
    /// [`Scales::BASES`]).
    pub fn scale_offset_index(&self) -> usize {
        self.scale_offset
    }

    /// Set the base note (offset) to use for the current scale by its index. Clamped to 0-11.
    pub fn set_scale_offset_by_index(&mut self, scale_offset_index: i32) {
        self.scale_offset = scale_offset_index.clamp(0, Self::OFFSETS.len() as i32 - 1) as usize;
    }

    /// Set the base note (offset) to use for the current scale by its name (e.g. "G"). Unknown
    /// names are ignored.
    pub fn set_scale_offset_by_name(&mut self, scale_offset_name: &str) {
        if let Some(index) = Self::BASES.iter().position(|&b| b == scale_offset_name) {
            self.scale_offset = index;
        }
    }

    /// Get the scale layout.
    pub fn scale_layout(&self) -> ScaleLayout {
        self.scale_layout
    }

    /// Set the scale layout. Also sets the orientation and the play shift and regenerates the
    /// matrices.
    pub fn set_scale_layout(&mut self, scale_layout: ScaleLayout) {
        self.scale_layout = scale_layout;
        self.orientation = if scale_layout.ordinal().is_multiple_of(2) { Orientation::OrientUp } else { Orientation::OrientRight };
        match scale_layout {
            ScaleLayout::FourthUp | ScaleLayout::FourthRight => self.set_play_shift(3, 5),
            ScaleLayout::ThirdUp | ScaleLayout::ThirdRight => self.set_play_shift(2, 4),
            ScaleLayout::SequentUp => self.set_play_shift(self.num_rows, self.num_rows),
            ScaleLayout::SequentRight => self.set_play_shift(self.num_columns, self.num_columns),
            ScaleLayout::EightUp | ScaleLayout::EightRight | ScaleLayout::EightUpCenter | ScaleLayout::EightRightCenter => self.set_play_shift(7, 12),
            // Note, scaleShift is dynamically determined by ScaleGrid depending on scale. It isn't
            // calculated here because scale could change without a subsequent layout change to
            // refresh the computed value.
            ScaleLayout::StaggeredUp | ScaleLayout::StaggeredRight => self.set_play_shift(0, 5),
        }
    }

    /// Set the scale layout by its name. Unknown names are ignored (Java would fail).
    pub fn set_scale_layout_by_name(&mut self, scale_layout_name: &str) {
        if let Some(layout) = ScaleLayout::by_name(scale_layout_name) {
            self.set_scale_layout(layout);
        }
    }

    /// Returns true if there is a previous scale layout to select.
    pub fn has_prev_scale_layout(&self) -> bool {
        self.scale_layout.ordinal() > 0
    }

    /// Returns true if there is a next scale layout to select.
    pub fn has_next_scale_layout(&self) -> bool {
        self.scale_layout.ordinal() < ScaleLayout::ALL.len() - 1
    }

    /// Select the previous scale layout.
    ///
    /// Note: as in the Java version this only changes the stored layout but does not update the
    /// orientation and matrices; call [`Scales::set_scale_layout`] for that.
    pub fn prev_scale_layout(&mut self) {
        self.scale_layout = ScaleLayout::ALL[self.scale_layout.ordinal().saturating_sub(1)];
    }

    /// Select the next scale layout.
    ///
    /// Note: as in the Java version this only changes the stored layout but does not update the
    /// orientation and matrices; call [`Scales::set_scale_layout`] for that.
    pub fn next_scale_layout(&mut self) {
        self.scale_layout = ScaleLayout::ALL[(self.scale_layout.ordinal() + 1).min(ScaleLayout::ALL.len() - 1)];
    }

    /// Dis-/enable chromatic mode.
    pub fn set_chromatic(&mut self, enable: bool) {
        self.chromatic_on = enable;
    }

    /// Toggle the chromatic setting.
    pub fn toggle_chromatic(&mut self) {
        self.chromatic_on = !self.chromatic_on;
    }

    /// True if chromatic.
    pub fn is_chromatic(&self) -> bool {
        self.chromatic_on
    }

    /// Sets the octave offset. Clamped to +/- [`Scales::OCTAVE_RANGE`].
    pub fn set_octave(&mut self, octave: i32) {
        self.octave = octave.clamp(-Self::OCTAVE_RANGE, Self::OCTAVE_RANGE);
    }

    /// Get the octave offset.
    pub fn octave(&self) -> i32 {
        self.octave
    }

    /// Increase the octave offset by 1.
    pub fn inc_octave(&mut self) {
        self.set_octave(self.octave + 1);
    }

    /// Decrease the octave offset by 1.
    pub fn dec_octave(&mut self) {
        self.set_octave(self.octave - 1);
    }

    /// Resets the octave offset to the default octave.
    pub fn reset_octave(&mut self) {
        self.set_octave(self.default_octave);
    }

    /// Get the default octave.
    pub fn default_octave(&self) -> i32 {
        self.default_octave
    }

    /// Get the current offset of the drum grid.
    pub fn drum_offset(&self) -> i32 {
        self.drum_offset
    }

    /// Returns true if the drum octave can be decreased.
    pub fn can_scroll_drum_octave_down(&self) -> bool {
        self.drum_offset - self.drum_default_offset >= Self::DRUM_NOTE_LOWER
    }

    /// Returns true if the drum octave can be increased.
    pub fn can_scroll_drum_octave_up(&self) -> bool {
        self.drum_offset + self.drum_default_offset <= Self::DRUM_NOTE_UPPER
    }

    /// Increases the drum layout by the default drum offset.
    pub fn inc_drum_octave(&mut self) {
        self.inc_drum_offset(self.drum_default_offset);
    }

    /// Decreases the drum layout by the default drum offset.
    pub fn dec_drum_octave(&mut self) {
        self.dec_drum_offset(self.drum_default_offset);
    }

    /// Resets the offset of the drum layout to the first drum note.
    pub fn reset_drum_octave(&mut self) {
        self.drum_offset = self.drum_note_start;
    }

    /// Increases the drum layout by the given offset (limited to [`Scales::DRUM_NOTE_UPPER`]).
    pub fn inc_drum_offset(&mut self, offset: i32) {
        self.drum_offset = Self::DRUM_NOTE_UPPER.min(self.drum_offset + offset);
    }

    /// Decreases the drum layout by the given offset (limited to [`Scales::DRUM_NOTE_LOWER`]).
    pub fn dec_drum_offset(&mut self, offset: i32) {
        self.drum_offset = Self::DRUM_NOTE_LOWER.max(self.drum_offset - offset);
    }

    /// Set the default value for de-/increasing the drum offset.
    pub fn set_drum_default_offset(&mut self, drum_default_offset: i32) {
        self.drum_default_offset = drum_default_offset;
    }

    /// Get the default value for de-/increasing the drum offset.
    pub fn drum_default_offset(&self) -> i32 {
        self.drum_default_offset
    }

    /// Sets the octave offset for the piano layout. Clamped to +/- [`Scales::PIANO_OCTAVE_RANGE`].
    pub fn set_piano_octave(&mut self, octave: i32) {
        self.piano_octave = octave.clamp(-Self::PIANO_OCTAVE_RANGE, Self::PIANO_OCTAVE_RANGE);
    }

    /// Get the octave offset for the piano layout.
    pub fn piano_octave(&self) -> i32 {
        self.piano_octave
    }

    /// Increases the piano layout by 1 octave.
    pub fn inc_piano_octave(&mut self) {
        self.set_piano_octave(self.piano_octave + 1);
    }

    /// Decreases the piano layout by 1 octave.
    pub fn dec_piano_octave(&mut self) {
        self.set_piano_octave(self.piano_octave - 1);
    }

    /// Sets the number of scale and semitone steps that the notes in the next rows are shifted
    /// (e.g. 4) and regenerates the matrices.
    pub fn set_play_shift(&mut self, scale_shift: i32, semitone_shift: i32) {
        self.scale_shift = scale_shift;
        self.semitone_shift = semitone_shift;
        self.generate_matrices();
    }

    /// Get the number of scale steps that the notes in the next rows are shifted (e.g. 4).
    pub fn scale_shift(&self) -> i32 {
        self.scale_shift
    }

    /// Get the number of semitones the notes in the next rows are shifted in chromatic mode.
    pub fn semitone_shift(&self) -> i32 {
        self.semitone_shift
    }

    /// Get the color ID (one of the `SCALE_COLOR_*` constants) for the given pad respecting the
    /// note map. `note` is the index into the note map (the pad's MIDI note), -1 gives
    /// [`Scales::SCALE_COLOR_OFF`].
    pub fn color(&self, note_map: &[i32], note: i32) -> &'static str {
        if note == -1 {
            return Self::SCALE_COLOR_OFF;
        }
        let midi_note = note_map[note as usize];
        if midi_note == -1 {
            return Self::SCALE_COLOR_OFF;
        }
        let note_in_octave = self.to_note_in_octave(midi_note);
        if note_in_octave == 0 {
            return Self::SCALE_COLOR_OCTAVE;
        }
        if !self.is_chromatic() {
            return Self::SCALE_COLOR_NOTE;
        }
        if self.is_in_scale(note_in_octave) {
            Self::SCALE_COLOR_NOTE
        } else {
            Self::SCALE_COLOR_OUT_OF_SCALE
        }
    }

    /// Convert the MIDI note (0-127) to the note in an octave (0-11). Respects the currently
    /// active base note.
    pub fn to_note_in_octave(&self, midi_note: i32) -> i32 {
        // Add 12 to prevent negative values
        (12 + midi_note - Self::OFFSETS[self.scale_offset]) % 12
    }

    /// Test if the note (0-11, relative to the base note) is part of the selected scale.
    pub fn is_in_scale(&self, note_in_octave: i32) -> bool {
        self.selected_scale.intervals().contains(&note_in_octave)
    }

    /// Get the MIDI note which is the closest in the active scale.
    pub fn nearest_note_in_scale(&self, midi_note: i32) -> i32 {
        let note_in_octave = self.to_note_in_octave(midi_note);

        let mut diff = 12;
        let mut result_note_in_octave = 0;
        for &interval in self.selected_scale.intervals() {
            let new_diff = (interval - note_in_octave).abs();
            if new_diff < diff {
                diff = new_diff;
                result_note_in_octave = interval;
            }
            if diff == 0 {
                break;
            }
        }

        let octaves = midi_note / 12 * 12;
        octaves + (result_note_in_octave + Self::OFFSETS[self.scale_offset]) % 12
    }

    /// Get the index of the note in the selected scale, -1 if the note is out of scale.
    pub fn scale_index(&self, midi_note: i32) -> i32 {
        let note_in_octave = self.to_note_in_octave(midi_note);
        self.selected_scale.intervals().iter().position(|&i| i == note_in_octave).map_or(-1, |p| p as i32)
    }

    /// Calculate the thirds on top of the given MIDI note. Respects the current octave, scale and
    /// scale base. Returns the additional 2 notes or an empty vector if the base note is out of
    /// the scale.
    pub fn third_chord(&self, base_note: i32) -> Vec<i32> {
        self.chord(base_note, &[3, 5])
    }

    /// Calculate the additional notes of a chord by adding the given (scale step) intervals, e.g.
    /// `[3, 5]` for a triad. Respects the current octave, scale and scale base (the octave of
    /// `base_note` is ignored, the notes are relative to the start note and octave). Returns the
    /// additional notes, excluding the base note, or an empty vector if the base note is out of
    /// the scale.
    pub fn chord(&self, base_note: i32, added_intervals: &[i32]) -> Vec<i32> {
        let scale_index = self.scale_index(base_note);
        if scale_index < 0 {
            return Vec::new();
        }

        let intervals = self.selected_scale.intervals();
        let len = intervals.len() as i32;
        let base_offset = self.start_note + Self::OFFSETS[self.scale_offset];
        added_intervals
            .iter()
            .map(|&added| {
                let note_index = scale_index + added - 1;
                let octave_note = intervals[(note_index % len) as usize];
                base_offset + (self.octave + note_index / len) * 12 + octave_note
            })
            .collect()
    }

    /// Get the active note matrix (scale or chromatic, depending on the chromatic setting).
    pub fn note_matrix(&self) -> NoteMatrix {
        self.note_matrix_from(self.active_matrix())
    }

    /// Get a note matrix from the given scale matrix (e.g. [`Scales::active_chord_matrix`]). The
    /// matrix must have at least `end_note - start_note` entries; base note, start note and
    /// octave are added.
    pub fn note_matrix_from(&self, matrix: &[i32]) -> NoteMatrix {
        let mut note_map = Self::empty_matrix();
        for note in self.start_note..self.end_note {
            let n = matrix[(note - self.start_note) as usize] + Self::OFFSETS[self.scale_offset] + self.start_note + self.octave * 12;
            note_map[note as usize] = if !(0..=127).contains(&n) { -1 } else { n };
        }
        note_map
    }

    /// Get the active sequencer matrix with `length` entries starting at `note_offset`. In
    /// chromatic mode the notes are consecutive, otherwise they are the scale notes.
    pub fn sequencer_matrix(&self, length: usize, note_offset: i32) -> Vec<i32> {
        if self.is_chromatic() {
            return (0..length as i32).map(|note| note_offset + note).collect();
        }

        let intervals = self.selected_scale.intervals();
        let len = intervals.len() as i32;

        let note_in_octave = note_offset % 12;
        let so = self.selected_scale.index_in_scale(note_in_octave).max(0);
        let no = note_offset / 12 * 12;

        (0..length as i32)
            .map(|note| {
                let index = so + note;
                let oct = index / len * 12;
                let n = Self::OFFSETS[self.scale_offset] + intervals[(index % len) as usize] + no + oct;
                if !(0..=127).contains(&n) {
                    -1
                } else {
                    n
                }
            })
            .collect()
    }

    /// Get the piano matrix for the given number of rows and columns. Each 2 rows form a piano
    /// keyboard (white keys in the lower, black keys in the upper row).
    pub fn piano_matrix(&self, rows: i32, columns: i32) -> NoteMatrix {
        let mut octave_offset = 3 + self.piano_octave;
        let mut counter = self.start_note;
        let row_offset = columns / 7;

        let mut note_map = Self::empty_matrix();

        for row in 0..rows {
            for col in 0..columns {
                let ns = PIANO_MATRIX[(row % 2) as usize][(col % 7) as usize];
                if ns >= 0 && counter < 128 {
                    note_map[counter as usize] = (ns + (octave_offset + col / 7) * 12).min(127);
                }
                counter += 1;
            }

            if row % 2 == 1 {
                octave_offset += row_offset;
            }
        }

        note_map
    }

    /// Get a new empty matrix. All notes are off (-1).
    pub fn empty_matrix() -> NoteMatrix {
        [-1; 128]
    }

    /// Get a new identity matrix. All notes are mapped to themselves.
    pub fn identity_matrix() -> NoteMatrix {
        std::array::from_fn(|i| i as i32)
    }

    /// Get the drum matrix (respects the drum offset).
    pub fn drum_matrix(&self) -> NoteMatrix {
        let mut note_map = Self::empty_matrix();
        for note in self.drum_note_start..self.drum_note_end {
            let ns = self.drum_matrix[(note - self.drum_note_start) as usize];
            let n = if ns == -1 { -1 } else { ns + self.drum_offset };
            note_map[note as usize] = if !(0..=127).contains(&n) { -1 } else { n };
        }
        note_map
    }

    /// Set a new drum matrix (relative pad offsets, -1 for off). It must have at least
    /// `drum_note_end - drum_note_start` entries.
    pub fn set_drum_matrix(&mut self, matrix: Vec<i32>) {
        self.drum_matrix = matrix;
    }

    /// Set the first drum note.
    pub fn set_drum_note_start(&mut self, drum_note_start: i32) {
        self.drum_note_start = drum_note_start;
    }

    /// Set the last drum note (+ 1).
    pub fn set_drum_note_end(&mut self, drum_note_end: i32) {
        self.drum_note_end = drum_note_end;
    }

    /// Create a text description for the current minimum and maximum notes of the scale,
    /// respecting offsets and octaves (e.g. "C1 to C5").
    pub fn range_text(&self) -> String {
        let matrix = self.active_matrix();
        let offset = Self::OFFSETS[self.scale_offset];
        format!("{} to {}", self.format_note(offset + matrix[0]), self.format_note(offset + matrix[matrix.len() - 1]))
    }

    /// Create a text description for the from and to note parameters.
    pub fn sequencer_range_text(from: i32, to: i32) -> String {
        format!("{} to {}", Self::format_note_and_octave(from, -3), Self::format_note_and_octave(to, -3))
    }

    /// Create a text description for the current offset of the drum layout (e.g. "Offset: 0 (C1)").
    pub fn drum_range_text(&self) -> String {
        let s = self.drum_offset;
        format!("Offset: {} ({})", s - self.drum_note_start, Self::format_drum_note(s))
    }

    /// Create a text description for the current minimum and maximum notes of the piano layout,
    /// respecting the octaves.
    pub fn piano_range_text(&self) -> String {
        format!("{} to {}", self.format_note(self.piano_octave * 12), self.format_note((self.piano_octave + 4) * 12))
    }

    /// Format a note as text, respecting the current octave.
    pub fn format_note(&self, note: i32) -> String {
        Self::format_note_and_octave(note, self.octave)
    }

    /// Format a drum note as text (MIDI note 36 = "C1").
    pub fn format_drum_note(note: i32) -> String {
        Self::format_note_and_octave(note, -3)
    }

    /// Formats a note with an octave offset, e.g. (60, -3) gives "C3".
    pub fn format_note_and_octave(note: i32, octave_offset: i32) -> String {
        format!("{}{}", Self::NOTE_NAMES[(note % 12).unsigned_abs() as usize], note / 12 + octave_offset + 1)
    }

    /// Get the matrix of the selected scale (scale or chromatic, `rows * columns` entries,
    /// relative semitone offsets).
    pub fn active_matrix(&self) -> &[i32] {
        let scale_grid = &self.scale_grids[self.selected_scale.ordinal()];
        if self.is_chromatic() {
            scale_grid.chromatic()
        } else {
            scale_grid.matrix()
        }
    }

    /// Get the chord matrix of the selected scale (`rows * columns` entries).
    pub fn active_chord_matrix(&self) -> &[i32] {
        self.chord_grids[self.selected_scale.ordinal()].matrix()
    }

    /// Translate the matrix for grids which do not send MIDI notes 36-100 (former overridable
    /// `translateMatrixToGrid`). Uses the installed [`GridTranslator`]; without one the matrix is
    /// returned unchanged.
    pub fn translate_matrix_to_grid(&self, matrix: &NoteMatrix) -> NoteMatrix {
        match &self.grid_translator {
            Some(translator) => translator(matrix),
            None => *matrix,
        }
    }

    /// Generate all matrices for all scales.
    fn generate_matrices(&mut self) {
        let rows = self.num_rows.max(0) as usize;
        let cols = self.num_columns.max(0) as usize;
        self.scale_grids = Scale::ALL
            .iter()
            .map(|&scale| ScaleGrid::new(scale, self.scale_layout, self.orientation, rows, cols, self.scale_shift, self.semitone_shift))
            .collect();
        self.chord_grids = Scale::ALL.iter().map(|&scale| ChordGrid::new(scale, rows, cols)).collect();
    }

    /// Get the first MIDI note of the pad grid.
    pub fn start_note(&self) -> i32 {
        self.start_note
    }

    /// Set the first MIDI note of the pad grid.
    pub fn set_start_note(&mut self, start_note: i32) {
        self.start_note = start_note;
    }

    /// Get the last MIDI note of the pad grid (+ 1).
    pub fn end_note(&self) -> i32 {
        self.end_note
    }

    /// Set the last MIDI note of the pad grid (+ 1).
    pub fn set_end_note(&mut self, end_note: i32) {
        self.end_note = end_note;
    }

    /// Get the number of columns of the pad grid.
    pub fn num_columns(&self) -> i32 {
        self.num_columns
    }

    /// Get the number of rows of the pad grid.
    pub fn num_rows(&self) -> i32 {
        self.num_rows
    }

    /// Get the scale settings to store in the configuration (replaces
    /// `updateScaleProperties (Configuration)`: the caller compares the values with the
    /// configuration and sets the changed ones).
    pub fn scale_properties(&self) -> ScaleProperties {
        ScaleProperties {
            scale: self.selected_scale.name(),
            scale_base: Self::BASES[self.scale_offset],
            scale_in_key: !self.chromatic_on,
            scale_layout: self.scale_layout.name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scales() -> Scales {
        Scales::new(&ValueChanger::twos_complement(128, 1), 36, 100, 8, 8)
    }

    #[test]
    fn major_fourth_up() {
        let s = scales();
        let m = s.note_matrix();
        assert_eq!(&m[36..44], &[36, 38, 40, 41, 43, 45, 47, 48]);
        assert_eq!(&m[44..52], &[41, 43, 45, 47, 48, 50, 52, 53]);
        assert_eq!(m[99], 84);
        assert_eq!(m[35], -1);
        assert_eq!(m[100], -1);
        assert_eq!(s.range_text(), "C1 to C5");
    }

    #[test]
    fn fourth_right_and_third() {
        let mut s = scales();
        s.set_scale_layout(ScaleLayout::FourthRight);
        let m = s.note_matrix();
        assert_eq!(m[36], 36);
        assert_eq!(m[37], 41);
        assert_eq!(m[44], 38);

        s.set_scale_layout(ScaleLayout::ThirdUp);
        assert_eq!((s.scale_shift(), s.semitone_shift()), (2, 4));
        assert_eq!(s.note_matrix()[44], 40);

        s.set_scale_layout(ScaleLayout::SequentUp);
        assert_eq!(s.scale_shift(), 8);
        assert_eq!(s.note_matrix()[44], 50);

        s.set_scale_layout_by_name("8th ^");
        assert_eq!(s.scale_layout(), ScaleLayout::EightUp);
        assert_eq!(s.note_matrix()[44], 48);
    }

    #[test]
    fn chromatic() {
        let mut s = scales();
        s.set_chromatic(true);
        let m = s.note_matrix();
        assert_eq!(&m[36..40], &[36, 37, 38, 39]);
        assert_eq!(m[44], 41);
        assert_eq!(s.color(&m, 36), Scales::SCALE_COLOR_OCTAVE);
        assert_eq!(s.color(&m, 37), Scales::SCALE_COLOR_OUT_OF_SCALE);
        assert_eq!(s.color(&m, 38), Scales::SCALE_COLOR_NOTE);
        assert_eq!(s.color(&m, 20), Scales::SCALE_COLOR_OFF);
        assert_eq!(s.color(&m, -1), Scales::SCALE_COLOR_OFF);
        s.toggle_chromatic();
        assert!(!s.is_chromatic());
        let m = s.note_matrix();
        assert_eq!(s.color(&m, 37), Scales::SCALE_COLOR_NOTE);
    }

    #[test]
    fn octave_and_base() {
        let mut s = scales();
        s.inc_octave();
        assert_eq!(s.note_matrix()[36], 48);
        s.set_octave(10);
        assert_eq!(s.octave(), 4);
        // 36 + 4 * 12 + 48 = 132 -> off
        assert_eq!(s.note_matrix()[99], -1);
        s.reset_octave();
        assert_eq!(s.octave(), 0);
        s.set_octave(-10);
        assert_eq!(s.octave(), -4);
        s.reset_octave();

        s.next_scale_offset();
        assert_eq!(s.scale_offset(), 7);
        assert_eq!(s.note_matrix()[36], 43);
        s.set_scale_offset_by_name("Bb");
        assert_eq!(s.scale_offset_index(), 7);
        assert_eq!(s.scale_offset(), 10);
        s.set_scale_offset_by_index(20);
        assert_eq!(s.scale_offset_index(), 11);
        assert!(!s.has_next_scale_offset());
        s.set_scale_offset_by_index(-1);
        assert!(!s.has_prev_scale_offset());
    }

    #[test]
    fn scale_selection() {
        let mut s = scales();
        assert!(!s.has_prev_scale());
        s.prev_scale();
        assert_eq!(s.scale(), Scale::Major);
        s.next_scale();
        assert_eq!(s.scale(), Scale::Minor);
        s.set_scale_by_name("Dorian");
        assert_eq!(s.scale(), Scale::Dorian);
        s.set_scale_by_name("does not exist");
        assert_eq!(s.scale(), Scale::Dorian);
        s.set_scale(Scale::Messiaen7);
        assert!(!s.has_next_scale());
        s.set_scale(Scale::Major);
        s.change_scale(20);
        assert_eq!(s.scale(), Scale::Dorian);
        s.change_scale(127);
        assert_eq!(s.scale(), Scale::Minor);
        s.change_scale(100);
        assert_eq!(s.scale(), Scale::Major);
    }

    #[test]
    fn drum_matrix() {
        let mut s = scales();
        let m = s.drum_matrix();
        assert_eq!(&m[36..44], &[36, 37, 38, 39, -1, -1, -1, -1]);
        assert_eq!(m[44], 40);
        assert_eq!(m[60], 48);
        assert_eq!(m[99], -1);
        assert_eq!(s.drum_range_text(), "Offset: 0 (C1)");
        s.inc_drum_octave();
        assert_eq!(s.drum_offset(), 52);
        assert_eq!(s.drum_matrix()[36], 52);
        s.dec_drum_offset(100);
        assert_eq!(s.drum_offset(), Scales::DRUM_NOTE_LOWER);
        assert!(!s.can_scroll_drum_octave_down());
        s.inc_drum_offset(200);
        assert_eq!(s.drum_offset(), Scales::DRUM_NOTE_UPPER);
        assert!(!s.can_scroll_drum_octave_up());
        s.reset_drum_octave();
        assert_eq!(s.drum_offset(), 36);
    }

    #[test]
    fn piano_matrix() {
        let mut s = scales();
        let m = s.piano_matrix(8, 8);
        assert_eq!(&m[36..44], &[36, 38, 40, 41, 43, 45, 47, 48]);
        assert_eq!(&m[44..52], &[-1, 37, 39, -1, 42, 44, 46, -1]);
        assert_eq!(m[52], 48);
        assert_eq!(s.piano_range_text(), "C1 to C5");
        s.inc_piano_octave();
        assert_eq!(s.piano_matrix(8, 8)[36], 48);
        s.set_piano_octave(9);
        assert_eq!(s.piano_octave(), 3);
    }

    #[test]
    fn sequencer_matrix() {
        let mut s = scales();
        assert_eq!(s.sequencer_matrix(8, 36), vec![36, 38, 40, 41, 43, 45, 47, 48]);
        assert_eq!(s.sequencer_matrix(3, 38), vec![38, 40, 41]);
        s.set_chromatic(true);
        assert_eq!(s.sequencer_matrix(4, 36), vec![36, 37, 38, 39]);
        assert_eq!(Scales::sequencer_range_text(36, 60), "C1 to C3");
    }

    #[test]
    fn chords_and_nearest() {
        let mut s = scales();
        assert_eq!(s.third_chord(36), vec![40, 43]);
        assert_eq!(s.third_chord(38), vec![41, 45]);
        assert_eq!(s.third_chord(37), Vec::<i32>::new());
        assert_eq!(s.chord(47, &[3, 5]), vec![50, 53]);
        assert_eq!(s.nearest_note_in_scale(61), 60);
        assert_eq!(s.nearest_note_in_scale(62), 62);
        assert_eq!(s.scale_index(41), 3);
        s.set_scale_offset_by_name("D");
        assert_eq!(s.to_note_in_octave(38), 0);
        assert_eq!(s.to_note_in_octave(36), 10);
        let chord = s.active_chord_matrix();
        assert_eq!(&chord[0..8], &[0, 2, 4, 5, 7, 9, 11, 12]);
        assert_eq!(s.note_matrix_from(chord)[44], 38);
    }

    #[test]
    fn formatting() {
        assert_eq!(Scales::format_note_and_octave(60, -3), "C3");
        assert_eq!(Scales::format_note_and_octave(61, 0), "Db6");
        assert_eq!(Scales::format_drum_note(36), "C1");
        assert_eq!(Scales::format_note_and_octave(-1, 0), "Db1");
    }

    #[test]
    fn matrices_and_translation() {
        let id = Scales::identity_matrix();
        assert_eq!(id[0], 0);
        assert_eq!(id[127], 127);
        assert!(Scales::empty_matrix().iter().all(|&n| n == -1));

        let mut s = scales();
        assert_eq!(s.translate_matrix_to_grid(&id), id);
        s.set_grid_translator(Some(Scales::offset_grid_translator(36, 100)));
        let t = s.translate_matrix_to_grid(&id);
        assert_eq!(t[0], 36);
        assert_eq!(t[63], 99);
        assert_eq!(t[64], -1);

        let table: Vec<i32> = (0..64).map(|i| 127 - i).collect();
        let s = scales().with_grid_translator(Scales::table_grid_translator(36, table));
        let t = s.translate_matrix_to_grid(&id);
        assert_eq!(t[127], 36);
        assert_eq!(t[64], 99);
        assert_eq!(t[0], -1);
    }

    #[test]
    fn properties() {
        let mut s = scales();
        s.set_scale(Scale::Minor);
        s.set_scale_offset_by_name("A");
        s.set_chromatic(true);
        s.set_scale_layout(ScaleLayout::StaggeredRight);
        assert_eq!(
            s.scale_properties(),
            ScaleProperties { scale: "Minor", scale_base: "A", scale_in_key: false, scale_layout: "Staggered >" }
        );
    }

    #[test]
    fn custom_drum_matrix() {
        let mut s = Scales::new(&ValueChanger::twos_complement(128, 1), 36, 76, 8, 5);
        s.set_drum_note_end(76);
        s.set_drum_matrix(DRUM_MATRIX[..40].iter().enumerate().map(|(i, &v)| if i < 24 { v } else { -1 }).collect());
        let m = s.drum_matrix();
        assert_eq!(m[52], 44);
        assert_eq!(m[60], -1);
        assert_eq!(m[76], -1);
        assert_eq!(s.note_matrix()[75], 69);
    }
}
