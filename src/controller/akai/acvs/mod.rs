// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Akai devices with the ACVS protocol: MPC Live I/II, One, X and Force (former
//! `controller.akai.acvs` and `ACVSLiveControllerInstance`).

pub mod colors;
pub mod definition;
pub mod display;
pub mod mode;
pub mod screen_item;
pub mod setup;
pub mod surface;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// Creates ACVS instances.
pub struct AcvsFactory;

impl ControllerFactory for AcvsFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        definition::info()
    }

    fn java_class_name(&self) -> &'static str {
        "de.mossgrabers.reaper.controller.akai.acvs.ACVSLiveControllerInstance"
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        definition::midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::acvs::for_reaper()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(host, sender, ports, configuration)
    }
}

/// The factories of the ACVS controllers.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    vec![Box::new(AcvsFactory)]
}
