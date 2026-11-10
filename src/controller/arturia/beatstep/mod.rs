// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Arturia Beatstep and Beatstep Pro (former `controller.arturia.beatstep` and
//! `BeatstepControllerInstance`). Both models use the same definition: the Beatstep Pro sends
//! the same MIDI on its controller channel.

pub mod colors;
pub mod command;
pub mod definition;
pub mod setup;
pub mod surface;
pub mod view;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// Creates Beatstep instances.
#[derive(Default)]
pub struct BeatstepFactory;

impl ControllerFactory for BeatstepFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        definition::info()
    }

    fn java_class_name(&self) -> &'static str {
        "de.mossgrabers.reaper.controller.arturia.beatstep.BeatstepControllerInstance"
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        definition::midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::beatstep::for_reaper()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(host, sender, ports, configuration)
    }
}

/// The factories of the Beatstep.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    vec![Box::new(BeatstepFactory)]
}
