// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The color manager of the Maschine controllers (former `MaschineColorManager`). The RGB
//! colors of the simulator GUI are not ported.

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::{abstract_drum_view, abstract_play_view, abstract_sequencer_view, browser_view};
use crate::framework::feature::{BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::grid::GRID_OFF;
use crate::scale::Scales;

pub const COLOR_BLACK: i32 = 0;
pub const COLOR_DARK_GREY: i32 = 76;
pub const COLOR_GREY: i32 = 77;
pub const COLOR_WHITE: i32 = 78;
pub const COLOR_ROSE: i32 = 7;
pub const COLOR_RED: i32 = 6;
pub const COLOR_RED_LO: i32 = 5;
pub const COLOR_AMBER: i32 = 14;
pub const COLOR_AMBER_LO: i32 = 13;
pub const COLOR_LIME: i32 = 34;
pub const COLOR_LIME_LO: i32 = 33;
pub const COLOR_GREEN: i32 = 30;
pub const COLOR_GREEN_LO: i32 = 29;
pub const COLOR_SPRING: i32 = 26;
pub const COLOR_SPRING_LO: i32 = 25;
pub const COLOR_TURQUOISE_LO: i32 = 27;
pub const COLOR_TURQUOISE: i32 = 31;
pub const COLOR_SKY: i32 = 38;
pub const COLOR_SKY_LO: i32 = 37;
pub const COLOR_BLUE: i32 = 42;
pub const COLOR_BLUE_LO: i32 = 45;
pub const COLOR_MAGENTA: i32 = 58;
pub const COLOR_MAGENTA_LO: i32 = 57;
pub const COLOR_PINK: i32 = 62;
pub const COLOR_PINK_LO: i32 = 61;
pub const COLOR_ORANGE: i32 = 10;
pub const COLOR_ORANGE_LO: i32 = 9;
pub const COLOR_PURPLE: i32 = 50;
pub const COLOR_PURPLE_LO: i32 = 53;
pub const COLOR_SKIN: i32 = 11;
pub const COLOR_YELLOW_LO: i32 = 21;
pub const COLOR_YELLOW: i32 = 22;

/// The colors of the 8 parameters.
pub const PARAM_COLORS: [i32; 8] = [COLOR_RED, COLOR_AMBER, COLOR_YELLOW, COLOR_GREEN, COLOR_LIME, COLOR_SKY, COLOR_PURPLE, COLOR_PINK];

/// Create the color manager with all color IDs used by the Maschine controllers.
pub fn create_color_manager() -> ColorManager {
    let mut cm = ColorManager::new();

    cm.register_color_index(GRID_OFF, COLOR_BLACK);

    cm.register_color_index(BUTTON_STATE_OFF, COLOR_BLACK);
    cm.register_color_index(BUTTON_STATE_ON, COLOR_BLACK);
    cm.register_color_index(BUTTON_STATE_HI, 127);

    cm.register_color_index(Scales::SCALE_COLOR_OFF, COLOR_BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, COLOR_BLUE);
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, COLOR_WHITE);
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, COLOR_BLACK);

    cm.register_color_index(BUTTON_COLOR_OFF, 0);
    cm.register_color_index(BUTTON_COLOR_ON, 0);
    cm.register_color_index(BUTTON_COLOR_HI, 127);

    cm.register_color_index(abstract_play_view::COLOR_PLAY, COLOR_GREEN);
    cm.register_color_index(abstract_play_view::COLOR_RECORD, COLOR_RED);
    cm.register_color_index(abstract_play_view::COLOR_OFF, COLOR_BLACK);

    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, COLOR_GREEN_LO);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED, COLOR_DARK_GREY);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, COLOR_GREY);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_SELECTED, COLOR_YELLOW);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT, COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT_4, COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT, COLOR_BLUE);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT_CONT, COLOR_BLUE_LO);
    cm.register_color_index(abstract_sequencer_view::COLOR_PAGE, COLOR_GREY);
    cm.register_color_index(abstract_sequencer_view::COLOR_ACTIVE_PAGE, COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_SELECTED_PAGE, COLOR_BLUE);
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION, COLOR_ORANGE);
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION_SELECTED, COLOR_RED);
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION_OFF, COLOR_BLACK);

    cm.register_color_index(abstract_drum_view::COLOR_PAD_OFF, COLOR_BLACK);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_RECORD, COLOR_RED);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_PLAY, COLOR_GREEN);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_SELECTED, COLOR_BLUE);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_MUTED, COLOR_AMBER);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT, COLOR_LIME);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_NO_CONTENT, COLOR_DARK_GREY);

    cm.register_color_index(browser_view::OFF, COLOR_BLACK);
    cm.register_color_index(browser_view::DISCARD, COLOR_RED);
    cm.register_color_index(browser_view::CONFIRM, COLOR_GREEN);
    cm.register_color_index(browser_view::PLAY, COLOR_AMBER_LO);
    cm.register_color_index(browser_view::COLUMN1, COLOR_WHITE);
    cm.register_color_index(browser_view::COLUMN2, COLOR_GREY);
    cm.register_color_index(browser_view::COLUMN3, COLOR_DARK_GREY);
    cm.register_color_index(browser_view::COLUMN4, COLOR_ROSE);
    cm.register_color_index(browser_view::COLUMN5, COLOR_SPRING);
    cm.register_color_index(browser_view::COLUMN6, COLOR_BLUE_LO);
    cm.register_color_index(browser_view::COLUMN7, COLOR_BLACK);
    cm.register_color_index(browser_view::COLUMN8, COLOR_YELLOW);

    cm.register_color_index("COLOR_OFF", COLOR_BLACK);
    let daw_colors = [
        (DawColor::GrayHalf, COLOR_DARK_GREY),
        (DawColor::DarkGray, COLOR_DARK_GREY),
        (DawColor::Gray, COLOR_GREY),
        (DawColor::LightGray, COLOR_GREY),
        (DawColor::Silver, COLOR_GREY),
        (DawColor::DarkBrown, COLOR_AMBER_LO),
        (DawColor::Brown, COLOR_AMBER_LO),
        (DawColor::DarkBlue, COLOR_BLUE_LO),
        (DawColor::PurpleBlue, COLOR_PURPLE_LO),
        (DawColor::Purple, COLOR_PURPLE),
        (DawColor::Pink, COLOR_PINK),
        (DawColor::Red, COLOR_RED),
        (DawColor::Orange, COLOR_ORANGE),
        (DawColor::LightOrange, COLOR_YELLOW),
        (DawColor::MossGreen, COLOR_LIME_LO),
        (DawColor::Green, COLOR_SPRING),
        (DawColor::ColdGreen, COLOR_TURQUOISE),
        (DawColor::Blue, COLOR_BLUE),
        (DawColor::LightPurple, COLOR_MAGENTA),
        (DawColor::LightPink, COLOR_PINK),
        (DawColor::Rose, COLOR_SKIN),
        (DawColor::ReddishBrown, COLOR_AMBER),
        (DawColor::LightBrown, COLOR_AMBER_LO),
        (DawColor::LightGreen, COLOR_SPRING),
        (DawColor::BluishGreen, COLOR_LIME),
        (DawColor::GreenBlue, COLOR_TURQUOISE_LO),
        (DawColor::LightBlue, COLOR_SKY),
    ];
    for (color, index) in daw_colors {
        cm.register_color_index(color.id(), index);
    }
    cm
}

/// Get the dimmed or highlighted color index of a DAW color (former `dimOrHighlightColor`).
pub fn dim_or_highlight_color(colors: &ColorManager, color: ColorEx, is_selected: bool) -> i32 {
    let color_index = colors.color_index(DawColor::color_id(color));
    if is_selected {
        return if color_index == COLOR_DARK_GREY { COLOR_WHITE } else { color_index };
    }
    color_index / 8 * 8 + 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_or_highlight() {
        let cm = create_color_manager();
        let red = DawColor::Red.color();
        assert_eq!(dim_or_highlight_color(&cm, red, true), COLOR_RED);
        assert_eq!(dim_or_highlight_color(&cm, red, false), 5);
        assert_eq!(dim_or_highlight_color(&cm, DawColor::DarkGray.color(), true), COLOR_WHITE);
        assert_eq!(cm.color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT), COLOR_LIME);
    }
}
