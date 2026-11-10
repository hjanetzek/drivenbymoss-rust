// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Open Sound Control (OSC) protocol (former `controller.osc`,
//! `OSCControllerDefinition` and `OSCControllerInstance`). The protocol is described in the
//! DrivenByMoss manual ("Open Sound Control (OSC)").
//!
//! The controller has one MIDI input (the virtual keyboard inserts its notes there) and no MIDI
//! output; it talks to its clients over UDP (see [`setup`] for the networking and the
//! differences to Java, the modules in [`module`]).
//!
//! Manual test on Linux (receive port 8000, send port 9000 by default): run the controller in a
//! host (Bitwig, or the Python reference host), then
//!
//! ```text
//! backends/bitwig/tools/osc_monitor.py --port 9000 --quiet-vu      # prints what it sends
//! backends/bitwig/tools/osc_monitor.py --send /track/1/volume 100  # sends to port 8000
//! oscsend localhost 8000 /refresh                                  # liblo-tools
//! oscdump 9000
//! ```

pub mod colors;
pub mod module;
pub mod setup;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// The UUID of the Java extension (former `EXTENSION_ID`).
pub const OSC_UUID: &str = "94DD41B0-EFEE-11E3-AC10-0800200C9A66";

/// Creates OSC controller instances.
pub struct OscFactory;

impl ControllerFactory for OscFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        ControllerDefinitionInfo {
            uuid: OSC_UUID.to_string(),
            vendor: "Open Sound Control".to_string(),
            hardware_model: "OSC".to_string(),
            num_midi_in_ports: 1,
            num_midi_out_ports: 0,
        }
    }

    fn java_class_name(&self) -> &'static str {
        "de.mossgrabers.reaper.controller.osc.OSCControllerInstance"
    }

    fn midi_discovery_pairs(&self, _os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        Vec::new()
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::osc::for_reaper()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(host, sender, ports, configuration)
    }
}

/// The factories of the OSC controller.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    vec![Box::new(OscFactory)]
}
#[cfg(test)]
mod tests;
#[cfg(test)]
mod golden;
