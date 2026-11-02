// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definition of the Akai devices with the ACVS protocol: MPC Live I/II, One, X and Force
//! (former `ACVSControllerDefinition` and `ACVSDevice`).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The UUID of the extension.
pub const EXTENSION_ID: &str = "CE97B67C-FFB7-4309-AFF2-45193C0C87A3";

/// The devices which support the ACVS protocol (former `ACVSDevice`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcvsDevice {
    MpcX,
    MpcLiveOne,
    Force,
}

impl AcvsDevice {
    pub const ALL: [AcvsDevice; 3] = [AcvsDevice::MpcX, AcvsDevice::MpcLiveOne, AcvsDevice::Force];

    pub fn name(self) -> &'static str {
        match self {
            AcvsDevice::MpcX => "MPC X",
            AcvsDevice::MpcLiveOne => "MPC Live / Live II / One",
            AcvsDevice::Force => "Force",
        }
    }

    /// The ID of the device in the system exclusive messages.
    pub fn id(self) -> u8 {
        match self {
            AcvsDevice::MpcX => 0x3A,
            AcvsDevice::MpcLiveOne => 0x3B,
            AcvsDevice::Force => 0x40,
        }
    }

    pub fn from_id(id: u8) -> Option<AcvsDevice> {
        Self::ALL.iter().copied().find(|d| d.id() == id)
    }

    pub fn from_name(name: &str) -> Option<AcvsDevice> {
        Self::ALL.iter().copied().find(|d| d.name() == name)
    }
}

/// General information about the controller.
pub fn info() -> ControllerDefinitionInfo {
    ControllerDefinitionInfo {
        uuid: EXTENSION_ID.to_string(),
        vendor: "Akai".to_string(),
        hardware_model: "MPC Live I/II, One, Force".to_string(),
        num_midi_in_ports: 2,
        num_midi_out_ports: 1,
    }
}

/// The port names for the automatic detection of the device.
pub fn midi_discovery_pairs(_os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    vec![(vec!["Akai Network - DAW Control".to_string(), "Akai Network - MIDI".to_string()], vec!["Akai Network - DAW Control".to_string()])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices() {
        assert_eq!(AcvsDevice::from_id(0x40), Some(AcvsDevice::Force));
        assert_eq!(AcvsDevice::from_id(0x41), None);
        assert_eq!(AcvsDevice::from_name("MPC X"), Some(AcvsDevice::MpcX));
        assert_eq!(info().to_string(), "Akai MPC Live I/II, One, Force");
        assert_eq!(midi_discovery_pairs(OperatingSystem::Linux)[0].0.len(), 2);
    }
}
