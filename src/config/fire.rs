// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Akai Fire (former `FireConfiguration`).
//!
//! Differences to Java: the setting "Turn off empty drum pads" requires the drum device
//! capability which Reaper does not have; it is not activated.

use super::configuration::{Configuration, NoteRepeatCapabilities, CATEGORY_HARDWARE_SETUP, CATEGORY_TRANSPORT, CATEGORY_WORKFLOW};
use super::setting::{Scope, SettingId};
use super::values::{ArpeggiatorMode, REAPER_ARP_MODES};
use crate::framework::ids::Views;

pub const PAD_BRIGHTNESS: SettingId = SettingId::NEXT_SETTING_ID;
pub const PAD_SATURATION: SettingId = SettingId::NEXT_SETTING_ID.offset(1);
/// Former `controlLastParamSetting`.
pub const CONTROL_LAST_PARAM: SettingId = SettingId::NEXT_SETTING_ID.offset(2);

const ON_OFF_OPTIONS: [&str; 2] = ["Off", "On"];

/// The views which can be selected as the startup view (former `PREFERRED_NOTE_VIEWS`).
pub const PREFERRED_NOTE_VIEWS: [Views; 6] = [Views::Play, Views::Piano, Views::Drum64, Views::Drum4, Views::Sequencer, Views::PolySequencer];

fn on_off() -> Vec<String> {
    ON_OFF_OPTIONS.iter().map(|s| s.to_string()).collect()
}

/// Create the configuration and activate all settings (the former `init`).
pub fn create(arpeggiator_modes: &[ArpeggiatorMode], note_repeat: NoteRepeatCapabilities) -> Configuration {
    let mut base = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;
    let document = Scope::Document;

    // Scale
    base.activate_scale_setting(document);
    base.activate_scale_base_setting(document);
    base.activate_scale_in_scale_setting(document);
    base.activate_scale_layout_setting(document);

    // Note Repeat
    base.activate_note_repeat_setting(document, note_repeat);

    // Session
    base.activate_select_clip_on_launch_setting(global);
    base.activate_draw_record_stripe_setting(global);
    base.activate_action_for_rec_armed_pad(global);

    // Transport
    base.activate_behaviour_on_pause_setting(global);
    base.activate_behaviour_on_stop_setting(global);
    base.activate_double_click_play_button(global);
    base.activate_double_click_stop_button(global);
    // Corrected label (removed automation)
    base.activate_enum_setting(SettingId::FLIP_RECORD, global, CATEGORY_TRANSPORT, "Flip arranger and clip record", on_off(), ON_OFF_OPTIONS[0], true);

    // Play and Sequence
    base.activate_accent_active_setting(global);
    base.activate_accent_value_setting(global);
    base.activate_quantize_amount_setting(global);
    base.activate_midi_edit_channel_setting(document);
    base.activate_startup_view_setting(global, &PREFERRED_NOTE_VIEWS);
    base.activate_start_with_session_view_setting(global);
    base.activate_turn_off_scale_pads_setting(global);
    base.activate_show_played_chords_setting(global);

    // Drum Sequencer
    base.activate_use_combination_button_to_sound_setting(global);

    // Workflow
    base.activate_enum_setting(CONTROL_LAST_PARAM, global, CATEGORY_WORKFLOW, "Control last touched/clicked parameter with SELECT", on_off(), ON_OFF_OPTIONS[0], false);
    base.activate_exclude_deactivated_items_setting(global);
    base.activate_new_clip_length_setting(global);
    base.activate_knob_speed_setting(global, 100, 60, false);
    base.activate_color_track_states(global);

    // Hardware
    base.activate_range_setting(PAD_BRIGHTNESS, global, CATEGORY_HARDWARE_SETUP, "Pad Brightness", 0, 100, 20, true);
    base.activate_range_setting(PAD_SATURATION, global, CATEGORY_HARDWARE_SETUP, "Pad Saturation", 0, 100, 100, true);
    base
}

/// Create the configuration with the Reaper arpeggiator modes and note repeat capabilities.
pub fn for_reaper() -> Configuration {
    create(&REAPER_ARP_MODES, NoteRepeatCapabilities::REAPER)
}

fn int_value(config: &Configuration, id: SettingId, default_value: i32) -> i32 {
    config.value(id).and_then(|v| v.parse().ok()).unwrap_or(default_value)
}

/// The pad brightness in percent.
pub fn pad_brightness(config: &Configuration) -> i32 {
    int_value(config, PAD_BRIGHTNESS, 20)
}

/// The pad saturation in percent.
pub fn pad_saturation(config: &Configuration) -> i32 {
    int_value(config, PAD_SATURATION, 100)
}

/// Should the SELECT knob control the last touched parameter?
pub fn is_control_last_param(config: &Configuration) -> bool {
    config.value(CONTROL_LAST_PARAM).as_deref() == Some(ON_OFF_OPTIONS[1])
}

pub fn toggle_control_last_param(config: &mut Configuration) {
    let value = ON_OFF_OPTIONS[if is_control_last_param(config) { 0 } else { 1 }];
    config.set_value(CONTROL_LAST_PARAM, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let mut c = for_reaper();
        assert_eq!(pad_brightness(&c), 20);
        assert_eq!(pad_saturation(&c), 100);
        assert!(!is_control_last_param(&c));
        toggle_control_last_param(&mut c);
        assert!(is_control_last_param(&c));
        assert!(!c.is_flip_record());
        assert_eq!(c.preferred_note_view(), Views::Play);
        c.set_value(PAD_BRIGHTNESS, "70");
        assert_eq!(pad_brightness(&c), 70);
        assert!(c.is_setting_active(SettingId::ACTIVATE_FIXED_ACCENT));
    }
}
