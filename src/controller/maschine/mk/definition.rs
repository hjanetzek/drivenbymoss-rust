// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definitions of the Maschine models which use the shared Maschine setup (former
//! `MaschineMk2ControllerDefinition`, `MaschineMk3ControllerDefinition`,
//! `MaschineMikroMk3ControllerDefinition`, `MaschinePlusControllerDefinition` and
//! `MaschineStudioControllerDefinition` and the Reaper `*ControllerInstance` classes).

use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The definition of a Maschine model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaschineDefinition {
    pub maschine: Maschine,
}

type Pairs = Vec<(Vec<String>, Vec<String>)>;

fn pair(input: &str, output: &str) -> (Vec<String>, Vec<String>) {
    (vec![input.to_string()], vec![output.to_string()])
}

/// Former `createWindowsDeviceDiscoveryPairs`: the patterns must contain a "%s".
fn windows_pairs(input_pattern: &str, output_pattern: &str) -> Pairs {
    let mut results = vec![pair(&input_pattern.replace("%s", ""), &output_pattern.replace("%s", ""))];
    for i in 1..20 {
        let prefix = format!("{i}- ");
        results.push(pair(&input_pattern.replace("%s", &prefix), &output_pattern.replace("%s", &prefix)));
    }
    results
}

/// Former `createDeviceDiscoveryPairs`.
fn device_pairs(device_name: &str) -> Pairs {
    let pattern = format!("%s{device_name}");
    let mut results = windows_pairs(&pattern, &pattern);
    for i in 1..20 {
        let name = format!("{device_name} MIDI {i}");
        results.push(pair(&name, &name));
        let name = format!("{device_name} {i} MIDI 1");
        results.push(pair(&name, &name));
    }
    results
}

impl MaschineDefinition {
    pub fn new(maschine: Maschine) -> Self {
        assert!(maschine != Maschine::Jam, "The Maschine JAM has its own definition.");
        MaschineDefinition { maschine }
    }

    pub fn info(&self) -> ControllerDefinitionInfo {
        let (uuid, model) = match self.maschine {
            Maschine::Mk2 => ("015E4908-C049-4808-A645-A7A44BEFE36B", "Maschine Mk2"),
            Maschine::Mk3 => ("9055C36B-0A41-48AD-8675-4D3F133E53AC", "Maschine Mk3"),
            Maschine::MikroMk3 => ("E87516A3-5401-44B1-9952-B4F821ED3DD5", "Maschine Mikro Mk3"),
            Maschine::Plus => ("BF0FD105-08A3-4A1D-9F19-A8F5A9C4E7DF", "Maschine Plus"),
            Maschine::Studio | Maschine::Jam => ("9E2D8B7C-D032-11EB-B8BC-0242AC130003", "Maschine Studio"),
        };
        ControllerDefinitionInfo {
            uuid: uuid.to_string(),
            vendor: "Native Instruments".to_string(),
            hardware_model: model.to_string(),
            num_midi_in_ports: 1,
            num_midi_out_ports: 1,
        }
    }

    /// The fully qualified name of the Java instance class.
    pub fn java_class_name(&self) -> &'static str {
        match self.maschine {
            Maschine::Mk2 => "de.mossgrabers.reaper.controller.ni.maschine.mk3.MaschineMk2ControllerInstance",
            Maschine::Mk3 => "de.mossgrabers.reaper.controller.ni.maschine.mk3.MaschineMk3ControllerInstance",
            Maschine::MikroMk3 => "de.mossgrabers.reaper.controller.ni.maschine.mk3.MaschineMikroMk3ControllerInstance",
            Maschine::Plus => "de.mossgrabers.reaper.controller.ni.maschine.mk3.MaschinePlusControllerInstance",
            Maschine::Studio | Maschine::Jam => "de.mossgrabers.reaper.controller.ni.maschine.mk3.MaschineStudioControllerInstance",
        }
    }

    pub fn midi_discovery_pairs(&self, os: OperatingSystem) -> Pairs {
        let is_mac = os == OperatingSystem::Mac;
        match self.maschine {
            Maschine::Mk2 => {
                if is_mac {
                    vec![pair("Maschine MK2 Virtual Input", "Maschine MK2 Virtual Output")]
                } else {
                    vec![pair("Maschine MK2 In", "Maschine MK2 Out")]
                }
            }
            Maschine::Mk3 => {
                if is_mac {
                    vec![pair("Maschine MK3 Virtual Input", "Maschine MK3 Virtual Output")]
                } else {
                    device_pairs("Maschine MK3 Ctrl MIDI")
                }
            }
            Maschine::MikroMk3 => device_pairs("Maschine Mikro MK3"),
            Maschine::Plus => vec![pair("Maschine Plus Virtual", "Maschine Plus Virtual")],
            Maschine::Studio | Maschine::Jam => {
                if is_mac {
                    vec![pair("Maschine Studio Virtual Input", "Maschine Studio Virtual Output")]
                } else {
                    windows_pairs("%sMaschine Studio In", "%sMaschine Studio Out 1")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery() {
        let mk2 = MaschineDefinition::new(Maschine::Mk2);
        assert_eq!(mk2.info().hardware_model, "Maschine Mk2");
        assert_eq!(mk2.midi_discovery_pairs(OperatingSystem::Windows), vec![pair("Maschine MK2 In", "Maschine MK2 Out")]);
        assert_eq!(mk2.midi_discovery_pairs(OperatingSystem::Mac), vec![pair("Maschine MK2 Virtual Input", "Maschine MK2 Virtual Output")]);
        let mk3 = MaschineDefinition::new(Maschine::Mk3);
        let pairs = mk3.midi_discovery_pairs(OperatingSystem::Linux);
        assert_eq!(pairs.len(), 20 + 38);
        assert_eq!(pairs[1], pair("1- Maschine MK3 Ctrl MIDI", "1- Maschine MK3 Ctrl MIDI"));
        let studio = MaschineDefinition::new(Maschine::Studio).midi_discovery_pairs(OperatingSystem::Windows);
        assert_eq!(studio[2], pair("2- Maschine Studio In", "2- Maschine Studio Out 1"));
    }
}
