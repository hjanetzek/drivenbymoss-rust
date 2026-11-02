// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Akai Fire (former `controller.akai.fire` and `FireControllerInstance`).
//!
//! The 128x64 OLED display is drawn with the generic [`crate::framework::graphics`] module and
//! sent as 1 bit stripes (see [`surface`]).
//!
//! Not ported:
//! - The XoX drum sequencer (former `DrumXoXView`, Views.DRUM): it requires the
//!   `AbstractDrumXoXView` which does not exist in the Rust framework. The DRUM button cycles
//!   through the Drum 4 and Drum 64 views only.
//! - Pinning of the cursor track (the Rust track model has no pinning): the pad of the shift
//!   view shows "not pinned" and does nothing.
//! - The simulator layout (former `layoutControls`) and the simulator colors of the buttons
//!   (former `FireColorManager.getColor` for buttons).
//! - The knob sensitivity changes of the browser mode (the surface has no sensitivity).

pub mod colors;
pub mod command;
pub mod mode;
pub mod setup;
pub mod surface;
pub mod view;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// The UUID of the Fire (former `EXTENSION_ID`).
pub const UUID: &str = "75CC171F-325C-45B8-B304-DAE96756D5BA";

/// The factory of the Fire (former `FireControllerDefinition`).
pub struct FireFactory;

fn pair(input: String, output: String) -> (Vec<String>, Vec<String>) {
    (vec![input], vec![output])
}

/// Former `createLinuxDeviceDiscoveryPairs`.
fn linux_pairs(name: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let mut results = vec![pair(name.to_string(), name.to_string())];
    for i in 1..20 {
        let hw = format!("{name} [hw:{i},0,0]");
        results.push(pair(hw.clone(), hw));
        let midi = format!("{name} MIDI {i}");
        results.push(pair(midi.clone(), midi));
    }
    results
}

/// The port names of the Fire per operating system (former `getMidiDiscoveryPairs`).
pub fn midi_discovery_pairs(os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    let mut results = Vec::new();
    match os {
        OperatingSystem::Windows => {
            // Former createWindowsDeviceDiscoveryPairs ("%sFL STUDIO FIRE", ...)
            results.push(pair("FL STUDIO FIRE".into(), "FL STUDIO FIRE".into()));
            for i in 1..20 {
                let name = format!("{i}- FL STUDIO FIRE");
                results.push(pair(name.clone(), name));
            }
            results.extend(super::device_discovery_pairs("Akai Fire"));
        }
        OperatingSystem::Linux => {
            results.extend(super::device_discovery_pairs("FL STUDIO FIRE"));
            results.extend(linux_pairs("FIRE"));
            results.extend(linux_pairs("FL STUDIO FIRE Jack 1"));
        }
        OperatingSystem::Mac => {
            for i in 1..20 {
                let name = format!("FL STUDIO FIRE Jack {i}");
                results.push(pair(name.clone(), name));
            }
            results.extend(super::device_discovery_pairs("FL STUDIO FIRE"));
        }
    }
    results
}

impl ControllerFactory for FireFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        ControllerDefinitionInfo {
            uuid: UUID.to_string(),
            vendor: "Akai".to_string(),
            hardware_model: "Fire".to_string(),
            num_midi_in_ports: 1,
            num_midi_out_ports: 1,
        }
    }

    fn java_class_name(&self) -> &'static str {
        "de.mossgrabers.reaper.controller.akai.fire.FireControllerInstance"
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::fire::for_reaper()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(host, sender, ports, configuration)
    }
}

/// The factory of the Fire.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    vec![Box::new(FireFactory)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition() {
        assert_eq!(FireFactory.info().to_string(), "Akai Fire");
        assert_eq!(midi_discovery_pairs(OperatingSystem::Windows)[1].0[0], "1- FL STUDIO FIRE");
        assert_eq!(midi_discovery_pairs(OperatingSystem::Linux)[0].0[0], "FL STUDIO FIRE");
        assert!(midi_discovery_pairs(OperatingSystem::Linux).iter().any(|p| p.0[0] == "FIRE [hw:1,0,0]"));
        assert_eq!(midi_discovery_pairs(OperatingSystem::Mac)[0].1[0], "FL STUDIO FIRE Jack 1");
    }
}
