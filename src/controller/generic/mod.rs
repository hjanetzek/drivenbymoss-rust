// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for generic MIDI controllers with user defined mappings (former
//! `controller.generic` and `GenericFlexiControllerInstance`).
//!
//! The user maps MIDI CCs, notes, program changes, pitchbend and MMC commands to functions
//! (commands, see [`command::FlexiCommand`]) in up to 300 command slots. The slots are stored in
//! a mapping file (see [`mapping`]) and can be edited with the settings of the slot editor (see
//! [`editor`]).

pub mod command;
pub mod definition;
pub mod editor;
pub mod handler;
pub mod mapping;
pub mod setup;
pub mod slot;
pub mod surface;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;

/// Creates Generic Flexi instances.
pub struct GenericFlexiFactory;

impl ControllerFactory for GenericFlexiFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        definition::info()
    }

    fn java_class_name(&self) -> &'static str {
        "de.mossgrabers.reaper.controller.generic.GenericFlexiControllerInstance"
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        definition::midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::generic_flexi::for_reaper()
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(host, sender, ports, configuration)
    }
}

/// The factories of the generic controllers.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    vec![Box::new(GenericFlexiFactory)]
}
