// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Akai APC mini (former `APCminiConfiguration`).

use super::configuration::{Configuration, CATEGORY_HARDWARE_SETUP};
use super::setting::{Scope, SettingId};
use super::values::{ArpeggiatorMode, REAPER_ARP_MODES};
use crate::framework::ids::Views;

/// Setting for the control of the faders.
pub const FADER_CTRL: SettingId = SettingId::NEXT_SETTING_ID;
/// Setting for the function of the track buttons (soft keys).
pub const SOFT_KEYS: SettingId = SettingId::NEXT_SETTING_ID.offset(1);
/// Setting for the brightness of the pads (Mk2 only).
pub const PAD_BRIGHTNESS: SettingId = SettingId::NEXT_SETTING_ID.offset(2);

/// The options of the fader control setting.
pub const FADER_CTRL_OPTIONS: [&str; 11] = ["Volume", "Pan", "Send 1", "Send 2", "Send 3", "Send 4", "Send 5", "Send 6", "Send 7", "Send 8", "Device"];
/// The options of the pad brightness setting.
pub const BRIGHTNESS_OPTIONS: [&str; 7] = ["10%", "25%", "50%", "65%", "75%", "90%", "100%"];
/// The options of the soft keys setting.
pub const SOFT_KEYS_OPTIONS: [&str; 5] = ["Clip Stop", "Solo", "Rec Arm", "Mute", "Select"];

/// The default pad brightness index.
const DEFAULT_PAD_BRIGHTNESS: usize = 4;

/// The views which can be selected as the startup view.
pub const PREFERRED_NOTE_VIEWS: [Views; 4] = [Views::Play, Views::Drum, Views::Sequencer, Views::Raindrops];

fn options(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

/// Create the configuration and activate all settings (the former `init`).
pub fn create(has_brightness: bool, arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut base = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;
    let document = Scope::Document;

    // Hardware
    if has_brightness {
        // Java does not add the setting to the observable ones but notifies its observers
        base.activate_enum_setting(
            PAD_BRIGHTNESS,
            global,
            CATEGORY_HARDWARE_SETUP,
            "Pad Brightness",
            options(&BRIGHTNESS_OPTIONS),
            BRIGHTNESS_OPTIONS[DEFAULT_PAD_BRIGHTNESS],
            false,
        );
    }

    // Scale
    base.activate_scale_setting(document);
    base.activate_scale_base_setting(document);
    base.activate_scale_in_scale_setting(document);
    base.activate_scale_layout_setting(document);

    // Play and Sequence
    base.activate_quantize_amount_setting(global);
    base.activate_startup_view_setting(global, &PREFERRED_NOTE_VIEWS);
    base.activate_start_with_session_view_setting(global);
    base.activate_show_played_chords_setting(global);

    // Workflow
    base.activate_exclude_deactivated_items_setting(global);
    base.activate_behaviour_on_pause_setting(global);
    base.activate_select_clip_on_launch_setting(global);
    base.activate_new_clip_length_setting(global);

    // Button Control
    base.activate_enum_setting(FADER_CTRL, global, "Button Control", "Fader Ctrl", options(&FADER_CTRL_OPTIONS), FADER_CTRL_OPTIONS[0], true);
    base.activate_enum_setting(SOFT_KEYS, global, "Button Control", "Soft Keys", options(&SOFT_KEYS_OPTIONS), SOFT_KEYS_OPTIONS[0], true);

    base
}

/// Create the configuration with the Reaper arpeggiator modes.
pub fn for_reaper(has_brightness: bool) -> Configuration {
    create(has_brightness, &REAPER_ARP_MODES)
}

/// The selected fader control option (former `getFaderCtrl`).
pub fn fader_ctrl(config: &Configuration) -> String {
    config.value(FADER_CTRL).unwrap_or_else(|| FADER_CTRL_OPTIONS[0].to_string())
}

/// Former `setFaderCtrl`.
pub fn set_fader_ctrl(config: &mut Configuration, fader_ctrl: &str) {
    config.set_value(FADER_CTRL, fader_ctrl);
}

/// The selected soft keys option (former `getSoftKeys`).
pub fn soft_keys(config: &Configuration) -> String {
    config.value(SOFT_KEYS).unwrap_or_else(|| SOFT_KEYS_OPTIONS[0].to_string())
}

/// Former `setSoftKeys`.
pub fn set_soft_keys(config: &mut Configuration, soft_keys: &str) {
    config.set_value(SOFT_KEYS, soft_keys);
}

/// The index of the pad brightness option (former `getPadBrightness`), 4 if not supported.
pub fn pad_brightness(config: &Configuration) -> i32 {
    if config.is_setting_active(PAD_BRIGHTNESS) {
        config.custom_index(PAD_BRIGHTNESS) as i32
    } else {
        DEFAULT_PAD_BRIGHTNESS as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::properties::Properties;

    #[test]
    fn defaults() {
        let c = for_reaper(true);
        assert_eq!(fader_ctrl(&c), "Volume");
        assert_eq!(soft_keys(&c), "Clip Stop");
        assert_eq!(pad_brightness(&c), 4);
        assert!(c.can_setting_be_observed(FADER_CTRL));
        assert!(c.can_setting_be_observed(SOFT_KEYS));
        assert!(!c.can_setting_be_observed(PAD_BRIGHTNESS));
        assert_eq!(c.preferred_note_view(), Views::Play);
        assert!(c.is_setting_active(SettingId::SELECT_CLIP_ON_LAUNCH));

        let mk1 = for_reaper(false);
        assert!(!mk1.is_setting_active(PAD_BRIGHTNESS));
        assert_eq!(pad_brightness(&mk1), 4);
    }

    #[test]
    fn persistence() {
        let mut c = for_reaper(true);
        set_fader_ctrl(&mut c, "Send 3");
        set_soft_keys(&mut c, "Mute");
        c.set_value(PAD_BRIGHTNESS, "100%");
        c.set_value(SettingId::STARTUP_VIEW, "Raindrop");
        assert_eq!(c.take_changed_settings(), vec![FADER_CTRL, SOFT_KEYS, PAD_BRIGHTNESS, SettingId::STARTUP_VIEW]);

        let mut props = Properties::new();
        c.store(Scope::Global, &mut props);
        assert_eq!(props.get("BUTTON_CONTROL_FADER_CTRL"), Some("Send 3"));
        assert_eq!(props.get("BUTTON_CONTROL_SOFT_KEYS"), Some("Mute"));
        assert_eq!(props.get("HARDWARE_SETUP_PAD_BRIGHTNESS"), Some("100%"));

        let mut d = for_reaper(true);
        d.load(Scope::Global, &Properties::parse(&props.to_text()));
        assert_eq!(fader_ctrl(&d), "Send 3");
        assert_eq!(soft_keys(&d), "Mute");
        assert_eq!(pad_brightness(&d), 6);
        assert_eq!(d.startup_view(), Views::Raindrops);
    }
}
