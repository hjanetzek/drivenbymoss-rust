// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Ableton Push 1, Push 2 and Push 3 (former `controller.ableton.push` and
//! `Push1/2/3ControllerInstance`).
//!
//! - Push 1: the 4x68 character text display is sent as sysex ([`display1`]).
//! - Push 2 and 3: the 960x160 display is drawn with [`crate::framework::graphics`] and each
//!   frame (header + BGR565 pixels, XOR-masked, [`usb_display`]) is handed to the host with
//!   [`crate::backend::Backend::usb_bulk_out`], which writes it to the USB bulk endpoint of the
//!   display (Bitwig: its USB API; Linux hosts without one: usbdevfs; the simulator: a UNIX
//!   socket). See `doc/HOST_BRIDGE_GUIDE.md` for the host side.
//! - Push 2 and 3 with the Push simulator: the encoder state as a side channel
//!   ([`sim_values`], [`crate::backend::Backend::simulator_data_out`]).

pub mod colors;
pub mod command;
pub mod display1;
pub mod help;
pub mod mode;
pub mod palette;
pub mod parameterprovider;
pub mod setup;
pub mod sim_values;
pub mod surface;
pub mod usb_display;
pub mod view;

use crate::backend::Sender;
use crate::config::push::PushVersion;
use crate::config::Configuration;
use crate::controller::{ControllerDefinitionInfo, ControllerFactory, InstanceParts, MidiPorts, OperatingSystem, UsbMatcher};
use crate::host::Host;

/// The UUID of the Push 1 (former `Push1ControllerDefinition.EXTENSION_ID`).
pub const UUID_PUSH1: &str = "DBED9610-C474-11E6-9598-0800200C9A66";
/// The UUID of the Push 2.
pub const UUID_PUSH2: &str = "15176AA0-C476-11E6-9598-0800200C9A66";
/// The UUID of the Push 3.
pub const UUID_PUSH3: &str = "F8D561F2-722D-4DC7-975A-C562788C63FA";

/// The USB vendor ID of Ableton.
pub const VENDOR_ID: u16 = 0x2982;
/// The USB product ID of the Push 2.
pub const PUSH2_PRODUCT_ID: u16 = 0x1967;
/// The USB product ID of the Push 3.
pub const PUSH3_PRODUCT_ID: u16 = 0x1969;

/// The factory of a Push model (former `Push1/2/3ControllerDefinition`).
pub struct PushFactory {
    pub version: PushVersion,
}

fn pair(input: &str, output: &str) -> (Vec<String>, Vec<String>) {
    (vec![input.to_string()], vec![output.to_string()])
}

/// Former `createWindowsDeviceDiscoveryPairs`: the pattern contains `%s` which is replaced by
/// "" and "1- " to "19- ".
fn windows_pairs(input_pattern: &str, output_pattern: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let mut results = vec![pair(&input_pattern.replace("%s", ""), &output_pattern.replace("%s", ""))];
    for i in 1..20 {
        let prefix = format!("{i}- ");
        results.push(pair(&input_pattern.replace("%s", &prefix), &output_pattern.replace("%s", &prefix)));
    }
    results
}

/// Former `createDeviceDiscoveryPairs`.
fn device_pairs(device_name: &str) -> Vec<(Vec<String>, Vec<String>)> {
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

/// Former `createLinuxDeviceDiscoveryPairs`.
fn linux_pairs(input: &str, output: &str, port: i32) -> Vec<(Vec<String>, Vec<String>)> {
    let mut results = vec![pair(input, output)];
    for i in 1..20 {
        results.push(pair(&format!("{input} [hw:{i},0,{port}]"), &format!("{output} [hw:{i},0,{port}]")));
        results.push(pair(&format!("{input} MIDI {i}"), &format!("{output} MIDI {i}")));
    }
    results
}

/// The port names of a Push per operating system (former `getMidiDiscoveryPairs`).
pub fn midi_discovery_pairs(version: PushVersion, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
    let mut results = Vec::new();
    match (version, os) {
        (PushVersion::Version1, OperatingSystem::Windows) => results.extend(windows_pairs("MIDIIN2 (%sAbleton Push)", "MIDIOUT2 (%sAbleton Push)")),
        (PushVersion::Version1, _) => {
            results.push(pair("Ableton Push User Port", "Ableton Push User Port"));
            results.extend(linux_pairs("Push", "Push", 1));
        }
        (PushVersion::Version2, OperatingSystem::Windows) => results.extend(device_pairs("Ableton Push 2")),
        (PushVersion::Version2, OperatingSystem::Mac) => {
            results.push(pair("Ableton Push 2 Live Port", "Ableton Push 2 Live Port"));
            results.push(pair("Ableton AG - Ableton Push 2 - Live Port", "Ableton AG - Ableton Push 2 - Live Port"));
        }
        (PushVersion::Version2, OperatingSystem::Linux) => {
            results.extend(linux_pairs("A2", "A2", 0));
            // The ALSA sequencer names of the Live port (current kernels, Python/Zynthian hosts
            // and the simulator; older kernels: "MIDI 1")
            results.push(pair("Ableton Push 2 Live Port", "Ableton Push 2 Live Port"));
            results.push(pair("Ableton Push 2 MIDI 1", "Ableton Push 2 MIDI 1"));
        }
        (PushVersion::Version3, OperatingSystem::Windows) => results.extend(device_pairs("Ableton Push 3 MIDI")),
        (PushVersion::Version3, OperatingSystem::Mac) => {
            results.push(pair("Ableton Push 3 Live Port", "Ableton Push 3 Live Port"));
            results.push(pair("Ableton AG - Ableton Push 3 - Live Port", "Ableton AG - Ableton Push 3 - Live Port"));
        }
        (PushVersion::Version3, OperatingSystem::Linux) => {
            results.push(pair("Ableton Push 3 Live Port", "Ableton Push 3 Live Port"));
            // The ALSA sequencer port of the Live port (Python/Zynthian hosts and the simulator)
            results.push(pair("Ableton Push 3 MIDI 1", "Ableton Push 3 MIDI 1"));
        }
    }
    results
}

impl ControllerFactory for PushFactory {
    fn info(&self) -> ControllerDefinitionInfo {
        let (uuid, model) = match self.version {
            PushVersion::Version1 => (UUID_PUSH1, "Push 1"),
            PushVersion::Version2 => (UUID_PUSH2, "Push 2"),
            PushVersion::Version3 => (UUID_PUSH3, "Push 3"),
        };
        ControllerDefinitionInfo { uuid: uuid.to_string(), vendor: "Ableton".to_string(), hardware_model: model.to_string(), num_midi_in_ports: 1, num_midi_out_ports: 1 }
    }

    fn java_class_name(&self) -> &'static str {
        match self.version {
            PushVersion::Version1 => "de.mossgrabers.reaper.controller.ableton.push.Push1ControllerInstance",
            PushVersion::Version2 => "de.mossgrabers.reaper.controller.ableton.push.Push2ControllerInstance",
            PushVersion::Version3 => "de.mossgrabers.reaper.controller.ableton.push.Push3ControllerInstance",
        }
    }

    fn midi_discovery_pairs(&self, os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        midi_discovery_pairs(self.version, os)
    }

    fn create_configuration(&self) -> Configuration {
        crate::config::push::for_reaper(self.version)
    }

    fn create(&self, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
        setup::create(self.version, host, sender, ports, configuration)
    }

    /// Former `claimUSBDevice`: the display of the Push 2 and 3.
    fn usb_devices(&self) -> Vec<UsbMatcher> {
        let product_id = match self.version {
            PushVersion::Version1 => return Vec::new(),
            PushVersion::Version2 => PUSH2_PRODUCT_ID,
            PushVersion::Version3 => PUSH3_PRODUCT_ID,
        };
        vec![UsbMatcher { vendor_id: VENDOR_ID, product_id, interface: 0, endpoint: 0x01 }]
    }
}

/// The factories of the Push models.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    vec![
        Box::new(PushFactory { version: PushVersion::Version1 }),
        Box::new(PushFactory { version: PushVersion::Version2 }),
        Box::new(PushFactory { version: PushVersion::Version3 }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions() {
        let factories = factories();
        assert_eq!(factories[1].info().to_string(), "Ableton Push 2");
        assert!(factories[0].usb_devices().is_empty());
        assert_eq!(factories[2].usb_devices()[0].product_id, 0x1969);
        assert_eq!(midi_discovery_pairs(PushVersion::Version1, OperatingSystem::Windows)[1].0[0], "MIDIIN2 (1- Ableton Push)");
        assert_eq!(midi_discovery_pairs(PushVersion::Version2, OperatingSystem::Linux)[0].0[0], "A2");
        assert!(midi_discovery_pairs(PushVersion::Version2, OperatingSystem::Windows).iter().any(|p| p.0[0] == "Ableton Push 2 MIDI 3"));
    }
}
