// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The application: manages the controller instances (former `MainApp`,
//! `ControllerInstanceManager` and `AbstractControllerInstance`).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::backend::{Backend, Sender};
use crate::config::properties::Properties;
use crate::config::reaper::{apply_document_settings, document_tag, format_document_settings, parse_document_settings, ControllerConfigFile};
use crate::config::{Scope, SettingId, SettingKind};
use crate::controller::{all_factories, find_factory, ControllerFactory, ControllerSetup, MidiPorts, OperatingSystem};
use crate::framework::ctx::Ctx;
use crate::host::{Host, MemoryLog, StderrLog, LogSink};
use crate::parser::{Parser, ParserEvent, UPDATE_NOTE_MAPPING_DELAY_MS};

/// The name of the main configuration file in the Reaper INI folder.
const MAIN_CONFIG_FILENAME: &str = "DrivenByMoss4Reaper.config";
/// The key prefix of the configured controller instances in the main configuration file.
const CONTROLLER_INSTANCE_TAG: &str = "CONTROLLER_INSTANCE";
/// The delay of the controller startup after its initialisation.
const STARTUP_DELAY_MS: u64 = 1000;

/// Logs to stderr and keeps the messages in memory (displayed with `dbm_display_window`).
struct AppLog {
    memory: Rc<MemoryLog>,
    backend: Rc<dyn Backend>,
}

impl LogSink for AppLog {
    fn log(&self, message: &str) {
        StderrLog.log(message);
        self.memory.log(message);
        self.backend.log(message);
    }
}

/// A started controller instance.
struct Running {
    ctx: Ctx,
    setup: Box<dyn ControllerSetup>,
    parser: Parser,
    /// The MIDI ports, one pair per surface (index 0 is the first surface).
    ports: Vec<MidiPorts>,
    /// When `startup` of the setup is called (delayed like in the Java version).
    startup_due: Option<Instant>,
}

/// A configured controller instance.
struct Instance {
    factory: Box<dyn ControllerFactory>,
    config_file: ControllerConfigFile,
    /// The document settings as received from Reaper before the instance was started.
    pending_document_settings: Option<Vec<(String, String)>>,
    running: Option<Running>,
}

impl Instance {
    fn hardware_model(&self) -> String {
        self.factory.info().hardware_model
    }
}

/// The application.
pub struct App {
    host: Host,
    sender: Sender,
    log: Rc<MemoryLog>,
    ini_path: PathBuf,
    instances: Vec<Instance>,
    devices: Vec<(String, String)>,
    is_started: bool,
    /// True for a host which runs exactly one controller on its own MIDI ports (Bitwig): there is
    /// no main configuration, the ports are not discovered and the controller is always enabled.
    single: bool,
    /// The time is set by [`App::set_time`] (replay of a recorded session), not taken from the
    /// clock at each call.
    fixed_time: bool,
}

impl App {
    pub fn new(backend: Rc<dyn Backend>, ini_path: &str, major_version: i32, minor_version: i32) -> Self {
        let mut app = Self::create(backend, ini_path, false);
        app.host.println(&format!("DrivenByMoss (Rust) starting, Reaper {major_version}.{minor_version}"));
        app.load_instances();
        app
    }

    /// Create the application for exactly one controller, identified by the UUID of its
    /// definition. The controller uses the host's MIDI ports 0. Returns `None` if the controller
    /// is not supported.
    pub fn new_single(backend: Rc<dyn Backend>, config_path: &str, uuid: &str, app_version: &str) -> Option<Self> {
        let factory = find_factory(uuid)?;
        let mut app = Self::create(backend, config_path, true);
        app.host.println(&format!("DrivenByMoss (Rust) starting {}, {app_version}", factory.info()));
        app.add_instance(factory);
        Some(app)
    }

    fn create(backend: Rc<dyn Backend>, ini_path: &str, single: bool) -> Self {
        let log = Rc::new(MemoryLog::default());
        let host = Host::new(Rc::new(AppLog { memory: log.clone(), backend: backend.clone() }));
        App {
            host,
            sender: Sender::new(backend),
            log,
            ini_path: PathBuf::from(ini_path),
            instances: Vec::new(),
            devices: Vec::new(),
            is_started: false,
            single,
            fixed_time: false,
        }
    }

    /// A call into the core: take the time of the call (see [`Host::now`]).
    fn enter(&self) {
        if !self.fixed_time {
            self.host.set_time(Instant::now());
        }
    }

    /// Run the core on the given time from now on, not on the clock (replay of a recorded
    /// session, tests).
    pub fn set_time(&mut self, time: Instant) {
        self.fixed_time = true;
        self.host.set_time(time);
    }

    fn main_config_path(&self) -> PathBuf {
        self.ini_path.join(MAIN_CONFIG_FILENAME)
    }

    /// Instantiate the controllers listed in the main configuration file.
    fn load_instances(&mut self) {
        let properties = Properties::load_file(&self.main_config_path()).unwrap_or_default();
        let factories = all_factories();
        let mut class_names: Vec<String> = Vec::new();
        let mut counter = 0;
        while let Some(name) = properties.get_string(&format!("{CONTROLLER_INSTANCE_TAG}{counter}")) {
            if !class_names.iter().any(|n| n == name) {
                class_names.push(name.to_string());
            }
            counter += 1;
        }
        for factory in factories {
            if class_names.iter().any(|n| n == factory.java_class_name()) {
                self.add_instance(factory);
            }
        }
        for name in &class_names {
            if !self.instances.iter().any(|i| i.factory.java_class_name() == name) {
                self.host.println(&format!("Controller not supported by the Rust version: {name}"));
            }
        }
    }

    fn add_instance(&mut self, factory: Box<dyn ControllerFactory>) {
        let path = ControllerConfigFile::path(&self.ini_path, &factory.info().hardware_model);
        let config_file = ControllerConfigFile::load(&path).unwrap_or_default();
        self.instances.push(Instance { factory, config_file, pending_document_settings: None, running: None });
    }

    /// Store the list of the configured controllers in the main configuration file.
    fn save_instances(&self) {
        let path = self.main_config_path();
        let mut properties = Properties::load_file(&path).unwrap_or_default();
        let old: Vec<String> = properties.iter().map(|(k, _)| k).filter(|k| k.starts_with(CONTROLLER_INSTANCE_TAG)).map(str::to_string).collect();
        for key in old {
            properties.remove(&key);
        }
        for (i, instance) in self.instances.iter().enumerate() {
            properties.put(&format!("{CONTROLLER_INSTANCE_TAG}{i}"), instance.factory.java_class_name());
        }
        if let Err(error) = properties.save_file(&path) {
            self.host.error(&format!("Could not store the main configuration: {error}"));
        }
    }

    /// Add a controller of the given hardware model (only one instance per type).
    pub fn add_controller(&mut self, hardware_model: &str) -> bool {
        if self.single {
            return false;
        }
        if self.instances.iter().any(|i| i.hardware_model() == hardware_model) {
            self.host.println("Only one instance of a controller type is supported!");
            return false;
        }
        let Some(factory) = all_factories().into_iter().find(|f| f.info().hardware_model == hardware_model) else {
            return false;
        };
        self.add_instance(factory);
        self.save_instances();
        if self.is_started {
            let index = self.instances.len() - 1;
            self.start_instance(index);
            self.sender.refresh();
        }
        true
    }

    /// Add an available device (a plugin), used by the device browser.
    pub fn add_device(&mut self, name: &str, identifier: &str) {
        self.devices.push((name.to_string(), identifier.to_string()));
    }

    /// Start all controller instances.
    pub fn start_infrastructure(&mut self) {
        self.enter();
        if self.is_started {
            return;
        }
        self.is_started = true;
        self.start_controllers();
    }

    fn start_controllers(&mut self) {
        for index in 0..self.instances.len() {
            self.start_instance(index);
        }
        self.sender.refresh();
    }

    /// Restart all configured controllers.
    pub fn restart_controllers(&mut self) {
        self.enter();
        for index in 0..self.instances.len() {
            self.stop_instance(index);
        }
        self.start_controllers();
    }

    /// Find the ID of the port with the given name.
    fn find_port(ports: &[(i32, String)], name: &str) -> Option<i32> {
        ports.iter().find(|(_, n)| n == name).map(|(id, _)| *id)
    }

    /// Get the configured MIDI ports of an instance (one pair per surface, see
    /// [`ControllerFactory::num_port_pairs`]), try to detect them if not configured.
    fn resolve_ports(&mut self, index: usize) -> Option<Vec<MidiPorts>> {
        let count = self.instances[index].factory.num_port_pairs().max(1);
        if self.single {
            // The host's ports are numbered 0..count
            return Some((0..count as i32).map(|i| MidiPorts { input: i, output: i }).collect());
        }
        let inputs = self.sender.backend().midi_inputs();
        let outputs = self.sender.backend().midi_outputs();
        let instance = &mut self.instances[index];
        let name = instance.factory.info().to_string();

        let configured: Vec<(Option<String>, Option<String>)> =
            (0..count).map(|i| (instance.config_file.midi_input(i).map(str::to_string), instance.config_file.midi_output(i).map(str::to_string))).collect();
        if configured.iter().all(|(input, output)| input.is_some() && output.is_some()) {
            let mut ports = Vec::with_capacity(count);
            for (input, output) in configured.iter().map(|(i, o)| (i.as_deref().unwrap_or_default(), o.as_deref().unwrap_or_default())) {
                match (Self::find_port(&inputs, input), Self::find_port(&outputs, output)) {
                    (Some(input), Some(output)) => ports.push(MidiPorts { input, output }),
                    (None, _) => {
                        self.host.println(&format!("{name}: MIDI input device not found: {input}"));
                        return None;
                    }
                    (_, None) => {
                        self.host.println(&format!("{name}: MIDI output device not found: {output}"));
                        return None;
                    }
                }
            }
            return Some(ports);
        }

        // Automatic detection: all ports of a pair must be present
        for (input_names, output_names) in instance.factory.midi_discovery_pairs(OperatingSystem::current()) {
            if input_names.len() < count || output_names.len() < count {
                continue;
            }
            let found: Option<Vec<MidiPorts>> = (0..count)
                .map(|i| match (Self::find_port(&inputs, &input_names[i]), Self::find_port(&outputs, &output_names[i])) {
                    (Some(input), Some(output)) => Some(MidiPorts { input, output }),
                    _ => None,
                })
                .collect();
            if let Some(ports) = found {
                self.host.println(&format!("{name}: Detected MIDI ports {} / {}", input_names[..count].join(", "), output_names[..count].join(", ")));
                for i in 0..count {
                    instance.config_file.set_midi_input(i, &input_names[i]);
                    instance.config_file.set_midi_output(i, &output_names[i]);
                }
                return Some(ports);
            }
        }
        self.host.println(&format!("{name}: MIDI input/output device not configured."));
        None
    }

    fn start_instance(&mut self, index: usize) {
        if self.instances[index].running.is_some() {
            return;
        }
        let name = self.instances[index].factory.info().to_string();
        if !self.single && !self.instances[index].config_file.is_enabled() {
            self.host.println(&format!("{name}: Deactivated."));
            return;
        }
        let Some(ports) = self.resolve_ports(index) else {
            return;
        };
        self.host.println(&format!("{name}: Starting..."));

        let backend = self.sender.backend().clone();
        if !ports.iter().all(|p| backend.open_midi_input(p.input) && backend.open_midi_output(p.output)) {
            self.host.error(&format!("{name}: Could not open the MIDI ports."));
            return;
        }

        let instance = &mut self.instances[index];
        let mut configuration = instance.factory.create_configuration();
        instance.config_file.apply_to(&mut configuration);
        if let Some(parsed) = instance.pending_document_settings.take() {
            apply_document_settings(&mut configuration, &instance.hardware_model(), &parsed);
        }

        // The note clips which the views request while they are created (former
        // model.getNoteClip in the constructors), so that the host can create them during init
        crate::daw::clip::take_requested_note_clips();
        match instance.factory.create_with_ports(self.host.clone(), self.sender.clone(), &ports, configuration) {
            Ok(mut parts) => {
                parts.setup.init(&mut parts.ctx);
                for (cols, rows) in crate::daw::clip::take_requested_note_clips() {
                    parts.ctx.model.note_clip(cols, rows);
                }
                parts.ctx.config.mark_all_changed();
                let startup_due = Some(self.host.now() + Duration::from_millis(STARTUP_DELAY_MS));
                instance.running = Some(Running { ctx: parts.ctx, setup: parts.setup, parser: Parser::new(), ports, startup_due });
                self.host.println(&format!("{name}: Running."));
            }
            Err(error) => {
                self.host.error(&format!("{name}: Could not start controller: {error}"));
                for port in &ports {
                    backend.close_midi_input(port.input);
                    backend.close_midi_output(port.output);
                }
            }
        }
    }

    fn stop_instance(&mut self, index: usize) {
        let instance = &mut self.instances[index];
        let Some(mut running) = instance.running.take() else {
            return;
        };
        let path = ControllerConfigFile::path(&self.ini_path, &instance.hardware_model());
        if let Err(error) = instance.config_file.save(&path, &running.ctx.config) {
            self.host.error(&format!("Could not store the controller configuration: {error}"));
        }
        running.setup.exit(&mut running.ctx);
        running.ctx.model.cleanup();
        let backend = self.sender.backend().clone();
        for port in &running.ports {
            backend.close_midi_input(port.input);
            backend.close_midi_output(port.output);
        }
        self.host.println(&format!("{}: Exited.", instance.factory.info()));
    }

    /// Shutdown all controllers.
    pub fn shutdown(&mut self) {
        for index in 0..self.instances.len() {
            self.stop_instance(index);
        }
    }

    /// Update the model from Reaper data: lines of `<address> <value>`.
    pub fn update_model(&mut self, data: &str) {
        self.enter();
        for line in data.split('\n') {
            if line.is_empty() {
                continue;
            }
            for instance in &mut self.instances {
                if let Some(running) = &mut instance.running {
                    let events = running.parser.parse_line(&mut running.ctx.model, line);
                    handle_parser_events(&mut running.ctx, events);
                }
            }
        }
    }

    /// Handle MIDI received from a Reaper MIDI input.
    pub fn on_midi_event(&mut self, device_id: i32, data: &[u8]) {
        self.enter();
        let Some(&status) = data.first() else {
            return;
        };
        for instance in &mut self.instances {
            let Some(running) = &mut instance.running else {
                continue;
            };
            // The surface whose input received the message (several with MCU extenders)
            let Some(surface) = running.ports.iter().position(|p| p.input == device_id) else {
                continue;
            };
            if status == 0xF0 {
                running.ctx.with_surface(surface, |ctx| ctx.handle_sysex(data));
            } else if data.len() == 3 {
                running.ctx.with_surface(surface, |ctx| ctx.handle_midi(status as i32, data[1] as i32, data[2] as i32));
            } else if !(data.len() == 1 && status == 0xFE) {
                self.host.println(&format!("Unknown MIDI data of length {}", data.len()));
            }
            return;
        }
    }

    /// Execute scheduled tasks and update the controllers (called every 30ms).
    pub fn tick(&mut self) {
        self.enter();
        let now = self.host.now();
        for instance in &mut self.instances {
            let Some(running) = &mut instance.running else {
                continue;
            };
            running.ctx.run_due_tasks(now);
            if running.startup_due.is_some_and(|due| due <= now) {
                running.startup_due = None;
                running.setup.startup(&mut running.ctx);
            }
            running.setup.handle_changes(&mut running.ctx);
            running.setup.flush(&mut running.ctx);
        }
        // Restart the instances which requested it (former IHost.restart)
        let restart: Vec<usize> =
            (0..self.instances.len()).filter(|&index| self.instances[index].running.as_ref().is_some_and(|running| running.ctx.restart_requested)).collect();
        for index in restart {
            self.stop_instance(index);
            self.start_instance(index);
        }
    }

    /// Reset the document settings of all controllers to their defaults.
    pub fn set_default_document_settings(&mut self) {
        self.enter();
        for instance in &mut self.instances {
            if let Some(running) = &mut instance.running {
                running.ctx.config.reset_scope(Scope::Document);
            }
        }
    }

    /// Get the document settings of all controllers formatted for the Reaper project.
    pub fn formatted_document_settings(&self) -> String {
        let entries: Vec<(String, String)> = self
            .instances
            .iter()
            .filter_map(|i| i.running.as_ref().map(|r| (document_tag(&i.hardware_model()), r.ctx.config.store_document_settings())))
            .collect();
        format_document_settings(&entries)
    }

    /// Apply the document settings from the Reaper project.
    pub fn set_formatted_document_settings(&mut self, data: &str) {
        self.enter();
        let parsed = parse_document_settings(data);
        for instance in &mut self.instances {
            let model = instance.hardware_model();
            match &mut instance.running {
                Some(running) => {
                    apply_document_settings(&mut running.ctx.config, &model, &parsed);
                }
                None => instance.pending_document_settings = Some(parsed.clone()),
            }
        }
    }

    // ----------------------------------------------------------------------------------------
    // Settings and notifications for hosts with their own UI (Bitwig: controller preferences,
    // document state and popup notifications). They address the first running controller, such a
    // host runs one controller per instance (see `new_single`).

    fn first_running(&mut self) -> Option<&mut Running> {
        self.instances.iter_mut().find_map(|i| i.running.as_mut())
    }

    /// All settings of the controller in the order of their activation, one line per setting with
    /// tab separated fields: ID, scope (`global`, `document`), kind (`enum`, `integer`,
    /// `boolean`, `text`, `action`, `signal`), category, label, default value, current value, then the
    /// options of an enumeration, the minimum and maximum of an integer or the button text of a
    /// signal. An `action` value is the ID of a host action; a `signal` has no value, the host
    /// sets it (to any value) when the button is pressed.
    pub fn settings(&mut self) -> String {
        self.enter();
        let Some(running) = self.first_running() else {
            return String::new();
        };
        let config = &running.ctx.config;
        let mut lines = Vec::new();
        for descriptor in config.descriptors() {
            let scope = match descriptor.scope {
                Scope::Global => "global",
                Scope::Document => "document",
            };
            let (kind, extra) = match &descriptor.kind {
                SettingKind::Enum { options } => ("enum", options.clone()),
                SettingKind::Integer { min, max } => ("integer", vec![min.to_string(), max.to_string()]),
                SettingKind::Boolean => ("boolean", Vec::new()),
                SettingKind::Text => ("text", Vec::new()),
                SettingKind::Action => ("action", Vec::new()),
                SettingKind::Signal { button } => ("signal", vec![button.clone()]),
            };
            let value = config.value(descriptor.id).unwrap_or_default();
            let mut fields = vec![descriptor.id.0.to_string(), scope.to_string(), kind.to_string(), descriptor.category.clone(), descriptor.label.clone(), descriptor.default_value.clone(), value];
            fields.extend(extra);
            lines.push(fields.iter().map(|f| f.replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t"));
        }
        lines.join("\n")
    }

    /// Set a setting from the UI of the host (the change is not reported back by
    /// [`Self::take_setting_changes`]).
    pub fn set_setting(&mut self, id: i32, value: &str) {
        self.enter();
        let Ok(id) = u16::try_from(id) else {
            return;
        };
        if let Some(running) = self.first_running() {
            running.ctx.config.set_value_from_host(SettingId(id), value);
        }
    }

    /// The settings which the controller changed since the last call, one `ID<tab>value` per
    /// line, to update the UI of the host.
    pub fn take_setting_changes(&mut self) -> String {
        self.enter();
        let Some(running) = self.first_running() else {
            return String::new();
        };
        let config = &mut running.ctx.config;
        let changes = config.take_host_changes();
        changes.iter().filter_map(|&id| config.value(id).map(|value| format!("{}\t{value}", id.0))).collect::<Vec<_>>().join("\n")
    }

    /// The settings whose visibility or enabled state changed since the last call, one per line:
    /// ID, visible (1/0), enabled (1/0), tab separated. The first call after the start of the
    /// controller reports the states its configuration set while it was created.
    pub fn take_setting_states(&mut self) -> String {
        self.enter();
        let Some(running) = self.first_running() else {
            return String::new();
        };
        let config = &mut running.ctx.config;
        let changes = config.take_state_changes();
        changes.iter().map(|&id| format!("{}\t{}\t{}", id.0, u8::from(config.is_visible(id)), u8::from(config.is_enabled(id)))).collect::<Vec<_>>().join("\n")
    }

    /// The notifications since the last call, one per line (popup notifications of the host).
    pub fn take_notifications(&mut self) -> String {
        self.host.take_notifications().iter().map(|n| n.replace('\n', " ")).collect::<Vec<_>>().join("\n")
    }

    /// There is no UI in the Rust version: print the status and the recent log instead.
    pub fn display_window(&mut self) {
        self.enter();
        let lines: Vec<String> = self.log.lines.borrow().iter().rev().take(20).rev().cloned().collect();
        for line in lines {
            eprintln!("[DrivenByMoss] {line}");
        }
        for instance in &self.instances {
            let state = if instance.running.is_some() { "running" } else { "stopped" };
            eprintln!("[DrivenByMoss] {}: {state}", instance.factory.info());
        }
    }

    /// The INI path (folder of the configuration files).
    pub fn ini_path(&self) -> &Path {
        &self.ini_path
    }

    /// Number of configured controller instances.
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// The model setup of the first running instance as JSON (see [`ModelSetup::to_json`]), or
    /// an empty string if no instance is running.
    ///
    /// [`ModelSetup::to_json`]: crate::daw::model_setup::ModelSetup::to_json
    pub fn model_setup_json(&self) -> String {
        self.instances.iter().find_map(|i| i.running.as_ref()).map(|r| r.ctx.model.setup_json()).unwrap_or_default()
    }

    /// Access the context of a running instance (for tests).
    pub fn instance_ctx(&mut self, index: usize) -> Option<&mut Ctx> {
        self.instances.get_mut(index).and_then(|i| i.running.as_mut()).map(|r| &mut r.ctx)
    }
}

/// Handle the events which the parser created for the controller.
fn handle_parser_events(ctx: &mut Ctx, events: Vec<ParserEvent>) {
    for event in events {
        match event {
            // All surfaces (one, except for e.g. MCU extenders)
            ParserEvent::RebindKnobs => ctx.for_each_surface(|ctx| {
                if let Some(mode) = ctx.active_mode() {
                    if let Ok(mut mode) = mode.try_borrow_mut() {
                        mode.on_activate(ctx);
                    }
                }
            }),
            ParserEvent::UpdateNoteMapping => ctx.schedule(UPDATE_NOTE_MAPPING_DELAY_MS, |ctx| ctx.update_view_note_mapping()),
            ParserEvent::QuantizeAmount(value) => ctx.config.set_quantize_amount(value),
            ParserEvent::NoteRepeatActive(active) => {
                ctx.surface.input.default_note_input_mut().note_repeat.is_active = active;
                ctx.config.set_note_repeat_active(active);
            }
            ParserEvent::NoteRepeatPeriod(period) => ctx.surface.input.default_note_input_mut().note_repeat.period = period,
            ParserEvent::NoteRepeatNoteLength(length) => ctx.surface.input.default_note_input_mut().note_repeat.note_length = length,
            ParserEvent::NoteRepeatMode(mode) => ctx.surface.input.default_note_input_mut().note_repeat.mode = mode.max(0) as usize,
            ParserEvent::NoteRepeatUsePressure(use_pressure) => ctx.surface.input.default_note_input_mut().note_repeat.use_pressure = use_pressure,
            ParserEvent::ActionSelected(_) => {}
        }
    }
}
