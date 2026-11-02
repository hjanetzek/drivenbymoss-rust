// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Akai APC mini Mk1 and Mk2 (former `controller.akai.apcmini` and
//! `APCminiMk1/Mk2ControllerInstance`).

pub mod colors;
pub mod definition;
pub mod setup;
pub mod surface;
pub mod view;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;
use definition::{ApcMiniDefinition, ApcMiniModel};

/// Creates APC mini instances of one model.
pub struct ApcMiniFactory {
    definition: ApcMiniDefinition,
}

impl ApcMiniFactory {
    pub fn new(model: ApcMiniModel) -> Self {
        ApcMiniFactory { definition: ApcMiniDefinition::new(model) }
    }
}

impl ControllerFactory for ApcMiniFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        self.definition.info()
    }

    fn java_class_name(&self) -> &'static str {
        match self.definition.model {
            ApcMiniModel::Mk1 => "de.mossgrabers.reaper.controller.akai.apcmini.APCminiMk1ControllerInstance",
            ApcMiniModel::Mk2 => "de.mossgrabers.reaper.controller.akai.apcmini.APCminiMk2ControllerInstance",
        }
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        self.definition.midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::apcmini::for_reaper(self.definition.has_brightness())
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(self.definition, host, sender, ports, configuration)
    }
}

/// The factories of all APC mini models.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    ApcMiniModel::ALL.iter().map(|m| Box::new(ApcMiniFactory::new(*m)) as Box<dyn ControllerFactory>).collect()
}
