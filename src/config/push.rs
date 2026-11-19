// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Ableton Push 1, 2 and 3 (former `PushConfiguration`).
//!
//! The settings are stored in the generic [`Configuration`] (as controller specific settings);
//! the functions of this module read and change them. The state which is not a setting in Java
//! (the lock state, the layer mix mode) lives in the surface state of the controller
//! (`controller::push::surface::PushState`).
//!
//! Differences to Java:
//! - The display colors are text settings with the color as `#RRGGBB` (the Rust settings have no
//!   color type); invalid values fall back to the default color.
//! - The device favorites (former `activateDeviceFavorites`, "Add Track - Device Favorites")
//!   are not activated: they need the device metadata of the host which the core does not have.
//! - The "Display window" debug button (former `DEBUG_WINDOW`) is activated but does nothing (no
//!   debug window).

use super::configuration::{Configuration, NoteRepeatCapabilities, CATEGORY_DEBUG, CATEGORY_HARDWARE_SETUP, CATEGORY_PADS, CATEGORY_SESSION, CATEGORY_WORKFLOW};
use super::setting::{Scope, SettingDescriptor, SettingId, SettingKind};
use super::values::{ArpeggiatorMode, AFTERTOUCH_CONVERT_OFF, REAPER_ARP_MODES};
use crate::daw::color::ColorEx;
use crate::framework::value_changer::ValueChanger;
use crate::framework::ids::{Modes, Views};

/// The Push model (former `PushVersion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PushVersion {
    Version1,
    Version2,
    Version3,
}

impl PushVersion {
    /// True for the Push 2 and 3 (former `isPushModern`).
    pub fn is_modern(self) -> bool {
        self != PushVersion::Version1
    }
}

/// The lock state of the mute, solo and stop clip buttons (former `PushConfiguration.LockState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LockState {
    /// No lock state.
    #[default]
    Off,
    /// Locked to mute.
    Mute,
    /// Locked to solo.
    Solo,
    /// Locked to clip stop.
    ClipStop,
}

const fn id(offset: u16) -> SettingId {
    SettingId::NEXT_SETTING_ID.offset(offset)
}

pub const RIBBON_MODE: SettingId = id(0);
pub const RIBBON_MODE_CC_VAL: SettingId = id(1);
pub const RIBBON_MODE_NOTE_REPEAT: SettingId = id(2);
pub const VELOCITY_CURVE: SettingId = id(3);
pub const PAD_PUSH1_THRESHOLD: SettingId = id(4);
pub const DISPLAY_BRIGHTNESS: SettingId = id(5);
pub const LED_BRIGHTNESS: SettingId = id(6);
pub const PAD_PUSH2_SENSITIVITY: SettingId = id(7);
pub const PAD_PUSH2_GAIN: SettingId = id(8);
pub const PAD_PUSH2_DYNAMICS: SettingId = id(9);
pub const PAD_PUSH3_THRESHOLD: SettingId = id(10);
pub const PAD_PUSH3_DRIVE: SettingId = id(11);
pub const PAD_PUSH3_COMPAND: SettingId = id(12);
pub const PAD_PUSH3_RANGE: SettingId = id(14);
pub const STOP_AUTOMATION_ON_KNOB_RELEASE: SettingId = id(15);
pub const DEBUG_MODE: SettingId = id(16);
pub const DEBUG_WINDOW: SettingId = id(17);
pub const COLOR_BACKGROUND: SettingId = id(20);
pub const COLOR_BORDER: SettingId = id(21);
pub const COLOR_TEXT: SettingId = id(22);
pub const COLOR_FADER: SettingId = id(23);
pub const COLOR_VU: SettingId = id(24);
pub const COLOR_EDIT: SettingId = id(25);
pub const COLOR_RECORD: SettingId = id(26);
pub const COLOR_SOLO: SettingId = id(27);
pub const COLOR_MUTE: SettingId = id(28);
pub const COLOR_BACKGROUND_DARKER: SettingId = id(29);
pub const COLOR_BACKGROUND_LIGHTER: SettingId = id(30);
pub const SESSION_VIEW: SettingId = id(31);
pub const DISPLAY_SCENES_CLIPS: SettingId = id(32);
pub const PER_PAD_PITCHBEND: SettingId = id(33);
pub const IN_TUNE_LOCATION: SettingId = id(34);
pub const IN_TUNE_WIDTH: SettingId = id(35);
pub const IN_TUNE_SLIDE_HEIGHT: SettingId = id(36);
pub const PEDAL_1: SettingId = id(37);
pub const PEDAL_2: SettingId = id(38);
pub const PREAMP_TYPE_1: SettingId = id(39);
pub const PREAMP_TYPE_2: SettingId = id(40);
pub const PREAMP_GAIN_1: SettingId = id(41);
pub const PREAMP_GAIN_2: SettingId = id(42);
pub const AUDIO_OUTPUTS: SettingId = id(43);
/// The "Reset colors to default" button (Java: a signal setting without an ID).
pub const COLOR_RESET: SettingId = id(44);

pub const RIBBON_MODE_PITCH: usize = 0;
pub const RIBBON_MODE_CC: usize = 1;
pub const RIBBON_MODE_CC_PB: usize = 2;
pub const RIBBON_MODE_PB_CC: usize = 3;
pub const RIBBON_MODE_FADER: usize = 4;
pub const RIBBON_MODE_LAST_TOUCHED: usize = 5;

pub const NOTE_REPEAT_OFF: usize = 0;
pub const NOTE_REPEAT_PERIOD: usize = 1;
pub const NOTE_REPEAT_LENGTH: usize = 2;

const CATEGORY_RIBBON: &str = "Ribbon";
const CATEGORY_AUDIO: &str = "Audio Interface";
const CATEGORY_COLORS: &str = "Display Colors";

pub const RIBBON_MODE_VALUES: [&str; 6] = ["Pitch", "CC", "CC/Pitch", "Pitch/CC", "Fader", "Last Touched"];
pub const RIBBON_NOTE_REPEAT_VALUES: [&str; 3] = ["Off", "Period", "Length"];
pub const SESSION_VIEW_OPTIONS: [&str; 3] = ["Clips", "Flipped", "Scenes"];
const ON_OFF_OPTIONS: [&str; 2] = ["Off", "On"];

/// The views which can be selected as the startup view (former `PREFERRED_NOTE_VIEWS`).
pub const PREFERRED_NOTE_VIEWS: [Views; 11] = [
    Views::Play,
    Views::Chords,
    Views::Piano,
    Views::Drum64,
    Views::Drum,
    Views::Drum4,
    Views::Drum8,
    Views::DrumXox,
    Views::Sequencer,
    Views::Raindrops,
    Views::PolySequencer,
];

/// The modes of the debug setting (former `DEBUG_MODES`, an `EnumSet`, i.e. in the order of the
/// `Modes` enumeration).
pub const DEBUG_MODES: [Modes; 51] = [
    Modes::Track,
    Modes::TrackDetails,
    Modes::Volume,
    Modes::Pan,
    Modes::Crossfader,
    Modes::RecArm,
    Modes::Send1,
    Modes::Send2,
    Modes::Send3,
    Modes::Send4,
    Modes::Send5,
    Modes::Send6,
    Modes::Send7,
    Modes::Send8,
    Modes::Master,
    Modes::MasterTemp,
    Modes::DeviceParams,
    Modes::DeviceChains,
    Modes::DeviceLayer,
    Modes::DeviceLayerVolume,
    Modes::DeviceLayerPan,
    Modes::DeviceLayerSend1,
    Modes::DeviceLayerSend2,
    Modes::DeviceLayerSend3,
    Modes::DeviceLayerSend4,
    Modes::DeviceLayerSend5,
    Modes::DeviceLayerSend6,
    Modes::DeviceLayerSend7,
    Modes::DeviceLayerSend8,
    Modes::DeviceLayerDetails,
    Modes::Browser,
    Modes::Clip,
    Modes::Note,
    Modes::Frame,
    Modes::Groove,
    Modes::Accent,
    Modes::Scales,
    Modes::ScaleLayout,
    Modes::Fixed,
    Modes::Ribbon,
    Modes::ViewSelect,
    Modes::Automation,
    Modes::Transport,
    Modes::Configuration,
    Modes::Setup,
    Modes::Info,
    Modes::SessionViewSelect,
    Modes::Session,
    Modes::Markers,
    Modes::RepeatNote,
    Modes::User,
];

pub const IN_TUNE_LOCATION_OPTIONS: [&str; 2] = ["Pad", "Finger"];
pub const IN_TUNE_WIDTH_OPTIONS: [&str; 12] = ["0", "1", "2", "2.5", "3", "4", "5", "6", "7", "10", "13", "20"];
pub const SLIDE_HEIGHT_OPTIONS: [&str; 7] = ["10", "11", "12", "13", "14", "15", "16"];
pub const FOOT_CV_OPTIONS: [&str; 2] = ["Footswitch", "CV Out"];
pub const PREAMP_TYPE_OPTIONS: [&str; 3] = ["Line", "Instrument", "High"];
pub const PREAMP_GAIN_OPTIONS: [&str; 21] = [
    "0dB", "1dB", "2dB", "3dB", "4dB", "5dB", "6dB", "7dB", "8dB", "9dB", "10dB", "11dB", "12dB", "13dB", "14dB", "15dB", "16dB", "17dB", "18dB", "19dB", "20dB",
];
pub const OUTPUT_CONFIGURATION: [&str; 3] = ["Headphones 1/2 - Speaker 1/2", "Headphones 3/4 - Speaker 1/2", "Headphones 1/2 - Speaker 3/4"];
pub const OUTPUT_CONFIGURATION_SHORT: [&str; 3] = ["H1/2-S1/2", "H3/4-S1/2", "H1/2-S3/4"];

/// The names of the velocity curves of the Push 1 (former `PUSH_PAD_CURVES_NAME`).
pub const PUSH_PAD_CURVES_NAME: [&str; 6] = ["Linear", "Log 1 (Default)", "Log 2", "Log 3", "Log 4", "Log 5"];

/// The names of the pad thresholds of the Push 1 (former `PUSH_PAD_THRESHOLDS_NAME`): "-20"
/// to "20", with "0 (Default)".
pub fn push_pad_thresholds_names() -> Vec<String> {
    (-20..=20).map(|i| if i == 0 { "0 (Default)".to_string() } else { i.to_string() }).collect()
}

/// The default display colors (former `DEFAULT_COLOR_*` of `AbstractConfiguration`).
pub const DEFAULT_COLOR_BACKGROUND: ColorEx = ColorEx::from_rgb(83, 83, 83);
pub const DEFAULT_COLOR_BORDER: ColorEx = ColorEx::BLACK;
pub const DEFAULT_COLOR_TEXT: ColorEx = ColorEx::BLACK;
pub const DEFAULT_COLOR_FADER: ColorEx = ColorEx::from_rgb(69, 44, 19);
pub const DEFAULT_COLOR_VU: ColorEx = ColorEx::GREEN;
pub const DEFAULT_COLOR_EDIT: ColorEx = ColorEx::from_rgb(240, 127, 17);
pub const DEFAULT_COLOR_RECORD: ColorEx = ColorEx::RED;
pub const DEFAULT_COLOR_SOLO: ColorEx = ColorEx::YELLOW;
pub const DEFAULT_COLOR_MUTE: ColorEx = ColorEx::from_rgb(245, 129, 17);
pub const DEFAULT_COLOR_BACKGROUND_DARKER: ColorEx = ColorEx::from_rgb(39, 39, 39);
pub const DEFAULT_COLOR_BACKGROUND_LIGHTER: ColorEx = ColorEx::from_rgb(200, 200, 200);

/// The color settings with their labels and defaults, in the order of their activation.
pub const COLOR_SETTINGS: [(SettingId, &str, ColorEx); 11] = [
    (COLOR_BACKGROUND, "Background", DEFAULT_COLOR_BACKGROUND),
    (COLOR_BACKGROUND_DARKER, "Background Darker", DEFAULT_COLOR_BACKGROUND_DARKER),
    (COLOR_BACKGROUND_LIGHTER, "Background Selected", DEFAULT_COLOR_BACKGROUND_LIGHTER),
    (COLOR_BORDER, "Border", DEFAULT_COLOR_BORDER),
    (COLOR_TEXT, "Text", DEFAULT_COLOR_TEXT),
    (COLOR_FADER, "Fader", DEFAULT_COLOR_FADER),
    (COLOR_VU, "VU", DEFAULT_COLOR_VU),
    (COLOR_EDIT, "Edit", DEFAULT_COLOR_EDIT),
    (COLOR_RECORD, "Record", DEFAULT_COLOR_RECORD),
    (COLOR_SOLO, "Solo", DEFAULT_COLOR_SOLO),
    (COLOR_MUTE, "Mute", DEFAULT_COLOR_MUTE),
];

fn options(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

/// The Java name of a mode (former `Modes.toString`), e.g. `TRACK_DETAILS`, `SEND1`.
pub fn java_mode_name(mode: Modes) -> String {
    let mut result = String::new();
    let mut previous_lower = false;
    for c in mode.name().chars() {
        if c.is_ascii_uppercase() && previous_lower {
            result.push('_');
        }
        previous_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        result.push(c.to_ascii_uppercase());
    }
    result
}

/// Format a color as `#RRGGBB`.
pub fn color_to_hex(color: ColorEx) -> String {
    let [r, g, b] = color.to_int_rgb255();
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Parse a color from `#RRGGBB`.
pub fn color_from_hex(text: &str) -> Option<ColorEx> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(ColorEx::from_rgb(((value >> 16) & 0xFF) as i32, ((value >> 8) & 0xFF) as i32, (value & 0xFF) as i32))
}

/// Create the configuration and activate all settings (the former `init`).
pub fn create(version: PushVersion, arpeggiator_modes: &[ArpeggiatorMode], note_repeat: NoteRepeatCapabilities) -> Configuration {
    let mut c = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;
    let document = Scope::Document;
    c.set_preferred_audio_view(Views::ClipLength);
    c.add_dont_notify_all(DEBUG_WINDOW);

    // Scale
    c.activate_scale_setting(document);
    c.activate_scale_base_setting(document);
    c.activate_scale_in_scale_setting(document);
    c.activate_scale_layout_setting(document);

    // Note Repeat
    c.activate_note_repeat_setting(document, note_repeat);

    // Session
    c.activate_enum_setting(SESSION_VIEW, global, CATEGORY_SESSION, "Pads", options(&SESSION_VIEW_OPTIONS), SESSION_VIEW_OPTIONS[0], true);
    c.activate_select_clip_on_launch_setting(global);
    c.activate_draw_record_stripe_setting(global);
    c.activate_action_for_rec_armed_pad(global);

    // Transport
    c.activate_behaviour_on_pause_setting(global);
    c.activate_double_click_play_button(global);
    c.activate_flip_record_setting(global);

    // Play and Sequence
    c.activate_accent_active_setting(global);
    c.activate_accent_value_setting(global);
    c.activate_quantize_amount_setting(global);
    c.activate_startup_view_setting(global, &PREFERRED_NOTE_VIEWS);
    c.activate_start_with_session_view_setting(global);
    c.activate_midi_edit_channel_setting(document);
    c.activate_turn_off_scale_pads_setting(global);
    c.activate_show_played_chords_setting(global);

    // Drum Sequencer (Java: only if the host has a drum device; the model decides on Reaper)
    c.activate_auto_select_drum_setting(global);
    c.activate_turn_off_empty_drum_pads_setting(global);

    // Workflow
    c.activate_track_navigation_setting(global, Some(CATEGORY_WORKFLOW), false);
    c.activate_cursor_keys_settings(global);
    c.activate_include_master_setting(global);
    c.activate_exclude_deactivated_items_setting(global);
    c.activate_enable_vu_meters_setting(global, CATEGORY_WORKFLOW);
    c.activate_footswitch_setting(global, 0, "Footswitch 2", None);
    c.activate_enum_setting(STOP_AUTOMATION_ON_KNOB_RELEASE, global, CATEGORY_WORKFLOW, "Stop automation recording on knob release", options(&ON_OFF_OPTIONS), ON_OFF_OPTIONS[0], true);
    c.activate_new_clip_length_setting(global);
    c.activate_knob_speed_setting(global, 100, 60, false);

    // Ribbon
    c.activate_enum_setting(RIBBON_MODE, global, CATEGORY_RIBBON, "Mode", options(&RIBBON_MODE_VALUES), RIBBON_MODE_VALUES[0], true);
    c.activate_range_setting(RIBBON_MODE_CC_VAL, global, CATEGORY_RIBBON, "CC", 0, 127, 1, true);
    c.activate_enum_setting(
        RIBBON_MODE_NOTE_REPEAT,
        global,
        CATEGORY_RIBBON,
        "Function if Note Repeat is active",
        options(&RIBBON_NOTE_REPEAT_VALUES),
        RIBBON_NOTE_REPEAT_VALUES[1],
        true,
    );

    // Hardware configuration
    match version {
        PushVersion::Version1 => {
            c.activate_enum_setting(VELOCITY_CURVE, global, CATEGORY_PADS, "Velocity Curve", options(&PUSH_PAD_CURVES_NAME), PUSH_PAD_CURVES_NAME[1], true);
            let thresholds = push_pad_thresholds_names();
            let default_threshold = thresholds[20].clone();
            c.activate_enum_setting(PAD_PUSH1_THRESHOLD, global, CATEGORY_PADS, "Pad Threshold", thresholds, &default_threshold, true);
            c.activate_convert_aftertouch_setting(global);
        }
        PushVersion::Version2 => {
            c.activate_range_setting(PAD_PUSH2_SENSITIVITY, global, CATEGORY_PADS, "Sensitivity", 0, 10, 5, true);
            c.activate_range_setting(PAD_PUSH2_GAIN, global, CATEGORY_PADS, "Gain", 0, 10, 5, true);
            c.activate_range_setting(PAD_PUSH2_DYNAMICS, global, CATEGORY_PADS, "Dynamics", 0, 10, 5, true);
            c.activate_convert_aftertouch_setting(global);
        }
        PushVersion::Version3 => {
            c.activate_range_setting(PAD_PUSH3_THRESHOLD, global, CATEGORY_PADS, "Threshold", 0, 100, 7, true);
            c.activate_range_setting(PAD_PUSH3_DRIVE, global, CATEGORY_PADS, "Drive", -50, 50, 4, true);
            c.activate_range_setting(PAD_PUSH3_COMPAND, global, CATEGORY_PADS, "Compand", -50, 50, -26, true);
            c.activate_range_setting(PAD_PUSH3_RANGE, global, CATEGORY_PADS, "Range", 0, 100, 35, true);

            c.activate_mpe_setting(global, CATEGORY_PADS, true);
            c.activate_enum_setting(PER_PAD_PITCHBEND, global, CATEGORY_PADS, "Per-Pad Pitchbend", options(&ON_OFF_OPTIONS), ON_OFF_OPTIONS[1], true);
            c.activate_enum_setting(IN_TUNE_LOCATION, global, CATEGORY_PADS, "In Tune Location", options(&IN_TUNE_LOCATION_OPTIONS), IN_TUNE_LOCATION_OPTIONS[1], true);
            c.activate_enum_setting(IN_TUNE_WIDTH, global, CATEGORY_PADS, "In Tune Width (mm)", options(&IN_TUNE_WIDTH_OPTIONS), IN_TUNE_WIDTH_OPTIONS[9], true);
            c.activate_enum_setting(IN_TUNE_SLIDE_HEIGHT, global, CATEGORY_PADS, "Slide Height (mm)", options(&SLIDE_HEIGHT_OPTIONS), SLIDE_HEIGHT_OPTIONS[3], true);

            c.set_convert_aftertouch_field(AFTERTOUCH_CONVERT_OFF);

            c.activate_enum_setting(PEDAL_1, global, CATEGORY_AUDIO, "Pedal & CV 1", options(&FOOT_CV_OPTIONS), FOOT_CV_OPTIONS[0], true);
            c.activate_enum_setting(PEDAL_2, global, CATEGORY_AUDIO, "Pedal & CV 2", options(&FOOT_CV_OPTIONS), FOOT_CV_OPTIONS[0], true);
            c.activate_enum_setting(PREAMP_TYPE_1, global, CATEGORY_AUDIO, "Preamp 1", options(&PREAMP_TYPE_OPTIONS), PREAMP_TYPE_OPTIONS[0], true);
            c.activate_enum_setting(PREAMP_TYPE_2, global, CATEGORY_AUDIO, "Preamp 2", options(&PREAMP_TYPE_OPTIONS), PREAMP_TYPE_OPTIONS[0], true);
            c.activate_enum_setting(PREAMP_GAIN_1, global, CATEGORY_AUDIO, "Gain 1", options(&PREAMP_GAIN_OPTIONS), PREAMP_GAIN_OPTIONS[0], true);
            c.activate_enum_setting(PREAMP_GAIN_2, global, CATEGORY_AUDIO, "Gain 2", options(&PREAMP_GAIN_OPTIONS), PREAMP_GAIN_OPTIONS[0], true);
            c.activate_enum_setting(AUDIO_OUTPUTS, global, CATEGORY_AUDIO, "Outputs", options(&OUTPUT_CONFIGURATION), OUTPUT_CONFIGURATION[0], true);
        }
    }

    if version.is_modern() {
        // Push 2 Hardware
        c.activate_range_setting(DISPLAY_BRIGHTNESS, global, CATEGORY_HARDWARE_SETUP, "Display Brightness", 0, 100, 100, true);
        c.activate_range_setting(LED_BRIGHTNESS, global, CATEGORY_HARDWARE_SETUP, "LED Brightness", 0, 100, 100, true);

        // Display colors
        c.activate_setting(SettingDescriptor {
            id: COLOR_RESET,
            scope: global,
            category: CATEGORY_COLORS.to_string(),
            label: "Reset colors to default".to_string(),
            kind: SettingKind::Signal { button: "Reset".to_string() },
            default_value: String::new(),
            observable: true,
        });
        for (id, label, default) in COLOR_SETTINGS {
            c.activate_setting(SettingDescriptor {
                id,
                scope: global,
                category: CATEGORY_COLORS.to_string(),
                label: label.to_string(),
                kind: SettingKind::Text,
                default_value: color_to_hex(default),
                observable: true,
            });
        }
    }

    // Debugging
    let modes: Vec<String> = DEBUG_MODES.iter().map(|m| java_mode_name(*m)).collect();
    c.activate_enum_setting(DEBUG_MODE, global, CATEGORY_DEBUG, "Display Mode", modes, "TRACK", true);
    if version.is_modern() {
        c.activate_setting(SettingDescriptor {
            id: DEBUG_WINDOW,
            scope: global,
            category: CATEGORY_DEBUG.to_string(),
            label: " ".to_string(),
            kind: SettingKind::Signal { button: "Display window".to_string() },
            default_value: String::new(),
            observable: true,
        });
    }
    c
}

/// Create the configuration with the Reaper arpeggiator modes and note repeat capabilities.
pub fn for_reaper(version: PushVersion) -> Configuration {
    create(version, &REAPER_ARP_MODES, NoteRepeatCapabilities::REAPER)
}

// ----------------------------------------------------------------------------------------------
// Ribbon

pub fn ribbon_mode(c: &Configuration) -> usize {
    c.custom_index(RIBBON_MODE)
}

pub fn set_ribbon_mode(c: &mut Configuration, mode: usize) {
    if let Some(value) = RIBBON_MODE_VALUES.get(mode) {
        c.set_value(RIBBON_MODE, value);
    }
}

pub fn ribbon_mode_cc_val(c: &Configuration) -> i32 {
    c.custom_int(RIBBON_MODE_CC_VAL)
}

pub fn set_ribbon_mode_cc(c: &mut Configuration, value: i32) {
    c.set_value(RIBBON_MODE_CC_VAL, &value.to_string());
}

pub fn ribbon_note_repeat(c: &Configuration) -> usize {
    c.custom_index(RIBBON_MODE_NOTE_REPEAT)
}

pub fn set_ribbon_note_repeat(c: &mut Configuration, mode: usize) {
    if let Some(value) = RIBBON_NOTE_REPEAT_VALUES.get(mode) {
        c.set_value(RIBBON_MODE_NOTE_REPEAT, value);
    }
}

// ----------------------------------------------------------------------------------------------
// Pads

fn change_index(c: &mut Configuration, vc: &ValueChanger, id: SettingId, control: i32, size: usize) {
    let current = c.custom_index(id) as i32;
    let value = vc.change_value_with(control, current, -100.0, size as i32);
    c.set_custom_index(id, value.clamp(0, size.saturating_sub(1) as i32) as usize);
}

fn change_int(c: &mut Configuration, vc: &ValueChanger, id: SettingId, control: i32, offset: i32, upper_bound: i32) {
    let current = c.custom_int(id) + offset;
    let value = vc.change_value_with(control, current, -100.0, upper_bound) - offset;
    c.set_value(id, &value.to_string());
}

/// Former `getVelocityCurve` (Push 1).
pub fn velocity_curve(c: &Configuration) -> usize {
    if c.is_setting_active(VELOCITY_CURVE) {
        c.custom_index(VELOCITY_CURVE)
    } else {
        1
    }
}

pub fn set_velocity_curve(c: &mut Configuration, index: usize) {
    c.set_custom_index(VELOCITY_CURVE, index);
}

pub fn change_velocity_curve(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_index(c, vc, VELOCITY_CURVE, control, PUSH_PAD_CURVES_NAME.len());
}

/// Former `getPadThresholdPush1`.
pub fn pad_threshold_push1(c: &Configuration) -> usize {
    if c.is_setting_active(PAD_PUSH1_THRESHOLD) {
        c.custom_index(PAD_PUSH1_THRESHOLD)
    } else {
        20
    }
}

pub fn set_pad_threshold_push1(c: &mut Configuration, index: usize) {
    c.set_custom_index(PAD_PUSH1_THRESHOLD, index);
}

pub fn change_pad_threshold(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_index(c, vc, PAD_PUSH1_THRESHOLD, control, 41);
}

fn int_or(c: &Configuration, id: SettingId, default: i32) -> i32 {
    if c.is_setting_active(id) {
        c.custom_int(id)
    } else {
        default
    }
}

/// Former `getDisplayBrightness` (0-100; Java's initial field value 255 is overwritten by the
/// setting).
pub fn display_brightness(c: &Configuration) -> i32 {
    int_or(c, DISPLAY_BRIGHTNESS, 100)
}

pub fn set_display_brightness(c: &mut Configuration, value: i32) {
    c.set_value(DISPLAY_BRIGHTNESS, &value.to_string());
}

pub fn change_display_brightness(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, DISPLAY_BRIGHTNESS, control, 0, 101);
}

pub fn led_brightness(c: &Configuration) -> i32 {
    int_or(c, LED_BRIGHTNESS, 100)
}

pub fn set_led_brightness(c: &mut Configuration, value: i32) {
    c.set_value(LED_BRIGHTNESS, &value.to_string());
}

pub fn change_led_brightness(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, LED_BRIGHTNESS, control, 0, 101);
}

pub fn pad_sensitivity_push2(c: &Configuration) -> i32 {
    int_or(c, PAD_PUSH2_SENSITIVITY, 5)
}

pub fn set_pad_sensitivity_push2(c: &mut Configuration, value: i32) {
    c.set_value(PAD_PUSH2_SENSITIVITY, &value.to_string());
}

pub fn change_pad_sensitivity(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, PAD_PUSH2_SENSITIVITY, control, 0, 11);
}

pub fn pad_gain_push2(c: &Configuration) -> i32 {
    int_or(c, PAD_PUSH2_GAIN, 5)
}

pub fn set_pad_gain_push2(c: &mut Configuration, value: i32) {
    c.set_value(PAD_PUSH2_GAIN, &value.to_string());
}

pub fn change_pad_gain(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, PAD_PUSH2_GAIN, control, 0, 11);
}

pub fn pad_dynamics_push2(c: &Configuration) -> i32 {
    int_or(c, PAD_PUSH2_DYNAMICS, 5)
}

pub fn set_pad_dynamics_push2(c: &mut Configuration, value: i32) {
    c.set_value(PAD_PUSH2_DYNAMICS, &value.to_string());
}

pub fn change_pad_dynamics(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, PAD_PUSH2_DYNAMICS, control, 0, 11);
}

pub fn pad_curve_threshold_push3(c: &Configuration) -> i32 {
    int_or(c, PAD_PUSH3_THRESHOLD, 7)
}

pub fn set_pad_curve_threshold_push3(c: &mut Configuration, value: i32) {
    c.set_value(PAD_PUSH3_THRESHOLD, &value.to_string());
}

pub fn change_pad_curve_threshold_push3(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, PAD_PUSH3_THRESHOLD, control, 0, 101);
}

pub fn pad_curve_drive_push3(c: &Configuration) -> i32 {
    int_or(c, PAD_PUSH3_DRIVE, 4)
}

pub fn set_pad_curve_drive_push3(c: &mut Configuration, value: i32) {
    c.set_value(PAD_PUSH3_DRIVE, &value.to_string());
}

pub fn change_pad_curve_drive_push3(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, PAD_PUSH3_DRIVE, control, 50, 101);
}

pub fn pad_curve_compand_push3(c: &Configuration) -> i32 {
    int_or(c, PAD_PUSH3_COMPAND, -26)
}

pub fn set_pad_curve_compand_push3(c: &mut Configuration, value: i32) {
    c.set_value(PAD_PUSH3_COMPAND, &value.to_string());
}

pub fn change_pad_curve_compand_push3(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, PAD_PUSH3_COMPAND, control, 50, 101);
}

pub fn pad_curve_range_push3(c: &Configuration) -> i32 {
    int_or(c, PAD_PUSH3_RANGE, 35)
}

pub fn set_pad_curve_range_push3(c: &mut Configuration, value: i32) {
    c.set_value(PAD_PUSH3_RANGE, &value.to_string());
}

pub fn change_pad_curve_range_push3(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    change_int(c, vc, PAD_PUSH3_RANGE, control, 0, 101);
}

// ----------------------------------------------------------------------------------------------
// Workflow

pub fn is_stop_automation_on_knob_release(c: &Configuration) -> bool {
    c.custom_bool(STOP_AUTOMATION_ON_KNOB_RELEASE)
}

/// Former `getMixerMode` (the debug mode setting).
pub fn mixer_mode(c: &Configuration) -> Modes {
    DEBUG_MODES.get(c.custom_index(DEBUG_MODE)).copied().unwrap_or(Modes::Track)
}

/// Former `setMixerMode`.
pub fn set_mixer_mode(c: &mut Configuration, mode: Modes) {
    c.set_value(DEBUG_MODE, &java_mode_name(mode));
}

/// True if the mode is one of the track (mix) modes (former `Modes.isTrackMode`).
pub fn is_track_mode(mode: Modes) -> bool {
    matches!(mode, Modes::Track | Modes::TrackDetails | Modes::Volume | Modes::Pan | Modes::Crossfader) || (Modes::Send1..=Modes::Send8).contains(&mode)
}

/// Former `getCurrentMixMode`.
pub fn current_mix_mode(c: &Configuration) -> Option<Modes> {
    let mode = mixer_mode(c);
    is_track_mode(mode).then_some(mode)
}

/// The layer mode which belongs to the current mix mode (former part of
/// `getCurrentLayerMixMode`, the cached `layerMode` is kept by the caller).
pub fn layer_mode_for_mix_mode(c: &Configuration, is_modern: bool) -> Modes {
    match current_mix_mode(c) {
        Some(mode) if is_modern => match mode {
            Modes::Volume => Modes::DeviceLayerVolume,
            Modes::Pan => Modes::DeviceLayerPan,
            m if (Modes::Send1..=Modes::Send8).contains(&m) => Modes::get(Modes::DeviceLayerSend1, m.ordinal() - Modes::Send1.ordinal()),
            _ => Modes::DeviceLayer,
        },
        _ => Modes::DeviceLayer,
    }
}

// ----------------------------------------------------------------------------------------------
// Session

/// Apply the session view setting to the flip session flag (former observer of
/// `sessionViewSetting`). Call it when [`SESSION_VIEW`] changed.
pub fn apply_session_view(c: &mut Configuration) {
    let index = c.custom_index(SESSION_VIEW);
    c.set_flip_session_field(index == 1);
}

/// Former `setFlipSession`.
pub fn set_flip_session(c: &mut Configuration, enabled: bool) {
    c.set_value(SESSION_VIEW, SESSION_VIEW_OPTIONS[usize::from(enabled)]);
    apply_session_view(c);
}

/// Former `setSceneView`.
pub fn set_scene_view(c: &mut Configuration) {
    c.set_value(SESSION_VIEW, SESSION_VIEW_OPTIONS[2]);
    apply_session_view(c);
}

/// Former `isScenesClipViewSelected`.
pub fn is_scenes_clip_view_selected(c: &Configuration) -> bool {
    c.custom_index(SESSION_VIEW) == 2
}

// ----------------------------------------------------------------------------------------------
// Display colors

/// The color of a display color setting (the default if not active or invalid).
pub fn color(c: &Configuration, id: SettingId) -> ColorEx {
    let default = COLOR_SETTINGS.iter().find(|(setting, _, _)| *setting == id).map_or(ColorEx::BLACK, |(_, _, default)| *default);
    c.value(id).and_then(|value| color_from_hex(&value)).unwrap_or(default)
}

/// Set a display color setting.
pub fn set_color(c: &mut Configuration, id: SettingId, color: ColorEx) {
    c.set_value(id, &color_to_hex(color));
}

/// Reset all display colors to their defaults (the "Reset" button).
pub fn reset_colors(c: &mut Configuration) {
    for (id, _, default) in COLOR_SETTINGS {
        set_color(c, id, default);
    }
}

pub fn is_color_setting(id: SettingId) -> bool {
    COLOR_SETTINGS.iter().any(|(setting, _, _)| *setting == id)
}

/// The graphics configuration of the Push 2/3 display from the color settings (former
/// `IGraphicsConfiguration` implemented by `PushConfiguration`).
pub fn graphics_configuration(c: &Configuration) -> crate::framework::graphics::GraphicsConfiguration {
    crate::framework::graphics::GraphicsConfiguration {
        color_text: color(c, COLOR_TEXT),
        color_background: color(c, COLOR_BACKGROUND),
        color_background_darker: color(c, COLOR_BACKGROUND_DARKER),
        color_background_lighter: color(c, COLOR_BACKGROUND_LIGHTER),
        color_border: color(c, COLOR_BORDER),
        color_edit: color(c, COLOR_EDIT),
        color_fader: color(c, COLOR_FADER),
        color_vu: color(c, COLOR_VU),
        color_record: color(c, COLOR_RECORD),
        color_solo: color(c, COLOR_SOLO),
        color_mute: color(c, COLOR_MUTE),
        ..crate::framework::graphics::GraphicsConfiguration::push_default()
    }
}

// ----------------------------------------------------------------------------------------------
// Push 3

pub fn is_per_pad_pitchbend(c: &Configuration) -> bool {
    !c.is_setting_active(PER_PAD_PITCHBEND) || c.custom_bool(PER_PAD_PITCHBEND)
}

pub fn set_per_pad_pitchbend_enabled(c: &mut Configuration, enable: bool) {
    c.set_value(PER_PAD_PITCHBEND, ON_OFF_OPTIONS[usize::from(enable)]);
}

pub fn change_per_pad_pitchbend_enabled(c: &mut Configuration, vc: &ValueChanger, control: i32) {
    let enable = vc.is_increase(control);
    set_per_pad_pitchbend_enabled(c, enable);
}

macro_rules! enum_accessors {
    ($get:ident, $set:ident, $change:ident, $id:expr, $len:expr) => {
        pub fn $get(c: &Configuration) -> usize {
            c.custom_index($id)
        }

        pub fn $set(c: &mut Configuration, index: usize) {
            c.set_custom_index($id, index);
        }

        pub fn $change(c: &mut Configuration, vc: &ValueChanger, control: i32) {
            change_index(c, vc, $id, control, $len);
        }
    };
}

enum_accessors!(in_tune_location, set_in_tune_location, change_in_tune_location, IN_TUNE_LOCATION, IN_TUNE_LOCATION_OPTIONS.len());
enum_accessors!(in_tune_width, set_in_tune_width, change_in_tune_width, IN_TUNE_WIDTH, IN_TUNE_WIDTH_OPTIONS.len());
enum_accessors!(in_tune_slide_height, set_slide_height, change_slide_height, IN_TUNE_SLIDE_HEIGHT, SLIDE_HEIGHT_OPTIONS.len());
enum_accessors!(pedal1, set_pedal1, change_pedal1, PEDAL_1, FOOT_CV_OPTIONS.len());
enum_accessors!(pedal2, set_pedal2, change_pedal2, PEDAL_2, FOOT_CV_OPTIONS.len());
enum_accessors!(preamp1_type, set_preamp1_type, change_preamp1_type, PREAMP_TYPE_1, PREAMP_TYPE_OPTIONS.len());
enum_accessors!(preamp2_type, set_preamp2_type, change_preamp2_type, PREAMP_TYPE_2, PREAMP_TYPE_OPTIONS.len());
enum_accessors!(preamp1_gain, set_preamp1_gain, change_preamp1_gain, PREAMP_GAIN_1, PREAMP_GAIN_OPTIONS.len());
enum_accessors!(preamp2_gain, set_preamp2_gain, change_preamp2_gain, PREAMP_GAIN_2, PREAMP_GAIN_OPTIONS.len());
enum_accessors!(audio_outputs, set_audio_outputs, change_audio_outputs, AUDIO_OUTPUTS, OUTPUT_CONFIGURATION.len());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names() {
        assert_eq!(java_mode_name(Modes::TrackDetails), "TRACK_DETAILS");
        assert_eq!(java_mode_name(Modes::Send1), "SEND1");
        assert_eq!(java_mode_name(Modes::DeviceLayerSend8), "DEVICE_LAYER_SEND8");
        assert_eq!(java_mode_name(Modes::RecArm), "REC_ARM");
        // The debug modes are in the order of the enumeration
        assert!(DEBUG_MODES.windows(2).all(|w| w[0].ordinal() < w[1].ordinal()));
    }

    #[test]
    fn settings_per_version() {
        let push1 = for_reaper(PushVersion::Version1);
        assert!(push1.is_setting_active(VELOCITY_CURVE));
        assert!(!push1.is_setting_active(DISPLAY_BRIGHTNESS));
        assert!(!push1.is_setting_active(COLOR_TEXT));
        assert_eq!(velocity_curve(&push1), 1);
        assert_eq!(pad_threshold_push1(&push1), 20);

        let push2 = for_reaper(PushVersion::Version2);
        assert!(push2.is_setting_active(PAD_PUSH2_GAIN));
        assert!(push2.is_setting_active(COLOR_TEXT));
        assert_eq!(display_brightness(&push2), 100);
        assert_eq!(pad_sensitivity_push2(&push2), 5);
        assert_eq!(color(&push2, COLOR_BACKGROUND), DEFAULT_COLOR_BACKGROUND);

        let push3 = for_reaper(PushVersion::Version3);
        assert!(push3.is_setting_active(PEDAL_1));
        assert!(!push3.is_setting_active(SettingId::CONVERT_AFTERTOUCH));
        assert_eq!(push3.convert_aftertouch(), AFTERTOUCH_CONVERT_OFF);
        assert_eq!(pad_curve_compand_push3(&push3), -26);
        assert_eq!(in_tune_width(&push3), 9);
        assert!(is_per_pad_pitchbend(&push3));
    }

    #[test]
    fn session_view_and_colors() {
        let mut c = for_reaper(PushVersion::Version2);
        assert!(!is_scenes_clip_view_selected(&c));
        set_flip_session(&mut c, true);
        assert!(c.is_flip_session());
        set_scene_view(&mut c);
        assert!(is_scenes_clip_view_selected(&c));
        assert!(!c.is_flip_session());

        set_color(&mut c, COLOR_TEXT, ColorEx::from_rgb(1, 2, 254));
        assert_eq!(c.value(COLOR_TEXT).as_deref(), Some("#0102FE"));
        assert_eq!(graphics_configuration(&c).color_text, ColorEx::from_rgb(1, 2, 254));
        reset_colors(&mut c);
        assert_eq!(color(&c, COLOR_TEXT), DEFAULT_COLOR_TEXT);

        assert_eq!(mixer_mode(&c), Modes::Track);
        set_mixer_mode(&mut c, Modes::Send2);
        assert_eq!(mixer_mode(&c), Modes::Send2);
        assert_eq!(layer_mode_for_mix_mode(&c, true), Modes::DeviceLayerSend2);
        assert_eq!(layer_mode_for_mix_mode(&c, false), Modes::DeviceLayer);
    }

    #[test]
    fn change_values() {
        let mut c = for_reaper(PushVersion::Version3);
        let vc = ValueChanger::twos_complement(1024, 10);
        change_pad_curve_drive_push3(&mut c, &vc, 1);
        assert_eq!(pad_curve_drive_push3(&c), 5);
        let mut c = for_reaper(PushVersion::Version1);
        change_velocity_curve(&mut c, &vc, 1);
        assert_eq!(velocity_curve(&c), 2);
    }
}
