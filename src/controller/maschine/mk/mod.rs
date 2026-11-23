// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Maschine Mk2, Mk3, Mikro Mk3, Plus and Studio, which share one setup (former
//! package `controller.ni.maschine.mk3`). The Mk2, Mk3, Mikro Mk3 and Plus are registered; the
//! Studio misses its monitor-encoder section.

pub mod definition;
pub mod mode;
pub mod surface;
pub mod view;
pub mod command;
pub mod setup;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;
use definition::MaschineDefinition;

/// Creates the instances of one Maschine model (former `Maschine*ControllerDefinition` and
/// `Maschine*ControllerInstance`).
pub struct MaschineFactory {
    definition: MaschineDefinition,
}

impl MaschineFactory {
    pub fn new(maschine: Maschine) -> Self {
        MaschineFactory { definition: MaschineDefinition::new(maschine) }
    }
}

impl ControllerFactory for MaschineFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        self.definition.info()
    }

    fn java_class_name(&self) -> &'static str {
        self.definition.java_class_name()
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        self.definition.midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        setup::create_configuration(self.definition.maschine)
    }

    /// The Mk2 over libdbm_nihost on Linux (the Mk3, Mikro Mk3 and Plus have NI's driver only).
    fn hid_device(&self) -> Option<crate::nihost::HidDevice> {
        (self.definition.maschine == Maschine::Mk2).then_some(crate::nihost::HidDevice::MK2)
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(self.definition, host, sender, ports, configuration)
    }
}

/// The registered models. The Studio is not: its monitor section (the monitor encoder with its
/// modes and VU, the MST/GRP/SND/CUE/IN1-4 buttons) is not ported.
pub const SUPPORTED_MODELS: [Maschine; 4] = [Maschine::Mk2, Maschine::Mk3, Maschine::MikroMk3, Maschine::Plus];

/// The factories of the supported models.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    SUPPORTED_MODELS.iter().map(|m| Box::new(MaschineFactory::new(*m)) as Box<dyn ControllerFactory>).collect()
}
