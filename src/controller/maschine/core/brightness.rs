// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The LED brightness setting of the Maschine controllers (not in the Java version).
//!
//! The RGB LEDs (pads, group and scene buttons, the JAM's touch strips) take an index into NI's
//! palette (`hue * 4 + level`, level 0 dim, 1 medium, 2 full, 3 pale, see
//! [`super::colors::NI_PALETTE`]); there is no separate brightness. The single-colour LEDs take
//! their brightness as the MIDI value (0..127). "Normal" sends the values of the core unchanged
//! (as Java); "Bright" raises them when they are sent ([`LedBrightness::rgb`],
//! [`LedBrightness::mono`]), so the setting covers all modes, views and the surface link. The
//! mapping is the two tables [`BRIGHT_LEVELS`] / [`BRIGHT_GREYS`] and [`bright_mono`].

use std::cell::Cell;
use std::rc::Rc;

use crate::config::{Configuration, SettingId};

/// The values of the setting, in the order of [`LedBrightness`].
pub const OPTIONS: [&str; 2] = ["Normal", "Bright"];

/// The label of the setting (also the key in the `.config` file).
pub const LABEL: &str = "LED Brightness";

/// The brightness of the LEDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LedBrightness {
    /// The values of the core (as Java).
    #[default]
    Normal,
    /// Raised palette levels and single-colour values.
    Bright,
}

/// The brightness, shared between the configuration (set on a change of the setting) and the
/// closures which send the LED values.
pub type SharedBrightness = Rc<Cell<LedBrightness>>;

/// The level which "Bright" sends for each level of a palette entry (0 dim, 1 medium, 2 full,
/// 3 pale): dim to medium, medium to full, full and pale stay.
pub const BRIGHT_LEVELS: [i32; 4] = [1, 2, 2, 3];

/// The entries which "Bright" sends for the greys 76..79 (dark grey, grey, white, white): the
/// dark grey becomes the grey, the others stay.
pub const BRIGHT_GREYS: [i32; 4] = [77, 77, 78, 79];

/// The first entry of the greys which are not organized in levels.
const GREYS: i32 = 76;

/// The single-colour value which "Bright" sends: off (0) and full (127) stay, the values in
/// between are raised half way to full.
pub fn bright_mono(value: i32) -> i32 {
    if value <= 0 || value >= 127 {
        value
    } else {
        (value + 127) / 2
    }
}

impl LedBrightness {
    /// The brightness of the value of the setting (an index into [`OPTIONS`]).
    pub fn from_index(index: usize) -> Self {
        if index == 1 {
            LedBrightness::Bright
        } else {
            LedBrightness::Normal
        }
    }

    /// The brightness which is selected in the configuration.
    pub fn from_config(config: &Configuration, id: SettingId) -> Self {
        Self::from_index(config.custom_index(id))
    }

    /// The palette index to send to an RGB LED for the given index of the core. Black (0..3)
    /// and values outside of the palette stay.
    pub fn rgb(self, index: i32) -> i32 {
        if self == LedBrightness::Normal || !(4..80).contains(&index) {
            return index;
        }
        if index >= GREYS {
            return BRIGHT_GREYS[(index - GREYS) as usize];
        }
        index / 4 * 4 + BRIGHT_LEVELS[(index % 4) as usize]
    }

    /// The value to send to a single-colour LED for the given value of the core.
    pub fn mono(self, value: i32) -> i32 {
        match self {
            LedBrightness::Normal => value,
            LedBrightness::Bright => bright_mono(value),
        }
    }
}

/// Activate the setting in a Maschine configuration (category "Hardware Setup").
pub fn activate_setting(config: &mut Configuration, id: SettingId) {
    use crate::config::configuration::CATEGORY_HARDWARE_SETUP;
    use crate::config::setting::Scope;
    use crate::config::values::to_options;
    config.activate_enum_setting(id, Scope::Global, CATEGORY_HARDWARE_SETUP, LABEL, to_options(&OPTIONS), OPTIONS[0], false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::maschine::core::colors::*;

    #[test]
    fn normal_keeps_all_values() {
        for value in 0..128 {
            assert_eq!(LedBrightness::Normal.rgb(value), value);
            assert_eq!(LedBrightness::Normal.mono(value), value);
        }
    }

    #[test]
    fn bright_raises_the_levels() {
        let bright = LedBrightness::Bright;
        // Black stays off
        for black in 0..4 {
            assert_eq!(bright.rgb(black), black);
        }
        // Each level of a hue: dim to medium, medium to full, full and pale stay
        assert_eq!(bright.rgb(4), 5);
        assert_eq!(bright.rgb(COLOR_RED_LO), COLOR_RED);
        assert_eq!(bright.rgb(COLOR_RED), COLOR_RED);
        assert_eq!(bright.rgb(COLOR_ROSE), COLOR_ROSE);
        assert_eq!(bright.rgb(COLOR_GREEN_LO), COLOR_GREEN);
        assert_eq!(bright.rgb(COLOR_BLUE_LO), 46);
        assert_eq!(bright.rgb(64), 65);
        assert_eq!(bright.rgb(67), 67);
        // The whites with levels (68..75)
        assert_eq!(bright.rgb(68), 69);
        assert_eq!(bright.rgb(69), 70);
        assert_eq!(bright.rgb(72), 73);
        assert_eq!(bright.rgb(75), 75);
        // The greys: the dark grey becomes grey, white stays white
        assert_eq!(bright.rgb(COLOR_DARK_GREY), COLOR_GREY);
        assert_eq!(bright.rgb(COLOR_GREY), COLOR_GREY);
        assert_eq!(bright.rgb(COLOR_WHITE), COLOR_WHITE);
        assert_eq!(bright.rgb(79), 79);
        // Outside of the palette
        assert_eq!(bright.rgb(127), 127);
        assert_eq!(bright.rgb(-1), -1);
        // The hue is kept for all entries with levels
        for index in 4..76 {
            assert_eq!(bright.rgb(index) / 4, index / 4);
            assert!(bright.rgb(index) % 4 >= index % 4);
        }
    }

    #[test]
    fn bright_single_colour_values() {
        let bright = LedBrightness::Bright;
        assert_eq!(bright.mono(0), 0);
        assert_eq!(bright.mono(127), 127);
        assert_eq!(bright.mono(1), 64);
        assert_eq!(bright.mono(0x30), 87);
        assert_eq!(bright.mono(126), 126);
        for value in 1..127 {
            assert!(bright.mono(value) >= value && bright.mono(value) < 127);
        }
    }

    #[test]
    fn options() {
        assert_eq!(LedBrightness::from_index(0), LedBrightness::Normal);
        assert_eq!(LedBrightness::from_index(1), LedBrightness::Bright);
        assert_eq!(LedBrightness::from_index(7), LedBrightness::Normal);
    }
}
