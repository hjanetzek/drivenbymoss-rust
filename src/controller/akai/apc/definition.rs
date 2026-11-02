// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definition of the Akai APC40 Mk1 and Mk2 (former `APCControllerDefinition`).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The device ID of the APC40 in the system exclusive messages.
const ID_APC_40: u8 = 0x73;
/// The device ID of the APC40 Mk2 in the system exclusive messages.
const ID_APC_40_MKII: u8 = 0x29;

/// The definition of an APC40 model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApcDefinition {
    pub is_mk2: bool,
}

impl ApcDefinition {
    pub fn new(is_mk2: bool) -> Self {
        ApcDefinition { is_mk2 }
    }

    pub fn uuid(&self) -> &'static str {
        if self.is_mk2 {
            "14787D10-35DE-11E4-8C21-0800200C9A66"
        } else {
            "35E958A0-345F-11E4-8C21-0800200C9A66"
        }
    }

    pub fn hardware_model(&self) -> &'static str {
        if self.is_mk2 {
            "APC40 mkII"
        } else {
            "APC40"
        }
    }

    pub fn info(&self) -> ControllerDefinitionInfo {
        ControllerDefinitionInfo {
            uuid: self.uuid().to_string(),
            vendor: "Akai".to_string(),
            hardware_model: self.hardware_model().to_string(),
            num_midi_in_ports: 1,
            num_midi_out_ports: 1,
        }
    }

    pub fn midi_discovery_pairs(&self, _os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        super::super::device_discovery_pairs(if self.is_mk2 { "APC40 mkII" } else { "Akai APC40" })
    }

    /// The system exclusive message to set the device to mode 2 (former constructor of
    /// `APCControlSurface`): F0 47 7F id 60 00 04 41 08 02 01 F7.
    pub fn mode2_sysex(&self) -> [u8; 12] {
        [0xF0, 0x47, 0x7F, if self.is_mk2 { ID_APC_40_MKII } else { ID_APC_40 }, 0x60, 0x00, 0x04, 0x41, 0x08, 0x02, 0x01, 0xF7]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions() {
        let mk1 = ApcDefinition::new(false);
        let mk2 = ApcDefinition::new(true);
        assert_eq!(mk1.info().to_string(), "Akai APC40");
        assert_eq!(mk2.info().to_string(), "Akai APC40 mkII");
        assert_eq!(mk1.mode2_sysex()[3], 0x73);
        assert_eq!(mk2.mode2_sysex(), [0xF0, 0x47, 0x7F, 0x29, 0x60, 0x00, 0x04, 0x41, 0x08, 0x02, 0x01, 0xF7]);
        assert_eq!(mk1.midi_discovery_pairs(OperatingSystem::Windows)[0].0[0], "Akai APC40");
        assert_eq!(mk2.midi_discovery_pairs(OperatingSystem::Linux)[2].1[0], "2- APC40 mkII");
    }
}
