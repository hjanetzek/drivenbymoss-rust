// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Definition of the NI Maschine JAM controller (former `MaschineJamControllerDefinition`).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

const MASCHINE_JAM_PREFIX: &str = "Maschine Jam - ";

/// General information about the controller.
pub fn info() -> ControllerDefinitionInfo {
    ControllerDefinitionInfo {
        uuid: "72F32A83-A697-446B-9016-64AA3F9476E4".to_string(),
        vendor: "Native Instruments".to_string(),
        hardware_model: "Maschine JAM".to_string(),
        num_midi_in_ports: 1,
        num_midi_out_ports: 1,
    }
}

/// The port names for the automatic detection of the device.
pub fn midi_discovery_pairs(os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    let pair = |input: String, output: String| (vec![input], vec![output]);
    let mut pairs = Vec::new();
    for i in 1..=4 {
        let name = format!("{MASCHINE_JAM_PREFIX}{i}");
        match os {
            OperatingSystem::Mac => pairs.push(pair(format!("{name} Input"), format!("{name} Output"))),
            _ => {
                if os == OperatingSystem::Linux {
                    // The ports of dbm-nihost (support/linux/ni_host), named like the ones of
                    // NI's driver on macOS: ALSA and through PipeWire's MIDI bridge, which removes
                    // the client name from the port name
                    pairs.push(pair(format!("{name} Input"), format!("{name} Output")));
                    pairs.push(pair(format!("Midi-Bridge:{name}: Input (capture)"), format!("Midi-Bridge:{name}: Output (playback)")));
                }
                // Former createDeviceDiscoveryPairs
                pairs.push(pair(name.clone(), name.clone()));
                for j in 1..20 {
                    let windows = format!("{j}- {name}");
                    pairs.push(pair(windows.clone(), windows));
                }
                for j in 1..20 {
                    let a = format!("{name} MIDI {j}");
                    pairs.push(pair(a.clone(), a));
                    let b = format!("{name} {j} MIDI 1");
                    pairs.push(pair(b.clone(), b));
                }
            }
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery() {
        let mac = midi_discovery_pairs(OperatingSystem::Mac);
        assert_eq!(mac.len(), 4);
        assert_eq!(mac[0], (vec!["Maschine Jam - 1 Input".to_string()], vec!["Maschine Jam - 1 Output".to_string()]));
        let win = midi_discovery_pairs(OperatingSystem::Windows);
        assert_eq!(win.len(), 4 * 58);
        assert_eq!(win[0].0[0], "Maschine Jam - 1");
        assert_eq!(win[1].0[0], "1- Maschine Jam - 1");
        assert_eq!(win[20].0[0], "Maschine Jam - 1 MIDI 1");
        assert_eq!(win[21].0[0], "Maschine Jam - 1 1 MIDI 1");
    }
}
