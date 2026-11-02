// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Mackie HUI protocol with 0-2 extenders (former `controller.mackie.hui` and
//! the Reaper `HUI1..3ControllerInstance`).

pub mod definition;
pub mod display;
pub mod mode;
pub mod setup;
pub mod surface;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// Creates HUI instances with the given number of extenders.
pub struct HuiFactory {
    num_extenders: usize,
}

impl HuiFactory {
    pub fn new(num_extenders: usize) -> Self {
        HuiFactory { num_extenders: num_extenders.min(2) }
    }
}

const JAVA_CLASS_NAMES: [&str; 3] = [
    "de.mossgrabers.reaper.controller.mackie.hui.HUI1ControllerInstance",
    "de.mossgrabers.reaper.controller.mackie.hui.HUI2ControllerInstance",
    "de.mossgrabers.reaper.controller.mackie.hui.HUI3ControllerInstance",
];

impl ControllerFactory for HuiFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        definition::info(self.num_extenders)
    }

    fn java_class_name(&self) -> &'static str {
        JAVA_CLASS_NAMES[self.num_extenders]
    }

    /// The HUI has no automatic detection (as in Java).
    fn midi_discovery_pairs(&self, _os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        Vec::new()
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::hui::for_reaper()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(host, sender, &vec![ports; self.num_extenders + 1], configuration)
    }

    fn num_port_pairs(&self) -> usize {
        self.num_extenders + 1
    }

    fn create_with_ports(&self, host: Host, sender: Sender, ports: &[MidiPorts], configuration: Configuration) -> Result<InstanceParts, String> {
        if ports.len() < self.num_extenders + 1 {
            return Err(format!("{} MIDI port pairs are required", self.num_extenders + 1));
        }
        setup::create(host, sender, ports, configuration)
    }
}
