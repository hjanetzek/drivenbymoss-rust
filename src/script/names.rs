// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The names of the IDs as the scripts see them: the buttons, continuous controls, modes and
//! views by the names of their Rust enum variants (`"Row11"`, `"Fader1"`, `"EqDeviceParams"`),
//! the other enums in lower case (`"cc"`, `"absolute_knob"`, `"down"`), the settings by the names
//! of their [`SettingId`] constants.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::ctx_stack::{ScriptError, ScriptResult};
use crate::config::{Configuration, Scope, SettingId};
use crate::daw::constants::DeviceId;
use crate::daw::events::ParameterBankId;
use crate::framework::commands::cursor::Direction;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::surface::ContinuousKind;
use crate::midi::{BindType, ButtonEvent};

fn unknown<T>(kind: &str, name: &str) -> ScriptResult<T> {
    Err(ScriptError::new(format!("unknown {kind} '{name}'")))
}

/// The IDs of an enum by name (the scripts look them up on each call, e.g. in the lights).
fn by_name<T: Copy + Send + Sync + 'static>(cell: &'static OnceLock<HashMap<&'static str, T>>, all: &'static [T], name_of: fn(T) -> &'static str) -> &'static HashMap<&'static str, T> {
    cell.get_or_init(|| all.iter().map(|id| (name_of(*id), *id)).collect())
}

pub fn button(name: &str) -> ScriptResult<ButtonId> {
    static NAMES: OnceLock<HashMap<&'static str, ButtonId>> = OnceLock::new();
    by_name(&NAMES, ButtonId::ALL, ButtonId::name).get(name).copied().map_or_else(|| unknown("button", name), Ok)
}

pub fn continuous(name: &str) -> ScriptResult<ContinuousId> {
    static NAMES: OnceLock<HashMap<&'static str, ContinuousId>> = OnceLock::new();
    by_name(&NAMES, ContinuousId::ALL, ContinuousId::name).get(name).copied().map_or_else(|| unknown("continuous control", name), Ok)
}

pub fn mode(name: &str) -> ScriptResult<Modes> {
    static NAMES: OnceLock<HashMap<&'static str, Modes>> = OnceLock::new();
    by_name(&NAMES, Modes::ALL, Modes::name).get(name).copied().map_or_else(|| unknown("mode", name), Ok)
}

pub fn view(name: &str) -> ScriptResult<Views> {
    static NAMES: OnceLock<HashMap<&'static str, Views>> = OnceLock::new();
    by_name(&NAMES, Views::ALL, Views::name).get(name).copied().map_or_else(|| unknown("view", name), Ok)
}

pub fn bind_type(name: &str) -> ScriptResult<BindType> {
    match name {
        "cc" => Ok(BindType::Cc),
        "note" => Ok(BindType::Note),
        "pitchbend" => Ok(BindType::PitchBend),
        "program_change" => Ok(BindType::ProgramChange),
        _ => unknown("bind type", name),
    }
}

pub fn continuous_kind(name: &str) -> ScriptResult<ContinuousKind> {
    match name {
        "fader" => Ok(ContinuousKind::Fader),
        "absolute_knob" => Ok(ContinuousKind::AbsoluteKnob),
        "relative_knob" => Ok(ContinuousKind::RelativeKnob),
        _ => unknown("continuous kind", name),
    }
}

pub fn button_event(name: &str) -> ScriptResult<ButtonEvent> {
    match name {
        "down" => Ok(ButtonEvent::Down),
        "up" => Ok(ButtonEvent::Up),
        "long" => Ok(ButtonEvent::Long),
        _ => unknown("button event", name),
    }
}

pub fn button_event_name(event: ButtonEvent) -> &'static str {
    match event {
        ButtonEvent::Down => "down",
        ButtonEvent::Up => "up",
        ButtonEvent::Long => "long",
    }
}

pub fn direction(name: &str) -> ScriptResult<Direction> {
    match name {
        "left" => Ok(Direction::Left),
        "right" => Ok(Direction::Right),
        "up" => Ok(Direction::Up),
        "down" => Ok(Direction::Down),
        _ => unknown("direction", name),
    }
}

pub const PARAMETER_BANKS: [(&str, ParameterBankId); 6] = [
    ("CursorDevice", ParameterBankId::CursorDevice),
    ("FirstInstrument", ParameterBankId::FirstInstrument),
    ("Eq", ParameterBankId::Eq),
    ("DrumDevice", ParameterBankId::DrumDevice),
    ("CursorTrack", ParameterBankId::CursorTrack),
    ("Project", ParameterBankId::Project),
];

pub fn parameter_bank(name: &str) -> ScriptResult<ParameterBankId> {
    PARAMETER_BANKS.iter().find(|(n, _)| *n == name).map_or_else(|| unknown("parameter bank", name), |(_, id)| Ok(*id))
}

pub fn device(name: &str) -> ScriptResult<DeviceId> {
    match name {
        "FirstInstrument" => Ok(DeviceId::FirstInstrument),
        "Eq" => Ok(DeviceId::Eq),
        "NiKomplete" => Ok(DeviceId::NiKomplete),
        _ => unknown("device", name),
    }
}

pub fn scope(name: &str) -> ScriptResult<Scope> {
    match name {
        "global" => Ok(Scope::Global),
        "document" => Ok(Scope::Document),
        _ => unknown("setting scope", name),
    }
}

/// The settings by the names of their constants.
pub const SETTINGS: &[(&str, SettingId)] = &[
    ("SCALES_SCALE", SettingId::SCALES_SCALE),
    ("SCALES_BASE", SettingId::SCALES_BASE),
    ("SCALES_IN_KEY", SettingId::SCALES_IN_KEY),
    ("SCALES_LAYOUT", SettingId::SCALES_LAYOUT),
    ("VU_METER_TYPE", SettingId::VU_METER_TYPE),
    ("ENABLE_VU_METERS", SettingId::ENABLE_VU_METERS),
    ("BEHAVIOUR_ON_STOP", SettingId::BEHAVIOUR_ON_STOP),
    ("BEHAVIOUR_ON_PAUSE", SettingId::BEHAVIOUR_ON_PAUSE),
    ("FLIP_SESSION", SettingId::FLIP_SESSION),
    ("SELECT_CLIP_ON_LAUNCH", SettingId::SELECT_CLIP_ON_LAUNCH),
    ("DRAW_RECORD_STRIPE", SettingId::DRAW_RECORD_STRIPE),
    ("CONVERT_AFTERTOUCH", SettingId::CONVERT_AFTERTOUCH),
    ("ACTIVATE_FIXED_ACCENT", SettingId::ACTIVATE_FIXED_ACCENT),
    ("FIXED_ACCENT_VALUE", SettingId::FIXED_ACCENT_VALUE),
    ("QUANTIZE_AMOUNT", SettingId::QUANTIZE_AMOUNT),
    ("FLIP_RECORD", SettingId::FLIP_RECORD),
    ("NEW_CLIP_LENGTH", SettingId::NEW_CLIP_LENGTH),
    ("AUTO_SELECT_DRUM", SettingId::AUTO_SELECT_DRUM),
    ("TURN_OFF_EMPTY_DRUM_PADS", SettingId::TURN_OFF_EMPTY_DRUM_PADS),
    ("TURN_OFF_SCALE_PADS", SettingId::TURN_OFF_SCALE_PADS),
    ("SOUND_DRUM_PADS", SettingId::SOUND_DRUM_PADS),
    ("ACTION_FOR_REC_ARMED_PAD", SettingId::ACTION_FOR_REC_ARMED_PAD),
    ("BROWSER_DISPLAY_FILTER1", SettingId::BROWSER_DISPLAY_FILTER1),
    ("BROWSER_DISPLAY_FILTER2", SettingId::BROWSER_DISPLAY_FILTER2),
    ("BROWSER_DISPLAY_FILTER3", SettingId::BROWSER_DISPLAY_FILTER3),
    ("BROWSER_DISPLAY_FILTER4", SettingId::BROWSER_DISPLAY_FILTER4),
    ("BROWSER_DISPLAY_FILTER5", SettingId::BROWSER_DISPLAY_FILTER5),
    ("BROWSER_DISPLAY_FILTER6", SettingId::BROWSER_DISPLAY_FILTER6),
    ("BROWSER_DISPLAY_FILTER7", SettingId::BROWSER_DISPLAY_FILTER7),
    ("BROWSER_DISPLAY_FILTER8", SettingId::BROWSER_DISPLAY_FILTER8),
    ("KNOB_SENSITIVITY_DEFAULT", SettingId::KNOB_SENSITIVITY_DEFAULT),
    ("KNOB_SENSITIVITY_SLOW", SettingId::KNOB_SENSITIVITY_SLOW),
    ("NOTEREPEAT_ACTIVE", SettingId::NOTEREPEAT_ACTIVE),
    ("NOTEREPEAT_PERIOD", SettingId::NOTEREPEAT_PERIOD),
    ("NOTEREPEAT_LENGTH", SettingId::NOTEREPEAT_LENGTH),
    ("NOTEREPEAT_MODE", SettingId::NOTEREPEAT_MODE),
    ("NOTEREPEAT_OCTAVE", SettingId::NOTEREPEAT_OCTAVE),
    ("MIDI_EDIT_CHANNEL", SettingId::MIDI_EDIT_CHANNEL),
    ("EXCLUDE_DEACTIVATED_ITEMS", SettingId::EXCLUDE_DEACTIVATED_ITEMS),
    ("RECORD_BUTTON_FUNCTION", SettingId::RECORD_BUTTON_FUNCTION),
    ("SHIFTED_RECORD_BUTTON_FUNCTION", SettingId::SHIFTED_RECORD_BUTTON_FUNCTION),
    ("HIERARCHICAL_TRACKS", SettingId::HIERARCHICAL_TRACKS),
    ("FOOTSWITCH_1", SettingId::FOOTSWITCH_1),
    ("FOOTSWITCH_2", SettingId::FOOTSWITCH_2),
    ("FOOTSWITCH_3", SettingId::FOOTSWITCH_3),
    ("FOOTSWITCH_4", SettingId::FOOTSWITCH_4),
    ("STARTUP_VIEW", SettingId::STARTUP_VIEW),
    ("STARTUP_MODE", SettingId::STARTUP_MODE),
    ("START_WITH_SESSION_VIEW", SettingId::START_WITH_SESSION_VIEW),
    ("ENABLED_MPE_ZONES", SettingId::ENABLED_MPE_ZONES),
    ("MPE_PITCHBEND_RANGE", SettingId::MPE_PITCHBEND_RANGE),
    ("COLOR_TRACK_STATES", SettingId::COLOR_TRACK_STATES),
    ("ENCODER_KNOB_SLOW_DOWN", SettingId::ENCODER_KNOB_SLOW_DOWN),
    ("SHOW_PLAYED_CHORDS", SettingId::SHOW_PLAYED_CHORDS),
    ("INCLUDE_MASTER", SettingId::INCLUDE_MASTER),
    ("TRACK_NAVIGATION", SettingId::TRACK_NAVIGATION),
    ("DOUBLE_CLICK_PLAY_BUTTON", SettingId::DOUBLE_CLICK_PLAY_BUTTON),
    ("DOUBLE_CLICK_STOP_BUTTON", SettingId::DOUBLE_CLICK_STOP_BUTTON),
    ("CURSOR_KEYS_TRACK_OPTION", SettingId::CURSOR_KEYS_TRACK_OPTION),
    ("CURSOR_KEYS_TRACK_SHIFTED_OPTION", SettingId::CURSOR_KEYS_TRACK_SHIFTED_OPTION),
    ("CURSOR_KEYS_SCENE_OPTION", SettingId::CURSOR_KEYS_SCENE_OPTION),
    ("CURSOR_KEYS_SCENE_SHIFTED_OPTION", SettingId::CURSOR_KEYS_SCENE_SHIFTED_OPTION),
];

/// The name of a setting, `None` for one of a controller-specific range (e.g. the Flexi
/// settings).
pub fn setting_name(id: SettingId) -> Option<&'static str> {
    SETTINGS.iter().find(|(_, s)| *s == id).map(|(n, _)| *n)
}

type Activator = fn(&mut Configuration, Scope);

/// The generic settings which a script can activate (the `activate_*_setting` functions of the
/// configuration which take only the scope), by name.
pub const SETTING_ACTIVATORS: &[(&str, Activator)] = &[
    ("scale", Configuration::activate_scale_setting),
    ("scale_base", Configuration::activate_scale_base_setting),
    ("scale_in_key", Configuration::activate_scale_in_scale_setting),
    ("scale_layout", Configuration::activate_scale_layout_setting),
    ("behaviour_on_stop", Configuration::activate_behaviour_on_stop_setting),
    ("behaviour_on_pause", Configuration::activate_behaviour_on_pause_setting),
    ("double_click_play", Configuration::activate_double_click_play_button),
    ("double_click_stop", Configuration::activate_double_click_stop_button),
    ("flip_session", Configuration::activate_flip_session_setting),
    ("select_clip_on_launch", Configuration::activate_select_clip_on_launch_setting),
    ("draw_record_stripe", Configuration::activate_draw_record_stripe_setting),
    ("action_for_rec_armed_pad", Configuration::activate_action_for_rec_armed_pad),
    ("convert_aftertouch", Configuration::activate_convert_aftertouch_setting),
    ("accent_active", Configuration::activate_accent_active_setting),
    ("accent_value", Configuration::activate_accent_value_setting),
    ("flip_record", Configuration::activate_flip_record_setting),
    ("include_master", Configuration::activate_include_master_setting),
    ("exclude_deactivated_items", Configuration::activate_exclude_deactivated_items_setting),
    ("new_clip_length", Configuration::activate_new_clip_length_setting),
    ("quantize_amount", Configuration::activate_quantize_amount_setting),
    ("midi_edit_channel", Configuration::activate_midi_edit_channel_setting),
    ("auto_select_drum", Configuration::activate_auto_select_drum_setting),
    ("turn_off_empty_drum_pads", Configuration::activate_turn_off_empty_drum_pads_setting),
    ("turn_off_scale_pads", Configuration::activate_turn_off_scale_pads_setting),
    ("use_combination_button_to_sound", Configuration::activate_use_combination_button_to_sound_setting),
    ("encoder_knob_speed", Configuration::activate_encoder_knob_speed_setting),
    ("record_button", Configuration::activate_record_button_setting),
    ("shifted_record_button", Configuration::activate_shifted_record_button_setting),
    ("start_with_session_view", Configuration::activate_start_with_session_view_setting),
    ("show_played_chords", Configuration::activate_show_played_chords_setting),
    ("color_track_states", Configuration::activate_color_track_states),
    ("cursor_keys", Configuration::activate_cursor_keys_settings),
];

/// Activate a generic setting by name.
pub fn activate_setting(config: &mut Configuration, name: &str, scope: Scope) -> ScriptResult<()> {
    match SETTING_ACTIVATORS.iter().find(|(n, _)| *n == name) {
        Some((_, activate)) => {
            activate(config, scope);
            Ok(())
        }
        None => unknown("setting", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(button("Row11"), Ok(ButtonId::Row11));
        assert!(button("Row99").is_err());
        assert_eq!(continuous("Fader1"), Ok(ContinuousId::Fader1));
        assert_eq!(mode("EqDeviceParams"), Ok(Modes::EqDeviceParams));
        assert_eq!(setting_name(SettingId::SCALES_IN_KEY), Some("SCALES_IN_KEY"));
        assert_eq!(parameter_bank("Project"), Ok(ParameterBankId::Project));
        let mut config = Configuration::new(&crate::config::values::REAPER_ARP_MODES);
        activate_setting(&mut config, "midi_edit_channel", Scope::Global).unwrap();
        assert!(config.is_setting_active(SettingId::MIDI_EDIT_CHANNEL));
        assert!(activate_setting(&mut config, "nothing", Scope::Global).is_err());
    }
}
