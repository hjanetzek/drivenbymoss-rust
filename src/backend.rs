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
