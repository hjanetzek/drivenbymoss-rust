// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The available scale layouts (former `ScaleLayout`).

/// The layouts in which a scale can be applied to a pad grid. Layouts with an even ordinal are
/// oriented upwards, the odd ones to the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScaleLayout {
    /// 4th, upwards.
    FourthUp,
    /// 4th, to the right.
    FourthRight,
    /// 3rd, upwards.
    ThirdUp,
    /// 3rd, to the right.
    ThirdRight,
    /// Sequential, upwards.
    SequentUp,
    /// Sequential, to the right.
    SequentRight,
    /// Octave, upwards.
    EightUp,
    /// Octave, to the right.
    EightRight,
    /// Octave, upwards, centered.
    EightUpCenter,
    /// Octave, to the right, centered.
    EightRightCenter,
    /// Staggered, upwards.
    StaggeredUp,
    /// Staggered, to the right.
    StaggeredRight,
}

impl ScaleLayout {
    /// All layouts in their (Java) order.
    pub const ALL: &'static [ScaleLayout] = &[
        ScaleLayout::FourthUp,
        ScaleLayout::FourthRight,
        ScaleLayout::ThirdUp,
        ScaleLayout::ThirdRight,
        ScaleLayout::SequentUp,
        ScaleLayout::SequentRight,
        ScaleLayout::EightUp,
        ScaleLayout::EightRight,
        ScaleLayout::EightUpCenter,
        ScaleLayout::EightRightCenter,
        ScaleLayout::StaggeredUp,
        ScaleLayout::StaggeredRight,
    ];

    /// The display names of all layouts (same order as [`ScaleLayout::ALL`]).
    pub const NAMES: &'static [&'static str] = &[
        "4th ^",
        "4th >",
        "3rd ^",
        "3rd >",
        "Seqent ^",
        "Seqent >",
        "8th ^",
        "8th >",
        "8th ^ centered",
        "8th > centered",
        "Staggered ^",
        "Staggered >",
    ];

    /// The index of the layout in [`ScaleLayout::ALL`].
    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// Get the layout at the given index.
    pub fn from_ordinal(ordinal: usize) -> Option<ScaleLayout> {
        Self::ALL.get(ordinal).copied()
    }

    /// The display name of the layout (e.g. "4th ^").
    pub fn name(self) -> &'static str {
        Self::NAMES[self.ordinal()]
    }

    /// The display names of all layouts.
    pub fn names() -> &'static [&'static str] {
        Self::NAMES
    }

    /// Get a layout by its display name.
    pub fn by_name(name: &str) -> Option<ScaleLayout> {
        Self::ALL.iter().copied().find(|l| l.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(ScaleLayout::ALL.len(), ScaleLayout::NAMES.len());
        for (i, l) in ScaleLayout::ALL.iter().enumerate() {
            assert_eq!(l.ordinal(), i);
            assert_eq!(ScaleLayout::by_name(l.name()), Some(*l));
        }
        assert_eq!(ScaleLayout::by_name("Seqent >"), Some(ScaleLayout::SequentRight));
        assert_eq!(ScaleLayout::by_name("foo"), None);
    }
}
