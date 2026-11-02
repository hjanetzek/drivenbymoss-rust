// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Akai APC40 Mk1 and Mk2 (former `controller.akai.apc` and
//! `APC40mkI/mkIIControllerInstance`).

pub mod colors;
pub mod command;
pub mod definition;
pub mod mode;
pub mod setup;
pub mod surface;
pub mod view;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;
use definition::ApcDefinition;

/// Creates APC40 instances of one model.
pub struct ApcFactory {
    definition: ApcDefinition,
}

impl ApcFactory {
    pub fn new(is_mk2: bool) -> Self {
        ApcFactory { definition: ApcDefinition::new(is_mk2) }
    }
}

impl ControllerFactory for ApcFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        self.definition.info()
    }

    fn java_class_name(&self) -> &'static str {
        if self.definition.is_mk2 {
            "de.mossgrabers.reaper.controller.akai.apc.APC40mkIIControllerInstance"
        } else {
            "de.mossgrabers.reaper.controller.akai.apc.APC40mkIControllerInstance"
        }
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        self.definition.midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::apc::for_reaper(self.definition.is_mk2)
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(self.definition, host, sender, ports, configuration)
    }
}

/// The factories of both APC40 models.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    vec![Box::new(ApcFactory::new(false)), Box::new(ApcFactory::new(true))]
}
