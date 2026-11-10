// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the OSC controller (former `OSCControllerSetup`, `OSCParser`, `OSCWriter`,
//! `AbstractOpenSoundControlParser` and `AbstractOpenSoundControlWriter.flush`).
//!
//! Networking and threading: the controller owns a non-blocking UDP server socket (the receive
//! port) and a non-blocking client socket (to the send host and port). Both are only used in the
//! calls of the core, which are serialized per instance (`doc/HOST_BRIDGE_GUIDE.md`):
//! [`ControllerSetup::handle_changes`] reads the datagrams which arrived since the last tick and
//! executes their commands, [`ControllerSetup::flush`] sends the changed values. So the
//! commands of a client are executed at the next tick of the core (every 30 ms), as Bitwig runs
//! the OSC callbacks of the Java version on its control thread. No thread, no new C function.
//!
//! Differences to Java:
//! - The modules are flushed at every tick (the Java version flushes when Bitwig calls
//!   `flush`); only changed values are sent in both.
//! - A changed value resolution restarts the controller instead of changing the value changer
//!   (the model keeps copies of the value changer).
//! - The bundles of a large update are sent one per tick (Java sleeps 10 ms between them).
//! - The `ConformanceModule` of the Java branch `conformance-osc` (additional values for the
//!   conformance tests of the Rust port) is not ported: it reads Bitwig's state for the test
//!   runner, independent of the port.

use std::time::{Duration, Instant};

use super::address::{self, OscIn, Parsed, SetupIn};
use super::colors::create_color_manager;
use super::module::action::ActionModule;
use super::module::browser::BrowserModule;
use super::module::clip::ClipModule;
use super::module::device::DeviceModule;
use super::module::global::GlobalModule;
use super::module::layout::LayoutModule;
use super::module::marker::MarkerModule;
use super::module::midi::MidiModule;
use super::module::project::ProjectModule;
use super::module::scene::SceneModule;
use super::module::track::TrackModule;
use super::module::transport::TransportModule;
use super::module::{check_guard, OscError, OscResult, OscShared, Value};
use crate::backend::Sender;
use crate::config::osc::{self as config, ValueResolution};
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::constants::DeviceId;
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::log_text;
use crate::framework::osc::{OscClient, OscMessage, OscServer};
use crate::framework::setup_helpers;
use crate::framework::surface::Surface;
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{MidiInput, MidiOutput};
use crate::scale::Scales;

/// The address which frames every update (former `flush ("/update")`).
pub const UPDATE_ADDRESS: &str = "/update";

/// The messages which are not logged if the heartbeat filter is on (former
/// `OSCWriter.HEARTBEAT_MESSAGES`).
const HEARTBEAT_MESSAGES: [&str; 3] = ["/update", "/time/str", "/beat/str"];

/// The delay of the first complete update after the startup (former `startup`).
const INITIAL_DUMP_DELAY_MS: u64 = 1000;

fn is_heartbeat_message(address: &str) -> bool {
    HEARTBEAT_MESSAGES.contains(&address)
}

/// Create an OSC controller instance.
pub fn create(host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let colors = create_color_manager();
    // The value resolution of the configuration (the Java observer of VALUE_RESOLUTION)
    let (upper_bound, step_size) = config::value_resolution(&configuration).value_changer_range();
    let value_changer = ValueChanger::twos_complement(upper_bound, step_size);

    // createScales
    let mut scales = Scales::new(&value_changer, 0, 128, 128, 1);
    scales.set_chromatic(true);

    // createModel
    let bank_page_size = config::bank_page_size(&configuration);
    let model_setup = model_setup(bank_page_size, configuration.is_track_navigation_flat());
    let model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);

    // createSurface: the MIDI input is only used to insert notes into the DAW, there is no output
    let output = MidiOutput::new(sender.clone(), None);
    let input = MidiInput::new(sender.clone(), Some(ports.input), &[])?;
    let surface = Surface::new(output, input, None);

    let ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    Ok(InstanceParts { ctx, setup: Box::new(OscSetup::new(bank_page_size)) })
}

/// The model setup (former `createModel`).
pub fn model_setup(bank_page_size: usize, flat_track_list: bool) -> ModelSetup {
    let size = bank_page_size as i32;
    let mut ms = ModelSetup::new();
    ms.device_ids = vec![DeviceId::FirstInstrument, DeviceId::Eq];
    ms.wants_focused_parameter = true;
    ms.num_tracks = bank_page_size;
    ms.num_scenes = bank_page_size;
    ms.has_flat_track_list = flat_track_list;
    ms.num_sends = bank_page_size;
    ms.num_devices_in_bank = size;
    ms.num_device_layers = size;
    ms.num_param_pages = bank_page_size;
    ms.num_params = size;
    ms.num_markers = bank_page_size;
    ms
}

/// The modules (former `OSCParser.modules`), flushed in this order.
struct Modules {
    transport: TransportModule,
    global: GlobalModule,
    layout: LayoutModule,
    marker: MarkerModule,
    project: ProjectModule,
    track: TrackModule,
    scene: SceneModule,
    device: DeviceModule,
    browser: BrowserModule,
    midi: MidiModule,
    action: ActionModule,
    clip: ClipModule,
}

/// The OSC controller setup.
pub struct OscSetup {
    shared: OscShared,
    modules: Modules,
    server: OscServer,
    client: Option<OscClient>,
    /// The value resolution the controller was created with.
    resolution: Option<ValueResolution>,
    /// When the first complete update is due (former `startup`).
    dump_due: Option<Instant>,
    /// The last error of the client, which is logged only once.
    last_send_error: Option<String>,
}

impl OscSetup {
    pub fn new(bank_page_size: usize) -> Self {
        let mut shared = OscShared::new();
        let w = &mut shared.writer;
        let modules = Modules {
            transport: TransportModule::new(w),
            global: GlobalModule,
            layout: LayoutModule::new(w),
            marker: MarkerModule::new(),
            project: ProjectModule::new(w, bank_page_size),
            track: TrackModule::new(w, bank_page_size),
            scene: SceneModule::new(),
            device: DeviceModule::new(w, bank_page_size),
            browser: BrowserModule::new(w),
            midi: MidiModule::new(w),
            action: ActionModule,
            clip: ClipModule::new(w),
        };
        OscSetup { shared, modules, server: OscServer::new(), client: None, resolution: None, dump_due: None, last_send_error: None }
    }

    /// The port the server listens on (None if it is not running).
    pub fn listening_port(&self) -> Option<u16> {
        self.server.listening_port()
    }

    /// Start the OSC server on the configured receive port (former `startOSCServer`).
    fn start_osc_server(&mut self, ctx: &mut Ctx) {
        let receive_port = config::receive_port(&ctx.config);
        if receive_port == config::send_port(&ctx.config) {
            let message = format!("Could not start OSC server. OSC send and receive port must be different! Both are: {receive_port}");
            ctx.host.show_notification(&message);
            ctx.host.println(&message);
            return;
        }
        if self.server.listening_port() == Some(receive_port) {
            return;
        }
        match self.server.start(receive_port) {
            Ok(()) => ctx.host.println(&format!("Started OSC server on port {receive_port}.")),
            Err(error) => ctx.host.error(&format!("Could not start OSC server. {error}")),
        }
    }

    /// Execute a received message (former `OSCParser.handle`).
    pub fn handle_message(&mut self, ctx: &mut Ctx, message: &OscMessage) {
        // Former logMessage (the parser does not filter the heartbeat messages)
        if config::should_log_input_commands(&ctx.config) {
            ctx.host.println(&format!("Receiving: {}", message.log_text()));
        }

        let Some(parsed) = address::parse(&message.address) else {
            return;
        };
        let value = Value::from_args(&message.args);
        let result = self.execute(ctx, parsed, value);
        let address = &message.address;
        match result {
            Ok(()) => {}
            Err(OscError::IllegalParameter(message)) => ctx.host.println(&format!("Illegal parameter: {address} {message}")),
            Err(OscError::UnknownCommand(message)) => ctx.host.println(&format!("Unknown OSC command: {address} {message}")),
            Err(OscError::MissingCommand) => ctx.host.println(&format!("Missing command: {address}")),
            Err(OscError::Runtime(message)) => ctx.host.error(&format!("OSC command {address} failed: {message}")),
        }
    }

    /// Check the guards of a parsed address and execute its command.
    fn execute(&mut self, ctx: &mut Ctx, parsed: Parsed, value: Value) -> OscResult {
        for guard in &parsed.guards {
            if !check_guard(ctx, guard, value)? {
                return Ok(());
            }
        }
        let m = &mut self.modules;
        let shared = &mut self.shared;
        match parsed.result? {
            OscIn::Setup(SetupIn::Refresh {}) => {
                self.flush_modules(ctx, true);
                Ok(())
            }
            OscIn::Transport(command) => m.transport.execute(ctx, command, value),
            OscIn::Global(command) => m.global.execute(ctx, command),
            OscIn::Layout(command) => m.layout.execute(ctx, command, value),
            OscIn::Marker(command) => m.marker.execute(ctx, command),
            OscIn::Project(command) => m.project.execute(ctx, command, value),
            OscIn::Param(command) => super::module::project::execute_param(ctx, command, value),
            OscIn::Track(command) => m.track.execute(ctx, command, value),
            OscIn::Scene(command) => m.scene.execute(ctx, command, value),
            OscIn::Device(command) => m.device.execute(ctx, command, value),
            OscIn::Browser(command) => m.browser.execute(ctx, command, value),
            OscIn::Midi(command) => m.midi.execute(ctx, shared, command, value),
            OscIn::Action(command) => m.action.execute(ctx, command),
            OscIn::Clip(command) => m.clip.execute(ctx, command, value),
        }
    }

    /// Flush all modules and send the changed values (former `OSCWriter.flush (dump)`).
    pub fn flush_modules(&mut self, ctx: &mut Ctx, dump: bool) {
        // Former isConnected
        if self.client.is_none() {
            return;
        }
        let messages = self.collect_updates(ctx, dump);
        if messages.is_empty() {
            return;
        }
        if config::should_log_output_commands(&ctx.config) {
            if let Some(text) = log_text(&messages, is_heartbeat_message, config::filter_heartbeat_messages(&ctx.config)) {
                ctx.host.println(&text);
            }
        }
        if let Some(client) = &mut self.client {
            let result = client.send_bundle(messages);
            self.handle_send_result(ctx, result);
        }
    }

    /// Flush all modules: the messages of the changed values (all values if `dump` is set),
    /// framed by `/update` (former `OSCWriter.flush (dump)` without sending them).
    pub fn collect_updates(&mut self, ctx: &Ctx, dump: bool) -> Vec<OscMessage> {
        let m = &mut self.modules;
        let w = &mut self.shared;
        m.transport.flush(ctx, w, dump);
        m.layout.flush(ctx, w, dump);
        m.marker.flush(ctx, w, dump);
        m.project.flush(ctx, w, dump);
        m.track.flush(ctx, w, dump);
        m.scene.flush(ctx, w, dump);
        m.device.flush(ctx, w, dump);
        m.browser.flush(ctx, w, dump);
        m.midi.flush(ctx, w, dump);
        m.clip.flush(ctx, w, dump);
        self.shared.writer.take_messages(Some(UPDATE_ADDRESS))
    }

    fn handle_send_result(&mut self, ctx: &Ctx, result: std::io::Result<()>) {
        match result {
            Ok(()) => self.last_send_error = None,
            Err(error) => {
                let text = error.to_string();
                if self.last_send_error.as_deref() != Some(text.as_str()) {
                    ctx.host.error(&format!("Could not send UDP message. {text}"));
                    self.last_send_error = Some(text);
                }
            }
        }
    }

    /// Read and execute the received messages.
    pub fn receive(&mut self, ctx: &mut Ctx) {
        let (messages, errors) = self.server.receive();
        for error in errors {
            ctx.host.println(&format!("Received an invalid OSC packet from {}: {}", error.source, error.error));
        }
        for message in &messages {
            self.handle_message(ctx, message);
        }
    }

    fn handle_setting_change(&mut self, ctx: &mut Ctx, id: SettingId) {
        if setup_helpers::handle_deactivated_items_setting(ctx, id) {
            return;
        }
        match id {
            config::RECEIVE_PORT => self.start_osc_server(ctx),
            config::VALUE_RESOLUTION => {
                // The model keeps copies of the value changer: restart with the new resolution
                let resolution = config::value_resolution(&ctx.config);
                if self.resolution.is_some_and(|r| r != resolution) {
                    ctx.restart_requested = true;
                }
            }
            _ => {}
        }
    }
}

impl OscSetup {
    /// The part of `init` without the network (also used by the tests).
    pub fn init_model(&mut self, ctx: &mut Ctx) {
        self.resolution = Some(config::value_resolution(&ctx.config));

        // The cursor clip of the parser and the modules (getNoteClip (8, 128))
        ctx.model.note_clip(8, 128);

        // OSCParser constructor
        ctx.model.current_track_bank_mut().set_indication(true);
        // MidiModule constructor (also sets the key translation table, as the parser)
        MidiModule::update_note_matrix(ctx, &mut self.shared);
    }

    /// The changed settings (former setting observers), public for the tests.
    pub fn handle_settings(&mut self, ctx: &mut Ctx) {
        for id in ctx.config.take_changed_settings() {
            self.handle_setting_change(ctx, id);
        }
    }
}

impl ControllerSetup for OscSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        self.init_model(ctx);

        // Send OSC messages (createSurface)
        let send_host = config::send_host(&ctx.config);
        let send_port = config::send_port(&ctx.config);
        ctx.host.println(&format!("Connecting to OSC server {send_host}:{send_port}"));
        match OscClient::connect(&send_host, send_port) {
            Ok(client) => self.client = Some(client),
            Err(error) => ctx.host.error(&format!("Could not connect to OSC server {send_host}:{send_port}: {error}")),
        }

        // createObservers: the receive port observer starts the server
        self.start_osc_server(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        // Initial flush of the whole DAW state
        self.dump_due = Some(ctx.host.now() + Duration::from_millis(INITIAL_DUMP_DELAY_MS));
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        // The received commands
        self.receive(ctx);

        // Model events (former selection and note observers of the track bank)
        for event in ctx.model.take_events() {
            match event {
                ModelEvent::TrackSelectionChanged { .. } => self.shared.key_manager.clear_pressed_keys(),
                ModelEvent::TrackNote { track_index, note, velocity } => {
                    let selected = ctx.model.current_track_bank().selected_index().map(|i| i as i32);
                    self.shared.key_manager.call(track_index, note, velocity, selected);
                }
                _ => {}
            }
        }

        self.handle_settings(ctx);
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        // The bundles of a large update which wait
        if let Some(client) = &mut self.client {
            let result = client.send_pending();
            self.handle_send_result(ctx, result);
        }
        let dump = self.dump_due.is_some_and(|due| due <= ctx.host.now());
        if dump {
            self.dump_due = None;
        }
        self.flush_modules(ctx, dump);
    }

    fn exit(&mut self, _ctx: &mut Ctx) {
        self.server.stop();
        self.client = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_setup_follows_the_page_size() {
        let ms = model_setup(4, false);
        assert_eq!((ms.num_tracks, ms.num_scenes, ms.num_sends, ms.num_param_pages, ms.num_markers), (4, 4, 4, 4, 4));
        assert_eq!((ms.num_devices_in_bank, ms.num_device_layers, ms.num_params), (4, 4, 4));
        assert!(!ms.has_flat_track_list);
        assert!(ms.wants_focused_parameter);
        assert_eq!(ms.device_ids, vec![DeviceId::FirstInstrument, DeviceId::Eq]);
    }
}
