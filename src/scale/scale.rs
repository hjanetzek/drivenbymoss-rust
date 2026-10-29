// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! All supported musical scales (former `Scale`).

/// A musical scale defined by its intervals (semitone offsets from the base note, 0-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scale {
    /// Major: { 0, 2, 4, 5, 7, 9, 11 }.
    Major,
    /// Minor: { 0, 2, 3, 5, 7, 8, 10 }.
    Minor,
    /// Dorian: { 0, 2, 3, 5, 7, 9, 10 }.
    Dorian,
    /// Mixolydian: { 0, 2, 4, 5, 7, 9, 10 }.
    Mixolydian,
    /// Lydian: { 0, 2, 4, 6, 7, 9, 11 }.
    Lydian,
    /// Phrygian: { 0, 1, 3, 5, 7, 8, 10 }.
    Phrygian,
    /// Locrian: { 0, 1, 3, 5, 6, 8, 10 }.
    Locrian,
    /// Whole Tone: { 0, 2, 4, 6, 8, 10 }.
    WholeTone,
    /// Half-whole Dim.: { 0, 1, 3, 4, 6, 7, 9, 10 }.
    HalfWhole,
    /// Whole-half Dim.: { 0, 2, 3, 5, 6, 8, 9, 11 }.
    WholeHalf,
    /// Minor Blues: { 0, 3, 5, 6, 7, 10 }.
    MinorBlues,
    /// Minor Pentatonic: { 0, 3, 5, 7, 10 }.
    MinorPentatonic,
    /// Major Pentatonic: { 0, 2, 4, 7, 9 }.
    MajorPentatonic,
    /// Harmonic Minor: { 0, 2, 3, 5, 7, 8, 11 }.
    HarmonicMinor,
    /// Harmonic Major: { 0, 2, 4, 5, 7, 8, 11 }.
    HarmonicMajor,
    /// Dorian #4: { 0, 2, 3, 6, 7, 9, 10 }.
    DorianNr4,
    /// Phrygian Dominant: { 0, 1, 4, 5, 7, 8, 10 }.
    PhrygianDominant,
    /// Melodic Minor: { 0, 2, 3, 5, 7, 9, 11 }.
    MelodicMinor,
    /// Lydian Augmented: { 0, 2, 4, 6, 8, 9, 11 }.
    LydianAugmented,
    /// Lydian Dominant: { 0, 2, 4, 6, 7, 9, 10 }.
    LydianDominant,
    /// Super Locrian: { 0, 1, 3, 4, 6, 8, 10 }.
    SuperLocrian,
    /// Spanish: { 0, 1, 4, 5, 7, 9, 10 }.
    Spanish,
    /// 8-Tone Spanish: { 0, 1, 3, 4, 5, 6, 8, 10 }.
    Tone8Spanish,
    /// Bhairav: { 0, 1, 4, 5, 7, 8, 11 }.
    Bhairav,
    /// Hungarian Minor: { 0, 2, 3, 6, 7, 8, 11 }.
    HungarianMinor,
    /// Hirojoshi: { 0, 2, 3, 7, 8 }.
    Hirojoshi,
    /// In-Sen: { 0, 1, 5, 7, 10 }.
    InSen,
    /// Iwato: { 0, 1, 5, 6, 10 }.
    Iwato,
    /// Kumoi: { 0, 2, 3, 7, 9 }.
    Kumoi,
    /// Pelog: { 0, 1, 3, 4, 7, 8 }.
    Pelog,
    /// Pelog Selisir: { 0, 1, 3, 7, 8 }.
    PelogSelisir,
    /// Pelog Tembung: { 0, 1, 5, 7, 8 }.
    PelogTembung,
    /// Messiaen 3: { 0, 2, 3, 4, 6, 7, 8, 10, 11 }.
    Messiaen3,
    /// Messiaen 4: { 0, 1, 2, 5, 6, 7, 8, 11 }.
    Messiaen4,
    /// Messiaen 5: { 0, 1, 5, 6, 7, 11 }.
    Messiaen5,
    /// Messiaen 6: { 0, 2, 4, 5, 6, 8, 10, 11 }.
    Messiaen6,
    /// Messiaen 7: { 0, 1, 2, 3, 5, 6, 7, 8, 9, 11 }.
    Messiaen7,
}

impl Scale {
    /// All scales in their (Java) order.
    pub const ALL: &'static [Scale] = &[
        Scale::Major,
        Scale::Minor,
        Scale::Dorian,
        Scale::Mixolydian,
        Scale::Lydian,
        Scale::Phrygian,
        Scale::Locrian,
        Scale::WholeTone,
        Scale::HalfWhole,
        Scale::WholeHalf,
        Scale::MinorBlues,
        Scale::MinorPentatonic,
        Scale::MajorPentatonic,
        Scale::HarmonicMinor,
        Scale::HarmonicMajor,
        Scale::DorianNr4,
        Scale::PhrygianDominant,
        Scale::MelodicMinor,
        Scale::LydianAugmented,
        Scale::LydianDominant,
        Scale::SuperLocrian,
        Scale::Spanish,
        Scale::Tone8Spanish,
        Scale::Bhairav,
        Scale::HungarianMinor,
        Scale::Hirojoshi,
        Scale::InSen,
        Scale::Iwato,
        Scale::Kumoi,
        Scale::Pelog,
        Scale::PelogSelisir,
        Scale::PelogTembung,
        Scale::Messiaen3,
        Scale::Messiaen4,
        Scale::Messiaen5,
        Scale::Messiaen6,
        Scale::Messiaen7,
    ];

    /// The display names of all scales (same order as [`Scale::ALL`]).
    pub const NAMES: &'static [&'static str] = &[
        "Major",
        "Minor",
        "Dorian",
        "Mixolydian",
        "Lydian",
        "Phrygian",
        "Locrian",
        "Whole Tone",
        "Half-whole Dim.",
        "Whole-half Dim.",
        "Minor Blues",
        "Minor Pentatonic",
        "Major Pentatonic",
        "Harmonic Minor",
        "Harmonic Major",
        "Dorian #4",
        "Phrygian Dominant",
        "Melodic Minor",
        "Lydian Augmented",
        "Lydian Dominant",
        "Super Locrian",
        "Spanish",
        "8-Tone Spanish",
        "Bhairav",
        "Hungarian Minor",
        "Hirojoshi",
        "In-Sen",
        "Iwato",
        "Kumoi",
        "Pelog",
        "Pelog Selisir",
        "Pelog Tembung",
        "Messiaen 3",
        "Messiaen 4",
        "Messiaen 5",
        "Messiaen 6",
        "Messiaen 7",
    ];

    /// The ordinal of the scale in [`Scale::ALL`].
    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// Get the scale at the given index.
    pub fn from_ordinal(ordinal: usize) -> Option<Scale> {
        Self::ALL.get(ordinal).copied()
    }

    /// The display name of the scale (e.g. "Major").
    pub fn name(self) -> &'static str {
        Self::NAMES[self.ordinal()]
    }

    /// The intervals of the scale (semitone offsets from the base note).
    pub fn intervals(self) -> &'static [i32] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Scale::WholeTone => &[0, 2, 4, 6, 8, 10],
            Scale::HalfWhole => &[0, 1, 3, 4, 6, 7, 9, 10],
            Scale::WholeHalf => &[0, 2, 3, 5, 6, 8, 9, 11],
            Scale::MinorBlues => &[0, 3, 5, 6, 7, 10],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::HarmonicMajor => &[0, 2, 4, 5, 7, 8, 11],
            Scale::DorianNr4 => &[0, 2, 3, 6, 7, 9, 10],
            Scale::PhrygianDominant => &[0, 1, 4, 5, 7, 8, 10],
            Scale::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Scale::LydianAugmented => &[0, 2, 4, 6, 8, 9, 11],
            Scale::LydianDominant => &[0, 2, 4, 6, 7, 9, 10],
            Scale::SuperLocrian => &[0, 1, 3, 4, 6, 8, 10],
            Scale::Spanish => &[0, 1, 4, 5, 7, 9, 10],
            Scale::Tone8Spanish => &[0, 1, 3, 4, 5, 6, 8, 10],
            Scale::Bhairav => &[0, 1, 4, 5, 7, 8, 11],
            Scale::HungarianMinor => &[0, 2, 3, 6, 7, 8, 11],
            Scale::Hirojoshi => &[0, 2, 3, 7, 8],
            Scale::InSen => &[0, 1, 5, 7, 10],
            Scale::Iwato => &[0, 1, 5, 6, 10],
            Scale::Kumoi => &[0, 2, 3, 7, 9],
            Scale::Pelog => &[0, 1, 3, 4, 7, 8],
            Scale::PelogSelisir => &[0, 1, 3, 7, 8],
            Scale::PelogTembung => &[0, 1, 5, 7, 8],
            Scale::Messiaen3 => &[0, 2, 3, 4, 6, 7, 8, 10, 11],
            Scale::Messiaen4 => &[0, 1, 2, 5, 6, 7, 8, 11],
            Scale::Messiaen5 => &[0, 1, 5, 6, 7, 11],
            Scale::Messiaen6 => &[0, 2, 4, 5, 6, 8, 10, 11],
            Scale::Messiaen7 => &[0, 1, 2, 3, 5, 6, 7, 8, 9, 11],
        }
    }

    /// Test if the given key (0-11, relative to the base note) is part of the scale.
    pub fn is_in_scale(self, key: i32) -> bool {
        self.intervals().contains(&key)
    }

    /// Get the index of the note in the scale. The note is reduced to the octave first (`note % 12`).
    /// Returns -1 if the note is not part of the scale.
    pub fn index_in_scale(self, note: i32) -> i32 {
        let key = note % 12;
        self.intervals().iter().position(|&i| i == key).map_or(-1, |p| p as i32)
    }

    /// The display names of all scales.
    pub fn names() -> &'static [&'static str] {
        Self::NAMES
    }

    /// Get a scale by its display name.
    pub fn by_name(name: &str) -> Option<Scale> {
        Self::ALL.iter().copied().find(|s| s.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_table() {
        assert_eq!(Scale::ALL.len(), 37);
        assert_eq!(Scale::NAMES.len(), 37);
        for (i, s) in Scale::ALL.iter().enumerate() {
            assert_eq!(s.ordinal(), i);
            assert_eq!(Scale::by_name(s.name()), Some(*s));
            assert_eq!(s.intervals()[0], 0);
        }
        assert_eq!(Scale::Major.intervals(), &[0, 2, 4, 5, 7, 9, 11]);
        assert_eq!(Scale::Messiaen7.intervals().len(), 10);
        assert_eq!(Scale::by_name("Half-whole Dim."), Some(Scale::HalfWhole));
        assert_eq!(Scale::by_name("Unknown"), None);
    }

    #[test]
    fn index_in_scale() {
        assert!(Scale::Minor.is_in_scale(3));
        assert!(!Scale::Minor.is_in_scale(4));
        assert_eq!(Scale::Major.index_in_scale(16), 2);
        assert_eq!(Scale::Major.index_in_scale(1), -1);
        assert_eq!(Scale::MajorPentatonic.index_in_scale(9), 4);
    }
}
