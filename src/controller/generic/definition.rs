// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definition of Generic Flexi (former `GenericFlexiControllerDefinition`).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The UUID of the extension.
pub const EXTENSION_ID: &str = "1B43F14F-CAAF-468A-9418-417C678653A0";

/// General information about the controller.
pub fn info() -> ControllerDefinitionInfo {
    ControllerDefinitionInfo {
        uuid: EXTENSION_ID.to_string(),
        vendor: "Generic".to_string(),
        hardware_model: "Flexi".to_string(),
        num_midi_in_ports: 1,
        num_midi_out_ports: 1,
    }
}

/// There is no automatic detection of the MIDI ports (`DefaultControllerDefinition` has no
/// discovery pairs).
pub fn midi_discovery_pairs(_os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition() {
        assert_eq!(info().to_string(), "Generic Flexi");
        assert!(midi_discovery_pairs(OperatingSystem::Linux).is_empty());
    }
}
