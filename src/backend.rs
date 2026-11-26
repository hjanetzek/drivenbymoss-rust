// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Interface to communicate with the C++ extension (former `BackendExchange` + JNI natives).

use std::cell::RefCell;
use std::rc::Rc;

use crate::protocol::{Cmd, CmdBuf};

/// The processors in the C++ extension which handle commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Processor {
    Transport,
    Project,
    Track,
    Master,
    Device,
    Eq,
    Browser,
    Marker,
    Clip,
    Session,
    Scene,
    NoteRepeat,
    PlayingNotes,
    Groove,
    Play,
    Stop,
    Record,
    Repeat,
    Time,
    Tempo,
    Automation,
    Action,
    Quantize,
    MetroVol,
    Undo,
    Redo,
    Cursor,
    Refresh,
    IniFile,
    Application,
    Arranger,
    Mixer,
    Beats,
    Bank,
    Drum,
    Primary,
    /// The clip launcher navigator of the host (see `HostFeature::ClipLauncherNavigator`).
    Navigator,
    /// The effect tracks (Bitwig's effect track bank; commands as for `track`).
    FxTrack,
    /// The parameter pages of the host (doc/HOST_PAGE_PLAN.md).
    HostPage,
    /// The controller itself: what the user does on the surface (`controller view`).
    Controller,
    /// The host's snapshots (`snapshot <index>/load|store|remove`).
    Snapshot,
}

impl Processor {
    /// The name of the processor as expected by the C++ side (the lower case Java enum name).
    pub fn id(self) -> &'static str {
        match self {
            Processor::Transport => "transport",
            Processor::Project => "project",
            Processor::Track => "track",
            Processor::Master => "master",
            Processor::Device => "device",
            Processor::Eq => "eq",
            Processor::Browser => "browser",
            Processor::Marker => "marker",
            Processor::Clip => "clip",
            Processor::Session => "session",
            Processor::Scene => "scene",
            Processor::NoteRepeat => "noterepeat",
            Processor::PlayingNotes => "playingnotes",
            Processor::Groove => "groove",
            Processor::Play => "play",
            Processor::Stop => "stop",
            Processor::Record => "record",
            Processor::Repeat => "repeat",
            Processor::Time => "time",
            Processor::Tempo => "tempo",
            Processor::Automation => "automation",
            Processor::Action => "action",
            Processor::Quantize => "quantize",
            Processor::MetroVol => "metro_vol",
            Processor::Undo => "undo",
            Processor::Redo => "redo",
            Processor::Cursor => "cursor",
            Processor::Refresh => "refresh",
            Processor::IniFile => "inifile",
            Processor::Application => "application",
            Processor::Arranger => "arranger",
            Processor::Mixer => "mixer",
            Processor::Beats => "beats",
            Processor::Bank => "bank",
            Processor::Drum => "drum",
            Processor::Primary => "primary",
            Processor::Navigator => "navigator",
            Processor::FxTrack => "fxtrack",
            Processor::HostPage => "hostpage",
            Processor::Controller => "controller",
            Processor::Snapshot => "snapshot",
        }
    }

    /// All processors. The position is the number of the processor in the binary protocol
    /// (`protocol::bin`): append only.
    pub const ALL: [Processor; 41] = [
        Processor::Transport,
        Processor::Project,
        Processor::Track,
        Processor::Master,
        Processor::Device,
        Processor::Eq,
        Processor::Browser,
        Processor::Marker,
        Processor::Clip,
        Processor::Session,
        Processor::Scene,
        Processor::NoteRepeat,
        Processor::PlayingNotes,
        Processor::Groove,
        Processor::Play,
        Processor::Stop,
        Processor::Record,
        Processor::Repeat,
        Processor::Time,
        Processor::Tempo,
        Processor::Automation,
        Processor::Action,
        Processor::Quantize,
        Processor::MetroVol,
        Processor::Undo,
        Processor::Redo,
        Processor::Cursor,
        Processor::Refresh,
        Processor::IniFile,
        Processor::Application,
        Processor::Arranger,
        Processor::Mixer,
        Processor::Beats,
        Processor::Bank,
        Processor::Drum,
        Processor::Primary,
        Processor::Navigator,
        Processor::FxTrack,
        Processor::HostPage,
        Processor::Controller,
        Processor::Snapshot,
    ];

    /// The processor with the given name (see [`Processor::id`]).
    pub fn from_id(id: &str) -> Option<Processor> {
        Processor::ALL.iter().copied().find(|p| p.id() == id)
    }
}

/// Functions provided by the C++ extension.
pub trait Backend {
    /// A command of the core. The text hosts receive its legacy form ([`Cmd::legacy`]), see
    /// [`FfiBackend`](crate::ffi::FfiBackend).
    fn send_command(&self, cmd: &Cmd);
    fn process_midi_arg(&self, device_id: i32, status: i32, data1: i32, data2: i32);
    fn midi_inputs(&self) -> Vec<(i32, String)>;
    fn midi_outputs(&self) -> Vec<(i32, String)>;
    fn open_midi_input(&self, device_id: i32) -> bool;
    fn open_midi_output(&self, device_id: i32) -> bool;
    fn close_midi_input(&self, device_id: i32);
    fn close_midi_output(&self, device_id: i32);
    fn send_midi_data(&self, device_id: i32, data: &[u8]);
    /// Write to a USB bulk OUT endpoint of a USB device of the controller instance (former
    /// `IHost.getUsbDevice (deviceIndex).getEndpoint (0, ...).send (data)`), e.g. a frame of the
    /// Push 2/3 display. `device_index` is the index of the device in
    /// `ControllerFactory::usb_devices`, `endpoint` the endpoint address (e.g. 0x01). Returns
    /// false if the host has no USB transport or the write failed. The default does nothing.
    fn usb_bulk_out(&self, _device_index: i32, _endpoint: u8, _data: &[u8]) -> bool {
        false
    }
    /// [`Backend::usb_bulk_out`] with the instance and the matcher of the device resolved (by
    /// [`UsbInstanceBackend`], which the application puts in front of the backend of an instance
    /// with USB devices). The C interface passes all of it to the host (`dbm_set_usb_callback`),
    /// so that a host needs no lookup. The default calls [`Backend::usb_bulk_out`].
    fn usb_bulk_out_to(&self, target: &UsbTarget, data: &[u8]) -> bool {
        self.usb_bulk_out(target.device_index, target.endpoint, data)
    }
    /// True if the host takes the simulator data ([`Backend::simulator_data_out`]): only a host
    /// which writes the USB data of the controller to a simulator (the Push simulator's socket)
    /// and registered a receiver (`dbm_set_simulator_callback`). The controllers build the data
    /// only then. The default: false.
    fn simulator_data_enabled(&self) -> bool {
        false
    }
    /// A message of the simulator side channel, e.g. the encoder state of the Push 2/3
    /// (`DBMV` message, [`crate::controller::push::sim_values`]): the exact bytes to write to
    /// the simulator's stream, between two USB writes. Never for a real device. Returns false
    /// if the host does not take it. The default calls [`Backend::simulator_data_out_to`] with
    /// instance -1.
    fn simulator_data_out(&self, data: &[u8]) -> bool {
        self.simulator_data_out_to(-1, data)
    }
    /// [`Backend::simulator_data_out`] with the index of the controller instance resolved (by
    /// [`UsbInstanceBackend`]). The default does nothing.
    fn simulator_data_out_to(&self, _instance: i32, _data: &[u8]) -> bool {
        false
    }
    fn set_note_input_filters(&self, device_id: i32, note_input_index: i32, filters: &[String]);
    fn set_note_input_key_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]);
    fn set_note_input_velocity_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]);

    /// A log message of the library. The default ignores it.
    fn log(&self, _message: &str) {}

    /// The command features of a host on the binary command form, derived from the commands it
    /// handles ([`command_features`](crate::daw::context::command_features), bit set of
    /// [`HostFeature`](crate::daw::context::HostFeature)); None for a text host, whose features
    /// come from `/host/features`. The default: None.
    fn command_features(&self) -> Option<u32> {
        None
    }

    /// True if the host handles the command: for a host on the binary command form from its
    /// handled-commands bitset ([`is_handled`](crate::daw::context::is_handled)); None for a
    /// text host. The default: None.
    fn handles_command(&self, _id: crate::protocol::CmdId) -> Option<bool> {
        None
    }

    /// True if the host shows the log itself ([`Backend::log`]); otherwise the library writes it
    /// to stderr.
    fn handles_log(&self) -> bool {
        false
    }
}

/// Convenience wrapper around a backend which is shared by all parts of the model.
#[derive(Clone)]
pub struct Sender {
    backend: Rc<dyn Backend>,
}

impl Sender {
    pub fn new(backend: Rc<dyn Backend>) -> Self {
        Sender { backend }
    }

    pub fn backend(&self) -> &Rc<dyn Backend> {
        &self.backend
    }

    /// Send a command to the backend.
    pub fn send(&self, cmd: &Cmd) {
        self.backend.send_command(cmd);
    }

    /// Send a refresh command to the backend, which then sends all data again.
    pub fn refresh(&self) {
        self.send(&Cmd::Refresh {});
    }

    /// Write to a USB bulk OUT endpoint, see [`Backend::usb_bulk_out`].
    pub fn usb_bulk_out(&self, device_index: i32, endpoint: u8, data: &[u8]) -> bool {
        self.backend.usb_bulk_out(device_index, endpoint, data)
    }

    /// True if the host takes simulator data, see [`Backend::simulator_data_enabled`].
    pub fn simulator_data_enabled(&self) -> bool {
        self.backend.simulator_data_enabled()
    }

    /// Send a message of the simulator side channel, see [`Backend::simulator_data_out`].
    pub fn simulator_data_out(&self, data: &[u8]) -> bool {
        self.backend.simulator_data_out(data)
    }
}

/// The destination of a USB bulk write ([`Backend::usb_bulk_out_to`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbTarget {
    /// The index of the controller instance in the application (0 for an application with one
    /// controller, `dbm_create_controller`).
    pub instance: i32,
    /// The index of the device in `ControllerFactory::usb_devices`.
    pub device_index: i32,
    pub vendor_id: u16,
    pub product_id: u16,
    /// The interface number of the device's matcher.
    pub interface: u8,
    /// The endpoint address of the write.
    pub endpoint: u8,
}

/// The backend of a controller instance with USB devices: forwards everything to the backend of
/// the application and resolves the USB writes of the instance ([`Backend::usb_bulk_out`]) to
/// their instance and device ([`Backend::usb_bulk_out_to`]).
pub struct UsbInstanceBackend {
    inner: Rc<dyn Backend>,
    instance: i32,
    devices: Vec<crate::controller::UsbMatcher>,
}

impl UsbInstanceBackend {
    pub fn new(inner: Rc<dyn Backend>, instance: i32, devices: Vec<crate::controller::UsbMatcher>) -> Self {
        UsbInstanceBackend { inner, instance, devices }
    }
}

impl Backend for UsbInstanceBackend {
    fn send_command(&self, cmd: &Cmd) {
        self.inner.send_command(cmd);
    }
    fn process_midi_arg(&self, device_id: i32, status: i32, data1: i32, data2: i32) {
        self.inner.process_midi_arg(device_id, status, data1, data2);
    }
    fn midi_inputs(&self) -> Vec<(i32, String)> {
        self.inner.midi_inputs()
    }
    fn midi_outputs(&self) -> Vec<(i32, String)> {
        self.inner.midi_outputs()
    }
    fn open_midi_input(&self, device_id: i32) -> bool {
        self.inner.open_midi_input(device_id)
    }
    fn open_midi_output(&self, device_id: i32) -> bool {
        self.inner.open_midi_output(device_id)
    }
    fn close_midi_input(&self, device_id: i32) {
        self.inner.close_midi_input(device_id);
    }
    fn close_midi_output(&self, device_id: i32) {
        self.inner.close_midi_output(device_id);
    }
    fn send_midi_data(&self, device_id: i32, data: &[u8]) {
        self.inner.send_midi_data(device_id, data);
    }
    /// Resolves the device; an index without a device is not written (false).
    fn usb_bulk_out(&self, device_index: i32, endpoint: u8, data: &[u8]) -> bool {
        let Some(device) = usize::try_from(device_index).ok().and_then(|i| self.devices.get(i)) else {
            return false;
        };
        let target = UsbTarget { instance: self.instance, device_index, vendor_id: device.vendor_id, product_id: device.product_id, interface: device.interface, endpoint };
        self.inner.usb_bulk_out_to(&target, data)
    }
    fn usb_bulk_out_to(&self, target: &UsbTarget, data: &[u8]) -> bool {
        self.inner.usb_bulk_out_to(target, data)
    }
    fn simulator_data_enabled(&self) -> bool {
        self.inner.simulator_data_enabled()
    }
    /// Adds the instance.
    fn simulator_data_out(&self, data: &[u8]) -> bool {
        self.inner.simulator_data_out_to(self.instance, data)
    }
    fn simulator_data_out_to(&self, instance: i32, data: &[u8]) -> bool {
        self.inner.simulator_data_out_to(instance, data)
    }
    fn set_note_input_filters(&self, device_id: i32, note_input_index: i32, filters: &[String]) {
        self.inner.set_note_input_filters(device_id, note_input_index, filters);
    }
    fn set_note_input_key_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        self.inner.set_note_input_key_translation_table(device_id, note_input_index, table);
    }
    fn set_note_input_velocity_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        self.inner.set_note_input_velocity_translation_table(device_id, note_input_index, table);
    }
    fn log(&self, message: &str) {
        self.inner.log(message);
    }
    fn command_features(&self) -> Option<u32> {
        self.inner.command_features()
    }
    fn handles_command(&self, id: crate::protocol::CmdId) -> Option<bool> {
        self.inner.handles_command(id)
    }
    fn handles_log(&self) -> bool {
        self.inner.handles_log()
    }
}

/// The FNV-1a hash (64 bit) of the data of a USB write, as recorded by [`RecordingBackend`]
/// ([`BackendCall::UsbBulkOut`]).
pub fn usb_data_hash(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}

/// A call to the backend, recorded by the [`RecordingBackend`].
///
/// The commands of the core are recorded typed ([`BackendCall::Command`]); the tests match on
/// them with [`BackendCall::cmd`] or the helpers of [`RecordedCalls`], e.g.
/// `calls.has_cmd(|c| matches!(c, Cmd::ChannelMute { value: true, .. }))`. A call compares equal
/// to a [`Cmd`], so `assert_eq!(backend.take_calls(), [Cmd::Undo {}])` works.
#[derive(Debug, Clone, PartialEq)]
pub enum BackendCall {
    /// A command of the core ([`Backend::send_command`]).
    Command(CmdBuf),
    Midi(i32, i32, i32, i32),
    OpenInput(i32),
    OpenOutput(i32),
    CloseInput(i32),
    CloseOutput(i32),
    SendMidi(i32, Vec<u8>),
    NoteInputFilters(i32, i32, Vec<String>),
    KeyTranslation(i32, i32, Vec<i32>),
    VelocityTranslation(i32, i32, Vec<i32>),
    /// A USB bulk write ([`Backend::usb_bulk_out`]): the device index, the endpoint, the length
    /// and the hash of the data ([`usb_data_hash`]; a display frame is 327,696 bytes, too large
    /// to keep for every write). The data of the last write is in
    /// [`RecordingBackend::last_usb_data`].
    UsbBulkOut { device_index: i32, endpoint: u8, length: usize, hash: u64 },
    /// A message of the simulator side channel ([`Backend::simulator_data_out`]): the instance
    /// and the data (only with [`RecordingBackend::simulator_data`] set).
    SimulatorData(i32, Vec<u8>),
}

impl BackendCall {
    /// The command of the call, None for the other calls.
    pub fn cmd(&self) -> Option<Cmd<'_>> {
        match self {
            BackendCall::Command(cmd) => Some(cmd.cmd()),
            _ => None,
        }
    }
}

impl PartialEq<Cmd<'_>> for BackendCall {
    fn eq(&self, other: &Cmd<'_>) -> bool {
        self.cmd().as_ref() == Some(other)
    }
}

/// Helpers of the tests to find commands among recorded calls (implemented for slices of
/// [`BackendCall`] and [`CmdBuf`]).
pub trait RecordedCalls {
    /// The commands among the calls, in their order.
    fn commands(&self) -> impl Iterator<Item = Cmd<'_>>;

    /// True if a command matches.
    fn has_cmd(&self, f: impl Fn(&Cmd<'_>) -> bool) -> bool {
        self.commands().any(|c| f(&c))
    }

    /// The number of the matching commands.
    fn count_cmd(&self, f: impl Fn(&Cmd<'_>) -> bool) -> usize {
        self.commands().filter(|c| f(c)).count()
    }

    /// The first matching command.
    fn first_cmd(&self, f: impl Fn(&Cmd<'_>) -> bool) -> Option<Cmd<'_>> {
        self.commands().find(|c| f(c))
    }

    /// The last matching command.
    fn last_cmd(&self, f: impl Fn(&Cmd<'_>) -> bool) -> Option<Cmd<'_>> {
        self.commands().filter(|c| f(c)).last()
    }

    /// The legacy lines of the commands ([`crate::protocol::bin::command_line`]), e.g. to
    /// compare with a text dump.
    fn legacy_lines(&self) -> Vec<String> {
        self.commands().map(|c| crate::protocol::bin::command_line(&c)).collect()
    }
}

impl RecordedCalls for [BackendCall] {
    fn commands(&self) -> impl Iterator<Item = Cmd<'_>> {
        self.iter().filter_map(BackendCall::cmd)
    }
}

impl RecordedCalls for [CmdBuf] {
    fn commands(&self) -> impl Iterator<Item = Cmd<'_>> {
        self.iter().map(CmdBuf::cmd)
    }
}

/// A backend which records all calls. Used for tests.
#[derive(Default)]
pub struct RecordingBackend {
    pub calls: RefCell<Vec<BackendCall>>,
    pub inputs: Vec<(i32, String)>,
    pub outputs: Vec<(i32, String)>,
    /// The log messages of the library (see [`Backend::log`]).
    pub logs: RefCell<Vec<String>>,
    /// The handled commands of a binary host (word i bit j = ID 0x8000 + 64*i + j, empty: all;
    /// the `handled` event of a recording): the command features come from them
    /// ([`Backend::command_features`]). None: a text host.
    pub handled: RefCell<Option<Vec<u64>>>,
    /// The data of the last USB write ([`BackendCall::UsbBulkOut`]).
    pub last_usb_data: RefCell<Vec<u8>>,
    /// True: the backend takes simulator data ([`Backend::simulator_data_enabled`], recorded as
    /// [`BackendCall::SimulatorData`]).
    pub simulator_data: bool,
}

impl RecordingBackend {
    pub fn take_calls(&self) -> Vec<BackendCall> {
        std::mem::take(&mut *self.calls.borrow_mut())
    }

    /// Take the calls and return their commands (the other calls are dropped).
    pub fn take_commands(&self) -> Vec<CmdBuf> {
        let calls = self.take_calls();
        calls.into_iter().filter_map(|call| match call {
            BackendCall::Command(cmd) => Some(cmd),
            _ => None,
        }).collect()
    }

    /// The commands recorded so far (not taken).
    pub fn commands(&self) -> Vec<CmdBuf> {
        self.calls.borrow().iter().filter_map(|call| match call {
            BackendCall::Command(cmd) => Some(cmd.clone()),
            _ => None,
        }).collect()
    }

    fn push(&self, call: BackendCall) {
        self.calls.borrow_mut().push(call);
    }
}

impl Backend for RecordingBackend {
    fn log(&self, message: &str) {
        self.logs.borrow_mut().push(message.to_string());
    }

    fn command_features(&self) -> Option<u32> {
        self.handled.borrow().as_deref().map(crate::daw::context::command_features)
    }

    fn handles_command(&self, id: crate::protocol::CmdId) -> Option<bool> {
        self.handled.borrow().as_deref().map(|handled| crate::daw::context::is_handled(handled, id as u16))
    }

    /// Records the command in its owned form; this checks that it survives its binary form
    /// ([`CmdBuf::new`]).
    fn send_command(&self, cmd: &Cmd) {
        self.push(BackendCall::Command(CmdBuf::new(cmd)));
    }

    fn process_midi_arg(&self, device_id: i32, status: i32, data1: i32, data2: i32) {
        self.push(BackendCall::Midi(device_id, status, data1, data2));
    }

    fn midi_inputs(&self) -> Vec<(i32, String)> {
        self.inputs.clone()
    }

    fn midi_outputs(&self) -> Vec<(i32, String)> {
        self.outputs.clone()
    }

    fn open_midi_input(&self, device_id: i32) -> bool {
        self.push(BackendCall::OpenInput(device_id));
        true
    }

    fn open_midi_output(&self, device_id: i32) -> bool {
        self.push(BackendCall::OpenOutput(device_id));
        true
    }

    fn close_midi_input(&self, device_id: i32) {
        self.push(BackendCall::CloseInput(device_id));
    }

    fn close_midi_output(&self, device_id: i32) {
        self.push(BackendCall::CloseOutput(device_id));
    }

    fn send_midi_data(&self, device_id: i32, data: &[u8]) {
        self.push(BackendCall::SendMidi(device_id, data.to_vec()));
    }

    fn usb_bulk_out(&self, device_index: i32, endpoint: u8, data: &[u8]) -> bool {
        self.push(BackendCall::UsbBulkOut { device_index, endpoint, length: data.len(), hash: usb_data_hash(data) });
        *self.last_usb_data.borrow_mut() = data.to_vec();
        true
    }

    fn simulator_data_enabled(&self) -> bool {
        self.simulator_data
    }

    fn simulator_data_out_to(&self, instance: i32, data: &[u8]) -> bool {
        if !self.simulator_data {
            return false;
        }
        self.push(BackendCall::SimulatorData(instance, data.to_vec()));
        true
    }

    fn set_note_input_filters(&self, device_id: i32, note_input_index: i32, filters: &[String]) {
        self.push(BackendCall::NoteInputFilters(device_id, note_input_index, filters.to_vec()));
    }

    fn set_note_input_key_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        self.push(BackendCall::KeyTranslation(device_id, note_input_index, table.to_vec()));
    }

    fn set_note_input_velocity_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        self.push(BackendCall::VelocityTranslation(device_id, note_input_index, table.to_vec()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::UsbMatcher;

    #[test]
    fn usb_writes_of_an_instance_are_recorded_with_their_device() {
        let recording = Rc::new(RecordingBackend::default());
        let push3 = UsbMatcher { vendor_id: 0x2982, product_id: 0x1969, interface: 0, endpoint: 0x01 };
        let instance = UsbInstanceBackend::new(recording.clone(), 0, vec![push3]);
        let frame = vec![0xFF, 0xCC, 0xAA, 0x88, 0, 0];
        assert!(Sender::new(Rc::new(instance)).usb_bulk_out(0, 0x01, &frame));
        assert_eq!(recording.take_calls(), [BackendCall::UsbBulkOut { device_index: 0, endpoint: 0x01, length: 6, hash: usb_data_hash(&frame) }]);
        assert_eq!(*recording.last_usb_data.borrow(), frame);
        assert_ne!(usb_data_hash(&frame), usb_data_hash(&frame[..5]));
    }

    #[test]
    fn simulator_data_only_when_enabled_with_the_instance() {
        let recording = Rc::new(RecordingBackend::default());
        let instance = Sender::new(Rc::new(UsbInstanceBackend::new(recording.clone(), 3, Vec::new())));
        assert!(!instance.simulator_data_enabled());
        assert!(!instance.simulator_data_out(b"DBMV"));
        assert!(recording.take_calls().is_empty());

        let recording = Rc::new(RecordingBackend { simulator_data: true, ..Default::default() });
        let instance = Sender::new(Rc::new(UsbInstanceBackend::new(recording.clone(), 3, Vec::new())));
        assert!(instance.simulator_data_enabled());
        assert!(instance.simulator_data_out(b"DBMV"));
        assert!(recording.simulator_data_out(b"x"));
        assert_eq!(recording.take_calls(), [BackendCall::SimulatorData(3, b"DBMV".to_vec()), BackendCall::SimulatorData(-1, b"x".to_vec())]);
    }
}
