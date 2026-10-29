// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Identifies the names of chords from a list of notes (former `framework.utils.ChordIdentifier`).

use crate::scale::Scales;

/// The known chords: name and pitch classes (sorted).
const CHORDS: &[(&str, &[i32])] = &[
    ("C Major", &[0, 4, 7]),
    ("C# Major", &[1, 5, 8]),
    ("D Major", &[2, 6, 9]),
    ("D# Major", &[3, 7, 10]),
    ("E Major", &[4, 8, 11]),
    ("F Major", &[0, 5, 9]),
    ("F# Major", &[1, 6, 10]),
    ("G Major", &[2, 7, 11]),
    ("G# Major", &[0, 3, 8]),
    ("A Major", &[1, 4, 9]),
    ("A# Major", &[2, 5, 10]),
    (" B Major", &[3, 6, 11]),
    ("C Minor", &[0, 3, 7]),
    ("C# Minor", &[1, 4, 8]),
    ("D Minor", &[2, 5, 9]),
    ("D# Minor", &[3, 6, 10]),
    ("E Minor", &[4, 7, 11]),
    ("F Minor", &[0, 5, 8]),
    ("F# Minor", &[1, 6, 9]),
    ("G Minor", &[2, 7, 10]),
    ("G# Minor", &[3, 8, 11]),
    ("A Minor", &[0, 4, 9]),
    ("A# Minor", &[1, 5, 10]),
    ("B Minor", &[2, 6, 11]),
    ("C Diminished", &[0, 3, 6]),
    ("C# Diminished", &[1, 4, 7]),
    ("D Diminished", &[2, 5, 8]),
    ("D# Diminished", &[3, 6, 9]),
    ("E Diminished", &[4, 7, 10]),
    ("F Diminished", &[5, 8, 11]),
    ("F# Diminished", &[0, 6, 9]),
    ("G Diminished", &[1, 7, 10]),
    ("G# Diminished", &[2, 8, 11]),
    ("A Diminished", &[0, 3, 9]),
    ("A# Diminished", &[1, 4, 10]),
    ("B Diminished", &[2, 5, 11]),
    ("C Augmented", &[0, 4, 8]),
    ("C# Augmented", &[1, 5, 9]),
    ("D Augmented", &[2, 6, 10]),
    ("D# Augmented", &[3, 7, 11]),
    ("E Augmented", &[0, 4, 8]),
    ("F Augmented", &[1, 5, 9]),
    ("F# Augmented", &[2, 6, 10]),
    ("G Augmented", &[3, 7, 11]),
    ("G# Augmented", &[0, 4, 8]),
    ("A Augmented", &[1, 5, 9]),
    ("A# Augmented", &[2, 6, 10]),
    ("B Augmented", &[3, 7, 11]),
    ("C Major 7th", &[0, 4, 7, 11]),
    ("C# Major 7th", &[0, 1, 5, 8]),
    ("D Major 7th", &[1, 2, 6, 9]),
    ("D# Major 7th", &[2, 3, 7, 10]),
    ("E Major 7th", &[3, 4, 8, 11]),
    ("F Major 7th", &[0, 4, 5, 9]),
    ("F# Major 7th", &[1, 5, 6, 10]),
    ("G Major 7th", &[2, 6, 7, 11]),
    ("G# Major 7th", &[0, 3, 7, 8]),
    ("A Major 7th", &[1, 4, 8, 9]),
    ("A# Major 7th", &[2, 5, 9, 10]),
    ("B Major 7th", &[3, 6, 10, 11]),
    ("C Minor 7th", &[0, 3, 7, 10]),
    ("C# Minor 7th", &[1, 4, 8, 11]),
    ("D Minor 7th", &[0, 2, 5, 9]),
    ("D# Minor 7th", &[1, 3, 6, 10]),
    ("E Minor 7th", &[2, 4, 7, 11]),
    ("F Minor 7th", &[0, 3, 5, 8]),
    ("F# Minor 7th", &[1, 4, 6, 9]),
    ("G Minor 7th", &[2, 5, 7, 10]),
    ("G# Minor 7th", &[3, 6, 8, 11]),
    ("A Minor 7th", &[0, 4, 7, 9]),
    ("A# Minor 7th", &[1, 5, 8, 10]),
    ("B Minor 7th", &[2, 6, 9, 11]),
    ("C Dominant 7th", &[0, 4, 7, 10]),
    ("C# Dominant 7th", &[1, 5, 8, 11]),
    ("D Dominant 7th", &[0, 2, 6, 9]),
    ("D# Dominant 7th", &[1, 3, 7, 10]),
    ("E Dominant 7th", &[2, 4, 8, 11]),
    ("F Dominant 7th", &[0, 3, 5, 9]),
    ("F# Dominant 7th", &[1, 4, 6, 10]),
    ("G Dominant 7th", &[2, 5, 7, 11]),
    ("G# Dominant 7th", &[0, 3, 6, 8]),
    ("A Dominant 7th", &[1, 4, 7, 9]),
    ("A# Dominant 7th", &[2, 5, 8, 10]),
    ("B Dominant 7th", &[3, 6, 9, 11]),
    ("C Sus2", &[0, 2, 7]),
    ("C# Sus2", &[1, 3, 8]),
    ("D Sus2", &[2, 4, 9]),
    ("D# Sus2", &[3, 5, 10]),
    ("E Sus2", &[4, 6, 11]),
    ("F Sus2", &[0, 5, 7]),
    ("F# Sus2", &[1, 6, 8]),
    ("G Sus2", &[2, 7, 9]),
    ("G# Sus2", &[3, 8, 10]),
    ("A Sus2", &[4, 9, 11]),
    ("A# Sus2", &[0, 5, 10]),
    ("B Sus2", &[1, 6, 11]),
    ("C Sus4", &[0, 5, 7]),
    ("C# Sus4", &[1, 6, 8]),
    ("D Sus4", &[2, 7, 9]),
    ("D# Sus4", &[3, 8, 10]),
    ("E Sus4", &[4, 9, 11]),
    ("F Sus4", &[0, 5, 10]),
    ("F# Sus4", &[1, 6, 11]),
    ("G Sus4", &[0, 2, 7]),
    ("G# Sus4", &[1, 3, 8]),
    ("A Sus4", &[2, 4, 9]),
    ("A# Sus4", &[3, 5, 10]),
    ("B Sus4", &[4, 6, 11]),
    ("C Major Add6", &[0, 4, 7, 9]),
    ("C# Major Add6", &[1, 5, 8, 10]),
    ("D Major Add6", &[2, 6, 9, 11]),
    ("D# Major Add6", &[0, 3, 7, 10]),
    ("E Major Add6", &[1, 4, 8, 11]),
    ("F Major Add6", &[0, 2, 5, 9]),
    ("F# Major Add6", &[1, 3, 6, 10]),
    ("G Major Add6", &[2, 4, 7, 11]),
    ("G# Major Add6", &[0, 3, 5, 8]),
    ("A Major Add6", &[1, 4, 6, 9]),
    ("A# Major Add6", &[2, 5, 7, 10]),
    ("B Major Add6", &[3, 6, 8, 11]),
    ("C Minor Add6", &[0, 3, 7, 9]),
    ("C# Minor Add6", &[1, 4, 8, 10]),
    ("D Minor Add6", &[2, 5, 9, 11]),
    ("D# Minor Add6", &[0, 3, 6, 10]),
    ("E Minor Add6", &[1, 4, 7, 11]),
    ("F Minor Add6", &[0, 2, 5, 8]),
    ("F# Minor Add6", &[1, 3, 6, 9]),
    ("G Minor Add6", &[2, 4, 7, 10]),
    ("G# Minor Add6", &[3, 5, 8, 11]),
    ("A Minor Add6", &[0, 4, 6, 9]),
    ("A# Minor Add6", &[1, 5, 7, 10]),
    ("B Minor Add6", &[2, 6, 8, 11]),
    ("C Major Add9", &[0, 2, 4, 7]),
    ("C# Major Add9", &[1, 3, 5, 8]),
    ("D Major Add9", &[2, 4, 6, 9]),
    ("D# Major Add9", &[3, 5, 7, 10]),
    ("E Major Add9", &[4, 6, 8, 11]),
    ("F Major Add9", &[0, 5, 7, 9]),
    ("F# Major Add9", &[1, 6, 8, 10]),
    ("G Major Add9", &[2, 7, 9, 11]),
    ("G# Major Add9", &[0, 3, 8, 10]),
    ("A Major Add9", &[1, 4, 9, 11]),
    ("A# Major Add9", &[0, 2, 5, 10]),
    ("B Major Add9", &[1, 3, 6, 11]),
    ("C Minor Add9", &[0, 2, 3, 7]),
    ("C# Minor Add9", &[1, 3, 4, 8]),
    ("D Minor Add9", &[2, 4, 5, 9]),
    ("D# Minor Add9", &[3, 5, 6, 10]),
    ("E Minor Add9", &[4, 6, 7, 11]),
    ("F Minor Add9", &[0, 5, 7, 8]),
    ("F# Minor Add9", &[1, 6, 8, 9]),
    ("G Minor Add9", &[2, 7, 9, 10]),
    ("G# Minor Add9", &[3, 8, 10, 11]),
    ("A Minor Add9", &[0, 4, 9, 11]),
    ("A# Minor Add9", &[0, 1, 5, 10]),
    ("B Minor Add9", &[1, 2, 6, 11]),
    ("C Major Add11", &[0, 4, 5, 7]),
    ("C# Major Add11", &[1, 5, 6, 8]),
    ("D Major Add11", &[2, 6, 7, 9]),
    ("D# Major Add11", &[3, 7, 8, 10]),
    ("E Major Add11", &[4, 8, 9, 11]),
    ("F Major Add11", &[0, 5, 9, 10]),
    ("F# Major Add11", &[1, 6, 10, 11]),
    ("G Major Add11", &[0, 2, 7, 11]),
    ("G# Major Add11", &[0, 1, 3, 8]),
    ("A Major Add11", &[1, 2, 4, 9]),
    ("A# Major Add11", &[2, 3, 5, 10]),
    ("B Major Add11", &[3, 4, 6, 11]),
    ("C Minor Add11", &[0, 3, 5, 7]),
    ("C# Minor Add11", &[1, 4, 6, 8]),
    ("D Minor Add11", &[2, 5, 7, 9]),
    ("D# Minor Add11", &[3, 6, 8, 10]),
    ("E Minor Add11", &[4, 7, 9, 11]),
    ("F Minor Add11", &[0, 5, 8, 10]),
    ("F# Minor Add11", &[1, 6, 9, 11]),
    ("G Minor Add11", &[0, 2, 7, 10]),
    ("G# Minor Add11", &[1, 3, 8, 11]),
    ("A Minor Add11", &[0, 2, 4, 9]),
    ("A# Minor Add11", &[1, 3, 5, 10]),
    ("B Minor Add11", &[2, 4, 6, 11]),
];

/// Returns the name of the chord for the given notes. If the chord is not known, the names of
/// the pitch classes are returned. Returns `None` if there are no notes.
pub fn identify_chord(notes: &[i32]) -> Option<String> {
    // Reduce notes to their pitch classes (0-11)
    let mut pitch_classes: Vec<i32> = notes.iter().map(|note| note % 12).filter(|&n| n != -1).collect();
    pitch_classes.sort_unstable();
    pitch_classes.dedup();

    // Note: the Java condition (length >= 3 || length <= 4) is always true
    if let Some((name, _)) = CHORDS.iter().find(|(_, chord)| *chord == pitch_classes.as_slice()) {
        return Some(name.to_string());
    }

    if pitch_classes.is_empty() {
        return None;
    }
    Some(pitch_classes.iter().map(|&pc| Scales::NOTE_NAMES[pc.rem_euclid(12) as usize]).collect::<Vec<_>>().join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify() {
        assert_eq!(identify_chord(&[60, 64, 67]).as_deref(), Some("C Major"));
        assert_eq!(identify_chord(&[60, 64, 67, 69]).as_deref(), Some("A Minor 7th"));
        assert_eq!(identify_chord(&[60, 62]).as_deref(), Some("C, D"));
        assert_eq!(identify_chord(&[-1]), None);
        assert_eq!(identify_chord(&[]), None);
    }
}
