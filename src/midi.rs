// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! MIDI input and output via the Reaper backend (former `MidiInputImpl`, `MidiOutputImpl`,
//! `NoteInputImpl` and `NoteRepeatImpl`).

use std::collections::{BTreeSet, HashMap};

use crate::backend::{Processor, Sender};
use crate::framework::ids::{ButtonId, ContinuousId};

pub const CMD_NOTE_OFF: i32 = 0x80;
pub const CMD_NOTE_ON: i32 = 0x90;
pub const CMD_POLY_AFTERTOUCH: i32 = 0xA0;
pub const CMD_CC: i32 = 0xB0;
pub const CMD_PROGRAM_CHANGE: i32 = 0xC0;
pub const CMD_CHANNEL_AFTERTOUCH: i32 = 0xD0;
pub const CMD_PITCHBEND: i32 = 0xE0;

/// How a hardware control is bound to MIDI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindType {
    Cc,
    Note,
    PitchBend,
    ProgramChange,
}

/// Convert a string of hex values (separated by spaces or not) to bytes.
pub fn from_hex_str(data: &str) -> Vec<u8> {
    let compact: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    compact
        .as_bytes()
        .chunks(2)
        .filter_map(|pair| std::str::from_utf8(pair).ok().and_then(|s| u8::from_str_radix(s, 16).ok()))
        .collect()
}

/// Convert bytes into an upper case hex string without separators.
pub fn to_hex_str(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect()
}

/// Sends MIDI to a device opened in Reaper.
#[derive(Clone)]
pub struct MidiOutput {
    sender: Sender,
    device_id: Option<i32>,
}

impl MidiOutput {
    pub fn new(sender: Sender, device_id: Option<i32>) -> Self {
        MidiOutput { sender, device_id }
    }

    pub fn device_id(&self) -> Option<i32> {
        self.device_id
    }

    pub fn send_midi_short(&self, status: i32, data1: i32, data2: i32) {
        if let Some(id) = self.device_id {
            self.sender.backend().send_midi_data(id, &[status as u8, (data1 & 0x7F) as u8, (data2 & 0x7F) as u8]);
        }
    }

    pub fn send_cc(&self, cc: i32, value: i32) {
        self.send_cc_ex(0, cc, value);
    }

    pub fn send_cc_ex(&self, channel: i32, cc: i32, value: i32) {
        self.send_midi_short(CMD_CC + channel, cc, value);
    }

    pub fn send_note(&self, note: i32, velocity: i32) {
        self.send_note_ex(0, note, velocity);
    }

    pub fn send_note_ex(&self, channel: i32, note: i32, velocity: i32) {
        self.send_midi_short(CMD_NOTE_ON + channel, note, velocity);
    }

    pub fn send_polyphonic_aftertouch(&self, channel: i32, data1: i32, data2: i32) {
        self.send_midi_short(CMD_POLY_AFTERTOUCH + channel, data1, data2);
    }

    pub fn send_channel_aftertouch(&self, channel: i32, data1: i32, data2: i32) {
        self.send_midi_short(CMD_CHANNEL_AFTERTOUCH + channel, data1, data2);
    }

    pub fn send_pitchbend(&self, channel: i32, data1: i32, data2: i32) {
        self.send_midi_short(CMD_PITCHBEND + channel, data1, data2);
    }

    pub fn send_program_change(&self, channel: i32, bank_msb: i32, bank_lsb: i32, value: i32) {
        self.send_cc_ex(channel, 0, bank_msb);
        self.send_cc_ex(channel, 32, bank_lsb);
        self.send_midi_short(CMD_PROGRAM_CHANGE + channel, value, 0);
    }

    pub fn send_sysex(&self, data: &[u8]) {
        if let Some(id) = self.device_id {
            self.sender.backend().send_midi_data(id, data);
        }
    }

    /// Send a sysex message given as a hex string, e.g. "F0 7E 7F 06 01 F7".
    pub fn send_sysex_str(&self, data: &str) {
        self.send_sysex(&from_hex_str(data));
    }
}

/// The note repeat (arpeggiator) settings of a note input.
#[derive(Clone)]
pub struct NoteRepeat {
    sender: Sender,
    pub is_active: bool,
    pub period: f64,
    pub note_length: f64,
    pub use_pressure: bool,
    pub mode: usize,
}

/// The arpeggiator modes supported by Reaper (in the order of the backend indices).
pub const ARP_MODES: &[&str] = &["Down", "Up", "Down-Up", "Up-Down"];

impl NoteRepeat {
    pub fn new(sender: Sender) -> Self {
        NoteRepeat { sender, is_active: false, period: 1.0, note_length: 1.0, use_pressure: false, mode: 0 }
    }

    pub fn set_active(&mut self, active: bool) {
        if self.is_active != active {
            self.is_active = active;
            self.sender.bool_arg(Processor::NoteRepeat, "active", active);
        }
    }

    pub fn toggle_active(&mut self) {
        self.set_active(!self.is_active);
    }

    pub fn set_period(&mut self, period: f64) {
        self.sender.double_arg(Processor::NoteRepeat, "rate", 1.0 / period);
    }

    pub fn set_note_length(&mut self, length: f64) {
        self.sender.double_arg(Processor::NoteRepeat, "notelength", length);
    }

    pub fn toggle_use_pressure(&mut self) {
        self.sender.bool_arg(Processor::NoteRepeat, "velocity", !self.use_pressure);
    }

    pub fn set_use_pressure(&mut self, use_pressure: bool) {
        self.sender.bool_arg(Processor::NoteRepeat, "velocity", use_pressure);
    }

    pub fn set_mode(&mut self, mode: usize) {
        if mode < ARP_MODES.len() {
            self.sender.int_arg(Processor::NoteRepeat, "mode", mode as i32);
        }
    }
}

/// A note input which forwards matching MIDI messages directly to the Reaper track input.
pub struct NoteInput {
    sender: Sender,
    device_id: Option<i32>,
    index: i32,
    pub note_repeat: NoteRepeat,
}

impl NoteInput {
    /// Create a note input. The filters have the form "90????" where '?' is a wildcard.
    pub fn new(sender: Sender, device_id: Option<i32>, index: i32, filters: &[&str]) -> Result<Self, String> {
        let backend_filters = Self::create_backend_filters(filters)?;
        if let Some(id) = device_id {
            let filters: Vec<String> = backend_filters.into_iter().collect();
            sender.backend().set_note_input_filters(id, index, &filters);
        }
        Ok(NoteInput { note_repeat: NoteRepeat::new(sender.clone()), sender, device_id, index })
    }

    /// A note input which passes nothing: an empty filter list for the host (Bitwig then creates
    /// no note input, Reaper passes no message). The default note input of a controller without a
    /// keyboard (the Java version on Bitwig creates none).
    pub fn without_filters(sender: Sender, device_id: Option<i32>, index: i32) -> Self {
        if let Some(id) = device_id {
            sender.backend().set_note_input_filters(id, index, &[]);
        }
        NoteInput { note_repeat: NoteRepeat::new(sender.clone()), sender, device_id, index }
    }

    fn create_backend_filters(filters: &[&str]) -> Result<BTreeSet<String>, String> {
        let mut backend_filters = BTreeSet::new();
        if filters.is_empty() {
            for f in ["90", "80", "B001", "B00B", "B040", "E0", "D0", "A0"] {
                backend_filters.insert(f.to_string());
            }
            return Ok(backend_filters);
        }

        for filter in filters {
            if filter.len() != 6 {
                return Err("Filter must be 6 characters long!".into());
            }
            let part = |range: std::ops::Range<usize>| filter[range].replace('?', " ").trim().to_string();
            let status = part(0..2);
            let data1 = part(2..4);
            let data2 = part(4..6);
            if status.is_empty() {
                return Err("Filter has missing status!".into());
            }
            if data1.is_empty() && !data2.is_empty() {
                return Err("First data byte filter cannot be empty if second is set!".into());
            }
            if data1.len() == 1 || data2.len() == 1 {
                return Err("Can only handle 2 byte data byte filters!".into());
            }

            let results: Vec<String> = if status.len() == 1 {
                (0..=0xF).map(|i| format!("{status}{i:X}")).collect()
            } else {
                vec![status]
            };
            for result in results {
                let mut final_filter = result;
                if !data1.is_empty() {
                    final_filter.push_str(&data1);
                    if !data2.is_empty() {
                        final_filter.push_str(&data2);
                    }
                }
                backend_filters.insert(final_filter);
            }
        }
        Ok(backend_filters)
    }

    /// Set the translation table for the note values. -1 blocks a note.
    pub fn set_key_translation_table(&self, table: &[i32]) {
        if let Some(id) = self.device_id {
            self.sender.backend().set_note_input_key_translation_table(id, self.index, table);
        }
    }

    pub fn set_velocity_translation_table(&self, table: &[i32]) {
        if let Some(id) = self.device_id {
            self.sender.backend().set_note_input_velocity_translation_table(id, self.index, table);
        }
    }
}

/// Button event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonEvent {
    Down,
    Up,
    Long,
}

/// The result of matching an incoming MIDI message against the bound controls.
#[derive(Debug, Clone, PartialEq)]
pub enum MidiDispatch {
    /// A button was pressed or released, the value is in the range of [0..1].
    Button(ButtonId, ButtonEvent, f64),
    /// A continuous control changed its value, normalized to [0..1].
    Continuous(ContinuousId, f64),
    /// A continuous control was touched or released.
    Touch(ContinuousId, bool),
}

/// Receives MIDI from a device opened in Reaper and matches it against the bound controls.
pub struct MidiInput {
    sender: Sender,
    device_id: Option<i32>,
    note_inputs: Vec<NoteInput>,
    /// channel -> control -> (value or -1) -> button
    cc_buttons: HashMap<i32, HashMap<i32, Vec<(i32, ButtonId)>>>,
    note_buttons: HashMap<i32, HashMap<i32, Vec<(i32, ButtonId)>>>,
    cc_continuous: HashMap<i32, HashMap<i32, (ContinuousId, bool)>>,
    pitchbend_continuous: HashMap<i32, ContinuousId>,
    cc_touch: HashMap<i32, HashMap<i32, ContinuousId>>,
    note_touch: HashMap<i32, HashMap<i32, ContinuousId>>,
    last_cc_values: [i32; 32],
}

impl MidiInput {
    pub fn new(sender: Sender, device_id: Option<i32>, filters: &[&str]) -> Result<Self, String> {
        let default_input = NoteInput::new(sender.clone(), device_id, 0, filters)?;
        Ok(Self::with_default_input(sender, device_id, default_input))
    }

    /// A MIDI input whose default note input passes nothing (see [`NoteInput::without_filters`]).
    pub fn without_note_input(sender: Sender, device_id: Option<i32>) -> Self {
        let default_input = NoteInput::without_filters(sender.clone(), device_id, 0);
        Self::with_default_input(sender, device_id, default_input)
    }

    fn with_default_input(sender: Sender, device_id: Option<i32>, default_input: NoteInput) -> Self {
        MidiInput {
            sender,
            device_id,
            note_inputs: vec![default_input],
            cc_buttons: HashMap::new(),
            note_buttons: HashMap::new(),
            cc_continuous: HashMap::new(),
            pitchbend_continuous: HashMap::new(),
            cc_touch: HashMap::new(),
            note_touch: HashMap::new(),
            last_cc_values: [0; 32],
        }
    }

    pub fn device_id(&self) -> Option<i32> {
        self.device_id
    }

    pub fn default_note_input(&self) -> &NoteInput {
        &self.note_inputs[0]
    }

    pub fn default_note_input_mut(&mut self) -> &mut NoteInput {
        &mut self.note_inputs[0]
    }

    pub fn create_note_input(&mut self, filters: &[&str]) -> Result<usize, String> {
        let index = self.note_inputs.len();
        self.note_inputs.push(NoteInput::new(self.sender.clone(), self.device_id, index as i32, filters)?);
        Ok(index)
    }

    pub fn note_input(&self, index: usize) -> Option<&NoteInput> {
        self.note_inputs.get(index)
    }

    /// Send a MIDI event to Reaper as if it was played on the device (e.g. a note from a pad).
    pub fn send_raw_midi_event(&self, status: i32, data1: i32, data2: i32) {
        if let Some(id) = self.device_id {
            self.sender.backend().process_midi_arg(id, status, data1, data2);
        }
    }

    pub fn bind_button(&mut self, button: ButtonId, bind_type: BindType, channel: i32, control: i32, value: i32) {
        self.unbind_button(button);
        let map = match bind_type {
            BindType::Cc => &mut self.cc_buttons,
            BindType::Note => &mut self.note_buttons,
            _ => return,
        };
        let entries = map.entry(channel).or_default().entry(control).or_default();
        entries.retain(|(v, _)| *v != value);
        entries.push((value, button));
    }

    pub fn unbind_button(&mut self, button: ButtonId) {
        for map in [&mut self.cc_buttons, &mut self.note_buttons] {
            for controls in map.values_mut() {
                for entries in controls.values_mut() {
                    entries.retain(|(_, b)| *b != button);
                }
            }
        }
    }

    pub fn bind_continuous(&mut self, control_id: ContinuousId, bind_type: BindType, channel: i32, control: i32, is_hi_res: bool) {
        self.unbind_continuous(control_id);
        match bind_type {
            BindType::Cc => {
                self.cc_continuous.entry(channel).or_default().insert(control, (control_id, is_hi_res));
            }
            BindType::PitchBend => {
                self.pitchbend_continuous.insert(channel, control_id);
            }
            _ => {}
        }
    }

    pub fn unbind_continuous(&mut self, control_id: ContinuousId) {
        for controls in self.cc_continuous.values_mut() {
            controls.retain(|_, (c, _)| *c != control_id);
        }
        self.pitchbend_continuous.retain(|_, c| *c != control_id);
    }

    pub fn bind_touch(&mut self, control_id: ContinuousId, bind_type: BindType, channel: i32, control: i32) {
        let map = match bind_type {
            BindType::Cc => &mut self.cc_touch,
            BindType::Note => &mut self.note_touch,
            _ => return,
        };
        map.entry(channel).or_default().insert(control, control_id);
    }

    /// Match a short MIDI message. Returns the dispatched control event (if any) and whether the
    /// message should additionally be handed to the surface MIDI callback.
    pub fn handle_short_message(&mut self, status: i32, data1: i32, data2: i32) -> (Option<MidiDispatch>, bool) {
        if status == 0xF8 {
            return (None, false);
        }
        let command = status & 0xF0;
        let channel = status & 0xF;
        let dispatch = match command {
            CMD_CC => self.handle_controls_cc(channel, data1, data2),
            CMD_NOTE_OFF | CMD_NOTE_ON => self.handle_controls_note(channel, data1, data2, command == CMD_NOTE_OFF || data2 == 0),
            CMD_PITCHBEND => self
                .pitchbend_continuous
                .get(&channel)
                .map(|id| MidiDispatch::Continuous(*id, (data2 * 128 + data1) as f64 / 16383.0)),
            _ => None,
        };
        let forward = dispatch.is_none() || command == CMD_NOTE_ON || command == CMD_NOTE_OFF;
        (dispatch, forward)
    }

    fn handle_controls_note(&mut self, channel: i32, data1: i32, data2: i32, is_note_off: bool) -> Option<MidiDispatch> {
        if let Some(entries) = self.note_buttons.get(&channel).and_then(|m| m.get(&data1)) {
            for (value, button) in entries {
                if *value == -1 || *value == data2 {
                    let event = if is_note_off { ButtonEvent::Up } else { ButtonEvent::Down };
                    return Some(MidiDispatch::Button(*button, event, data2 as f64 / 127.0));
                }
            }
        }
        self.note_touch
            .get(&channel)
            .and_then(|m| m.get(&data1))
            .map(|id| MidiDispatch::Touch(*id, !is_note_off))
    }

    fn handle_controls_cc(&mut self, channel: i32, data1: i32, data2: i32) -> Option<MidiDispatch> {
        if let Some(entries) = self.cc_buttons.get(&channel).and_then(|m| m.get(&data1)) {
            for (value, button) in entries {
                if *value == -1 || *value == data2 {
                    let event = if *value == 0 || data2 > 0 { ButtonEvent::Down } else { ButtonEvent::Up };
                    return Some(MidiDispatch::Button(*button, event, data2 as f64 / 127.0));
                }
            }
        }
        if let Some(id) = self.cc_touch.get(&channel).and_then(|m| m.get(&data1)) {
            return Some(MidiDispatch::Touch(*id, data2 > 0));
        }
        if let Some((id, is_hi_res)) = self.cc_continuous.get(&channel).and_then(|m| m.get(&data1)).copied() {
            if is_hi_res {
                if data1 < 32 {
                    self.last_cc_values[data1 as usize] = data2;
                    return None;
                } else if data1 < 64 {
                    let value = self.last_cc_values[(data1 - 32) as usize] * 128 + data2;
                    return Some(MidiDispatch::Continuous(id, value as f64 / 16383.0));
                }
            }
            return Some(MidiDispatch::Continuous(id, data2 as f64 / 127.0));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend};
    use std::rc::Rc;

    #[test]
    fn filters_are_expanded() {
        let filters = NoteInput::create_backend_filters(&["80????", "9?????", "B040??"]).unwrap();
        assert!(filters.contains("80"));
        assert!(filters.contains("90"));
        assert!(filters.contains("9F"));
        assert!(filters.contains("B040"));
        assert_eq!(filters.len(), 18);
        assert!(NoteInput::create_backend_filters(&["80??"]).is_err());
    }

    #[test]
    fn button_binding_matches() {
        let backend = Rc::new(RecordingBackend::default());
        let mut input = MidiInput::new(Sender::new(backend.clone()), Some(3), &[]).unwrap();
        assert!(matches!(backend.take_calls()[0], BackendCall::NoteInputFilters(3, 0, _)));
        input.bind_button(ButtonId::Play, BindType::Cc, 0, 20, -1);
        let (dispatch, forward) = input.handle_short_message(0xB0, 20, 127);
        assert_eq!(dispatch, Some(MidiDispatch::Button(ButtonId::Play, ButtonEvent::Down, 1.0)));
        assert!(!forward);
        let (dispatch, _) = input.handle_short_message(0xB0, 20, 0);
        assert_eq!(dispatch, Some(MidiDispatch::Button(ButtonId::Play, ButtonEvent::Up, 0.0)));
        input.unbind_button(ButtonId::Play);
        assert_eq!(input.handle_short_message(0xB0, 20, 127), (None, true));
    }

    #[test]
    fn hex_conversion() {
        assert_eq!(from_hex_str("F0 00 20 29 F7"), vec![0xF0, 0x00, 0x20, 0x29, 0xF7]);
        assert_eq!(to_hex_str(&[0xF0, 0x7E]), "F07E");
    }
}

/// A MIDI device inquiry (universal system exclusive) and the parser for its response.
pub struct DeviceInquiry {
    data: Vec<u8>,
    response_type: InquiryResponseType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InquiryResponseType {
    Short,
    Long,
    NotInSpec,
    Error,
}

impl DeviceInquiry {
    const LENGTH_RESULT_SHORT: usize = 15;
    const LENGTH_RESULT_LONG: usize = 17;
    const LENGTH_DEVICE_FAMILY: usize = 4;
    const LENGTH_SOFTWARE_REVISION: usize = 4;
    const OFFSET_DEVICE_ID: usize = 2;
    const OFFSET_CONTENT_SHORT: usize = 6;
    const OFFSET_CONTENT_LONG: usize = 8;

    /// The sysex message to query the device.
    pub const QUERY: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

    pub fn new(data: &[u8]) -> Self {
        let mut response_type = match data.len() {
            Self::LENGTH_RESULT_SHORT => InquiryResponseType::Short,
            Self::LENGTH_RESULT_LONG => InquiryResponseType::Long,
            n if n > Self::LENGTH_RESULT_LONG => InquiryResponseType::NotInSpec,
            _ => InquiryResponseType::Error,
        };
        if response_type != InquiryResponseType::Error
            && (data[0] != 0xF0 || data[1] != 0x7E || data[3] != 0x06 || data[4] != 0x02 || data[data.len() - 1] != 0xF7)
        {
            response_type = InquiryResponseType::Error;
        }
        DeviceInquiry { data: data.to_vec(), response_type }
    }

    pub fn is_valid(&self) -> bool {
        self.response_type != InquiryResponseType::Error
    }

    pub fn device_id(&self) -> i32 {
        if self.is_valid() {
            self.data[Self::OFFSET_DEVICE_ID] as i32
        } else {
            -1
        }
    }

    fn content_start(&self) -> usize {
        if self.response_type == InquiryResponseType::Short {
            Self::OFFSET_CONTENT_SHORT
        } else {
            Self::OFFSET_CONTENT_LONG
        }
    }

    pub fn revision_level(&self) -> Vec<u8> {
        if !self.is_valid() {
            return Vec::new();
        }
        let start = self.content_start() + Self::LENGTH_DEVICE_FAMILY;
        self.data.get(start..start + Self::LENGTH_SOFTWARE_REVISION).map(|s| s.to_vec()).unwrap_or_default()
    }
}
