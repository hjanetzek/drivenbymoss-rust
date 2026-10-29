// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Parser for the messages sent from Reaper (former `MessageParser`).
//!
//! Reaper sends text messages of the form `<address> <value>`, e.g. `/track/3/volume 0.7`. The
//! parser updates the [`Model`] accordingly. Everything which the Java parser did with the
//! controller setup (re-binding knobs, updating the note mapping, configuration values, note
//! repeat settings of the surface) is returned as [`ParserEvent`]s which the app layer handles.

use crate::daw::clip::Note;
use crate::daw::color::ColorEx;
use crate::daw::constants::{AutomationMode, ChannelType, DeviceId, GrooveParameterId};
use crate::daw::device::SpecificDevice;
use crate::daw::model::Model;
use crate::daw::parameter_bank::ParameterBank;
use crate::daw::track::Track;
use crate::daw::track_bank::TrackRef;

/// Actions which the app layer has to execute after parsing a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ParserEvent {
    /// The size of a bank (tracks, sends, parameters) changed: the active mode of all surfaces
    /// should be re-activated (`mode.on_activate()`) to rebind the knobs.
    RebindKnobs,
    /// The note mapping of the active view should be updated. Java executed this with a delay
    /// of [`UPDATE_NOTE_MAPPING_DELAY_MS`] milliseconds.
    UpdateNoteMapping,
    /// Set the quantize amount (`configuration.setQuantizeAmount`).
    QuantizeAmount(i32),
    /// Set the note repeat active state (`configuration.setNoteRepeatActive`).
    NoteRepeatActive(bool),
    /// Set the note repeat period (already converted: 1 / received value).
    NoteRepeatPeriod(f64),
    /// Set the note repeat note length.
    NoteRepeatNoteLength(f64),
    /// Set the note repeat mode (index of the arpeggiator mode).
    NoteRepeatMode(i32),
    /// Set if the note repeat uses the pressure for velocity.
    NoteRepeatUsePressure(bool),
    /// An action was selected in Reaper (`/action/select`, for the configuration dialog).
    ActionSelected(String),
}

/// The delay of the note mapping update in Java.
pub const UPDATE_NOTE_MAPPING_DELAY_MS: u64 = 1000;

const TAG_POSITION: &str = "position";
const TAG_ACTIVE: &str = "active";
const TAG_PARAM: &str = "param";
const TAG_COLOR: &str = "color";
const TAG_SELECT: &str = "select";
const TAG_NUMBER: &str = "number";
const TAG_COUNT: &str = "count";
const TAG_EXISTS: &str = "exists";
const TAG_NAME: &str = "name";
const TAG_VOLUME: &str = "volume";

/// A queue of path parts.
struct Parts<'a> {
    parts: Vec<&'a str>,
    pos: usize,
}

impl<'a> Parts<'a> {
    fn poll(&mut self) -> Option<&'a str> {
        let part = self.parts.get(self.pos).copied();
        if part.is_some() {
            self.pos += 1;
        }
        part
    }

    fn peek(&self) -> Option<&'a str> {
        self.parts.get(self.pos).copied()
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.parts.len()
    }

    fn remaining(&self) -> Parts<'a> {
        Parts { parts: self.parts.clone(), pos: self.pos }
    }
}

fn parse_double(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn parse_int(value: &str) -> Option<i32> {
    let v = value.trim();
    v.parse::<i32>().ok().or_else(|| v.parse::<f64>().ok().map(|d| d as i32))
}

fn parse_index(value: &str) -> Option<usize> {
    let v = value.trim();
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse::<usize>().ok()
}

/// Parses the messages from Reaper and updates the model.
pub struct Parser {
    events: Vec<ParserEvent>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Parser { events: Vec::new() }
    }

    /// Parse a message line `"<address> <value>"` (the value may contain spaces).
    pub fn parse_line(&mut self, model: &mut Model, line: &str) -> Vec<ParserEvent> {
        let (address, value) = line.split_once(' ').unwrap_or((line, ""));
        self.parse(model, address, value)
    }

    /// Parse a message and update the model. Returns the events for the app layer.
    pub fn parse(&mut self, model: &mut Model, address: &str, value: &str) -> Vec<ParserEvent> {
        if address == "/action/select" || address == "action/select" {
            return vec![ParserEvent::ActionSelected(value.to_string())];
        }
        self.parse_osc(model, address, value);
        model.after_parse();
        std::mem::take(&mut self.events)
    }

    fn error(model: &Model, message: &str) {
        model.host().error(message);
    }

    fn rebind_knobs(&mut self) {
        self.events.push(ParserEvent::RebindKnobs);
    }

    fn update_note_mapping(&mut self) {
        self.events.push(ParserEvent::UpdateNoteMapping);
    }

    fn parse_osc(&mut self, model: &mut Model, osc: &str, value: &str) {
        let path = osc.strip_prefix('/').unwrap_or(osc);
        if path.is_empty() {
            return;
        }
        let mut parts = Parts { parts: path.split('/').collect(), pos: 0 };
        let Some(command) = parts.poll() else {
            return;
        };

        if self.parse_transport(model, command, &mut parts, value) {
            return;
        }

        match command {
            "project" => {
                let project_cmd = parts.poll().unwrap_or("");
                match project_cmd {
                    TAG_NAME => {
                        model.project_mut().set_internal_name(value);
                        self.update_note_mapping();
                    }
                    "engine" => {
                        if let Some(v) = parse_int(value) {
                            model.application_mut().set_internal_engine_active(v > 0);
                        }
                    }
                    "canUndo" => {
                        if let Some(v) = parse_int(value) {
                            model.application_mut().set_can_undo_state(v > 0);
                        }
                    }
                    "canRedo" => {
                        if let Some(v) = parse_int(value) {
                            model.application_mut().set_can_redo_state(v > 0);
                        }
                    }
                    "isDirty" => {
                        if let Some(v) = parse_int(value) {
                            model.project_mut().set_dirty(v > 0);
                        }
                    }
                    _ => Self::error(model, &format!("Unhandled Project parameter: {project_cmd}")),
                }
            }
            "click" => Self::parse_click(model, &mut parts, value),
            "track" => self.parse_track(model, &mut parts, value),
            "master" => self.parse_master_track_value(model, &mut parts, value),
            "device" => self.parse_device_of(model, DeviceTarget::Cursor, value, &mut parts),
            "primary" => self.parse_device_of(model, DeviceTarget::Specific(DeviceId::FirstInstrument), value, &mut parts),
            "eq" => self.parse_device_of(model, DeviceTarget::Specific(DeviceId::Eq), value, &mut parts),
            "clip" => Self::parse_clip_value(model, &mut parts, value),
            "browser" => Self::parse_browser_value(model, &mut parts, value),
            "marker" => Self::parse_marker(model, &mut parts, value),
            "scene" => Self::parse_scene(model, &parts, value),
            "quantize" => {
                if parts.poll() == Some("strength") {
                    if let Some(v) = parse_int(value) {
                        self.events.push(ParserEvent::QuantizeAmount(v));
                    }
                }
            }
            "noterepeat" => self.parse_note_repeat(model, &mut parts, value),
            "groove" => Self::parse_groove(model, &mut parts, value),
            _ => Self::error(model, &format!("Unhandled OSC address: {osc} {value}")),
        }
    }

    fn parse_click(model: &mut Model, parts: &mut Parts, value: &str) {
        let transport = model.transport_mut();
        let Some(click_command) = parts.poll() else {
            if let Some(v) = parse_double(value) {
                transport.set_metronome_state(v > 0.0);
            }
            return;
        };
        match click_command {
            "preroll" => {
                if let Some(v) = parse_int(value) {
                    transport.set_preroll_metronome_internal((v & 2) > 0);
                }
            }
            "prerollMeasures" => {
                if let Some(v) = parse_double(value) {
                    transport.set_preroll_measures_internal(v as i32);
                }
            }
            TAG_VOLUME => {
                if let Some(v) = parse_double(value) {
                    transport.set_internal_metronome_volume(v);
                }
            }
            "volumeStr" => transport.metronome_volume_parameter_mut().set_metronome_volume_str(value),
            _ => Self::error(model, &format!("Unhandled Click Parameter: {click_command}")),
        }
    }

    fn parse_transport(&mut self, model: &mut Model, command: &str, parts: &mut Parts, value: &str) -> bool {
        let d = parse_double(value);
        match command {
            "play" => {
                if let Some(v) = d {
                    model.transport_mut().set_play_state(v > 0.0);
                }
            }
            "stop" => {
                if let Some(v) = d {
                    model.transport_mut().set_play_state(v == 0.0);
                }
            }
            "repeat" => {
                if let Some(v) = d {
                    model.transport_mut().set_looping_state(v > 0.0);
                }
            }
            "record" => {
                if let Some(v) = d {
                    model.transport_mut().set_record_state(v > 0.0);
                }
            }
            "tempo" => {
                if let Some(v) = d {
                    model.transport_mut().tempo_parameter_mut().set_internal_value(v);
                }
            }
            "time" => Self::parse_time(model, parts, value),
            "beat" => model.transport_mut().set_beats(value),
            "numerator" => {
                if let Some(v) = d {
                    if v as i32 > 0 {
                        model.transport_mut().set_numerator(v as i32);
                    }
                }
            }
            "denominator" => {
                if let Some(v) = d {
                    if v as i32 > 0 {
                        model.transport_mut().set_denominator(v as i32);
                    }
                }
            }
            "followPlayback" => {
                if let Some(v) = d {
                    model.arranger_mut().set_playback_follow(v > 0.0);
                }
            }
            "automode" => {
                let mode = match d.map(|v| v as i32) {
                    Some(0) => Some(AutomationMode::TrimRead),
                    Some(1) => Some(AutomationMode::Read),
                    Some(2) => Some(AutomationMode::Touch),
                    Some(3) => Some(AutomationMode::Write),
                    Some(4) => Some(AutomationMode::Latch),
                    Some(5) => Some(AutomationMode::LatchPreview),
                    _ => None,
                };
                if let Some(mode) = mode {
                    model.transport_mut().set_automation_write_mode_state(mode);
                }
            }
            _ => return false,
        }
        true
    }

    fn parse_time(model: &mut Model, parts: &mut Parts, value: &str) {
        let Some(cmd) = parts.poll() else {
            if let Some(v) = parse_double(value) {
                model.transport_mut().set_position_value(v);
            }
            return;
        };
        match cmd {
            "str" => model.transport_mut().set_position_text(value),
            "hzoom" => {
                if let Some(v) = parse_double(value) {
                    model.transport_mut().set_h_zoom(v);
                }
            }
            "loop" => {
                let Some(loop_cmd) = parts.poll() else {
                    return;
                };
                match loop_cmd {
                    "start" => match parts.poll() {
                        None => {
                            if let Some(v) = parse_double(value) {
                                model.transport_mut().set_loop_start_value(v);
                            }
                        }
                        Some("str") => model.transport_mut().set_loop_start_text(value),
                        Some(_) => model.transport_mut().set_loop_start_beat_text(value),
                    },
                    "length" => match parts.poll() {
                        None => {
                            if let Some(v) = parse_double(value) {
                                model.transport_mut().set_loop_length_value(v);
                            }
                        }
                        Some("str") => model.transport_mut().set_loop_length_text(value),
                        Some(_) => model.transport_mut().set_loop_length_beat_text(value),
                    },
                    _ => Self::error(model, &format!("Unhandled Loop parameter: {loop_cmd}")),
                }
            }
            _ => Self::error(model, &format!("Unhandled Time parameter: {cmd}")),
        }
    }

    fn parse_track(&mut self, model: &mut Model, parts: &mut Parts, value: &str) {
        let part = parts.poll().unwrap_or("");
        match part {
            "fx" => self.parse_track_fx_parameter(model, value, parts),
            TAG_COUNT => {
                if let Some(count) = parse_int(value) {
                    let tb = model.track_bank_mut();
                    tb.set_item_count(count.max(0) as usize);
                    tb.mark_dirty();
                    self.rebind_knobs();
                }
            }
            _ => match parse_index(part) {
                Some(position) => self.parse_track_value(model, TrackRef::Position(position), parts, value),
                None => Self::error(model, &format!("Unhandled Track command: {part}")),
            },
        }
    }

    fn parse_master_track_value(&mut self, model: &mut Model, parts: &mut Parts, value: &str) {
        if parts.peek() == Some("fx") {
            parts.poll();
            let cmd = parts.poll();
            if cmd == Some(TAG_PARAM) {
                let param_cmd = parts.poll().unwrap_or("");
                match parse_index(param_cmd) {
                    Some(param_no) => {
                        let host = model.host().clone();
                        let Some(bank) = model.project_mut().parameter_bank_mut() else {
                            return;
                        };
                        Self::parse_device_param_value(&host, param_no, bank, parts, value);
                        if param_no == 0 {
                            // Copy the values to the crossfader parameter
                            let mut copy = None;
                            if let Some(bank) = model.project().parameter_bank() {
                                let source = bank.unpaged_item(0);
                                copy = Some((source.name().to_string(), source.does_exist(), source.internal_value(), source.displayed_value()));
                            }
                            if let (Some((name, exists, v, s)), Some(dest)) = (copy, model.crossfade_parameter_mut()) {
                                dest.set_internal_name(&name);
                                dest.set_exists(exists);
                                dest.set_internal_value(v);
                                dest.set_value_str(&s);
                            }
                        }
                    }
                    None => {
                        if param_cmd == TAG_COUNT {
                            if let (Some(count), Some(bank)) = (parse_int(value), model.project_mut().parameter_bank_mut()) {
                                bank.set_item_count(count.max(0) as usize);
                                self.rebind_knobs();
                            }
                        } else {
                            Self::error(model, &format!("Unhandled Track FX Param parameter: {}", cmd.unwrap_or("")));
                        }
                    }
                }
            }
            return;
        }
        self.parse_track_value(model, TrackRef::Master, parts, value);
    }

    fn parse_track_value(&mut self, model: &mut Model, track_ref: TrackRef, parts: &mut Parts, value: &str) {
        let command = parts.poll().unwrap_or("");
        let d = parse_double(value);
        let color = if command == TAG_COLOR { Some(model.parse_color(value).unwrap_or(ColorEx::GRAY.to_double_rgb())) } else { None };
        let host = model.host().clone();
        let tb = model.track_bank_mut();
        let track: &mut Track = tb.track_mut(track_ref);
        match command {
            TAG_EXISTS => {
                if let Some(v) = d {
                    track.set_exists(v > 0.0);
                }
            }
            "depth" => {
                if let Some(v) = parse_int(value) {
                    track.set_depth(v);
                    tb.mark_dirty();
                }
            }
            TAG_ACTIVE => {
                if let Some(v) = d {
                    track.set_internal_is_activated(v > 0.0);
                    tb.mark_dirty();
                }
            }
            "type" => match ChannelType::from_name(value.trim()) {
                Some(t) => track.set_type(t),
                None => host.error(&format!("Unknown channel type: {value}")),
            },
            "isGroupExpanded" => {
                if let Some(v) = parse_int(value) {
                    track.set_is_group_expanded(v > 0);
                }
            }
            TAG_SELECT => {
                if let Some(v) = d {
                    let is_selected = v > 0.0;
                    track.set_selected(is_selected);
                    tb.handle_bank_track_selection(track_ref, is_selected);
                    if is_selected {
                        self.update_note_mapping();
                    }
                }
            }
            TAG_NUMBER => {
                if let Some(v) = parse_int(value) {
                    if track_ref == TrackRef::Master {
                        // The position of the master track is calculated by the track bank
                    } else {
                        track.set_position(v);
                    }
                }
            }
            TAG_NAME => track.set_internal_name(value),
            TAG_VOLUME => match parts.poll() {
                None => {
                    if let Some(v) = d {
                        track.set_internal_volume(v);
                    }
                }
                Some("str") => track.set_volume_str(value),
                Some(_) => {}
            },
            "pan" => match parts.poll() {
                None => {
                    if let Some(v) = d {
                        track.set_internal_pan(v);
                    }
                }
                Some("str") => track.set_pan_str(value),
                Some(_) => {}
            },
            "vu" | "vuleft" | "vuright" | "vuholdleft" | "vuholdright" | "mute" | "solo" | "recarm" | "monitor" | "autoMonitor" | "overdub"
            | "inQuantLengthEnabled" | "inQuantResolution" => {
                let Some(v) = d else {
                    return;
                };
                match command {
                    "vu" => track.set_vu(v),
                    "vuleft" => track.set_vu_left(v),
                    "vuright" => track.set_vu_right(v),
                    "vuholdleft" => track.set_vu_hold_left(v),
                    "vuholdright" => track.set_vu_hold_right(v),
                    "mute" => track.set_mute_state(v > 0.0),
                    "solo" => track.set_solo_state(v > 0.0),
                    "recarm" => track.set_rec_arm_state(v > 0.0),
                    "monitor" => track.set_monitor_state(v > 0.0),
                    "autoMonitor" => track.set_auto_monitor_state(v > 0.0),
                    "overdub" => track.set_overdub(v > 0.0),
                    "inQuantLengthEnabled" => track.set_record_quantization_note_length_state(v > 0.0),
                    _ => track.set_record_quantization_grid_value(v),
                }
            }
            TAG_COLOR => {
                if let Some(c) = color {
                    track.set_color_state(c);
                }
            }
            "send" => self.parse_send(&host, track, parts, value),
            "playingnotes" => {
                if let TrackRef::Position(p) = track_ref {
                    let notes = Note::parse_notes(value);
                    tb.handle_notes(p, &notes);
                }
            }
            _ => host.error(&format!("Unhandled Track Parameter: {command}")),
        }
    }

    fn parse_send(&mut self, host: &crate::host::Host, track: &mut Track, parts: &mut Parts, value: &str) {
        let Some(send_cmd) = parts.poll() else {
            return;
        };
        let send_bank = track.send_bank_mut();
        match parse_index(send_cmd) {
            Some(position) => {
                let send = send_bank.unpaged_item_mut(position);
                let command = parts.poll().unwrap_or("");
                match command {
                    TAG_ACTIVE => send.set_internal_enabled(parse_int(value).map(|v| v > 0).unwrap_or(false)),
                    TAG_NAME => {
                        send.set_internal_name(value);
                        send.set_exists(!value.is_empty());
                    }
                    TAG_VOLUME => match parts.poll() {
                        None => {
                            if let Some(v) = parse_double(value) {
                                send.set_internal_value(v);
                            }
                        }
                        Some("str") => send.set_value_str(value),
                        Some(_) => {}
                    },
                    TAG_COLOR => {
                        let c = parse_color_static(host, value).unwrap_or(ColorEx::GRAY.to_double_rgb());
                        send.set_color_state(c);
                    }
                    _ => host.error(&format!("Unhandled Send command: {command}")),
                }
            }
            None => {
                if send_cmd == TAG_COUNT {
                    if let Some(count) = parse_int(value) {
                        send_bank.set_item_count(count.max(0) as usize);
                        self.rebind_knobs();
                    }
                } else {
                    host.error(&format!("Unhandled Send command: {send_cmd}"));
                }
            }
        }
    }

    fn parse_device_of(&mut self, model: &mut Model, target: DeviceTarget, value: &str, parts: &mut Parts) {
        let host = model.host().clone();
        let device = match target {
            DeviceTarget::Cursor => Some(model.cursor_device_mut()),
            DeviceTarget::Specific(id) => model.specific_device_mut(id),
        };
        let Some(device) = device else {
            return;
        };
        let is_cursor = target == DeviceTarget::Cursor;
        let command = parts.poll().unwrap_or("");
        match command {
            TAG_COUNT => {
                if is_cursor {
                    if let Some(v) = parse_int(value) {
                        device.set_device_count(v.max(0) as usize);
                    }
                }
            }
            TAG_EXISTS => {
                if let Some(v) = parse_int(value) {
                    device.set_exists(v > 0);
                }
            }
            TAG_POSITION => {
                if let Some(v) = parse_int(value) {
                    device.set_position(v);
                }
            }
            "bypass" => {
                if let Some(v) = parse_int(value) {
                    device.set_enabled(v == 0);
                }
            }
            TAG_NAME => device.set_name(value),
            "window" => {
                if let Some(v) = parse_double(value) {
                    device.set_window_open(v > 0.0);
                }
            }
            "expand" => {
                if let Some(v) = parse_double(value) {
                    device.set_expanded(v > 0.0);
                }
            }
            "sibling" => {
                if is_cursor {
                    Self::parse_sibling(&host, device, parts, value);
                }
            }
            TAG_PARAM => {
                let cmd = parts.poll().unwrap_or("");
                match parse_index(cmd) {
                    Some(param_no) => {
                        if let Some(bank) = device.parameter_bank_mut() {
                            Self::parse_device_param_value(&host, param_no, bank, parts, value);
                        }
                    }
                    None => {
                        if cmd == TAG_COUNT {
                            if let Some(v) = parse_int(value) {
                                device.set_parameter_count(v.max(0) as usize);
                                self.rebind_knobs();
                            }
                        } else {
                            host.error(&format!("Unhandled Device Param parameter: {cmd}"));
                        }
                    }
                }
            }
            "touchedParam" => {
                if let Some(v) = parse_int(value) {
                    device.set_last_touched_parameter_index(v);
                }
            }
            "band" => {
                if target == DeviceTarget::Specific(DeviceId::Eq) {
                    let band_index = parts.poll().unwrap_or("");
                    match (parse_index(band_index), parse_int(value)) {
                        (Some(position), Some(v)) => device.set_type_internal(position, v),
                        _ => host.error(&format!("Unhandled Equalizer command: {band_index}")),
                    }
                }
            }
            _ => host.error(&format!("Unhandled Device parameter: {command}")),
        }
    }

    fn parse_sibling(host: &crate::host::Host, device: &mut SpecificDevice, parts: &mut Parts, value: &str) {
        let sibling_cmd = parts.poll().unwrap_or("");
        let Some(sibling_no) = parse_int(sibling_cmd).filter(|_| parse_index(sibling_cmd).is_some()) else {
            host.error(&format!("Unhandled device sibling parameter: {sibling_cmd}"));
            return;
        };
        let sibling_no = sibling_no - 1;
        let bank = device.device_bank_mut();
        if sibling_no < 0 || sibling_no as usize >= bank.page_size() {
            return;
        }
        let sibling = bank.item_mut(sibling_no as usize);
        let sub_cmd = parts.poll().unwrap_or("");
        match sub_cmd {
            TAG_NAME => {
                sibling.set_name(value);
                sibling.set_exists(!value.is_empty());
            }
            "bypass" => {
                if let Some(v) = parse_int(value) {
                    sibling.set_enabled(v == 0);
                }
            }
            TAG_POSITION => {
                if let Some(v) = parse_int(value) {
                    sibling.set_position(v);
                }
            }
            "selected" => {
                if let Some(v) = parse_int(value) {
                    sibling.set_selected(v > 0);
                }
            }
            _ => host.error(&format!("Unhandled device indexed sibling parameter: {sub_cmd}")),
        }
    }

    fn parse_track_fx_parameter(&mut self, model: &mut Model, value: &str, parts: &mut Parts) {
        let type_part = parts.poll().unwrap_or("");
        if type_part != TAG_PARAM {
            Self::error(model, &format!("Unhandled Track FX parameter: {type_part}"));
            return;
        }
        let host = model.host().clone();
        let Some(selected) = model.track_bank_mut().selected_item_mut() else {
            return;
        };
        let cmd = parts.poll().unwrap_or("");
        let Some(bank) = selected.parameter_bank_mut() else {
            return;
        };
        match parse_index(cmd) {
            Some(param_no) => Self::parse_device_param_value(&host, param_no, bank, parts, value),
            None => {
                if cmd == TAG_COUNT {
                    if let Some(v) = parse_int(value) {
                        bank.set_item_count(v.max(0) as usize);
                        self.rebind_knobs();
                    }
                } else {
                    host.error(&format!("Unhandled Track FX Param parameter: {cmd}"));
                }
            }
        }
    }

    fn parse_device_param_value(host: &crate::host::Host, param_no: usize, bank: &mut ParameterBank, parts: &mut Parts, value: &str) {
        let command = parts.poll().unwrap_or("");
        match command {
            TAG_NAME => {
                let p = bank.unpaged_item_mut(param_no);
                p.set_internal_name(value);
                p.set_position(param_no as i32);
                p.set_exists(!value.is_empty());
            }
            "value" => match parts.poll() {
                None => {
                    if let Some(v) = parse_double(value) {
                        bank.unpaged_item_mut(param_no).set_internal_value(v);
                        bank.notify_value_observers(param_no);
                    }
                }
                Some("str") => bank.unpaged_item_mut(param_no).set_value_str(value),
                Some(_) => {}
            },
            "steps" => {
                if let Some(v) = parse_int(value) {
                    bank.unpaged_item_mut(param_no).set_internal_number_of_steps(v);
                }
            }
            _ => host.error(&format!("Unhandled FX Param Value: {command}")),
        }
    }

    fn parse_browser_value(model: &mut Model, parts: &mut Parts, value: &str) {
        let host = model.host().clone();
        let Some(browser) = model.browser_mut() else {
            return;
        };
        let command = parts.poll().unwrap_or("");
        match command {
            "presetsfile" => browser.set_presets_file(value),
            "selected" => match parts.poll() {
                Some(TAG_NAME) => {}
                Some("index") => {
                    if let Some(v) = parse_int(value) {
                        browser.set_preset_selected(v);
                    }
                }
                _ => host.error(&format!("Unhandled Browser Parameter Selected: {command}")),
            },
            _ => host.error(&format!("Unhandled Browser Parameter: {command}")),
        }
    }

    fn parse_marker(model: &mut Model, parts: &mut Parts, value: &str) {
        let part = parts.poll().unwrap_or("");
        let color = model.parse_color(value);
        let host = model.host().clone();
        let bank = model.marker_bank_mut();
        match parse_index(part) {
            Some(position) => {
                let marker = bank.unpaged_item_mut(position);
                let command = parts.poll().unwrap_or("");
                match command {
                    TAG_EXISTS => {
                        if let Some(v) = parse_double(value) {
                            marker.set_exists(v > 0.0);
                        }
                    }
                    TAG_NUMBER => {
                        if let Some(v) = parse_int(value) {
                            marker.set_position(v);
                        }
                    }
                    TAG_NAME => marker.set_internal_name(value),
                    TAG_COLOR => marker.set_color_state(color.unwrap_or(ColorEx::GRAY.to_double_rgb())),
                    TAG_POSITION | "endPosition" => {}
                    _ => host.error(&format!("Unhandled Marker Parameter: {command}")),
                }
            }
            None => {
                if part == TAG_COUNT {
                    if let Some(v) = parse_int(value) {
                        bank.set_item_count(v.max(0) as usize);
                    }
                } else {
                    host.error(&format!("Unhandled Marker command: {part}"));
                }
            }
        }
    }

    fn parse_scene(model: &mut Model, parts: &Parts, value: &str) {
        let color = model.parse_color(value);
        let host = model.host().clone();
        model.for_each_scene_bank(|scene_bank| {
            let mut parts_copy = parts.remaining();
            let part = parts_copy.poll().unwrap_or("");
            if part == TAG_COUNT {
                if let Some(v) = parse_int(value) {
                    scene_bank.set_item_count(v.max(0) as usize);
                }
                return;
            }
            let Some(position) = parse_index(part) else {
                host.error(&format!("Unhandled Scene command: {part}"));
                return;
            };
            let scene = scene_bank.unpaged_item_mut(position);
            let command = parts_copy.poll().unwrap_or("");
            match command {
                TAG_EXISTS => {
                    if let Some(v) = parse_double(value) {
                        scene.set_exists(v > 0.0);
                    }
                }
                TAG_NUMBER => {
                    if let Some(v) = parse_int(value) {
                        scene.set_position(v);
                    }
                }
                TAG_NAME => scene.set_internal_name(value),
                TAG_COLOR => scene.set_color_state(color.map(ColorEx::from_array).unwrap_or(ColorEx::GRAY)),
                TAG_POSITION => {
                    if let Some(v) = parse_double(value) {
                        scene.set_begin_position(v);
                    }
                }
                "endPosition" => {
                    if let Some(v) = parse_double(value) {
                        scene.set_end_position(v);
                    }
                }
                _ => host.error(&format!("Unhandled Scene Parameter: {command}")),
            }
        });
    }

    fn parse_clip_value(model: &mut Model, parts: &mut Parts, value: &str) {
        let command = parts.poll().unwrap_or("");
        let d = parse_double(value);
        match command {
            TAG_EXISTS => {
                if let Some(v) = d {
                    model.set_cursor_clip_exists(v > 0.0);
                }
            }
            "start" => {
                if let Some(v) = d {
                    model.set_cursor_clip_play_start(v);
                }
            }
            "end" => {
                if let Some(v) = d {
                    model.set_cursor_clip_play_end(v);
                }
            }
            "playposition" => {
                if let Some(v) = d {
                    model.set_cursor_clip_play_position(v);
                }
            }
            TAG_COLOR => {
                let color = model.parse_color(value).unwrap_or(ColorEx::GRAY.to_double_rgb());
                model.set_cursor_clip_color_value(color);
            }
            "loop" => {
                if let Some(v) = d {
                    model.set_cursor_clip_loop_is_enabled(v > 0.0);
                }
            }
            "notes" => model.set_cursor_clip_notes(Note::parse_notes(value)),
            "all" => model.set_clips(value),
            _ => Self::error(model, &format!("Unhandled Clip Parameter: {command}")),
        }
    }

    fn parse_note_repeat(&mut self, model: &mut Model, parts: &mut Parts, value: &str) {
        let command = parts.poll().unwrap_or("");
        let event = match command {
            TAG_ACTIVE => parse_double(value).map(|v| ParserEvent::NoteRepeatActive(v > 0.0)),
            "period" => parse_double(value).map(|v| ParserEvent::NoteRepeatPeriod(1.0 / v)),
            "notelength" => parse_double(value).map(ParserEvent::NoteRepeatNoteLength),
            "mode" => parse_int(value).map(ParserEvent::NoteRepeatMode),
            "velocity" => parse_int(value).map(|v| ParserEvent::NoteRepeatUsePressure(v > 0)),
            _ => {
                Self::error(model, &format!("Unhandled NoteRepeat Parameter: {command}"));
                None
            }
        };
        if let Some(event) = event {
            self.events.push(event);
        }
    }

    fn parse_groove(model: &mut Model, parts: &mut Parts, value: &str) {
        let command = parts.poll().unwrap_or("");
        let Some(v) = parse_double(value) else {
            return;
        };
        match command {
            TAG_ACTIVE => model.groove_mut().parameter_mut(GrooveParameterId::Enabled).set_internal_value(v),
            "amount" => model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount).set_internal_value(v),
            _ => Self::error(model, &format!("Unhandled Groove Parameter: {command}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceTarget {
    Cursor,
    Specific(DeviceId),
}

fn parse_color_static(host: &crate::host::Host, value: &str) -> Option<[f64; 3]> {
    let values: Vec<&str> = value.split(' ').collect();
    if values.len() != 3 {
        host.error(&format!("Color: Wrong number of arguments: {}", values.len()));
        return None;
    }
    let r: f64 = values[0].trim().parse().ok()?;
    if r < 0.0 {
        return None;
    }
    let g: f64 = values[1].trim().parse().ok()?;
    let b: f64 = values[2].trim().parse().ok()?;
    Some([r / 255.0, g / 255.0, b / 255.0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use crate::daw::bank::BankItem;
    use crate::daw::events::ModelEvent;
    use crate::daw::model_setup::ModelSetup;
    use crate::framework::value_changer::ValueChanger;
    use crate::host::{Host, MemoryLog};
    use crate::scale::Scales;
    use std::rc::Rc;

    fn create(setup: ModelSetup) -> (Model, Parser, Rc<RecordingBackend>, Rc<MemoryLog>) {
        let backend = Rc::new(RecordingBackend::default());
        let log = Rc::new(MemoryLog::default());
        let vc = ValueChanger::twos_complement(128, 1);
        let scales = Scales::new(&vc, 36, 100, 8, 8);
        let model = Model::new(Host::new(log.clone()), Sender::new(backend.clone()), setup, vc, scales);
        (model, Parser::new(), backend, log)
    }

    fn feed(model: &mut Model, parser: &mut Parser, lines: &[&str]) -> Vec<ParserEvent> {
        lines.iter().flat_map(|l| parser.parse_line(model, l)).collect()
    }

    #[test]
    fn tracks() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        let events = feed(
            &mut model,
            &mut parser,
            &[
                "/track/count 10",
                "/track/0/exists 1",
                "/track/0/name Drums",
                "/track/0/number 0",
                "track/0/volume 0.5",
                "/track/0/volume/str -6.0 dB",
                "/track/0/mute 1",
                "/track/0/color 255 0 0",
                "/track/0/type INSTRUMENT",
                "/track/1/exists 1",
                "/track/1/select 1",
                "/play 1",
                "/tempo 133.5",
            ],
        );
        assert!(events.contains(&ParserEvent::RebindKnobs));
        assert!(events.contains(&ParserEvent::UpdateNoteMapping));
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let tb = model.track_bank();
        assert_eq!(tb.item_count(), 10);
        assert_eq!(tb.item(0).name(), "Drums");
        assert_eq!(tb.item(0).volume(), 64);
        assert_eq!(tb.item(0).volume_str(), "-6.0 dB");
        assert!(tb.item(0).is_mute());
        assert_eq!(tb.item(0).color(), ColorEx::RED);
        assert_eq!(tb.item(0).channel_type(), ChannelType::Instrument);
        assert!(!tb.item(2).does_exist());
        assert!(!tb.item(8).does_exist(), "Out of page");
        assert_eq!(tb.selected_index(), Some(1));
        assert_eq!(model.cursor_track().position(), 1);
        assert!(model.transport().is_playing());
        assert_eq!(model.transport().tempo(), 133.5);
        let events = model.take_events();
        assert!(events.contains(&ModelEvent::TrackSelectionChanged { index: 1, is_selected: true }));

        // Selecting a track on the second page scrolls the bank
        feed(&mut model, &mut parser, &["/track/1/select 0", "/track/9/exists 1", "/track/9/select 1"]);
        assert_eq!(model.track_bank().scroll_position(), 8);
        assert_eq!(model.track_bank().item(1).position(), 9);
        assert_eq!(model.cursor_track().position(), 9);

        // Commands
        backend.take_calls();
        model.cursor_track_mut().set_volume(127);
        model.track_bank_mut().item_mut(1).toggle_solo();
        model.track_bank_mut().item_mut(5).toggle_solo();
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::DoubleArg("track".into(), "9/volume".into(), 1.0), BackendCall::IntArg("track".into(), "9/solo".into(), 1)]
        );
    }

    #[test]
    fn track_paging() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        let mut lines = vec!["/track/count 20".to_string()];
        for i in 0..20 {
            lines.push(format!("/track/{i}/exists 1"));
        }
        lines.push("/track/0/select 1".into());
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        feed(&mut model, &mut parser, &refs);
        backend.take_calls();
        let tb = model.track_bank_mut();
        assert!(tb.can_scroll_page_forwards());
        assert!(!tb.can_scroll_page_backwards());
        tb.select_next_page();
        assert_eq!(tb.scroll_position(), 8);
        // The previously selected track is no longer on the new page, therefore no deselect
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "8/select".into(), 1)]);
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "8/select".into(), 1)]);
        model.track_bank_mut().scroll_to(17, true);
        assert_eq!(backend.take_calls(), vec![BackendCall::NoArg("track".into(), "16/scrollto".into())]);
        model.track_bank_mut().select_next_page();
        model.track_bank_mut().select_next_page();
        assert_eq!(model.track_bank().scroll_position(), 16);
        assert!(!model.track_bank().can_scroll_page_forwards());
        assert!(model.track_bank().item(3).does_exist());
        assert!(!model.track_bank().item(4).does_exist());
        // Shrink
        feed(&mut model, &mut parser, &["/track/count 5"]);
        assert_eq!(model.track_bank().scroll_position(), 0);
    }

    #[test]
    fn full_flat_track_list_with_master() {
        let mut setup = ModelSetup::default();
        setup.set_has_full_flat_track_list(true);
        let (mut model, mut parser, _, _) = create(setup);
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1", "/master/select 1"]);
        let tb = model.track_bank();
        assert_eq!(tb.item_count(), 3);
        assert!(tb.item(2).is_master());
        assert_eq!(tb.item(2).name(), "Master");
        assert_eq!(model.cursor_track_ref(), Some(TrackRef::Master));
    }

    #[test]
    fn folders() {
        let mut setup = ModelSetup::default();
        setup.set_has_flat_track_list(false);
        let (mut model, mut parser, _, _) = create(setup);
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/count 4",
                "/track/0/exists 1",
                "/track/0/type GROUP",
                "/track/1/exists 1",
                "/track/1/depth 1",
                "/track/2/exists 1",
                "/track/2/depth 1",
                "/track/3/exists 1",
            ],
        );
        assert_eq!(model.track_bank().item_count(), 2);
        assert_eq!(model.track_bank().item(1).position(), 3);
        feed(&mut model, &mut parser, &["/track/2/select 1"]);
        assert!(model.track_bank().has_parent());
        assert_eq!(model.track_bank().item_count(), 2);
        assert_eq!(model.track_bank().item(1).position(), 2);
        assert_eq!(model.track_bank().item(1).index(), 1);
    }

    #[test]
    fn sends_scenes_slots() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/count 2",
                "/track/0/exists 1",
                "/track/0/send/count 2",
                "/track/0/send/1/name FX",
                "/track/0/send/1/volume 0.25",
                "/track/0/send/1/volume/str -12 dB",
                "/scene/count 10",
                "/scene/0/exists 1",
                "/scene/0/name ",
                "/scene/0/position 0",
                "/scene/0/endPosition 10",
                "/scene/9/exists 1",
                "/scene/9/name Outro",
                "/clip/all 0;2;Clip A;1;255 0 0;0;Clip B;0;-1 0 0;1;",
                "/time 5",
                "/play 1",
            ],
        );
        let track = model.track_bank().item(0);
        let send = track.send_bank().item(1);
        assert!(send.does_exist());
        assert_eq!(send.name(), "FX");
        assert_eq!(send.value(), 32);
        assert_eq!(send.displayed_value(), "-12 dB");
        assert!(!track.send_bank().item(0).does_exist());

        assert_eq!(model.scene_bank().item(0).name(), "Scene 1");
        let slot = track.slot_bank().item(0);
        assert!(slot.does_exist() && slot.has_content());
        assert_eq!(slot.name(), "Clip A");
        assert_eq!(slot.color(), ColorEx::RED);
        assert!(slot.is_selected());
        assert!(slot.is_playing());
        assert!(!track.slot_bank().item(1).is_playing());
        assert!(track.slot_bank().item(1).is_muted());
        let track1 = model.track_bank().item(1);
        assert!(track1.slot_bank().item(1).does_exist());
        assert!(!track1.slot_bank().item(1).has_content());

        backend.take_calls();
        model.track_bank_mut().item_mut(0).slot_bank_mut().item_mut(1).launch(true, false);
        model.track_bank_mut().item_mut(0).send_bank_mut().item_mut(1).set_value(127);
        model.scene_bank_mut().item_mut(0).launch(true, false);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("track".into(), "0/clip/1/launch".into()),
                BackendCall::DoubleArg("track".into(), "0/send/1/volume".into(), 1.0),
                BackendCall::NoArg("scene".into(), "0/launch".into()),
            ]
        );

        // Scrolling the scene bank moves the slot banks
        model.scene_bank_mut().scroll_page_forwards();
        assert_eq!(model.scene_bank().item(1).name(), "Outro");
        // As in Java the offset of the slot bank is clamped to its item count (2 clips)
        assert_eq!(model.track_bank().item(0).slot_bank().scroll_position(), 1);
    }

    #[test]
    fn devices_and_params() {
        let mut setup = ModelSetup::default();
        setup.enable_device(DeviceId::Eq);
        let (mut model, mut parser, backend, log) = create(setup);
        let events = feed(
            &mut model,
            &mut parser,
            &[
                "/device/exists 1",
                "/device/name VST: ReaEQ (Cockos)",
                "/device/count 3",
                "/device/position 0",
                "/device/bypass 0",
                "/device/param/count 12",
                "/device/param/9/name Gain",
                "/device/param/9/value 0.5",
                "/device/param/9/value/str 0 dB",
                "/device/sibling/2/name VST: ReaComp (Cockos)",
                "/eq/exists 1",
                "/eq/band/1/value 8",
                "/primary/exists 1",
            ],
        );
        assert!(events.contains(&ParserEvent::RebindKnobs));
        assert!(log.lines.borrow().iter().all(|l| !l.contains("Unhandled") || l.contains("band")), "{:?}", log.lines.borrow());
        let device = model.cursor_device();
        assert_eq!(device.name(), "ReaEQ");
        assert!(device.is_enabled());
        assert!(device.can_select_next());
        assert_eq!(device.device_bank().item(1).name(), "ReaComp");
        let bank = device.parameter_bank().unwrap();
        assert_eq!(bank.page_bank().item_count(), 2);
        assert!(!bank.item(1).does_exist());
        model.take_events();
        let bank = model.cursor_device_mut().parameter_bank_mut().unwrap();
        bank.select_next_item();
        assert_eq!(bank.item(1).name(), "Gain");
        assert_eq!(bank.item(1).value(), 64);
        assert_eq!(bank.item(1).displayed_value(), "0 dB");
        backend.take_calls();
        bank.item_mut(1).set_value(127);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("device".into(), "param/9/value".into(), 1.0)]);
        assert!(model.take_events().contains(&ModelEvent::ParameterPageChanged { bank: crate::daw::events::ParameterBankId::CursorDevice }));
    }

    #[test]
    fn clip_and_transport() {
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        model.note_clip(8, 128);
        feed(
            &mut model,
            &mut parser,
            &[
                "/clip/exists 1",
                "/clip/start 0",
                "/clip/end 4",
                "/clip/loop 1",
                "/clip/color 0 255 0",
                "/time/str 1:23.456",
                "/time/loop/start 2",
                "/time/loop/length/str 0:04.000",
                "/time/loop/length/beat 2.1.00",
                "/numerator 3",
                "/denominator 4",
                "/automode 3",
                "/click 1",
                "/click/volumeStr -3 dB",
                "/project/name Test",
                "/groove/active 1",
                "/groove/amount 0",
                "/marker/count 1",
                "/marker/0/exists 1",
            ],
        );
        let clip = model.cursor_clip().unwrap();
        assert!(clip.does_exist());
        assert!(clip.is_loop_enabled());
        assert_eq!(clip.color(), ColorEx::GREEN);
        let t = model.transport();
        assert_eq!(t.position_text(), "1.23:456");
        assert_eq!(t.loop_start(), 2.0);
        // The time text must not overwrite the beat text and vice versa
        assert_ne!(t.loop_length_str(), "");
        assert_ne!(t.loop_length_beat_text(), "");
        assert_ne!(t.loop_length_str(), t.loop_length_beat_text());
        assert_eq!(t.quarters_per_measure(), 3);
        assert!(t.is_writing_arranger_automation());
        assert!(t.is_metronome_on());
        assert_eq!(t.metronome_volume_str(), "-3 dB");
        assert_eq!(model.project().name(), "Test");
        assert_eq!(model.groove().parameter(GrooveParameterId::Enabled).value(), 127);
        assert_eq!(model.groove().parameter(GrooveParameterId::ShuffleAmount).displayed_value(), "0.0 %");
        assert_eq!(model.marker_bank().item(0).name(), "Marker 1");
    }

    #[test]
    fn note_repeat_and_notes() {
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        let events = feed(&mut model, &mut parser, &["/noterepeat/active 1", "/noterepeat/period 4", "/quantize/strength 50"]);
        assert_eq!(events, vec![ParserEvent::NoteRepeatActive(true), ParserEvent::NoteRepeatPeriod(0.25), ParserEvent::QuantizeAmount(50)]);
        assert_eq!(parser.parse(&mut model, "/action/select", "123"), vec![ParserEvent::ActionSelected("123".into())]);
        feed(&mut model, &mut parser, &["/track/count 1", "/track/0/exists 1"]);
        model.take_events();
        let notes = "0:0:0.0:1.0:0:60:100";
        let parsed = Note::parse_notes(notes);
        if !parsed.is_empty() {
            feed(&mut model, &mut parser, &[&format!("/track/0/playingnotes {notes}")]);
            let events = model.take_events();
            assert!(events.iter().any(|e| matches!(e, ModelEvent::TrackNote { track_index: 0, .. })), "{events:?}");
        }
    }

    #[test]
    fn pinning() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/count 3", "/track/0/exists 1", "/track/1/exists 1", "/track/1/select 1"]);
        backend.take_calls();
        model.set_cursor_track_pinned(true);
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "1/pin".into(), 1)]);
        feed(&mut model, &mut parser, &["/track/1/select 0", "/track/0/select 1"]);
        assert_eq!(model.cursor_track().position(), 1);
        assert!(model.track_bank().item(1).is_selected());
        assert!(!model.track_bank().item(0).is_selected());
        model.set_cursor_track_pinned(false);
        assert_eq!(model.cursor_track().position(), 0);
    }

    #[test]
    fn unhandled_is_logged() {
        let (mut model, mut parser, _, log) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/foo/bar 1", "/track/abc/name x", "/track/0/volume notanumber"]);
        assert_eq!(log.lines.borrow().len(), 2);
    }
}
