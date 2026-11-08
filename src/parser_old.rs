// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The former path-matching parser (before the typed protocol, step 0b), kept for the
//! differential test of the new parser (test only).
//!
//! Reaper sends text messages of the form `<address> <value>`, e.g. `/track/3/volume 0.7`. The
//! parser updates the [`Model`] accordingly. Everything which the Java parser did with the
//! controller setup (re-binding knobs, updating the note mapping, configuration values, note
//! repeat settings of the surface) is returned as [`ParserEvent`]s which the app layer handles.

#![allow(dead_code)]

use crate::daw::clip::{Note, StepInfo};
use crate::daw::color::ColorEx;
use crate::daw::constants::{AutomationMode, ChannelType, DeviceId, GrooveParameterId, RecordQuantization};
use crate::daw::device::SpecificDevice;
use crate::daw::model::Model;
use crate::daw::parameter::ParameterKind;
use crate::daw::parameter_bank::ParameterBank;
use crate::daw::track::Track;
use crate::daw::track_bank::TrackRef;

use crate::parser::ParserEvent;

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

    fn remaining(&self) -> Parts<'a> {
        Parts { parts: self.parts.clone(), pos: self.pos }
    }
}

fn parse_double(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

/// A boolean value: true if the number is positive.
fn parse_flag(value: &str) -> bool {
    parse_double(value).is_some_and(|v| v > 0.0)
}

fn parse_int(value: &str) -> Option<i32> {
    let v = value.trim();
    v.parse::<i32>().ok().or_else(|| v.parse::<f64>().ok().map(|d| d as i32))
}

/// The length of a shortened text: "&lt;value&gt;/str/&lt;length&gt;" (Bitwig, `getLimited`).
fn parse_limit(parts: &mut Parts) -> Option<usize> {
    parts.poll().and_then(|p| p.parse().ok())
}

fn parse_index(value: &str) -> Option<usize> {
    let v = value.trim();
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse::<usize>().ok()
}

/// Parses the messages from Reaper and updates the model.
pub struct OldParser {
    events: Vec<ParserEvent>,
}

impl Default for OldParser {
    fn default() -> Self {
        Self::new()
    }
}

impl OldParser {
    pub fn new() -> Self {
        OldParser { events: Vec::new() }
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
                    // Bitwig (ProjectImpl): the project's remote controls (page cursors on the
                    // root track group, as the parameters of a device)
                    TAG_PARAM => {
                        let host = model.host().clone();
                        let param_cmd = parts.poll().unwrap_or("");
                        if let Some(bank) = model.project_mut().host_parameter_bank_mut() {
                            match parse_index(param_cmd) {
                                Some(param_no) => Self::parse_device_param_value(&host, param_no, bank, &mut parts, value),
                                None if param_cmd == TAG_COUNT => {
                                    if let Some(count) = parse_int(value) {
                                        bank.set_item_count(count.max(0) as usize);
                                        self.rebind_knobs();
                                    }
                                }
                                None => host.error(&format!("Unhandled project parameter: {param_cmd}")),
                            }
                        }
                    }
                    "page" => {
                        if let Some(bank) = model.project_mut().host_parameter_bank_mut() {
                            match (parts.poll(), parts.poll()) {
                                (Some(TAG_COUNT), None) => {
                                    if let Some(v) = parse_int(value) {
                                        bank.set_page_count(v.max(0) as usize);
                                    }
                                }
                                (Some("selected"), None) => {
                                    if let Some(v) = parse_int(value).filter(|v| *v >= 0) {
                                        bank.set_selected_page_from_host(v as usize);
                                    }
                                }
                                (Some(index), Some(TAG_NAME)) => {
                                    if let Some(page) = parse_index(index) {
                                        bank.set_page_name(page, value);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // Bitwig (ProjectImpl): solo and mute of all tracks, cue volume and mix
                    "hasSolo" => model.track_bank_mut().set_host_project_solo(parse_flag(value)),
                    "hasMute" => model.track_bank_mut().set_host_project_mute(parse_flag(value)),
                    "cueVolume" | "cueMix" => {
                        let project = model.project_mut();
                        let param = if project_cmd == "cueVolume" { project.cue_volume_parameter_mut() } else { project.cue_mix_parameter_mut() };
                        Self::parse_host_parameter(param, &mut parts, value);
                    }
                    _ => Self::error(model, &format!("Unhandled Project parameter: {project_cmd}")),
                }
            }
            // Bitwig: application.panelLayout (), the visibility flags of the arranger and the
            // mixer (ArrangerImpl, MixerImpl)
            "layout" => model.application_mut().set_internal_panel_layout(value),
            // Bitwig (ApplicationImpl): the global record quantization grid and note length
            "recordQuantization" => match parts.poll() {
                None => model.ctx().update_host_record_quantization(None, Some(RecordQuantization::lookup(value.trim()))),
                Some("noteLength") => model.ctx().update_host_record_quantization(Some(parse_flag(value)), None),
                Some(other) => Self::error(model, &format!("Unhandled record quantization parameter: {other}")),
            },
            "arranger" => {
                let flag = parts.poll().unwrap_or("");
                model.arranger_mut().set_flag_state(flag, parse_flag(value));
            }
            "mixer" => {
                let flag = parts.poll().unwrap_or("");
                model.mixer_mut().set_flag_state(flag, parse_flag(value));
            }
            "click" => Self::parse_click(model, &mut parts, value),
            "track" => self.parse_track(model, &mut parts, value),
            // The effect tracks (Bitwig's effect track bank): count, offset and the track values
            "fxtrack" => self.parse_effect_track(model, &mut parts, value),
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
            // Bitwig (TransportImpl.getCrossfadeParameter): the crossfader of the transport;
            // the master crossfader then sends `transport crossfade`
            "crossfade" => {
                if let Some(param) = model.crossfade_parameter_mut() {
                    if param.kind() != ParameterKind::HostCrossfade {
                        param.set_kind(ParameterKind::HostCrossfade);
                    }
                    Self::parse_host_parameter(param, &mut parts, value);
                }
            }
            "drum" => Self::parse_drum(model, &mut parts, value),
            // The command forms the host understands (HostFeature), e.g. `/host/features
            // valueInc valueReset`
            "host" => match parts.poll() {
                Some("features") => model.ctx().set_host_features(value),
                _ => Self::error(model, &format!("Unhandled host parameter: {osc}")),
            },
            _ => Self::error(model, &format!("Unhandled OSC address: {osc} {value}")),
        }
    }

    /// The drum machine of the cursor track with its 128 pads (protocol v2), applied to the main
    /// and the additional drum devices.
    /// A parameter of the host which is reported as `<prefix> <value>`, `<prefix>/str <text>`,
    /// `<prefix>/str/<limit> <text>` and `<prefix>/name <name>` (protocol v2). The parameter
    /// exists once its value is reported.
    fn parse_host_parameter(param: &mut crate::daw::parameter::Parameter, parts: &mut Parts, value: &str) {
        match parts.poll() {
            None => {
                if let Some(v) = parse_double(value) {
                    param.set_exists(true);
                    param.set_internal_value(v);
                }
            }
            Some("str") => match parse_limit(parts) {
                Some(limit) => param.set_limited_value_str(limit, value),
                None => param.set_value_str(value),
            },
            Some(TAG_NAME) => param.set_internal_name(value),
            Some(_) => {}
        }
    }

    fn parse_drum(model: &mut Model, parts: &mut Parts, value: &str) {
        let host = model.host().clone();
        let flag = parse_double(value).map(|v| v > 0.0);
        let command = parts.poll().unwrap_or("");
        let (pad, field, sub) = (parts.poll(), parts.poll(), parts.poll());
        let color = if field == Some(TAG_COLOR) { parse_color_static(&host, value) } else { None };
        model.for_each_drum_device(|device| match command {
            TAG_EXISTS => device.set_exists(flag.unwrap_or(false)),
            TAG_NAME => device.set_name(value),
            "bypass" => device.set_enabled(!flag.unwrap_or(false)),
            "window" => device.set_window_open(flag.unwrap_or(false)),
            "expand" => device.set_expanded(flag.unwrap_or(false)),
            "parameters" => device.set_parameter_page_section_visible(flag.unwrap_or(true)),
            "hasDrumPads" => device.set_has_drum_pads(flag.unwrap_or(false)),
            "hasLayers" => device.set_has_layers(flag.unwrap_or(false)),
            "hasMutedPads" => device.drum_pad_bank_mut().set_has_muted_pads(flag.unwrap_or(false)),
            "hasSoloedPads" => device.drum_pad_bank_mut().set_has_soloed_pads(flag.unwrap_or(false)),
            "pad" => {
                let Some(position) = pad.and_then(parse_index) else {
                    return;
                };
                let bank = device.drum_pad_bank_mut();
                bank.set_host_reported();
                Self::parse_device_channel(&host, bank.unpaged_item_mut(position), field.unwrap_or(""), sub, value, color);
            }
            other => host.error(&format!("Unhandled Drum parameter: {other}")),
        });
    }

    /// A field of a drum pad or layer of a device (protocol v2).
    fn parse_device_channel(host: &crate::host::Host, channel: &mut crate::daw::channel::Channel, field: &str, sub: Option<&str>, value: &str, color: Option<[f64; 3]>) {
        let flag = parse_double(value).is_some_and(|v| v > 0.0);
        match (field, sub) {
            (TAG_EXISTS, None) => channel.set_exists(flag),
            (TAG_NAME, None) => channel.set_internal_name(value),
            (TAG_COLOR, None) => channel.set_color_state(color.unwrap_or(ColorEx::GRAY.to_double_rgb())),
            (TAG_SELECT, None) => channel.set_selected(flag),
            (TAG_ACTIVE, None) => channel.set_internal_is_activated(flag),
            ("mute", None) => channel.set_mute_state(flag),
            ("solo", None) => channel.set_solo_state(flag),
            (TAG_VOLUME, None) => {
                if let Some(v) = parse_double(value) {
                    channel.set_internal_volume(v);
                }
            }
            (TAG_VOLUME, Some("str")) => channel.set_volume_str(value),
            ("pan", None) => {
                if let Some(v) = parse_double(value) {
                    channel.set_internal_pan(v);
                }
            }
            ("pan", Some("str")) => channel.set_pan_str(value),
            (other, _) => host.error(&format!("Unhandled Device Channel parameter: {other}")),
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
            // Bitwig: isMetronomeTickPlaybackEnabled
            "ticks" => transport.set_host_switch_state("ticks", parse_double(value).is_some_and(|v| v > 0.0)),
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
            "beats" => Self::parse_beats(model, parts, value),
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
            // Bitwig: the independent punch switches, the fill mode
            "punchIn" | "punchOut" | "fill" => model.transport_mut().set_host_switch_state(command, d.is_some_and(|v| v > 0.0)),
            "followPlayback" => {
                if let Some(v) = d {
                    model.arranger_mut().set_playback_follow(v > 0.0);
                }
            }
            "overdub" => {
                // Bitwig: the arranger and clip launcher overdub switches
                let enabled = d.is_some_and(|v| v > 0.0);
                match parts.poll() {
                    Some("arranger") => model.transport_mut().set_arranger_overdub_state(enabled),
                    Some("launcher") => model.transport_mut().set_launcher_overdub_state(enabled),
                    _ => return false,
                }
            }
            "automode" => {
                // Bitwig: the arranger and clip launcher automation write switches
                match parts.poll() {
                    Some("arranger") => {
                        model.transport_mut().set_writing_arranger_automation_state(d.is_some_and(|v| v > 0.0));
                        return true;
                    }
                    Some("launcher") => {
                        model.transport_mut().set_writing_clip_launcher_automation_state(d.is_some_and(|v| v > 0.0));
                        return true;
                    }
                    _ => {}
                }
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

    /// The play position and the loop in quarter notes (protocol v2); "text" values are formatted
    /// by the host and used as is.
    fn parse_beats(model: &mut Model, parts: &mut Parts, value: &str) {
        let transport = model.transport_mut();
        match (parts.poll(), parts.poll(), parts.poll()) {
            (Some("position"), None, None) => {
                if let Some(v) = parse_double(value) {
                    transport.set_position_beats_value(v);
                }
            }
            (Some("text"), None, None) => transport.set_beats_verbatim(value),
            (Some("loop"), Some("start"), None) => {
                if let Some(v) = parse_double(value) {
                    transport.set_loop_start_beats_value(v);
                }
            }
            (Some("loop"), Some("length"), None) => {
                if let Some(v) = parse_double(value) {
                    transport.set_loop_length_beats_value(v);
                }
            }
            (Some("loop"), Some("start"), Some("text")) => transport.set_loop_start_beat_text_verbatim(value),
            (Some("loop"), Some("length"), Some("text")) => transport.set_loop_length_beat_text_verbatim(value),
            _ => Self::error(model, &format!("Unhandled beats parameter: {value}")),
        }
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
            // Formatted by the host, used as is (Bitwig)
            "text" => model.transport_mut().set_position_text_verbatim(value),
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
            // The host pages the tracks (protocol v2); the core keeps its own offset
            "offset" => model.track_bank_mut().enable_host_paging(),
            // Bitwig (CursorTrackImpl): the position of the pinned cursor track (-1 if not
            // pinned), hasPrevious (), hasNext ()
            "pinned" => {
                if let Some(position) = parse_int(value) {
                    model.set_host_pinned_track(position);
                }
            }
            "hasPrevious" => model.set_host_cursor_track_neighbours(Some(parse_flag(value)), None),
            "hasNext" => model.set_host_cursor_track_neighbours(None, Some(parse_flag(value))),
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

    /// `/fxtrack/count`, `/fxtrack/offset` and `/fxtrack/<position>/<attribute>` (the attributes
    /// of `/track/<position>/…`), the effect track bank of the host.
    fn parse_effect_track(&mut self, model: &mut Model, parts: &mut Parts, value: &str) {
        let part = parts.poll().unwrap_or("");
        match part {
            "offset" => model.reported_effect_track_bank_mut().enable_host_paging(),
            TAG_COUNT => {
                if let Some(count) = parse_int(value) {
                    let tb = model.reported_effect_track_bank_mut();
                    tb.set_item_count(count.max(0) as usize);
                    tb.mark_dirty();
                    self.rebind_knobs();
                }
            }
            _ => match parse_index(part) {
                Some(position) => self.parse_track_value_in(model, true, TrackRef::Position(position), parts, value),
                None => Self::error(model, &format!("Unhandled effect track command: {part}")),
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
        self.parse_track_value_in(model, false, track_ref, parts, value);
    }

    /// A value of a track of the track bank or (`effect`) of the effect track bank.
    fn parse_track_value_in(&mut self, model: &mut Model, effect: bool, track_ref: TrackRef, parts: &mut Parts, value: &str) {
        let command = parts.poll().unwrap_or("");
        let d = parse_double(value);
        let color = if command == TAG_COLOR { Some(model.parse_color(value).unwrap_or(ColorEx::GRAY.to_double_rgb())) } else { None };
        let host = model.host().clone();
        let tb = if effect { model.reported_effect_track_bank_mut() } else { model.track_bank_mut() };
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
            // Bitwig (TrackImpl): canHoldNoteData (), canHoldAudioData ()
            "canHoldNotes" => track.set_can_hold_notes_state(parse_flag(value)),
            "canHoldAudio" => track.set_can_hold_audio_state(parse_flag(value)),
            "type" => match ChannelType::from_name(value.trim()) {
                Some(t) => {
                    track.set_type(t);
                    // A master in the track list of the host replaces the one of the core
                    if t == ChannelType::Master {
                        tb.mark_dirty();
                    }
                }
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
                    // Only a change: Bitwig reports the values of a track again after a command
                    // to it, a repeated selection must not e.g. recall the preferred view. The
                    // selection itself, not the pinned track (Track::is_selected)
                    if track.channel().is_selected() == is_selected {
                        return;
                    }
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
                Some("str") => match parse_limit(parts) {
                    Some(limit) => track.volume_parameter_mut().set_limited_value_str(limit, value),
                    None => track.set_volume_str(value),
                },
                Some(_) => {}
            },
            "pan" => match parts.poll() {
                None => {
                    if let Some(v) = d {
                        track.set_internal_pan(v);
                    }
                }
                Some("str") => match parse_limit(parts) {
                    Some(limit) => track.pan_parameter_mut().set_limited_value_str(limit, value),
                    None => track.set_pan_str(value),
                },
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
            "slot" => Self::parse_slot(&host, track, parts, value),
            "isStopped" => {
                if let Some(v) = d {
                    track.set_stopped_state(v > 0.0);
                }
            }
            "playingnotes" => {
                if let TrackRef::Position(p) = track_ref {
                    let notes = Note::parse_notes(value);
                    tb.handle_notes(p, &notes);
                }
            }
            _ => host.error(&format!("Unhandled Track Parameter: {command}")),
        }
    }

    /// The clip launcher slots of a track (protocol v2): "count" or "&lt;position&gt;/&lt;field&gt;" with
    /// the absolute slot (scene) position. Reported states replace the derived ones.
    fn parse_slot(host: &crate::host::Host, track: &mut Track, parts: &mut Parts, value: &str) {
        let flag = || parse_double(value).map(|v| v > 0.0);
        let slot_bank = track.slot_bank_mut();
        let Some(part) = parts.poll() else {
            return;
        };
        if part == TAG_COUNT {
            if let Some(count) = parse_int(value) {
                slot_bank.set_item_count(count.max(0) as usize);
            }
            return;
        }
        let Some(position) = parse_index(part) else {
            host.error(&format!("Unhandled Slot parameter: {part}"));
            return;
        };
        let slot = slot_bank.unpaged_item_mut(position);
        slot.set_position(position as i32);
        match parts.poll().unwrap_or("") {
            TAG_EXISTS => slot.set_exists(flag().unwrap_or(false)),
            "hasContent" => slot.set_has_content(flag().unwrap_or(false)),
            TAG_NAME => slot.set_internal_name(value),
            TAG_COLOR => slot.set_color_state(parse_color_static(host, value).map(ColorEx::from_array).unwrap_or(ColorEx::GRAY)),
            "selected" => slot.set_selected(flag().unwrap_or(false)),
            "playing" => slot.set_playing_state(flag().unwrap_or(false)),
            "recording" => slot.set_recording_state(flag().unwrap_or(false)),
            "playQueued" => slot.set_play_queued_state(flag().unwrap_or(false)),
            "recQueued" => slot.set_rec_queued_state(flag().unwrap_or(false)),
            "stopQueued" => slot.set_stop_queued_state(flag().unwrap_or(false)),
            other => host.error(&format!("Unhandled Slot parameter: {other}")),
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
                        Some("str") => match parse_limit(parts) {
                            Some(limit) => send.set_limited_value_str(limit, value),
                            None => send.set_value_str(value),
                        },
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
            "pinned" => {
                if let Some(v) = parse_double(value) {
                    device.set_pinned_state(v > 0.0);
                }
            }
            "expand" => {
                if let Some(v) = parse_double(value) {
                    device.set_expanded(v > 0.0);
                }
            }
            "parameters" => {
                if let Some(v) = parse_double(value) {
                    device.set_parameter_page_section_visible(v > 0.0);
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
            // Drum pads and layers of a device (protocol v2)
            "hasDrumPads" => device.set_has_drum_pads(parse_double(value).is_some_and(|v| v > 0.0)),
            "hasLayers" => device.set_has_layers(parse_double(value).is_some_and(|v| v > 0.0)),
            "hasMutedPads" => device.drum_pad_bank_mut().set_has_muted_pads(parse_double(value).is_some_and(|v| v > 0.0)),
            "hasSoloedPads" => device.drum_pad_bank_mut().set_has_soloed_pads(parse_double(value).is_some_and(|v| v > 0.0)),
            "pad" | "layer" => {
                let (index, field, sub) = (parts.poll().unwrap_or(""), parts.poll().unwrap_or(""), parts.poll());
                let color = if field == TAG_COLOR { parse_color_static(&host, value) } else { None };
                if index == TAG_COUNT {
                    if command == "layer" {
                        if let Some(v) = parse_int(value) {
                            device.layer_bank_mut().set_item_count(v.max(0) as usize);
                        }
                    }
                } else if let Some(position) = parse_index(index) {
                    if command == "pad" {
                        let bank = device.drum_pad_bank_mut();
                        bank.set_host_reported();
                        Self::parse_device_channel(&host, bank.unpaged_item_mut(position), field, sub, value, color);
                    } else {
                        Self::parse_device_channel(&host, device.layer_bank_mut().unpaged_item_mut(position), field, sub, value, color);
                    }
                } else {
                    host.error(&format!("Unhandled Device {command} parameter: {index}"));
                }
            }
            // The named parameter pages (protocol v2): count, selected, <index>/name
            "page" => {
                if let Some(bank) = device.parameter_bank_mut() {
                    match (parts.poll(), parts.poll()) {
                        (Some(TAG_COUNT), None) => {
                            if let Some(v) = parse_int(value) {
                                bank.set_page_count(v.max(0) as usize);
                            }
                        }
                        (Some("selected"), None) => {
                            if let Some(v) = parse_int(value) {
                                if v >= 0 {
                                    bank.set_selected_page_from_host(v as usize);
                                }
                            }
                        }
                        (Some(index), Some(TAG_NAME)) => match parse_index(index) {
                            Some(page) => bank.set_page_name(page, value),
                            None => host.error(&format!("Unhandled Device Page parameter: {index}")),
                        },
                        _ => host.error("Unhandled Device Page parameter"),
                    }
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
        // Bitwig: the named pages of the cursor track's remote controls and the page selected
        // in Bitwig (CursorTrackImpl.getParameterBank of the Java version)
        if type_part == "page" {
            if let Some(bank) = model.cursor_track_parameter_bank_mut() {
                match (parts.poll(), parts.poll()) {
                    (Some(TAG_COUNT), None) => {
                        if let Some(v) = parse_int(value) {
                            bank.set_page_count(v.max(0) as usize);
                        }
                    }
                    (Some("selected"), None) => {
                        if let Some(v) = parse_int(value).filter(|v| *v >= 0) {
                            bank.set_selected_page_from_host(v as usize);
                        }
                    }
                    (Some(index), Some(TAG_NAME)) => {
                        if let Some(page) = parse_index(index) {
                            bank.set_page_name(page, value);
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        if type_part != TAG_PARAM {
            Self::error(model, &format!("Unhandled Track FX parameter: {type_part}"));
            return;
        }
        let host = model.host().clone();
        let cmd = parts.poll().unwrap_or("");
        // A host which pins its cursor track (Bitwig) reports the remote controls of that track
        // (CursorTrackImpl.getParameterBank of the Java version), Reaper those of the selected
        // track
        let bank = if model.host_pins_track() {
            model.cursor_track_parameter_bank_mut()
        } else {
            match model.track_bank_mut().selected_item_mut() {
                Some(selected) => selected.parameter_bank_mut(),
                None => None,
            }
        };
        let Some(bank) = bank else {
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
                Some("str") => match parse_limit(parts) {
                    Some(limit) => bank.unpaged_item_mut(param_no).set_limited_value_str(limit, value),
                    None => bank.unpaged_item_mut(param_no).set_value_str(value),
                },
                Some(_) => {}
            },
            "steps" => {
                if let Some(v) = parse_int(value) {
                    bank.unpaged_item_mut(param_no).set_internal_number_of_steps(v);
                }
            }
            "modulated" => {
                if let Some(v) = parse_double(value) {
                    bank.unpaged_item_mut(param_no).set_modulated_value(v);
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
            // The browser of the host (protocol v2)
            TAG_EXISTS => browser.set_host_active(parse_flag(value)),
            "preview" => browser.set_preview(parse_flag(value)),
            "contentType" => match (parts.poll(), parts.poll()) {
                (Some(TAG_COUNT), None) => browser.set_content_type_count(parse_int(value).unwrap_or(0).max(0) as usize),
                (Some("selected"), None) => browser.set_selected_content_type(parse_int(value).unwrap_or(0).max(0) as usize),
                (Some("selected"), Some(TAG_NAME)) => browser.set_selected_content_type_name(value),
                (Some(index), Some(TAG_NAME)) => match parse_index(index) {
                    Some(i) => browser.set_content_type_name(i, value),
                    None => host.error(&format!("Unhandled Browser Content Type: {index}")),
                },
                (other, _) => host.error(&format!("Unhandled Browser Content Type: {}", other.unwrap_or(""))),
            },
            "column" => {
                let index = parts.poll().unwrap_or("");
                if index == TAG_COUNT {
                    browser.set_filter_column_count(parse_int(value).unwrap_or(0).max(0) as usize);
                    return;
                }
                let Some(column_index) = parse_index(index) else {
                    host.error(&format!("Unhandled Browser Column: {index}"));
                    return;
                };
                let column = browser.filter_column_mut(column_index);
                match (parts.poll(), parts.poll(), parts.poll()) {
                    (Some(TAG_EXISTS), None, None) => column.exists = parse_flag(value),
                    (Some(TAG_NAME), None, None) => column.name = value.to_string(),
                    (Some("wildcard"), None, None) => column.wildcard = value.to_string(),
                    (Some("cursorIndex"), None, None) => column.cursor_index = parse_int(value).unwrap_or(-1),
                    (Some("cursor"), Some(TAG_EXISTS), None) => column.cursor_exists = parse_flag(value),
                    (Some("cursor"), Some(TAG_NAME), None) => column.cursor_name = value.to_string(),
                    (Some("item"), Some(item), Some(field)) => {
                        if let Some(item) = parse_index(item).and_then(|i| column.items.get_mut(i)) {
                            Self::parse_browser_item(item, field, value);
                        }
                    }
                    (field, _, _) => host.error(&format!("Unhandled Browser Column Parameter: {}", field.unwrap_or(""))),
                }
            }
            "result" => match (parts.poll(), parts.poll()) {
                (Some("cursor"), Some(TAG_NAME)) => browser.set_selected_result_name(value),
                (Some(index), Some(field)) => {
                    if let Some(item) = parse_index(index).and_then(|i| browser.result_item_mut(i)) {
                        Self::parse_browser_item(item, field, value);
                    }
                }
                (other, _) => host.error(&format!("Unhandled Browser Result: {}", other.unwrap_or(""))),
            },
            _ => host.error(&format!("Unhandled Browser Parameter: {command}")),
        }
    }

    fn parse_browser_item(item: &mut crate::daw::browser::BrowserColumnItem, field: &str, value: &str) {
        match field {
            TAG_EXISTS => item.exists = parse_flag(value),
            TAG_NAME => item.name = value.to_string(),
            TAG_SELECT => item.is_selected = parse_flag(value),
            "hits" => item.hit_count = parse_int(value).unwrap_or(0),
            _ => {}
        }
    }

    fn parse_marker(model: &mut Model, parts: &mut Parts, value: &str) {
        let part = parts.poll().unwrap_or("");
        // Only the value of the color is a color (the other values logged color errors)
        let color = if parts.peek() == Some(TAG_COLOR) { model.parse_color(value) } else { None };
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
        // The host pages the scenes (protocol v2); the core keeps its own offset
        if parts.peek() == Some("offset") {
            model.track_bank_mut().enable_host_scene_paging();
            return;
        }
        // Only parse the color for the color command, other values are no colors
        let is_color = {
            let mut p = parts.remaining();
            p.poll();
            p.poll() == Some(TAG_COLOR)
        };
        let color = if is_color { model.parse_color(value) } else { None };
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
                "select" => scene.set_selected(parse_double(value).is_some_and(|v| v > 0.0)),
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
            // The step grids of the host (protocol v2)
            "loopStart" | "loopLength" | "shuffle" | "accent" | "pinned" => {
                let v = d.unwrap_or(0.0);
                model.for_each_cursor_clip(|clip| {
                    clip.set_host_grid();
                    match command {
                        "loopStart" => clip.set_loop_start_value(v),
                        "loopLength" => clip.set_loop_length_value(v),
                        "shuffle" => clip.set_shuffle_value(v > 0.0),
                        "accent" => clip.set_accent_value(v),
                        _ => clip.set_pinned_value(v > 0.0),
                    }
                });
            }
            "grid" => {
                let id = parts.poll().unwrap_or("");
                let field = parts.poll().unwrap_or("");
                let Some(clip) = model.note_clip_by_grid_id_mut(id) else {
                    Self::error(model, &format!("Unknown Clip Grid: {id}"));
                    return;
                };
                match field {
                    "playingStep" => clip.set_host_playing_step(parse_int(value).unwrap_or(-1)),
                    "step" => {
                        let mut index = || parts.poll().and_then(parse_index).map(|i| i as i32);
                        if let (Some(channel), Some(step), Some(row)) = (index(), index(), index()) {
                            clip.set_host_step(channel, step, row, StepInfo::from_wire(value));
                        }
                    }
                    _ => Self::error(model, &format!("Unhandled Clip Grid Parameter: {field}")),
                }
            }
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
        // Bitwig: the six parameters of GrooveImpl
        if let Some(param) = model.groove_mut().host_parameter_mut(command) {
            Self::parse_host_parameter(param, parts, value);
            return;
        }
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


/// The differential test of step 0b: the new parser ([`crate::parser::Parser`], the table and
/// `apply`) and the former one react the same to the model lines of all recordings and to every
/// line of the table with valid and invalid values: the same log lines (an "Unhandled …" error
/// counts as one, whatever its text), events and commands.
#[cfg(test)]
mod tests {
    use super::OldParser;
    use crate::backend::{RecordingBackend, Sender};
    use crate::daw::constants::DeviceId;
    use crate::daw::model::Model;
    use crate::daw::model_setup::ModelSetup;
    use crate::framework::value_changer::ValueChanger;
    use crate::host::{Host, MemoryLog};
    use crate::parser::Parser;
    use crate::protocol::MsgId;
    use crate::scale::Scales;
    use std::path::Path;
    use std::rc::Rc;

    struct Side {
        model: Model,
        backend: Rc<RecordingBackend>,
        log: Rc<MemoryLog>,
    }

    impl Side {
        fn new(grids: &[(usize, usize)]) -> Side {
            let mut setup = ModelSetup::default();
            setup.enable_device(DeviceId::FirstInstrument);
            setup.enable_device(DeviceId::Eq);
            setup.set_additional_drum_devices(vec![16]);
            setup.set_num_markers(8);
            let backend = Rc::new(RecordingBackend::default());
            let log = Rc::new(MemoryLog::default());
            let vc = ValueChanger::twos_complement(128, 1);
            let scales = Scales::new(&vc, 36, 100, 8, 8);
            let mut model = Model::new(Host::new(log.clone()), Sender::new(backend.clone()), setup, vc, scales);
            for (cols, rows) in grids {
                model.note_clip(*cols, *rows);
            }
            Side { model, backend, log }
        }

        /// The reaction to the last line: log lines, commands.
        fn take(&self) -> (Vec<String>, Vec<String>) {
            let mut lines: Vec<String> =
                self.log.lines.borrow_mut().drain(..).map(|l| if l.contains("Unhandled") { "Unhandled".to_string() } else { l }).collect();
            lines.dedup();
            let calls = self.backend.take_calls().iter().map(|c| format!("{c:?}")).collect();
            (lines, calls)
        }
    }

    /// Feed the lines to both parsers; returns the differences.
    pub(super) fn compare(lines: &[String], grids: &[(usize, usize)]) -> Vec<String> {
        let (mut old, mut new) = (Side::new(grids), Side::new(grids));
        let (mut old_parser, mut new_parser) = (OldParser::new(), Parser::new());
        old.take();
        new.take();
        let mut differences = Vec::new();
        for line in lines {
            let old_events = old_parser.parse_line(&mut old.model, line);
            let new_events = new_parser.parse_line(&mut new.model, line);
            let (o, n) = (old.take(), new.take());
            if (old_events != new_events || o != n) && differences.len() < 40 {
                differences.push(format!("{line:?}: old {old_events:?} {o:?}, new {new_events:?} {n:?}"));
            }
        }
        differences
    }

    fn grids_of(lines: &[String]) -> Vec<(usize, usize)> {
        let mut grids: Vec<(usize, usize)> = lines
            .iter()
            .filter_map(|l| l.strip_prefix("/clip/grid/")?.split('/').next()?.split_once('x').map(|(c, r)| (c.parse().ok(), r.parse().ok())))
            .filter_map(|(c, r)| Some((c?, r?)))
            .collect();
        grids.sort();
        grids.dedup();
        grids
    }

    #[test]
    fn the_new_parser_reacts_as_the_old_one_to_the_recordings() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
        let mut files: Vec<_> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .flat_map(|c| std::fs::read_dir(c.path().join("replay")).into_iter().flatten().flatten())
            .map(|f| f.path())
            .filter(|p| p.extension().is_some_and(|e| e == "rec"))
            .collect();
        files.sort();
        assert!(!files.is_empty());
        let mut differences = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let mut lines = Vec::new();
            for event in text.lines() {
                let mut fields = event.split('\t');
                if fields.nth(1) != Some("model") {
                    continue;
                }
                let model = fields.next().unwrap_or("").replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\");
                lines.extend(model.split('\n').filter(|l| !l.is_empty()).map(str::to_string));
            }
            let grids = grids_of(&lines);
            differences.extend(compare(&lines, &grids).into_iter().map(|d| format!("{}: {d}", file.display())));
        }
        assert!(differences.is_empty(), "{}", differences.join("\n"));
    }

    #[test]
    fn the_new_parser_reacts_as_the_old_one_to_every_table_line() {
        let mut lines = Vec::new();
        for id in MsgId::ALL {
            for k in 0..6 {
                let line = id.sample(k).to_text();
                let address = line.split_once(' ').map_or(line.as_str(), |(a, _)| a).to_string();
                lines.push(line.clone());
                for value in ["x", "", "-1", "0.5", "2", "1 2", "300 -2 7", "1:0:0.5:1:0:60:100"] {
                    lines.push(format!("{address} {value}"));
                }
                lines.push(format!("{} 1", address.trim_start_matches('/')));
            }
        }
        let differences = compare(&lines, &[(8, 128), (64, 1), (16, 8)]);
        assert!(differences.is_empty(), "{} differences:\n{}", differences.len(), differences.join("\n"));
    }
}

