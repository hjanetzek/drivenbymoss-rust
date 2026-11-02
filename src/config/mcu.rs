// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Mackie MCU protocol (former `MCUConfiguration`).
//!
//! The settings are stored with the same keys (category + label) as in Java. Differences:
//! - Java uses the ID `VU_METER_TYPE` for the VU meter style; in Rust that ID is the generic
//!   VU meter on/off setting, therefore the style has its own ID ([`VU_METER_STYLE`]).
//! - The settings without a Java ID (profile, extender device types, include FX tracks, the
//!   assignable functions and actions) got IDs; they are not observable, as in Java. A change of
//!   an assignable function is notified as [`ASSIGNABLE_BUTTONS`] by the setup.
//! - "Pin FX tracks to last device" is only activated if the host has an effect track bank,
//!   which Reaper has not (as in Java); it is never activated here.

use super::configuration::{Configuration, CATEGORY_HARDWARE_SETUP, CATEGORY_WORKFLOW};
use super::setting::{Scope, SettingId};
use super::values::{to_options, ArpeggiatorMode, ON_OFF_OPTIONS};
use crate::framework::ids::Modes;

pub const ZOOM_STATE: SettingId = SettingId::NEXT_SETTING_ID;
pub const DISPLAY_MODE_TIME_OR_BEATS: SettingId = SettingId::NEXT_SETTING_ID.offset(1);
pub const DISPLAY_MODE_TICKS_OR_TEMPO: SettingId = SettingId::NEXT_SETTING_ID.offset(2);
pub const MAIN_DISPLAY: SettingId = SettingId::NEXT_SETTING_ID.offset(3);
pub const SECOND_DISPLAY: SettingId = SettingId::NEXT_SETTING_ID.offset(4);
pub const HAS_SEGMENT_DISPLAY: SettingId = SettingId::NEXT_SETTING_ID.offset(5);
pub const HAS_ASSIGNMENT_DISPLAY: SettingId = SettingId::NEXT_SETTING_ID.offset(6);
pub const HAS_MOTOR_FADERS: SettingId = SettingId::NEXT_SETTING_ID.offset(7);
pub const HAS_ONLY_1_FADER: SettingId = SettingId::NEXT_SETTING_ID.offset(8);
pub const DISPLAY_TRACK_NAMES: SettingId = SettingId::NEXT_SETTING_ID.offset(9);
pub const USE_VERT_ZOOM_FOR_MODES: SettingId = SettingId::NEXT_SETTING_ID.offset(10);
pub const USE_FADERS_AS_KNOBS: SettingId = SettingId::NEXT_SETTING_ID.offset(11);
pub const TOUCH_SELECTS_CHANNEL: SettingId = SettingId::NEXT_SETTING_ID.offset(12);
pub const TOUCH_CHANNEL_VOLUME_MODE: SettingId = SettingId::NEXT_SETTING_ID.offset(13);
pub const ALWAYS_SEND_VU_METERS: SettingId = SettingId::NEXT_SETTING_ID.offset(14);
pub const PIN_FXTRACKS_TO_LAST_CONTROLLER: SettingId = SettingId::NEXT_SETTING_ID.offset(16);
pub const X_TOUCH_DISPLAY_COLORS: SettingId = SettingId::NEXT_SETTING_ID.offset(17);
/// Notified by the setup when an assignable function changed (as in Java).
pub const ASSIGNABLE_BUTTONS: SettingId = SettingId::NEXT_SETTING_ID.offset(18);
/// The device profile, which sets several hardware settings at once.
pub const PROFILE: SettingId = SettingId::NEXT_SETTING_ID.offset(19);
/// The VU meter style (Java: `VU_METER_TYPE`).
pub const VU_METER_STYLE: SettingId = SettingId::NEXT_SETTING_ID.offset(20);
/// The type of the device with index 0 (+1, +2, +3 for the others).
pub const DEVICE_TYPE_1: SettingId = SettingId::NEXT_SETTING_ID.offset(21);
pub const INCLUDE_FX_TRACKS: SettingId = SettingId::NEXT_SETTING_ID.offset(25);
/// The function of the assignable button with index 0 (+1 .. +9 for the others).
pub const ASSIGNABLE_1: SettingId = SettingId::NEXT_SETTING_ID.offset(30);
/// The action of the assignable button with index 0 (+1 .. +9 for the others).
pub const ASSIGNABLE_ACTION_1: SettingId = SettingId::NEXT_SETTING_ID.offset(40);

pub const FOOTSWITCH_PREV_MODE: i32 = 15;
pub const FOOTSWITCH_NEXT_MODE: i32 = 16;
pub const FOOTSWITCH_SHOW_MARKER_MODE: i32 = 17;
pub const FOOTSWITCH_USE_FADERS_LIKE_EDIT_KNOBS: i32 = 18;
pub const FOOTSWITCH_TOGGLE_MOTOR_FADERS_ON_OFF: i32 = 19;
pub const FOOTSWITCH_PUNCH_IN: i32 = 20;
pub const FOOTSWITCH_PUNCH_OUT: i32 = 21;
pub const FOOTSWITCH_DEVICE_ON_OFF: i32 = 22;
pub const PREV_CHANNEL: i32 = 23;
pub const NEXT_CHANNEL: i32 = 24;
pub const CONTROL_LAST_PARAM_ENCODER: i32 = 25;
pub const CONTROL_LAST_PARAM_MASTER_FADER: i32 = 26;
pub const FOOTSWITCH_ACTION: i32 = 27;

const TAG_ASPARION: &str = "Asparion";
const CATEGORY_EXTENDER_SETUP: &str = "Extender Setup (requires restart)";
const CATEGORY_SEGMENT_DISPLAY: &str = "Segment Display";
const CATEGORY_TRACKS: &str = "Tracks (requires restart)";
const CATEGORY_ASSIGNABLE_BUTTONS: &str = "Assignable buttons";

const DEVICE_SELECT: &str = "<Select a profile>";
const DEVICE_ASPARION_D700: &str = "Asparion D700";
const DEVICE_BEHRINGER_X_TOUCH: &str = "Behringer X-Touch";
const DEVICE_BEHRINGER_X_TOUCH_ONE: &str = "Behringer X-Touch One";
const DEVICE_ICON_P1_M: &str = "iCON P1-M";
const DEVICE_ICON_P1_NANO: &str = "iCON P1-NANO";
const DEVICE_ICON_PLATFORM_M: &str = "iCON Platform M / M+";
const DEVICE_ICON_QCON_PRO_X: &str = "iCON QConPro X";
const DEVICE_ICON_QCON_V1M: &str = "iCON V1-M";
const DEVICE_MACKIE_MCU_PRO: &str = "Mackie MCU Pro";
const DEVICE_ZOOM_R16: &str = "Zoom R16";

pub const DEVICE_OPTIONS: [&str; 11] = [
    DEVICE_SELECT,
    DEVICE_ASPARION_D700,
    DEVICE_BEHRINGER_X_TOUCH,
    DEVICE_BEHRINGER_X_TOUCH_ONE,
    DEVICE_ICON_P1_M,
    DEVICE_ICON_P1_NANO,
    DEVICE_ICON_PLATFORM_M,
    DEVICE_ICON_QCON_PRO_X,
    DEVICE_ICON_QCON_V1M,
    DEVICE_MACKIE_MCU_PRO,
    DEVICE_ZOOM_R16,
];

pub const ASSIGNABLE_VALUES: [&str; 28] = [
    "Toggle Play",
    "Toggle Record",
    "Stop All Clips",
    "Toggle Clip Overdub",
    "Undo",
    "Tap Tempo",
    "New Button",
    "Clip Based Looper",
    "Panel layout arrange",
    "Panel layout mix",
    "Panel layout edit",
    "Add instrument track",
    "Add audio track",
    "Add effect track",
    "Quantize",
    "Previous mode",
    "Next mode",
    "Marker mode",
    "Toggle use faders like editing knobs",
    "Toggle motor faders on/off",
    "Punch In",
    "Punch Out",
    "Device on/off",
    "Channel Prev",
    "Channel Next",
    "Last parameter (encoder)",
    "Last parameter (master fader)",
    "Action",
];

pub const ASSIGNABLE_BUTTON_NAMES: [&str; 10] = ["Footswitch 1", "Footswitch 2", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8"];

const ASSIGNABLE_BUTTON_DEFAULTS: [usize; 10] = [0, 1, 11, 12, 13, 6, 14, 20, 21, 22];

const TIME_OR_BEATS_OPTIONS: [&str; 2] = ["Time", "Beats"];
const TEMPO_OR_TICKS_OPTIONS: [&str; 2] = ["Ticks", "Tempo"];
const MCU_DEVICE_TYPE_OPTIONS: [&str; 3] = ["Main", "Extender", "Mackie Extender"];
const MCU_DEVICE_1_LEFT: &str = "MCU Device 1 - left";
const MCU_DEVICE_DESCRIPTORS: [&[&str]; 4] = [
    &["MCU Device 1"],
    &[MCU_DEVICE_1_LEFT, "MCU Device 2 - right"],
    &[MCU_DEVICE_1_LEFT, "MCU Device 2 - center", "MCU Device 3 - right"],
    &[MCU_DEVICE_1_LEFT, "MCU Device 2", "MCU Device 3", "MCU Device 4 - right"],
];
const MAIN_DISPLAY_OPTIONS: [&str; 4] = ["None", TAG_ASPARION, "Mackie - 6 characters", "Mackie - 7 characters"];
const DISPLAY_COLORS_OPTIONS: [&str; 5] = ["None", TAG_ASPARION, "Behringer (X-Touch Firmware 1.22)", "Behringer (X-Touch Firmware 1.25)", "iCON"];
const VU_METER_STYLES: [&str; 4] = ["None", TAG_ASPARION, "iCON", "Mackie"];
const SECOND_DISPLAY_OPTIONS: [&str; 4] = ["None", TAG_ASPARION, "iCON QCon Pro-X", DEVICE_ICON_QCON_V1M];

/// The modes which can be selected as the startup mode (with the names of `Modes.getModeName`).
pub const STARTUP_MODES: [(Modes, &str); 27] = [
    (Modes::Pan, "Panning"),
    (Modes::Volume, "Volume"),
    (Modes::Track, "Track"),
    (Modes::Send1, "Send 1"),
    (Modes::Send2, "Send 2"),
    (Modes::Send3, "Send 3"),
    (Modes::Send4, "Send 4"),
    (Modes::Send5, "Send 5"),
    (Modes::Send6, "Send 6"),
    (Modes::Send7, "Send 7"),
    (Modes::Send8, "Send 8"),
    (Modes::DeviceLayer, "Layer"),
    (Modes::DeviceLayerVolume, "Layer Volume"),
    (Modes::DeviceLayerPan, "Layer Panning"),
    (Modes::DeviceLayerSend1, "Layer Send 1"),
    (Modes::DeviceLayerSend2, "Layer Send 2"),
    (Modes::DeviceLayerSend3, "Layer Send 3"),
    (Modes::DeviceLayerSend4, "Layer Send 4"),
    (Modes::DeviceLayerSend5, "Layer Send 5"),
    (Modes::DeviceLayerSend6, "Layer Send 6"),
    (Modes::DeviceLayerSend7, "Layer Send 7"),
    (Modes::DeviceLayerSend8, "Layer Send 8"),
    (Modes::DeviceParams, "Parameters"),
    // The Java version has no name for the user mode
    (Modes::User, "Missing mode name"),
    (Modes::EqDeviceParams, "Equalizer"),
    (Modes::InstrumentDeviceParams, "Instrument Parameters"),
    (Modes::Markers, "Markers"),
];

/// The type of the main display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainDisplay {
    None,
    Asparion,
    Mackie6Characters,
    Mackie7Characters,
}

/// The type of the second display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondDisplay {
    None,
    Asparion,
    Qcon,
    V1m,
}

/// The style of the VU meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VuMeterStyle {
    None,
    Asparion,
    Icon,
    Mackie,
}

/// The protocol of the display colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayColors {
    None,
    Asparion,
    Behringer122,
    Behringer125,
    Icon,
}

/// The type of a device (former `MCUDeviceType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McuDeviceType {
    Main,
    Extender,
    MackieExtender,
}

fn on_off(on: bool) -> &'static str {
    ON_OFF_OPTIONS[usize::from(on)]
}

/// Create the MCU configuration for the given number of devices (main + extenders) with all
/// settings activated (former `MCUConfiguration.init`).
pub fn create(num_devices: usize, arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut c = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;
    let enum_setting = |c: &mut Configuration, id: SettingId, category: &str, label: &str, options: &[&str], default: &str, observable: bool| {
        c.activate_enum_setting(id, global, category, label, to_options(options), default, observable);
    };

    // Hardware
    enum_setting(&mut c, PROFILE, CATEGORY_HARDWARE_SETUP, "Profile", &DEVICE_OPTIONS, DEVICE_OPTIONS[0], false);
    enum_setting(&mut c, MAIN_DISPLAY, CATEGORY_HARDWARE_SETUP, "Main display", &MAIN_DISPLAY_OPTIONS, MAIN_DISPLAY_OPTIONS[2], true);
    enum_setting(&mut c, SECOND_DISPLAY, CATEGORY_HARDWARE_SETUP, "Has a second display", &SECOND_DISPLAY_OPTIONS, SECOND_DISPLAY_OPTIONS[0], true);
    enum_setting(&mut c, HAS_SEGMENT_DISPLAY, CATEGORY_HARDWARE_SETUP, "Has a segment display", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[1], true);
    enum_setting(&mut c, HAS_ASSIGNMENT_DISPLAY, CATEGORY_HARDWARE_SETUP, "Has an assignment display", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[1], true);
    enum_setting(&mut c, HAS_MOTOR_FADERS, CATEGORY_HARDWARE_SETUP, "Has motor faders", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[1], true);
    enum_setting(&mut c, HAS_ONLY_1_FADER, CATEGORY_HARDWARE_SETUP, "Has only 1 fader", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[0], true);
    enum_setting(&mut c, DISPLAY_TRACK_NAMES, CATEGORY_HARDWARE_SETUP, "Display track names in 1st display", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[0], true);
    enum_setting(&mut c, USE_VERT_ZOOM_FOR_MODES, CATEGORY_HARDWARE_SETUP, "Use vertical zoom to change modes", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[0], true);
    enum_setting(&mut c, USE_FADERS_AS_KNOBS, CATEGORY_HARDWARE_SETUP, "Use faders like editing knobs", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[0], true);
    enum_setting(&mut c, VU_METER_STYLE, CATEGORY_HARDWARE_SETUP, "VU Meter Type", &VU_METER_STYLES, VU_METER_STYLES[0], true);
    enum_setting(&mut c, ALWAYS_SEND_VU_METERS, CATEGORY_HARDWARE_SETUP, "Always send VU Meters", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[0], true);
    enum_setting(&mut c, SettingId::ENABLE_VU_METERS, CATEGORY_HARDWARE_SETUP, "VU Meters", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[1], true);
    enum_setting(&mut c, X_TOUCH_DISPLAY_COLORS, CATEGORY_HARDWARE_SETUP, "Display colors", &DISPLAY_COLORS_OPTIONS, DISPLAY_COLORS_OPTIONS[0], true);

    // Extenders: the last device is the main device by default
    let num_devices = num_devices.clamp(1, 4);
    for i in 0..num_devices {
        let label = MCU_DEVICE_DESCRIPTORS[num_devices - 1][i];
        let default = MCU_DEVICE_TYPE_OPTIONS[if i == num_devices - 1 { 0 } else { 1 }];
        enum_setting(&mut c, DEVICE_TYPE_1.offset(i as u16), CATEGORY_EXTENDER_SETUP, label, &MCU_DEVICE_TYPE_OPTIONS, default, false);
    }

    // Segment display
    enum_setting(&mut c, DISPLAY_MODE_TIME_OR_BEATS, CATEGORY_SEGMENT_DISPLAY, "Display time or beats", &TIME_OR_BEATS_OPTIONS, TIME_OR_BEATS_OPTIONS[0], true);
    enum_setting(
        &mut c,
        DISPLAY_MODE_TICKS_OR_TEMPO,
        CATEGORY_SEGMENT_DISPLAY,
        "Display tempo or ticks/milliseconds",
        &TEMPO_OR_TICKS_OPTIONS,
        TEMPO_OR_TICKS_OPTIONS[0],
        true,
    );

    // Tracks
    c.activate_track_navigation_setting(global, Some(CATEGORY_TRACKS), true);
    enum_setting(&mut c, INCLUDE_FX_TRACKS, CATEGORY_TRACKS, "Include FX and master tracks in track bank", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[0], false);

    // Assignable buttons
    for (i, name) in ASSIGNABLE_BUTTON_NAMES.iter().enumerate() {
        let default = ASSIGNABLE_VALUES[ASSIGNABLE_BUTTON_DEFAULTS[i]];
        enum_setting(&mut c, ASSIGNABLE_1.offset(i as u16), CATEGORY_ASSIGNABLE_BUTTONS, name, &ASSIGNABLE_VALUES, default, false);
        c.activate_setting(crate::config::SettingDescriptor {
            id: ASSIGNABLE_ACTION_1.offset(i as u16),
            scope: global,
            category: CATEGORY_ASSIGNABLE_BUTTONS.to_string(),
            label: format!("{name} - Action"),
            // A host action (IActionSetting): Bitwig shows its action list, Reaper a text
            kind: crate::config::SettingKind::Action,
            default_value: String::new(),
            observable: false,
        });
    }

    // Transport
    c.activate_behaviour_on_pause_setting(global);
    c.activate_behaviour_on_stop_setting(global);
    c.activate_double_click_play_button(global);
    c.activate_double_click_stop_button(global);
    c.activate_flip_record_setting(global);

    // Play and Sequence
    c.activate_quantize_amount_setting(global);

    // Workflow
    c.activate_exclude_deactivated_items_setting(global);
    c.activate_startup_mode_setting(global, &STARTUP_MODES);
    c.activate_new_clip_length_setting(global);
    enum_setting(&mut c, ZOOM_STATE, CATEGORY_WORKFLOW, "Zoom", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[0], true);
    enum_setting(&mut c, TOUCH_SELECTS_CHANNEL, CATEGORY_WORKFLOW, "Select Channel on Fader Touch", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[1], true);
    enum_setting(&mut c, TOUCH_CHANNEL_VOLUME_MODE, CATEGORY_WORKFLOW, "Activate Volume mode on Fader Touch", &ON_OFF_OPTIONS, ON_OFF_OPTIONS[1], true);
    c.activate_knob_speed_setting(global, 100, 60, false);
    c.activate_encoder_knob_speed_setting(global);
    c
}

/// Create the configuration with the Reaper arpeggiator modes.
pub fn for_reaper(num_devices: usize) -> Configuration {
    create(num_devices, &super::values::REAPER_ARP_MODES)
}

/// Apply the selected profile to the hardware settings and reset the profile selection (former
/// observer of the profile setting). Returns true if a profile was applied.
pub fn apply_profile(c: &mut Configuration) -> bool {
    let Some(profile) = c.value(PROFILE) else {
        return false;
    };
    // main, second, segment, assignment, motor faders, 1 fader, track names, vertical zoom,
    // faders as knobs, always send VU, VU meter style, display colors
    type Profile = (usize, usize, bool, bool, bool, bool, bool, bool, bool, bool, usize, usize);
    let values: Profile = match profile.as_str() {
        DEVICE_MACKIE_MCU_PRO => (2, 0, true, true, true, false, true, false, false, false, 3, 0),
        DEVICE_ASPARION_D700 => (1, 1, false, false, true, false, true, false, false, false, 1, 1),
        DEVICE_BEHRINGER_X_TOUCH => (3, 0, true, true, true, false, true, false, false, false, 3, 3),
        DEVICE_BEHRINGER_X_TOUCH_ONE => (3, 0, true, true, true, true, true, false, false, false, 3, 0),
        DEVICE_ICON_P1_M => (3, 0, true, false, true, false, true, false, false, false, 2, 4),
        DEVICE_ICON_P1_NANO => (3, 0, true, false, true, true, true, false, false, false, 2, 4),
        DEVICE_ICON_PLATFORM_M => (0, 0, false, false, true, false, false, true, false, false, 0, 0),
        DEVICE_ICON_QCON_PRO_X => (2, 2, true, false, true, false, false, false, false, false, 2, 0),
        DEVICE_ICON_QCON_V1M => (3, 3, true, false, true, false, false, false, false, false, 2, 4),
        DEVICE_ZOOM_R16 => (0, 0, false, false, false, false, false, false, true, false, 0, 0),
        _ => return false,
    };
    let (main, second, segment, assignment, motor, one_fader, names, vert_zoom, faders_as_knobs, always_vu, vu_style, colors) = values;
    c.set_value(MAIN_DISPLAY, MAIN_DISPLAY_OPTIONS[main]);
    c.set_value(SECOND_DISPLAY, SECOND_DISPLAY_OPTIONS[second]);
    c.set_value(HAS_SEGMENT_DISPLAY, on_off(segment));
    c.set_value(HAS_ASSIGNMENT_DISPLAY, on_off(assignment));
    c.set_value(HAS_MOTOR_FADERS, on_off(motor));
    c.set_value(HAS_ONLY_1_FADER, on_off(one_fader));
    c.set_value(DISPLAY_TRACK_NAMES, on_off(names));
    c.set_value(USE_VERT_ZOOM_FOR_MODES, on_off(vert_zoom));
    c.set_value(USE_FADERS_AS_KNOBS, on_off(faders_as_knobs));
    c.set_value(ALWAYS_SEND_VU_METERS, on_off(always_vu));
    c.set_value(VU_METER_STYLE, VU_METER_STYLES[vu_style]);
    c.set_value(X_TOUCH_DISPLAY_COLORS, DISPLAY_COLORS_OPTIONS[colors]);
    c.set_value(PROFILE, DEVICE_SELECT);
    true
}

fn toggle(c: &mut Configuration, id: SettingId) {
    let on = c.custom_bool(id);
    c.set_value(id, on_off(!on));
}

/// The MCU specific settings of a configuration created by [`create`].
pub trait McuConfig {
    fn is_zoom_state(&self) -> bool;
    fn toggle_zoom_state(&mut self);
    fn is_display_time(&self) -> bool;
    fn toggle_display_time(&mut self);
    fn is_display_ticks(&self) -> bool;
    fn toggle_display_ticks(&mut self);
    fn main_display_type(&self) -> MainDisplay;
    fn second_display_type(&self) -> SecondDisplay;
    fn has_segment_display(&self) -> bool;
    fn has_assignment_display(&self) -> bool;
    fn has_motor_faders(&self) -> bool;
    fn toggle_motor_faders(&mut self);
    fn has_only_1_fader(&self) -> bool;
    fn is_display_track_names(&self) -> bool;
    fn toggle_display_track_names(&mut self);
    fn use_vert_zoom_for_modes(&self) -> bool;
    fn always_send_vu_meters(&self) -> bool;
    /// The VU meters on/off setting (former `areVuMetersEnabled`).
    fn are_vu_meters_enabled(&self) -> bool;
    fn toggle_vu_meters_enabled(&mut self);
    /// The style is not None (former override of `isEnableVUMeters`).
    fn has_vu_meters(&self) -> bool;
    fn vu_meter_style(&self) -> VuMeterStyle;
    fn display_colors(&self) -> DisplayColors;
    fn use_faders_as_knobs(&self) -> bool;
    fn toggle_use_faders_as_knobs(&mut self);
    fn assignable(&self, index: usize) -> i32;
    fn assignable_action(&self, index: usize) -> String;
    fn is_touch_selects_channel(&self) -> bool;
    fn is_touch_channel_volume_mode(&self) -> bool;
    fn should_include_fx_tracks_in_track_bank(&self) -> bool;
    fn should_pin_fx_tracks_to_last_controller(&self) -> bool;
    fn device_type(&self, index: usize) -> McuDeviceType;
}

impl McuConfig for Configuration {
    fn is_zoom_state(&self) -> bool {
        self.custom_bool(ZOOM_STATE)
    }

    fn toggle_zoom_state(&mut self) {
        toggle(self, ZOOM_STATE);
    }

    fn is_display_time(&self) -> bool {
        self.custom_index(DISPLAY_MODE_TIME_OR_BEATS) == 0
    }

    fn toggle_display_time(&mut self) {
        let index = if self.is_display_time() { 1 } else { 0 };
        self.set_value(DISPLAY_MODE_TIME_OR_BEATS, TIME_OR_BEATS_OPTIONS[index]);
    }

    fn is_display_ticks(&self) -> bool {
        self.custom_index(DISPLAY_MODE_TICKS_OR_TEMPO) == 0
    }

    fn toggle_display_ticks(&mut self) {
        let index = if self.is_display_ticks() { 1 } else { 0 };
        self.set_value(DISPLAY_MODE_TICKS_OR_TEMPO, TEMPO_OR_TICKS_OPTIONS[index]);
    }

    fn main_display_type(&self) -> MainDisplay {
        match self.custom_index(MAIN_DISPLAY) {
            1 => MainDisplay::Asparion,
            2 => MainDisplay::Mackie6Characters,
            3 => MainDisplay::Mackie7Characters,
            _ => MainDisplay::None,
        }
    }

    fn second_display_type(&self) -> SecondDisplay {
        match self.custom_index(SECOND_DISPLAY) {
            1 => SecondDisplay::Asparion,
            2 => SecondDisplay::Qcon,
            3 => SecondDisplay::V1m,
            _ => SecondDisplay::None,
        }
    }

    fn has_segment_display(&self) -> bool {
        self.custom_bool(HAS_SEGMENT_DISPLAY)
    }

    fn has_assignment_display(&self) -> bool {
        self.custom_bool(HAS_ASSIGNMENT_DISPLAY)
    }

    fn has_motor_faders(&self) -> bool {
        self.custom_bool(HAS_MOTOR_FADERS)
    }

    fn toggle_motor_faders(&mut self) {
        toggle(self, HAS_MOTOR_FADERS);
    }

    fn has_only_1_fader(&self) -> bool {
        self.custom_bool(HAS_ONLY_1_FADER)
    }

    fn is_display_track_names(&self) -> bool {
        self.custom_bool(DISPLAY_TRACK_NAMES)
    }

    fn toggle_display_track_names(&mut self) {
        toggle(self, DISPLAY_TRACK_NAMES);
    }

    fn use_vert_zoom_for_modes(&self) -> bool {
        self.custom_bool(USE_VERT_ZOOM_FOR_MODES)
    }

    fn always_send_vu_meters(&self) -> bool {
        self.custom_bool(ALWAYS_SEND_VU_METERS)
    }

    fn are_vu_meters_enabled(&self) -> bool {
        self.custom_bool(SettingId::ENABLE_VU_METERS)
    }

    fn toggle_vu_meters_enabled(&mut self) {
        toggle(self, SettingId::ENABLE_VU_METERS);
    }

    fn has_vu_meters(&self) -> bool {
        self.vu_meter_style() != VuMeterStyle::None
    }

    fn vu_meter_style(&self) -> VuMeterStyle {
        match self.custom_index(VU_METER_STYLE) {
            1 => VuMeterStyle::Asparion,
            2 => VuMeterStyle::Icon,
            3 => VuMeterStyle::Mackie,
            _ => VuMeterStyle::None,
        }
    }

    fn display_colors(&self) -> DisplayColors {
        match self.custom_index(X_TOUCH_DISPLAY_COLORS) {
            1 => DisplayColors::Asparion,
            2 => DisplayColors::Behringer122,
            3 => DisplayColors::Behringer125,
            4 => DisplayColors::Icon,
            _ => DisplayColors::None,
        }
    }

    fn use_faders_as_knobs(&self) -> bool {
        self.custom_bool(USE_FADERS_AS_KNOBS)
    }

    fn toggle_use_faders_as_knobs(&mut self) {
        toggle(self, USE_FADERS_AS_KNOBS);
    }

    fn assignable(&self, index: usize) -> i32 {
        self.custom_index(ASSIGNABLE_1.offset(index as u16)) as i32
    }

    fn assignable_action(&self, index: usize) -> String {
        self.value(ASSIGNABLE_ACTION_1.offset(index as u16)).unwrap_or_default()
    }

    fn is_touch_selects_channel(&self) -> bool {
        self.custom_bool(TOUCH_SELECTS_CHANNEL)
    }

    fn is_touch_channel_volume_mode(&self) -> bool {
        self.custom_bool(TOUCH_CHANNEL_VOLUME_MODE)
    }

    fn should_include_fx_tracks_in_track_bank(&self) -> bool {
        self.custom_bool(INCLUDE_FX_TRACKS)
    }

    fn should_pin_fx_tracks_to_last_controller(&self) -> bool {
        self.custom_bool(PIN_FXTRACKS_TO_LAST_CONTROLLER)
    }

    fn device_type(&self, index: usize) -> McuDeviceType {
        match self.custom_index(DEVICE_TYPE_1.offset(index as u16)) {
            0 => McuDeviceType::Main,
            2 => McuDeviceType::MackieExtender,
            _ => McuDeviceType::Extender,
        }
    }
}

/// True if the setting is one of the assignable functions (Java notifies `ASSIGNABLE_BUTTONS`).
pub fn is_assignable_setting(id: SettingId) -> bool {
    (ASSIGNABLE_1.0..ASSIGNABLE_1.0 + 10).contains(&id.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Properties;

    #[test]
    fn assignable_actions_are_host_actions() {
        let c = for_reaper(1);
        let descriptor = c.descriptor(ASSIGNABLE_ACTION_1.offset(3)).unwrap();
        assert_eq!(descriptor.kind, crate::config::SettingKind::Action);
        assert_eq!(descriptor.label, "F2 - Action");
    }

    #[test]
    fn defaults() {
        let c = for_reaper(2);
        assert_eq!(c.main_display_type(), MainDisplay::Mackie6Characters);
        assert_eq!(c.second_display_type(), SecondDisplay::None);
        assert!(c.has_segment_display() && c.has_assignment_display() && c.has_motor_faders());
        assert!(!c.has_only_1_fader() && !c.is_display_track_names() && !c.use_faders_as_knobs());
        assert!(c.are_vu_meters_enabled());
        assert!(!c.has_vu_meters());
        assert_eq!(c.vu_meter_style(), VuMeterStyle::None);
        assert_eq!(c.display_colors(), DisplayColors::None);
        assert_eq!(c.device_type(0), McuDeviceType::Extender);
        assert_eq!(c.device_type(1), McuDeviceType::Main);
        assert!(c.is_display_time() && c.is_display_ticks());
        assert!(c.is_touch_selects_channel() && c.is_touch_channel_volume_mode());
        assert_eq!(c.assignable(0), 0);
        assert_eq!(c.assignable(2), 11);
        assert_eq!(c.assignable(9), FOOTSWITCH_DEVICE_ON_OFF);
        assert_eq!(c.assignable_action(3), "");
        assert_eq!(c.startup_mode(), Modes::Pan);
        assert!(c.is_track_navigation_flat());
        assert!(!c.is_setting_active(PIN_FXTRACKS_TO_LAST_CONTROLLER));
        assert!(!c.is_setting_active(DEVICE_TYPE_1.offset(2)));
        assert!(for_reaper(4).is_setting_active(DEVICE_TYPE_1.offset(3)));
    }

    #[test]
    fn profiles() {
        let mut c = for_reaper(1);
        c.take_changed_settings();
        assert!(!apply_profile(&mut c));
        c.set_value(PROFILE, DEVICE_BEHRINGER_X_TOUCH);
        assert!(apply_profile(&mut c));
        assert_eq!(c.main_display_type(), MainDisplay::Mackie7Characters);
        assert_eq!(c.vu_meter_style(), VuMeterStyle::Mackie);
        assert_eq!(c.display_colors(), DisplayColors::Behringer125);
        assert!(c.is_display_track_names());
        assert_eq!(c.value(PROFILE).as_deref(), Some(DEVICE_SELECT));
        assert!(c.take_changed_settings().contains(&MAIN_DISPLAY));
        assert_eq!(c.device_type(0), McuDeviceType::Main);
    }

    #[test]
    fn toggles_and_persistence() {
        let mut c = for_reaper(3);
        c.toggle_zoom_state();
        c.toggle_display_ticks();
        c.toggle_vu_meters_enabled();
        c.set_value(ASSIGNABLE_ACTION_1.offset(4), "40044");
        assert!(c.is_zoom_state() && !c.is_display_ticks() && !c.are_vu_meters_enabled());
        let mut properties = Properties::new();
        c.store(Scope::Global, &mut properties);
        assert_eq!(properties.get_string("WORKFLOW_ZOOM"), Some("On"));
        assert_eq!(properties.get_string("HARDWARE_SETUP_VU_METER_TYPE"), Some("None"));
        assert_eq!(properties.get_string("HARDWARE_SETUP_VU_METERS"), Some("Off"));
        assert_eq!(properties.get_string("EXTENDER_SETUP_(REQUIRES_RESTART)_MCU_DEVICE_3_-_RIGHT"), Some("Main"));
        assert_eq!(properties.get_string("ASSIGNABLE_BUTTONS_F3_-_ACTION"), Some("40044"));
        assert_eq!(properties.get_string("SEGMENT_DISPLAY_DISPLAY_TEMPO_OR_TICKS/MILLISECONDS"), Some("Tempo"));
        let mut loaded = for_reaper(3);
        loaded.load(Scope::Global, &properties);
        assert!(loaded.is_zoom_state());
        assert_eq!(loaded.assignable_action(4), "40044");
        assert!(is_assignable_setting(ASSIGNABLE_1.offset(9)));
        assert!(!is_assignable_setting(ASSIGNABLE_ACTION_1));
    }
}
