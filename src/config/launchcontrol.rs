// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Novation Launch Control XL (former
//! `LaunchControlXLConfiguration`).
//!
//! The runtime state of the Java configuration (the active template and if the device
//! parameters are active on the 3rd knob row) is not persisted and therefore kept in the
//! controller specific state of the surface (`controller::launchcontrol::surface::XlState`).

use super::configuration::Configuration;
use super::setting::Scope;
use super::values::{ArpeggiatorMode, REAPER_ARP_MODES};

/// Create the configuration and activate all settings (the former `init`).
pub fn create_configuration(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut base = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;
    let document = Scope::Document;

    // Scale
    base.activate_scale_setting(document);
    base.activate_scale_base_setting(document);
    base.activate_scale_in_scale_setting(document);

    // Transport
    base.activate_behaviour_on_pause_setting(global);
    base.activate_double_click_play_button(global);

    // Play & Sequence
    base.activate_midi_edit_channel_setting(global);

    // Workflow
    base.activate_exclude_deactivated_items_setting(global);
    base.activate_new_clip_length_setting(global);

    base
}

/// Create the configuration with the Reaper arpeggiator modes.
pub fn for_reaper() -> Configuration {
    create_configuration(&REAPER_ARP_MODES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SettingId;

    #[test]
    fn activated_settings() {
        let c = for_reaper();
        assert!(c.is_setting_active(SettingId::SCALES_SCALE));
        assert!(c.is_setting_active(SettingId::SCALES_BASE));
        assert!(c.is_setting_active(SettingId::SCALES_IN_KEY));
        assert!(!c.is_setting_active(SettingId::SCALES_LAYOUT));
        assert!(c.is_setting_active(SettingId::BEHAVIOUR_ON_PAUSE));
        assert!(c.is_setting_active(SettingId::MIDI_EDIT_CHANNEL));
        assert!(c.is_setting_active(SettingId::EXCLUDE_DEACTIVATED_ITEMS));
        assert!(c.is_setting_active(SettingId::NEW_CLIP_LENGTH));
        assert!(!c.is_setting_active(SettingId::INCLUDE_MASTER));
        assert!(!c.is_setting_active(SettingId::FOOTSWITCH_1));
    }
}
