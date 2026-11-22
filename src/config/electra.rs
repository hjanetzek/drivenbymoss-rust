// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings of the Electra One and the Electra One Mini (former
//! `AbstractElectraOneConfiguration`, `ElectraOneConfiguration` and
//! `ElectraOneMiniConfiguration`).
//!
//! Java notifies the one ID `KNOB_TOUCH_COMBINATION_COMMANDS` for all assignable settings and
//! gives the actions no ID. Every setting of the Rust version has an ID, so the assignable
//! functions and their actions get the IDs after [`LOG_TO_CONSOLE`] (Java uses the ID 50 for
//! it, which the Rust version has for the MPE bend range).

use super::configuration::{Configuration, CATEGORY_HARDWARE_SETUP};
use super::setting::{Scope, SettingDescriptor, SettingId, SettingKind};
use super::values::{to_options, ArpeggiatorMode, FOOTSWITCH_STOP_ALL_CLIPS, FOOTSWITCH_TOGGLE_PLAY, FOOTSWITCH_TOGGLE_RECORD, FOOTSWITCH_UNDO, ON_OFF_OPTIONS, REAPER_ARP_MODES};

/// Former `LOG_TO_CONSOLE`.
pub const LOG_TO_CONSOLE: SettingId = SettingId::NEXT_SETTING_ID;
/// The first of the assignable knob touch combinations (former `KNOB_TOUCH_COMBINATION_COMMANDS`).
pub const ASSIGNABLE_1: SettingId = SettingId::NEXT_SETTING_ID.offset(1);
/// The first of the actions of the assignable knob touch combinations (no IDs in Java).
pub const ASSIGNABLE_ACTION_1: SettingId = SettingId::NEXT_SETTING_ID.offset(1 + NUM_ASSIGNABLES_MAX as u16);

/// The most knob touch combinations of a model (the normal one).
pub const NUM_ASSIGNABLES_MAX: usize = 38;

const CATEGORY_ASSIGNABLE_BUTTONS: &str = "Assignable button touches";

// The functions of the normal model (former `ElectraOneConfiguration.ELECTRA_ONE_FUNC_*`)

pub const ELECTRA_ONE_FUNC_OFF: usize = 0;
pub const ELECTRA_ONE_FUNC_SHIFT_BUTTON: usize = 1;
pub const ELECTRA_ONE_FUNC_MODE_MIXER: usize = 2;
pub const ELECTRA_ONE_FUNC_MODE_SENDS: usize = 3;
pub const ELECTRA_ONE_FUNC_MODE_DEVICE: usize = 4;
pub const ELECTRA_ONE_FUNC_MODE_EQ: usize = 5;
pub const ELECTRA_ONE_FUNC_MODE_TRANSPORT: usize = 6;
pub const ELECTRA_ONE_FUNC_MODE_SESSION: usize = 7;
pub const ELECTRA_ONE_FUNC_MODE_PARAMS: usize = 8;
pub const ELECTRA_ONE_FUNC_MODE_SYNTH_PRESET: usize = 9;
pub const ELECTRA_ONE_FUNC_MODE_ACTION: usize = 25;
const FOOTSWITCH_CMDS_OFFSET: usize = ELECTRA_ONE_FUNC_MODE_SYNTH_PRESET + 1;

// The functions of the Mini (former `ElectraOneMiniConfiguration.ELECTRA_ONE_FUNC_*`)

pub const MINI_FUNC_OFF: usize = 0;
pub const MINI_FUNC_MODE_MIXER: usize = 1;
pub const MINI_FUNC_MODE_DEVICE: usize = 2;
pub const MINI_FUNC_MODE_TRANSPORT: usize = 3;
/// As in Java 25, although "Action" is the option 19 of the Mini (see
/// doc/UPSTREAM_CANDIDATES.md).
pub const MINI_FUNC_MODE_ACTION: usize = 25;
const MINI_FOOTSWITCH_CMDS_OFFSET: usize = MINI_FUNC_MODE_TRANSPORT + 1;

/// The options of the normal model (former `ASSIGNABLE_VALUES`).
pub const ASSIGNABLE_VALUES: [&str; 26] = [
    "Off",
    "Shift Button",
    "Mode: Mixer",
    "Mode: Sends",
    "Mode: Device",
    "Mode: EQ",
    "Mode: Transport",
    "Mode: Session",
    "Mode: Project/Track Parameters",
    "Mode: Select Synth Preset",
    "Function: Toggle Play",
    "Function: Toggle Record",
    "Function: Stop All Clips",
    "Function: Toggle Clip Overdub",
    "Function: Undo",
    "Function: Tap Tempo",
    "Function: New Button",
    "Function: Clip Based Looper",
    "Function: Panel layout arrange",
    "Function: Panel layout mix",
    "Function: Panel layout edit",
    "Function: Add instrument track",
    "Function: Add audio track",
    "Function: Add effect track",
    "Function: Quantize",
    "Action",
];

/// The options of the Mini (former `ElectraOneMiniConfiguration.ASSIGNABLE_VALUES`).
pub const MINI_ASSIGNABLE_VALUES: [&str; 20] = [
    "Off",
    "Mode: Mixer",
    "Mode: Device",
    "Mode: Transport",
    "Function: Toggle Play",
    "Function: Toggle Record",
    "Function: Stop All Clips",
    "Function: Toggle Clip Overdub",
    "Function: Undo",
    "Function: Tap Tempo",
    "Function: New Button",
    "Function: Clip Based Looper",
    "Function: Panel layout arrange",
    "Function: Panel layout mix",
    "Function: Panel layout edit",
    "Function: Add instrument track",
    "Function: Add audio track",
    "Function: Add effect track",
    "Function: Quantize",
    "Action",
];

/// The names of the knob touch combinations of the normal model (former
/// `ASSIGNABLE_BUTTON_NAMES`), in the order of the touch patterns.
pub const ASSIGNABLE_BUTTON_NAMES: [&str; 38] = [
    "Linear 1 - 3",
    "Linear 2 - 4",
    "Linear 3 - 5",
    "Linear 4 - 6",
    "Linear 7 - 9",
    "Linear 8 - 10",
    "Linear 9 - 11",
    "Linear 10 - 12",
    "Linear Left 1, 2, 4",
    "Linear Left 2, 3, 5",
    "Linear Left 3, 4, 6",
    "Linear Left 7, 8, 10",
    "Linear Left 8, 9, 11",
    "Linear Left 9, 10, 12",
    "Linear Right 1, 3, 4",
    "Linear Right 2, 4, 5",
    "Linear Right 3, 5, 6",
    "Linear Right 7, 9, 10",
    "Linear Right 8, 10, 11",
    "Linear Right 9, 11, 12",
    "Linear Broad 1, 3, 5",
    "Linear Broad 2, 4, 6",
    "Linear Broad 7, 9, 11",
    "Linear Broad 8, 10, 12",
    "Corner Left 1, 2, 7",
    "Corner Left 2, 3, 8",
    "Corner Left 3, 4, 9",
    "Corner Left 4, 5, 10",
    "Corner Left 5, 6, 11",
    "Corner Right 1, 2, 8",
    "Corner Right 2, 3, 9",
    "Corner Right 3, 4, 10",
    "Corner Right 4, 5, 11",
    "Corner Right 5, 6, 12",
    "Triangle 2, 7, 9",
    "Triangle 3, 8, 10",
    "Triangle 4, 9, 11",
    "Triangle 5, 10, 12",
];

/// The names of the knob touch combinations of the Mini.
pub const MINI_ASSIGNABLE_BUTTON_NAMES: [&str; 16] = [
    "Linear 1 - 3",
    "Linear 2 - 4",
    "Linear 5 - 7",
    "Linear 6 - 8",
    "Linear Left 1, 2, 4",
    "Linear Left 5, 6, 8",
    "Linear Right 1, 3, 4",
    "Linear Right 5, 7, 8",
    "Corner Left 1, 2, 5",
    "Corner Left 2, 3, 6",
    "Corner Left 3, 4, 7",
    "Corner Right 1, 2, 6",
    "Corner Right 2, 3, 7",
    "Corner Right 3, 4, 8",
    "Triangle 2, 5, 7",
    "Triangle 3, 6, 8",
];

/// The default functions of the normal model (former `ASSIGNABLE_BUTTON_DEFAULTS`).
pub const ASSIGNABLE_BUTTON_DEFAULTS: [usize; 38] = [
    // Linear
    ELECTRA_ONE_FUNC_MODE_MIXER,
    ELECTRA_ONE_FUNC_MODE_SENDS,
    ELECTRA_ONE_FUNC_MODE_DEVICE,
    ELECTRA_ONE_FUNC_MODE_EQ,
    ELECTRA_ONE_FUNC_MODE_TRANSPORT,
    ELECTRA_ONE_FUNC_MODE_SESSION,
    ELECTRA_ONE_FUNC_MODE_PARAMS,
    ELECTRA_ONE_FUNC_MODE_SYNTH_PRESET,
    // Linear Left
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    // Linear Right
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    // Linear Broad
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    // Corner Left
    ELECTRA_ONE_FUNC_SHIFT_BUTTON,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    // Corner Right
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_OFF,
    ELECTRA_ONE_FUNC_SHIFT_BUTTON,
    // Triangle
    FOOTSWITCH_CMDS_OFFSET + FOOTSWITCH_TOGGLE_PLAY as usize,
    FOOTSWITCH_CMDS_OFFSET + FOOTSWITCH_TOGGLE_RECORD as usize,
    FOOTSWITCH_CMDS_OFFSET + FOOTSWITCH_STOP_ALL_CLIPS as usize,
    FOOTSWITCH_CMDS_OFFSET + FOOTSWITCH_UNDO as usize,
];

/// The default functions of the Mini.
pub const MINI_ASSIGNABLE_BUTTON_DEFAULTS: [usize; 16] = [
    // Linear
    MINI_FUNC_MODE_MIXER,
    MINI_FUNC_MODE_DEVICE,
    MINI_FUNC_OFF,
    MINI_FUNC_MODE_TRANSPORT,
    // Linear Left
    MINI_FUNC_OFF,
    MINI_FUNC_OFF,
    // Linear Right
    MINI_FUNC_OFF,
    MINI_FUNC_OFF,
    // Corner Left
    MINI_FUNC_OFF,
    MINI_FUNC_OFF,
    MINI_FUNC_OFF,
    // Corner Right
    MINI_FUNC_OFF,
    MINI_FUNC_OFF,
    MINI_FUNC_OFF,
    // Triangle
    MINI_FOOTSWITCH_CMDS_OFFSET + FOOTSWITCH_TOGGLE_PLAY as usize,
    MINI_FOOTSWITCH_CMDS_OFFSET + FOOTSWITCH_TOGGLE_RECORD as usize,
];

/// The ID of the assignable function setting of a knob touch combination.
pub fn assignable_setting(index: usize) -> SettingId {
    ASSIGNABLE_1.offset(index as u16)
}

/// The ID of the action setting of a knob touch combination.
pub fn assignable_action_setting(index: usize) -> SettingId {
    ASSIGNABLE_ACTION_1.offset(index as u16)
}

/// True if the setting is one of the assignable functions (former observer of
/// `KNOB_TOUCH_COMBINATION_COMMANDS`).
pub fn is_assignable_setting(id: SettingId) -> bool {
    (ASSIGNABLE_1.0..ASSIGNABLE_1.0 + NUM_ASSIGNABLES_MAX as u16).contains(&id.0)
}

/// Former `isLogToConsoleEnabled`.
pub fn is_log_to_console_enabled(config: &Configuration) -> bool {
    config.custom_bool(LOG_TO_CONSOLE)
}

/// The index of the function of a knob touch combination (former `getAssignable`).
pub fn assignable(config: &Configuration, index: usize) -> usize {
    config.custom_index(assignable_setting(index))
}

/// The action of a knob touch combination (former `getAssignableAction`).
pub fn assignable_action(config: &Configuration, index: usize) -> String {
    config.value(assignable_action_setting(index)).unwrap_or_default()
}

/// Create the configuration and activate all settings in the order of the Java `init`.
fn create(arpeggiator_modes: &[ArpeggiatorMode], names: &[&str], defaults: &[usize], values: &[&str], default_knob_speed_normal: usize) -> Configuration {
    let mut c = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;

    // Hardware
    c.activate_enum_setting(
        LOG_TO_CONSOLE,
        global,
        CATEGORY_HARDWARE_SETUP,
        "Enable Electra.One logging (written to Controller Script Console)",
        to_options(&ON_OFF_OPTIONS),
        ON_OFF_OPTIONS[0],
        true,
    );
    c.activate_knob_speed_setting(global, default_knob_speed_normal, 20, false);

    // Transport
    c.activate_behaviour_on_pause_setting(global);
    c.activate_double_click_play_button(global);

    // Workflow
    c.activate_exclude_deactivated_items_setting(global);
    c.activate_new_clip_length_setting(global);

    // Assignable knob touch combinations
    let options = to_options(values);
    for (i, name) in names.iter().enumerate() {
        c.activate_enum_setting(assignable_setting(i), global, CATEGORY_ASSIGNABLE_BUTTONS, name, options.clone(), values[defaults[i]], true);
        c.activate_setting(SettingDescriptor {
            id: assignable_action_setting(i),
            scope: global,
            category: CATEGORY_ASSIGNABLE_BUTTONS.to_string(),
            label: format!("{name} - Action"),
            kind: SettingKind::Action,
            default_value: String::new(),
            observable: false,
        });
    }
    c
}

/// The configuration of the Electra One Mk1/Mk2 (former `ElectraOneConfiguration`).
pub fn create_normal(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    create(arpeggiator_modes, &ASSIGNABLE_BUTTON_NAMES, &ASSIGNABLE_BUTTON_DEFAULTS, &ASSIGNABLE_VALUES, 100)
}

/// The configuration of the Electra One Mini (former `ElectraOneMiniConfiguration`).
pub fn create_mini(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    create(arpeggiator_modes, &MINI_ASSIGNABLE_BUTTON_NAMES, &MINI_ASSIGNABLE_BUTTON_DEFAULTS, &MINI_ASSIGNABLE_VALUES, 50)
}

/// Create the configuration with the Reaper arpeggiator modes (the Bitwig extension uses the
/// same list, see the other controllers).
pub fn for_reaper(is_mini: bool) -> Configuration {
    if is_mini {
        create_mini(&REAPER_ARP_MODES)
    } else {
        create_normal(&REAPER_ARP_MODES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings() {
        let c = for_reaper(false);
        assert!(!is_log_to_console_enabled(&c));
        assert_eq!(assignable(&c, 0), ELECTRA_ONE_FUNC_MODE_MIXER);
        assert_eq!(assignable(&c, 7), ELECTRA_ONE_FUNC_MODE_SYNTH_PRESET);
        assert_eq!(assignable(&c, 24), ELECTRA_ONE_FUNC_SHIFT_BUTTON);
        assert_eq!(assignable(&c, 34), 10);
        assert_eq!(assignable(&c, 37), 14);
        assert_eq!(assignable_action(&c, 3), "");
        assert_eq!(c.knob_sensitivity_default(), 0);
        let descriptor = c.descriptor(assignable_action_setting(37)).unwrap();
        assert_eq!(descriptor.label, "Triangle 5, 10, 12 - Action");
        assert_eq!(descriptor.kind, SettingKind::Action);
        assert!(is_assignable_setting(assignable_setting(37)));
        assert!(!is_assignable_setting(assignable_action_setting(0)));

        let mini = for_reaper(true);
        assert_eq!(assignable(&mini, 3), MINI_FUNC_MODE_TRANSPORT);
        assert_eq!(mini.value(assignable_setting(14)).as_deref(), Some("Function: Toggle Play"));
        assert_eq!(mini.knob_sensitivity_default(), -50);
        assert!(mini.descriptor(assignable_setting(16)).is_none());
    }
}
