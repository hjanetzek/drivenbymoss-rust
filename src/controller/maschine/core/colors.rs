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

/// NI's color palette of the Maschines (hue * 4 + level; level 0 dim, 1 medium, 2 full, 3 pale)
/// in RGB, as the devices show it: 0..71 from cabl's JAM palette (ni-usb/cabl
/// `MaschineJamHelper.cpp`, (C) 2015 Vincenzo Pacella, MIT license), 72..79 greys and white (as
/// support/linux/ni_host/src/palette.cpp). The nearest entry is the color of an RGB color which
/// does not come from Bitwig ([`ColorManager::set_palette_rgb`]).
pub const NI_PALETTE: [(u8, u8, u8); 80] = [
    (0, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0),
    (32, 0, 0), (64, 0, 0), (255, 0, 0), (255, 32, 16),
    (32, 16, 0), (64, 16, 0), (255, 32, 0), (255, 64, 32),
    (32, 48, 8), (64, 64, 0), (255, 96, 0), (255, 128, 48),
    (40, 56, 0), (64, 96, 0), (255, 224, 0), (128, 255, 64),
    (24, 40, 0), (96, 176, 0), (96, 255, 0), (112, 255, 64),
    (16, 40, 0), (56, 112, 0), (72, 255, 0), (64, 255, 64),
    (0, 40, 0), (0, 96, 0), (0, 255, 0), (48, 255, 48),
    (0, 40, 16), (0, 112, 32), (0, 255, 48), (56, 255, 40),
    (0, 40, 40), (0, 112, 96), (0, 255, 226), (64, 255, 255),
    (0, 24, 48), (0, 64, 112), (0, 96, 255), (40, 160, 255),
    (0, 0, 40), (0, 0, 128), (0, 0, 255), (40, 104, 255),
    (8, 0, 44), (32, 0, 128), (40, 0, 255), (48, 88, 255),
    (24, 0, 40), (72, 0, 128), (104, 0, 255), (88, 80, 255),
    (32, 0, 48), (96, 0, 96), (255, 0, 128), (255, 64, 160),
    (32, 0, 16), (90, 0, 48), (255, 0, 64), (255, 40, 104),
    (40, 0, 8), (112, 0, 16), (255, 0, 16), (255, 48, 56),
    (40, 40, 40), (96, 96, 96), (226, 226, 226), (160, 255, 255),
    (40, 40, 40), (96, 96, 96), (226, 226, 226), (255, 255, 255),
    (24, 24, 24), (64, 64, 64), (255, 255, 255), (255, 255, 255),
];

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
    let palette: Vec<(i32, ColorEx)> = NI_PALETTE.iter().enumerate().map(|(i, &(r, g, b))| (i as i32, ColorEx::from_rgb(r.into(), g.into(), b.into()))).collect();
    cm.set_palette_rgb(&palette);
    cm
}

/// Get the dimmed or highlighted color index of a DAW color (former `dimOrHighlightColor`).
/// On the direct path (not Bitwig) the dimmed color is the medium level of the same hue, the
/// greys a dark grey: Java's `index / 8 * 8 + 5` is meant for its DAW color table and would
/// change the hue of other entries (yellow 18 to the green 21).
pub fn dim_or_highlight_color(colors: &ColorManager, color: ColorEx, is_selected: bool) -> i32 {
    let color_index = colors.index_for_color(color);
    if is_selected {
        let is_dark_grey = color_index == COLOR_DARK_GREY || colors.is_direct() && matches!(color_index, 68 | 72);
        return if is_dark_grey { COLOR_WHITE } else { color_index };
    }
    if colors.is_direct() {
        return if color_index >= 68 { COLOR_DARK_GREY } else { color_index / 4 * 4 + 1 };
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
