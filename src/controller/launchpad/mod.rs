// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Novation Launchpad controllers (MkII, Pro, Pro Mk3, X, Mini MkIII).

pub mod colors;
pub mod definition;
pub mod surface;
pub mod setup;

use crate::backend::Sender;
use crate::config::launchpad::{LaunchpadConfigParams, LaunchpadConfiguration};
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;
use definition::{LaunchpadDefinition, LaunchpadModel};

/// Creates Launchpad instances of one model.
pub struct LaunchpadFactory {
    definition: LaunchpadDefinition,
}

impl LaunchpadFactory {
    pub fn new(model: LaunchpadModel) -> Self {
        LaunchpadFactory { definition: LaunchpadDefinition::new(model) }
    }
}

impl ControllerFactory for LaunchpadFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        self.definition.info()
    }

    fn java_class_name(&self) -> &'static str {
        match self.definition.model {
            LaunchpadModel::MkII => "de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadMkIIControllerInstance",
            LaunchpadModel::Pro => "de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadProControllerInstance",
            LaunchpadModel::ProMk3 => "de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadProMk3ControllerInstance",
            LaunchpadModel::X => "de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadXControllerInstance",
            LaunchpadModel::MiniMkIII => "de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadMiniMkIIIControllerInstance",
        }
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        self.definition.midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        LaunchpadConfiguration::for_reaper(LaunchpadConfigParams { has_brightness: self.definition.supports_brightness() }).into_base()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(self.definition.clone(), host, sender, ports, configuration)
    }
}

/// The factories of all Launchpad models.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    LaunchpadModel::ALL.iter().map(|m| Box::new(LaunchpadFactory::new(*m)) as Box<dyn ControllerFactory>).collect()
}
pub mod command;
pub mod view;
