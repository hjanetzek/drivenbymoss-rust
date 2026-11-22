// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the Electra One (former `ElectraOneColorManager`). The controls get their RGB
//! colors with the JSON control updates; the color indices only serve the pad lights, which
//! send 127 (on) or 0 (off).

use crate::daw::color::ColorEx;
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::feature::BUTTON_COLOR_OFF;

pub const COLOR_BUTTON_STATE_ON: i32 = 127;
pub const COLOR_BUTTON_STATE_OFF: i32 = 0;

pub const PLAY_OFF: ColorEx = ColorEx::DARK_GREEN;
pub const PLAY_ON: ColorEx = ColorEx::GREEN;
pub const RECORD_OFF: ColorEx = ColorEx::DARK_RED;
pub const RECORD_ON: ColorEx = ColorEx::RED;
pub const REC_ARM_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const REC_ARM_ON: ColorEx = ColorEx::DARK_RED;
pub const MUTE_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const MUTE_ON: ColorEx = ColorEx::ORANGE;
pub const SOLO_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const SOLO_ON: ColorEx = ColorEx::YELLOW;
pub const SELECT_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const SELECT_ON: ColorEx = ColorEx::CYAN;
pub const PINNED_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const PINNED_ON: ColorEx = ColorEx::CYAN;
pub const EXPANDED_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const EXPANDED_ON: ColorEx = ColorEx::MINT;
pub const PARAM_PAGE: ColorEx = ColorEx::DARK_BLUE;
pub const PARAM_PAGE_SELECTED: ColorEx = ColorEx::SKY_BLUE;
pub const DEVICE: ColorEx = ColorEx::DARK_PURPLE;
pub const DEVICE_SELECTED: ColorEx = ColorEx::PURPLE;
pub const WINDOW: ColorEx = ColorEx::DARK_GRAY;
pub const WINDOW_OPEN: ColorEx = ColorEx::DARK_BROWN;
pub const DEVICE_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const DEVICE_ON: ColorEx = ColorEx::DARK_GREEN;
pub const PROJECT_PARAM_PAGE: ColorEx = ColorEx::DARK_PINK;
pub const PROJECT_PARAM_PAGE_SELECTED: ColorEx = ColorEx::PINK;
pub const TRACK_PARAM_PAGE: ColorEx = ColorEx::DARK_MINT;
pub const TRACK_PARAM_PAGE_SELECTED: ColorEx = ColorEx::MINT;
pub const BAND_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const BAND_ON: ColorEx = ColorEx::GREEN;
pub const METRONOME_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const METRONOME_ON: ColorEx = ColorEx::GREEN;
pub const AUTO_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const AUTO_ON: ColorEx = ColorEx::RED;
pub const AUTO_MODE_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const AUTO_MODE_ON: ColorEx = ColorEx::ORANGE;
pub const MARKER_LAUNCH_OFF: ColorEx = ColorEx::DARK_GRAY;
pub const MARKER_LAUNCH_ON: ColorEx = ColorEx::GREEN;

/// Create the color manager (former `ElectraOneColorManager`).
pub fn create_color_manager() -> ColorManager {
    let mut cm = ColorManager::new();
    cm.register_color_index(BUTTON_COLOR_OFF, 0);
    cm.register_color_index(BUTTON_STATE_OFF, COLOR_BUTTON_STATE_OFF);
    cm.register_color_index(BUTTON_STATE_ON, COLOR_BUTTON_STATE_ON);
    cm.register_color_index(BUTTON_STATE_HI, COLOR_BUTTON_STATE_ON);
    for i in 0..127 {
        cm.register_color(i, (0.0, 0.0, 0.0));
    }
    cm.register_color(127, (1.0, 1.0, 1.0));
    cm
}

/// The color as the JSON of a control update wants it: `RRGGBB` in upper case (former
/// `StringUtils.formatColor`).
pub fn format_color(color: ColorEx) -> String {
    let [r, g, b] = color.to_int_rgb255();
    format!("{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(format_color(ColorEx::ORANGE), "FF5000");
        assert_eq!(format_color(ColorEx::SKY_BLUE), "61EEFF");
        assert_eq!(format_color(ColorEx::BLACK), "000000");
        let cm = create_color_manager();
        assert_eq!(cm.color_index(BUTTON_STATE_HI), 127);
        assert_eq!(cm.color(127), (1.0, 1.0, 1.0));
    }
}
