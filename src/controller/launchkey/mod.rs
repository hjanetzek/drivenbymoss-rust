// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Novation Launchkey Mk3 (25, 37, 49, 61 and 88 keys) and the Launchkey Mini
//! Mk3 (former `controller.novation.launchkey.maxi` and `.mini`, and
//! `LaunchkeyMk3/LaunchkeyMiniMk3ControllerInstance`).
//!
//! Both models are one setup with the model as a parameter, since the Java packages are mostly
//! copies of each other. The Launchkey has two MIDI inputs: the DAW port (pads, knobs, faders,
//! buttons) and the keyboard port (keys, wheels, pedal), which is the further MIDI input of the
//! instance (`ControllerFactory::num_extra_midi_inputs`).
//!
//! The Launchkey Mk4 models are not supported by the Java version, so they are not ported.
//!
//! Deviations from Java (see also `doc/BITWIG_DIVERGENCES.md`):
//! - The display of the parameter mode shows the page name, not `Optional[...]`.
//! - The display sends the ISO-8859-2 bytes of the characters above 127.
//! - The state of the Select/RecArm toggle (Mk3) is kept per instance (Java: static).
//! - The lights of the mode selection pads, which only exist for the simulator, are not created.

pub mod colors;
pub mod command;
pub mod definition;
pub mod display;
pub mod mode;
pub mod setup;
pub mod surface;
pub mod view;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;
use definition::LaunchkeyModel;

/// Creates Launchkey instances of one model.
pub struct LaunchkeyFactory {
    model: LaunchkeyModel,
}

impl LaunchkeyFactory {
    pub fn new(model: LaunchkeyModel) -> Self {
        LaunchkeyFactory { model }
    }
}

impl ControllerFactory for LaunchkeyFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        self.model.info()
    }

    fn java_class_name(&self) -> &'static str {
        self.model.java_class_name()
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        self.model.midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::launchkey::for_reaper(self.model.is_mini())
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(self.model, host, sender, ports, None, configuration)
    }

    /// The keyboard port.
    fn num_extra_midi_inputs(&self) -> usize {
        1
    }

    fn create_with_extra_inputs(
        &self,
        host: Host,
        sender: Sender,
        ports: &[MidiPorts],
        extra_inputs: &[Option<i32>],
        configuration: Configuration,
    ) -> Result<InstanceParts, String> {
        let first = ports.first().copied().ok_or_else(|| "No MIDI ports".to_string())?;
        setup::create(self.model, host, sender, first, extra_inputs.first().copied().flatten(), configuration)
    }
}

/// The factories of all Launchkey models.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    LaunchkeyModel::ALL.iter().map(|m| Box::new(LaunchkeyFactory::new(*m)) as Box<dyn ControllerFactory>).collect()
}
