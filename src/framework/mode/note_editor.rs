// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Stores the notes which are currently edited (former `INoteEditor`, `INoteEditorMode` and
//! `NoteEditor`).
//!
//! Views and modes which provide a note editor (former `instanceof INoteEditorMode` /
//! `INoteEditor`) return it from `View::note_editor(_mut)` / `Mode::note_editor(_mut)`.

use crate::daw::clip::NotePosition;

/// Identifies a cursor clip of the model by its number of steps (columns) and rows, see
/// [`crate::daw::model::Model::note_clip`]. Replaces the clip reference of the Java version.
pub type ClipKey = (usize, usize);

/// Stores the notes which are currently edited, e.g. by a note editor mode or view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteEditor {
    clip: Option<ClipKey>,
    notes: Vec<NotePosition>,
}

impl NoteEditor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the (key of the) clip which contains the edited notes.
    pub fn clip(&self) -> Option<ClipKey> {
        self.clip
    }

    /// Remove all notes from editing.
    pub fn clear_notes(&mut self) {
        self.notes.clear();
    }

    /// Set a note for editing. Removes all other edited notes.
    pub fn set_note(&mut self, clip: ClipKey, note_position: &NotePosition) {
        self.notes.clear();
        self.add_note(clip, note_position);
    }

    /// Add a note for editing.
    pub fn add_note(&mut self, clip: ClipKey, note_position: &NotePosition) {
        // Is the note already edited? Remove it.
        self.remove_note(clip, note_position);
        self.notes.push(*note_position);
    }

    /// Remove a note from editing. If the clip differs from the clip of the edited notes, all
    /// notes are removed.
    pub fn remove_note(&mut self, clip: ClipKey, note_position: &NotePosition) {
        if self.clip != Some(clip) {
            self.notes.clear();
            self.clip = Some(clip);
        }
        if let Some(index) = self.notes.iter().position(|n| n == note_position) {
            self.notes.remove(index);
        }
    }

    /// Is the note at the position edited?
    pub fn is_note_edited(&self, clip: ClipKey, note_position: &NotePosition) -> bool {
        self.clip == Some(clip) && self.notes.contains(note_position)
    }

    /// Get all edited notes.
    pub fn notes(&self) -> &[NotePosition] {
        &self.notes
    }

    /// Get the note positions which are edited by the parameter with the given index. The
    /// simple editor returns all notes; complex modes need to implement this themselves.
    pub fn note_positions(&self, _parameter_index: i32) -> &[NotePosition] {
        &self.notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_and_clip_change() {
        let mut editor = NoteEditor::new();
        let a = NotePosition::new(0, 1, 60);
        let b = NotePosition::new(0, 2, 62);
        editor.add_note((8, 128), &a);
        editor.add_note((8, 128), &b);
        assert_eq!(editor.notes(), &[a, b]);
        assert!(editor.is_note_edited((8, 128), &a));
        assert!(!editor.is_note_edited((16, 128), &a));

        // Adding again moves it to the end
        editor.add_note((8, 128), &a);
        assert_eq!(editor.notes(), &[b, a]);

        editor.set_note((8, 128), &b);
        assert_eq!(editor.notes(), &[b]);

        // A different clip clears the notes
        editor.add_note((16, 128), &a);
        assert_eq!(editor.notes(), &[a]);
        assert_eq!(editor.clip(), Some((16, 128)));

        editor.remove_note((16, 128), &a);
        assert!(editor.notes().is_empty());
    }
}
