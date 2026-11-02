// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Mackie protocols MCU and HUI with extenders (former packages
//! `controller.mackie.mcu` and `controller.mackie.hui`). An instance has one surface per device
//! (main device and extenders), each with its own MIDI port pair, see
//! `doc/MULTI_SURFACE_PLAN.md`.

pub mod hui;
pub mod mcu;

use crate::config::values::fix_ascii;
use crate::controller::ControllerFactory;
use crate::daw::util::optimize_name;

/// The factories of all supported Mackie models: MCU with 0-3 extenders and HUI with 0-2
/// extenders (as the Reaper instance classes).
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    let mut factories: Vec<Box<dyn ControllerFactory>> = Vec::new();
    for extenders in 0..4 {
        factories.push(Box::new(mcu::McuFactory::new(extenders)));
    }
    for extenders in 0..3 {
        factories.push(Box::new(hui::HuiFactory::new(extenders)));
    }
    factories
}

/// First replaces umlauts with alternative writing, then shortens a text to the given length
/// (former `StringUtils.shortenAndFixASCII`).
pub fn shorten_and_fix_ascii(text: &str, length: usize) -> String {
    optimize_name(&fix_ascii(text), length)
}

/// Creates the 58 variations of a device name for the automatic detection (former
/// `createDeviceDiscoveryPairs`: the 20 Windows variations "<n>- name" and "name MIDI <n>",
/// "name <n> MIDI 1" for 1..19, for all operating systems).
pub fn create_device_discovery_pairs(name: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let pair = |n: String| (vec![n.clone()], vec![n]);
    let mut results = vec![pair(name.to_string())];
    for i in 1..20 {
        results.push(pair(format!("{i}- {name}")));
    }
    for i in 1..20 {
        results.push(pair(format!("{name} MIDI {i}")));
        results.push(pair(format!("{name} {i} MIDI 1")));
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(shorten_and_fix_ascii("Grüne Gitarre", 7), "GrnGtrr");
        let pairs = create_device_discovery_pairs("X");
        assert_eq!(pairs.len(), 58);
        assert_eq!(pairs[19].0[0], "19- X");
        assert_eq!(pairs[57].1[0], "X 19 MIDI 1");
    }

    #[test]
    fn registered() {
        let factories = factories();
        assert_eq!(factories.len(), 7);
        assert_eq!(factories[3].java_class_name(), "de.mossgrabers.reaper.controller.mackie.mcu.MCU4ControllerInstance");
        assert_eq!(factories[3].num_port_pairs(), 4);
        assert_eq!(factories[6].java_class_name(), "de.mossgrabers.reaper.controller.mackie.hui.HUI3ControllerInstance");
        assert_eq!(factories[6].num_port_pairs(), 3);
    }
}
