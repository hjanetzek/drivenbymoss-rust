// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definitions of the Electra One models (former `ElectraOneControllerDefinition`,
//! `ElectraOneMiniControllerDefinition`, the constants of `AbstractElectraOneControlSurface`,
//! `ElectraOneControlSurface` and `ElectraOneMiniControlSurface`, and the SysEx builders).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};
use crate::framework::ids::{ButtonId, ContinuousId, Modes};

/// The two DrivenByMoss controllers of the Electra One series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElectraVariant {
    /// The Electra One Mk1 and Mk2 (former `normal` package): 12 knobs, a 6x6 grid per page.
    Normal,
    /// The Electra One Mini (former `mini` package): 8 pushable knobs and 4 buttons.
    Mini,
}

impl ElectraVariant {
    pub const ALL: [ElectraVariant; 2] = [ElectraVariant::Normal, ElectraVariant::Mini];

    pub fn uuid(self) -> &'static str {
        match self {
            ElectraVariant::Normal => "24342204-B891-4F62-BC43-8ABA1DE1D690",
            ElectraVariant::Mini => "610CBC4D-E7D0-4D62-BCEB-97E91419C73F",
        }
    }

    pub fn hardware_model(self) -> &'static str {
        match self {
            ElectraVariant::Normal => "Electra One",
            ElectraVariant::Mini => "Electra One Mini",
        }
    }

    /// The fully qualified name of the Java instance class of DrivenByMoss4Reaper.
    pub fn java_class_name(self) -> &'static str {
        match self {
            ElectraVariant::Normal => "de.mossgrabers.reaper.controller.electra.one.ElectraOneControllerInstance",
            ElectraVariant::Mini => "de.mossgrabers.reaper.controller.electra.one.ElectraOneMiniControllerInstance",
        }
    }

    /// 2 MIDI inputs and outputs: Port 1 (the controls) and CTRL (the SysEx API).
    pub fn info(self) -> ControllerDefinitionInfo {
        ControllerDefinitionInfo {
            uuid: self.uuid().to_string(),
            vendor: "Electra".to_string(),
            hardware_model: self.hardware_model().to_string(),
            num_midi_in_ports: 2,
            num_midi_out_ports: 2,
        }
    }

    /// The name of the preset which DrivenByMoss selects on the device (former `templateName`).
    pub fn template_name(self) -> &'static str {
        match self {
            ElectraVariant::Normal => "DrivenByMoss",
            ElectraVariant::Mini => "DrivenByMoss Mini",
        }
    }

    /// The number of knobs (and of the knob states for the touch combinations).
    pub fn number_of_knobs(self) -> usize {
        match self {
            ElectraVariant::Normal => 12,
            ElectraVariant::Mini => 8,
        }
    }

    /// The modes in the order of the preset pages (former `MODES`).
    pub fn modes(self) -> &'static [Modes] {
        match self {
            ElectraVariant::Normal => &NORMAL_MODES,
            ElectraVariant::Mini => &MINI_MODES,
        }
    }

    /// The knob touch patterns (former `TOUCH_PATTERNS`).
    pub fn touch_patterns(self) -> &'static [&'static [i32]] {
        match self {
            ElectraVariant::Normal => &NORMAL_TOUCH_PATTERNS,
            ElectraVariant::Mini => &MINI_TOUCH_PATTERNS,
        }
    }

    /// The rows and columns of a page of controls (the page cache).
    pub fn grid(self) -> (usize, usize) {
        match self {
            ElectraVariant::Normal => (6, 6),
            ElectraVariant::Mini => (2, 8),
        }
    }

    /// The controls of the modes (former `KNOB_IDS`).
    pub fn knob_ids(self) -> Vec<ContinuousId> {
        match self {
            ElectraVariant::Normal => NORMAL_CTRL_ROW_IDS.iter().flat_map(|first| (0..6).map(move |i| ContinuousId::get(*first, i))).collect(),
            ElectraVariant::Mini => (0..8).map(|i| ContinuousId::get(ContinuousId::Knob1, i)).collect(),
        }
    }

    /// The port names for the automatic detection of the device (former
    /// `getMidiDiscoveryPairs`, the same for both): the first port is Port 1, the second CTRL.
    pub fn midi_discovery_pairs(self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        fn names(list: &[&str]) -> Vec<String> {
            list.iter().map(|s| s.to_string()).collect()
        }
        let mut pairs = Vec::new();
        match os {
            OperatingSystem::Windows => {
                pairs.push((names(&["Electra Controller", "MIDIIN3 (Electra Controller)"]), names(&["Electra Controller", "MIDIOUT3 (Electra Controller)"])));
                for i in 2..10 {
                    let inputs = vec![format!("Electra Controller #{i}"), format!("MIDIIN{} (Electra Controller)", 3 * i)];
                    let outputs = vec![format!("Electra Controller #{i}"), format!("MIDIOUT{} (Electra Controller)", 3 * i)];
                    pairs.push((inputs, outputs));
                }
            }
            OperatingSystem::Mac => {
                let ports = names(&["Electra Controller Electra Port 1", ELECTRA_CONTROLLER_ELECTRA_CTRL]);
                pairs.push((ports.clone(), ports));
            }
            OperatingSystem::Linux => {
                // As in Java: "Electra Port" (Bitwig matches it with "Electra Port 1")
                let ports = names(&["Electra Controller Electra Port", ELECTRA_CONTROLLER_ELECTRA_CTRL]);
                pairs.push((ports.clone(), ports));
            }
        }
        pairs
    }
}

const ELECTRA_CONTROLLER_ELECTRA_CTRL: &str = "Electra Controller Electra CTRL";

// The MIDI of the controls (all on MIDI channel 16)

/// The MIDI channel of the controls (16).
pub const ELECTRA_MIDI_CHANNEL: i32 = 15;
/// The CC of the first control of the normal model (former `ELECTRA_CTRL_1`): row r, column c
/// is `10 + 10 * r + c`. The page cache of the Mini uses it as well.
pub const ELECTRA_CTRL_1: i32 = 10;
/// The CC of the first pad of the normal model (former `ELECTRA_ROW_1`).
pub const ELECTRA_ROW_1: i32 = 70;
/// The CC of the first knob of the Mini (former `ELECTRA_KNOB_1`).
pub const ELECTRA_MINI_KNOB_1: i32 = 10;
/// The CC of the push of the first knob of the Mini (former `ELECTRA_CTRL_PUSH_1`).
pub const ELECTRA_MINI_CTRL_PUSH_1: i32 = 20;
/// The CC of the first function button of the Mini (former `ELECTRA_FUNCTION_BUTTON_1`).
pub const ELECTRA_MINI_FUNCTION_BUTTON_1: i32 = 30;
/// The CC of the touch of the first knob of the Mini (former `ELECTRA_KNOB_TOUCH_1`, unused).
pub const ELECTRA_MINI_KNOB_TOUCH_1: i32 = 40;

/// The CC of a control of the 6x6 grid (and of the page cache of both models).
pub fn ctrl_cc(row: usize, column: usize) -> i32 {
    ELECTRA_CTRL_1 + 10 * row as i32 + column as i32
}

/// The first continuous ID of each row of the normal model (former `CTRL_ROW_IDS`).
const NORMAL_CTRL_ROW_IDS: [ContinuousId; 6] =
    [ContinuousId::VolumeKnob1, ContinuousId::PanKnob1, ContinuousId::Fader1, ContinuousId::Knob1, ContinuousId::ParamKnob1, ContinuousId::DeviceKnob1];

/// The first button ID of each row of the normal model (former `BUTTON_ROW_IDS`).
const NORMAL_BUTTON_ROW_IDS: [ButtonId; 6] = [ButtonId::Row11, ButtonId::Row21, ButtonId::Row31, ButtonId::Row41, ButtonId::Row51, ButtonId::Row61];

/// The button of a pad of the normal model (former `ElectraOneControlSurface.getButtonID`).
pub fn button_id(row: usize, column: usize) -> ButtonId {
    ButtonId::get(NORMAL_BUTTON_ROW_IDS[row], column)
}

/// The control of the normal model (former `ElectraOneControlSurface.getContinuousID`).
pub fn continuous_id(row: usize, column: usize) -> ContinuousId {
    ContinuousId::get(NORMAL_CTRL_ROW_IDS[row], column)
}

const NORMAL_MODES: [Modes; 7] = [Modes::Volume, Modes::Send, Modes::DeviceParams, Modes::EqDeviceParams, Modes::Transport, Modes::Session, Modes::Project];
const MINI_MODES: [Modes; 3] = [Modes::Volume, Modes::DeviceParams, Modes::Transport];

// The knob touch patterns: 0 = not touched, 1 = touched

#[rustfmt::skip]
const NORMAL_TOUCH_PATTERNS: [&[i32]; 38] = [
    // Linear 1 - 3, 2 - 4, 3 - 5, 4 - 6, 7 - 9, 8 - 10, 9 - 11, 10 - 12
    &[1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0],
    &[0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1],
    // Linear Left 1, 2, 4 / 2, 3, 5 / 3, 4, 6 / 7, 8, 10 / 8, 9, 11 / 9, 10, 12
    &[1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0],
    &[0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1],
    // Linear Right 1, 3, 4 / 2, 4, 5 / 3, 5, 6 / 7, 9, 10 / 8, 10, 11 / 9, 11, 12
    &[1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0],
    &[0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1],
    // Linear Broad 1, 3, 5 / 2, 4, 6 / 7, 9, 11 / 8, 10, 12
    &[1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    &[0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0],
    &[0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0],
    &[0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1],
    // Corner Left 1, 2, 7 / 2, 3, 8 / 3, 4, 9 / 4, 5, 10 / 5, 6, 11
    &[1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0],
    &[0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0],
    &[0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0],
    &[0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0],
    &[0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0],
    // Corner Right 1, 2, 8 / 2, 3, 9 / 3, 4, 10 / 4, 5, 11 / 5, 6, 12
    &[1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0],
    &[0, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0],
    &[0, 0, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0],
    &[0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1, 0],
    &[0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1],
    // Triangle 2, 7, 9 / 3, 8, 10 / 4, 9, 11 / 5, 10, 12
    &[0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0],
    &[0, 0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0],
    &[0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 1, 0],
    &[0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 1],
];

#[rustfmt::skip]
const MINI_TOUCH_PATTERNS: [&[i32]; 16] = [
    // Linear 1 - 3, 2 - 4, 5 - 7, 6 - 8
    &[1, 1, 1, 0, 0, 0, 0, 0],
    &[0, 1, 1, 1, 0, 0, 0, 0],
    &[0, 0, 0, 0, 1, 1, 1, 0],
    &[0, 0, 0, 0, 0, 1, 1, 1],
    // Linear Left 1, 2, 4 / 5, 6, 8
    &[1, 1, 0, 1, 0, 0, 0, 0],
    &[0, 0, 0, 0, 1, 1, 0, 1],
    // Linear Right 1, 3, 4 / 5, 7, 8
    &[1, 0, 1, 1, 0, 0, 0, 0],
    &[0, 0, 0, 0, 1, 0, 1, 1],
    // Corner Left 1, 2, 5 / 2, 3, 6 / 3, 4, 7
    &[1, 1, 0, 0, 1, 0, 0, 0],
    &[0, 1, 1, 0, 0, 1, 0, 0],
    &[0, 0, 1, 1, 0, 0, 1, 0],
    // Corner Right 1, 2, 6 / 2, 3, 7 / 3, 4, 8
    &[1, 1, 0, 0, 0, 1, 0, 0],
    &[0, 1, 1, 0, 0, 0, 1, 0],
    &[0, 0, 1, 1, 0, 0, 0, 1],
    // Triangle 2, 5, 7 / 3, 6, 8
    &[0, 1, 0, 0, 1, 0, 1, 0],
    &[0, 0, 1, 0, 0, 1, 0, 1],
];

// ---------------------------------------------------------------------------------------------
// SysEx (on the CTRL port)

/// The header of all messages (former `SYSEX_HDR_BYTE`): F0, manufacturer ID 00 21 45.
pub const SYSEX_HEADER: [u8; 4] = [0xF0, 0x00, 0x21, 0x45];

pub const SYSEX_INFO_DEVICE: [u8; 2] = [0x02, 0x7F];
pub const SYSEX_INFO_PRESET_LIST: [u8; 2] = [0x02, 0x04];
pub const SYSEX_RUNTIME_EXECUTE_LUA: [u8; 2] = [0x08, 0x0D];
pub const SYSEX_RUNTIME_SWITCH_PRESET: [u8; 2] = [0x09, 0x08];
pub const SYSEX_RUNTIME_SWITCH_PAGE: [u8; 2] = [0x09, 0x0A];
pub const SYSEX_RUNTIME_CONTROL_UPDATE: [u8; 2] = [0x14, 0x07];
pub const SYSEX_RUNTIME_VALUE_LABEL_UPDATE: [u8; 2] = [0x14, 0x0E];
pub const SYSEX_RUNTIME_SUBSCRIBE_EVENTS: [u8; 2] = [0x14, 0x79];
pub const SYSEX_RUNTIME_SET_REPAINT_ENABLED: [u8; 2] = [0x7F, 0x7A];
pub const SYSEX_RUNTIME_ENABLE_LOGGER: [u8; 2] = [0x7F, 0x7D];

/// The controller events to subscribe to: pages, pots (= knob touch) and buttons (former
/// `SUBSCRIBED_EVENTS`; 0x10 and 0x20 are reserved in the firmware).
pub const SUBSCRIBED_EVENTS: u8 = 0x01 | 0x08 | 0x10 | 0x20;

// Command categories
pub const CMD_INFO: u8 = 0x01;
pub const CMD_CONTROLLER: u8 = 0x7E;
pub const CMD_SYSTEM_CALL: u8 = 0x7F;

// The IDs of the controller events
pub const EVENT_PRESET_SWITCH: u8 = 0x02;
pub const EVENT_PAGE_SWITCH: u8 = 0x06;
pub const EVENT_POT_TOUCH: u8 = 0x0A;

// The IDs of the system calls
pub const SYSTEM_CALL_LOGGING: u8 = 0x00;

// The IDs of the information commands
pub const INFO_PRESET_LIST: u8 = 0x04;
pub const INFO_DEVICE: u8 = 0x7F;

/// The position of the command byte (former `CMD_START_POS`).
pub const CMD_START_POS: usize = SYSEX_HEADER.len();
/// The position of the sub-command byte (former `SUB_CMD_START_POS`).
pub const SUB_CMD_START_POS: usize = SYSEX_HEADER.len() + 1;

/// A SysEx message: the header, the command, the content and F7 (former `sendSysex`).
pub fn sysex(command: &[u8], content: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(SYSEX_HEADER.len() + command.len() + content.len() + 1);
    message.extend_from_slice(&SYSEX_HEADER);
    message.extend_from_slice(command);
    message.extend_from_slice(content);
    message.push(0xF7);
    message
}

/// A command with a control ID (14 bit, LSB first; former `createCommand`).
pub fn control_command(control_id: i32, command: [u8; 2]) -> [u8; 4] {
    [command[0], command[1], (control_id & 0x7F) as u8, ((control_id >> 7) & 0x7F) as u8]
}

/// The Lua call which sets the label of a group (former `SET_GROUP_TITLE`, `sgt` is a function of
/// the DrivenByMoss preset). The label is not escaped, as in Java.
pub fn set_group_title(group_id: i32, label: &str) -> String {
    format!("sgt({group_id},\"{label}\")")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering() {
        assert_eq!(ctrl_cc(0, 0), 10);
        assert_eq!(ctrl_cc(5, 5), 65);
        assert_eq!(button_id(0, 0), ButtonId::Row11);
        assert_eq!(button_id(5, 5), ButtonId::Row66);
        assert_eq!(continuous_id(2, 3), ContinuousId::Fader4);
        assert_eq!(continuous_id(5, 5), ContinuousId::DeviceKnob6);
        let knobs = ElectraVariant::Normal.knob_ids();
        assert_eq!(knobs.len(), 36);
        assert_eq!(knobs[6], ContinuousId::PanKnob1);
        assert_eq!(ElectraVariant::Mini.knob_ids()[7], ContinuousId::Knob8);
        assert_eq!(ElectraVariant::Normal.touch_patterns().len(), 38);
        assert!(ElectraVariant::Normal.touch_patterns().iter().all(|p| p.len() == 12 && p.iter().sum::<i32>() == 3));
        assert_eq!(ElectraVariant::Mini.touch_patterns().len(), 16);
        assert!(ElectraVariant::Mini.touch_patterns().iter().all(|p| p.len() == 8 && p.iter().sum::<i32>() == 3));
    }

    #[test]
    fn sysex_builders() {
        assert_eq!(sysex(&SYSEX_INFO_DEVICE, &[]), [0xF0, 0x00, 0x21, 0x45, 0x02, 0x7F, 0xF7]);
        assert_eq!(sysex(&SYSEX_RUNTIME_SUBSCRIBE_EVENTS, &[SUBSCRIBED_EVENTS]), [0xF0, 0x00, 0x21, 0x45, 0x14, 0x79, 0x39, 0xF7]);
        assert_eq!(control_command(252, SYSEX_RUNTIME_CONTROL_UPDATE), [0x14, 0x07, 0x7C, 0x01]);
        assert_eq!(set_group_title(501, "1: Track"), "sgt(501,\"1: Track\")");
    }
}
