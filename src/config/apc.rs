// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for the Akai APC40 Mk1 and Mk2 (former `APCConfiguration`).

use super::configuration::{Configuration, NoteRepeatCapabilities};
use super::setting::Scope;
use super::values::{ArpeggiatorMode, REAPER_ARP_MODES};
use crate::framework::ids::Views;

/// The views which can be selected as the startup view.
pub const PREFERRED_NOTE_VIEWS: [Views; 4] = [Views::Play, Views::Drum, Views::Sequencer, Views::Raindrops];

/// Create the configuration and activate all settings (the former `init`).
pub fn create(is_mk2: bool, arpeggiator_modes: &[ArpeggiatorMode], note_repeat: NoteRepeatCapabilities) -> Configuration {
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

    // Play and Sequence
    base.activate_quantize_amount_setting(global);
    base.activate_startup_view_setting(global, &PREFERRED_NOTE_VIEWS);
    base.activate_start_with_session_view_setting(global);
    if is_mk2 {
        base.activate_turn_off_scale_pads_setting(global);
        base.activate_show_played_chords_setting(global);
    }

    // Transport
    base.activate_behaviour_on_pause_setting(global);
    base.activate_double_click_play_button(global);

    // Drum Sequencer: the turn off empty drum pads setting requires the drum device capability,
    // which Reaper does not have

    // Workflow
    base.activate_exclude_deactivated_items_setting(global);
    base.activate_new_clip_length_setting(global);
    base.activate_footswitch_setting(global, 0, "Footswitch 1", None);
    if !is_mk2 {
        base.activate_footswitch_setting(global, 1, "Footswitch 2", None);
    }
    base
}

/// Create the configuration with the Reaper arpeggiator modes and note repeat capabilities.
pub fn for_reaper(is_mk2: bool) -> Configuration {
    create(is_mk2, &REAPER_ARP_MODES, NoteRepeatCapabilities::REAPER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::properties::Properties;
    use crate::config::SettingId;

    #[test]
    fn defaults_and_persistence() {
        let mk1 = for_reaper(false);
        assert!(mk1.is_setting_active(SettingId::FOOTSWITCH_1.offset(1)));
        assert!(!mk1.is_setting_active(SettingId::TURN_OFF_SCALE_PADS));
        let mut mk2 = for_reaper(true);
        assert!(!mk2.is_setting_active(SettingId::FOOTSWITCH_1.offset(1)));
        assert!(mk2.is_setting_active(SettingId::TURN_OFF_SCALE_PADS));
        assert!(mk2.is_setting_active(SettingId::NOTEREPEAT_ACTIVE));
        assert_eq!(mk2.startup_view(), Views::Play);

        mk2.set_value(SettingId::STARTUP_VIEW, "Sequencer");
        let mut props = Properties::new();
        mk2.store(Scope::Global, &mut props);
        let mut loaded = for_reaper(true);
        loaded.load(Scope::Global, &Properties::parse(&props.to_text()));
        assert_eq!(loaded.startup_view(), Views::Sequencer);
    }
}
