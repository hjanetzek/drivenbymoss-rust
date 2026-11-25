// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The in-process device transport of the NI Maschines on Linux: the core drives the Mk2 and the
//! JAM itself through `libdbm_nihost.so` (support/linux/ni_host), with the MIDI of dbm-nihost's
//! ports (template MIDI, the hello / gone / query sysex), but without the dbm-nihost daemon,
//! snd-virmidi or ALSA/PipeWire MIDI ports.
//!
//! - **Selection** ([`choose`]) at the start of a controller, only in a host which asked for it
//!   (`DBM_FLAG_DEVICE_TRANSPORT` of `dbm_create_controller`) and only on Linux:
//!   `DBM_MASCHINE_TRANSPORT=hid|midi|auto` (default auto). Auto drives the device in-process
//!   unless the library is missing, dbm-nihost runs (for the device) or the device's hidraw
//!   node is held by another process or not accessible; then the host's MIDI ports are used as
//!   before. A device which is not connected yet is waited for (the library looks for it once a
//!   second).
//! - **Threading:** no thread of the core. The library has its own (it reads the device into a
//!   queue); the core reads the queue without waiting on its tick and on `dbm_poll_devices`,
//!   under the instance lock, and writes the changed LEDs and displays after each tick's flush.
//! - **Handshake:** the library sends the device's hello when it attaches and "gone" when it is
//!   unplugged. The hello goes to the controller as with dbm-nihost (the Mk2's Shift, the JAM's
//!   refresh); "gone" is taken here (logged, the sounding notes are ended). The library keeps
//!   the core's LED and display state, a re-attached device gets it at once; the core sends all
//!   states again as well (the Mk2's library resets its LEDs on attach).
//! - **Note inputs:** without a host MIDI port the host's note input gets nothing from the
//!   device, so the core applies the note inputs itself ([`NoteInputs`]: the filters and the key
//!   and velocity tables it sets for the host) and plays the matching messages into the host's
//!   default note input (`process_midi_arg`, Bitwig `NoteInput.sendRawMidiEvent`).

#[cfg(target_os = "linux")]
pub mod library;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::backend::{Backend, UsbTarget};
use crate::host::Host;
use crate::protocol::Cmd;

/// The kinds of the library (`DBM_NIHOST_MK2`, `DBM_NIHOST_JAM`).
pub const KIND_MK2: i32 = 1;
pub const KIND_JAM: i32 = 2;

/// The USB vendor ID of Native Instruments.
pub const VENDOR_NI: u16 = 0x17CC;

/// The environment variable which picks the transport: `hid`, `midi` or `auto` (the default).
pub const ENV_TRANSPORT: &str = "DBM_MASCHINE_TRANSPORT";

/// The Mk2's hello (dbm-nihost's answer to the query, also sent on attach).
pub const HELLO_MK2: [u8; 8] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0x01, 0xF7];
/// The JAM's hello: NI's "return from host".
pub const HELLO_JAM: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x46, 0x01, 0xF7];
/// The device is gone (unplugged).
pub const MESSAGE_GONE: [u8; 7] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x13, 0xF7];

/// A device which the core can drive in-process ([`crate::controller::ControllerFactory::hid_device`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HidDevice {
    /// The kind in the library ([`KIND_MK2`], [`KIND_JAM`]).
    pub kind: i32,
    pub vendor_id: u16,
    pub product_id: u16,
}

impl HidDevice {
    pub const MK2: HidDevice = HidDevice { kind: KIND_MK2, vendor_id: VENDOR_NI, product_id: 0x1140 };
    pub const JAM: HidDevice = HidDevice { kind: KIND_JAM, vendor_id: VENDOR_NI, product_id: 0x1500 };

    /// The hello of the device.
    pub fn hello(&self) -> &'static [u8] {
        if self.kind == KIND_MK2 {
            &HELLO_MK2
        } else {
            &HELLO_JAM
        }
    }
}

/// The transport asked for with [`ENV_TRANSPORT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// In-process if possible, else the MIDI ports.
    Auto,
    /// Always in-process (waits for the device, also while another process holds it).
    Hid,
    /// Always the host's MIDI ports.
    Midi,
}

impl TransportMode {
    /// The mode of a value of [`ENV_TRANSPORT`] (unset or empty: auto).
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("auto") => Ok(TransportMode::Auto),
            Some("hid") => Ok(TransportMode::Hid),
            Some("midi") => Ok(TransportMode::Midi),
            Some(other) => Err(format!("{ENV_TRANSPORT}={other} is not hid, midi or auto")),
        }
    }
}

/// The hidraw node of a device, as far as the selection needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    /// Not connected.
    Absent,
    /// Connected, its node (e.g. `/dev/hidraw7`) can be opened and locked.
    Free(String),
    /// Connected, but another process holds the node's lock (dbm-nihost, a zynthian driver,
    /// another DAW).
    Locked(String),
    /// Connected, but the node can't be opened (the error; the udev rule is missing).
    NoAccess(String, String),
}

/// The checks of the system for [`choose`] (a test stands in for them).
pub trait SystemProbe {
    /// Load the library: its path, or why it is not there.
    fn library(&self) -> Result<String, String>;
    /// True if the dbm-nihost daemon runs for the device.
    fn daemon_drives(&self, device: HidDevice) -> bool;
    fn device_state(&self, device: HidDevice) -> DeviceState;
}

/// The result of [`choose`], with the reason for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Hid(String),
    Midi(String),
}

/// Choose the transport of a device (see the module documentation).
pub fn choose(mode: TransportMode, device: HidDevice, probe: &dyn SystemProbe) -> Choice {
    if mode == TransportMode::Midi {
        return Choice::Midi(format!("{ENV_TRANSPORT}=midi"));
    }
    let library = match probe.library() {
        Ok(path) => path,
        Err(error) => return Choice::Midi(format!("libdbm_nihost {error}")),
    };
    if mode == TransportMode::Hid {
        return Choice::Hid(format!("{library}, {ENV_TRANSPORT}=hid"));
    }
    if probe.daemon_drives(device) {
        return Choice::Midi("dbm-nihost runs and drives the device (stop it to drive the device in-process)".to_string());
    }
    match probe.device_state(device) {
        DeviceState::Free(node) => Choice::Hid(format!("{library}, {node}")),
        DeviceState::Absent => Choice::Hid(format!("{library}, the device is not connected yet")),
        DeviceState::Locked(node) => Choice::Midi(format!("{node} is in use by another process")),
        DeviceState::NoAccess(node, error) => Choice::Midi(format!("no access to {node} ({error}), see the udev rule 70-dbm-nihost.rules")),
    }
}

/// The MIDI of an in-process device. The library is thread safe; the core calls it on the
/// thread of the call into the instance.
pub trait DeviceIo {
    /// The queued messages of the device (template MIDI, hello, gone), without waiting.
    fn read(&self, out: &mut Vec<Vec<u8>>);
    /// One message of the core (a short message or a complete sysex).
    fn send(&self, data: &[u8]);
    /// Write the changed LEDs and displays; the number of reports, -1 if a write failed.
    fn flush(&self) -> i32;
}

/// The result of [`DeviceProvider::open`].
pub enum Opened {
    /// Drive the device in-process (the device, a description for the log).
    Hid(Rc<dyn DeviceIo>, String),
    /// Use the host's MIDI ports (the reason for the log).
    Midi(String),
    /// Not on this system (not Linux): the MIDI ports, nothing to log.
    NotApplicable,
}

/// Opens the in-process device of a controller (the system's: [`SystemProvider`]; tests have
/// their own).
pub trait DeviceProvider {
    fn open(&self, device: HidDevice) -> Opened;
}

/// The provider of the real system: [`choose`] with the environment and the system's checks,
/// then the library's device.
pub struct SystemProvider;

impl DeviceProvider for SystemProvider {
    #[cfg(target_os = "linux")]
    fn open(&self, device: HidDevice) -> Opened {
        let value = std::env::var(ENV_TRANSPORT).ok();
        let (mode, warning) = match TransportMode::parse(value.as_deref()) {
            Ok(mode) => (mode, None),
            Err(error) => (TransportMode::Auto, Some(error)),
        };
        let warning = warning.map(|w| format!(" ({w}, auto)")).unwrap_or_default();
        match choose(mode, device, &library::LinuxProbe) {
            Choice::Midi(reason) => Opened::Midi(reason + &warning),
            Choice::Hid(description) => match library::load().and_then(|lib| library::NihostDevice::open(lib, device)) {
                Ok(opened) => Opened::Hid(Rc::new(opened), description + &warning),
                Err(error) => Opened::Midi(format!("libdbm_nihost: {error}")),
            },
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn open(&self, _device: HidDevice) -> Opened {
        Opened::NotApplicable
    }
}

/// One note input of the controller as the host gets it (`set_note_input_*`).
#[derive(Debug, Default, Clone)]
struct NoteInputState {
    /// Hex prefixes of the messages (e.g. `90`, `B040`).
    filters: Vec<String>,
    /// The key table (`table[note]`, -1 blocks), empty: unchanged.
    keys: Vec<i32>,
    /// The velocity table, empty: unchanged.
    velocities: Vec<i32>,
}

/// The note inputs of a controller whose device is driven in-process: the core applies the
/// filters and the key and velocity tables, as the host does with the MIDI of its port.
#[derive(Debug, Default)]
pub struct NoteInputs {
    inputs: BTreeMap<i32, NoteInputState>,
    /// The sounding notes, (channel, key of the device) -> key in the DAW: the note-off (and
    /// aftertouch) goes to the key of the note-on, also if the table changed meanwhile.
    sounding: HashMap<(u8, u8), u8>,
}

impl NoteInputs {
    pub fn set_filters(&mut self, index: i32, filters: &[String]) {
        if filters.is_empty() {
            self.inputs.remove(&index);
        } else {
            self.inputs.entry(index).or_default().filters = filters.iter().map(|f| f.to_ascii_uppercase()).collect();
        }
    }

    pub fn set_keys(&mut self, index: i32, table: &[i32]) {
        if let Some(input) = self.inputs.get_mut(&index) {
            input.keys = table.to_vec();
        }
    }

    pub fn set_velocities(&mut self, index: i32, table: &[i32]) {
        if let Some(input) = self.inputs.get_mut(&index) {
            input.velocities = table.to_vec();
        }
    }

    /// The message for the DAW's note input if a note input takes it (translated), else None.
    pub fn translate(&mut self, data: &[u8]) -> Option<[u8; 3]> {
        let &[status, data1, data2] = data else {
            return None;
        };
        if !(0x80..0xF0).contains(&status) {
            return None;
        }
        let hex = crate::midi::to_hex_str(data);
        let input = self.inputs.values().find(|input| input.filters.iter().any(|f| hex.starts_with(f.as_str())))?;
        let channel = status & 0x0F;
        let lookup = |table: &[i32], value: u8| -> Option<u8> {
            match table.get(usize::from(value)) {
                None => Some(value),
                Some(&v) if (0..128).contains(&v) => Some(v as u8),
                Some(_) => None,
            }
        };
        match status & 0xF0 {
            0x90 if data2 > 0 => {
                let Some(key) = lookup(&input.keys, data1) else {
                    self.sounding.remove(&(channel, data1));
                    return None;
                };
                let velocity = lookup(&input.velocities, data2).unwrap_or(data2);
                self.sounding.insert((channel, data1), key);
                Some([status, key, velocity])
            }
            0x80 | 0x90 => {
                let key = match self.sounding.remove(&(channel, data1)) {
                    Some(key) => key,
                    None => lookup(&input.keys, data1)?,
                };
                Some([status, key, data2])
            }
            0xA0 => {
                let key = match self.sounding.get(&(channel, data1)) {
                    Some(&key) => key,
                    None => lookup(&input.keys, data1)?,
                };
                Some([status, key, data2])
            }
            _ => Some([status, data1, data2]),
        }
    }

    /// The note-offs of all sounding notes (the device is gone, the controller stops).
    pub fn release_all(&mut self) -> Vec<[u8; 3]> {
        let mut offs: Vec<[u8; 3]> = self.sounding.drain().map(|((channel, _), key)| [0x80 | channel, key, 0]).collect();
        offs.sort();
        offs
    }
}

/// The backend of a controller instance whose device is driven in-process: the MIDI to the
/// device ID of the device goes to the library, the note inputs are recorded for
/// [`NoteInputs`] (and passed on: the host creates them, Bitwig only during its init),
/// everything else goes to the backend of the application.
pub struct DeviceBackend {
    inner: Rc<dyn Backend>,
    device_id: i32,
    io: Rc<dyn DeviceIo>,
    notes: Rc<RefCell<NoteInputs>>,
}

impl DeviceBackend {
    pub fn new(inner: Rc<dyn Backend>, device_id: i32, io: Rc<dyn DeviceIo>, notes: Rc<RefCell<NoteInputs>>) -> Self {
        DeviceBackend { inner, device_id, io, notes }
    }
}

impl Backend for DeviceBackend {
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
    /// The device's ports are always there.
    fn open_midi_input(&self, device_id: i32) -> bool {
        device_id == self.device_id || self.inner.open_midi_input(device_id)
    }
    fn open_midi_output(&self, device_id: i32) -> bool {
        device_id == self.device_id || self.inner.open_midi_output(device_id)
    }
    fn close_midi_input(&self, device_id: i32) {
        if device_id != self.device_id {
            self.inner.close_midi_input(device_id);
        }
    }
    fn close_midi_output(&self, device_id: i32) {
        if device_id != self.device_id {
            self.inner.close_midi_output(device_id);
        }
    }
    fn send_midi_data(&self, device_id: i32, data: &[u8]) {
        if device_id == self.device_id {
            self.io.send(data);
        } else {
            self.inner.send_midi_data(device_id, data);
        }
    }
    fn usb_bulk_out(&self, device_index: i32, endpoint: u8, data: &[u8]) -> bool {
        self.inner.usb_bulk_out(device_index, endpoint, data)
    }
    fn usb_bulk_out_to(&self, target: &UsbTarget, data: &[u8]) -> bool {
        self.inner.usb_bulk_out_to(target, data)
    }
    fn simulator_data_enabled(&self) -> bool {
        self.inner.simulator_data_enabled()
    }
    fn simulator_data_out(&self, data: &[u8]) -> bool {
        self.inner.simulator_data_out(data)
    }
    fn simulator_data_out_to(&self, instance: i32, data: &[u8]) -> bool {
        self.inner.simulator_data_out_to(instance, data)
    }
    fn set_note_input_filters(&self, device_id: i32, note_input_index: i32, filters: &[String]) {
        if device_id == self.device_id {
            self.notes.borrow_mut().set_filters(note_input_index, filters);
        }
        self.inner.set_note_input_filters(device_id, note_input_index, filters);
    }
    fn set_note_input_key_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        if device_id == self.device_id {
            self.notes.borrow_mut().set_keys(note_input_index, table);
        }
        self.inner.set_note_input_key_translation_table(device_id, note_input_index, table);
    }
    fn set_note_input_velocity_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        if device_id == self.device_id {
            self.notes.borrow_mut().set_velocities(note_input_index, table);
        }
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

/// What a read of the device brings for the controller ([`DeviceLink::take_input`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
    /// The device is there again after it was gone: send all states again.
    Reattached,
    /// A message for the controller (as from its MIDI input).
    Midi(Vec<u8>),
}

/// The in-process device of a running controller instance.
pub struct DeviceLink {
    pub device: HidDevice,
    /// The device ID of the device's MIDI in the controller (0: the first port pair).
    pub device_id: i32,
    io: Rc<dyn DeviceIo>,
    notes: Rc<RefCell<NoteInputs>>,
    /// The host, for the notes of the note inputs.
    host_backend: Rc<dyn Backend>,
    /// A hello came, and no "gone" after it.
    attached: bool,
    /// The device was gone (its next hello is a re-attach).
    was_gone: bool,
    buffer: Vec<Vec<u8>>,
}

impl DeviceLink {
    /// The link and the backend of the instance (wrapping `inner`, which also gets the notes of
    /// the note inputs).
    pub fn new(device: HidDevice, device_id: i32, io: Rc<dyn DeviceIo>, inner: Rc<dyn Backend>) -> (Self, Rc<dyn Backend>) {
        let notes = Rc::new(RefCell::new(NoteInputs::default()));
        let backend: Rc<dyn Backend> = Rc::new(DeviceBackend::new(inner.clone(), device_id, io.clone(), notes.clone()));
        let link = DeviceLink { device, device_id, io, notes, host_backend: inner, attached: false, was_gone: false, buffer: Vec::new() };
        (link, backend)
    }

    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// Read the device: the hello and "gone" set its state (logged with the controller's name),
    /// the notes of the note inputs go to the host at once, the messages for the controller are
    /// returned.
    pub fn take_input(&mut self, host: &Host, name: &str) -> Vec<LinkEvent> {
        self.buffer.clear();
        self.io.read(&mut self.buffer);
        let mut events = Vec::with_capacity(self.buffer.len());
        for data in std::mem::take(&mut self.buffer) {
            if data == MESSAGE_GONE {
                if self.attached {
                    host.println(&format!("{name}: the device is gone (unplugged)"));
                }
                self.attached = false;
                self.was_gone = true;
                self.release_notes();
                continue;
            }
            if data == self.device.hello() {
                if !self.attached {
                    host.println(&format!("{name}: the device is {}", if self.was_gone { "back" } else { "there" }));
                    if self.was_gone {
                        events.push(LinkEvent::Reattached);
                    }
                }
                self.attached = true;
                self.was_gone = false;
            } else if let Some([status, data1, data2]) = self.notes.borrow_mut().translate(&data) {
                self.host_backend.process_midi_arg(self.device_id, i32::from(status), i32::from(data1), i32::from(data2));
            }
            events.push(LinkEvent::Midi(data));
        }
        events
    }

    /// End the notes which still sound in the DAW.
    pub fn release_notes(&mut self) {
        for [status, key, velocity] in self.notes.borrow_mut().release_all() {
            self.host_backend.process_midi_arg(self.device_id, i32::from(status), i32::from(key), i32::from(velocity));
        }
    }

    /// Write the changed LEDs and displays.
    pub fn flush(&self) -> i32 {
        self.io.flush()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend};
    use std::cell::RefCell;

    /// A device for the tests: the queued input, the sent messages, the number of flushes.
    #[derive(Default)]
    pub struct FakeDevice {
        pub input: RefCell<Vec<Vec<u8>>>,
        pub sent: RefCell<Vec<Vec<u8>>>,
        pub flushes: std::cell::Cell<usize>,
    }

    impl DeviceIo for FakeDevice {
        fn read(&self, out: &mut Vec<Vec<u8>>) {
            out.append(&mut self.input.borrow_mut());
        }
        fn send(&self, data: &[u8]) {
            self.sent.borrow_mut().push(data.to_vec());
        }
        fn flush(&self) -> i32 {
            self.flushes.set(self.flushes.get() + 1);
            0
        }
    }

    struct Probe {
        library: Result<String, String>,
        daemon: bool,
        state: DeviceState,
    }

    impl SystemProbe for Probe {
        fn library(&self) -> Result<String, String> {
            self.library.clone()
        }
        fn daemon_drives(&self, _device: HidDevice) -> bool {
            self.daemon
        }
        fn device_state(&self, _device: HidDevice) -> DeviceState {
            self.state.clone()
        }
    }

    fn probe(library: bool, daemon: bool, state: DeviceState) -> Probe {
        Probe { library: if library { Ok("/lib/libdbm_nihost.so".into()) } else { Err("not found (/lib/libdbm_nihost.so)".into()) }, daemon, state }
    }

    #[test]
    fn the_mode_from_the_environment() {
        assert_eq!(TransportMode::parse(None), Ok(TransportMode::Auto));
        assert_eq!(TransportMode::parse(Some("")), Ok(TransportMode::Auto));
        assert_eq!(TransportMode::parse(Some(" HID ")), Ok(TransportMode::Hid));
        assert_eq!(TransportMode::parse(Some("midi")), Ok(TransportMode::Midi));
        assert!(TransportMode::parse(Some("usb")).is_err());
    }

    #[test]
    fn selection() {
        let free = DeviceState::Free("/dev/hidraw7".into());
        let jam = HidDevice::JAM;
        let is_hid = |choice: Choice| matches!(choice, Choice::Hid(_));
        // Auto: in-process when the library is there and nobody else drives the device
        assert!(is_hid(choose(TransportMode::Auto, jam, &probe(true, false, free.clone()))));
        assert!(is_hid(choose(TransportMode::Auto, jam, &probe(true, false, DeviceState::Absent))));
        // The fallbacks: no library, dbm-nihost, the device locked or not accessible
        assert!(matches!(choose(TransportMode::Auto, jam, &probe(false, false, free.clone())), Choice::Midi(r) if r.contains("not found")));
        assert!(matches!(choose(TransportMode::Auto, jam, &probe(true, true, free.clone())), Choice::Midi(r) if r.contains("dbm-nihost")));
        assert!(matches!(choose(TransportMode::Auto, jam, &probe(true, false, DeviceState::Locked("/dev/hidraw7".into()))), Choice::Midi(r) if r.contains("in use")));
        assert!(matches!(choose(TransportMode::Auto, jam, &probe(true, false, DeviceState::NoAccess("/dev/hidraw7".into(), "Permission denied".into()))), Choice::Midi(r) if r.contains("udev")));
        // Explicit: hid also with the daemon (the library waits for the lock), never without
        // the library; midi always
        assert!(is_hid(choose(TransportMode::Hid, jam, &probe(true, true, DeviceState::Locked("/dev/hidraw7".into())))));
        assert!(!is_hid(choose(TransportMode::Hid, jam, &probe(false, false, free.clone()))));
        assert!(!is_hid(choose(TransportMode::Midi, jam, &probe(true, false, free))));
    }

    #[test]
    fn note_inputs_filter_and_translate() {
        let mut notes = NoteInputs::default();
        assert_eq!(notes.translate(&[0x90, 36, 100]), None);
        let filters: Vec<String> = ["80", "90"].iter().map(|s| s.to_string()).collect();
        notes.set_filters(0, &filters);
        // Identity until the tables are set; CCs pass no filter
        assert_eq!(notes.translate(&[0x90, 36, 100]), Some([0x90, 36, 100]));
        assert_eq!(notes.translate(&[0x80, 36, 0]), Some([0x80, 36, 0]));
        assert_eq!(notes.translate(&[0xB0, 1, 2]), None);
        assert_eq!(notes.translate(&[0x91, 36, 100]), None);
        let mut keys = vec![-1; 128];
        keys[36] = 60;
        notes.set_keys(0, &keys);
        let mut velocities: Vec<i32> = (0..128).collect();
        velocities[100] = 127;
        notes.set_velocities(0, &velocities);
        assert_eq!(notes.translate(&[0x90, 36, 100]), Some([0x90, 60, 127]));
        assert_eq!(notes.translate(&[0x90, 37, 100]), None);
        // The note-off of a sounding note goes to its key, also after a table change
        keys[36] = 72;
        notes.set_keys(0, &keys);
        assert_eq!(notes.translate(&[0x90, 36, 0]), Some([0x90, 60, 0]));
        assert_eq!(notes.translate(&[0x80, 36, 0]), Some([0x80, 72, 0]));
        assert_eq!(notes.translate(&[0x90, 36, 64]), Some([0x90, 72, 64]));
        assert_eq!(notes.release_all(), [[0x80, 72, 0]]);
        assert!(notes.release_all().is_empty());
    }

    #[test]
    fn the_handshake_of_a_link() {
        let device = Rc::new(FakeDevice::default());
        let recording = Rc::new(RecordingBackend::default());
        let (mut link, backend) = DeviceLink::new(HidDevice::JAM, 0, device.clone(), recording.clone());
        let host = Host::new(Rc::new(crate::host::MemoryLog::default()));
        backend.set_note_input_filters(0, 0, &["90".to_string(), "80".to_string()]);
        recording.take_calls();
        // The core's MIDI goes to the device, not to the host's port
        backend.send_midi_data(0, &[0xB0, 1, 2]);
        assert_eq!(*device.sent.borrow(), [vec![0xB0, 1, 2]]);
        assert!(recording.take_calls().is_empty());
        // The hello on attach goes to the controller; a pad plays into the host's note input
        device.input.borrow_mut().extend([HELLO_JAM.to_vec(), vec![0x90, 36, 100]]);
        assert_eq!(link.take_input(&host, "JAM"), [LinkEvent::Midi(HELLO_JAM.to_vec()), LinkEvent::Midi(vec![0x90, 36, 100])]);
        assert!(link.is_attached());
        assert_eq!(recording.take_calls(), [BackendCall::Midi(0, 0x90, 36, 100)]);
        // Unplugged with the pad down: "gone" is taken, the note ends
        device.input.borrow_mut().push(MESSAGE_GONE.to_vec());
        assert!(link.take_input(&host, "JAM").is_empty());
        assert!(!link.is_attached());
        assert_eq!(recording.take_calls(), [BackendCall::Midi(0, 0x80, 36, 0)]);
        // Back: everything is sent again
        device.input.borrow_mut().push(HELLO_JAM.to_vec());
        assert_eq!(link.take_input(&host, "JAM"), [LinkEvent::Reattached, LinkEvent::Midi(HELLO_JAM.to_vec())]);
        // A second hello (an answer to the query) is no re-attach
        device.input.borrow_mut().push(HELLO_JAM.to_vec());
        assert_eq!(link.take_input(&host, "JAM"), [LinkEvent::Midi(HELLO_JAM.to_vec())]);
    }

    struct FakeProvider(Option<Rc<FakeDevice>>);

    impl DeviceProvider for FakeProvider {
        fn open(&self, _device: HidDevice) -> Opened {
            match &self.0 {
                Some(device) => Opened::Hid(device.clone(), "fake".to_string()),
                None => Opened::Midi("fake: no device".to_string()),
            }
        }
    }

    const JAM_UUID: &str = "72F32A83-A697-446B-9016-64AA3F9476E4";
    const MK2_UUID: &str = "015E4908-C049-4808-A645-A7A44BEFE36B";

    fn start(uuid: &str, provider: Option<FakeProvider>) -> (crate::app::App, Rc<RecordingBackend>) {
        let dir = std::env::temp_dir().join(format!("dbm-test-nihost-{uuid}-{}-{}", std::process::id(), provider.as_ref().map_or(0, |p| 1 + usize::from(p.0.is_some()))));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let backend = Rc::new(RecordingBackend::default());
        let mut app = crate::app::App::new_single(backend.clone(), dir.to_str().unwrap(), uuid, "Bitwig Studio 6.1").unwrap();
        if let Some(provider) = provider {
            app.enable_device_transport(Rc::new(provider));
        }
        app.set_time_ms(0);
        app.start_infrastructure();
        (app, backend)
    }

    #[test]
    fn a_controller_on_the_in_process_device() {
        let device = Rc::new(FakeDevice::default());
        let (mut app, backend) = start(JAM_UUID, Some(FakeProvider(Some(device.clone()))));
        assert!(app.drives_device(0));
        let calls = backend.take_calls();
        // No host ports, but the note input for the host
        assert!(!calls.iter().any(|c| matches!(c, BackendCall::OpenInput(_) | BackendCall::OpenOutput(_))), "{calls:?}");
        assert!(calls.iter().any(|c| matches!(c, BackendCall::NoteInputFilters(0, 0, f) if f.contains(&"90".to_string()))), "{calls:?}");
        app.update_model("track/count 1\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\nplay 0\n");
        // The startup: the LEDs go to the device, flushed after the tick
        app.set_time_ms(1100);
        app.tick();
        assert!(device.sent.borrow().len() > 20, "{}", device.sent.borrow().len());
        assert!(device.flushes.get() > 0);
        assert!(!backend.take_calls().iter().any(|c| matches!(c, BackendCall::SendMidi(..))));
        // The hello, then a pad: into the host's note input (the play view's key table) and to
        // the controller
        device.input.borrow_mut().extend([HELLO_JAM.to_vec(), vec![0x90, 36, 100]]);
        device.sent.borrow_mut().clear();
        assert_eq!(app.poll_devices(), 2);
        let calls = backend.take_calls();
        assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(0, 0x90, _, 100))), "{calls:?}");
        device.input.borrow_mut().push(vec![0x80, 36, 0]);
        app.set_time_ms(1200);
        app.tick();
        assert!(backend.take_calls().iter().any(|c| matches!(c, BackendCall::Midi(0, 0x80, _, 0))));
        // Unplugged and back: all states again
        device.input.borrow_mut().push(MESSAGE_GONE.to_vec());
        assert_eq!(app.poll_devices(), 0);
        app.set_time_ms(1300);
        app.tick();
        device.sent.borrow_mut().clear();
        device.input.borrow_mut().push(HELLO_JAM.to_vec());
        app.set_time_ms(1400);
        app.tick();
        assert!(device.sent.borrow().len() > 20, "{}", device.sent.borrow().len());
        // The exit's LEDs are flushed
        let flushes = device.flushes.get();
        app.shutdown();
        assert_eq!(device.flushes.get(), flushes + 1);
        assert!(!backend.take_calls().iter().any(|c| matches!(c, BackendCall::CloseInput(_) | BackendCall::CloseOutput(_))));
    }

    #[test]
    fn the_mk2_asks_the_library_for_its_shift() {
        let device = Rc::new(FakeDevice::default());
        let (mut app, _backend) = start(MK2_UUID, Some(FakeProvider(Some(device.clone()))));
        app.set_time_ms(1100);
        app.tick();
        assert!(device.sent.borrow().iter().any(|m| m[..] == crate::controller::maschine::mk::setup::HOST_QUERY), "{:?}", device.sent.borrow());
    }

    #[test]
    fn the_midi_ports_without_the_device() {
        for provider in [None, Some(FakeProvider(None))] {
            let (app, backend) = start(JAM_UUID, provider);
            assert!(!app.drives_device(0));
            let calls = backend.take_calls();
            assert!(calls.contains(&BackendCall::OpenInput(0)) && calls.contains(&BackendCall::OpenOutput(0)), "{calls:?}");
        }
        // A controller without a device which the core drives: its ports in any case
        let (mut app, _) = start("72F32A83-A697-446B-9016-64AA3F9476E4", None);
        assert_eq!(app.poll_devices(), -1);
    }
}
