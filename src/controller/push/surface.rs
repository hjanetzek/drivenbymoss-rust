// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The surface of the Push 1, 2 and 3 (former `PushControlSurface`): the MIDI numbers of the
//! controls, the state of the controller in [`PushState`] (stored in `ctx.ext`) and the
//! functions which send the configuration of the device (ribbon, pad sensitivity, brightness,
//! MPE, Push 3 audio interface).
//!
//! Differences to Java:
//! - The MPE note input of the host is not switched (former `INoteInput.enableMPE` and
//!   `setMPEPitchBendSensitivity`): the core has no host function for it. The device is set to
//!   MPE and the pitch bend range is sent to the device as in Java.
//! - The pads stay bound to MIDI channel 1 (former `PushPadGrid.translateToController` returns
//!   channel -1 in MPE mode, which Java's pad grid sends as channel 1 as well).
//! - Fixed bug (UPSTREAM_CANDIDATES): Java's `sendPadSensitivityPush3` builds the message with
//!   the command byte 0x43 but sends only the curve; the port sends the command byte.

use crate::config::push::{self as config, LockState, PushVersion};
use crate::framework::ctx::Ctx;
use crate::framework::ids::Modes;
use crate::midi::{DeviceInquiry, MidiOutput};

pub const PUSH_BUTTON_TAP: i32 = 3;
pub const PUSH_BUTTON_METRONOME: i32 = 9;
pub const PUSH_SMALL_KNOB1: i32 = 14;
pub const PUSH_SMALL_KNOB1_PRESS: i32 = 15;
pub const PUSH_SMALL_KNOB2: i32 = 15;
pub const PUSH_BUTTON_ROW1_1: i32 = 20;
pub const PUSH_BUTTON_MASTER: i32 = 28;
pub const PUSH_BUTTON_STOP_CLIP: i32 = 29;
pub const PUSH_BUTTON_SETUP: i32 = 30;
pub const PUSH_BUTTON_LAYOUT: i32 = 31;
pub const PUSH_BUTTON_ADD: i32 = 32;
pub const PUSH_BUTTON_HOT_SWAP: i32 = 33;
pub const PUSH_BUTTON_SESSION_DISPLAY: i32 = 34;
pub const PUSH_BUTTON_CONVERT: i32 = 35;
pub const PUSH_BUTTON_SCENE1: i32 = 36;
pub const PUSH_BUTTON_SCENE8: i32 = 43;
pub const PUSH_BUTTON_LEFT: i32 = 44;
pub const PUSH_BUTTON_RIGHT: i32 = 45;
pub const PUSH_BUTTON_UP: i32 = 46;
pub const PUSH_BUTTON_DOWN: i32 = 47;
pub const PUSH_BUTTON_SELECT: i32 = 48;
pub const PUSH_BUTTON_SHIFT: i32 = 49;
pub const PUSH_BUTTON_NOTE: i32 = 50;
pub const PUSH_BUTTON_SESSION: i32 = 51;
pub const PUSH_BUTTON_ADD_EFFECT: i32 = 52;
pub const PUSH_BUTTON_ADD_TRACK: i32 = 53;
pub const PUSH_BUTTON_OCTAVE_DOWN: i32 = 54;
pub const PUSH_BUTTON_OCTAVE_UP: i32 = 55;
pub const PUSH_BUTTON_REPEAT: i32 = 56;
pub const PUSH_BUTTON_ACCENT: i32 = 57;
pub const PUSH_BUTTON_SCALES: i32 = 58;
pub const PUSH_BUTTON_USER_MODE: i32 = 59;
pub const PUSH_BUTTON_MUTE: i32 = 60;
pub const PUSH_BUTTON_SOLO: i32 = 61;
pub const PUSH_BUTTON_DEVICE_LEFT: i32 = 62;
pub const PUSH_BUTTON_DEVICE_RIGHT: i32 = 63;
pub const PUSH_FOOTSWITCH1: i32 = 64;
pub const PUSH_CAPTURE_MIDI: i32 = 65;
pub const PUSH_FOOTSWITCH2: i32 = 69;
pub const PUSH_KNOB_ENCODER: i32 = 70;
pub const PUSH_KNOB1: i32 = 71;
pub const PUSH_KNOB9: i32 = 79;
pub const PUSH_BUTTON_FILES: i32 = 80;
pub const PUSH_BUTTON_HELP: i32 = 81;
pub const PUSH_BUTTON_SAVE: i32 = 82;
pub const PUSH_BUTTON_LOCK: i32 = 83;
pub const PUSH_BUTTON_PLAY: i32 = 85;
pub const PUSH_BUTTON_RECORD: i32 = 86;
pub const PUSH_BUTTON_NEW: i32 = 87;
pub const PUSH_BUTTON_DUPLICATE: i32 = 88;
pub const PUSH_BUTTON_AUTOMATION: i32 = 89;
pub const PUSH_BUTTON_FIXED_LENGTH: i32 = 90;
pub const PUSH_BUTTON_CURSOR_CENTER: i32 = 91;
pub const PUSH_3_BUTTON_NEW: i32 = 92;
pub const PUSH_ENCODER_LEFT: i32 = 93;
pub const PUSH_BUTTON_ENCODER: i32 = 94;
pub const PUSH_ENCODER_RIGHT: i32 = 95;
pub const PUSH_BUTTON_ROW2_1: i32 = 102;
pub const PUSH_BUTTON_DEVICE: i32 = 110;
pub const PUSH_BUTTON_BROWSE: i32 = 111;
pub const PUSH_BUTTON_TOGGLE_MASTER_CUE_VOLUME: i32 = 111;
pub const PUSH_BUTTON_TRACK: i32 = 112;
pub const PUSH_BUTTON_CLIP: i32 = 113;
pub const PUSH_BUTTON_VOLUME: i32 = 114;
pub const PUSH_BUTTON_PAN_SEND: i32 = 115;
pub const PUSH_BUTTON_QUANTIZE: i32 = 116;
pub const PUSH_BUTTON_DOUBLE: i32 = 117;
pub const PUSH_BUTTON_DELETE: i32 = 118;
pub const PUSH_BUTTON_UNDO: i32 = 119;

pub const PUSH_KNOB1_TOUCH: i32 = 0;
pub const PUSH_KNOB9_TOUCH: i32 = 8;
pub const PUSH_SMALL_KNOB1_TOUCH: i32 = 10;
pub const PUSH_SMALL_KNOB2_TOUCH: i32 = 9;
pub const PUSH_RIBBON_TOUCH: i32 = 12;

pub const PUSH_RIBBON_PITCHBEND: i32 = 0;
pub const PUSH_RIBBON_VOLUME: i32 = 1;
pub const PUSH_RIBBON_PAN: i32 = 2;
pub const PUSH_RIBBON_DISCRETE: i32 = 3;

/// The first note of the pad grid (lower left pad).
pub const PAD_START_NOTE: i32 = 36;

const CHROMATIC_SCALE: [i32; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

const PUSH_PAD_CURVES_DATA: [&str; 6] = [
    "00 00 00 01 08 06 0A 00 00 00 00 00 0A 0F 0C 08 00 00 00 00 00 00 00 00",
    "00 00 00 01 04 0C 00 08 00 00 00 01 0D 04 0C 00 00 00 00 00 0E 0A 06 00",
    "00 00 00 01 04 0C 00 08 00 00 00 01 0D 04 0C 00 00 00 00 00 0C 03 05 00",
    "00 00 00 01 08 06 0A 00 00 00 00 01 0D 04 0C 00 00 00 00 00 0C 03 05 00",
    "00 00 00 01 0F 0B 0D 00 00 00 00 01 0D 04 0C 00 00 00 00 00 0C 03 05 00",
    "00 00 00 02 02 02 0E 00 00 00 00 01 0D 04 0C 00 00 00 00 00 00 00 00 00",
];

/// 4 Byte: peak_sampling_time, 4 Byte: aftertouch_gate_time
const PUSH_PAD_THRESHOLDS_DATA: [&str; 41] = [
    "00 00 00 0A 00 00 00 0A",
    "00 00 01 03 00 00 01 04",
    "00 00 01 0C 00 00 01 0E",
    "00 00 02 05 00 00 02 08",
    "00 00 02 0E 00 00 03 02",
    "00 00 03 07 00 00 03 0C",
    "00 00 04 00 00 00 04 06",
    "00 00 04 09 00 00 05 00",
    "00 00 05 02 00 00 05 0A",
    "00 00 05 0B 00 00 06 04",
    "00 00 06 04 00 00 06 0E",
    "00 00 06 0D 00 00 07 08",
    "00 00 07 06 00 00 08 02",
    "00 00 07 0F 00 00 08 0C",
    "00 00 08 08 00 00 09 06",
    "00 00 09 01 00 00 0A 00",
    "00 00 09 0A 00 00 0A 0A",
    "00 00 0A 03 00 00 0B 04",
    "00 00 0A 0C 00 00 0B 0E",
    "00 00 0B 05 00 00 0C 08",
    "00 00 0B 0E 00 00 0D 02",
    "00 00 0C 07 00 00 0D 0C",
    "00 00 0D 00 00 00 0E 06",
    "00 00 0D 08 00 00 0E 0F",
    "00 00 0E 02 00 00 0F 0A",
    "00 00 0E 0B 00 01 00 04",
    "00 00 0F 04 00 01 00 0E",
    "00 00 0F 0D 00 01 01 08",
    "00 01 00 06 00 01 02 02",
    "00 01 00 0F 00 01 02 0C",
    "00 01 01 08 00 01 03 06",
    "00 01 02 01 00 01 04 00",
    "00 01 02 0A 00 01 04 0A",
    "00 01 03 03 00 01 05 04",
    "00 01 03 0C 00 01 05 0E",
    "00 01 04 05 00 01 06 08",
    "00 01 04 0E 00 01 07 02",
    "00 01 05 07 00 01 07 0C",
    "00 01 06 00 00 01 08 06",
    "00 01 06 09 00 01 09 00",
    "00 01 07 02 00 01 09 0A",
];

const MAXW: [i32; 11] = [1700, 1660, 1590, 1510, 1420, 1300, 1170, 1030, 860, 640, 400];
const PUSH2_CPMIN: [i32; 11] = [1650, 1580, 1500, 1410, 1320, 1220, 1110, 1000, 900, 800, 700];
const PUSH2_CPMAX: [i32; 11] = [2050, 1950, 1850, 1750, 1650, 1570, 1490, 1400, 1320, 1240, 1180];
const GAMMA: [f64; 11] = [0.7, 0.64, 0.58, 0.54, 0.5, 0.46, 0.43, 0.4, 0.36, 0.32, 0.25];
const MINV: [i32; 11] = [1, 1, 1, 1, 1, 1, 3, 6, 12, 24, 36];
const MAXV: [i32; 11] = [96, 102, 116, 121, 124, 127, 127, 127, 127, 127, 127];
const ALPHA: [i32; 11] = [90, 70, 54, 40, 28, 20, 10, -5, -25, -55, -90];

/// 0mm, 1mm, 2mm, 2.5mm, 3mm, 4mm, 5mm, 6mm, 7mm, 10mm, 13mm, 20mm
const TUNE_WIDTH_VALUES: [i32; 12] = [0x00, 0x04, 0x08, 0x0C, 0x10, 0x14, 0x18, 0x1C, 0x20, 0x30, 0x40, 0x60];
/// 16mm, 15mm, 14mm, 13mm, 12mm, 11mm, 10mm
const SLIDE_HEIGHT_VALUES: [i32; 7] = [0x13, 0x19, 0x20, 0x27, 0x2D, 0x34, 0x3B];

/// The sysex header of the Push 1.
pub const SYSEX_HEADER_PUSH1: [u8; 4] = [0xF0, 0x47, 0x7F, 0x15];
/// The sysex header of the Push 2 and 3.
pub const SYSEX_HEADER: [u8; 6] = [0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01];
const SYSEX_ZERO_PADDING: [u8; 13] = [0; 13];

const PAD_VELOCITY_CURVE_CHUNK_SIZE: usize = 16;
const NUM_VELOCITY_CURVE_ENTRIES: usize = 128;
const MIN_OUT: i32 = 1;
const MAX_OUT: i32 = 127;

/// The MIDI status of MPE zone 1 (former `AbstractMidiOutput.ZONE_1`).
const ZONE_1: i32 = 0xB0;

/// The notes and slide values of the MPE channels (former `MPEStatus`).
#[derive(Debug, Clone)]
pub struct MpeStatus {
    notes: [i32; 16],
    slide_value: [i32; 16],
}

impl Default for MpeStatus {
    fn default() -> Self {
        MpeStatus { notes: [-1; 16], slide_value: [0; 16] }
    }
}

impl MpeStatus {
    pub fn handle_note(&mut self, channel: usize, note: i32, is_on: bool) {
        self.notes[channel & 15] = if is_on { note } else { -1 };
    }

    pub fn note_status(&self, channel: usize) -> i32 {
        self.notes[channel & 15]
    }

    pub fn slide_value(&self, channel: usize) -> i32 {
        self.slide_value[channel & 15]
    }

    pub fn set_slide_value(&mut self, channel: usize, value: i32) {
        self.slide_value[channel & 15] = value;
    }
}

/// The controller state of the Push (the fields of the former `PushControlSurface` and the
/// non-setting state of `PushConfiguration`), stored in `ctx.ext`.
pub struct PushState {
    pub version: PushVersion,
    ribbon_mode: i32,
    ribbon_value: i32,
    pub major_version: i32,
    pub minor_version: i32,
    pub build_number: i32,
    pub serial_number: i32,
    pub board_revision: i32,
    current_pad_push2: Option<(i32, i32, i32)>,
    current_pad_push3: Option<(i32, i32, i32, i32)>,
    current_curve: Vec<i32>,
    pub mpe_status: MpeStatus,
    /// Former `PushConfiguration.lockState`.
    pub lock_state: LockState,
    /// Former `PushConfiguration.layerMode`.
    pub layer_mode: Option<Modes>,
    /// The state of the color palette of the Push 2/3 (former `ColorPalette`).
    pub palette: super::palette::ColorPalette,
    /// The frame state of the USB display of the Push 2/3.
    pub usb_display: super::usb_display::UsbDisplay,
    /// The last encoder state sent to the simulator ([`super::sim_values`]).
    pub sim_values: super::sim_values::SimValues,
    /// The controller specific state of the modes and views, which other parts need.
    pub extra: PushExtra,
}

/// Further state shared between the modes, views and commands.
#[derive(Default)]
pub struct PushExtra {
    /// True if the cursor track was selected with the master button (former
    /// `MastertrackCommand` state).
    pub master_temp: bool,
}

impl PushState {
    pub fn new(version: PushVersion) -> Self {
        PushState {
            version,
            ribbon_mode: -1,
            ribbon_value: -1,
            major_version: -1,
            minor_version: -1,
            build_number: -1,
            serial_number: -1,
            board_revision: -1,
            current_pad_push2: None,
            current_pad_push3: None,
            current_curve: Vec::new(),
            mpe_status: MpeStatus::default(),
            lock_state: LockState::Off,
            layer_mode: None,
            palette: super::palette::ColorPalette::new(),
            usb_display: super::usb_display::UsbDisplay::new(),
            sim_values: super::sim_values::SimValues::new(),
            extra: PushExtra::default(),
        }
    }
}

/// The Push state of a context.
pub fn state(ctx: &Ctx) -> &PushState {
    ctx.ext::<PushState>()
}

/// The Push state of a context.
pub fn state_mut(ctx: &mut Ctx) -> &mut PushState {
    ctx.ext_mut::<PushState>()
}

/// The model of the controller.
pub fn version(ctx: &Ctx) -> PushVersion {
    state(ctx).version
}

/// True for the Push 2 and 3 (former `PushConfiguration.isPushModern`).
pub fn is_modern(ctx: &Ctx) -> bool {
    version(ctx).is_modern()
}

/// True for the Push 1.
pub fn is_push1(ctx: &Ctx) -> bool {
    version(ctx) == PushVersion::Version1
}

/// Former `PushConfiguration.getLockState`.
pub fn lock_state(ctx: &Ctx) -> LockState {
    state(ctx).lock_state
}

/// Former `PushConfiguration.setLockState`.
pub fn set_lock_state(ctx: &mut Ctx, lock_state: LockState) {
    state_mut(ctx).lock_state = lock_state;
}

/// Former `PushConfiguration.isSoloState`.
pub fn is_solo_state(ctx: &Ctx, is_solo_long_pressed: bool) -> bool {
    is_solo_long_pressed || lock_state(ctx) == LockState::Solo
}

/// Former `PushConfiguration.isMuteState`.
pub fn is_mute_state(ctx: &Ctx, is_mute_long_pressed: bool) -> bool {
    is_mute_long_pressed || lock_state(ctx) == LockState::Mute
}

/// Former `PushConfiguration.isClipStopState`.
pub fn is_clip_stop_state(ctx: &Ctx, is_clip_stop_long_pressed: bool) -> bool {
    is_clip_stop_long_pressed || lock_state(ctx) == LockState::ClipStop
}

/// Former `PushConfiguration.setLayerMixMode`.
pub fn set_layer_mix_mode(ctx: &mut Ctx, mode: Option<Modes>) {
    state_mut(ctx).layer_mode = mode;
}

/// Former `PushConfiguration.getCurrentLayerMixMode`.
pub fn current_layer_mix_mode(ctx: &mut Ctx) -> Modes {
    if let Some(mode) = state(ctx).layer_mode {
        return mode;
    }
    let mode = config::layer_mode_for_mix_mode(&ctx.config, is_modern(ctx));
    state_mut(ctx).layer_mode = Some(mode);
    mode
}

// ----------------------------------------------------------------------------------------------
// Sysex

/// Send a sysex message to the Push 2/3 (former `sendSysex (int [])`).
pub fn send_sysex(output: &MidiOutput, parameters: &[u8]) {
    let mut data = Vec::with_capacity(SYSEX_HEADER.len() + parameters.len() + 1);
    data.extend_from_slice(&SYSEX_HEADER);
    data.extend(parameters.iter().map(|b| b & 0x7F));
    data.push(0xF7);
    output.send_sysex(&data);
}

/// Send a sysex message to the Push 1 (former `sendSysExPush1`).
pub fn send_sysex_push1(output: &MidiOutput, parameters: &[u8]) {
    let mut data = Vec::with_capacity(SYSEX_HEADER_PUSH1.len() + parameters.len() + 1);
    data.extend_from_slice(&SYSEX_HEADER_PUSH1);
    data.extend_from_slice(parameters);
    data.push(0xF7);
    output.send_sysex(&data);
}

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace().filter_map(|b| u8::from_str_radix(b, 16).ok()).collect()
}

fn send_sysex_padded(output: &MidiOutput, command: u8, value: i32) {
    let mut data = vec![0x37, command, value.clamp(0, 127) as u8];
    data.extend_from_slice(&SYSEX_ZERO_PADDING);
    send_sysex(output, &data);
}

// ----------------------------------------------------------------------------------------------
// Ribbon

/// Former `setRibbonMode`.
pub fn set_ribbon_mode(ctx: &mut Ctx, mode: i32) {
    let state = state_mut(ctx);
    if state.ribbon_mode == mode {
        return;
    }
    state.ribbon_mode = mode;
    if state.version.is_modern() {
        // See section 2.10.1 in Push 2 programmer manual for status codes
        let status = match mode {
            PUSH_RIBBON_PITCHBEND => 122,
            PUSH_RIBBON_VOLUME => 1,
            PUSH_RIBBON_PAN => 17,
            PUSH_RIBBON_DISCRETE => 9,
            _ => 0,
        };
        send_sysex(&ctx.surface.output, &[23, status]);
    } else {
        send_sysex_push1(&ctx.surface.output, &[0x63, 0x00, 0x01, mode as u8]);
    }
}

/// The current ribbon mode (former field `ribbonMode`).
pub fn ribbon_mode(ctx: &Ctx) -> i32 {
    state(ctx).ribbon_mode
}

/// Former `setRibbonValue`.
pub fn set_ribbon_value(ctx: &mut Ctx, value: i32) {
    let state = state_mut(ctx);
    if state.ribbon_value == value {
        return;
    }
    state.ribbon_value = value;
    ctx.surface.output.send_pitchbend(0, 0, value);
}

// ----------------------------------------------------------------------------------------------
// Pad sensitivity

/// Former `sendPadSensitivityPush1`.
pub fn send_pad_sensitivity_push1(ctx: &mut Ctx) {
    let threshold = config::pad_threshold_push1(&ctx.config).min(PUSH_PAD_THRESHOLDS_DATA.len() - 1);
    let curve = config::velocity_curve(&ctx.config).min(PUSH_PAD_CURVES_DATA.len() - 1);
    let mut data = vec![0x5D, 0x00, 0x20];
    data.extend(hex(PUSH_PAD_THRESHOLDS_DATA[threshold]));
    data.extend(hex(PUSH_PAD_CURVES_DATA[curve]));
    send_sysex_push1(&ctx.surface.output, &data);
}

/// Former `sendPadSensitivityPush2`.
pub fn send_pad_sensitivity_push2(ctx: &mut Ctx) {
    send_pad_velocity_curve_push2(ctx);
    send_pad_threshold_push2(ctx);
}

/// Former `sendPadSensitivityPush3`: the command 0x43 followed by the 128 curve values (Java
/// sends the curve without the command byte, a bug).
pub fn send_pad_sensitivity_push3(ctx: &mut Ctx) {
    let curve = create_pad_sensitivity_curve_push3(ctx);
    let mut data = vec![0x43u8];
    data.extend(curve.iter().map(|v| *v as u8));
    send_sysex(&ctx.surface.output, &data);
}

/// Former `createPadSensitivityCurvePush3`.
pub fn create_pad_sensitivity_curve_push3(ctx: &mut Ctx) -> Vec<i32> {
    let threshold = config::pad_curve_threshold_push3(&ctx.config);
    let drive = config::pad_curve_drive_push3(&ctx.config);
    let compand = config::pad_curve_compand_push3(&ctx.config);
    let range = config::pad_curve_range_push3(&ctx.config);
    let state = state_mut(ctx);
    let key = (threshold, drive, compand, range);
    if state.current_pad_push3 == Some(key) {
        return state.current_curve.clone();
    }
    state.current_pad_push3 = Some(key);
    state.current_pad_push2 = None;
    let curve = calc_pad_sensitivity_curve_push3(threshold, drive, compand, range);
    state.current_curve = curve.clone();
    curve
}

/// The calculation of `createPadSensitivityCurvePush3`.
pub fn calc_pad_sensitivity_curve_push3(threshold: i32, drive: i32, compand: i32, range: i32) -> Vec<i32> {
    // Scale threshold to a maximum of 16 values
    let num_threshold_values = (threshold.clamp(0, 100) as f64 * 16.0 / 100.0).round() as i32;
    // Scale range to a maximum of 103 values
    let num_range_values = (range.clamp(0, 100) as f64 * 103.0 / 100.0).round() as i32;
    // Calculate 2 more curve values since the first is 1 and the last 127
    let num_curve_values = 128 - num_threshold_values - num_range_values + 2;

    let mut curve = vec![0; 128];
    for value in curve.iter_mut().take(num_threshold_values.max(0) as usize) {
        *value = 1;
    }

    // Compand: g>0.5 = slow attack, g<0.5 = fast attack
    let g = (0.5 + compand as f64 / 100.0).clamp(0.01, 0.99);
    // Drive: b>0.5 = faster early increase, b<0.5 = slower early increase
    let b = (0.5 + drive as f64 / 100.0).clamp(0.01, 0.99);
    for i in 0..num_curve_values {
        let x = i as f64 / (num_curve_values - 1) as f64;
        let y = bias(gain(x, g), b);
        let v = ((MIN_OUT as f64 + y * (MAX_OUT - MIN_OUT) as f64).round() as i32).clamp(MIN_OUT, MAX_OUT);
        if num_threshold_values + i <= 128 {
            curve[(num_threshold_values + i - 1).max(0) as usize] = v;
        }
    }

    // Fill the range part
    let offset = (128 - num_range_values).max(0) as usize;
    for value in curve.iter_mut().skip(offset) {
        *value = 127;
    }
    curve
}

/// Schlick bias: b in (0,1); 0.5 ≈ linear; >0.5 faster early rise; <0.5 slower early rise
fn bias(x: f64, b: f64) -> f64 {
    x / ((1.0 / b - 2.0) * (1.0 - x) + 1.0)
}

/// Schlick gain: g in (0,1); 0.5 = linear; g>0.5 slow attack; g<0.5 fast attack
fn gain(x: f64, g: f64) -> f64 {
    if x < 0.5 {
        0.5 * bias(2.0 * x, 1.0 - g)
    } else {
        1.0 - 0.5 * bias(2.0 - 2.0 * x, 1.0 - g)
    }
}

/// Former `sendPressureMode`.
pub fn send_pressure_mode(ctx: &mut Ctx, is_poly_pressure: bool) {
    if is_modern(ctx) {
        send_sysex(&ctx.surface.output, &[0x1E, u8::from(is_poly_pressure)]);
    } else {
        send_sysex_push1(&ctx.surface.output, &[0x5C, 0x00, 0x01, u8::from(!is_poly_pressure)]);
    }
}

fn add_7l5m(array: &mut [u8], index: usize, value: i32) {
    array[index] = (value & 127) as u8;
    array[index + 1] = ((value >> 7) & 31) as u8;
}

fn send_pad_threshold_push2(ctx: &mut Ctx) {
    let mut args = [0u8; 9];
    args[0] = 27;
    add_7l5m(&mut args, 1, 33); // threshold0
    add_7l5m(&mut args, 3, 31); // threshold1
    let pad_sensitivity = config::pad_sensitivity_push2(&ctx.config).clamp(0, 10) as usize;
    add_7l5m(&mut args, 5, PUSH2_CPMIN[pad_sensitivity]); // cpmin
    add_7l5m(&mut args, 7, PUSH2_CPMAX[pad_sensitivity]); // cpmax
    send_sysex(&ctx.surface.output, &args);
}

fn send_pad_velocity_curve_push2(ctx: &mut Ctx) {
    let velocities = create_pad_sensitivity_curve_push2(ctx);
    for index in (0..velocities.len()).step_by(PAD_VELOCITY_CURVE_CHUNK_SIZE) {
        let mut args = vec![32u8, index as u8];
        args.extend(velocities[index..index + PAD_VELOCITY_CURVE_CHUNK_SIZE].iter().map(|v| *v as u8));
        send_sysex(&ctx.surface.output, &args);
    }
}

/// Former `sendDisplayBrightness`.
pub fn send_display_brightness(ctx: &mut Ctx) {
    let brightness = config::display_brightness(&ctx.config) * 255 / 100;
    send_sysex(&ctx.surface.output, &[8, (brightness & 127) as u8, ((brightness >> 7) & 1) as u8]);
}

/// Former `sendLEDBrightness`.
pub fn send_led_brightness(ctx: &mut Ctx) {
    let brightness = config::led_brightness(&ctx.config) * 127 / 100;
    send_sysex(&ctx.surface.output, &[6, brightness as u8]);
}

/// Former `sendMPEActive`.
pub fn send_mpe_active(ctx: &mut Ctx, enable: bool) {
    // Same command as sendPressureMode
    send_sysex(&ctx.surface.output, &[0x1E, if enable { 2 } else { 1 }]);
}

/// Former `sendPerPadPitchbendActive`.
pub fn send_per_pad_pitchbend_active(ctx: &mut Ctx, enable: bool) {
    send_sysex(&ctx.surface.output, &[0x26, 0x07, 0x08, if enable { 2 } else { 0 }, 0x00]);
}

/// Former `sendInTuneLocation`.
pub fn send_in_tune_location(ctx: &mut Ctx, in_tune_location: usize) {
    send_sysex(&ctx.surface.output, &[0x26, 0x07, 0x0E, in_tune_location as u8, 0x00]);
}

/// Former `sendInTuneWidth`.
pub fn send_in_tune_width(ctx: &mut Ctx, index: usize) {
    send_sysex(&ctx.surface.output, &[0x26, 0x07, 0x14, TUNE_WIDTH_VALUES[index.min(TUNE_WIDTH_VALUES.len() - 1)] as u8, 0x00]);
}

/// Former `sendSlideHeight`.
pub fn send_slide_height(ctx: &mut Ctx, index: usize) {
    send_sysex(&ctx.surface.output, &[0x26, 0x07, 0x24, SLIDE_HEIGHT_VALUES[index.min(SLIDE_HEIGHT_VALUES.len() - 1)] as u8, 0x00]);
}

/// Former `sendPedals`.
pub fn send_pedals(ctx: &mut Ctx) {
    let use_cv1 = config::pedal1(&ctx.config) > 0;
    let use_cv2 = config::pedal2(&ctx.config) > 0;
    let value = match (use_cv1, use_cv2) {
        (true, true) => 0x0F,
        (true, false) => 0x43,
        (false, true) => 0x1C,
        (false, false) => 0x50,
    };
    send_sysex_padded(&ctx.surface.output, 0x26, value);
}

/// Former `sendPreamp1Type` (0 = Line, 1 = Instrument, 2 = High).
pub fn send_preamp1_type(ctx: &mut Ctx) {
    send_sysex_padded(&ctx.surface.output, 0x1A, config::preamp1_type(&ctx.config) as i32);
}

/// Former `sendPreamp2Type`.
pub fn send_preamp2_type(ctx: &mut Ctx) {
    send_sysex_padded(&ctx.surface.output, 0x1B, config::preamp2_type(&ctx.config) as i32);
}

/// The gain in steps of two (1dB = 2) in the range of 0x00 (20dB) to 0x28 (no gain).
fn preamp_gain_value(gain: usize) -> i32 {
    (config::PREAMP_GAIN_OPTIONS.len() as i32 - 1 - gain as i32) * 2
}

/// Former `sendPreamp1Gain`.
pub fn send_preamp1_gain(ctx: &mut Ctx) {
    send_sysex_padded(&ctx.surface.output, 0x02, preamp_gain_value(config::preamp1_gain(&ctx.config)));
}

/// Former `sendPreamp2Gain`.
pub fn send_preamp2_gain(ctx: &mut Ctx) {
    send_sysex_padded(&ctx.surface.output, 0x03, preamp_gain_value(config::preamp2_gain(&ctx.config)));
}

/// Former `sendOutputConfiguration`: 0 = Headphones 1/2 - Speaker 1/2, 1 = Headphones 3/4 -
/// Speaker 1/2, 2 = Headphones 1/2 - Speaker 3/4.
pub fn send_output_configuration(ctx: &mut Ctx) {
    let audio_outputs = config::audio_outputs(&ctx.config) as i32;
    let value = if audio_outputs == 0 { 0 } else { audio_outputs + 1 };
    send_sysex_padded(&ctx.surface.output, 0x11, value);
}

/// Former `createPadSensitivityCurvePush2`.
pub fn create_pad_sensitivity_curve_push2(ctx: &mut Ctx) -> Vec<i32> {
    let sensitivity = config::pad_sensitivity_push2(&ctx.config).clamp(0, 10);
    let gain = config::pad_gain_push2(&ctx.config).clamp(0, 10);
    let dynamics = config::pad_dynamics_push2(&ctx.config).clamp(0, 10);
    let state = state_mut(ctx);
    let key = (sensitivity, gain, dynamics);
    if state.current_pad_push2 == Some(key) {
        return state.current_curve.clone();
    }
    state.current_pad_push2 = Some(key);
    state.current_pad_push3 = None;
    let curve = calc_pad_sensitivity_curve_push2(sensitivity as usize, gain as usize, dynamics as usize);
    state.current_curve = curve.clone();
    curve
}

/// The calculation of `createPadSensitivityCurvePush2`.
pub fn calc_pad_sensitivity_curve_push2(sensitivity: usize, gain: usize, dynamics: usize) -> Vec<i32> {
    let minw = 160;
    let maxw = MAXW[sensitivity];
    let minv = MINV[gain];
    let maxv = MAXV[gain];
    let (p1x, p1y, p2x, p2y) = calculate_points_push2(ALPHA[dynamics] as f64);

    let mut curve = vec![0; NUM_VELOCITY_CURVE_ENTRIES];
    let minw_index = minw / 32;
    let maxw_index = maxw / 32;
    let mut t = 0.0;
    for (index, entry) in curve.iter_mut().enumerate() {
        let index = index as i32;
        let w = index as f64 * 32.0;
        let velocity = if w <= minw as f64 {
            1.0 + (minv as f64 - 1.0) * index as f64 / minw_index as f64
        } else if w >= maxw as f64 {
            maxv as f64 + (127.0 - maxv as f64) * (index - maxw_index) as f64 / (128 - maxw_index) as f64
        } else {
            let wnorm = (w - minw as f64) / (maxw - minw) as f64;
            let (b, new_t) = bezier_push2(wnorm, t, p1x, p1y, p2x, p2y);
            t = new_t;
            let velonorm = gamma_func(b, GAMMA[gain]);
            minv as f64 + velonorm * (maxv - minv) as f64
        };
        // Java Math.round rounds half up
        *entry = ((velocity + 0.5).floor() as i64).clamp(1, 127) as i32;
    }
    curve
}

fn bezier_push2(x: f64, t: f64, p1x: f64, p1y: f64, p2x: f64, p2y: f64) -> (f64, f64) {
    let (p0x, p0y, p3x, p3y) = (0.0, 0.0, 1.0, 1.0);
    let mut tl = t;
    while tl <= 1.0 {
        let s = 1.0 - tl;
        let t2 = tl * tl;
        let t3 = t2 * tl;
        let s2 = s * s;
        let s3 = s2 * s;
        let xt = s3 * p0x + 3.0 * tl * s2 * p1x + 3.0 * t2 * s * p2x + t3 * p3x;
        if xt >= x {
            return (s3 * p0y + 3.0 * tl * s2 * p1y + 3.0 * t2 * s * p2y + t3 * p3y, tl);
        }
        tl += 0.0001;
    }
    (1.0, tl)
}

fn calculate_points_push2(alpha: f64) -> (f64, f64, f64, f64) {
    let a1 = (225.0 - alpha) * std::f64::consts::PI / 180.0;
    let a2 = (45.0 - alpha) * std::f64::consts::PI / 180.0;
    let r = 0.4;
    (0.5 + r * a1.cos(), 0.5 + r * a1.sin(), 0.5 + r * a2.cos(), 0.5 + r * a2.sin())
}

fn gamma_func(x: f64, gamma: f64) -> f64 {
    x.powf((-4.0 + 8.0 * gamma).exp())
}

// ----------------------------------------------------------------------------------------------
// Incoming MIDI

/// True if the data starts with the Push 2/3 sysex header and ends with F7 (former
/// `isPush2Data`).
pub fn is_push2_data(data: &[u8]) -> bool {
    data.len() >= SYSEX_HEADER.len() && data.starts_with(&SYSEX_HEADER) && data.last() == Some(&0xF7)
}

/// Handle a sysex message from the Push (former `handleSysEx`).
pub fn handle_sysex(ctx: &mut Ctx, hex_data: &str) {
    let data = crate::midi::from_hex_str(hex_data);
    let inquiry = DeviceInquiry::new(&data);
    if inquiry.is_valid() {
        handle_device_inquiry_response(ctx, &data);
        return;
    }
    // Color palette entry message?
    if is_modern(ctx) && is_push2_data(&data) {
        state_mut(ctx).palette.handle_color_palette_message(&data);
        for error in state_mut(ctx).palette.take_errors() {
            ctx.host.error(&error);
        }
    }
}

/// Former `handleDeviceInquiryResponse`.
fn handle_device_inquiry_response(ctx: &mut Ctx, data: &[u8]) {
    let is_modern = is_modern(ctx);
    let state = state_mut(ctx);
    if is_modern {
        // Former getUnspecifiedData: everything after the software revision (offset 5 + 4 +
        // 4 of the long response) until F7
        let start = if data.len() == 15 { 13 } else { 15 }.min(data.len().saturating_sub(1));
        let unspecified = &data[start..data.len() - 1];
        if unspecified.len() < 10 {
            return;
        }
        let u = |i: usize| unspecified[i] as i32;
        state.major_version = u(0);
        state.minor_version = u(1);
        state.build_number = u(2) + (u(3) << 7);
        state.serial_number = u(4) + (u(5) << 7) + (u(6) << 14) + (u(7) << 21) + (u(8) << 28);
        state.board_revision = u(9);
    } else {
        // Former getData: the whole message
        if data.len() != 35 {
            return;
        }
        let d = |i: usize| data[i] as i32;
        state.major_version = d(10) + d(9) * 10;
        state.minor_version = d(12) + d(11) * 10;
        state.build_number = 0;
        state.serial_number = 0;
        state.board_revision = 0;
    }
}

/// True if the active view is an expression view and MPE is enabled (former `isMPEEnabled`).
pub fn is_mpe_enabled(ctx: &Ctx) -> bool {
    ctx.config.is_mpe_enabled() && ctx.active_view().is_some_and(|view| view.try_borrow().is_ok_and(|v| v.is_expression_view()))
}

fn is_expression_view_active(ctx: &Ctx) -> bool {
    ctx.active_view().is_some_and(|view| view.try_borrow().is_ok_and(|v| v.is_expression_view()))
}

/// The MIDI handler of the Push (former override of `handleMidi`) for the messages which are not
/// bound to a control: the Push 3 MPE notes and pitch bend.
pub fn handle_midi(ctx: &mut Ctx, status: i32, data1: i32, data2: i32) {
    // Ignore active sensing, which seems to be sent from some Push devices
    if status == 254 {
        return;
    }
    let code = status & 0xF0;
    let channel = (status & 0xF) as usize;
    // Only the Push 3 sends MPE; Java ignores 'All MIDI Note off' (CC 123 on channel 1) on
    // startup and the MPE brightness (CC 74), which are not bound to a control either
    if version(ctx) != PushVersion::Version3 || channel == 0 {
        return;
    }
    match code {
        0x90 | 0x80 => {
            if ctx.config.is_mpe_enabled() && is_expression_view_active(ctx) {
                let is_note_on = code == 0x90 && data2 > 0;
                // Reset pitch-bend
                if !is_note_on {
                    ctx.surface.input.send_raw_midi_event(0xE0 + channel as i32, 0, 64);
                }
                let note = ctx.surface.key_translation_table().get(data1 as usize).copied().unwrap_or(-1);
                state_mut(ctx).mpe_status.handle_note(channel, note, is_note_on);
            }
        }
        0xE0 => handle_mpe_pitchbend(ctx, channel, data1, data2),
        _ => {}
    }
}

/// Former `handleMPEPitchbend`.
pub fn handle_mpe_pitchbend(ctx: &mut Ctx, channel: usize, data1: i32, data2: i32) {
    if !ctx.config.is_mpe_enabled() {
        ctx.surface.input.send_raw_midi_event(0xE0 + channel as i32, data1, data2);
        return;
    }
    if !is_expression_view_active(ctx) {
        return;
    }
    let note = state(ctx).mpe_status.note_status(channel);
    if note == -1 {
        return;
    }
    let scales = ctx.model.scales();
    let intervals: Vec<i32> = if scales.is_chromatic() { CHROMATIC_SCALE.to_vec() } else { scales.scale().intervals().to_vec() };
    let scale_offset = scales.scale_offset();

    // Range of -1279 to 1279
    let value = calculate_bend_value(ctx, channel, data1, data2);
    if let Some((lsb, msb)) = calc_mpe_pitchbend(&intervals, scale_offset, note, value) {
        // TODO Bitwig currently ignores the MIDI channel for MPE!
        ctx.surface.input.send_raw_midi_event(0xE0 + channel as i32, lsb, msb);
    }
}

/// The pitch bend for a slide of `value` from the note in the scale (part of former
/// `handleMPEPitchbend`), as LSB and MSB.
pub fn calc_mpe_pitchbend(intervals: &[i32], scale_offset: i32, note: i32, value: f64) -> Option<(i32, i32)> {
    if intervals.is_empty() {
        return None;
    }
    let len = intervals.len() as i32;
    // Scale to 0..7
    let pad_index = (value / 170.6).round() as i32;
    let note_index = calc_note_index(intervals, note, scale_offset);
    // Work in a single linear index, then wrap with floorDiv/floorMod
    let linear = note_index + pad_index;
    let deg = linear.rem_euclid(len);
    let octave = linear.div_euclid(len);
    // Semi-tone offset to target degree
    let semitones = 12 * octave + intervals[deg as usize] - intervals[note_index as usize];
    // Residual within the current pad, in the current scale
    let residual = value - pad_index as f64 * 170.6;
    // Next/previous degree for cents interpolation
    let linear_next = linear + if residual >= 0.0 { 1 } else { -1 };
    let next_deg = linear_next.rem_euclid(len);
    let next_oct = linear_next.div_euclid(len);
    let next_prev_semitones = 12 * next_oct + intervals[next_deg as usize] - intervals[note_index as usize];
    let diff = (next_prev_semitones - semitones) as f64 * 100.0 / 2.0;
    // 'diff' already carries the sign of direction; therefore ignore residual sign
    let cents = (residual.abs() / 85.3 * diff).round() as i32;
    // Convert to pitch-bend
    let bend_range = 24.0;
    let pitch_semis = semitones as f64 + cents as f64 / 100.0;
    let f = (pitch_semis / bend_range).clamp(-1.0, 1.0);
    let delta = (8191.0 * f).round() as i64;
    let value14 = (8192 + delta).clamp(0, 16383) as i32;
    Some((value14 & 0x7F, (value14 >> 7) & 0x7F))
}

fn calculate_bend_value(ctx: &mut Ctx, channel: usize, data1: i32, data2: i32) -> f64 {
    // Range of -10 to 10
    let bend_index = data2 - 64;
    let value = bend_index as f64 * 128.0 + if data1 >= 64 { (data1 - 127) as f64 } else { data1 as f64 };
    let state = state_mut(ctx);
    let prev_value = state.mpe_status.slide_value(channel) as f64;
    let delta = value - prev_value;
    // Max change per event ~1 pad width; an adjacent row touch would exceed this
    let max_delta = 170.0;
    let clamped_value = prev_value + delta.clamp(-max_delta, max_delta);
    // Store clamped value for next event
    state.mpe_status.set_slide_value(channel, clamped_value as i32);
    clamped_value
}

fn calc_note_index(intervals: &[i32], note: i32, tonic: i32) -> i32 {
    let rel = (note - tonic).rem_euclid(12);
    intervals.iter().position(|i| *i == rel).unwrap_or(0) as i32
}

/// Former `updateMPE`: switch the device to MPE if the active view is an expression view and MPE
/// is enabled.
pub fn update_mpe(ctx: &mut Ctx) {
    let mpe_enabled = is_mpe_enabled(ctx);
    send_mpe_active(ctx, mpe_enabled);
    ctx.surface.rebind_grid();
}

/// Former `updateMPEPitchbendRange`.
pub fn update_mpe_pitchbend_range(ctx: &mut Ctx) {
    let range = ctx.config.mpe_pitch_bend_range();
    let output = &ctx.surface.output;
    // Former AbstractMidiOutput.sendMPEPitchbendRange
    output.send_midi_short(ZONE_1, 0x65, 0x00);
    output.send_midi_short(ZONE_1, 0x64, 0x00);
    output.send_midi_short(ZONE_1, 0x06, range);
    output.send_midi_short(ZONE_1, 0x26, 0);
    output.send_midi_short(ZONE_1, 0x65, 127);
    output.send_midi_short(ZONE_1, 0x64, 127);
}

/// Former `updateColorPalette`: read and write the palette entries step by step (scheduled
/// every 10ms), then let the device reapply the palette.
pub fn update_color_palette(ctx: &mut Ctx) {
    use super::palette::{ColorPalette, PaletteUpdate, REAPPLY_DELAY_MS, UPDATE_INTERVAL_MS};
    let output = ctx.surface.output.clone();
    let now = ctx.host.now();
    let result = state_mut(ctx).palette.update_palette(&output, now);
    for error in state_mut(ctx).palette.take_errors() {
        ctx.host.error(&error);
    }
    match result {
        PaletteUpdate::Continue => ctx.schedule(UPDATE_INTERVAL_MS, update_color_palette),
        PaletteUpdate::Reapply => ctx.schedule(REAPPLY_DELAY_MS, |ctx| ColorPalette::send_reapply(&ctx.surface.output)),
        PaletteUpdate::Done => {}
    }
}

/// Former `internalShutdown`.
pub fn shutdown(ctx: &mut Ctx) {
    set_ribbon_mode(ctx, PUSH_RIBBON_PITCHBEND);
    set_ribbon_value(ctx, 0);
}

/// The selected pad threshold of the Push 1 as text (former `getSelectedPadThreshold`).
pub fn selected_pad_threshold(ctx: &Ctx) -> String {
    config::push_pad_thresholds_names().get(config::pad_threshold_push1(&ctx.config)).cloned().unwrap_or_default()
}

/// The selected velocity curve of the Push 1 as text (former `getSelectedVelocityCurve`).
pub fn selected_velocity_curve(ctx: &Ctx) -> &'static str {
    config::PUSH_PAD_CURVES_NAME.get(config::velocity_curve(&ctx.config)).copied().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push2_curve() {
        let curve = calc_pad_sensitivity_curve_push2(5, 5, 5);
        assert_eq!(curve.len(), 128);
        assert_eq!(curve[0], 1);
        assert_eq!(curve[127], 127);
        assert!(curve.windows(2).all(|w| w[0] <= w[1]), "{curve:?}");
    }

    #[test]
    fn push3_curve() {
        let curve = calc_pad_sensitivity_curve_push3(7, 4, -26, 35);
        assert_eq!(curve.len(), 128);
        // 7% threshold = 1 value of 1, 35% range = 36 values of 127
        assert_eq!(curve[0], 1);
        assert_eq!(curve[127], 127);
        assert_eq!(curve[128 - 36], 127);
        assert!(curve[1..92].iter().all(|v| (1..=127).contains(v)));
        assert!(curve.windows(2).all(|w| w[0] <= w[1]), "{curve:?}");
    }

    #[test]
    fn mpe_pitchbend() {
        let major = [0, 2, 4, 5, 7, 9, 11];
        // No slide: centered
        assert_eq!(calc_mpe_pitchbend(&major, 0, 60, 0.0), Some((0, 64)));
        // One pad to the right in C major from C: 2 semitones up of 24
        let (lsb, msb) = calc_mpe_pitchbend(&major, 0, 60, 170.6).unwrap();
        let value14 = (msb << 7) | lsb;
        assert_eq!(value14, 8192 + (8191.0f64 * 2.0 / 24.0).round() as i32);
    }

    #[test]
    fn push2_data() {
        assert!(is_push2_data(&[0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x04, 0xF7]));
        assert!(!is_push2_data(&[0xF0, 0x47, 0x7F, 0x15, 0xF7]));
    }
}
