// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Novation Launchpad (former `LaunchpadConfiguration`).

use std::ops::{Deref, DerefMut};

use super::configuration::{Configuration, NoteRepeatCapabilities, CATEGORY_PADS};
use super::setting::{Scope, SettingId};
use super::values::ArpeggiatorMode;
use crate::framework::ids::Views;

/// The views which can be selected as the startup (preferred note) view.
pub const PREFERRED_NOTE_VIEWS: [Views; 10] = [
    Views::Play,
    Views::Chords,
    Views::Piano,
    Views::Drum64,
    Views::Drum,
    Views::Drum4,
    Views::Drum8,
    Views::Sequencer,
    Views::Raindrops,
    Views::PolySequencer,
];

/// The information needed from the Launchpad controller definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchpadConfigParams {
    /// True if the device supports setting the pad brightness, which is the case if
    /// `ILaunchpadControllerDefinition.getBrightnessSysex ()` is not empty (Launchpad X and
    /// Launchpad Mini MkIII).
    pub has_brightness: bool,
}

/// The Launchpad settings: the base configuration (accessible via `Deref`) plus the pad
/// brightness.
#[derive(Debug, Clone)]
pub struct LaunchpadConfiguration {
    base: Configuration,
    params: LaunchpadConfigParams,
}

impl LaunchpadConfiguration {
    /// Setting for the brightness of the pad LEDs.
    pub const PAD_BRIGHTNESS: SettingId = SettingId::NEXT_SETTING_ID;

    /// Create the configuration and activate all settings (the former `init`).
    pub fn new(params: LaunchpadConfigParams, arpeggiator_modes: &[ArpeggiatorMode], note_repeat: NoteRepeatCapabilities) -> Self {
        let mut base = Configuration::new(arpeggiator_modes);
        base.set_preferred_audio_view(Views::ClipLength);

        let global = Scope::Global;
        let document = Scope::Document;

        // Scale
        base.activate_scale_setting(document);
        base.activate_scale_base_setting(document);
        base.activate_scale_in_scale_setting(document);
        base.activate_scale_layout_setting(document);

        // Note Repeat
        base.activate_note_repeat_setting(document, note_repeat);

        // Play and Sequence
        base.activate_accent_active_setting(global);
        base.activate_accent_value_setting(global);
        base.activate_quantize_amount_setting(global);
        base.activate_startup_view_setting(global, &PREFERRED_NOTE_VIEWS);
        base.activate_start_with_session_view_setting(global);
        base.activate_turn_off_scale_pads_setting(global);
        base.activate_show_played_chords_setting(global);

        // Transport
        base.activate_behaviour_on_pause_setting(global);
        base.activate_record_button_setting(global);
        base.activate_shifted_record_button_setting(global);
        base.activate_double_click_play_button(global);

        // Session
        base.activate_flip_session_setting(global);
        base.activate_select_clip_on_launch_setting(global);
        base.activate_action_for_rec_armed_pad(global);

        // Drum Sequencer
        base.activate_auto_select_drum_setting(global);
        base.activate_turn_off_empty_drum_pads_setting(global);

        // Workflow
        base.activate_exclude_deactivated_items_setting(global);
        base.activate_include_master_setting(global);
        base.activate_new_clip_length_setting(global);

        // Pad Sensitivity
        base.activate_convert_aftertouch_setting(global);
        if params.has_brightness {
            let values: Vec<String> = (0..128).map(|i| i.to_string()).collect();
            base.activate_enum_setting(Self::PAD_BRIGHTNESS, global, CATEGORY_PADS, "Brightness", values, "127", true);
        }

        LaunchpadConfiguration { base, params }
    }

    /// Create the configuration with the Reaper arpeggiator modes and note repeat capabilities.
    pub fn for_reaper(params: LaunchpadConfigParams) -> Self {
        Self::new(params, &super::values::REAPER_ARP_MODES, NoteRepeatCapabilities::REAPER)
    }

    pub fn params(&self) -> LaunchpadConfigParams {
        self.params
    }

    /// The pad brightness (0-127), 0 if the device does not support it.
    pub fn pad_brightness(&self) -> i32 {
        self.base.custom_index(Self::PAD_BRIGHTNESS) as i32
    }

    /// Set the pad brightness (0-127), ignored if the device does not support it.
    pub fn set_pad_brightness(&mut self, brightness: i32) {
        if (0..128).contains(&brightness) {
            self.base.set_custom_index(Self::PAD_BRIGHTNESS, brightness as usize);
        }
    }

    pub fn base(&self) -> &Configuration {
        &self.base
    }

    /// Get the base configuration, which also holds the Launchpad specific settings.
    pub fn into_base(self) -> Configuration {
        self.base
    }

    pub fn base_mut(&mut self) -> &mut Configuration {
        &mut self.base
    }
}

impl Deref for LaunchpadConfiguration {
    type Target = Configuration;

    fn deref(&self) -> &Configuration {
        &self.base
    }
}

impl DerefMut for LaunchpadConfiguration {
    fn deref_mut(&mut self) -> &mut Configuration {
        &mut self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::properties::Properties;
    use crate::config::values::Resolution;

    #[test]
    fn defaults() {
        let c = LaunchpadConfiguration::for_reaper(LaunchpadConfigParams { has_brightness: true });
        assert_eq!(c.preferred_audio_view(), Views::ClipLength);
        assert_eq!(c.preferred_note_view(), Views::Play);
        assert_eq!(c.pad_brightness(), 127);
        assert!(c.can_setting_be_observed(LaunchpadConfiguration::PAD_BRIGHTNESS));
        assert!(c.can_setting_be_observed(SettingId::EXCLUDE_DEACTIVATED_ITEMS));
        assert!(!c.can_setting_be_observed(SettingId::BEHAVIOUR_ON_STOP));
        assert!(c.is_play_double_click_return_to_zero());
        assert_eq!(c.note_repeat_period(), Resolution::Res1_16);
        assert!(!c.is_turn_off_empty_drum_pads());
        assert!(c.are_master_tracks_included());

        let no = LaunchpadConfiguration::for_reaper(LaunchpadConfigParams { has_brightness: false });
        assert_eq!(no.pad_brightness(), 0);
        assert!(!no.can_setting_be_observed(LaunchpadConfiguration::PAD_BRIGHTNESS));
    }

    #[test]
    fn brightness_and_persistence() {
        let mut c = LaunchpadConfiguration::for_reaper(LaunchpadConfigParams { has_brightness: true });
        c.set_pad_brightness(64);
        c.set_pad_brightness(200); // ignored
        c.set_value(SettingId::STARTUP_VIEW, "Poly Seq.");
        c.set_flip_session(true);
        assert_eq!(c.take_changed_settings(), vec![LaunchpadConfiguration::PAD_BRIGHTNESS, SettingId::STARTUP_VIEW, SettingId::FLIP_SESSION]);

        let mut props = Properties::new();
        c.store(Scope::Global, &mut props);
        assert_eq!(props.get("PADS_BRIGHTNESS"), Some("64"));
        assert_eq!(props.get("SESSION_FLIP_SESSION"), Some("On"));
        assert_eq!(props.get("TRANSPORT_DOUBLE_CLICK_PLAY_RETURNS_TO_ZERO"), Some("On"));

        let mut d = LaunchpadConfiguration::for_reaper(LaunchpadConfigParams { has_brightness: true });
        d.load(Scope::Global, &Properties::parse(&props.to_text()));
        assert_eq!(d.pad_brightness(), 64);
        assert_eq!(d.startup_view(), Views::PolySequencer);
        assert!(d.is_flip_session());
    }
}
