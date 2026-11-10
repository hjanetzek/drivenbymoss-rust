// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Arturia Beatstep (former `BeatstepConfiguration`).

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
    base.activate_scale_layout_setting(document);

    // Workflow
    base.activate_exclude_deactivated_items_setting(global);

    // Pad Sensitivity
    base.activate_convert_aftertouch_setting(global);

    base
}

/// Create the configuration with the Reaper arpeggiator modes.
pub fn for_reaper() -> Configuration {
    create_configuration(&REAPER_ARP_MODES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::properties::Properties;
    use crate::config::SettingId;

    #[test]
    fn activated_settings() {
        let c = for_reaper();
        for id in [
            SettingId::SCALES_SCALE,
            SettingId::SCALES_BASE,
            SettingId::SCALES_IN_KEY,
            SettingId::SCALES_LAYOUT,
            SettingId::EXCLUDE_DEACTIVATED_ITEMS,
            SettingId::CONVERT_AFTERTOUCH,
        ] {
            assert!(c.is_setting_active(id), "{id:?}");
        }
        for id in [SettingId::BEHAVIOUR_ON_PAUSE, SettingId::ACTIVATE_FIXED_ACCENT, SettingId::FIXED_ACCENT_VALUE] {
            assert!(!c.is_setting_active(id), "{id:?}");
        }
    }

    #[test]
    fn persistence() {
        let mut c = for_reaper();
        c.set_scale("Minor");
        c.set_scale_in_key(false);
        let mut props = Properties::new();
        c.store(Scope::Document, &mut props);
        let mut d = for_reaper();
        d.load(Scope::Document, &Properties::parse(&props.to_text()));
        assert_eq!(d.scale(), "Minor");
        assert!(!d.is_scale_in_key());
    }
}
