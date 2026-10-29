// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definitions of the supported Launchpad models (former `ILaunchpadControllerDefinition`,
//! `AbstractLaunchpadDefinition`, the model specific definitions and `ButtonSetup`).

use std::collections::{BTreeMap, HashMap};

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};
use crate::framework::grid::LightInfo;

/// Control number of the logo LED.
pub const LAUNCHPAD_LOGO: i32 = 99;

/// The buttons of a Launchpad (not all models have all of them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LaunchpadButton {
    Shift,
    User,
    Project,
    Click,
    Undo,
    Delete,
    Quantize,
    Duplicate,
    Play,
    Record,
    RecArm,
    TrackSelect,
    Mute,
    Solo,
    Volume,
    Pan,
    Sends,
    StopClip,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Session,
    Note,
    Device,
    Scene1,
    Scene2,
    Scene3,
    Scene4,
    Scene5,
    Scene6,
    Scene7,
    Scene8,
}

impl LaunchpadButton {
    pub const SCENES: [LaunchpadButton; 8] = [
        LaunchpadButton::Scene1,
        LaunchpadButton::Scene2,
        LaunchpadButton::Scene3,
        LaunchpadButton::Scene4,
        LaunchpadButton::Scene5,
        LaunchpadButton::Scene6,
        LaunchpadButton::Scene7,
        LaunchpadButton::Scene8,
    ];
}

/// The MIDI control of a button and if it is only available in combination with Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchpadButtonInfo {
    pub control: i32,
    pub is_shifted: bool,
}

impl LaunchpadButtonInfo {
    /// A button which does not exist on the hardware.
    pub const VIRTUAL: LaunchpadButtonInfo = LaunchpadButtonInfo { control: -1, is_shifted: false };

    pub fn is_virtual(&self) -> bool {
        self.control < 0
    }
}

/// The supported Launchpad models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchpadModel {
    MkII,
    Pro,
    ProMk3,
    X,
    MiniMkIII,
}

impl LaunchpadModel {
    pub const ALL: [LaunchpadModel; 5] = [LaunchpadModel::MkII, LaunchpadModel::Pro, LaunchpadModel::ProMk3, LaunchpadModel::X, LaunchpadModel::MiniMkIII];
}

/// The scene button controls (top to bottom).
pub const SCENE_CONTROLS: [i32; 8] = [89, 79, 69, 59, 49, 39, 29, 19];

/// The definition of a Launchpad model.
#[derive(Debug, Clone)]
pub struct LaunchpadDefinition {
    pub model: LaunchpadModel,
    buttons: HashMap<LaunchpadButton, LaunchpadButtonInfo>,
}

impl LaunchpadDefinition {
    pub fn new(model: LaunchpadModel) -> Self {
        let mut definition = LaunchpadDefinition { model, buttons: HashMap::new() };

        use LaunchpadButton as B;
        // Defaults of all models (AbstractLaunchpadDefinition)
        definition.set_button(B::ArrowUp, 91);
        definition.set_button(B::ArrowDown, 92);
        definition.set_button(B::ArrowLeft, 93);
        definition.set_button(B::ArrowRight, 94);
        definition.set_button(B::Session, 95);
        definition.set_button(B::Note, 96);
        definition.set_button(B::Device, 97);
        definition.set_button(B::Shift, 98);
        for (scene, control) in LaunchpadButton::SCENES.iter().zip(SCENE_CONTROLS) {
            definition.set_button(*scene, control);
        }

        match model {
            LaunchpadModel::MkII => {
                definition.set_button(B::Shift, 111);
                definition.set_button(B::ArrowUp, 104);
                definition.set_button(B::ArrowDown, 105);
                definition.set_button(B::ArrowLeft, 106);
                definition.set_button(B::ArrowRight, 107);
                definition.set_button(B::Session, 108);
                definition.set_button(B::Note, 109);
                definition.set_button(B::Device, 110);
            }
            LaunchpadModel::Pro => {
                definition.set_button(B::Shift, 80);
                definition.set_button(B::User, 98);
                definition.set_button(B::Click, 70);
                definition.set_button(B::Undo, 60);
                definition.set_button(B::Delete, 50);
                definition.set_button(B::Quantize, 40);
                definition.set_button(B::Duplicate, 30);
                definition.set_button(B::Play, 20);
                definition.set_button(B::Record, 10);
                definition.set_button(B::RecArm, 1);
                definition.set_button(B::TrackSelect, 2);
                definition.set_button(B::Mute, 3);
                definition.set_button(B::Solo, 4);
                definition.set_button(B::Volume, 5);
                definition.set_button(B::Pan, 6);
                definition.set_button(B::Sends, 7);
                definition.set_button(B::StopClip, 8);
            }
            LaunchpadModel::ProMk3 => {
                definition.set_button(B::Shift, 90);
                definition.set_button(B::User, 96);
                definition.set_button(B::Project, 98);
                definition.set_button(B::Session, 93);
                definition.set_button(B::Note, 94);
                definition.set_button(B::Device, 7);
                definition.set_button(B::ArrowUp, 80);
                definition.set_button(B::ArrowDown, 70);
                definition.set_button(B::ArrowLeft, 91);
                definition.set_button(B::ArrowRight, 92);
                definition.set_shifted_button(B::Click, 3);
                definition.set_shifted_button(B::Undo, 1);
                definition.set_button(B::Delete, 60);
                definition.set_button(B::Quantize, 40);
                definition.set_button(B::Duplicate, 50);
                definition.set_button(B::Play, 20);
                definition.set_button(B::Record, 10);
                definition.set_button(B::RecArm, 1);
                definition.set_button(B::Mute, 2);
                definition.set_button(B::Solo, 3);
                definition.set_button(B::Volume, 4);
                definition.set_button(B::Pan, 5);
                definition.set_button(B::Sends, 6);
                definition.set_button(B::StopClip, 8);
            }
            LaunchpadModel::X | LaunchpadModel::MiniMkIII => {}
        }
        definition
    }

    fn set_button(&mut self, button: LaunchpadButton, control: i32) {
        self.buttons.insert(button, LaunchpadButtonInfo { control, is_shifted: false });
    }

    fn set_shifted_button(&mut self, button: LaunchpadButton, control: i32) {
        self.buttons.insert(button, LaunchpadButtonInfo { control, is_shifted: true });
    }

    /// Get the MIDI control info of a button. Returns a virtual info if the model does not have
    /// the button.
    pub fn button(&self, button: LaunchpadButton) -> LaunchpadButtonInfo {
        self.buttons.get(&button).copied().unwrap_or(LaunchpadButtonInfo::VIRTUAL)
    }

    pub fn hardware_model(&self) -> &'static str {
        match self.model {
            LaunchpadModel::MkII => "Launchpad MkII",
            LaunchpadModel::Pro => "Launchpad Pro",
            LaunchpadModel::ProMk3 => "Launchpad Pro Mk3",
            LaunchpadModel::X => "Launchpad X",
            LaunchpadModel::MiniMkIII => "Launchpad Mini MkIII",
        }
    }

    pub fn uuid(&self) -> &'static str {
        match self.model {
            LaunchpadModel::MkII => "4E01A0B0-67B1-11E5-A837-0800200C9A66",
            LaunchpadModel::Pro => "80B63970-64F1-11E5-A837-0800200C9A66",
            LaunchpadModel::ProMk3 => "4EDED44C-7817-4C66-A334-66A9E342AAA0",
            LaunchpadModel::X => "CD196CCF-DF98-4AB0-9ABC-F0F29A60ACED",
            LaunchpadModel::MiniMkIII => "A17B269D-2641-452F-B5A2-81BBACDA0D17",
        }
    }

    pub fn is_pro(&self) -> bool {
        matches!(self.model, LaunchpadModel::Pro | LaunchpadModel::ProMk3)
    }

    pub fn has_track_selection_buttons(&self) -> bool {
        self.model == LaunchpadModel::ProMk3
    }

    pub fn sysex_header(&self) -> &'static str {
        match self.model {
            LaunchpadModel::MkII => "F0 00 20 29 02 18 ",
            LaunchpadModel::Pro => "F0 00 20 29 02 10 ",
            LaunchpadModel::ProMk3 => "F0 00 20 29 02 0E ",
            LaunchpadModel::X => "F0 00 20 29 02 0C ",
            LaunchpadModel::MiniMkIII => "F0 00 20 29 02 0D ",
        }
    }

    pub fn standalone_mode_command(&self) -> &'static str {
        match self.model {
            LaunchpadModel::MkII | LaunchpadModel::Pro => "21 01",
            LaunchpadModel::ProMk3 | LaunchpadModel::X | LaunchpadModel::MiniMkIII => "10 00",
        }
    }

    pub fn program_mode_command(&self) -> &'static str {
        match self.model {
            LaunchpadModel::MkII => "22 00",
            LaunchpadModel::Pro => "2C 03",
            LaunchpadModel::ProMk3 | LaunchpadModel::X | LaunchpadModel::MiniMkIII => "0E 01",
        }
    }

    /// The command (without header) to reset the Launchpad to its default mode, if any.
    pub fn reset_mode_command(&self) -> Option<&'static str> {
        match self.model {
            LaunchpadModel::MkII => None,
            LaunchpadModel::Pro => Some("2C 00"),
            LaunchpadModel::ProMk3 | LaunchpadModel::X | LaunchpadModel::MiniMkIII => Some("0E 00"),
        }
    }

    pub fn scene_buttons_use_cc(&self) -> bool {
        self.model != LaunchpadModel::MkII
    }

    /// How the logo LED is set.
    pub fn logo_command(&self, color: i32) -> LogoCommand {
        match self.model {
            LaunchpadModel::MkII => LogoCommand::None,
            LaunchpadModel::Pro => LogoCommand::Sysex(format!("0A 63 {color:02X}")),
            LaunchpadModel::ProMk3 => LogoCommand::Sysex(format!("03 00 63 {color:02X}")),
            LaunchpadModel::X | LaunchpadModel::MiniMkIII => LogoCommand::Trigger(LAUNCHPAD_LOGO, color),
        }
    }

    /// The sysex message format string for the pad brightness (value as 2 hex digits), if
    /// supported.
    pub fn brightness_sysex(&self, brightness: i32) -> Option<String> {
        match self.model {
            LaunchpadModel::X => Some(format!("F0 00 20 29 02 0C 08 {brightness:02X} F7")),
            LaunchpadModel::MiniMkIII => Some(format!("F0 00 20 29 02 0D 08 {brightness:02X} F7")),
            _ => None,
        }
    }

    pub fn supports_brightness(&self) -> bool {
        self.brightness_sysex(0).is_some()
    }

    /// Build the sysex messages to update the given pads (controller note -> light info).
    pub fn build_led_update(&self, pad_infos: &BTreeMap<i32, LightInfo>) -> Vec<String> {
        match self.model {
            LaunchpadModel::MkII | LaunchpadModel::Pro => {
                // Note: The MkII has an additional prefixed 00 compared to the Pro!
                let prefix = if self.model == LaunchpadModel::MkII { "00 " } else { "" };
                let mut normal = String::new();
                let mut flash = String::new();
                let mut pulse = String::new();
                for (note, info) in pad_infos {
                    normal.push_str(&format!("{note:02X} {:02X} ", info.color));
                    if info.blink_color > 0 {
                        let entry = format!("{prefix}{note:02X} {:02X} ", info.blink_color);
                        if info.fast {
                            flash.push_str(&entry);
                        } else {
                            pulse.push_str(&entry);
                        }
                    }
                }
                let header = self.sysex_header();
                let mut result = Vec::new();
                if !normal.is_empty() {
                    result.push(format!("{header}0A {normal}F7"));
                }
                if !flash.is_empty() {
                    result.push(format!("{header}23 {flash}F7"));
                }
                if !pulse.is_empty() {
                    result.push(format!("{header}28 {pulse}F7"));
                }
                result
            }
            LaunchpadModel::ProMk3 | LaunchpadModel::X | LaunchpadModel::MiniMkIII => {
                let mut sb = format!("{}03 ", self.sysex_header());
                for (note, info) in pad_infos {
                    if info.blink_color <= 0 {
                        // 00h: Static color from palette
                        sb.push_str(&format!("00 {note:02X} {:02X} ", info.color));
                    } else if info.fast {
                        // 01h: Flashing color, Color B and Color A
                        sb.push_str(&format!("01 {note:02X} {:02X} {:02X} ", info.blink_color, info.color));
                    } else {
                        // 02h: Pulsing color
                        sb.push_str(&format!("02 {note:02X} {:02X} ", info.color));
                    }
                }
                sb.push_str("F7");
                vec![sb]
            }
        }
    }

    /// The MIDI port names for the automatic detection of the device (input names, output
    /// names).
    pub fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        let pair = |input: &str, output: &str| (vec![input.to_string()], vec![output.to_string()]);
        let windows = |input: &str, output: &str| {
            let mut results = vec![pair(&input.replace("%s", ""), &output.replace("%s", ""))];
            for i in 1..20 {
                let prefix = format!("{i}- ");
                results.push(pair(&input.replace("%s", &prefix), &output.replace("%s", &prefix)));
            }
            results
        };
        match (self.model, os) {
            (LaunchpadModel::MkII, _) => {
                let mut results = windows("%sLaunchpad MK2", "%sLaunchpad MK2");
                for i in 1..20 {
                    results.push(pair(&format!("Launchpad MK2 MIDI {i}"), &format!("Launchpad MK2 MIDI {i}")));
                    results.push(pair(&format!("Launchpad MK2 {i} MIDI 1"), &format!("Launchpad MK2 {i} MIDI 1")));
                }
                results
            }
            (LaunchpadModel::Pro, OperatingSystem::Windows) => windows("MIDIIN2 (%sLaunchpad Pro)", "MIDIOUT2 (%sLaunchpad Pro)"),
            (LaunchpadModel::Pro, OperatingSystem::Linux) => vec![
                pair("Launchpad Pro MIDI 2", "Launchpad Pro MIDI 2"),
                pair("Launchpad Pro Standalone Port", "Launchpad Pro Standalone Port"),
            ],
            (LaunchpadModel::Pro, OperatingSystem::Mac) => vec![pair("Launchpad Pro Standalone Port", "Launchpad Pro Standalone Port")],
            (LaunchpadModel::ProMk3, OperatingSystem::Windows) => windows("%sLPProMK3 MIDI", "%sLPProMK3 MIDI"),
            (LaunchpadModel::ProMk3, OperatingSystem::Linux) => vec![
                pair("Launchpad Pro MK3 MIDI 1", "Launchpad Pro MK3 MIDI 1"),
                pair("Launchpad Pro MK3 LPProMK3 MIDI", "Launchpad Pro MK3 LPProMK3 MIDI"),
            ],
            (LaunchpadModel::ProMk3, OperatingSystem::Mac) => vec![pair("Launchpad Pro MK3 LPProMK3 MIDI", "Launchpad Pro MK3 LPProMK3 MIDI")],
            (LaunchpadModel::X, OperatingSystem::Windows) => windows("MIDIIN2 (%sLPX MIDI)", "MIDIOUT2 (%sLPX MIDI)"),
            (LaunchpadModel::X, OperatingSystem::Linux) => {
                vec![pair("Launchpad X MIDI 2", "Launchpad X MIDI 2"), pair("Launchpad X LPX MIDI Out", "Launchpad X LPX MIDI In")]
            }
            (LaunchpadModel::X, OperatingSystem::Mac) => vec![pair("Launchpad X LPX MIDI Out", "Launchpad X LPX MIDI In")],
            (LaunchpadModel::MiniMkIII, OperatingSystem::Windows) => windows("MIDIIN2 (%sLPMiniMK3 MIDI)", "MIDIOUT2 (%sLPMiniMK3 MIDI)"),
            (LaunchpadModel::MiniMkIII, OperatingSystem::Linux) => vec![
                pair("Launchpad Mini MK3 MIDI 2", "Launchpad Mini MK3 MIDI 2"),
                pair("Launchpad Mini MK3 LPMiniMK3 MI", "Launchpad Mini MK3 LPMiniMK3 MI"),
            ],
            (LaunchpadModel::MiniMkIII, OperatingSystem::Mac) => {
                vec![pair("Launchpad Mini MK3 LPMiniMK3 MIDI Out", "Launchpad Mini MK3 LPMiniMK3 MIDI In")]
            }
        }
    }

    pub fn info(&self) -> ControllerDefinitionInfo {
        ControllerDefinitionInfo {
            uuid: self.uuid().to_string(),
            vendor: "Novation".to_string(),
            hardware_model: self.hardware_model().to_string(),
            num_midi_in_ports: 1,
            num_midi_out_ports: 1,
        }
    }
}

/// How the logo LED of a model is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogoCommand {
    None,
    /// Sysex data (without header and the closing F7).
    Sysex(String),
    /// Send as a trigger (CC) with the control and value.
    Trigger(i32, i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_updates() {
        let mut infos = BTreeMap::new();
        infos.insert(11, LightInfo { color: 5, blink_color: 0, fast: false });
        infos.insert(12, LightInfo { color: 5, blink_color: 7, fast: true });
        let x = LaunchpadDefinition::new(LaunchpadModel::X);
        assert_eq!(x.build_led_update(&infos), vec!["F0 00 20 29 02 0C 03 00 0B 05 01 0C 07 05 F7".to_string()]);
        let pro = LaunchpadDefinition::new(LaunchpadModel::Pro);
        assert_eq!(
            pro.build_led_update(&infos),
            vec!["F0 00 20 29 02 10 0A 0B 05 0C 05 F7".to_string(), "F0 00 20 29 02 10 23 0C 07 F7".to_string()]
        );
    }

    #[test]
    fn buttons() {
        let mk3 = LaunchpadDefinition::new(LaunchpadModel::ProMk3);
        assert_eq!(mk3.button(LaunchpadButton::Click), LaunchpadButtonInfo { control: 3, is_shifted: true });
        assert!(LaunchpadDefinition::new(LaunchpadModel::X).button(LaunchpadButton::Play).is_virtual());
        assert_eq!(LaunchpadDefinition::new(LaunchpadModel::X).button(LaunchpadButton::Scene8).control, 19);
    }
}
