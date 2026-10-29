// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Maschine Mk2, Mk3, Mikro Mk3, Plus and Studio (former
//! `MaschineConfiguration`).

use std::ops::{Deref, DerefMut};

use super::configuration::{Configuration, NoteRepeatCapabilities, CATEGORY_PLAY_AND_SEQUENCE};
use super::setting::{Scope, SettingId};
use super::values::ArpeggiatorMode;
use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::maschine::core::ribbon_mode::RibbonMode;
use crate::framework::ids::Views;
use crate::scale::ScaleLayout;

/// The views which can be selected as the startup (preferred note) view.
pub const PREFERRED_NOTE_VIEWS: [Views; 2] = [Views::Play, Views::Drum];

/// The Maschine settings: the base configuration (accessible via `Deref`) plus the ribbon mode.
#[derive(Debug, Clone)]
pub struct MaschineConfiguration {
    base: Configuration,
}

impl MaschineConfiguration {
    /// Setting for the mode of the touch strip (ribbon).
    pub const RIBBON_MODE: SettingId = SettingId::NEXT_SETTING_ID;

    /// Create the configuration and activate all settings (the former `init`).
    pub fn new(maschine: Maschine, arpeggiator_modes: &[ArpeggiatorMode], note_repeat: NoteRepeatCapabilities) -> Self {
        let mut base = Configuration::new(arpeggiator_modes);

        let global = Scope::Global;
        let document = Scope::Document;

        // Scale
        base.activate_scale_setting(document);
        base.activate_scale_base_setting(document);
        base.activate_scale_in_scale_setting(document);
        base.activate_scale_layout_setting_with(document, ScaleLayout::SequentUp.name());

        // Note Repeat
        base.activate_note_repeat_setting(document, note_repeat);

        // Transport
        base.activate_behaviour_on_pause_setting(global);
        base.activate_double_click_play_button(global);
        base.activate_record_button_setting(global);
        base.activate_shifted_record_button_setting(global);

        // Play and Sequence
        base.activate_accent_active_setting(global);
        base.activate_accent_value_setting(global);
        base.activate_quantize_amount_setting(global);
        base.activate_turn_off_scale_pads_setting(global);
        base.activate_show_played_chords_setting(global);
        base.activate_startup_view_setting(global, &PREFERRED_NOTE_VIEWS);

        let ribbon_mode_names = RibbonMode::names();
        let default_value = ribbon_mode_names[0].clone();
        base.activate_enum_setting(Self::RIBBON_MODE, global, CATEGORY_PLAY_AND_SEQUENCE, "Ribbon Mode", ribbon_mode_names, &default_value, true);

        // Session
        base.activate_select_clip_on_launch_setting(global);
        base.activate_action_for_rec_armed_pad(global);

        // Workflow
        base.activate_exclude_deactivated_items_setting(global);
        base.activate_new_clip_length_setting(global);
        base.activate_knob_speed_setting(global, 100, 60, false);

        let footswitches = maschine.footswitches();
        if footswitches >= 2 {
            base.activate_footswitch_setting(global, 0, "Footswitch (Tip)", None);
            base.activate_footswitch_setting(global, 1, "Footswitch (Ring)", None);
            if footswitches == 4 {
                base.activate_footswitch_setting(global, 2, "Footswitch 2 (Tip)", None);
                base.activate_footswitch_setting(global, 3, "Footswitch 2 (Ring)", None);
            }
        }

        // Pads
        base.activate_convert_aftertouch_setting(global);

        MaschineConfiguration { base }
    }

    /// Create the configuration with the Reaper arpeggiator modes and note repeat capabilities.
    pub fn for_reaper(maschine: Maschine) -> Self {
        Self::new(maschine, &super::values::REAPER_ARP_MODES, NoteRepeatCapabilities::REAPER)
    }

    /// Get the base configuration, which also holds the Maschine specific settings.
    pub fn into_base(self) -> Configuration {
        self.base
    }

    /// Get the ribbon mode from a configuration created by [`MaschineConfiguration::new`].
    pub fn ribbon_mode(config: &Configuration) -> RibbonMode {
        RibbonMode::ALL.get(config.custom_index(Self::RIBBON_MODE)).copied().unwrap_or(RibbonMode::PitchDown)
    }

    /// Set the ribbon mode of a configuration created by [`MaschineConfiguration::new`].
    pub fn set_ribbon_mode(config: &mut Configuration, mode: RibbonMode) {
        config.set_value(Self::RIBBON_MODE, mode.name());
    }
}

impl Deref for MaschineConfiguration {
    type Target = Configuration;

    fn deref(&self) -> &Configuration {
        &self.base
    }
}

impl DerefMut for MaschineConfiguration {
    fn deref_mut(&mut self) -> &mut Configuration {
        &mut self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = MaschineConfiguration::for_reaper(Maschine::Mk2).into_base();
        assert_eq!(MaschineConfiguration::ribbon_mode(&c), RibbonMode::PitchDown);
        assert_eq!(c.startup_view(), Views::Play);
        assert_eq!(c.scale_layout(), ScaleLayout::SequentUp.name());
        assert!(!c.is_setting_active(SettingId::FOOTSWITCH_1));
        let mut c = c;
        MaschineConfiguration::set_ribbon_mode(&mut c, RibbonMode::Cc11);
        assert_eq!(MaschineConfiguration::ribbon_mode(&c), RibbonMode::Cc11);
        assert!(c.take_changed_settings().contains(&MaschineConfiguration::RIBBON_MODE));

        let studio = MaschineConfiguration::for_reaper(Maschine::Studio);
        assert!(studio.is_setting_active(SettingId::FOOTSWITCH_1.offset(3)));
    }
}
