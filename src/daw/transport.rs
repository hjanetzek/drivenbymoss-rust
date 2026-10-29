// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The transport (former `ITransport` / `TransportImpl`) with the tempo and metronome volume
//! parameters.
//!
//! Functions which need other parts of the model are on the model: crossfader
//! (`Model::crossfade_parameter`...) and the launcher overdub of the selected track
//! (`Model::is_launcher_overdub` / `Model::set_launcher_overdub`).

use crate::backend::Processor;
use crate::framework::value_changer::ValueChanger;

use super::constants::{transport_constants, AutomationMode, LaunchQuantization, PostRecordingAction};
use super::context::DawContext;

const ZOOM_RESOLUTIONS: [(f64, f64); 15] = [
    (8.8, 1.0),
    (27.94, 1.0 / 4.0),
    (279.11, 1.0 / 16.0),
    (661.61, 1.0 / 32.0),
    (1176.20, 1.0 / 64.0),
    (2091.03, 1.0 / 128.0),
    (4956.52, 1.0 / 256.0),
    (8811.59, 1.0 / 512.0),
    (20886.75, 1.0 / 1024.0),
    (37132.00, 1.0 / 2048.0),
    (66012.45, 1.0 / 4096.0),
    (156473.96, 1.0 / 8192.0),
    (278175.93, 1.0 / 16384.0),
    (600000.0, 1.0 / 32768.0),
    (800000.0, 1.0 / 65536.0),
];

const PUNCH_OFF: i32 = 0;
const PUNCH_ITEMS: i32 = 1;
const PUNCH_LOOP: i32 = 2;

const DEFAULT_TEMPO: f64 = 120.0;
const DEFAULT_METRONOME_VOLUME: f64 = 0.592449;

/// Format like Java's `DecimalFormat("#.00")`.
pub fn format_tempo(tempo: f64) -> String {
    let s = format!("{tempo:.2}");
    if let Some(rest) = s.strip_prefix("0.") {
        format!(".{rest}")
    } else if let Some(rest) = s.strip_prefix("-0.") {
        format!("-.{rest}")
    } else {
        s
    }
}

/// Format like Java's `DecimalFormat("###")`.
pub fn format_tempo_no_fraction(tempo: f64) -> String {
    format!("{tempo:.0}")
}

/// The tempo parameter (former `TempoParameterImpl`).
pub struct TempoParameter {
    ctx: DawContext,
    tempo: f64,
}

impl TempoParameter {
    pub fn does_exist(&self) -> bool {
        true
    }

    pub fn name(&self) -> &str {
        "Tempo"
    }

    /// The tempo in BPM.
    pub fn internal_value(&self) -> f64 {
        self.tempo
    }

    /// Not supported; the tempo can only be changed relatively.
    pub fn value(&self) -> i32 {
        0
    }

    pub fn displayed_value(&self) -> String {
        format_tempo(self.tempo)
    }

    /// Change the tempo with a relative knob value.
    pub fn change_value_with(&mut self, value_changer: &ValueChanger, control: i32) {
        let increase = value_changer.is_increase(control);
        let slow = value_changer.calc_knob_change(control).abs() > 1.0;
        let dir = match (increase, slow) {
            (true, true) => "+",
            (true, false) => "++",
            (false, true) => "-",
            (false, false) => "--",
        };
        self.ctx.send(Processor::Tempo, dir);
    }

    pub fn change_value(&mut self, control: i32) {
        let vc = self.ctx.value_changer().clone();
        self.change_value_with(&vc, control);
    }

    /// Sets only the local value (as in Java).
    pub fn set_value(&mut self, value: i32) {
        self.tempo = value as f64;
    }

    pub fn reset_value(&mut self) {
        self.tempo = DEFAULT_TEMPO;
    }

    pub fn set_internal_value(&mut self, value: f64) {
        self.tempo = value;
    }
}

/// The metronome volume parameter (former `MetronomeVolumeParameterImpl`).
pub struct MetronomeVolumeParameter {
    ctx: DawContext,
    volume: f64,
    volume_str: String,
}

impl MetronomeVolumeParameter {
    pub fn does_exist(&self) -> bool {
        true
    }

    pub fn name(&self) -> &str {
        "Metronome Volume"
    }

    pub fn value(&self) -> i32 {
        self.ctx.value_changer().from_normalized_value(self.volume.max(0.0))
    }

    pub fn displayed_value(&self) -> &str {
        &self.volume_str
    }

    pub fn set_value(&mut self, value: i32) {
        let v = self.ctx.value_changer().to_normalized_value(value as f64);
        self.set_normalized_value(v);
    }

    pub fn set_normalized_value(&mut self, value: f64) {
        self.volume = value;
        self.ctx.send_int(Processor::MetroVol, "", (self.volume * 127.0).round() as i32);
    }

    pub fn change_value(&mut self, control: i32) {
        let value = self.ctx.value_changer().change_value(control, self.value());
        self.set_value(value);
    }

    pub fn reset_value(&mut self) {
        self.set_normalized_value(DEFAULT_METRONOME_VOLUME);
    }

    pub fn inc(&mut self, increment: f64) {
        self.ctx.send(Processor::MetroVol, if increment > 0.0 { "+" } else { "-" });
    }

    pub fn set_internal_metronome_volume(&mut self, volume: f64) {
        self.volume = volume;
    }

    pub fn set_metronome_volume_str(&mut self, volume_str: &str) {
        self.volume_str = volume_str.to_string();
    }
}

/// The transport.
pub struct Transport {
    ctx: DawContext,
    position: f64,
    position_str: String,
    beats_str: String,
    loop_start: f64,
    loop_length: f64,
    loop_start_beats_str: String,
    loop_length_beats_str: String,
    /// The play position, loop start and loop length in quarter notes (`/beats/*`).
    position_beats: f64,
    loop_start_beats: f64,
    loop_length_beats: f64,
    loop_start_str: String,
    loop_length_str: String,
    is_metronome_on: bool,
    is_playing: bool,
    is_recording: bool,
    is_looping: bool,
    numerator: i32,
    denominator: i32,
    preroll_metronome: bool,
    preroll_measures: i32,
    punch_mode: i32,
    metronome_volume: MetronomeVolumeParameter,
    tempo: TempoParameter,
    automation_mode: AutomationMode,
    visible_pixels_per_second: f64,
}

impl Transport {
    pub fn new(ctx: DawContext) -> Self {
        Transport {
            metronome_volume: MetronomeVolumeParameter { ctx: ctx.clone(), volume: DEFAULT_METRONOME_VOLUME, volume_str: "0.0".into() },
            tempo: TempoParameter { ctx: ctx.clone(), tempo: DEFAULT_TEMPO },
            ctx,
            position: 0.0,
            position_str: String::new(),
            beats_str: String::new(),
            loop_start: 0.0,
            loop_length: 0.0,
            loop_start_beats_str: String::new(),
            loop_length_beats_str: String::new(),
            position_beats: 0.0,
            loop_start_beats: 0.0,
            loop_length_beats: 0.0,
            loop_start_str: String::new(),
            loop_length_str: String::new(),
            is_metronome_on: false,
            is_playing: false,
            is_recording: false,
            is_looping: false,
            numerator: 4,
            denominator: 4,
            preroll_metronome: false,
            preroll_measures: 2,
            punch_mode: PUNCH_OFF,
            automation_mode: AutomationMode::TrimRead,
            visible_pixels_per_second: 0.0,
        }
    }

    pub fn enable_observers(&self, enable: bool) {
        self.ctx.enable_updates(Processor::Transport, enable);
    }

    // ----------------------------------------------------------------------------------------
    // Play / record

    pub fn play(&mut self) {
        if self.is_playing && self.is_recording {
            self.ctx.send(Processor::Record, "");
        }
        self.ctx.send(Processor::Play, "");
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub fn restart(&mut self) {
        if self.is_playing {
            self.stop();
        }
        self.play();
    }

    pub fn stop(&mut self) {
        if self.is_playing && self.is_recording {
            self.ctx.send(Processor::Record, "");
        }
        self.ctx.send(Processor::Stop, "");
    }

    pub fn stop_and_rewind(&mut self) {
        self.stop();
        self.ctx.send_int(Processor::Time, "", 0);
    }

    pub fn start_recording(&mut self) {
        self.ctx.send(Processor::Record, "");
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording
    }

    // ----------------------------------------------------------------------------------------
    // Metronome

    pub fn set_metronome(&mut self, on: bool) {
        self.ctx.send_bool(Processor::Transport, "metronome", on);
    }

    pub fn is_metronome_on(&self) -> bool {
        self.is_metronome_on
    }

    pub fn toggle_metronome(&mut self) {
        self.ctx.send(Processor::Transport, "toggleMetronome");
    }

    pub fn is_metronome_ticks_on(&self) -> bool {
        false
    }

    pub fn toggle_metronome_ticks(&mut self) {}

    pub fn set_metronome_ticks(&mut self, _on: bool) {}

    pub fn metronome_volume_parameter(&self) -> &MetronomeVolumeParameter {
        &self.metronome_volume
    }

    pub fn metronome_volume_parameter_mut(&mut self) -> &mut MetronomeVolumeParameter {
        &mut self.metronome_volume
    }

    pub fn metronome_volume_str(&self) -> &str {
        self.metronome_volume.displayed_value()
    }

    pub fn change_metronome_volume(&mut self, control: i32) {
        let inc = if self.ctx.value_changer().is_increase(control) { 1.0 } else { -1.0 };
        self.metronome_volume.inc(inc);
    }

    pub fn set_metronome_volume(&mut self, value: i32) {
        self.metronome_volume.set_value(value);
    }

    pub fn metronome_volume(&self) -> i32 {
        self.metronome_volume.value()
    }

    pub fn is_preroll_metronome_enabled(&self) -> bool {
        self.preroll_metronome
    }

    pub fn toggle_preroll_metronome(&mut self) {
        self.ctx.send(Processor::Transport, "togglePrerollMetronome");
    }

    pub fn preroll_measures(&self) -> i32 {
        self.preroll_measures
    }

    pub fn set_preroll_measures(&mut self, measures: i32) {
        self.preroll_measures = measures;
        self.ctx.send_int(Processor::IniFile, "reaper/prerollmeas", measures);
    }

    // ----------------------------------------------------------------------------------------
    // Loop

    pub fn set_loop(&mut self, on: bool) {
        if on != self.is_looping {
            self.ctx.send_int(Processor::Repeat, "", 1);
        }
    }

    pub fn toggle_loop(&mut self) {
        self.ctx.send_int(Processor::Repeat, "", 1);
    }

    pub fn is_loop(&self) -> bool {
        self.is_looping
    }

    // ----------------------------------------------------------------------------------------
    // Automation

    pub fn is_writing_clip_launcher_automation(&self) -> bool {
        false
    }

    pub fn is_writing_arranger_automation(&self) -> bool {
        self.automation_mode != AutomationMode::Read && self.automation_mode != AutomationMode::TrimRead
    }

    pub fn automation_write_modes(&self) -> &'static [AutomationMode] {
        AutomationMode::ALL
    }

    pub fn automation_write_mode(&self) -> AutomationMode {
        self.automation_mode
    }

    pub fn set_automation_write_mode(&mut self, mode: AutomationMode) {
        let id = match mode {
            AutomationMode::TrimRead => 0,
            AutomationMode::Read => 1,
            AutomationMode::Touch => 2,
            AutomationMode::Write => 3,
            AutomationMode::Latch => 4,
            AutomationMode::LatchPreview => 5,
        };
        self.ctx.send_int(Processor::Automation, "", id);
    }

    pub fn previous_automation_write_mode(&mut self) {
        let modes = self.automation_write_modes();
        let pos = modes.iter().position(|m| *m == self.automation_mode).unwrap_or(0);
        self.set_automation_write_mode(modes[pos.saturating_sub(1)]);
    }

    pub fn next_automation_write_mode(&mut self, wrap: bool) {
        let modes = self.automation_write_modes();
        let mut pos = modes.iter().position(|m| *m == self.automation_mode).map(|p| p + 1).unwrap_or(0);
        if pos >= modes.len() {
            pos = if wrap { 0 } else { modes.len() - 1 };
        }
        self.set_automation_write_mode(modes[pos]);
    }

    /// The label of the automation mode (former `AutomationModeParameter.getDisplayedValue`).
    pub fn automation_mode_label(&self) -> &'static str {
        self.automation_mode.label()
    }

    /// Change the automation mode with a relative knob value (former `AutomationModeParameter`).
    pub fn change_automation_mode(&mut self, control: i32) {
        if self.ctx.value_changer().is_increase(control) {
            self.next_automation_write_mode(false);
        } else {
            self.previous_automation_write_mode();
        }
    }

    pub fn toggle_write_arranger_automation(&mut self) {
        if self.is_writing_arranger_automation() {
            self.set_automation_write_mode(AutomationMode::TrimRead);
        } else {
            self.set_automation_write_mode(AutomationMode::Write);
        }
    }

    pub fn toggle_write_clip_launcher_automation(&mut self) {}

    pub fn reset_automation_overrides(&mut self) {}

    pub fn return_to_arrangement(&mut self) {}

    // ----------------------------------------------------------------------------------------
    // Position

    pub fn position_text(&self) -> &str {
        &self.position_str
    }

    pub fn beat_text(&self) -> &str {
        &self.beats_str
    }

    /// The play position in seconds.
    pub fn position(&self) -> f64 {
        self.position
    }

    /// Set the play position in seconds (snapped to the grid).
    pub fn set_position(&mut self, time: f64) {
        self.set_position_with_snap(time, true);
    }

    fn set_position_with_snap(&mut self, time: f64, snap: bool) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.delay_updates(Processor::Transport);
        }
        self.position = time;
        self.ctx.update_transport_snapshot(|s| s.position = time);
        if snap {
            self.ctx.send_double(Processor::Time, "", self.position);
        } else {
            self.ctx.send_double(Processor::Time, "nosnap", self.position);
        }
    }

    pub fn set_position_to_end(&mut self) {
        self.ctx.send(Processor::Transport, "end");
    }

    /// The play position in quarter notes.
    pub fn position_beats(&self) -> f64 {
        self.position_beats
    }

    /// Set the play position in quarter notes; snapped to the grid if `snap` is set.
    pub fn set_position_beats(&mut self, beats: f64, snap: bool) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.delay_updates(Processor::Transport);
        }
        self.position_beats = beats;
        self.ctx.send_double(Processor::Beats, if snap { "position" } else { "position/nosnap" }, beats);
    }

    /// Move the play position depending on the zoom level.
    pub fn change_position(&mut self, increase: bool, slow: bool) {
        let resolution = self.zoom_resolution();
        let fraction = if slow { resolution } else { resolution * self.quarters_per_measure() as f64 * 4.0 };
        let pos = (self.position_beats / fraction).round() * fraction;
        let beats = if increase { pos + fraction } else { (pos - fraction).max(0.0) };
        self.set_position_beats(beats, !slow);
    }

    fn zoom_resolution(&self) -> f64 {
        for (key, value) in ZOOM_RESOLUTIONS {
            if self.visible_pixels_per_second < key {
                return value;
            }
        }
        800000.0
    }

    /// The loop start in seconds.
    pub fn loop_start(&self) -> f64 {
        self.loop_start
    }

    /// The loop start in quarter notes.
    pub fn loop_start_beats(&self) -> f64 {
        self.loop_start_beats
    }

    /// The loop length in quarter notes.
    pub fn loop_length_beats(&self) -> f64 {
        self.loop_length_beats
    }

    pub fn loop_end(&self) -> f64 {
        self.loop_start + self.loop_length
    }

    pub fn loop_length(&self) -> f64 {
        self.loop_length
    }

    pub fn loop_start_str(&self) -> &str {
        &self.loop_start_str
    }

    pub fn loop_start_beat_text(&self) -> &str {
        &self.loop_start_beats_str
    }

    pub fn loop_length_str(&self) -> &str {
        &self.loop_length_str
    }

    pub fn loop_length_beat_text(&self) -> &str {
        &self.loop_length_beats_str
    }

    pub fn set_loop_start(&mut self, time: f64) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.delay_updates(Processor::Transport);
        }
        self.loop_start = time;
        self.ctx.send_double(Processor::Time, "loop/start", self.loop_start);
    }

    /// Set the loop start in quarter notes, the length stays the same.
    pub fn set_loop_start_beats(&mut self, beats: f64) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.delay_updates(Processor::Transport);
        }
        self.loop_start_beats = beats;
        self.ctx.send_double(Processor::Beats, "loop/start", beats);
    }

    pub fn change_loop_start(&mut self, increase: bool, slow: bool) {
        let frac = if slow { transport_constants::INC_FRACTION_TIME_SLOW } else { transport_constants::INC_FRACTION_TIME };
        let v = if increase { self.loop_start_beats + frac } else { (self.loop_start_beats - frac).max(0.0) };
        self.set_loop_start_beats(v);
    }

    pub fn select_loop_start(&mut self) {
        self.set_position_beats(self.loop_start_beats, true);
    }

    pub fn select_loop_end(&mut self) {
        self.set_position_beats(self.loop_start_beats + self.loop_length_beats, true);
    }

    pub fn set_loop_length(&mut self, time: f64) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.delay_updates(Processor::Transport);
        }
        self.loop_length = time;
        self.ctx.send_double(Processor::Time, "loop/length", self.loop_length);
    }

    /// Set the loop length in quarter notes.
    pub fn set_loop_length_beats(&mut self, beats: f64) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.delay_updates(Processor::Transport);
        }
        self.loop_length_beats = beats;
        self.ctx.send_double(Processor::Beats, "loop/length", beats);
    }

    pub fn change_loop_length(&mut self, increase: bool, slow: bool) {
        let frac = if slow { transport_constants::INC_FRACTION_TIME_SLOW } else { transport_constants::INC_FRACTION_TIME };
        let v = if increase { self.loop_length_beats + frac } else { (self.loop_length_beats - frac).max(0.0) };
        self.set_loop_length_beats(v);
    }

    // ----------------------------------------------------------------------------------------
    // Punch

    pub fn toggle_punch_in(&mut self) {
        let v = !self.is_punch_in_enabled();
        self.set_punch_in(v);
    }

    pub fn set_punch_in(&mut self, enable: bool) {
        self.ctx.send_bool(Processor::Transport, "punchIn", enable);
        self.punch_mode = if enable { PUNCH_LOOP } else { PUNCH_OFF };
    }

    pub fn is_punch_in_enabled(&self) -> bool {
        self.punch_mode != PUNCH_OFF
    }

    pub fn toggle_punch_out(&mut self) {
        let v = !self.is_punch_out_enabled();
        self.set_punch_out(v);
    }

    pub fn set_punch_out(&mut self, enable: bool) {
        self.ctx.send_bool(Processor::Transport, "punchOut", enable);
        self.punch_mode = if enable { PUNCH_ITEMS } else { PUNCH_OFF };
    }

    pub fn is_punch_out_enabled(&self) -> bool {
        self.punch_mode != PUNCH_OFF
    }

    // ----------------------------------------------------------------------------------------
    // Tempo

    pub fn tap_tempo(&mut self) {
        self.ctx.send(Processor::Transport, "tapTempo");
    }

    pub fn change_tempo(&mut self, increase: bool, slow: bool) {
        let dir = match (increase, slow) {
            (true, true) => "+",
            (true, false) => "++",
            (false, true) => "-",
            (false, false) => "--",
        };
        self.ctx.send(Processor::Tempo, dir);
    }

    pub fn tempo_parameter(&self) -> &TempoParameter {
        &self.tempo
    }

    pub fn tempo_parameter_mut(&mut self) -> &mut TempoParameter {
        &mut self.tempo
    }

    pub fn set_tempo(&mut self, tempo: f64) {
        self.ctx.send_double(Processor::Tempo, "", tempo);
    }

    /// The tempo in BPM.
    pub fn tempo(&self) -> f64 {
        self.tempo.internal_value()
    }

    pub fn minimum_tempo(&self) -> f64 {
        2.0
    }

    pub fn maximum_tempo(&self) -> f64 {
        960.0
    }

    pub fn format_tempo(&self, tempo: f64) -> String {
        format_tempo(tempo)
    }

    pub fn format_tempo_no_fraction(&self, tempo: f64) -> String {
        format_tempo_no_fraction(tempo)
    }

    pub fn set_tempo_indication(&mut self, _is_touched: bool) {}

    /// Scale the tempo to the range of [0..max_value-1].
    pub fn scale_tempo(&self, tempo: f64, max_value: i32) -> f64 {
        let v = tempo - transport_constants::MIN_TEMPO as f64;
        v * (max_value - 1) as f64 / (transport_constants::MAX_TEMPO - transport_constants::MIN_TEMPO) as f64
    }

    // ----------------------------------------------------------------------------------------
    // Time signature

    pub fn numerator(&self) -> i32 {
        self.numerator
    }

    pub fn denominator(&self) -> i32 {
        self.denominator
    }

    pub fn set_time_signature(&mut self, numerator: i32, denominator: i32) {
        self.ctx.send_int(Processor::Time, "signature", numerator + denominator * 100);
    }

    pub fn quarters_per_measure(&self) -> i32 {
        4 * self.numerator / self.denominator
    }

    // ----------------------------------------------------------------------------------------
    // Not supported in Reaper

    pub fn clip_launcher_post_recording_action(&self) -> PostRecordingAction {
        PostRecordingAction::Off
    }

    pub fn set_clip_launcher_post_recording_action(&mut self, _action: PostRecordingAction) {}

    pub fn clip_launcher_post_recording_time_offset(&self) -> f64 {
        0.0
    }

    pub fn set_clip_launcher_post_recording_time_offset(&mut self, _beats: f64) {}

    pub fn default_launch_quantization(&self) -> LaunchQuantization {
        LaunchQuantization::ResNone
    }

    pub fn set_default_launch_quantization(&mut self, _quantization: LaunchQuantization) {}

    pub fn next_launch_quantization(&mut self) {}

    pub fn is_fill_mode_active(&self) -> bool {
        false
    }

    pub fn set_fill_mode_active(&mut self, _active: bool) {}

    pub fn toggle_fill_mode_active(&mut self) {}

    // ----------------------------------------------------------------------------------------
    // Internal setters (parser)

    pub fn set_play_state(&mut self, is_playing: bool) {
        self.is_playing = is_playing;
        self.ctx.update_transport_snapshot(|s| s.is_playing = is_playing);
    }

    pub fn set_record_state(&mut self, is_recording: bool) {
        self.is_recording = is_recording;
        self.ctx.update_transport_snapshot(|s| s.is_recording = is_recording);
    }

    pub fn set_looping_state(&mut self, is_looping: bool) {
        self.is_looping = is_looping;
    }

    pub fn set_metronome_state(&mut self, on: bool) {
        self.is_metronome_on = on;
    }

    pub fn set_preroll_metronome_internal(&mut self, enable: bool) {
        self.preroll_metronome = enable;
    }

    pub fn set_preroll_measures_internal(&mut self, measures: i32) {
        self.preroll_measures = if (0..=4).contains(&measures) { measures } else { 2 };
    }

    pub fn set_internal_metronome_volume(&mut self, volume: f64) {
        self.metronome_volume.set_internal_metronome_volume(volume);
    }

    pub fn set_automation_write_mode_state(&mut self, mode: AutomationMode) {
        self.automation_mode = mode;
        self.ctx.set_automation_rec_active(self.is_writing_arranger_automation());
    }

    pub fn set_position_text(&mut self, text: &str) {
        self.position_str = reformat_time(text);
    }

    pub fn set_beats(&mut self, beats: &str) {
        self.beats_str = reformat_beats(beats, 1);
    }

    pub fn set_position_text_verbatim(&mut self, text: &str) {
        self.position_str = text.to_string();
    }

    pub fn set_beats_verbatim(&mut self, text: &str) {
        self.beats_str = text.to_string();
    }

    pub fn set_loop_start_beat_text_verbatim(&mut self, text: &str) {
        self.loop_start_beats_str = text.to_string();
    }

    pub fn set_loop_length_beat_text_verbatim(&mut self, text: &str) {
        self.loop_length_beats_str = text.to_string();
    }

    pub fn set_position_beats_value(&mut self, beats: f64) {
        self.position_beats = beats;
    }

    pub fn set_loop_start_beats_value(&mut self, beats: f64) {
        self.loop_start_beats = beats;
    }

    pub fn set_loop_length_beats_value(&mut self, beats: f64) {
        self.loop_length_beats = beats;
    }

    pub fn set_position_value(&mut self, time: f64) {
        self.position = time;
        self.ctx.update_transport_snapshot(|s| s.position = time);
    }

    pub fn set_loop_start_value(&mut self, time: f64) {
        self.loop_start = time;
    }

    pub fn set_loop_start_text(&mut self, time: &str) {
        self.loop_start_str = reformat_time(time);
    }

    pub fn set_loop_start_beat_text(&mut self, beats: &str) {
        self.loop_start_beats_str = reformat_beats(beats, 1);
    }

    pub fn set_loop_length_value(&mut self, time: f64) {
        self.loop_length = time;
    }

    pub fn set_loop_length_text(&mut self, time: &str) {
        self.loop_length_str = reformat_time(time);
    }

    pub fn set_loop_length_beat_text(&mut self, beats: &str) {
        self.loop_length_beats_str = reformat_beats(beats, 0);
    }

    pub fn set_numerator(&mut self, numerator: i32) {
        self.numerator = numerator;
    }

    pub fn set_denominator(&mut self, denominator: i32) {
        self.denominator = denominator;
    }

    pub fn set_h_zoom(&mut self, visible_pixels_per_second: f64) {
        self.visible_pixels_per_second = visible_pixels_per_second;
    }
}

fn reformat_beats(beats_str: &str, offset: i32) -> String {
    let pos = match beats_str.rfind('.') {
        Some(p) if p >= 1 => p,
        _ => return "0:00".into(),
    };
    let ticks: i32 = beats_str[pos + 1..].trim().parse().unwrap_or(0);
    let scaled = (ticks as f64 * 399.0 / 99.0).round() as i32;
    let sub_beats = scaled / 100;
    let ticks = scaled % 100;
    format!("{}.{}:{:02}", &beats_str[..pos], sub_beats + offset, ticks)
}

fn reformat_time(time_str: &str) -> String {
    // Mimic Java's String.split: an empty string gives [""], trailing empty strings are removed
    if time_str.is_empty() {
        return String::new();
    }
    let mut split: Vec<&str> = time_str.split('.').collect();
    while split.last() == Some(&"") {
        split.pop();
    }
    if split.is_empty() {
        return "0:000".into();
    }
    let replace = split[0].replace(':', ".");
    if split.len() == 1 {
        return replace;
    }
    format!("{}:{}", replace, split[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendCall;
    use crate::daw::bank::tests::test_ctx;

    #[test]
    fn formatting() {
        assert_eq!(reformat_beats("3.2.50", 1), "3.2.3:02");
        assert_eq!(reformat_time("1:23.456"), "1.23:456");
        assert_eq!(format_tempo(120.0), "120.00");
        assert_eq!(format_tempo(0.5), ".50");
    }

    #[test]
    fn moves_in_beats() {
        let (ctx, backend) = test_ctx();
        let mut t = Transport::new(ctx);
        t.set_position_beats_value(5.1);
        t.set_loop_start_beats_value(8.0);
        t.set_loop_length_beats_value(4.0);
        // Without a zoom level the resolution is one quarter, a measure has 4 quarters
        t.change_position(true, true);
        t.change_position(false, false);
        t.change_loop_start(true, false);
        t.change_loop_length(false, true);
        t.select_loop_end();
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::DoubleArg("beats".into(), "position/nosnap".into(), 6.0),
                BackendCall::DoubleArg("beats".into(), "position".into(), 0.0),
                BackendCall::DoubleArg("beats".into(), "loop/start".into(), 12.0),
                BackendCall::DoubleArg("beats".into(), "loop/length".into(), 3.0),
                BackendCall::DoubleArg("beats".into(), "position".into(), 15.0),
            ]
        );
    }

    #[test]
    fn commands() {
        let (ctx, backend) = test_ctx();
        let mut t = Transport::new(ctx.clone());
        t.set_play_state(true);
        t.set_record_state(true);
        t.stop();
        t.toggle_metronome();
        t.set_automation_write_mode(AutomationMode::Latch);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("record".into(), "".into()),
                BackendCall::NoArg("stop".into(), "".into()),
                BackendCall::NoArg("transport".into(), "toggleMetronome".into()),
                BackendCall::IntArg("automation".into(), "".into(), 4),
            ]
        );
        t.set_automation_write_mode_state(AutomationMode::Write);
        assert!(ctx.is_automation_rec_active());
        assert!(ctx.transport_snapshot().is_playing);
    }
}
