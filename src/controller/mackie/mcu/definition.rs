// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definition of the Mackie MCU protocol (former `MCUControllerDefinition` and the
//! constants of `MCUControlSurface`).

use crate::controller::mackie::create_device_discovery_pairs;
use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The UUIDs of the models with 0-3 extenders.
pub const EXTENSION_ID: [&str; 4] =
    ["5F10A0CD-F866-41C0-B16A-AEA16282B657", "7FF808DD-45DB-4026-AA6E-844ED8C05B55", "8E0EDA26-ACB9-4F5E-94FB-B886C9468C7A", "4923C47A-B1DC-48C8-AE89-A332AA26BA87"];

/// The names of the models with 0-3 extenders.
pub const HARDWARE_MODEL: [&str; 4] =
    ["MCU - Control Universal", "MCU - Control Universal + 1 Extender", "MCU - Control Universal + 2 Extenders", "MCU - Control Universal + 3 Extenders"];

const I_CON_QCON_PRO_X_V1_15: &str = "iCON QCON Pro X V1.15";

/// General information about the model with the given number of extenders (0-3).
pub fn info(num_extenders: usize) -> ControllerDefinitionInfo {
    ControllerDefinitionInfo {
        uuid: EXTENSION_ID[num_extenders].to_string(),
        vendor: "Mackie".to_string(),
        hardware_model: HARDWARE_MODEL[num_extenders].to_string(),
        num_midi_in_ports: num_extenders + 1,
        num_midi_out_ports: num_extenders + 1,
    }
}

/// The MIDI port names for the automatic detection (former `getMidiDiscoveryPairs`).
///
/// Difference to Java: the Java version creates the pair of the iCON QCON Pro X with its
/// extender for the model with 1 extender, but does not add it to the list (a bug); it is added
/// here.
pub fn midi_discovery_pairs(num_extenders: usize, _os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    match num_extenders + 1 {
        1 => {
            let mut pairs = Vec::new();
            for name in [I_CON_QCON_PRO_X_V1_15, "Platform M V1.14", "Platform M+ V1.07", "X-Touch One", "ZOOM R16_R24", "ZOOM R16_R24 Audio Interface"] {
                pairs.extend(create_device_discovery_pairs(name));
            }
            pairs
        }
        2 => {
            let names = vec![I_CON_QCON_PRO_X_V1_15.to_string(), "iCON QCON Pro XS1 V1.08".to_string()];
            vec![(names.clone(), names)]
        }
        _ => Vec::new(),
    }
}

// Notes 0x90
pub const MCU_ARM1: i32 = 0x00;
pub const MCU_SOLO1: i32 = 0x08;
pub const MCU_MUTE1: i32 = 0x10;
pub const MCU_SELECT1: i32 = 0x18;
pub const MCU_VSELECT1: i32 = 0x20;
pub const MCU_MODE_IO: i32 = 0x28;
pub const MCU_MODE_SENDS: i32 = 0x29;
pub const MCU_MODE_PAN: i32 = 0x2A;
pub const MCU_MODE_PLUGIN: i32 = 0x2B;
pub const MCU_MODE_EQ: i32 = 0x2C;
pub const MCU_MODE_DYN: i32 = 0x2D;
pub const MCU_BANK_LEFT: i32 = 0x2E;
pub const MCU_BANK_RIGHT: i32 = 0x2F;
pub const MCU_TRACK_LEFT: i32 = 0x30;
pub const MCU_TRACK_RIGHT: i32 = 0x31;
pub const MCU_FLIP: i32 = 0x32;
pub const MCU_EDIT: i32 = 0x33;
pub const MCU_NAME_VALUE: i32 = 0x34;
pub const MCU_SMPTE_BEATS: i32 = 0x35;
pub const MCU_F1: i32 = 0x36;
pub const MCU_MIDI_TRACKS: i32 = 0x3E;
pub const MCU_INPUTS: i32 = 0x3F;
pub const MCU_AUDIO_TRACKS: i32 = 0x40;
pub const MCU_AUDIO_INSTR: i32 = 0x41;
pub const MCU_AUX: i32 = 0x42;
pub const MCU_BUSSES: i32 = 0x43;
pub const MCU_OUTPUTS: i32 = 0x44;
pub const MCU_USER: i32 = 0x45;
pub const MCU_SHIFT: i32 = 0x46;
pub const MCU_OPTION: i32 = 0x47;
pub const MCU_CONTROL: i32 = 0x48;
pub const MCU_ALT: i32 = 0x49;
pub const MCU_READ: i32 = 0x4A;
pub const MCU_WRITE: i32 = 0x4B;
pub const MCU_TRIM: i32 = 0x4C;
pub const MCU_TOUCH: i32 = 0x4D;
pub const MCU_LATCH: i32 = 0x4E;
pub const MCU_GROUP: i32 = 0x4F;
pub const MCU_SAVE: i32 = 0x50;
pub const MCU_UNDO: i32 = 0x51;
pub const MCU_CANCEL: i32 = 0x52;
pub const MCU_ENTER: i32 = 0x53;
pub const MCU_MARKER: i32 = 0x54;
pub const MCU_NUDGE: i32 = 0x55;
pub const MCU_REPEAT: i32 = 0x56;
pub const MCU_DROP: i32 = 0x57;
pub const MCU_REPLACE: i32 = 0x58;
pub const MCU_CLICK: i32 = 0x59;
pub const MCU_SOLO: i32 = 0x5A;
pub const MCU_REWIND: i32 = 0x5B;
pub const MCU_FORWARD: i32 = 0x5C;
pub const MCU_STOP: i32 = 0x5D;
pub const MCU_PLAY: i32 = 0x5E;
pub const MCU_RECORD: i32 = 0x5F;
pub const MCU_ARROW_UP: i32 = 0x60;
pub const MCU_ARROW_DOWN: i32 = 0x61;
pub const MCU_ARROW_LEFT: i32 = 0x62;
pub const MCU_ARROW_RIGHT: i32 = 0x63;
pub const MCU_ZOOM: i32 = 0x64;
pub const MCU_SCRUB: i32 = 0x65;
pub const MCU_USER_A: i32 = 0x66;
pub const MCU_USER_B: i32 = 0x67;
pub const MCU_FADER_TOUCH1: i32 = 0x68;
pub const MCU_FADER_MASTER: i32 = 0x70;
pub const MCU_SMPTE_LED: i32 = 0x71;
pub const MCU_BEATS_LED: i32 = 0x72;

// CC: 41-43 increase, 1-3 decrease
pub const MCU_CC_VPOT1: i32 = 0x10;
pub const MCU_CC_JOG: i32 = 0x3C;

/// The header of the system exclusive messages.
pub const SYSEX_HDR: &str = "F0 00 00 66 14 ";

pub const VUMODE_LED: i32 = 1;
pub const VUMODE_OFF: i32 = 2;
pub const VUMODE_LED_AND_LCD: i32 = 3;
pub const VUMODE_LCD: i32 = 4;

pub const KNOB_LED_MODE_SINGLE_DOT: i32 = 0;
pub const KNOB_LED_MODE_BOOST_CUT: i32 = 1;
pub const KNOB_LED_MODE_WRAP: i32 = 2;
pub const KNOB_LED_MODE_SPREAD: i32 = 3;

/// State for button LED on.
pub const MCU_BUTTON_STATE_ON: i32 = 127;
/// State for button LED off.
pub const MCU_BUTTON_STATE_OFF: i32 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models() {
        assert_eq!(info(0).to_string(), "Mackie MCU - Control Universal");
        assert_eq!(info(3).num_midi_in_ports, 4);
        assert_eq!(info(2).uuid, "8E0EDA26-ACB9-4F5E-94FB-B886C9468C7A");
    }

    #[test]
    fn discovery() {
        let pairs = midi_discovery_pairs(0, OperatingSystem::Linux);
        assert_eq!(pairs.len(), 6 * 58);
        assert_eq!(pairs[0], (vec![I_CON_QCON_PRO_X_V1_15.to_string()], vec![I_CON_QCON_PRO_X_V1_15.to_string()]));
        assert_eq!(pairs[1].0[0], "1- iCON QCON Pro X V1.15");
        assert_eq!(pairs[20].0[0], "iCON QCON Pro X V1.15 MIDI 1");
        assert_eq!(pairs[21].0[0], "iCON QCON Pro X V1.15 1 MIDI 1");
        let pairs = midi_discovery_pairs(1, OperatingSystem::Windows);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0.len(), 2);
        assert!(midi_discovery_pairs(3, OperatingSystem::Mac).is_empty());
    }
}
