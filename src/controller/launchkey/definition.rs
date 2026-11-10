// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definitions of the Novation Launchkey models (former `LaunchkeyMk3ControllerDefinition`,
//! `LaunchkeyMiniMk3ControllerDefinition` and the MIDI constants of `LaunchkeyMk3ControlSurface`
//! and `LaunchkeyMiniMk3ControlSurface`).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The Launchkey models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchkeyModel {
    /// Launchkey 25, 37, 49, 61 and 88 Mk3 (former `maxi` package).
    Mk3,
    /// Launchkey Mini Mk3 (former `mini` package).
    MiniMk3,
}

impl LaunchkeyModel {
    pub const ALL: [LaunchkeyModel; 2] = [LaunchkeyModel::Mk3, LaunchkeyModel::MiniMk3];

    /// The UUID of the extension.
    pub fn uuid(self) -> &'static str {
        match self {
            LaunchkeyModel::Mk3 => "10CB5692-541C-4A5D-9EB4-07D80F34A02C",
            LaunchkeyModel::MiniMk3 => "5359D5B1-28CD-4457-B49D-F8D3D7BC52B9",
        }
    }

    pub fn hardware_model(self) -> &'static str {
        match self {
            LaunchkeyModel::Mk3 => "Launchkey Mk3",
            LaunchkeyModel::MiniMk3 => "Launchkey Mini Mk3",
        }
    }

    /// The fully qualified name of the Java instance class of DrivenByMoss4Reaper.
    pub fn java_class_name(self) -> &'static str {
        match self {
            LaunchkeyModel::Mk3 => "de.mossgrabers.reaper.controller.novation.launchkey.LaunchkeyMk3ControllerInstance",
            LaunchkeyModel::MiniMk3 => "de.mossgrabers.reaper.controller.novation.launchkey.LaunchkeyMiniMk3ControllerInstance",
        }
    }

    pub fn is_mini(self) -> bool {
        self == LaunchkeyModel::MiniMk3
    }

    /// General information about the controller: 2 MIDI inputs (DAW and keyboard port) and 1
    /// output (DAW port).
    pub fn info(self) -> ControllerDefinitionInfo {
        ControllerDefinitionInfo {
            uuid: self.uuid().to_string(),
            vendor: "Novation".to_string(),
            hardware_model: self.hardware_model().to_string(),
            num_midi_in_ports: 2,
            num_midi_out_ports: 1,
        }
    }

    /// The port names for the automatic detection of the device (former
    /// `getMidiDiscoveryPairs`). The first input is the DAW port, the second the keyboard port.
    pub fn midi_discovery_pairs(self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        fn pair(inputs: [&str; 2], output: &str) -> (Vec<String>, Vec<String>) {
            (inputs.iter().map(|s| s.to_string()).collect(), vec![output.to_string()])
        }
        let mut pairs = Vec::new();
        match self {
            LaunchkeyModel::Mk3 => match os {
                OperatingSystem::Windows => pairs.push(pair(["MIDIIN2 (LKMK3 MIDI)", "LKMK3 MIDI"], "MIDIOUT2 (LKMK3 MIDI)")),
                OperatingSystem::Mac => {
                    for size in KEY_SIZES {
                        let device = format!("Launchkey MK3 {size}");
                        pairs.push(pair([&format!("{device} LKMK3 DAW Out"), &format!("{device} LKMK3 MIDI Out")], &format!("{device} LKMK3 DAW In")));
                    }
                }
                OperatingSystem::Linux => {
                    for size in KEY_SIZES {
                        let device = format!("Launchkey MK3 {size}");
                        // Kernel 5.13+
                        pairs.push(pair([&format!("{device} LKMK3 DAW Out"), &format!("{device} LKMK3 MIDI Out")], &format!("{device} LKMK3 DAW In")));
                        // Kernel prior 5.13
                        pairs.push(pair([&format!("{device} MIDI 2"), &format!("{device} MIDI 1")], &format!("{device} MIDI 2")));
                        // Reaper specific
                        let id = format!("L{size}");
                        pairs.push(pair([&format!("{id} [hw:1,0,1]"), &format!("{id} [hw:1,0,0]")], &format!("{id} [hw:1,0,1]")));
                    }
                }
            },
            LaunchkeyModel::MiniMk3 => match os {
                OperatingSystem::Windows => {
                    pairs.push(pair(["MIDIIN2 (Launchkey Mini MK3)", "Launchkey Mini MK3"], "MIDIOUT2 (Launchkey Mini MK3)"));
                    pairs.push(pair(["MIDIIN2 (Launchkey Mini MK3 MID", "Launchkey Mini MK3 MIDI"], "MIDIOUT2 (Launchkey Mini MK3 MI"));
                }
                OperatingSystem::Mac => pairs.push(pair(["Launchkey Mini MK3 DAW Port", "Launchkey Mini MK3 MIDI Port"], "Launchkey Mini MK3 DAW Port")),
                OperatingSystem::Linux => {
                    pairs.push(pair(["MK3 [hw:1,0,1]", "MK3 [hw:1,0,0]"], "MK3 [hw:1,0,1]"));
                    pairs.push(pair(["MK3 [hw:2,0,1]", "MK3 [hw:2,0,0]"], "MK3 [hw:2,0,1]"));
                    pairs.push(pair(["Launchkey Mini MK3 MIDI 2", "Launchkey Mini MK3 MIDI 1"], "Launchkey Mini MK3 MIDI 2"));
                    pairs.push(pair(["Launchkey Mini MK3 Launchkey Mi #2", "Launchkey Mini MK3 Launchkey Mi"], "Launchkey Mini MK3 Launchkey Mi #2"));
                }
            },
        }
        pairs
    }
}

/// The key sizes of the Launchkey Mk3 models (former `KEY_SIZES`).
pub const KEY_SIZES: [i32; 5] = [25, 37, 49, 61, 88];

// Device family codes of the Launchkey Mk3 models

pub const LAUNCHKEY_25: i32 = 52;
pub const LAUNCHKEY_37: i32 = 53;
pub const LAUNCHKEY_49: i32 = 54;
pub const LAUNCHKEY_61: i32 = 55;
pub const LAUNCHKEY_88: i32 = 64;

/// The name of a Launchkey Mk3 model by its device family code (former `LAUNCHKEY_MODEL_NAMES`).
pub fn model_name(device_family_code: i32) -> Option<&'static str> {
    Some(match device_family_code {
        LAUNCHKEY_25 => "Launchkey 25",
        LAUNCHKEY_37 => "Launchkey 37",
        LAUNCHKEY_49 => "Launchkey 49",
        LAUNCHKEY_61 => "Launchkey 61",
        LAUNCHKEY_88 => "Launchkey 88",
        _ => return None,
    })
}

// Buttons & Knobs (MIDI channel 16 if not noted otherwise)

pub const LAUNCHKEY_VIEW_SELECT: i32 = 0x03;
pub const LAUNCHKEY_MODE_SELECT: i32 = 0x09;
pub const LAUNCHKEY_FADER_SELECT: i32 = 0x0A;
pub const LAUNCHKEY_TOUCH_EVENTS: i32 = 0x0B;
pub const LAUNCHKEY_DAW_ONLINE: i32 = 0x0C;
pub const LAUNCHKEY_KNOB_1: i32 = 0x15;
pub const LAUNCHKEY_SELECT1: i32 = 0x25;
pub const LAUNCHKEY_TOGGLE_SELECT: i32 = 0x2D;
pub const LAUNCHKEY_DEVICE_SELECT: i32 = 0x33;
pub const LAUNCHKEY_DEVICE_LOCK: i32 = 0x34;
pub const LAUNCHKEY_FADER_1: i32 = 0x35;
pub const LAUNCHKEY_FADER_MASTER: i32 = 0x3D;
pub const LAUNCHKEY_CAPTURE_MIDI: i32 = 0x4A;
pub const LAUNCHKEY_QUANTIZE: i32 = 0x4B;
pub const LAUNCHKEY_CLICK: i32 = 0x4C;
pub const LAUNCHKEY_UNDO: i32 = 0x4D;
pub const LAUNCHKEY_TRACK_RIGHT: i32 = 0x66;
pub const LAUNCHKEY_TRACK_LEFT: i32 = 0x67;
pub const LAUNCHKEY_SCENE1: i32 = 0x68;
pub const LAUNCHKEY_SCENE2: i32 = 0x69;
pub const LAUNCHKEY_ARROW_UP: i32 = 0x6A;
pub const LAUNCHKEY_ARROW_DOWN: i32 = 0x6B;
pub const LAUNCHKEY_SHIFT: i32 = 0x6C;
pub const LAUNCHKEY_PLAY: i32 = 0x73;
pub const LAUNCHKEY_STOP: i32 = 0x74;
pub const LAUNCHKEY_RECORD: i32 = 0x75;
pub const LAUNCHKEY_LOOP: i32 = 0x76;

// Fader Modes (Mk3)

pub const FADER_MODE_CUSTOM: i32 = 0;
pub const FADER_MODE_VOLUME: i32 = 1;
pub const FADER_MODE_PARAMS: i32 = 2;
pub const FADER_MODE_SEND1: i32 = 4;
pub const FADER_MODE_SEND2: i32 = 5;
pub const FADER_MODE_CUSTOM1: i32 = 6;
pub const FADER_MODE_CUSTOM2: i32 = 7;
pub const FADER_MODE_CUSTOM3: i32 = 8;
pub const FADER_MODE_CUSTOM4: i32 = 9;

// Knob Modes (Mk3; the Mini has KNOB_MODE_VOLUME to KNOB_MODE_SEND2 and KNOB_MODE_MINI_CUSTOM)

pub const KNOB_MODE_CUSTOM: i32 = 0;
pub const KNOB_MODE_VOLUME: i32 = 1;
pub const KNOB_MODE_PARAMS: i32 = 2;
pub const KNOB_MODE_PAN: i32 = 3;
pub const KNOB_MODE_SEND1: i32 = 4;
pub const KNOB_MODE_SEND2: i32 = 5;
pub const KNOB_MODE_CUSTOM1: i32 = 6;
pub const KNOB_MODE_CUSTOM2: i32 = 7;
pub const KNOB_MODE_CUSTOM3: i32 = 8;
pub const KNOB_MODE_CUSTOM4: i32 = 9;
/// The custom knob mode of the Mini (former `LaunchkeyMiniMk3ControlSurface.KNOB_MODE_CUSTOM`).
pub const KNOB_MODE_MINI_CUSTOM: i32 = 6;

// Pad Modes (Mk3; the Mini has PAD_MODE_DRUM, PAD_MODE_SESSION and PAD_MODE_MINI_CUSTOM)

pub const PAD_MODE_CUSTOM: i32 = 0;
pub const PAD_MODE_DRUM: i32 = 1;
pub const PAD_MODE_SESSION: i32 = 2;
pub const PAD_MODE_SCALE_CHORDS: i32 = 3;
pub const PAD_MODE_USER_CHORDS: i32 = 4;
pub const PAD_MODE_CUSTOM_MODE0: i32 = 5;
pub const PAD_MODE_CUSTOM_MODE1: i32 = 6;
pub const PAD_MODE_CUSTOM_MODE2: i32 = 7;
pub const PAD_MODE_CUSTOM_MODE3: i32 = 8;
pub const PAD_MODE_DEVICE_SELECT: i32 = 9;
pub const PAD_MODE_NAVIGATION: i32 = 10;
/// The custom pad mode of the Mini (former `LaunchkeyMiniMk3ControlSurface.PAD_MODE_CUSTOM`).
pub const PAD_MODE_MINI_CUSTOM: i32 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infos() {
        assert_eq!(LaunchkeyModel::Mk3.info().to_string(), "Novation Launchkey Mk3");
        assert_eq!(LaunchkeyModel::MiniMk3.info().to_string(), "Novation Launchkey Mini Mk3");
        assert_eq!(LaunchkeyModel::Mk3.info().num_midi_in_ports, 2);
        assert!(LaunchkeyModel::MiniMk3.is_mini() && !LaunchkeyModel::Mk3.is_mini());
        assert_eq!(model_name(LAUNCHKEY_88), Some("Launchkey 88"));
        assert_eq!(model_name(1), None);
    }

    #[test]
    fn discovery_pairs() {
        let linux = LaunchkeyModel::Mk3.midi_discovery_pairs(OperatingSystem::Linux);
        assert_eq!(linux.len(), 15);
        assert_eq!(linux[0].0, vec!["Launchkey MK3 25 LKMK3 DAW Out", "Launchkey MK3 25 LKMK3 MIDI Out"]);
        assert_eq!(linux[0].1, vec!["Launchkey MK3 25 LKMK3 DAW In"]);
        assert_eq!(linux[2].0[1], "L25 [hw:1,0,0]");
        assert_eq!(LaunchkeyModel::Mk3.midi_discovery_pairs(OperatingSystem::Mac).len(), 5);
        assert_eq!(LaunchkeyModel::Mk3.midi_discovery_pairs(OperatingSystem::Windows)[0].1, vec!["MIDIOUT2 (LKMK3 MIDI)"]);
        let mini = LaunchkeyModel::MiniMk3.midi_discovery_pairs(OperatingSystem::Linux);
        assert_eq!(mini.len(), 4);
        assert_eq!(mini[3].0[0], "Launchkey Mini MK3 Launchkey Mi #2");
        assert_eq!(LaunchkeyModel::MiniMk3.midi_discovery_pairs(OperatingSystem::Windows).len(), 2);
    }
}
