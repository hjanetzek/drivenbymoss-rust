// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Maschine JAM (former `MaschineJamConfiguration`).

use std::ops::{Deref, DerefMut};

use super::configuration::{Configuration, NoteRepeatCapabilities, CATEGORY_WORKFLOW};
use super::setting::{Scope, SettingId};
use super::values::{to_options, ArpeggiatorMode, ON_OFF_OPTIONS};
use crate::framework::ids::{Modes, Views};

/// The views which can be selected as the startup view.
pub const STARTUP_VIEWS: [Views; 10] = [
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

/// The modes which can be selected as the startup mode (with the names of `Modes.getModeName`;
/// the Java version has no name for the user mode).
pub const STARTUP_MODES: [(Modes, &str); 13] = [
    (Modes::Volume, "Volume"),
    (Modes::Pan, "Panning"),
    (Modes::Track, "Track"),
    (Modes::Send1, "Send 1"),
    (Modes::Send2, "Send 2"),
    (Modes::Send3, "Send 3"),
    (Modes::Send4, "Send 4"),
    (Modes::Send5, "Send 5"),
    (Modes::Send6, "Send 6"),
    (Modes::Send7, "Send 7"),
    (Modes::Send8, "Send 8"),
    (Modes::DeviceParams, "Parameters"),
    (Modes::User, "Missing mode name"),
];

/// The Maschine JAM settings: the base configuration (accessible via `Deref`) plus the slow
/// fader change setting.
#[derive(Debug, Clone)]
pub struct MaschineJamConfiguration {
    base: Configuration,
}

impl MaschineJamConfiguration {
    /// Setting to change the fader values slowly.
    pub const SLOW_FADER_CHANGE: SettingId = SettingId::NEXT_SETTING_ID;

    /// Create the configuration and activate all settings (the former `init`).
    pub fn new(arpeggiator_modes: &[ArpeggiatorMode], note_repeat: NoteRepeatCapabilities, has_drum_device: bool) -> Self {
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

        // Transport
        base.activate_behaviour_on_pause_setting(global);
        base.activate_double_click_play_button(global);
        base.activate_record_button_setting(global);
        base.activate_shifted_record_button_setting(global);

        // Play and Sequence
        base.activate_accent_active_setting(global);
        base.activate_accent_value_setting(global);
        base.activate_quantize_amount_setting(global);
        base.activate_startup_view_setting(global, &STARTUP_VIEWS);
        base.activate_start_with_session_view_setting(global);
        base.activate_turn_off_scale_pads_setting(global);
        base.activate_show_played_chords_setting(global);

        // Drum Sequencer
        if has_drum_device {
            base.activate_turn_off_empty_drum_pads_setting(global);
        }

        // Session
        base.activate_select_clip_on_launch_setting(global);
        base.activate_action_for_rec_armed_pad(global);
        base.activate_draw_record_stripe_setting(global);
        base.activate_flip_session_setting(global);

        // Workflow
        base.activate_exclude_deactivated_items_setting(global);
        base.activate_enum_setting(Self::SLOW_FADER_CHANGE, global, CATEGORY_WORKFLOW, "Slow Fader Change", to_options(&ON_OFF_OPTIONS), ON_OFF_OPTIONS[0], false);
        base.activate_startup_mode_setting(global, &STARTUP_MODES);
        base.activate_new_clip_length_setting(global);
        base.activate_knob_speed_setting(global, 100, 60, false);
        base.activate_footswitch_setting(global, 0, "Footswitch (Tip)", None);
        base.activate_footswitch_setting(global, 1, "Footswitch (Ring)", None);

        MaschineJamConfiguration { base }
    }

    /// Create the configuration with the Reaper arpeggiator modes and note repeat capabilities
    /// (Reaper supports a drum device).
    pub fn for_reaper() -> Self {
        Self::new(&super::values::REAPER_ARP_MODES, NoteRepeatCapabilities::REAPER, true)
    }

    /// Get the base configuration, which also holds the Maschine JAM specific settings.
    pub fn into_base(self) -> Configuration {
        self.base
    }

    /// Is the slow fader change active? Works on a configuration created by
    /// [`MaschineJamConfiguration::new`].
    pub fn is_slow_fader_change(config: &Configuration) -> bool {
        config.custom_bool(Self::SLOW_FADER_CHANGE)
    }

    /// Toggle the slow fader change.
    pub fn toggle_slow_fader_change(config: &mut Configuration) {
        let is_slow = Self::is_slow_fader_change(config);
        config.set_value(Self::SLOW_FADER_CHANGE, ON_OFF_OPTIONS[if is_slow { 0 } else { 1 }]);
    }
}

impl Deref for MaschineJamConfiguration {
    type Target = Configuration;

    fn deref(&self) -> &Configuration {
        &self.base
    }
}

impl DerefMut for MaschineJamConfiguration {
    fn deref_mut(&mut self) -> &mut Configuration {
        &mut self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let mut c = MaschineJamConfiguration::for_reaper().into_base();
        assert!(!MaschineJamConfiguration::is_slow_fader_change(&c));
        MaschineJamConfiguration::toggle_slow_fader_change(&mut c);
        assert!(MaschineJamConfiguration::is_slow_fader_change(&c));
        assert_eq!(c.startup_mode(), Modes::Volume);
        assert_eq!(c.startup_view(), Views::Play);
        assert!(c.is_setting_active(SettingId::FOOTSWITCH_1));
    }
}
