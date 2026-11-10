// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration settings of the OSC controller (former `OSCConfiguration` and
//! `AbstractOpenSoundControlConfiguration`).
//!
//! Java gives only the observable settings an ID; the others (send host and port, bank page
//! size, the actions) get the following IDs here, since every setting of the Rust version has
//! one. The settings marked "requires restart" are read when the controller is created.

use super::configuration::{Configuration, CATEGORY_DEBUG};
use super::setting::{Scope, SettingDescriptor, SettingId, SettingKind};
use super::values::{to_options, ArpeggiatorMode, ON_OFF_OPTIONS, REAPER_ARP_MODES};

/// Former `LOG_INPUT_COMMANDS`.
pub const LOG_INPUT_COMMANDS: SettingId = SettingId::NEXT_SETTING_ID;
/// Former `LOG_OUTPUT_COMMANDS`.
pub const LOG_OUTPUT_COMMANDS: SettingId = SettingId::NEXT_SETTING_ID.offset(1);
/// Former `FILTER_HEARTBEAT_COMMANDS`.
pub const FILTER_HEARTBEAT_COMMANDS: SettingId = SettingId::NEXT_SETTING_ID.offset(2);
/// Former `RECEIVE_PORT`.
pub const RECEIVE_PORT: SettingId = SettingId::NEXT_SETTING_ID.offset(3);
/// Former `VALUE_RESOLUTION`.
pub const VALUE_RESOLUTION: SettingId = SettingId::NEXT_SETTING_ID.offset(4);
/// The host to send to (no ID in Java).
pub const SEND_HOST: SettingId = SettingId::NEXT_SETTING_ID.offset(5);
/// The port to send to (no ID in Java).
pub const SEND_PORT: SettingId = SettingId::NEXT_SETTING_ID.offset(6);
/// The bank page size (no ID in Java).
pub const BANK_PAGE_SIZE: SettingId = SettingId::NEXT_SETTING_ID.offset(7);
/// The first of the assignable actions (no IDs in Java).
pub const ACTION_1: SettingId = SettingId::NEXT_SETTING_ID.offset(8);

/// The number of assignable actions (former `NUM_ASSIGNABLE_ACTIONS`).
pub const NUM_ASSIGNABLE_ACTIONS: usize = 20;

/// The default host to send to (former `DEFAULT_SERVER`).
pub const DEFAULT_SERVER: &str = "127.0.0.1";
/// The default port to receive on.
pub const DEFAULT_RECEIVE_PORT: u16 = 8000;
/// The default port to send to.
pub const DEFAULT_SEND_PORT: u16 = 9000;

const CATEGORY_SETUP: &str = "Setup";
const CATEGORY_PROTOCOL: &str = "Protocol (must match your client template!)";
const CATEGORY_ACTIONS: &str = "Actions";

/// The options of the value resolution setting.
pub const VALUE_RESOLUTION_OPTIONS: [&str; 3] = ["Low (128)", "Medium (1024)", "High (16384)"];

/// The resolution of the values (former `ValueResolution`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueResolution {
    /// Low resolution, 128 values.
    Low,
    /// Medium resolution, 1024 values.
    Medium,
    /// High resolution, 16384 values.
    High,
}

impl ValueResolution {
    /// The upper bound and the step size of the value changer (former observer of
    /// `VALUE_RESOLUTION` in `OSCControllerSetup.createObservers`).
    pub fn value_changer_range(self) -> (i32, i32) {
        match self {
            ValueResolution::Low => (128, 1),
            ValueResolution::Medium => (1024, 8),
            ValueResolution::High => (16384, 128),
        }
    }
}

/// The ID of the assignable action setting with the index (0-19).
pub fn action_setting(index: usize) -> SettingId {
    ACTION_1.offset(index as u16)
}

fn activate(config: &mut Configuration, id: SettingId, category: &str, label: &str, kind: SettingKind, default_value: &str, observable: bool) {
    config.activate_setting(SettingDescriptor {
        id,
        scope: Scope::Global,
        category: category.to_string(),
        label: label.to_string(),
        kind,
        default_value: default_value.to_string(),
        observable,
    });
}

/// Create the configuration and activate all settings in the order of the Java `init`.
pub fn create(arpeggiator_modes: &[ArpeggiatorMode]) -> Configuration {
    let mut c = Configuration::new(arpeggiator_modes);
    let global = Scope::Global;

    // Network
    let port = SettingKind::Integer { min: 1024, max: 65535 };
    activate(&mut c, RECEIVE_PORT, CATEGORY_SETUP, "Port to receive on", port.clone(), &DEFAULT_RECEIVE_PORT.to_string(), true);
    activate(&mut c, SEND_HOST, CATEGORY_SETUP, "Host to send to (requires restart)", SettingKind::Text, DEFAULT_SERVER, false);
    activate(&mut c, SEND_PORT, CATEGORY_SETUP, "Port to send to (requires restart)", port, &DEFAULT_SEND_PORT.to_string(), false);

    // Protocol
    c.activate_enum_setting(VALUE_RESOLUTION, global, CATEGORY_PROTOCOL, "Value resolution", to_options(&VALUE_RESOLUTION_OPTIONS), VALUE_RESOLUTION_OPTIONS[0], true);
    let page_sizes: Vec<String> = (1..=200).map(|i| i.to_string()).collect();
    c.activate_enum_setting(BANK_PAGE_SIZE, global, CATEGORY_PROTOCOL, "Bank Page Size (requires restart)", page_sizes, "8", false);

    // Transport
    c.activate_behaviour_on_stop_setting(global);

    // Play and Sequence
    c.activate_track_navigation_setting(global, None, true);
    c.activate_accent_active_setting(global);
    c.activate_accent_value_setting(global);

    // Workflow
    c.activate_exclude_deactivated_items_setting(global);
    c.activate_enable_vu_meters_setting(global, super::configuration::CATEGORY_WORKFLOW);

    // Actions
    for i in 0..NUM_ASSIGNABLE_ACTIONS {
        activate(&mut c, action_setting(i), CATEGORY_ACTIONS, &format!("Action {}", i + 1), SettingKind::Action, "", false);
    }

    // Debug (former activateOSCLogging)
    let on_off = to_options(&ON_OFF_OPTIONS);
    c.activate_enum_setting(LOG_INPUT_COMMANDS, global, CATEGORY_DEBUG, "Log input commands", on_off.clone(), ON_OFF_OPTIONS[0], true);
    c.activate_enum_setting(LOG_OUTPUT_COMMANDS, global, CATEGORY_DEBUG, "Log output commands", on_off.clone(), ON_OFF_OPTIONS[0], true);
    c.activate_enum_setting(FILTER_HEARTBEAT_COMMANDS, global, CATEGORY_DEBUG, "Filter heartbeat commands (ping etc.)", on_off, ON_OFF_OPTIONS[0], true);

    c
}

/// Create the configuration with the Reaper arpeggiator modes (the Bitwig extension uses the
/// same list, see the other controllers).
pub fn for_reaper() -> Configuration {
    create(&REAPER_ARP_MODES)
}

/// The port of the host on which the extension receives OSC messages (former `getReceivePort`).
pub fn receive_port(config: &Configuration) -> u16 {
    u16::try_from(config.custom_int(RECEIVE_PORT)).ok().filter(|&p| p != 0).unwrap_or(DEFAULT_RECEIVE_PORT)
}

/// The host to which the extension sends OSC messages (former `getSendHost`).
pub fn send_host(config: &Configuration) -> String {
    config.value(SEND_HOST).filter(|h| !h.trim().is_empty()).map(|h| h.trim().to_string()).unwrap_or_else(|| DEFAULT_SERVER.to_string())
}

/// The port to which the extension sends OSC messages (former `getSendPort`).
pub fn send_port(config: &Configuration) -> u16 {
    u16::try_from(config.custom_int(SEND_PORT)).ok().filter(|&p| p != 0).unwrap_or(DEFAULT_SEND_PORT)
}

/// The selected value resolution (former `getValueResolution`).
pub fn value_resolution(config: &Configuration) -> ValueResolution {
    match config.custom_index(VALUE_RESOLUTION) {
        1 => ValueResolution::Medium,
        2 => ValueResolution::High,
        _ => ValueResolution::Low,
    }
}

/// The bank page size (former `getBankPageSize`).
pub fn bank_page_size(config: &Configuration) -> usize {
    config.custom_index(BANK_PAGE_SIZE) + 1
}

/// The ID of the action to execute for the assignable action (former `getAssignableAction`),
/// empty if none is assigned.
pub fn assignable_action(config: &Configuration, index: usize) -> String {
    config.value(action_setting(index)).unwrap_or_default()
}

/// Former `shouldLogInputCommands`.
pub fn should_log_input_commands(config: &Configuration) -> bool {
    config.custom_bool(LOG_INPUT_COMMANDS)
}

/// Former `shouldLogOutputCommands`.
pub fn should_log_output_commands(config: &Configuration) -> bool {
    config.custom_bool(LOG_OUTPUT_COMMANDS)
}

/// Former `filterHeartbeatMessages`.
pub fn filter_heartbeat_messages(config: &Configuration) -> bool {
    config.custom_bool(FILTER_HEARTBEAT_COMMANDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::properties::Properties;

    #[test]
    fn defaults() {
        let c = for_reaper();
        assert_eq!(receive_port(&c), 8000);
        assert_eq!(send_host(&c), "127.0.0.1");
        assert_eq!(send_port(&c), 9000);
        assert_eq!(value_resolution(&c), ValueResolution::Low);
        assert_eq!(bank_page_size(&c), 8);
        assert_eq!(assignable_action(&c, 0), "");
        assert!(!should_log_input_commands(&c));
        assert!(!should_log_output_commands(&c));
        assert!(!filter_heartbeat_messages(&c));
        assert!(c.is_track_navigation_flat());
        assert!(c.is_enable_vu_meters());
        assert!(!c.is_accent_active());
        for id in [RECEIVE_PORT, VALUE_RESOLUTION, LOG_INPUT_COMMANDS, LOG_OUTPUT_COMMANDS, FILTER_HEARTBEAT_COMMANDS] {
            assert!(c.can_setting_be_observed(id), "{id:?}");
        }
        for id in [SEND_HOST, SEND_PORT, BANK_PAGE_SIZE, action_setting(19)] {
            assert!(c.is_setting_active(id) && !c.can_setting_be_observed(id), "{id:?}");
        }
        // The order of the Java init
        let labels: Vec<&str> = c.descriptors().iter().map(|d| d.label.as_str()).collect();
        assert_eq!(&labels[..5], ["Port to receive on", "Host to send to (requires restart)", "Port to send to (requires restart)", "Value resolution", "Bank Page Size (requires restart)"]);
        assert_eq!(labels.last(), Some(&"Filter heartbeat commands (ping etc.)"));
        assert_eq!(ValueResolution::High.value_changer_range(), (16384, 128));
    }

    #[test]
    fn persistence() {
        let mut c = for_reaper();
        assert!(c.set_value(RECEIVE_PORT, "8100"));
        assert!(!c.set_value(RECEIVE_PORT, "80"), "below the range");
        c.set_value(SEND_HOST, "192.168.1.20");
        c.set_value(SEND_PORT, "9100");
        c.set_value(VALUE_RESOLUTION, VALUE_RESOLUTION_OPTIONS[2]);
        c.set_value(BANK_PAGE_SIZE, "16");
        c.set_value(action_setting(2), "1234");
        c.set_value(LOG_INPUT_COMMANDS, "On");
        let mut props = Properties::new();
        c.store(Scope::Global, &mut props);
        assert_eq!(props.get("SETUP_PORT_TO_RECEIVE_ON"), Some("8100"));

        let mut d = for_reaper();
        d.load(Scope::Global, &Properties::parse(&props.to_text()));
        assert_eq!(receive_port(&d), 8100);
        assert_eq!(send_host(&d), "192.168.1.20");
        assert_eq!(send_port(&d), 9100);
        assert_eq!(value_resolution(&d), ValueResolution::High);
        assert_eq!(bank_page_size(&d), 16);
        assert_eq!(assignable_action(&d, 2), "1234");
        assert!(should_log_input_commands(&d));
    }
}
