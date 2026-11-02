// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the APC40 Mk1 and Mk2 (former `APCColorManager`).

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::*;
use crate::framework::feature::BUTTON_COLOR_OFF;
use crate::framework::grid::GRID_OFF;
use crate::scale::Scales;

// APC Colors for 5x8 clip matrix
pub const APC_COLOR_BLACK: i32 = 0;
pub const APC_COLOR_GREEN: i32 = 1;
pub const APC_COLOR_GREEN_BLINK: i32 = 2;
pub const APC_COLOR_RED: i32 = 3;
pub const APC_COLOR_RED_BLINK: i32 = 4;
pub const APC_COLOR_YELLOW: i32 = 5;
pub const APC_COLOR_YELLOW_BLINK: i32 = 6;

// APC40mkII Colors for 5x8 clip matrix
pub const APC_MKII_COLOR_BLACK: i32 = 0;
pub const APC_MKII_COLOR_GREY_LO: i32 = 1;
pub const APC_MKII_COLOR_GREY_MD: i32 = 103;
pub const APC_MKII_COLOR_GREY_LT: i32 = 2;
pub const APC_MKII_COLOR_WHITE: i32 = 3;
pub const APC_MKII_COLOR_ROSE: i32 = 4;
pub const APC_MKII_COLOR_RED_HI: i32 = 5;
pub const APC_MKII_COLOR_RED: i32 = 6;
pub const APC_MKII_COLOR_RED_LO: i32 = 7;
pub const APC_MKII_COLOR_RED_AMBER: i32 = 8;
pub const APC_MKII_COLOR_AMBER_HI: i32 = 9;
pub const APC_MKII_COLOR_AMBER: i32 = 10;
pub const APC_MKII_COLOR_AMBER_LO: i32 = 11;
pub const APC_MKII_COLOR_AMBER_YELLOW: i32 = 12;
pub const APC_MKII_COLOR_YELLOW_HI: i32 = 13;
pub const APC_MKII_COLOR_YELLOW: i32 = 14;
pub const APC_MKII_COLOR_YELLOW_LO: i32 = 15;
pub const APC_MKII_COLOR_YELLOW_LIME: i32 = 16;
pub const APC_MKII_COLOR_LIME_HI: i32 = 17;
pub const APC_MKII_COLOR_LIME: i32 = 18;
pub const APC_MKII_COLOR_LIME_LO: i32 = 19;
pub const APC_MKII_COLOR_LIME_GREEN: i32 = 20;
pub const APC_MKII_COLOR_GREEN_HI: i32 = 21;
pub const APC_MKII_COLOR_GREEN: i32 = 22;
pub const APC_MKII_COLOR_GREEN_LO: i32 = 23;
pub const APC_MKII_COLOR_GREEN_SPRING: i32 = 24;
pub const APC_MKII_COLOR_SPRING_HI: i32 = 25;
pub const APC_MKII_COLOR_SPRING: i32 = 26;
pub const APC_MKII_COLOR_SPRING_LO: i32 = 27;
pub const APC_MKII_COLOR_SPRING_TURQUOISE: i32 = 28;
pub const APC_MKII_COLOR_TURQUOISE_LO: i32 = 29;
pub const APC_MKII_COLOR_TURQUOISE: i32 = 30;
pub const APC_MKII_COLOR_TURQUOISE_HI: i32 = 31;
pub const APC_MKII_COLOR_TURQUOISE_CYAN: i32 = 32;
pub const APC_MKII_COLOR_CYAN_HI: i32 = 33;
pub const APC_MKII_COLOR_CYAN: i32 = 34;
pub const APC_MKII_COLOR_CYAN_LO: i32 = 35;
pub const APC_MKII_COLOR_CYAN_SKY: i32 = 36;
pub const APC_MKII_COLOR_SKY_HI: i32 = 37;
pub const APC_MKII_COLOR_SKY: i32 = 38;
pub const APC_MKII_COLOR_SKY_LO: i32 = 39;
pub const APC_MKII_COLOR_SKY_OCEAN: i32 = 40;
pub const APC_MKII_COLOR_OCEAN_HI: i32 = 41;
pub const APC_MKII_COLOR_OCEAN: i32 = 42;
pub const APC_MKII_COLOR_OCEAN_LO: i32 = 43;
pub const APC_MKII_COLOR_OCEAN_BLUE: i32 = 44;
pub const APC_MKII_COLOR_BLUE_HI: i32 = 45;
pub const APC_MKII_COLOR_BLUE: i32 = 46;
pub const APC_MKII_COLOR_BLUE_LO: i32 = 47;
pub const APC_MKII_COLOR_BLUE_ORCHID: i32 = 48;
pub const APC_MKII_COLOR_ORCHID_HI: i32 = 49;
pub const APC_MKII_COLOR_ORCHID: i32 = 50;
pub const APC_MKII_COLOR_ORCHID_LO: i32 = 51;
pub const APC_MKII_COLOR_ORCHID_MAGENTA: i32 = 52;
pub const APC_MKII_COLOR_MAGENTA_HI: i32 = 53;
pub const APC_MKII_COLOR_MAGENTA: i32 = 54;
pub const APC_MKII_COLOR_MAGENTA_LO: i32 = 55;
pub const APC_MKII_COLOR_MAGENTA_PINK: i32 = 56;
pub const APC_MKII_COLOR_PINK_HI: i32 = 57;
pub const APC_MKII_COLOR_PINK: i32 = 58;
pub const APC_MKII_COLOR_PINK_LO: i32 = 59;

pub const COLOR_VIEW_SELECTED: &str = "COLOR_VIEW_SELECTED";
pub const COLOR_VIEW_UNSELECTED: &str = "COLOR_VIEW_UNSELECTED";
pub const COLOR_VIEW_OFF: &str = "COLOR_VIEW_OFF";
pub const COLOR_KEY_WHITE: &str = "COLOR_KEY_WHITE";
pub const COLOR_KEY_BLACK: &str = "COLOR_KEY_BLACK";
pub const COLOR_KEY_SELECTED: &str = "COLOR_KEY_SELECTED";
pub const BUTTON_STATE_BLINK: &str = "BUTTON_STATE_BLINK";

fn rgb(c: ColorEx) -> (f64, f64, f64) {
    (c.red(), c.green(), c.blue())
}

/// Create the color manager (former `APCColorManager`).
pub fn create_color_manager(is_mk2: bool) -> ColorManager {
    let mut cm = ColorManager::new();
    let mut reg = |key: &str, mk2: i32, mk1: i32| cm.register_color_index(key, if is_mk2 { mk2 } else { mk1 });

    reg(Scales::SCALE_COLOR_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(Scales::SCALE_COLOR_OCTAVE, APC_MKII_COLOR_OCEAN_HI, APC_COLOR_YELLOW);
    reg(Scales::SCALE_COLOR_NOTE, APC_MKII_COLOR_WHITE, APC_COLOR_BLACK);
    reg(Scales::SCALE_COLOR_OUT_OF_SCALE, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);

    reg(BUTTON_COLOR_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);

    reg(abstract_session_view::COLOR_SCENE, APC_MKII_COLOR_GREEN, APC_COLOR_GREEN);
    reg(abstract_session_view::COLOR_SELECTED_SCENE, APC_MKII_COLOR_GREEN_HI, APC_COLOR_GREEN_BLINK);
    reg(abstract_session_view::COLOR_SCENE_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);

    reg(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, APC_MKII_COLOR_GREEN_HI, APC_COLOR_GREEN);
    reg(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, APC_MKII_COLOR_GREEN_LO, APC_COLOR_GREEN);
    reg(abstract_sequencer_view::COLOR_STEP_MUTED, APC_MKII_COLOR_GREY_MD, APC_COLOR_GREEN);
    reg(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, APC_MKII_COLOR_GREY_LO, APC_COLOR_GREEN);
    reg(abstract_sequencer_view::COLOR_STEP_SELECTED, APC_MKII_COLOR_YELLOW, APC_COLOR_GREEN);
    reg(abstract_sequencer_view::COLOR_NO_CONTENT, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(abstract_sequencer_view::COLOR_NO_CONTENT_4, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(abstract_sequencer_view::COLOR_CONTENT, APC_MKII_COLOR_BLUE_HI, APC_COLOR_RED);
    reg(abstract_sequencer_view::COLOR_CONTENT_CONT, APC_MKII_COLOR_BLUE_LO, APC_COLOR_RED);
    reg(abstract_sequencer_view::COLOR_PAGE, APC_MKII_COLOR_WHITE, APC_COLOR_GREEN);
    reg(abstract_sequencer_view::COLOR_ACTIVE_PAGE, APC_MKII_COLOR_GREEN_HI, APC_COLOR_YELLOW);
    reg(abstract_sequencer_view::COLOR_SELECTED_PAGE, APC_MKII_COLOR_BLUE_LO, APC_COLOR_RED);

    reg(abstract_drum_view::COLOR_PAD_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(abstract_drum_view::COLOR_PAD_RECORD, APC_MKII_COLOR_RED_HI, APC_COLOR_RED);
    reg(abstract_drum_view::COLOR_PAD_PLAY, APC_MKII_COLOR_GREEN_HI, APC_COLOR_GREEN);
    reg(abstract_drum_view::COLOR_PAD_SELECTED, APC_MKII_COLOR_BLUE_HI, APC_COLOR_YELLOW_BLINK);
    reg(abstract_drum_view::COLOR_PAD_MUTED, APC_MKII_COLOR_AMBER_LO, APC_COLOR_BLACK);
    reg(abstract_drum_view::COLOR_PAD_HAS_CONTENT, APC_MKII_COLOR_YELLOW_HI, APC_COLOR_YELLOW);
    reg(abstract_drum_view::COLOR_PAD_NO_CONTENT, APC_MKII_COLOR_YELLOW_LO, APC_COLOR_BLACK);

    reg(abstract_drum_ex_view::COLOR_EX_SELECT_ON, APC_MKII_COLOR_WHITE, APC_COLOR_GREEN_BLINK);
    reg(abstract_drum_ex_view::COLOR_EX_SELECT_OFF, APC_MKII_COLOR_GREY_LO, APC_COLOR_GREEN);
    reg(abstract_drum_ex_view::COLOR_EX_MUTE_ON, APC_MKII_COLOR_YELLOW_HI, APC_COLOR_RED_BLINK);
    reg(abstract_drum_ex_view::COLOR_EX_MUTE_OFF, APC_MKII_COLOR_YELLOW_LO, APC_COLOR_RED);
    reg(abstract_drum_ex_view::COLOR_EX_SOLO_ON, APC_MKII_COLOR_BLUE_HI, APC_COLOR_YELLOW_BLINK);
    reg(abstract_drum_ex_view::COLOR_EX_SOLO_OFF, APC_MKII_COLOR_BLUE_LO, APC_COLOR_YELLOW);
    reg(abstract_drum_ex_view::COLOR_EX_BROWSE_ON, APC_MKII_COLOR_CYAN_HI, APC_COLOR_GREEN_BLINK);
    reg(abstract_drum_ex_view::COLOR_EX_BROWSE_OFF, APC_MKII_COLOR_CYAN_LO, APC_COLOR_GREEN);
    reg(abstract_drum_ex_view::COLOR_EX_NOTE_REPEAT_ON, APC_MKII_COLOR_ORCHID_HI, APC_COLOR_RED_BLINK);
    reg(abstract_drum_ex_view::COLOR_EX_NOTE_REPEAT_OFF, APC_MKII_COLOR_ORCHID_LO, APC_COLOR_RED);
    reg(abstract_drum_ex_view::COLOR_EX_NOTE_REPEAT_PERIOD_ON, APC_MKII_COLOR_SKY_HI, APC_COLOR_YELLOW_BLINK);
    reg(abstract_drum_ex_view::COLOR_EX_NOTE_REPEAT_PERIOD_OFF, APC_MKII_COLOR_SKY_LO, APC_COLOR_YELLOW);
    reg(abstract_drum_ex_view::COLOR_EX_NOTE_REPEAT_LENGTH_ON, APC_MKII_COLOR_PINK_HI, APC_COLOR_GREEN_BLINK);
    reg(abstract_drum_ex_view::COLOR_EX_NOTE_REPEAT_LENGTH_OFF, APC_MKII_COLOR_PINK_LO, APC_COLOR_GREEN);
    reg(abstract_drum_ex_view::COLOR_EX_TOGGLE_ON, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(abstract_drum_ex_view::COLOR_EX_TOGGLE_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);

    reg(abstract_play_view::COLOR_PLAY, APC_MKII_COLOR_GREEN, APC_COLOR_GREEN);
    reg(abstract_play_view::COLOR_RECORD, APC_MKII_COLOR_RED, APC_COLOR_RED);
    reg(abstract_play_view::COLOR_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);

    reg(GRID_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(DawColor::ColorOff.id(), APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);

    reg(BUTTON_STATE_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(BUTTON_STATE_ON, 1, 1);
    reg(BUTTON_STATE_HI, 2, 2);
    reg(BUTTON_STATE_BLINK, 3, 3);

    reg(COLOR_VIEW_SELECTED, APC_MKII_COLOR_GREEN, APC_COLOR_GREEN);
    reg(COLOR_VIEW_UNSELECTED, APC_MKII_COLOR_AMBER, APC_COLOR_BLACK);
    reg(COLOR_VIEW_OFF, APC_MKII_COLOR_BLACK, APC_COLOR_BLACK);
    reg(COLOR_KEY_WHITE, APC_MKII_COLOR_AMBER_LO, APC_COLOR_GREEN);
    reg(COLOR_KEY_BLACK, APC_MKII_COLOR_RED_HI, APC_COLOR_RED);
    reg(COLOR_KEY_SELECTED, APC_MKII_COLOR_GREEN_HI, APC_COLOR_YELLOW);

    if is_mk2 {
        let daw_colors = [
            (DawColor::GrayHalf, APC_MKII_COLOR_GREY_MD),
            (DawColor::DarkGray, APC_MKII_COLOR_GREY_LO),
            (DawColor::Gray, APC_MKII_COLOR_GREY_MD),
            (DawColor::LightGray, APC_MKII_COLOR_GREY_LT),
            (DawColor::Silver, APC_MKII_COLOR_SKY_OCEAN),
            (DawColor::DarkBrown, APC_MKII_COLOR_AMBER_LO),
            (DawColor::Brown, APC_MKII_COLOR_AMBER_YELLOW),
            (DawColor::DarkBlue, APC_MKII_COLOR_OCEAN),
            (DawColor::PurpleBlue, APC_MKII_COLOR_OCEAN_BLUE),
            (DawColor::Purple, APC_MKII_COLOR_PINK),
            (DawColor::Pink, APC_MKII_COLOR_PINK_HI),
            (DawColor::Red, APC_MKII_COLOR_RED),
            (DawColor::Orange, APC_MKII_COLOR_AMBER),
            (DawColor::LightOrange, APC_MKII_COLOR_YELLOW),
            (DawColor::MossGreen, APC_MKII_COLOR_SPRING),
            (DawColor::Green, APC_MKII_COLOR_LIME_LO),
            (DawColor::ColdGreen, APC_MKII_COLOR_TURQUOISE),
            (DawColor::Blue, APC_MKII_COLOR_SKY_HI),
            (DawColor::LightPurple, APC_MKII_COLOR_BLUE_ORCHID),
            (DawColor::LightPink, APC_MKII_COLOR_MAGENTA_PINK),
            (DawColor::Rose, APC_MKII_COLOR_ROSE),
            (DawColor::ReddishBrown, APC_MKII_COLOR_AMBER),
            (DawColor::LightBrown, APC_MKII_COLOR_AMBER_HI),
            (DawColor::LightGreen, APC_MKII_COLOR_LIME),
            (DawColor::BluishGreen, APC_MKII_COLOR_TURQUOISE_CYAN),
            (DawColor::GreenBlue, APC_MKII_COLOR_SPRING_HI),
            (DawColor::LightBlue, APC_MKII_COLOR_OCEAN_HI),
        ];
        for (color, index) in daw_colors {
            cm.register_color_index(color.id(), index);
        }

        for i in 0..128 {
            cm.register_color(i, rgb(ColorEx::BLACK));
        }
        for color in DawColor::ALL {
            if cm.has_color_index(color.id()) {
                let index = cm.color_index(color.id());
                cm.register_color(index, rgb(color.color()));
            }
        }
        cm.register_color(APC_MKII_COLOR_BLACK, rgb(ColorEx::BLACK));
        cm.register_color(APC_MKII_COLOR_WHITE, rgb(ColorEx::WHITE));
        cm.register_color(APC_MKII_COLOR_RED_HI, rgb(ColorEx::RED));
        cm.register_color(APC_MKII_COLOR_YELLOW_HI, rgb(ColorEx::YELLOW));
        cm.register_color(APC_MKII_COLOR_YELLOW_LO, rgb(ColorEx::even_darker(ColorEx::YELLOW)));
        cm.register_color(APC_MKII_COLOR_GREEN_HI, rgb(ColorEx::GREEN));
        cm.register_color(APC_MKII_COLOR_GREEN, rgb(ColorEx::darker(ColorEx::GREEN)));
        cm.register_color(APC_MKII_COLOR_GREEN_LO, rgb(ColorEx::even_darker(ColorEx::GREEN)));
        cm.register_color(APC_MKII_COLOR_BLUE_HI, rgb(ColorEx::brighter(ColorEx::BLUE)));
        cm.register_color(APC_MKII_COLOR_BLUE, rgb(ColorEx::BLUE));
        cm.register_color(APC_MKII_COLOR_BLUE_LO, rgb(ColorEx::even_darker(ColorEx::BLUE)));
    } else {
        cm.register_color(APC_COLOR_BLACK, rgb(ColorEx::BLACK));
        cm.register_color(APC_COLOR_GREEN, rgb(ColorEx::GREEN));
        cm.register_color(APC_COLOR_GREEN_BLINK, rgb(ColorEx::GREEN));
        cm.register_color(APC_COLOR_RED, rgb(ColorEx::RED));
        cm.register_color(APC_COLOR_RED_BLINK, rgb(ColorEx::RED));
        cm.register_color(APC_COLOR_YELLOW, rgb(ColorEx::YELLOW));
        cm.register_color(APC_COLOR_YELLOW_BLINK, rgb(ColorEx::YELLOW));
        for i in APC_COLOR_YELLOW_BLINK + 1..128 {
            cm.register_color(i, rgb(ColorEx::BLACK));
        }
    }
    // Note: the simulator colors of the buttons (former getColor (colorIndex, buttonID)) are
    // not ported
    cm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        let mk1 = create_color_manager(false);
        let mk2 = create_color_manager(true);
        assert_eq!(mk1.color_index(Scales::SCALE_COLOR_OCTAVE), APC_COLOR_YELLOW);
        assert_eq!(mk2.color_index(Scales::SCALE_COLOR_OCTAVE), APC_MKII_COLOR_OCEAN_HI);
        assert_eq!(mk2.color_index(BUTTON_STATE_BLINK), 3);
        assert_eq!(mk2.color_index(DawColor::LightBlue.id()), APC_MKII_COLOR_OCEAN_HI);
        assert!(!mk1.has_color_index(DawColor::LightBlue.id()));
        assert_eq!(mk1.color_index(COLOR_VIEW_UNSELECTED), APC_COLOR_BLACK);
    }
}
