// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the touch strip (former `RibbonMode`).

/// The modes of the touch strip (ribbon).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RibbonMode {
    PitchDown,
    PitchUp,
    PitchDownUp,
    Cc1,
    Cc11,
    MasterVolume,
    NoteRepeatPeriod,
    NoteRepeatLength,
}

impl RibbonMode {
    pub const ALL: [RibbonMode; 8] = [
        RibbonMode::PitchDown,
        RibbonMode::PitchUp,
        RibbonMode::PitchDownUp,
        RibbonMode::Cc1,
        RibbonMode::Cc11,
        RibbonMode::MasterVolume,
        RibbonMode::NoteRepeatPeriod,
        RibbonMode::NoteRepeatLength,
    ];

    pub fn name(self) -> &'static str {
        match self {
            RibbonMode::PitchDown => "Pitch Down",
            RibbonMode::PitchUp => "Pitch Up",
            RibbonMode::PitchDownUp => "Pitch Down/Up",
            RibbonMode::Cc1 => "Modulation (CC 1)",
            RibbonMode::Cc11 => "Expression (CC 11)",
            RibbonMode::MasterVolume => "Master Volume",
            RibbonMode::NoteRepeatPeriod => "Note Repeat: Period",
            RibbonMode::NoteRepeatLength => "Note Repeat: Length",
        }
    }

    /// The names of all ribbon modes.
    pub fn names() -> Vec<String> {
        Self::ALL.iter().map(|m| m.name().to_string()).collect()
    }

    /// Look up a ribbon mode by its name, the default is Pitch Down.
    pub fn lookup_by_name(name: &str) -> RibbonMode {
        Self::ALL.iter().copied().find(|m| m.name() == name).unwrap_or(RibbonMode::PitchDown)
    }
}
