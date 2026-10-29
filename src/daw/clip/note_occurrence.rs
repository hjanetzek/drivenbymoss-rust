// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The different types of note occurrences (former `NoteOccurrenceType`).

/// The different types of note occurrences (conditions when a note is played).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NoteOccurrenceType {
    /// Always mode.
    #[default]
    Always,
    /// On first mode.
    First,
    /// Never first mode.
    NotFirst,
    /// With previous mode.
    Prev,
    /// Without previous mode.
    NotPrev,
    /// With previous channel.
    PrevChannel,
    /// Without previous channel.
    NotPrevChannel,
    /// With previous key mode.
    PrevKey,
    /// Without previous key mode.
    NotPrevKey,
    /// Fill on mode.
    Fill,
    /// Fill off mode.
    NotFill,
}

impl NoteOccurrenceType {
    /// All occurrence types in their declaration order.
    pub const ALL: &'static [NoteOccurrenceType] = &[
        NoteOccurrenceType::Always,
        NoteOccurrenceType::First,
        NoteOccurrenceType::NotFirst,
        NoteOccurrenceType::Prev,
        NoteOccurrenceType::NotPrev,
        NoteOccurrenceType::PrevChannel,
        NoteOccurrenceType::NotPrevChannel,
        NoteOccurrenceType::PrevKey,
        NoteOccurrenceType::NotPrevKey,
        NoteOccurrenceType::Fill,
        NoteOccurrenceType::NotFill,
    ];

    /// Get the display name of the mode, e.g. "On First".
    pub fn name(self) -> &'static str {
        match self {
            NoteOccurrenceType::Always => "Always",
            NoteOccurrenceType::First => "On First",
            NoteOccurrenceType::NotFirst => "Never First",
            NoteOccurrenceType::Prev => "With Previous",
            NoteOccurrenceType::NotPrev => "Without Previous",
            NoteOccurrenceType::PrevChannel => "With Prev Channel",
            NoteOccurrenceType::NotPrevChannel => "Without Prev Channel",
            NoteOccurrenceType::PrevKey => "With Prev Key",
            NoteOccurrenceType::NotPrevKey => "Without Prev Key",
            NoteOccurrenceType::Fill => "Fill On",
            NoteOccurrenceType::NotFill => "Fill Off",
        }
    }

    /// Get the identifier of the mode (the former Java enum constant name, e.g. "NOT_FIRST").
    pub fn identifier(self) -> &'static str {
        match self {
            NoteOccurrenceType::Always => "ALWAYS",
            NoteOccurrenceType::First => "FIRST",
            NoteOccurrenceType::NotFirst => "NOT_FIRST",
            NoteOccurrenceType::Prev => "PREV",
            NoteOccurrenceType::NotPrev => "NOT_PREV",
            NoteOccurrenceType::PrevChannel => "PREV_CHANNEL",
            NoteOccurrenceType::NotPrevChannel => "NOT_PREV_CHANNEL",
            NoteOccurrenceType::PrevKey => "PREV_KEY",
            NoteOccurrenceType::NotPrevKey => "NOT_PREV_KEY",
            NoteOccurrenceType::Fill => "FILL",
            NoteOccurrenceType::NotFill => "NOT_FILL",
        }
    }

    /// Lookup a note occurrence mode by its identifier (see [`Self::identifier`]). Returns
    /// `None` if there is no such mode (Java's `valueOf` threw an exception).
    pub fn lookup(identifier: &str) -> Option<NoteOccurrenceType> {
        Self::ALL
            .iter()
            .copied()
            .find(|m| m.identifier() == identifier)
    }

    /// Lookup a note occurrence mode by its display name (see [`Self::name`]). Returns
    /// [`NoteOccurrenceType::Always`] if no mode with that name exists.
    pub fn lookup_by_name(name: &str) -> NoteOccurrenceType {
        Self::ALL
            .iter()
            .copied()
            .find(|m| m.name() == name)
            .unwrap_or(Self::ALL[0])
    }

    /// The index of the mode in [`Self::ALL`].
    pub fn ordinal(self) -> usize {
        self as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert_eq!(
            NoteOccurrenceType::lookup("PREV_KEY"),
            Some(NoteOccurrenceType::PrevKey)
        );
        assert_eq!(NoteOccurrenceType::lookup("xxx"), None);
        assert_eq!(
            NoteOccurrenceType::lookup_by_name("Fill Off"),
            NoteOccurrenceType::NotFill
        );
        assert_eq!(
            NoteOccurrenceType::lookup_by_name("xxx"),
            NoteOccurrenceType::Always
        );
        for (i, m) in NoteOccurrenceType::ALL.iter().enumerate() {
            assert_eq!(m.ordinal(), i);
        }
    }
}
