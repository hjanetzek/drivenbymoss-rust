// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Manages the pressed keys of a pad grid (former `framework.utils.KeyManager`).

use super::{NoteMatrix, Scales};

/// Manages the pressed keys of a pad grid and the note map which maps the pads (MIDI notes sent by
/// the controller grid) to the MIDI notes which are played.
///
/// Unlike the Java version it does not hold the model, scales and pad grid: methods which need
/// them take the required values as parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyManager {
    pressed_keys: [i32; 128],
    note_map: NoteMatrix,
}

impl Default for KeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyManager {
    /// Constructor. No keys are pressed and the note map is empty.
    pub fn new() -> Self {
        KeyManager { pressed_keys: [0; 128], note_map: Scales::empty_matrix() }
    }

    /// Clear all pressed keys.
    pub fn clear_pressed_keys(&mut self) {
        self.pressed_keys = [0; 128];
    }

    /// Returns true if the key (pad) is pressed.
    pub fn is_key_pressed(&self, key: i32) -> bool {
        self.pressed_keys[key as usize] > 0
    }

    /// Set the key (pad) as pressed with the velocity (0 = released).
    pub fn set_key_pressed(&mut self, key: i32, velocity: i32) {
        self.pressed_keys[key as usize] = velocity;
    }

    /// Set all pads which are mapped to the given MIDI note to the velocity.
    pub fn set_all_keys_pressed(&mut self, key: i32, velocity: i32) {
        for i in 0..128 {
            if self.note_map[i] == key {
                self.pressed_keys[i] = velocity;
            }
        }
    }

    /// Note observer callback (former `INoteObserver.call`): a note of the track with the index
    /// `track_index` was played. Marks all pads which are mapped to the note if the track is the
    /// selected track of the current track bank (`selected_track_index`, `None` if no track is
    /// selected).
    pub fn call(&mut self, track_index: i32, note: i32, velocity: i32, selected_track_index: Option<i32>) {
        if selected_track_index == Some(track_index) {
            self.set_all_keys_pressed(note, velocity);
        }
    }

    /// Get all pressed keys (pads).
    pub fn pressed_keys(&self) -> Vec<i32> {
        (0..128).filter(|&i| self.pressed_keys[i as usize] != 0).collect()
    }

    /// Returns true if there is at least one pressed key.
    pub fn has_pressed_keys(&self) -> bool {
        self.pressed_keys.iter().any(|&v| v != 0)
    }

    /// Get the MIDI note for a grid note. The caller translates the grid note before with the pad
    /// grid (Java: `map (padGrid.translateToGrid (note))`, or -1 if there is no pad grid).
    pub fn midi_note_from_grid(&self, translated_grid_note: i32) -> i32 {
        self.map(translated_grid_note)
    }

    /// Get the MIDI note for a grid note, using the given translation function of the pad grid
    /// (e.g. `|n| pad_grid.translate_to_grid(n)`).
    pub fn midi_note_from_grid_with(&self, note: i32, translate_to_grid: impl Fn(i32) -> i32) -> i32 {
        self.map(translate_to_grid(note))
    }

    /// Get the MIDI notes of all pressed keys.
    pub fn midi_notes_from_pressed_keys(&self) -> Vec<i32> {
        self.pressed_keys().into_iter().map(|k| self.map(k)).collect()
    }

    /// Map the key (pad) to the MIDI note to play. Returns -1 if the key is negative or not mapped.
    pub fn map(&self, note: i32) -> i32 {
        if note < 0 {
            -1
        } else {
            self.note_map[note as usize]
        }
    }

    /// Get the color ID (one of the `Scales::SCALE_COLOR_*` constants) of the pad.
    pub fn color(&self, scales: &Scales, pad: i32) -> &'static str {
        scales.color(&self.note_map, pad)
    }

    /// Set the note matrix which maps the pads to MIDI notes.
    pub fn set_note_matrix(&mut self, matrix: NoteMatrix) {
        self.note_map = matrix;
    }

    /// Get the note matrix which maps the pads to MIDI notes.
    pub fn note_matrix(&self) -> &NoteMatrix {
        &self.note_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::value_changer::ValueChanger;

    #[test]
    fn mapping_and_pressed_keys() {
        let scales = Scales::new(&ValueChanger::twos_complement(128, 1), 36, 100, 8, 8);
        let mut km = KeyManager::new();
        assert_eq!(km.map(36), -1);
        km.set_note_matrix(scales.note_matrix());
        assert_eq!(km.map(37), 38);
        assert_eq!(km.map(-1), -1);
        assert_eq!(km.midi_note_from_grid(44), 41);
        assert_eq!(km.midi_note_from_grid_with(8, |n| n + 36), 41);
        assert_eq!(km.color(&scales, 36), Scales::SCALE_COLOR_OCTAVE);
        assert_eq!(km.color(&scales, 37), Scales::SCALE_COLOR_NOTE);
        assert_eq!(km.color(&scales, 0), Scales::SCALE_COLOR_OFF);

        assert!(!km.has_pressed_keys());
        km.set_key_pressed(36, 100);
        assert!(km.is_key_pressed(36));
        assert_eq!(km.pressed_keys(), vec![36]);
        assert_eq!(km.midi_notes_from_pressed_keys(), vec![36]);

        // MIDI note 41 is on pads 39 (row 0) and 44 (row 1)
        km.clear_pressed_keys();
        km.call(2, 41, 90, Some(1));
        assert!(!km.has_pressed_keys());
        km.call(2, 41, 90, None);
        assert!(!km.has_pressed_keys());
        km.call(2, 41, 90, Some(2));
        assert_eq!(km.pressed_keys(), vec![39, 44]);
        assert_eq!(km.midi_notes_from_pressed_keys(), vec![41, 41]);
        km.set_all_keys_pressed(41, 0);
        assert!(!km.has_pressed_keys());
    }
}
