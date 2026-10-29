// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Interface to communicate with the C++ extension (former `BackendExchange` + JNI natives).

use std::cell::RefCell;
use std::rc::Rc;

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
        }
    }
}

/// Functions provided by the C++ extension.
pub trait Backend {
    fn process_no_arg(&self, processor: &str, command: &str);
    fn process_string_arg(&self, processor: &str, command: &str, value: &str);
    fn process_string_args(&self, processor: &str, command: &str, values: &[&str]);
    fn process_int_arg(&self, processor: &str, command: &str, value: i32);
    fn process_double_arg(&self, processor: &str, command: &str, value: f64);
    fn delay_updates(&self, processor: &str);
    fn enable_updates(&self, processor: &str, enable: bool);
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

    /// A log message of the library. The default ignores it (it is also written to stderr).
    fn log(&self, _message: &str) {}
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

    pub fn no_arg(&self, processor: Processor, command: &str) {
        self.backend.process_no_arg(processor.id(), command);
    }

    pub fn string_arg(&self, processor: Processor, command: &str, value: &str) {
        self.backend.process_string_arg(processor.id(), command, value);
    }

    pub fn string_args(&self, processor: Processor, command: &str, values: &[&str]) {
        self.backend.process_string_args(processor.id(), command, values);
    }

    pub fn int_arg(&self, processor: Processor, command: &str, value: i32) {
        self.backend.process_int_arg(processor.id(), command, value);
    }

    pub fn double_arg(&self, processor: Processor, command: &str, value: f64) {
        self.backend.process_double_arg(processor.id(), command, value);
    }

    pub fn bool_arg(&self, processor: Processor, command: &str, value: bool) {
        self.int_arg(processor, command, if value { 1 } else { 0 });
    }

    pub fn delay_updates(&self, processor: Processor) {
        self.backend.delay_updates(processor.id());
    }

    pub fn enable_updates(&self, processor: Processor, enable: bool) {
        self.backend.enable_updates(processor.id(), enable);
    }


    /// Send a refresh command to the backend, which then sends all data again.
    pub fn refresh(&self) {
        self.no_arg(Processor::Refresh, "");
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
}

impl RecordingBackend {
    pub fn take_calls(&self) -> Vec<BackendCall> {
        std::mem::take(&mut *self.calls.borrow_mut())
    }

    fn push(&self, call: BackendCall) {
        self.calls.borrow_mut().push(call);
    }
}

impl Backend for RecordingBackend {
    fn process_no_arg(&self, processor: &str, command: &str) {
        self.push(BackendCall::NoArg(processor.into(), command.into()));
    }

    fn process_string_arg(&self, processor: &str, command: &str, value: &str) {
        self.push(BackendCall::StringArg(processor.into(), command.into(), value.into()));
    }

    fn process_string_args(&self, processor: &str, command: &str, values: &[&str]) {
        self.push(BackendCall::StringArgs(
            processor.into(),
            command.into(),
            values.iter().map(|v| v.to_string()).collect(),
        ));
    }

    fn process_int_arg(&self, processor: &str, command: &str, value: i32) {
        self.push(BackendCall::IntArg(processor.into(), command.into(), value));
    }

    fn process_double_arg(&self, processor: &str, command: &str, value: f64) {
        self.push(BackendCall::DoubleArg(processor.into(), command.into(), value));
    }

    fn delay_updates(&self, processor: &str) {
        self.push(BackendCall::DelayUpdates(processor.into()));
    }

    fn enable_updates(&self, processor: &str, enable: bool) {
        self.push(BackendCall::EnableUpdates(processor.into(), enable));
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
