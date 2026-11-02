// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Akai MPC/Force with the ACVS protocol (former
//! `ACVSConfiguration`).
//!
//! Differences to Java: the Java configuration sets `selectClipOnLaunch` to true without
//! activating the setting; it is not used by the ACVS controllers.

use super::configuration::{Configuration, CATEGORY_HARDWARE_SETUP, CATEGORY_SESSION};
use super::setting::{Scope, SettingId};
use super::values::{ArpeggiatorMode, REAPER_ARP_MODES};
use crate::controller::akai::acvs::definition::AcvsDevice;

/// Setting for launching clips or scenes with the pads.
pub const LAUNCH_CLIPS_OR_SCENES: SettingId = SettingId::NEXT_SETTING_ID;
/// The active ACVS device (not editable, it is set when the device answers the ping).
pub const ACVS_DEVICE: SettingId = SettingId::NEXT_SETTING_ID.offset(1);

const SCENE_CLIPS_OPTIONS: [&str; 2] = ["Clips", "Scenes"];

/// Create the configuration and activate all settings (the former `init`).
pub fn create(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut base = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;

    // Hardware
    let devices: Vec<String> = AcvsDevice::ALL.iter().map(|d| d.name().to_string()).collect();
    base.activate_enum_setting(ACVS_DEVICE, global, CATEGORY_HARDWARE_SETUP, "Device", devices, AcvsDevice::MpcLiveOne.name(), false);
    // Only displayed: the device reports itself
    base.set_enabled(ACVS_DEVICE, false);

    // Session
    let options: Vec<String> = SCENE_CLIPS_OPTIONS.iter().map(|s| s.to_string()).collect();
    base.activate_enum_setting(LAUNCH_CLIPS_OR_SCENES, global, CATEGORY_SESSION, "Launch", options, SCENE_CLIPS_OPTIONS[0], false);

    // Transport
    base.activate_behaviour_on_pause_setting(global);
    base.activate_behaviour_on_stop_setting(global);
    base.activate_double_click_play_button(global);
    base.activate_double_click_stop_button(global);

    // Play and Sequence
    base.activate_quantize_amount_setting(global);

    // Workflow
    base.activate_exclude_deactivated_items_setting(global);
    base.activate_new_clip_length_setting(global);
    base
}

/// Create the configuration with the Reaper arpeggiator modes.
pub fn for_reaper() -> Configuration {
    create(&REAPER_ARP_MODES)
}

/// Former `isLaunchClips`.
pub fn is_launch_clips(config: &Configuration) -> bool {
    config.value(LAUNCH_CLIPS_OR_SCENES).is_none_or(|value| value == SCENE_CLIPS_OPTIONS[0])
}

/// Former `setLaunchClipsOrScenes`.
pub fn set_launch_clips_or_scenes(config: &mut Configuration, launch_clips: bool) {
    config.set_value(LAUNCH_CLIPS_OR_SCENES, SCENE_CLIPS_OPTIONS[if launch_clips { 0 } else { 1 }]);
}

/// Former `getACVSActiveDevice`.
pub fn active_device(config: &Configuration) -> AcvsDevice {
    config.value(ACVS_DEVICE).and_then(|name| AcvsDevice::from_name(&name)).unwrap_or(AcvsDevice::MpcLiveOne)
}

/// Former `setACVSActiveDevice`.
pub fn set_active_device(config: &mut Configuration, device: AcvsDevice) {
    config.set_value(ACVS_DEVICE, device.name());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::properties::Properties;

    #[test]
    fn defaults_and_persistence() {
        let mut c = for_reaper();
        assert!(is_launch_clips(&c));
        assert_eq!(active_device(&c), AcvsDevice::MpcLiveOne);
        set_launch_clips_or_scenes(&mut c, false);
        set_active_device(&mut c, AcvsDevice::Force);
        assert!(!is_launch_clips(&c));

        let mut props = Properties::new();
        c.store(Scope::Global, &mut props);
        assert_eq!(props.get("HARDWARE_SETUP_DEVICE"), Some("Force"));
        assert_eq!(props.get("SESSION_LAUNCH"), Some("Scenes"));
        let mut d = for_reaper();
        d.load(Scope::Global, &Properties::parse(&props.to_text()));
        assert_eq!(active_device(&d), AcvsDevice::Force);
        assert!(!is_launch_clips(&d));
    }

    #[test]
    fn device_is_only_displayed() {
        let mut c = for_reaper();
        assert!(!c.is_enabled(ACVS_DEVICE));
        assert!(c.take_state_changes().contains(&ACVS_DEVICE));
    }
}
