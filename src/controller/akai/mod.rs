// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Akai controllers (former `controller.akai`): APC mini Mk1/Mk2.

pub mod apcmini;

use crate::controller::ControllerFactory;

/// The factories of all Akai controllers.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    apcmini::factories()
}

/// The port names for the automatic detection of a device with the same input and output name
/// (former `DefaultControllerDefinition.createDeviceDiscoveryPairs`; the Java version uses the
/// same names on all operating systems).
pub fn device_discovery_pairs(device_name: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let pair = |name: String| (vec![name.clone()], vec![name]);
    // Former createWindowsDeviceDiscoveryPairs ("%s" + deviceName, "%s" + deviceName)
    let mut results = vec![pair(device_name.to_string())];
    for i in 1..20 {
        results.push(pair(format!("{i}- {device_name}")));
    }
    for i in 1..20 {
        results.push(pair(format!("{device_name} MIDI {i}")));
        results.push(pair(format!("{device_name} {i} MIDI 1")));
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_pairs() {
        let pairs = device_discovery_pairs("APC MINI");
        assert_eq!(pairs.len(), 20 + 38);
        assert_eq!(pairs[0], (vec!["APC MINI".to_string()], vec!["APC MINI".to_string()]));
        assert_eq!(pairs[1].0[0], "1- APC MINI");
        assert_eq!(pairs[20].0[0], "APC MINI MIDI 1");
        assert_eq!(pairs[21].1[0], "APC MINI 1 MIDI 1");
    }
}
