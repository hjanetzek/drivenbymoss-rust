// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Mackie MCU protocol with 0-3 extenders (former `controller.mackie.mcu` and
//! the Reaper `MCU1..4ControllerInstance`).

pub mod command;
pub mod definition;
pub mod display;
pub mod mode;
pub mod setup;
pub mod surface;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// Creates MCU instances with the given number of extenders.
pub struct McuFactory {
    num_extenders: usize,
}

impl McuFactory {
    pub fn new(num_extenders: usize) -> Self {
        McuFactory { num_extenders: num_extenders.min(3) }
    }
}

const JAVA_CLASS_NAMES: [&str; 4] = [
    "de.mossgrabers.reaper.controller.mackie.mcu.MCU1ControllerInstance",
    "de.mossgrabers.reaper.controller.mackie.mcu.MCU2ControllerInstance",
    "de.mossgrabers.reaper.controller.mackie.mcu.MCU3ControllerInstance",
    "de.mossgrabers.reaper.controller.mackie.mcu.MCU4ControllerInstance",
];

impl ControllerFactory for McuFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        definition::info(self.num_extenders)
    }

    fn java_class_name(&self) -> &'static str {
        JAVA_CLASS_NAMES[self.num_extenders]
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        definition::midi_discovery_pairs(self.num_extenders, os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::mcu::for_reaper(self.num_extenders + 1)
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
