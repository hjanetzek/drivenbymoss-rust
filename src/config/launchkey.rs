// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Novation Launchkey Mk3 and Mini Mk3 (former
//! `LaunchkeyMk3Configuration` and `LaunchkeyMiniMk3Configuration`).

use super::configuration::Configuration;
use super::setting::Scope;
use super::values::{ArpeggiatorMode, REAPER_ARP_MODES};
use crate::framework::ids::Modes;

/// The modes of the startup mode setting of the Mini (former `STARTUP_MODES`). The names are
/// the ones of `Modes.getModeName`, which has no name for the user mode.
pub const MINI_STARTUP_MODES: [(Modes, &str); 6] = [
    (Modes::Volume, "Volume"),
    (Modes::Pan, "Panning"),
    (Modes::Send1, "Send 1"),
    (Modes::Send2, "Send 2"),
    (Modes::DeviceParams, "Parameters"),
    (Modes::User, "Missing mode name"),
];

/// Create the configuration of the Mk3 and activate all settings (the former `init`).
pub fn create_mk3(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut c = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;

    // Transport
    c.activate_behaviour_on_stop_setting(global);
    c.activate_behaviour_on_pause_setting(global);
    c.activate_double_click_play_button(global);
    c.activate_double_click_stop_button(global);
    c.activate_record_button_setting(global);
    c.activate_shifted_record_button_setting(global);

    // Workflow
    c.activate_exclude_deactivated_items_setting(global);
    c.activate_include_master_setting(global);
    c.activate_new_clip_length_setting(global);
    c.activate_select_clip_on_launch_setting(global);
    c
}

/// Create the configuration of the Mini Mk3 and activate all settings (the former `init`).
pub fn create_mini(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut c = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;

    // Transport
    c.activate_behaviour_on_pause_setting(global);
    c.activate_record_button_setting(global);
    c.activate_shifted_record_button_setting(global);
    c.activate_new_clip_length_setting(global);

    // Workflow
    c.activate_exclude_deactivated_items_setting(global);
    c.activate_startup_mode_setting(global, &MINI_STARTUP_MODES);
    c.activate_include_master_setting(global);
    c.activate_select_clip_on_launch_setting(global);
    c
}

/// Create the configuration with the Reaper arpeggiator modes.
pub fn for_reaper(is_mini: bool) -> Configuration {
    if is_mini {
        create_mini(&REAPER_ARP_MODES)
    } else {
        create_mk3(&REAPER_ARP_MODES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SettingId;

    #[test]
    fn settings() {
        let mk3 = for_reaper(false);
        assert!(mk3.is_setting_active(SettingId::BEHAVIOUR_ON_STOP));
        assert!(!mk3.is_setting_active(SettingId::STARTUP_MODE));
        let mini = for_reaper(true);
        assert!(mini.is_setting_active(SettingId::STARTUP_MODE));
        assert!(!mini.is_setting_active(SettingId::BEHAVIOUR_ON_STOP));
        assert!(mini.is_setting_active(SettingId::NEW_CLIP_LENGTH));
    }
}
