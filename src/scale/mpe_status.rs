// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! MPE status of the 16 MIDI channels (former `MPEStatus`).

/// Stores the currently playing note and the slide value of each of the 16 MIDI channels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpeStatus {
    notes: [i32; 16],
    slide_value: [i32; 16],
}

impl Default for MpeStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl MpeStatus {
    /// Constructor. No notes are playing (-1) and all slide values are 0.
    pub fn new() -> Self {
        MpeStatus { notes: [-1; 16], slide_value: [0; 16] }
    }

    /// Store the note status of a channel: the note if `is_on`, otherwise -1.
    pub fn handle_note(&mut self, channel: usize, note: i32, is_on: bool) {
        self.notes[channel] = if is_on { note } else { -1 };
    }

    /// Get the note playing on the channel or -1 if none.
    pub fn note_status(&self, channel: usize) -> i32 {
        self.notes[channel]
    }

    /// Get the slide value of the channel.
    pub fn slide_value(&self, channel: usize) -> i32 {
        self.slide_value[channel]
    }

    /// Set the slide value of the channel.
    pub fn set_slide_value(&mut self, channel: usize, slide_value: i32) {
        self.slide_value[channel] = slide_value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpe_status() {
        let mut s = MpeStatus::new();
        assert_eq!(s.note_status(3), -1);
        s.handle_note(3, 60, true);
        assert_eq!(s.note_status(3), 60);
        s.handle_note(3, 60, false);
        assert_eq!(s.note_status(3), -1);
        s.set_slide_value(15, 99);
        assert_eq!(s.slide_value(15), 99);
        assert_eq!(s.slide_value(0), 0);
    }
}
