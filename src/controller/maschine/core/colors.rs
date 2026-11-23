// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The color manager of the Maschine controllers (former `MaschineColorManager`). The RGB
//! colors of the simulator GUI are not ported.

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, ColorRole, RoleLevels, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
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
/// in RGB, as the devices drive it: 4..67 (hues 1..16) are the JAM's factory pad palette, read
/// from the device (feature report 0xF1, `support/linux/ni_host/data/jam-feature-reports-factory.json`,
/// 0..127 scaled to 0..255; LED drive values, not measured light); 68..79 are cabl's greys and
/// white (ni-usb/cabl `MaschineJamHelper.cpp`, (C) 2015 Vincenzo Pacella, MIT license, as
/// support/linux/ni_host/src/palette.cpp), which all show the same white on the JAM (the
/// device's hue 17 is a greenish drive value of that white). The direct path takes the hue of an
/// RGB color from the full levels ([`ni_hues`], [`ColorManager::set_palette_hue_levels`]).
pub const NI_PALETTE: [(u8, u8, u8); 80] = [
    (0, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0),
    (54, 0, 0), (78, 0, 0), (255, 0, 0), (255, 30, 30),
    (60, 14, 0), (106, 22, 0), (211, 28, 0), (255, 80, 30),
    (48, 24, 0), (66, 50, 0), (251, 112, 0), (255, 137, 30),
    (68, 72, 0), (90, 90, 0), (247, 221, 0), (211, 255, 60),
    (42, 60, 0), (82, 102, 0), (181, 231, 0), (151, 235, 60),
    (26, 90, 0), (58, 151, 0), (84, 255, 0), (70, 255, 50),
    (0, 52, 0), (0, 102, 0), (0, 255, 0), (50, 255, 40),
    (0, 84, 12), (0, 110, 30), (0, 255, 62), (54, 255, 104),
    (0, 64, 52), (0, 110, 98), (0, 255, 241), (50, 255, 255),
    (0, 44, 84), (0, 70, 110), (0, 96, 255), (40, 151, 255),
    (0, 0, 60), (0, 0, 110), (0, 0, 255), (40, 100, 255),
    (16, 0, 76), (40, 0, 131), (56, 0, 255), (70, 80, 255),
    (24, 0, 80), (66, 0, 126), (100, 0, 255), (100, 80, 255),
    (72, 0, 40), (102, 0, 90), (255, 0, 124), (255, 60, 153),
    (48, 0, 14), (90, 0, 36), (255, 0, 60), (255, 40, 100),
    (84, 0, 6), (110, 0, 22), (255, 0, 20), (255, 50, 48),
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
    cm.set_palette_hue_levels(&ni_hues(), NI_LEVELS, NI_WHITES);
    cm
}

/// The levels of NI's palette for the roles of the direct path (offsets from the first index of
/// a hue): disabled dim, inactive medium, normal full, highlight pale. All four are
/// distinguishable on the JAM (measured 2026-11-23).
pub const NI_LEVELS: RoleLevels = RoleLevels::new(0, 1, 2, 3);

/// The whites of NI's palette for the roles (cabl's grey steps; on the JAM 68..79 are all the
/// same white, on other devices they may differ).
pub const NI_WHITES: RoleLevels = RoleLevels::new(COLOR_DARK_GREY, COLOR_GREY, COLOR_WHITE, 79);

/// The 16 hues of NI's palette (1..16, first index hue * 4) with their full level.
pub fn ni_hues() -> Vec<(i32, ColorEx)> {
    (1..17)
        .map(|hue| {
            let (r, g, b) = NI_PALETTE[hue * 4 + 2];
            (hue as i32 * 4, ColorEx::from_rgb(r.into(), g.into(), b.into()))
        })
        .collect()
}

/// Get the dimmed or highlighted color index of a DAW color (former `dimOrHighlightColor`).
/// On the direct path (not Bitwig) the roles of the color manager: a selected item is the color
/// at its normal (full) level, the others the inactive (medium) level of the same hue, the
/// achromatic colors white and grey. Java's `index / 8 * 8 + 5` is meant for its DAW color
/// table and would change the hue of other entries (yellow 18 to the green 21).
pub fn dim_or_highlight_color(colors: &ColorManager, color: ColorEx, is_selected: bool) -> i32 {
    if colors.is_direct() {
        return colors.index_for_color_role(color, if is_selected { ColorRole::Normal } else { ColorRole::Inactive });
    }
    let color_index = colors.index_for_color(color);
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
