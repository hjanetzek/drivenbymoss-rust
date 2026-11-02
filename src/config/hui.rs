// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Mackie HUI protocol (former `HUIConfiguration`).
//!
//! The settings are stored with the same keys (category + label) as in Java. The profile and
//! the assignable functions have no Java ID; they got IDs and are not observable, as in Java.
//! The "Send ping" setting notifies its own ID (Java notifies `HAS_MOTOR_FADERS`, which nobody
//! observes).

use super::configuration::{Configuration, CATEGORY_HARDWARE_SETUP, CATEGORY_WORKFLOW};
use super::setting::{Scope, SettingId};
use super::values::{to_options, ArpeggiatorMode, FOOTSWITCH_VALUES, ON_OFF_OPTIONS};

pub const ZOOM_STATE: SettingId = SettingId::NEXT_SETTING_ID;
pub const HAS_DISPLAY1: SettingId = SettingId::NEXT_SETTING_ID.offset(1);
pub const SEND_PING: SettingId = SettingId::NEXT_SETTING_ID.offset(2);
pub const HAS_SEGMENT_DISPLAY: SettingId = SettingId::NEXT_SETTING_ID.offset(3);
pub const HAS_MOTOR_FADERS: SettingId = SettingId::NEXT_SETTING_ID.offset(4);
pub const TOUCH_CHANNEL: SettingId = SettingId::NEXT_SETTING_ID.offset(5);
/// The device profile, which sets several hardware settings at once.
pub const PROFILE: SettingId = SettingId::NEXT_SETTING_ID.offset(6);
/// The function of the assignable button with index 0 (+1 .. +9 for the others).
pub const ASSIGNABLE_1: SettingId = SettingId::NEXT_SETTING_ID.offset(10);

const DEVICE_SELECT: &str = "<Select a profile>";
const DEVICE_ICON_QCON_PRO_X: &str = "icon QConPro X";
const DEVICE_MACKIE_HUI: &str = "Mackie HUI";
const DEVICE_NOVATION_SLMKIII: &str = "Novation MkIII";
const DEVICE_YAMAHA_DM3: &str = "Yamaha DM3";

pub const DEVICE_OPTIONS: [&str; 5] = [DEVICE_SELECT, DEVICE_ICON_QCON_PRO_X, DEVICE_MACKIE_HUI, DEVICE_NOVATION_SLMKIII, DEVICE_YAMAHA_DM3];

pub const ASSIGNABLE_BUTTON_NAMES: [&str; 10] = ["Footswitch 1", "Footswitch 2", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8"];

/// Create the HUI configuration with all settings activated (former `HUIConfiguration.init`).
pub fn create(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut c = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;
    let on_off = |c: &mut Configuration, id: SettingId, category: &str, label: &str, default_on: bool| {
        c.activate_enum_setting(id, global, category, label, to_options(&ON_OFF_OPTIONS), ON_OFF_OPTIONS[usize::from(default_on)], true);
    };

    // Hardware
    c.activate_enum_setting(PROFILE, global, CATEGORY_HARDWARE_SETUP, "Profile", to_options(&DEVICE_OPTIONS), DEVICE_OPTIONS[0], false);
    on_off(&mut c, HAS_DISPLAY1, CATEGORY_HARDWARE_SETUP, "Has a display", true);
    on_off(&mut c, HAS_SEGMENT_DISPLAY, CATEGORY_HARDWARE_SETUP, "Has a position/tempo display", true);
    on_off(&mut c, HAS_MOTOR_FADERS, CATEGORY_HARDWARE_SETUP, "Has motor faders", true);
    on_off(&mut c, SEND_PING, CATEGORY_HARDWARE_SETUP, "Send ping", true);
    c.activate_enable_vu_meters_setting(global, CATEGORY_HARDWARE_SETUP);

    // Assignable buttons
    for (i, name) in ASSIGNABLE_BUTTON_NAMES.iter().enumerate() {
        c.activate_enum_setting(ASSIGNABLE_1.offset(i as u16), global, "Assignable buttons", name, to_options(&FOOTSWITCH_VALUES), FOOTSWITCH_VALUES[6], false);
    }

    // Transport
    c.activate_behaviour_on_pause_setting(global);
    c.activate_behaviour_on_stop_setting(global);
    c.activate_double_click_play_button(global);
    c.activate_double_click_stop_button(global);

    // Workflow
    c.activate_exclude_deactivated_items_setting(global);
    on_off(&mut c, ZOOM_STATE, CATEGORY_WORKFLOW, "Zoom", false);
    on_off(&mut c, TOUCH_CHANNEL, CATEGORY_WORKFLOW, "Select Channel on Fader Touch", true);
    c.activate_new_clip_length_setting(global);
    c
}

/// Create the configuration with the Reaper arpeggiator modes.
pub fn for_reaper() -> Configuration {
    create(&super::values::REAPER_ARP_MODES)
}

/// Apply the selected profile and reset the profile selection (former observer of the profile
/// setting). Returns true if a profile was applied.
pub fn apply_profile(c: &mut Configuration) -> bool {
    let on = |b: bool| ON_OFF_OPTIONS[usize::from(b)];
    let (display, segment, motor, vu) = match c.value(PROFILE).as_deref() {
        Some(DEVICE_MACKIE_HUI | DEVICE_ICON_QCON_PRO_X | DEVICE_YAMAHA_DM3) => (true, true, true, true),
        Some(DEVICE_NOVATION_SLMKIII) => (true, false, false, false),
        _ => return false,
    };
    c.set_value(HAS_DISPLAY1, on(display));
    c.set_value(HAS_SEGMENT_DISPLAY, on(segment));
    c.set_value(HAS_MOTOR_FADERS, on(motor));
    c.set_vu_meters_enabled(vu);
    c.set_value(PROFILE, DEVICE_SELECT);
    true
}

/// The HUI specific settings of a configuration created by [`create`].
pub trait HuiConfig {
    fn is_zoom_state(&self) -> bool;
    fn toggle_zoom_state(&mut self);
    fn has_display1(&self) -> bool;
    fn has_segment_display(&self) -> bool;
    fn has_motor_faders(&self) -> bool;
    fn should_send_ping(&self) -> bool;
    fn assignable(&self, index: usize) -> i32;
    fn is_touch_channel(&self) -> bool;
}

impl HuiConfig for Configuration {
    fn is_zoom_state(&self) -> bool {
        self.custom_bool(ZOOM_STATE)
    }

    fn toggle_zoom_state(&mut self) {
        let on = self.is_zoom_state();
        self.set_value(ZOOM_STATE, ON_OFF_OPTIONS[usize::from(!on)]);
    }

    fn has_display1(&self) -> bool {
        self.custom_bool(HAS_DISPLAY1)
    }

    fn has_segment_display(&self) -> bool {
        self.custom_bool(HAS_SEGMENT_DISPLAY)
    }

    fn has_motor_faders(&self) -> bool {
        self.custom_bool(HAS_MOTOR_FADERS)
    }

    fn should_send_ping(&self) -> bool {
        self.custom_bool(SEND_PING)
    }

    fn assignable(&self, index: usize) -> i32 {
        self.custom_index(ASSIGNABLE_1.offset(index as u16)) as i32
    }

    fn is_touch_channel(&self) -> bool {
        self.custom_bool(TOUCH_CHANNEL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Properties;

    #[test]
    fn defaults_and_profiles() {
        let mut c = for_reaper();
        assert!(c.has_display1() && c.has_segment_display() && c.has_motor_faders() && c.should_send_ping() && c.is_touch_channel());
        assert!(!c.is_zoom_state());
        assert!(c.is_enable_vu_meters());
        assert_eq!(c.assignable(3), 6);
        c.set_value(PROFILE, DEVICE_NOVATION_SLMKIII);
        assert!(apply_profile(&mut c));
        assert!(!c.has_segment_display() && !c.has_motor_faders() && !c.is_enable_vu_meters());
        assert_eq!(c.value(PROFILE).as_deref(), Some(DEVICE_SELECT));
        assert!(!apply_profile(&mut c));

        c.toggle_zoom_state();
        let mut properties = Properties::new();
        c.store(Scope::Global, &mut properties);
        assert_eq!(properties.get_string("WORKFLOW_ZOOM"), Some("On"));
        assert_eq!(properties.get_string("HARDWARE_SETUP_HAS_A_POSITION/TEMPO_DISPLAY"), Some("Off"));
        assert_eq!(properties.get_string("HARDWARE_SETUP_VU_METERS"), Some("Off"));
        assert_eq!(properties.get_string("ASSIGNABLE_BUTTONS_F8"), Some("New Button"));
        let mut loaded = for_reaper();
        loaded.load(Scope::Global, &properties);
        assert!(loaded.is_zoom_state() && !loaded.has_motor_faders());
    }
}
