// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the MPC/Force (former `ACVSColorManager`).

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::grid::GRID_OFF;

pub const BUTTON_UNDO_STATE_ON: &str = "BUTTON_UNDO_STATE_ON";
pub const BUTTON_UNDO_STATE_HI: &str = "BUTTON_UNDO_STATE_HI";
pub const BUTTON_ARROW_STATE_ON: &str = "BUTTON_ARROW_STATE_ON";
pub const BUTTON_ARROW_STATE_HI: &str = "BUTTON_ARROW_STATE_HI";

pub const COLOR_BLACK: i32 = 0;
pub const COLOR_DARK_GRAY: i32 = 77;
pub const COLOR_GRAY: i32 = 35;
pub const COLOR_GRAY_HALF: i32 = 47;
pub const COLOR_LIGHT_GRAY: i32 = 21;
pub const COLOR_SILVER: i32 = 33;
pub const COLOR_DARK_BROWN: i32 = 1;
pub const COLOR_BROWN: i32 = 24;
pub const COLOR_DARK_BLUE: i32 = 71;
pub const COLOR_PURPLE_BLUE: i32 = 32;
pub const COLOR_PURPLE: i32 = 75;
pub const COLOR_PINK: i32 = 76;
pub const COLOR_RED: i32 = 3;
pub const COLOR_ORANGE: i32 = 23;
pub const COLOR_LIGHT_ORANGE: i32 = 5;
pub const COLOR_MOSS_GREEN: i32 = 27;
pub const COLOR_GREEN: i32 = 69;
pub const COLOR_COLD_GREEN: i32 = 28;
pub const COLOR_BLUE: i32 = 17;
pub const COLOR_LIGHT_PURPLE: i32 = 19;
pub const COLOR_LIGHT_PINK: i32 = 20;
pub const COLOR_ROSE: i32 = 22;
pub const COLOR_REDDISH_BROWN: i32 = 36;
pub const COLOR_LIGHT_BROWN: i32 = 9;
pub const COLOR_LIGHT_GREEN: i32 = 4;
pub const COLOR_BLUISH_GREEN: i32 = 6;
pub const COLOR_GREEN_BLUE: i32 = 14;
pub const COLOR_LIGHT_BLUE: i32 = 15;

fn rgb(c: ColorEx) -> (f64, f64, f64) {
    (c.red(), c.green(), c.blue())
}

/// Create the color manager (former `ACVSColorManager`).
pub fn create_color_manager() -> ColorManager {
    let mut cm = ColorManager::new();
    cm.register_color_index(GRID_OFF, 0);
    cm.register_color_index(BUTTON_STATE_OFF, 0);
    cm.register_color_index(BUTTON_STATE_ON, 0);
    cm.register_color_index(BUTTON_STATE_HI, 127);
    cm.register_color_index(BUTTON_UNDO_STATE_ON, 2);
    cm.register_color_index(BUTTON_UNDO_STATE_HI, 1);
    cm.register_color_index(BUTTON_ARROW_STATE_ON, 3);
    cm.register_color_index(BUTTON_ARROW_STATE_HI, 4);

    let colors = [
        (COLOR_BLACK, ColorEx::BLACK),
        (COLOR_GRAY_HALF, ColorEx::LIGHT_GRAY),
        (COLOR_DARK_GRAY, ColorEx::DARK_GRAY),
        (COLOR_GRAY, ColorEx::GRAY),
        (COLOR_LIGHT_GRAY, ColorEx::LIGHT_GRAY),
        (COLOR_SILVER, ColorEx::LIGHT_GRAY),
        (COLOR_DARK_BROWN, ColorEx::DARK_BROWN),
        (COLOR_BROWN, ColorEx::BROWN),
        (COLOR_DARK_BLUE, ColorEx::DARK_BLUE),
        (COLOR_PURPLE_BLUE, ColorEx::BLUE),
        (COLOR_PURPLE, ColorEx::PURPLE),
        (COLOR_PINK, ColorEx::PINK),
        (COLOR_RED, ColorEx::RED),
        (COLOR_ORANGE, ColorEx::ORANGE),
        (COLOR_LIGHT_ORANGE, ColorEx::ORANGE),
        (COLOR_MOSS_GREEN, ColorEx::GREEN),
        (COLOR_GREEN, ColorEx::GREEN),
        (COLOR_COLD_GREEN, ColorEx::GREEN),
        (COLOR_BLUE, ColorEx::BLUE),
        (COLOR_LIGHT_PURPLE, ColorEx::PURPLE),
        (COLOR_LIGHT_PINK, ColorEx::PINK),
        (COLOR_ROSE, ColorEx::PINK),
        (COLOR_REDDISH_BROWN, ColorEx::BROWN),
        (COLOR_LIGHT_BROWN, ColorEx::BROWN),
        (COLOR_LIGHT_GREEN, ColorEx::GREEN),
        (COLOR_BLUISH_GREEN, ColorEx::GREEN),
        (COLOR_GREEN_BLUE, ColorEx::BLUE),
        (COLOR_LIGHT_BLUE, ColorEx::BLUE),
        (2, ColorEx::RED),
        (127, ColorEx::BLACK),
    ];
    for (index, color) in colors {
        cm.register_color(index, rgb(color));
    }

    let daw_colors = [
        (DawColor::ColorOff, COLOR_BLACK),
        (DawColor::GrayHalf, COLOR_GRAY_HALF),
        (DawColor::DarkGray, COLOR_DARK_GRAY),
        (DawColor::Gray, COLOR_GRAY),
        (DawColor::LightGray, COLOR_LIGHT_GRAY),
        (DawColor::Silver, COLOR_SILVER),
        (DawColor::DarkBrown, COLOR_DARK_BROWN),
        (DawColor::Brown, COLOR_BROWN),
        (DawColor::DarkBlue, COLOR_DARK_BLUE),
        (DawColor::PurpleBlue, COLOR_PURPLE_BLUE),
        (DawColor::Purple, COLOR_PURPLE),
        (DawColor::Pink, COLOR_PINK),
        (DawColor::Red, COLOR_RED),
        (DawColor::Orange, COLOR_ORANGE),
        (DawColor::LightOrange, COLOR_LIGHT_ORANGE),
        (DawColor::MossGreen, COLOR_MOSS_GREEN),
        (DawColor::Green, COLOR_GREEN),
        (DawColor::ColdGreen, COLOR_COLD_GREEN),
        (DawColor::Blue, COLOR_BLUE),
        (DawColor::LightPurple, COLOR_LIGHT_PURPLE),
        (DawColor::LightPink, COLOR_LIGHT_PINK),
        (DawColor::Rose, COLOR_ROSE),
        (DawColor::ReddishBrown, COLOR_REDDISH_BROWN),
        (DawColor::LightBrown, COLOR_LIGHT_BROWN),
        (DawColor::LightGreen, COLOR_LIGHT_GREEN),
        (DawColor::BluishGreen, COLOR_BLUISH_GREEN),
        (DawColor::GreenBlue, COLOR_GREEN_BLUE),
        (DawColor::LightBlue, COLOR_LIGHT_BLUE),
    ];
    for (color, index) in daw_colors {
        cm.register_color_index(color.id(), index);
    }
    cm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        let cm = create_color_manager();
        assert_eq!(cm.color_index(BUTTON_STATE_HI), 127);
        assert_eq!(cm.color_index(BUTTON_UNDO_STATE_HI), 1);
        assert_eq!(cm.color_index(DawColor::LightBlue.id()), COLOR_LIGHT_BLUE);
    }
}
