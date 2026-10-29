// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Constants and enumerations of the DAW model (former `framework.daw.constants`, `ChannelType`
//! and `GrooveParameterID`).

/// Automation write modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AutomationMode {
    #[default]
    TrimRead,
    Read,
    Touch,
    Latch,
    LatchPreview,
    Write,
}

impl AutomationMode {
    pub const ALL: &'static [AutomationMode] = &[
        AutomationMode::TrimRead,
        AutomationMode::Read,
        AutomationMode::Touch,
        AutomationMode::Latch,
        AutomationMode::LatchPreview,
        AutomationMode::Write,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AutomationMode::TrimRead => "Trim/Read",
            AutomationMode::Read => "Read",
            AutomationMode::Touch => "Touch",
            AutomationMode::Latch => "Latch",
            AutomationMode::LatchPreview => "Latch Preview",
            AutomationMode::Write => "Write",
        }
    }

    /// The Java enum name.
    pub fn name(self) -> &'static str {
        match self {
            AutomationMode::TrimRead => "TRIM_READ",
            AutomationMode::Read => "READ",
            AutomationMode::Touch => "TOUCH",
            AutomationMode::Latch => "LATCH",
            AutomationMode::LatchPreview => "LATCH_PREVIEW",
            AutomationMode::Write => "WRITE",
        }
    }

    /// The lower case name.
    pub fn identifier(self) -> String {
        self.name().to_lowercase()
    }

    pub fn lookup(identifier: &str) -> AutomationMode {
        Self::ALL.iter().copied().find(|m| m.name().eq_ignore_ascii_case(identifier)).unwrap_or(AutomationMode::TrimRead)
    }
}

/// Capabilities of a DAW.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    NoteRepeatLength,
    NoteRepeatSwing,
    NoteRepeatMode,
    NoteRepeatOctaves,
    NoteRepeatIsFreeRunning,
    NoteRepeatUsePressureToVelocity,
    NoteRepeatLatch,
    QuantizeInputNoteLength,
    QuantizeAmount,
    CueVolume,
    HasSlotChains,
    HasDrumDevice,
    HasCrossfader,
    HasDeviceLayers,
    HasPinning,
    HasParameterPageSection,
    HasEffectBank,
    HasBrowserPreview,
    MpePitchRange,
}

/// IDs of specific devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceId {
    FirstInstrument,
    Eq,
    NiKomplete,
}

/// Launch quantization values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LaunchQuantization {
    #[default]
    ResNone,
    Res8,
    Res4,
    Res2,
    Res1,
    Res1_2,
    Res1_4,
    Res1_8,
    Res1_16,
}

impl LaunchQuantization {
    pub const ALL: &'static [LaunchQuantization] = &[
        LaunchQuantization::ResNone,
        LaunchQuantization::Res8,
        LaunchQuantization::Res4,
        LaunchQuantization::Res2,
        LaunchQuantization::Res1,
        LaunchQuantization::Res1_2,
        LaunchQuantization::Res1_4,
        LaunchQuantization::Res1_8,
        LaunchQuantization::Res1_16,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LaunchQuantization::ResNone => "None",
            other => other.value(),
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            LaunchQuantization::ResNone => "none",
            LaunchQuantization::Res8 => "8",
            LaunchQuantization::Res4 => "4",
            LaunchQuantization::Res2 => "2",
            LaunchQuantization::Res1 => "1",
            LaunchQuantization::Res1_2 => "1/2",
            LaunchQuantization::Res1_4 => "1/4",
            LaunchQuantization::Res1_8 => "1/8",
            LaunchQuantization::Res1_16 => "1/16",
        }
    }

    pub fn lookup(value: &str) -> LaunchQuantization {
        Self::ALL.iter().copied().find(|q| q.value() == value).unwrap_or_default()
    }
}

/// Actions after recording a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PostRecordingAction {
    #[default]
    Off,
    PlayRecorded,
    RecordNextFreeSlot,
    Stop,
    ReturnToArrangement,
    ReturnToPreviousClip,
    PlayRandom,
}

impl PostRecordingAction {
    pub const ALL: &'static [PostRecordingAction] = &[
        PostRecordingAction::Off,
        PostRecordingAction::PlayRecorded,
        PostRecordingAction::RecordNextFreeSlot,
        PostRecordingAction::Stop,
        PostRecordingAction::ReturnToArrangement,
        PostRecordingAction::ReturnToPreviousClip,
        PostRecordingAction::PlayRandom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PostRecordingAction::Off => "Off",
            PostRecordingAction::PlayRecorded => "Read",
            PostRecordingAction::RecordNextFreeSlot => "Record into next free slot",
            PostRecordingAction::Stop => "Stop",
            PostRecordingAction::ReturnToArrangement => "Return to arrangement",
            PostRecordingAction::ReturnToPreviousClip => "Return to last clip",
            PostRecordingAction::PlayRandom => "Play random",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PostRecordingAction::Off => "OFF",
            PostRecordingAction::PlayRecorded => "PLAY_RECORDED",
            PostRecordingAction::RecordNextFreeSlot => "RECORD_NEXT_FREE_SLOT",
            PostRecordingAction::Stop => "STOP",
            PostRecordingAction::ReturnToArrangement => "RETURN_TO_ARRANGEMENT",
            PostRecordingAction::ReturnToPreviousClip => "RETURN_TO_PREVIOUS_CLIP",
            PostRecordingAction::PlayRandom => "PLAY_RANDOM",
        }
    }

    pub fn identifier(self) -> String {
        self.name().to_lowercase()
    }

    pub fn lookup(identifier: &str) -> PostRecordingAction {
        Self::ALL.iter().copied().find(|m| m.name().eq_ignore_ascii_case(identifier)).unwrap_or_default()
    }
}

/// Record quantization values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RecordQuantization {
    #[default]
    ResOff,
    Res1_32,
    Res1_16,
    Res1_8,
    Res1_4,
}

impl RecordQuantization {
    pub const ALL: &'static [RecordQuantization] = &[
        RecordQuantization::ResOff,
        RecordQuantization::Res1_32,
        RecordQuantization::Res1_16,
        RecordQuantization::Res1_8,
        RecordQuantization::Res1_4,
    ];

    pub fn name(self) -> &'static str {
        match self {
            RecordQuantization::ResOff => "Off",
            other => other.value(),
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            RecordQuantization::ResOff => "OFF",
            RecordQuantization::Res1_32 => "1/32",
            RecordQuantization::Res1_16 => "1/16",
            RecordQuantization::Res1_8 => "1/8",
            RecordQuantization::Res1_4 => "1/4",
        }
    }

    /// The resolution in beats as used by Reaper (0 for off).
    pub fn beats(self) -> f64 {
        match self {
            RecordQuantization::ResOff => 0.0,
            RecordQuantization::Res1_32 => 0.125,
            RecordQuantization::Res1_16 => 0.25,
            RecordQuantization::Res1_8 => 0.5,
            RecordQuantization::Res1_4 => 1.0,
        }
    }

    pub fn lookup(value: &str) -> RecordQuantization {
        Self::ALL.iter().copied().find(|q| q.value() == value).unwrap_or_default()
    }
}

/// Note resolutions (grid sizes) in beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Resolution {
    Res1_4,
    Res1_4T,
    Res1_8,
    Res1_8T,
    #[default]
    Res1_16,
    Res1_16T,
    Res1_32,
    Res1_32T,
}

impl Resolution {
    pub const ALL: &'static [Resolution] = &[
        Resolution::Res1_4,
        Resolution::Res1_4T,
        Resolution::Res1_8,
        Resolution::Res1_8T,
        Resolution::Res1_16,
        Resolution::Res1_16T,
        Resolution::Res1_32,
        Resolution::Res1_32T,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Resolution::Res1_4 => "1/4",
            Resolution::Res1_4T => "1/4t",
            Resolution::Res1_8 => "1/8",
            Resolution::Res1_8T => "1/8t",
            Resolution::Res1_16 => "1/16",
            Resolution::Res1_16T => "1/16t",
            Resolution::Res1_32 => "1/32",
            Resolution::Res1_32T => "1/32t",
        }
    }

    pub fn value(self) -> f64 {
        match self {
            Resolution::Res1_4 => 1.0,
            Resolution::Res1_4T => 2.0 / 3.0,
            Resolution::Res1_8 => 1.0 / 2.0,
            Resolution::Res1_8T => 1.0 / 3.0,
            Resolution::Res1_16 => 1.0 / 4.0,
            Resolution::Res1_16T => 1.0 / 6.0,
            Resolution::Res1_32 => 1.0 / 8.0,
            Resolution::Res1_32T => 1.0 / 12.0,
        }
    }

    pub fn ordinal(self) -> usize {
        self as usize
    }

    pub fn value_at(index: usize) -> f64 {
        Self::ALL[index].value()
    }

    pub fn name_at(index: usize) -> &'static str {
        Self::ALL[index].name()
    }

    pub fn by_name(name: &str) -> Option<Resolution> {
        Self::ALL.iter().copied().find(|r| r.name() == name)
    }

    pub fn matches(index: usize, value: f64) -> bool {
        (value - Self::value_at(index)).abs() < 0.001
    }

    /// Get the index of the resolution which is closest to the given value.
    pub fn get_match(value: f64) -> usize {
        let mut min = 1.0;
        let mut result = 0;
        for i in 0..Self::ALL.len() {
            let diff = (value - Self::value_at(i)).abs();
            if diff < min {
                min = diff;
                result = i;
            }
        }
        result
    }

    /// Increase or decrease the resolution index (clamped).
    pub fn change(index: usize, inc: bool) -> usize {
        let i = index as i64 + if inc { 1 } else { -1 };
        i.clamp(0, Self::ALL.len() as i64 - 1) as usize
    }

    pub fn names() -> Vec<&'static str> {
        Self::ALL.iter().map(|r| r.name()).collect()
    }
}

/// Transport related constants.
pub mod transport_constants {
    pub const INC_FRACTION_TIME: f64 = 4.0;
    pub const INC_FRACTION_TIME_SLOW: f64 = 1.0;
    pub const INC_FRACTION_ACCENT: f64 = 1.0;
    pub const INC_FRACTION_ACCENT_SLOW: f64 = 1.0 / 16.0;
    pub const MIN_TEMPO: i32 = 20;
    pub const MAX_TEMPO: i32 = 666;
}

/// The type of a channel (former `resource.ChannelType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelType {
    #[default]
    Unknown,
    Audio,
    Instrument,
    Hybrid,
    Group,
    GroupOpen,
    Effect,
    Master,
    Layer,
    Cue,
}

impl ChannelType {
    pub fn label(self) -> &'static str {
        match self {
            ChannelType::Unknown => "Unknown",
            ChannelType::Audio => "Audio",
            ChannelType::Instrument => "Instrument",
            ChannelType::Hybrid => "Hybrid",
            ChannelType::Group | ChannelType::GroupOpen => "Group",
            ChannelType::Effect => "Effect",
            ChannelType::Master => "Master",
            ChannelType::Layer => "Layer",
            ChannelType::Cue => "Cue",
        }
    }

    /// The Java enum name as sent by Reaper (e.g. "INSTRUMENT").
    pub fn name(self) -> &'static str {
        match self {
            ChannelType::Unknown => "UNKNOWN",
            ChannelType::Audio => "AUDIO",
            ChannelType::Instrument => "INSTRUMENT",
            ChannelType::Hybrid => "HYBRID",
            ChannelType::Group => "GROUP",
            ChannelType::GroupOpen => "GROUP_OPEN",
            ChannelType::Effect => "EFFECT",
            ChannelType::Master => "MASTER",
            ChannelType::Layer => "LAYER",
            ChannelType::Cue => "CUE",
        }
    }

    /// Parse the Java enum name (former `ChannelType.valueOf`).
    pub fn from_name(name: &str) -> Option<ChannelType> {
        use ChannelType::*;
        [Unknown, Audio, Instrument, Hybrid, Group, GroupOpen, Effect, Master, Layer, Cue].into_iter().find(|t| t.name() == name)
    }
}

/// The parameters of the groove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrooveParameterId {
    Enabled,
    ShuffleAmount,
    ShuffleRate,
    AccentAmount,
    AccentRate,
    AccentPhase,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution() {
        assert_eq!(Resolution::get_match(0.25), 4);
        assert_eq!(Resolution::change(0, false), 0);
        assert_eq!(Resolution::change(7, true), 7);
        assert_eq!(ChannelType::from_name("GROUP"), Some(ChannelType::Group));
    }
}
