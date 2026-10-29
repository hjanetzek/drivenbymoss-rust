// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A note as sent by Reaper (former `reaper.framework.daw.Note`).

/// A note of a MIDI clip (or a currently playing note) as sent by the C++ extension.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    is_selected: bool,
    is_muted: bool,
    start: f64,
    end: f64,
    channel: i32,
    pitch: i32,
    velocity: i32,
}

impl Note {
    /// Create a note.
    ///
    /// * `start`, `end` - The start and end of the note in beats (relative to the clip start)
    /// * `channel` - The MIDI channel (0-15)
    /// * `pitch` - The MIDI note (0-127)
    /// * `velocity` - The MIDI velocity (0-127)
    pub fn new(
        is_selected: bool,
        is_muted: bool,
        start: f64,
        end: f64,
        channel: i32,
        pitch: i32,
        velocity: i32,
    ) -> Self {
        Note {
            is_selected,
            is_muted,
            start,
            end,
            channel,
            pitch,
            velocity,
        }
    }

    /// True if the note is selected.
    pub fn is_selected(&self) -> bool {
        self.is_selected
    }

    /// True if the note is muted.
    pub fn is_muted(&self) -> bool {
        self.is_muted
    }

    /// The MIDI channel of the note.
    pub fn channel(&self) -> i32 {
        self.channel
    }

    /// The start of the note in beats.
    pub fn start(&self) -> f64 {
        self.start
    }

    /// The end of the note in beats.
    pub fn end(&self) -> f64 {
        self.end
    }

    /// The pitch (MIDI note) of the note.
    pub fn pitch(&self) -> i32 {
        self.pitch
    }

    /// The MIDI velocity of the note (0-127).
    pub fn velocity(&self) -> i32 {
        self.velocity
    }

    /// Parse notes from a string.
    ///
    /// The format is `selected:muted:start:end:channel:pitch:velocity;...` (one entry per note,
    /// separated by `;`; `selected` and `muted` are integers, > 0 means true). An empty string
    /// results in no notes. Malformed entries are skipped (Java threw a `NumberFormatException`
    /// which aborted the whole message).
    pub fn parse_notes(s: &str) -> Vec<Note> {
        s.trim().split(';').filter_map(Self::parse_note).collect()
    }

    fn parse_note(part: &str) -> Option<Note> {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        let mut it = part.split(':').map(str::trim);
        let is_selected = it.next()?.parse::<i32>().ok()? > 0;
        let is_muted = it.next()?.parse::<i32>().ok()? > 0;
        let start = it.next()?.parse::<f64>().ok()?;
        let end = it.next()?.parse::<f64>().ok()?;
        let channel = it.next()?.parse::<i32>().ok()?;
        let pitch = it.next()?.parse::<i32>().ok()?;
        let velocity = it.next()?.parse::<i32>().ok()?;
        Some(Note::new(
            is_selected,
            is_muted,
            start,
            end,
            channel,
            pitch,
            velocity,
        ))
    }
}

/// Parse notes from a string, see [`Note::parse_notes`].
pub fn parse_notes(s: &str) -> Vec<Note> {
    Note::parse_notes(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_and_multiple() {
        let notes = parse_notes("1:0:0.0:0.25:0:60:100;0:1:1.5:2:9:36:127;");
        assert_eq!(
            notes,
            vec![
                Note::new(true, false, 0.0, 0.25, 0, 60, 100),
                Note::new(false, true, 1.5, 2.0, 9, 36, 127),
            ]
        );
        assert_eq!(notes[0].pitch(), 60);
        assert_eq!(notes[0].velocity(), 100);
        assert_eq!(notes[1].channel(), 9);
        assert_eq!(notes[1].start(), 1.5);
        assert_eq!(notes[1].end(), 2.0);
        assert!(notes[1].is_muted());
        assert!(!notes[1].is_selected());
    }

    #[test]
    fn parse_empty_and_malformed() {
        assert!(parse_notes("").is_empty());
        assert!(parse_notes("  \n").is_empty());
        let notes = parse_notes("1:0:x:1:0:60:100;0:0:0:1:0:61:90;0:0:0:1");
        assert_eq!(notes, vec![Note::new(false, false, 0.0, 1.0, 0, 61, 90)]);
    }
}
