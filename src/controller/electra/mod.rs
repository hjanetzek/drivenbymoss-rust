// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Electra One Mk1/Mk2 and the Electra One Mini (former
//! `controller.electra.one.base`, `.normal` and `.mini`, and
//! `ElectraOne/ElectraOneMiniControllerInstance`).
//!
//! Both are one setup with the variant as a parameter, since they share the base package. The
//! device has two port pairs: Port 1 (the controls, CCs on MIDI channel 16, and notes on the
//! channels 1-15 into the DAW) and CTRL (the SysEx API, the further MIDI input and output of the
//! instance). The DrivenByMoss preset (`DrivenByMoss.eproj` / `DrivenByMoss Mini.eproj` of the
//! Java version) has to be installed on the device; the controller selects it, follows its pages
//! with its modes, and updates its controls with JSON (name, color, visibility), value texts and
//! the Lua function `sgt` of the preset (group labels). See `surface` for the conversation.
//!
//! Deviations from Java (see also `doc/BITWIG_DIVERGENCES.md` §9 and the module documentation of
//! `surface` and `setup`): errors of the handshake are logged instead of thrown; going offline
//! twice keeps the mode of the DrivenByMoss preset; the note inputs filter the channel messages
//! of their channel.

pub mod colors;
pub mod command;
pub mod definition;
pub mod mode;
pub mod page_cache;
pub mod setup;
pub mod surface;

use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem};
use crate::host::Host;
use definition::ElectraVariant;

/// Creates Electra One instances of one variant.
pub struct ElectraOneFactory {
    variant: ElectraVariant,
}

impl ElectraOneFactory {
    pub fn new(variant: ElectraVariant) -> Self {
        ElectraOneFactory { variant }
    }
}

impl ControllerFactory for ElectraOneFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        self.variant.info()
    }

    fn java_class_name(&self) -> &'static str {
        self.variant.java_class_name()
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        self.variant.midi_discovery_pairs(os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::electra::for_reaper(self.variant == ElectraVariant::Mini)
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(self.variant, host, sender, ports, None, configuration)
    }

    /// The CTRL port.
    fn num_extra_midi_inputs(&self) -> usize {
        1
    }

    /// The CTRL port.
    fn num_extra_midi_outputs(&self) -> usize {
        1
    }

    fn create_with_extra_ports(
        &self,
        host: Host,
        sender: Sender,
        ports: &[MidiPorts],
        _extra_inputs: &[Option<i32>],
        extra_outputs: &[Option<i32>],
        configuration: Configuration,
    ) -> Result<InstanceParts, String> {
        let first = ports.first().copied().ok_or_else(|| "No MIDI ports".to_string())?;
        setup::create(self.variant, host, sender, first, extra_outputs.first().copied().flatten(), configuration)
    }
}

/// The factories of both variants.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    ElectraVariant::ALL.iter().map(|v| Box::new(ElectraOneFactory::new(*v)) as Box<dyn ControllerFactory>).collect()
}
