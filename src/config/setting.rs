// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Setting identifiers and descriptors (replaces `ISetting`, `IEnumSetting`, `IIntegerSetting`,
//! ... and the Reaper `*SettingImpl` classes, without any UI).

use super::values::fix_ascii;

/// Identifies a setting. Mirrors the `Integer` constants of `AbstractConfiguration`; controller
/// specific settings start at [`SettingId::NEXT_SETTING_ID`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingId(pub u16);

impl SettingId {
    pub const SCALES_SCALE: SettingId = SettingId(0);
    pub const SCALES_BASE: SettingId = SettingId(1);
    pub const SCALES_IN_KEY: SettingId = SettingId(2);
    pub const SCALES_LAYOUT: SettingId = SettingId(3);
    /// Notified when the VU meter on/off setting changes (the Java code uses this ID for it).
    pub const VU_METER_TYPE: SettingId = SettingId(4);
    pub const ENABLE_VU_METERS: SettingId = SettingId(5);
    pub const BEHAVIOUR_ON_STOP: SettingId = SettingId(6);
    pub const BEHAVIOUR_ON_PAUSE: SettingId = SettingId(7);
    pub const FLIP_SESSION: SettingId = SettingId(8);
    pub const SELECT_CLIP_ON_LAUNCH: SettingId = SettingId(9);
    pub const DRAW_RECORD_STRIPE: SettingId = SettingId(10);
    pub const CONVERT_AFTERTOUCH: SettingId = SettingId(11);
    pub const ACTIVATE_FIXED_ACCENT: SettingId = SettingId(12);
    pub const FIXED_ACCENT_VALUE: SettingId = SettingId(13);
    pub const QUANTIZE_AMOUNT: SettingId = SettingId(14);
    pub const FLIP_RECORD: SettingId = SettingId(15);
    pub const NEW_CLIP_LENGTH: SettingId = SettingId(16);
    pub const AUTO_SELECT_DRUM: SettingId = SettingId(17);
    pub const TURN_OFF_EMPTY_DRUM_PADS: SettingId = SettingId(18);
    pub const TURN_OFF_SCALE_PADS: SettingId = SettingId(19);
    pub const SOUND_DRUM_PADS: SettingId = SettingId(20);
    pub const ACTION_FOR_REC_ARMED_PAD: SettingId = SettingId(21);
    pub const BROWSER_DISPLAY_FILTER1: SettingId = SettingId(22);
    pub const BROWSER_DISPLAY_FILTER2: SettingId = SettingId(23);
    pub const BROWSER_DISPLAY_FILTER3: SettingId = SettingId(24);
    pub const BROWSER_DISPLAY_FILTER4: SettingId = SettingId(25);
    pub const BROWSER_DISPLAY_FILTER5: SettingId = SettingId(26);
    pub const BROWSER_DISPLAY_FILTER6: SettingId = SettingId(27);
    pub const BROWSER_DISPLAY_FILTER7: SettingId = SettingId(28);
    pub const BROWSER_DISPLAY_FILTER8: SettingId = SettingId(29);
    pub const KNOB_SENSITIVITY_DEFAULT: SettingId = SettingId(30);
    pub const KNOB_SENSITIVITY_SLOW: SettingId = SettingId(31);
    pub const NOTEREPEAT_ACTIVE: SettingId = SettingId(32);
    pub const NOTEREPEAT_PERIOD: SettingId = SettingId(33);
    pub const NOTEREPEAT_LENGTH: SettingId = SettingId(34);
    pub const NOTEREPEAT_MODE: SettingId = SettingId(35);
    pub const NOTEREPEAT_OCTAVE: SettingId = SettingId(36);
    pub const MIDI_EDIT_CHANNEL: SettingId = SettingId(37);
    pub const EXCLUDE_DEACTIVATED_ITEMS: SettingId = SettingId(38);
    pub const RECORD_BUTTON_FUNCTION: SettingId = SettingId(39);
    pub const SHIFTED_RECORD_BUTTON_FUNCTION: SettingId = SettingId(40);
    pub const HIERARCHICAL_TRACKS: SettingId = SettingId(41);
    pub const FOOTSWITCH_1: SettingId = SettingId(42);
    pub const FOOTSWITCH_2: SettingId = SettingId(43);
    pub const FOOTSWITCH_3: SettingId = SettingId(44);
    pub const FOOTSWITCH_4: SettingId = SettingId(45);
    pub const STARTUP_VIEW: SettingId = SettingId(46);
    pub const STARTUP_MODE: SettingId = SettingId(47);
    pub const START_WITH_SESSION_VIEW: SettingId = SettingId(48);
    pub const ENABLED_MPE_ZONES: SettingId = SettingId(49);
    pub const MPE_PITCHBEND_RANGE: SettingId = SettingId(50);
    pub const COLOR_TRACK_STATES: SettingId = SettingId(51);
    pub const ENCODER_KNOB_SLOW_DOWN: SettingId = SettingId(52);

    // The following settings have no ID in Java (they cannot be observed there). They got one
    // since all settings are kept in one registry. `can_setting_be_observed` returns false for
    // them, like in Java.

    pub const SHOW_PLAYED_CHORDS: SettingId = SettingId(53);
    pub const INCLUDE_MASTER: SettingId = SettingId(54);
    pub const TRACK_NAVIGATION: SettingId = SettingId(55);
    pub const DOUBLE_CLICK_PLAY_BUTTON: SettingId = SettingId(56);
    pub const DOUBLE_CLICK_STOP_BUTTON: SettingId = SettingId(57);
    pub const CURSOR_KEYS_TRACK_OPTION: SettingId = SettingId(58);
    pub const CURSOR_KEYS_TRACK_SHIFTED_OPTION: SettingId = SettingId(59);
    pub const CURSOR_KEYS_SCENE_OPTION: SettingId = SettingId(60);
    pub const CURSOR_KEYS_SCENE_SHIFTED_OPTION: SettingId = SettingId(61);

    /// Controller implementation IDs start at 100.
    pub const NEXT_SETTING_ID: SettingId = SettingId(100);

    /// Get the ID which is `offset` after this one, e.g. `SettingId::FOOTSWITCH_1.offset(2)`.
    pub const fn offset(self, offset: u16) -> SettingId {
        SettingId(self.0 + offset)
    }
}

/// Where a setting is stored: in the controller configuration file (global) or in the Reaper
/// project (document).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Global,
    Document,
}

/// The type and value range of a setting.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingKind {
    /// One of a list of texts (`IEnumSetting`).
    Enum { options: Vec<String> },
    /// An integer in the range of [min..max] (`IIntegerSetting`).
    Integer { min: i32, max: i32 },
    /// A boolean stored as "true"/"false" (`IBooleanSetting`).
    Boolean,
    /// A free text (`IStringSetting`).
    Text,
    /// A host action, stored as its ID (`IActionSetting`). A host with its own action list shows
    /// a selection of it (Bitwig: a category and the actions of the category), others a text.
    Action,
    /// A button (`ISignalSetting`) with the given text. It has no value and is not stored; every
    /// press is recorded as a change of the setting.
    Signal { button: String },
}

/// Describes an activated setting.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingDescriptor {
    pub id: SettingId,
    pub scope: Scope,
    pub category: String,
    pub label: String,
    pub kind: SettingKind,
    /// The initial value as stored in the properties.
    pub default_value: String,
    /// True if Java registered the ID as observable (`canSettingBeObserved`).
    pub observable: bool,
}

impl SettingDescriptor {
    /// The key in the properties (`BaseSetting.getID`), e.g. "PLAY_AND_SEQUENCE_QUANTIZE_AMOUNT".
    pub fn key(&self) -> String {
        setting_key(&self.category, &self.label)
    }

    /// Normalize a value: returns the value as it is stored or `None` if it is not valid for this
    /// setting.
    pub fn normalize(&self, value: &str) -> Option<String> {
        match &self.kind {
            SettingKind::Enum { options } => options.iter().find(|o| *o == value).cloned(),
            SettingKind::Integer { min, max } => {
                value.trim().parse::<i32>().ok().filter(|v| (*min..=*max).contains(v)).map(|v| v.to_string())
            }
            SettingKind::Boolean => Some(if value.eq_ignore_ascii_case("true") { "true" } else { "false" }.to_string()),
            SettingKind::Text | SettingKind::Action => Some(value.to_string()),
            SettingKind::Signal { .. } => Some(String::new()),
        }
    }

    /// False for a button, which has no value to store.
    pub fn is_stored(&self) -> bool {
        !matches!(self.kind, SettingKind::Signal { .. })
    }

    /// The options of an enumeration setting (empty for other kinds).
    pub fn options(&self) -> &[String] {
        match &self.kind {
            SettingKind::Enum { options } => options,
            _ => &[],
        }
    }
}

/// Create the properties key from category and label like the Java implementation.
pub fn setting_key(category: &str, label: &str) -> String {
    let id = format!("{category}_{label}").to_uppercase();
    fix_ascii(&id).replace([' ', '?'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(setting_key("Play and Sequence", "Activate Fixed Accent"), "PLAY_AND_SEQUENCE_ACTIVATE_FIXED_ACCENT");
        assert_eq!(setting_key("Note Repeat", "Period"), "NOTE_REPEAT_PERIOD");
        assert_eq!(
            setting_key("Workflow", "Include (Group-)Mastertrack (requires restart)"),
            "WORKFLOW_INCLUDE_(GROUP-)MASTERTRACK_(REQUIRES_RESTART)"
        );
    }

    #[test]
    fn normalize() {
        let d = SettingDescriptor {
            id: SettingId::QUANTIZE_AMOUNT,
            scope: Scope::Global,
            category: "C".into(),
            label: "L".into(),
            kind: SettingKind::Integer { min: 1, max: 100 },
            default_value: "100".into(),
            observable: true,
        };
        assert_eq!(d.normalize("50"), Some("50".into()));
        assert_eq!(d.normalize("0"), None);
        assert_eq!(d.normalize("x"), None);
    }
}
