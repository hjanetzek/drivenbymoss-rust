// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Native Instruments Maschine JAM (former package
//! `controller.ni.maschine.jam`).

pub mod definition;
pub mod surface;
pub mod view;
pub mod mode;
pub mod command;
pub mod setup;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// Creates Maschine JAM instances (former `MaschineJamControllerDefinition` and
/// `MaschineJamControllerInstance`).
pub struct MaschineJamFactory;

impl ControllerFactory for MaschineJamFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        definition::info()
    }

    fn java_class_name(&self) -> &'static str {
        "de.mossgrabers.reaper.controller.ni.maschine.jam.MaschineJamControllerInstance"
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        definition::midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        setup::create_configuration()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(host, sender, ports, configuration)
    }
}
