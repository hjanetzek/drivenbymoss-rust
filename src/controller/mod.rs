// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The supported controllers.

pub mod akai;
pub mod generic;
pub mod launchcontrol;
pub mod launchpad;
pub mod maschine;
pub mod mackie;

use crate::framework::ctx::Ctx;

/// The operating system, used for the automatic detection of MIDI ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingSystem {
    Windows,
    Linux,
    Mac,
}

impl OperatingSystem {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            OperatingSystem::Windows
        } else if cfg!(target_os = "macos") {
            OperatingSystem::Mac
        } else {
            OperatingSystem::Linux
        }
    }
}

/// General information about a controller (former `IControllerDefinition`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerDefinitionInfo {
    pub uuid: String,
    pub vendor: String,
    pub hardware_model: String,
    pub num_midi_in_ports: usize,
    pub num_midi_out_ports: usize,
}

impl std::fmt::Display for ControllerDefinitionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.vendor, self.hardware_model)
    }
}

/// The controller specific setup (former `IControllerSetup`). The setup creates the modes, views
/// and hardware controls in the context and handles controller specific events.
pub trait ControllerSetup {
    /// Create modes, views, controls and observers.
    fn init(&mut self, ctx: &mut Ctx);

    /// Called once after everything is set up (delayed after init in the Java version).
    fn startup(&mut self, ctx: &mut Ctx);

    /// Update the hardware.
    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
    }

    /// Handle the changes recorded by the model, the configuration and the message parser since
    /// the last call. Called before each flush.
    fn handle_changes(&mut self, ctx: &mut Ctx);

    /// Shutdown the controller.
    fn exit(&mut self, ctx: &mut Ctx);
}

/// The parts of a running controller instance created by a [`ControllerFactory`].
pub struct InstanceParts {
    pub ctx: Ctx,
    pub setup: Box<dyn ControllerSetup>,
}

/// The MIDI ports of a controller instance (the Reaper device IDs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiPorts {
    pub input: i32,
    pub output: i32,
}

/// Creates the instances of a controller type (former `IControllerDefinition` together with the
/// `*ControllerInstance` classes).
pub trait ControllerFactory {
    fn info(&self) -> ControllerDefinitionInfo;

    /// The fully qualified name of the Java instance class. It is stored in the main
    /// configuration file to remember which controllers are configured.
    fn java_class_name(&self) -> &'static str;

    /// The port names for the automatic detection of the device (input names, output names).
    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)>;

    /// Create the configuration with all settings of the controller activated.
    fn create_configuration(&self) -> crate::config::Configuration;

    /// Create a controller instance. The configuration has already been loaded.
    fn create(
        &self,
        host: crate::host::Host,
        sender: crate::backend::Sender,
        ports: MidiPorts,
        configuration: crate::config::Configuration,
    ) -> Result<InstanceParts, String>;

    /// The number of MIDI port pairs (input and output) of an instance: one per surface, e.g. a
    /// Mackie MCU with extenders (see `doc/MULTI_SURFACE_PLAN.md`). The default is 1.
    fn num_port_pairs(&self) -> usize {
        1
    }

    /// Create a controller instance with all its port pairs ([`Self::num_port_pairs`] many).
    /// The default calls [`Self::create`] with the first pair.
    fn create_with_ports(
        &self,
        host: crate::host::Host,
        sender: crate::backend::Sender,
        ports: &[MidiPorts],
        configuration: crate::config::Configuration,
    ) -> Result<InstanceParts, String> {
        let first = ports.first().copied().ok_or_else(|| "No MIDI ports".to_string())?;
        self.create(host, sender, first, configuration)
    }
}

/// All supported controllers.
pub fn all_factories() -> Vec<Box<dyn ControllerFactory>> {
    let mut factories = launchpad::factories();
    factories.extend(maschine::factories());
    factories.extend(launchcontrol::factories());
    factories.extend(akai::factories());
    factories.extend(mackie::factories());
    factories.extend(generic::factories());
    // The controllers written in Lua: built in and <ini folder>/DrivenByMoss/controllers
    #[cfg(feature = "lua")]
    factories.extend(crate::script::factories());
    factories
}

/// Find the controller with the given UUID (case is ignored).
pub fn find_factory(uuid: &str) -> Option<Box<dyn ControllerFactory>> {
    all_factories().into_iter().find(|f| f.info().uuid.eq_ignore_ascii_case(uuid))
}

/// The supported controllers as JSON, for hosts which declare each controller themselves
/// (Bitwig): an array of objects with uuid, vendor, model, the number of MIDI ports and the port
/// names for the automatic detection per operating system (`windows`, `linux`, `mac`; each a
/// list of `{"inputs": [...], "outputs": [...]}`).
pub fn catalog_json() -> String {
    let mut json = crate::json::JsonWriter::new();
    json.begin_array();
    for factory in all_factories() {
        let info = factory.info();
        json.begin_object();
        json.key("uuid").string(&info.uuid);
        json.key("vendor").string(&info.vendor);
        json.key("model").string(&info.hardware_model);
        json.key("numMidiInPorts").number(info.num_midi_in_ports as i64);
        json.key("numMidiOutPorts").number(info.num_midi_out_ports as i64);
        json.key("discovery").begin_object();
        for (key, os) in [("windows", OperatingSystem::Windows), ("linux", OperatingSystem::Linux), ("mac", OperatingSystem::Mac)] {
            json.key(key).begin_array();
            for (inputs, outputs) in factory.midi_discovery_pairs(os) {
                json.begin_object();
                json.key("inputs").string_array(&inputs);
                json.key("outputs").string_array(&outputs);
                json.end_object();
            }
            json.end_array();
        }
        json.end_object();
        json.end_object();
    }
    json.end_array();
    json.finish()
}
