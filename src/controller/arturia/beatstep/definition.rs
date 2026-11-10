// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definition of the Arturia Beatstep (former `BeatstepControllerDefinition`) and its MIDI
//! numbers and sysex messages (the constants of the former `BeatstepControlSurface`).
//!
//! The Java package supports the Beatstep and the Beatstep Pro with one definition: the Pro
//! sends the same MIDI on its controller channel. Its step buttons (`BEATSTEP_PRO_STEP1-16`,
//! `StepCommand`) are defined but not registered by the Java setup.

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The UUID of the extension.
pub const EXTENSION_ID: &str = "F7FF1750-7EC3-11E4-B4A9-0800200C9A66";
/// The name of the hardware.
pub const HARDWARE_MODEL: &str = "Beatstep";
/// The vendor.
pub const VENDOR: &str = "Arturia";
/// The device name for the automatic detection of the MIDI ports.
pub const DEVICE_NAME: &str = "Arturia BeatStep";

/// The MIDI channel of the knobs and pads (MIDI channel 3).
pub const BEATSTEP_CHANNEL: i32 = 2;

/// The note of the Shift button (MIDI channel 1, enabled by [`SYSEX_ENABLE_SHIFT`]).
pub const BEATSTEP_SHIFT: i32 = 7;

/// The CC of the first knob of the upper row (knobs 1-8: CC 20-27).
pub const BEATSTEP_KNOB_1: i32 = 20;
/// The CC of the first knob of the lower row (knobs 9-16: CC 30-37).
pub const BEATSTEP_KNOB_9: i32 = 30;
/// The CC of the large knob.
pub const BEATSTEP_KNOB_MAIN: i32 = 40;

/// The CC of the first step button of the Beatstep Pro (steps 1-16: CC 50-65).
pub const BEATSTEP_PRO_STEP1: i32 = 50;

/// The sysex ID of the first pad of the upper row (pads 1-16: 0x70-0x7F).
pub const BEATSTEP_PAD_1: u8 = 0x70;
/// The sysex ID of the first pad of the lower row.
pub const BEATSTEP_PAD_9: u8 = 0x78;

/// The start of the sysex message which sets the color of a pad (former `SYSEX_HEADER`).
pub const SYSEX_HEADER: [u8; 9] = [0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x10];
/// The end of a sysex message (former `SYSEX_END`).
pub const SYSEX_END: u8 = 0xF7;

/// Enables the Shift button to send MIDI note 7 (sent on startup).
pub const SYSEX_ENABLE_SHIFT: [u8; 12] = [0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x01, 0x5E, 0x09, 0xF7];

/// The number of pad columns.
pub const NUM_COLUMNS: i32 = 8;
/// The note of the first pad of the grid.
pub const START_NOTE: i32 = 36;

/// The sysex message which sets the color (0, 1, 16 or 17) of the pad of a grid note (former
/// `BeatstepPadGrid.lightPad`). The grid notes 36-43 are the pads 9-16, the notes 44-51 the
/// pads 1-8.
pub fn light_pad_message(note: i32, color: i32) -> [u8; 12] {
    let n = note - START_NOTE;
    let pad = if n < NUM_COLUMNS { BEATSTEP_PAD_9 as i32 + n } else { BEATSTEP_PAD_1 as i32 + n - NUM_COLUMNS };
    let mut message = [0; 12];
    message[..9].copy_from_slice(&SYSEX_HEADER);
    message[9] = (pad & 0x7F) as u8;
    message[10] = (color & 0x7F) as u8;
    message[11] = SYSEX_END;
    message
}

/// General information about the controller.
pub fn info() -> ControllerDefinitionInfo {
    ControllerDefinitionInfo {
        uuid: EXTENSION_ID.to_string(),
        vendor: VENDOR.to_string(),
        hardware_model: HARDWARE_MODEL.to_string(),
        num_midi_in_ports: 1,
        num_midi_out_ports: 1,
    }
}

/// The port names for the automatic detection of the device (former
/// `createDeviceDiscoveryPairs ("Arturia BeatStep")`, the same on all operating systems).
pub fn midi_discovery_pairs(_os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    crate::controller::akai::device_discovery_pairs(DEVICE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_messages() {
        // The lower grid row (notes 36-43) are the pads 9-16
        assert_eq!(light_pad_message(36, 16), [0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x10, 0x78, 0x10, 0xF7]);
        assert_eq!(light_pad_message(43, 1)[9], 0x7F);
        // The upper grid row (notes 44-51) are the pads 1-8
        assert_eq!(light_pad_message(44, 17)[9..11], [0x70, 0x11]);
        assert_eq!(light_pad_message(51, 0)[9..11], [0x77, 0x00]);
    }

    #[test]
    fn infos() {
        assert_eq!(info().to_string(), "Arturia Beatstep");
        let pairs = midi_discovery_pairs(OperatingSystem::Linux);
        assert_eq!(pairs.len(), 58);
        assert_eq!(pairs[0], (vec![DEVICE_NAME.to_string()], vec![DEVICE_NAME.to_string()]));
        assert_eq!(pairs[1].0[0], "1- Arturia BeatStep");
        assert_eq!(pairs[20].0[0], "Arturia BeatStep MIDI 1");
        assert_eq!(pairs[21].1[0], "Arturia BeatStep 1 MIDI 1");
    }
}
