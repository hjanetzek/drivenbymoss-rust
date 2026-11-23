// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The LED brightness setting of the Maschine controllers (not in the Java version).
//!
//! The RGB LEDs (pads, group and scene buttons, the JAM's touch strips) take an index into NI's
//! palette (`hue * 4 + level`, level 0 dim, 1 medium, 2 full, 3 pale, see
//! [`super::colors::NI_PALETTE`]); there is no separate brightness. The single-colour LEDs take
//! their brightness as the MIDI value (0..127). "Normal" sends the values of the core unchanged
//! (as Java); "Bright" raises the dim ones when they are sent ([`LedBrightness::rgb`],
//! [`LedBrightness::mono`]), so the setting covers all modes, views and the surface link. The
//! mapping is the table [`BRIGHT_LEVELS`] and [`bright_mono`].
//!
//! Measured on the real JAM (2026-11-23, `dbm-nihost-led-test --pattern rainbow --level N`):
//! all four levels are distinguishable (dim clearly dimmer than medium, medium than full, full
//! than pale); dim is visible. Full (2) and pale (3) are both bright enough for studio use, pale
//! is the brightest; the dim look comes from the dim and medium levels. The entries 68..79 all
//! show the same white: the grey steps of the cabl table ([`super::colors::NI_PALETTE`]) do not
//! exist on the device, so Bright leaves them as they are; 80..127 show nothing new. The JAM drives its LEDs in time slices (stripes on a photo,
//! about 25% duty), so even pale stays dimmer than the firmware's startup animation; a
//! device-wide brightness setting of the JAM may exist, but it is not known.
//!
//! The JAM's feature report 0xF1 is its live pad palette (68 RGB entries, 17 hues x 4 levels,
//! 0..127), which the device stores in flash (it survives a replug). DrivenByMoss does not
//! write it; a custom palette (e.g. brighter levels) would be possible, but every write wears
//! the flash, so it must not be written on every start or setting change.

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
/// 3 pale): only the dimmest level is raised, dim becomes medium; medium, full and pale stay.
/// The core tells states apart by the level: full for active (the normal colors), pale for a
/// highlight, medium for inactive or available (the `*_LO` colors); on the direct color path
/// these are the roles of [`crate::framework::color::ColorRole`] (normal full, highlight pale,
/// inactive medium, disabled dim). They stay distinct in Bright, as in Normal. Bright is applied
/// after the color mapping, when the LEDs are sent, so an inactive medium stays medium; dim
/// becomes medium, so Bright merges disabled and inactive (the "brighter, fewer steps" option).
pub const BRIGHT_LEVELS: [i32; 4] = [1, 1, 2, 3];

/// The first entry of the whites and greys (68..79), which all look the same on the device.
const WHITES: i32 = 68;

/// The single-colour value which "Bright" sends: off (0) and full (127) stay, the values in
/// between are raised half way to full. The core sends only 0 and 127 to the single-colour
/// LEDs (`BUTTON_STATE_ON` is 0, `BUTTON_STATE_HI` 127), so today this changes nothing; it is
/// kept for values in between. Their brightness on the device is the curve of dbm-nihost
/// (`jamMonoBrightness`) or of NI's driver.
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

    /// The palette index to send to an RGB LED for the given index of the core. Black (0..3),
    /// the whites (68..79) and values outside of the palette stay.
    pub fn rgb(self, index: i32) -> i32 {
        if self == LedBrightness::Normal || !(4..WHITES).contains(&index) {
            return index;
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
        // Only dim goes up (to medium); medium, full and pale stay
        assert_eq!(bright.rgb(4), 5);
        assert_eq!(bright.rgb(COLOR_RED_LO), COLOR_RED_LO);
        assert_eq!(bright.rgb(COLOR_RED), COLOR_RED);
        assert_eq!(bright.rgb(COLOR_ROSE), COLOR_ROSE);
        assert_eq!(bright.rgb(COLOR_GREEN_LO), COLOR_GREEN_LO);
        assert_eq!(bright.rgb(COLOR_GREEN), COLOR_GREEN);
        assert_eq!(bright.rgb(44), COLOR_BLUE_LO);
        assert_eq!(bright.rgb(64), 65);
        assert_eq!(bright.rgb(66), 66);
        assert_eq!(bright.rgb(67), 67);
        // Inactive (medium), active (full) and highlight (pale) stay distinct
        for hue in 1..17 {
            let (dim, medium, full, pale) = (hue * 4, hue * 4 + 1, hue * 4 + 2, hue * 4 + 3);
            assert_eq!(bright.rgb(dim), medium);
            assert_ne!(bright.rgb(medium), bright.rgb(full));
            assert_ne!(bright.rgb(full), bright.rgb(pale));
            assert_eq!(bright.rgb(full), full);
            assert_eq!(bright.rgb(pale), pale);
        }
        // The whites and greys (68..79) look the same on the device and stay
        for white in 68..80 {
            assert_eq!(bright.rgb(white), white);
        }
        assert_eq!(bright.rgb(COLOR_DARK_GREY), COLOR_DARK_GREY);
        assert_eq!(bright.rgb(COLOR_WHITE), COLOR_WHITE);
        // Outside of the palette
        assert_eq!(bright.rgb(127), 127);
        assert_eq!(bright.rgb(-1), -1);
        // The hue is kept for all entries with levels
        for index in 4..68 {
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
