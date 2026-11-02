// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The items of the MPC/Force screens and pads which are set with notes or CCs (former
//! `ScreenItem`). The Java enum with one constant per item (e.g. `TRACK1_SELECT` ...
//! `TRACK8_SELECT`) is represented by a variant per item group with the index.

/// The notes of the MPC parameters on MIDI channel 0x0A.
pub mod mpc {
    pub const METRONOME: i32 = 0;
    pub const CAPTURE_MIDI: i32 = 1;
    pub const ABLETON_LINK: i32 = 2;
    pub const ARRANGE_OVERDUB: i32 = 3;
    pub const ARRANGER_AUTOMATION_ARM: i32 = 4;
    pub const LOOP_SWITCH: i32 = 5;
    pub const LAUNCH_QUANTIZE: i32 = 6;
    pub const ARRANGEMENT_SESSION: i32 = 7;
    pub const FOLLOW: i32 = 8;
    pub const CLIP_DEVICE_VIEW: i32 = 9;
    pub const DEVICE_LOCK: i32 = 10;
    pub const DETAIL_VIEW: i32 = 11;
    pub const QUANTIZE_INTERVAL: i32 = 15;
    pub const CLIP_SCENE_LAUNCH: i32 = 23;
}

/// An item of the screen (or a pad) of the MPC/Force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenItem {
    TrackSelect(usize),
    TrackType(usize),
    TrackClipProgress(usize),
    TrackNumberOfScenes,
    ClipColor(usize),
    ClipState(usize),
    SceneLaunchState(usize),
    SceneSelect(usize),
    TrackSolo(usize),
    TrackMute(usize),
    TrackSoloMute(usize),
    TrackCue(usize),
    TrackCrossfader(usize),
    TrackRecArm(usize),
    TrackLevel(usize),
    TrackPan(usize),
    TrackSendsNo,
    /// The level of a send (send index 0-3, track index).
    TrackSendLevel(usize, usize),
    TrackVuLeft(usize),
    TrackVuRight(usize),
    TrackVuPeakLeft(usize),
    TrackVuPeakRight(usize),
    DeviceParamValue(usize),
    DeviceCount,
    CurrentDeviceIndex,
    CurrentDeviceActive,
    DeviceParamEnabled(usize),
    /// One of the [`mpc`] parameters.
    Mpc(i32),
    MpcPadState(usize),
    MpcPadColor(usize),
    ForcePadState(usize),
    ForcePadColor(usize),
    ForceTrackColor(usize),
    ForceTrackAssign(usize),
    ForceScene(usize),
    /// Force & MPC-X
    KnobValue(usize),
    KnobStyleColor(usize),
}

impl ScreenItem {
    /// The MIDI channel, if the item is a note and the note or CC number.
    pub fn midi(self) -> (i32, bool, i32) {
        use ScreenItem::*;
        let i = |index: usize| index as i32;
        match self {
            TrackSelect(t) => (0, true, i(t)),
            TrackType(t) => (0, true, 8 + i(t)),
            TrackClipProgress(t) => (0, false, i(t)),
            TrackNumberOfScenes => (0, false, 8),
            ClipColor(c) => (0, false, 24 + i(c)),
            ClipState(c) => (0, true, 24 + i(c)),
            SceneLaunchState(s) => (0, true, 88 + i(s)),
            SceneSelect(s) => (0, true, 96 + i(s)),
            TrackSolo(t) => (1 + i(t), true, 0),
            TrackMute(t) => (1 + i(t), true, 1),
            TrackSoloMute(t) => (1 + i(t), true, 2),
            TrackCue(t) => (1 + i(t), true, 3),
            TrackCrossfader(t) => (1 + i(t), true, 4),
            TrackRecArm(t) => (1 + i(t), true, 5),
            TrackLevel(t) => (1 + i(t), false, 0),
            TrackPan(t) => (1 + i(t), false, 1),
            TrackSendsNo => (1, false, 2),
            TrackSendLevel(s, t) => (1 + i(t), false, 3 + i(s)),
            TrackVuLeft(t) => (1 + i(t), false, 124),
            TrackVuRight(t) => (1 + i(t), false, 125),
            TrackVuPeakLeft(t) => (1 + i(t), false, 126),
            TrackVuPeakRight(t) => (1 + i(t), false, 127),
            DeviceParamValue(p) => (0x09, false, i(p)),
            DeviceCount => (0x09, false, 16),
            CurrentDeviceIndex => (0x09, false, 17),
            CurrentDeviceActive => (0x09, true, 0),
            DeviceParamEnabled(p) => (0x09, true, 112 + i(p)),
            Mpc(note) => (0x0A, true, note),
            MpcPadState(p) => (0x0C, true, i(p)),
            MpcPadColor(p) => (0x0C, false, i(p)),
            ForcePadState(p) => (0x0C, true, 16 + i(p)),
            ForcePadColor(p) => (0x0C, false, 40 + i(p)),
            ForceTrackColor(t) => (0x0C, false, 24 + i(t)),
            ForceTrackAssign(t) => (0x0C, false, 32 + i(t)),
            ForceScene(s) => (0x0C, true, 80 + i(s)),
            KnobValue(k) => (0x0D, false, i(k)),
            KnobStyleColor(k) => (0x0D, true, 16 + i(k)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_mapping() {
        assert_eq!(ScreenItem::TrackSelect(7).midi(), (0, true, 7));
        assert_eq!(ScreenItem::ClipState(63).midi(), (0, true, 87));
        assert_eq!(ScreenItem::TrackSendLevel(3, 7).midi(), (8, false, 6));
        assert_eq!(ScreenItem::ForcePadColor(63).midi(), (0x0C, false, 103));
        assert_eq!(ScreenItem::KnobStyleColor(15).midi(), (0x0D, true, 31));
        assert_eq!(ScreenItem::Mpc(mpc::CLIP_SCENE_LAUNCH).midi(), (0x0A, true, 23));
    }
}
