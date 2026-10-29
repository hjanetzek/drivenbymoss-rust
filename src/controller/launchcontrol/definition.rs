// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definition of the Novation Launch Control XL (former
//! `LaunchControlXLControllerDefinition`).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The UUID of the extension.
pub const EXTENSION_ID: &str = "5AFA5045-FF92-4737-A091-5B371CD5E529";
/// The name of the hardware.
pub const LAUNCH_CONTROL_XL: &str = "Launch Control XL";

/// General information about the controller.
pub fn info() -> ControllerDefinitionInfo {
    ControllerDefinitionInfo {
        uuid: EXTENSION_ID.to_string(),
        vendor: "Novation".to_string(),
        hardware_model: LAUNCH_CONTROL_XL.to_string(),
        num_midi_in_ports: 1,
        num_midi_out_ports: 1,
    }
}

/// The MIDI port names for the automatic detection of the device (input names, output names).
pub fn midi_discovery_pairs(os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    let pair = |name: &str| (vec![name.to_string()], vec![name.to_string()]);
    match os {
        OperatingSystem::Windows => {
            // Former createWindowsDeviceDiscoveryPairs ("%sLaunch Control XL", ...)
            let mut results = vec![pair(LAUNCH_CONTROL_XL)];
            for i in 1..20 {
                results.push(pair(&format!("{i}- {LAUNCH_CONTROL_XL}")));
            }
            results
        }
        OperatingSystem::Mac => vec![pair(LAUNCH_CONTROL_XL)],
        OperatingSystem::Linux => vec![pair(LAUNCH_CONTROL_XL), pair(&format!("{LAUNCH_CONTROL_XL} Launch Contro"))],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery() {
        let linux = midi_discovery_pairs(OperatingSystem::Linux);
        assert_eq!(linux.len(), 2);
        assert_eq!(linux[1].0[0], "Launch Control XL Launch Contro");
        let windows = midi_discovery_pairs(OperatingSystem::Windows);
        assert_eq!(windows.len(), 20);
        assert_eq!(windows[1].1[0], "1- Launch Control XL");
        assert_eq!(midi_discovery_pairs(OperatingSystem::Mac), vec![(vec![LAUNCH_CONTROL_XL.to_string()], vec![LAUNCH_CONTROL_XL.to_string()])]);
        assert_eq!(info().to_string(), "Novation Launch Control XL");
    }
}
