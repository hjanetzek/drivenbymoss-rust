// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings for Generic Flexi (former `GenericFlexiConfiguration`).
//!
//! All settings of the Java version are activated in the same order with the same categories and
//! labels, so they are stored with the same keys in the controller configuration file. The
//! command slots themselves are not part of the configuration file; they are stored in the
//! mapping file (see `controller::generic::mapping`), which is loaded at startup from the file
//! set in the "Filename" setting.
//!
//! The logic of the slot editor (the settings of the selected slot) and of the MIDI learn needs
//! the command slots and is in `controller::generic::editor`.
//!
//! Differences to Java:
//! - The signal settings (the buttons "Set", "Save" and "Load") are not settings here (there is
//!   no UI). Their functions are `editor::learn_set`, `mapping::save_file` and
//!   `mapping::load_file`.
//! - The settings which have no ID in Java got one (see the constants).

use super::configuration::{Configuration, NoteRepeatCapabilities};
use super::setting::{Scope, SettingDescriptor, SettingId, SettingKind};
use super::values::{lookup_index, midi_channel_options, to_options, ArpeggiatorMode, ON_OFF_OPTIONS, REAPER_ARP_MODES};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::KnobMode;

/// The name of the controller, the default name of the keyboard input.
pub const GENERIC_FLEXI_NAME: &str = "Generic Flexi";

/// Export signal (not a setting in Rust, see the module documentation).
pub const BUTTON_SAVE: SettingId = SettingId::NEXT_SETTING_ID;
/// Import signal (not a setting in Rust).
pub const BUTTON_LOAD: SettingId = SettingId::NEXT_SETTING_ID.offset(1);
/// Enable MMC (unused, as in Java).
pub const ENABLE_MMC: SettingId = SettingId::NEXT_SETTING_ID.offset(2);
/// The selected mode.
pub const SELECTED_MODE: SettingId = SettingId::NEXT_SETTING_ID.offset(3);

/// The selected slot of the slot editor.
pub const SLOT_SELECTED: SettingId = SettingId::NEXT_SETTING_ID.offset(10);
/// The MIDI learn values (read only in the Java UI).
pub const LEARN_TYPE: SettingId = SettingId::NEXT_SETTING_ID.offset(11);
pub const LEARN_NUMBER: SettingId = SettingId::NEXT_SETTING_ID.offset(12);
pub const LEARN_MIDI_CHANNEL: SettingId = SettingId::NEXT_SETTING_ID.offset(13);
pub const LEARN_RESOLUTION: SettingId = SettingId::NEXT_SETTING_ID.offset(14);
/// The settings of the selected slot.
pub const SLOT_TYPE: SettingId = SettingId::NEXT_SETTING_ID.offset(15);
pub const SLOT_NUMBER: SettingId = SettingId::NEXT_SETTING_ID.offset(16);
pub const SLOT_MIDI_CHANNEL: SettingId = SettingId::NEXT_SETTING_ID.offset(17);
pub const SLOT_RESOLUTION: SettingId = SettingId::NEXT_SETTING_ID.offset(18);
pub const SLOT_KNOB_MODE: SettingId = SettingId::NEXT_SETTING_ID.offset(19);
pub const SLOT_FUNCTION_LAYER: SettingId = SettingId::NEXT_SETTING_ID.offset(20);
pub const SLOT_SEND_VALUE: SettingId = SettingId::NEXT_SETTING_ID.offset(21);
pub const SLOT_SEND_VALUE_WHEN_RECEIVED: SettingId = SettingId::NEXT_SETTING_ID.offset(22);
/// The function settings of the selected slot, one per command category (in the order of
/// [`CommandCategory::ALL`]), see [`function_setting`].
pub const SLOT_FUNCTION_FIRST: SettingId = SettingId::NEXT_SETTING_ID.offset(30);
/// The file name of the mapping file.
pub const FILENAME: SettingId = SettingId::NEXT_SETTING_ID.offset(60);
/// The keyboard settings (all require a restart).
pub const KEYBOARD_INPUT_NAME: SettingId = SettingId::NEXT_SETTING_ID.offset(61);
pub const KEYBOARD_CHANNEL: SettingId = SettingId::NEXT_SETTING_ID.offset(62);
pub const KEYBOARD_ROUTE_TIMBRE: SettingId = SettingId::NEXT_SETTING_ID.offset(63);
pub const KEYBOARD_ROUTE_MODULATION: SettingId = SettingId::NEXT_SETTING_ID.offset(64);
pub const KEYBOARD_ROUTE_EXPRESSION: SettingId = SettingId::NEXT_SETTING_ID.offset(65);
pub const KEYBOARD_ROUTE_SUSTAIN: SettingId = SettingId::NEXT_SETTING_ID.offset(66);
pub const KEYBOARD_ROUTE_PROGRAM_CHANGE: SettingId = SettingId::NEXT_SETTING_ID.offset(67);
pub const KEYBOARD_ROUTE_PITCHBEND: SettingId = SettingId::NEXT_SETTING_ID.offset(68);
/// The 8 assignable actions (Reaper action IDs), see [`action_setting`].
pub const ACTION_FIRST: SettingId = SettingId::NEXT_SETTING_ID.offset(70);
/// The number of assignable actions.
pub const NUM_ACTIONS: usize = 8;

/// A setting of a slot has changed (not a setting, only a change notification).
pub const SLOT_CHANGE: SettingId = SettingId(1000);

/// The number of command slots.
pub const NUM_SLOTS: usize = 300;

pub const CATEGORY_SLOT: &str = "Slot";
pub const CATEGORY_LEARN: &str = "Use a knob/fader/button then click Set...";
pub const CATEGORY_SLOT_TRIGGER: &str = "Selected Slot - MIDI trigger";
pub const CATEGORY_SLOT_UPDATE: &str = "Selected Slot - MIDI device update";
pub const CATEGORY_SLOT_FUNCTION: &str = "Selected Slot - Function";
pub const CATEGORY_LOAD_SAVE: &str = "Load / Save";
pub const CATEGORY_KEYBOARD: &str = "Keyboard / Pads (requires restart)";
pub const CATEGORY_OPTIONS: &str = "Options";

/// The types (the index - 1 is the type of the slot).
pub const OPTIONS_TYPE: [&str; 6] = ["Off", "CC", "Note", "Program Change", "Pitchbend", "MMC"];

/// The resolution options.
pub const OPTIONS_RESOLUTION: [&str; 2] = ["7-bit", "14-bit"];

/// The function layer options (the index - 1 is the function layer of the slot).
pub const FUNCTION_LAYERS: [&str; 11] = ["All", "Layer 1", "Layer 2", "Layer 3", "Layer 4", "Layer 5", "Layer 6", "Layer 7", "Layer 8", "Layer 9", "Layer 10"];

/// The Modes options.
pub const MODES: [&str; 12] = ["Track", "Volume", "Panning", "Send 1", "Send 2", "Send 3", "Send 4", "Send 5", "Send 6", "Send 7", "Send 8", "Device"];

/// The MIDI channel options of the keyboard (index - 1 is the channel, 16 is all channels).
pub const KEYBOARD_CHANNELS: [&str; 18] = ["Off", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "All"];

/// The names of the numbers of a slot (CC, note, MMC).
pub const NUMBER_NAMES: [&str; 128] = [
    "00  MMC Reserved for extensions, CC Bank Select (MSB)",
    "01  MMC Stop, CC Modulation Wheel (MSB)",
    "02  MMC Play, CC Breath Controller (MSB)",
    "03  MMC Deferred play, CC Undefined (MSB)",
    "04  MMC Fast forward, CC Foot Controller (MSB)",
    "05  MMC Rewind, CC Portamento Time (MSB)",
    "06  MMC Record strobe, CC Data Entry (MSB)",
    "07  MMC Record exit, CC Channel Volume (MSB)",
    "08  MMC Record pause, CC Balance (MSB)",
    "09  MMC Pause, CC Undefined (MSB)",
    "10  MMC Eject, CC Pan (MSB)",
    "11  MMC Chase, CC Expression (MSB)",
    "12  MMC Command error reset, CC Effect Control 1 (MSB)",
    "13  MMC MMC reset, CC Effect Control 2 (MSB)",
    "14  MMC Write, CC Undefined (MSB)",
    "15  MMC Masked write, CC Undefined (MSB)",
    "16  MMC Read, CC General Purpose Controller 1 (MSB)",
    "17  MMC Update, CC General Purpose Controller 2 (MSB)",
    "18  MMC Locate, CC General Purpose Controller 3 (MSB)",
    "19  MMC Variable play, CC General Purpose Controller 4 (MSB)",
    "20  MMC Search, CC Undefined (MSB)",
    "21  MMC Shuttle, CC Undefined (MSB)",
    "22  MMC Step, CC Undefined (MSB)",
    "23  MMC Assign system master, CC Undefined (MSB)",
    "24  MMC Generator command, CC Undefined (MSB)",
    "25  MMC Midi time code command, CC Undefined (MSB)",
    "26  MMC Move, CC Undefined (MSB)",
    "27  MMC Add, CC Undefined (MSB)",
    "28  MMC Subtract, CC Undefined (MSB)",
    "29  MMC Drop frame adjust, CC Undefined (MSB)",
    "30  MMC Procedure, CC Undefined (MSB)",
    "31  MMC Event, CC Undefined (MSB)",
    "32  MMC Group, CC Bank Select (LSB)",
    "33  MMC Command segment, CC Modulation Wheel (LSB)",
    "34  MMC Deferred variable play, CC Breath Controller (LSB)",
    "35  MMC Record strobe variable, CC Undefined (LSB)",
    "36  CC Foot Controller (LSB)",
    "37  CC Portamento Time (LSB)",
    "38  CC Data Entry (LSB)",
    "39  CC Channel Volume (LSB)",
    "40  CC Balance (LSB)",
    "41  CC Undefined (LSB)",
    "42  CC Pan (LSB)",
    "43  CC Expression (LSB)",
    "44  CC Effect Control 1 (LSB)",
    "45  CC Effect Control 2 (LSB)",
    "46  CC Undefined (LSB)",
    "47  CC Undefined (LSB)",
    "48  CC General Purpose Controller 1 (LSB)",
    "49  CC General Purpose Controller 2 (LSB)",
    "50  CC General Purpose Controller 3 (LSB)",
    "51  CC General Purpose Controller 4 (LSB)",
    "52  CC Undefined (LSB)",
    "53  CC Undefined (LSB)",
    "54  CC Undefined (LSB)",
    "55  CC Undefined (LSB)",
    "56  CC Undefined (LSB)",
    "57  CC Undefined (LSB)",
    "58  CC Undefined (LSB)",
    "59  CC Undefined (LSB)",
    "60  CC Undefined (LSB)",
    "61  CC Undefined (LSB)",
    "62  CC Undefined (LSB)",
    "63  CC Undefined (LSB)",
    "64  CC Sustain Pedal",
    "65  CC Portamento On/Off",
    "66  CC Sostenuto",
    "67  CC Soft Pedal",
    "68  CC Legato Footswitch",
    "69  CC Hold 2",
    "70  CC Sound Controller 1 - Sound Variation",
    "71  CC Sound Controller 2 - Timbre/Harmonic Intensity",
    "72  CC Sound Controller 3 - Release Time",
    "73  CC Sound Controller 4 - Attack Time",
    "74  CC Sound Controller 5 - Brightness",
    "75  CC Sound Controller 6 - Decay Time",
    "76  CC Sound Controller 7 - Vibrato Rate",
    "77  CC Sound Controller 8 - Vibrato Depth",
    "78  CC Sound Controller 9 - Vibrato Delay",
    "79  CC Sound Controller 10",
    "80  CC General Purpose 5",
    "81  CC General Purpose 6",
    "82  CC General Purpose 7",
    "83  CC General Purpose 8",
    "84  CC Portamento Control",
    "85  CC Undefined",
    "86  CC Undefined",
    "87  CC Undefined",
    "88  CC High Resolution Velocity Prefix",
    "89  CC Undefined",
    "90  CC Undefined",
    "91  CC Effect 1 Depth - Reverb Send Level",
    "92  CC Effect 2 Depth - Tremolo Depth",
    "93  CC Effect 3 Depth - Chorus Send Level",
    "94  CC Effect 4 Depth - Celeste [Detune] Depth",
    "95  CC Effect 5 Depth - Phaser Depth",
    "96  CC Data Increment",
    "97  CC Data Decrement",
    "98  CC Non-Registered Parameter Number (NRPN) LSB",
    "99  CC Non-Registered Parameter Number (NRPN) MSB",
    "100 CC Registered Parameter Number (RPN) LSB",
    "101 CC Registered Parameter Number (RPN) MSB",
    "102 CC Undefined",
    "103 CC Undefined",
    "104 CC Undefined",
    "105 CC Undefined",
    "106 CC Undefined",
    "107 CC Undefined",
    "108 CC Undefined",
    "109 CC Undefined",
    "110 CC Undefined",
    "111 CC Undefined",
    "112 CC Undefined",
    "113 CC Undefined",
    "114 CC Undefined",
    "115 CC Undefined",
    "116 CC Undefined",
    "117 CC Undefined",
    "118 CC Undefined",
    "119 CC Undefined",
    "120 CC All Sound Off",
    "121 CC Reset All Controllers",
    "122 CC Local On/Off Switch",
    "123 CC All Notes Off",
    "124 MMC Wait, CC Omni Mode Off",
    "125 MMC Resume, CC Omni Mode On",
    "126 CC Mono Mode",
    "127 CC Poly Mode",
];

/// The MIDI channel options of a slot ("1" - "16", "All"; the index is the channel).
pub fn controller_channels() -> Vec<String> {
    to_options(&KEYBOARD_CHANNELS[1..])
}

/// The function setting of a command category.
pub fn function_setting(category: CommandCategory) -> SettingId {
    SLOT_FUNCTION_FIRST.offset(category.ordinal() as u16)
}

/// The category of a function setting, if it is one.
pub fn function_setting_category(id: SettingId) -> Option<CommandCategory> {
    let index = id.0.checked_sub(SLOT_FUNCTION_FIRST.0)? as usize;
    CommandCategory::ALL.get(index).copied()
}

/// The setting of the assignable action with the index (0-7).
pub fn action_setting(index: usize) -> SettingId {
    ACTION_FIRST.offset(index as u16)
}

/// The options of the function setting of a category: "Off" and the names of all commands of
/// the category (former `createFunctionSetting`).
pub fn function_options(category: CommandCategory) -> Vec<String> {
    let mut names = vec![FlexiCommand::Off.name().to_string()];
    names.extend(FlexiCommand::ALL.iter().filter(|c| c.category() == Some(category)).map(|c| c.name().to_string()));
    names
}

fn activate_text_setting(config: &mut Configuration, id: SettingId, category: &str, label: &str, default_value: &str) {
    config.activate_setting(SettingDescriptor {
        id,
        scope: Scope::Global,
        category: category.to_string(),
        label: label.to_string(),
        kind: SettingKind::Text,
        default_value: default_value.to_string(),
        observable: false,
    });
}

fn activate_enum(config: &mut Configuration, id: SettingId, category: &str, label: &str, options: Vec<String>, default_index: usize) {
    let default_value = options[default_index].clone();
    config.activate_enum_setting(id, Scope::Global, category, label, options, &default_value, false);
}

/// Create the configuration and activate all settings (the former `init`).
pub fn create_configuration(arpeggiator_modes: &[ArpeggiatorMode], note_repeat: NoteRepeatCapabilities) -> Configuration {
    let mut config = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;

    let slot_entries: Vec<String> = (1..=NUM_SLOTS).map(|i| i.to_string()).collect();
    activate_enum(&mut config, SLOT_SELECTED, CATEGORY_SLOT, "Selected:", slot_entries, 0);

    // The MIDI learn section
    activate_enum(&mut config, LEARN_TYPE, CATEGORY_LEARN, "Type:", to_options(&OPTIONS_TYPE), 0);
    activate_enum(&mut config, LEARN_NUMBER, CATEGORY_LEARN, "Number:", to_options(&NUMBER_NAMES), 0);
    activate_enum(&mut config, LEARN_MIDI_CHANNEL, CATEGORY_LEARN, "Midi Channel:", midi_channel_options(), 0);
    activate_enum(&mut config, LEARN_RESOLUTION, CATEGORY_LEARN, "Resolution:", to_options(&OPTIONS_RESOLUTION), 0);

    // Selected Slot - MIDI trigger
    activate_enum(&mut config, SLOT_TYPE, CATEGORY_SLOT_TRIGGER, "Type:", to_options(&OPTIONS_TYPE), 0);
    activate_enum(&mut config, SLOT_NUMBER, CATEGORY_SLOT_TRIGGER, "Number:", to_options(&NUMBER_NAMES), 0);
    activate_enum(&mut config, SLOT_MIDI_CHANNEL, CATEGORY_SLOT_TRIGGER, "Midi Channel:", controller_channels(), 0);
    activate_enum(&mut config, SLOT_RESOLUTION, CATEGORY_SLOT_TRIGGER, "Resolution:", to_options(&OPTIONS_RESOLUTION), 0);
    activate_enum(&mut config, SLOT_KNOB_MODE, CATEGORY_SLOT_TRIGGER, "Knob Mode:", KnobMode::labels(), 0);
    activate_enum(&mut config, SLOT_FUNCTION_LAYER, CATEGORY_SLOT_TRIGGER, "Functions Layer:", to_options(&FUNCTION_LAYERS), 0);

    // Selected Slot - MIDI device update
    activate_enum(&mut config, SLOT_SEND_VALUE, CATEGORY_SLOT_UPDATE, "Send value to device:", to_options(&ON_OFF_OPTIONS), 1);
    activate_enum(&mut config, SLOT_SEND_VALUE_WHEN_RECEIVED, CATEGORY_SLOT_UPDATE, "Send value to device when received (only buttons):", to_options(&ON_OFF_OPTIONS), 1);

    // Selected Slot - Function
    for category in CommandCategory::ALL {
        let label = format!("{}:", category.name());
        activate_enum(&mut config, function_setting(category), CATEGORY_SLOT_FUNCTION, &label, function_options(category), 0);
    }

    // Export/Import section: the file to auto-load
    activate_text_setting(&mut config, FILENAME, CATEGORY_LOAD_SAVE, "Filename", "");

    // Keyboard / Pads
    activate_text_setting(&mut config, KEYBOARD_INPUT_NAME, CATEGORY_KEYBOARD, "Input Name", GENERIC_FLEXI_NAME);
    config.activate_mpe_setting(global, CATEGORY_KEYBOARD, false);
    activate_enum(&mut config, KEYBOARD_CHANNEL, CATEGORY_KEYBOARD, "Midi Channel", to_options(&KEYBOARD_CHANNELS), 1);
    activate_enum(&mut config, KEYBOARD_ROUTE_TIMBRE, CATEGORY_KEYBOARD, "Route Timbre (CC74)", to_options(&ON_OFF_OPTIONS), 0);
    activate_enum(&mut config, KEYBOARD_ROUTE_MODULATION, CATEGORY_KEYBOARD, "Route Modulation (CC01)", to_options(&ON_OFF_OPTIONS), 1);
    activate_enum(&mut config, KEYBOARD_ROUTE_EXPRESSION, CATEGORY_KEYBOARD, "Route Expression (CC11)", to_options(&ON_OFF_OPTIONS), 0);
    activate_enum(&mut config, KEYBOARD_ROUTE_SUSTAIN, CATEGORY_KEYBOARD, "Route Sustain (CC64)", to_options(&ON_OFF_OPTIONS), 1);
    activate_enum(&mut config, KEYBOARD_ROUTE_PROGRAM_CHANGE, CATEGORY_KEYBOARD, "Route Program Changes", to_options(&ON_OFF_OPTIONS), 0);
    activate_enum(&mut config, KEYBOARD_ROUTE_PITCHBEND, CATEGORY_KEYBOARD, "Route Pitchbend", to_options(&ON_OFF_OPTIONS), 1);

    // Options
    config.activate_enum_setting(SELECTED_MODE, global, CATEGORY_OPTIONS, "Selected Mode", to_options(&MODES), MODES[0], true);
    for i in 0..NUM_ACTIONS {
        activate_text_setting(&mut config, action_setting(i), CATEGORY_OPTIONS, &format!("Action {}", i + 1), "");
    }

    // Workflow
    config.activate_knob_speed_setting(global, 100, 60, false);
    config.activate_exclude_deactivated_items_setting(global);
    config.activate_note_repeat_setting(Scope::Document, note_repeat);
    config.activate_new_clip_length_setting(global);

    // Transport
    config.activate_behaviour_on_pause_setting(global);
    config.activate_behaviour_on_stop_setting(global);

    config
}

/// Create the configuration with the Reaper arpeggiator modes and note repeat capabilities.
pub fn for_reaper() -> Configuration {
    create_configuration(&REAPER_ARP_MODES, NoteRepeatCapabilities::REAPER)
}

// ------------------------------------------------------------------------------------------
// Getters

fn is_on(config: &Configuration, id: SettingId) -> bool {
    config.value(id).as_deref() == Some(ON_OFF_OPTIONS[1])
}

/// The name of the mapping file.
pub fn filename(config: &Configuration) -> String {
    config.value(FILENAME).unwrap_or_default()
}

/// The name to use for the MIDI keyboard input.
pub fn keyboard_input_name(config: &Configuration) -> String {
    config.value(KEYBOARD_INPUT_NAME).unwrap_or_default()
}

/// The keyboard channel: -1 = off, 0-15 the MIDI channel, 16 = omni.
pub fn keyboard_channel(config: &Configuration) -> i32 {
    lookup_index(&KEYBOARD_CHANNELS, &config.value(KEYBOARD_CHANNEL).unwrap_or_default()) as i32 - 1
}

/// Should CC timbre be routed directly to the DAW?
pub fn is_keyboard_route_timbre(config: &Configuration) -> bool {
    is_on(config, KEYBOARD_ROUTE_TIMBRE)
}

/// Should CC modulation be routed directly to the DAW?
pub fn is_keyboard_route_modulation(config: &Configuration) -> bool {
    is_on(config, KEYBOARD_ROUTE_MODULATION)
}

/// Should CC expression be routed directly to the DAW?
pub fn is_keyboard_route_expression(config: &Configuration) -> bool {
    is_on(config, KEYBOARD_ROUTE_EXPRESSION)
}

/// Should CC sustain be routed directly to the DAW?
pub fn is_keyboard_route_sustain(config: &Configuration) -> bool {
    is_on(config, KEYBOARD_ROUTE_SUSTAIN)
}

/// Should program changes be routed directly to the DAW?
pub fn is_keyboard_route_program_change(config: &Configuration) -> bool {
    is_on(config, KEYBOARD_ROUTE_PROGRAM_CHANGE)
}

/// Should pitchbend be routed directly to the DAW?
pub fn is_keyboard_route_pitchbend(config: &Configuration) -> bool {
    is_on(config, KEYBOARD_ROUTE_PITCHBEND)
}

/// The name of the selected mode.
pub fn selected_mode_name(config: &Configuration) -> String {
    config.value(SELECTED_MODE).unwrap_or_default()
}

/// The ID of the Reaper action of an assignable action (0-7), empty if none.
pub fn assignable_action(config: &Configuration, index: usize) -> String {
    config.value(action_setting(index)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Properties;

    #[test]
    fn settings_and_keys() {
        let config = for_reaper();
        assert_eq!(filename(&config), "");
        assert_eq!(keyboard_input_name(&config), GENERIC_FLEXI_NAME);
        assert_eq!(keyboard_channel(&config), 0);
        assert!(is_keyboard_route_modulation(&config));
        assert!(is_keyboard_route_sustain(&config));
        assert!(is_keyboard_route_pitchbend(&config));
        assert!(!is_keyboard_route_timbre(&config));
        assert!(!is_keyboard_route_expression(&config));
        assert!(!is_keyboard_route_program_change(&config));
        assert_eq!(selected_mode_name(&config), "Track");
        assert_eq!(assignable_action(&config, 3), "");
        assert!(config.can_setting_be_observed(SELECTED_MODE));
        assert!(config.is_setting_active(SettingId::NOTEREPEAT_LENGTH));
        assert!(config.is_setting_active(SettingId::BEHAVIOUR_ON_STOP));
        assert_eq!(config.descriptor(SLOT_SELECTED).unwrap().key(), "SLOT_SELECTED:");
        assert_eq!(config.descriptor(FILENAME).unwrap().key(), "LOAD_/_SAVE_FILENAME");
        assert_eq!(config.descriptor(KEYBOARD_CHANNEL).unwrap().key(), "KEYBOARD_/_PADS_(REQUIRES_RESTART)_MIDI_CHANNEL");
        assert_eq!(config.descriptor(action_setting(0)).unwrap().key(), "OPTIONS_ACTION_1");
        assert_eq!(config.descriptor(function_setting(CommandCategory::Track)).unwrap().key(), "SELECTED_SLOT_-_FUNCTION_TRACK:");
        assert_eq!(function_setting_category(function_setting(CommandCategory::Action)), Some(CommandCategory::Action));
        assert_eq!(function_setting_category(FILENAME), None);
        assert_eq!(function_options(CommandCategory::Groove).len(), 4);
        assert_eq!(controller_channels().len(), 17);
    }

    #[test]
    fn persistence() {
        let mut config = for_reaper();
        config.set_value(FILENAME, "/tmp/my.properties");
        config.set_value(KEYBOARD_CHANNEL, "All");
        config.set_value(action_setting(1), "40001");
        config.set_value(SELECTED_MODE, "Device");
        let mut props = Properties::new();
        config.store(Scope::Global, &mut props);
        assert_eq!(props.get("LOAD_/_SAVE_FILENAME"), Some("/tmp/my.properties"));
        assert_eq!(props.get("OPTIONS_SELECTED_MODE"), Some("Device"));

        let mut loaded = for_reaper();
        loaded.load(Scope::Global, &Properties::parse(&props.to_text()));
        assert_eq!(filename(&loaded), "/tmp/my.properties");
        assert_eq!(keyboard_channel(&loaded), 16);
        assert_eq!(assignable_action(&loaded, 1), "40001");
        assert_eq!(selected_mode_name(&loaded), "Device");
    }
}
