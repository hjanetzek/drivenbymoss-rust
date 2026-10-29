// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The position of a note in a clip (former `NotePosition`).

/// A position of a note in a clip. The position is described by its MIDI channel, the index of
/// the sequencer step (relative to the current edit page) and the MIDI note (= the row in the
/// clip).
///
/// The values are signed since views calculate with them freely (e.g. -1 as "before the first
/// row"); the clip treats out of range positions as empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NotePosition {
    /// The MIDI channel, 0-15.
    pub channel: i32,
    /// The step, relative to the edit page.
    pub step: i32,
    /// The MIDI note, 0-127.
    pub note: i32,
}

impl NotePosition {
    /// Create a new note position.
    pub const fn new(channel: i32, step: i32, note: i32) -> Self {
        NotePosition {
            channel,
            step,
            note,
        }
    }

    /// Get the MIDI channel.
    pub fn channel(&self) -> i32 {
        self.channel
    }

    /// Set the MIDI channel (0-15).
    pub fn set_channel(&mut self, channel: i32) {
        self.channel = channel;
    }

    /// Get the step.
    pub fn step(&self) -> i32 {
        self.step
    }

    /// Set the step.
    pub fn set_step(&mut self, step: i32) {
        self.step = step;
    }

    /// Get the MIDI note.
    pub fn note(&self) -> i32 {
        self.note
    }

    /// Set the MIDI note (0-127).
    pub fn set_note(&mut self, note: i32) {
        self.note = note;
    }

    /// Copies the values of the given note position into this note position.
    pub fn set(&mut self, other: &NotePosition) {
        *self = *other;
    }
}
