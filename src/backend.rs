// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Interface to communicate with the C++ extension (former `BackendExchange` + JNI natives).

use std::cell::RefCell;
use std::rc::Rc;

use crate::protocol::{Arg, Cmd, Legacy};

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
        }
    }

    /// All processors. The position is the number of the processor in the binary protocol
    /// (`protocol::bin`): append only.
    pub const ALL: [Processor; 38] = [
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
}

/// A call to the backend, recorded by the [`RecordingBackend`].
#[derive(Debug, Clone, PartialEq)]
pub enum BackendCall {
    NoArg(String, String),
    StringArg(String, String, String),
    StringArgs(String, String, Vec<String>),
    IntArg(String, String, i32),
    DoubleArg(String, String, f64),
    DelayUpdates(String),
    EnableUpdates(String, bool),
    Midi(i32, i32, i32, i32),
    OpenInput(i32),
    OpenOutput(i32),
    CloseInput(i32),
    CloseOutput(i32),
    SendMidi(i32, Vec<u8>),
    NoteInputFilters(i32, i32, Vec<String>),
    KeyTranslation(i32, i32, Vec<i32>),
    VelocityTranslation(i32, i32, Vec<i32>),
}

/// A backend which records all calls. Used for tests and when running without Reaper.
#[derive(Default)]
pub struct RecordingBackend {
    pub calls: RefCell<Vec<BackendCall>>,
    pub inputs: Vec<(i32, String)>,
    pub outputs: Vec<(i32, String)>,
    /// The log messages of the library (see [`Backend::log`]).
    pub logs: RefCell<Vec<String>>,
    /// Send the commands through their binary form: each command is encoded as a record and
    /// decoded again before its legacy form is recorded (the binary mode of the replay).
    pub binary: std::cell::Cell<bool>,
    /// The handled commands of a binary host (word i bit j = ID 0x8000 + 64*i + j, empty: all;
    /// the `handled` event of a recording): the command features come from them
    /// ([`Backend::command_features`]). None: a text host.
    pub handled: RefCell<Option<Vec<u64>>>,
}

impl RecordingBackend {
    pub fn take_calls(&self) -> Vec<BackendCall> {
        std::mem::take(&mut *self.calls.borrow_mut())
    }

    fn push(&self, call: BackendCall) {
        self.calls.borrow_mut().push(call);
    }

    fn record_legacy(&self, legacy: Legacy<'_>) {
        let Legacy { processor, command, arg } = legacy;
        let p = processor.id().to_string();
        self.push(match arg {
            Arg::None => BackendCall::NoArg(p, command),
            Arg::Int(v) => BackendCall::IntArg(p, command, v),
            Arg::Double(v) => BackendCall::DoubleArg(p, command, v),
            Arg::Str(v) => BackendCall::StringArg(p, command, v.into_owned()),
            Arg::Strs(v) => BackendCall::StringArgs(p, command, v.iter().map(|v| v.to_string()).collect()),
            Arg::Delay => BackendCall::DelayUpdates(p),
            Arg::Enable(v) => BackendCall::EnableUpdates(p, v),
        });
    }
}

impl Backend for RecordingBackend {
    fn log(&self, message: &str) {
        self.logs.borrow_mut().push(message.to_string());
    }

    fn command_features(&self) -> Option<u32> {
        self.handled.borrow().as_deref().map(crate::daw::context::command_features)
    }

    /// Records the legacy form of the command, as the text hosts receive it.
    fn send_command(&self, cmd: &Cmd) {
        if self.binary.get() {
            let mut w = crate::protocol::bin::Writer::new();
            cmd.encode(&mut w);
            let record = crate::protocol::bin::records(w.as_bytes()).ok().and_then(|mut r| r.next()).and_then(Result::ok);
            let decoded = record.map(|record| Cmd::decode_record(&record));
            match decoded {
                Some(Ok(decoded)) => return self.record_legacy(decoded.legacy()),
                other => panic!("the command {cmd:?} does not survive its binary form: {other:?}"),
            }
        }
        self.record_legacy(cmd.legacy());
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
