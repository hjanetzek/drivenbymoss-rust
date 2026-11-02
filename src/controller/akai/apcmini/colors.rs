// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the APC mini (former `APCminiMk1ColorManager` and `APCminiMk2ColorManager`).

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::*;
use crate::framework::grid::GRID_OFF;
use crate::scale::Scales;

/// The colors of the Mk1 (former `APCminiMk1ColorManager`).
pub mod mk1 {
    /// Off.
    pub const APC_COLOR_BLACK: i32 = 0;
    /// Green, 7-127 also green.
    pub const APC_COLOR_GREEN: i32 = 1;
    pub const APC_COLOR_GREEN_BLINK: i32 = 2;
    pub const APC_COLOR_RED: i32 = 3;
    pub const APC_COLOR_RED_BLINK: i32 = 4;
    pub const APC_COLOR_YELLOW: i32 = 5;
    pub const APC_COLOR_YELLOW_BLINK: i32 = 6;
}

/// The colors of the Mk2 (former `APCminiMk2ColorManager`).
#[allow(dead_code)]
pub mod mk2 {
    pub(super) const SIMPLE_OFF: i32 = 0;
    pub(super) const SIMPLE_ON: i32 = 1;
    pub(super) const SIMPLE_BLINK: i32 = 2;

    pub const BLACK: i32 = 0;
    pub const DARKEST_GRAY: i32 = 1;
    pub const GRAY: i32 = 2;
    pub const WHITE: i32 = 3;
    pub const LIGHT_RED: i32 = 4;
    pub const RED: i32 = 5;
    pub const DARK_RED: i32 = 6;
    pub const DEEP_RED: i32 = 7;
    pub const PEACH: i32 = 8;
    pub const ORANGE: i32 = 9;
    pub const BROWN: i32 = 10;
    pub const DARK_BROWN: i32 = 11;
    pub const YELLOW: i32 = 12;
    pub const GOLD: i32 = 13;
    pub const OLIVE: i32 = 14;
    pub const DARK_GREEN: i32 = 15;
    pub const LIGHT_GREEN: i32 = 16;
    pub const BRIGHT_GREEN: i32 = 17;
    pub const FOREST_GREEN: i32 = 18;
    pub const DEEP_GREEN: i32 = 19;
    pub const LIGHTER_GREEN: i32 = 20;
    pub const GREEN: i32 = 21;
    pub const DARK_GREEN_2: i32 = 22;
    pub const DEEP_GREEN_2: i32 = 23;
    pub const LIGHT_TEAL: i32 = 24;
    pub const TURQUOISE: i32 = 25;
    pub const DARK_TEAL: i32 = 26;
    pub const DEEP_TEAL: i32 = 27;
    pub const LIGHT_CYAN: i32 = 28;
    pub const CYAN: i32 = 29;
    pub const DARK_CYAN: i32 = 30;
    pub const DEEP_CYAN: i32 = 31;
    pub const LIGHT_BLUE: i32 = 32;
    pub const BLUE: i32 = 33;
    pub const DARK_BLUE: i32 = 34;
    pub const DEEP_BLUE: i32 = 35;
    pub const SKY_BLUE: i32 = 36;
    pub const LIGHT_SKY_BLUE: i32 = 37;
    pub const DARK_SKY_BLUE: i32 = 38;
    pub const NAVY: i32 = 39;
    pub const MEDIUM_BLUE: i32 = 40;
    pub const BRIGHT_BLUE: i32 = 41;
    pub const DEEP_BLUE_2: i32 = 42;
    pub const BLACK_2: i32 = 43;
    pub const PURPLE: i32 = 44;
    pub const DARK_PURPLE: i32 = 45;
    pub const DEEP_PURPLE: i32 = 46;
    pub const LIGHT_VIOLET: i32 = 47;
    pub const MAGENTA: i32 = 48;
    pub const FUCHSIA: i32 = 49;
    pub const DEEP_MAGENTA: i32 = 50;
    pub const DARK_MAGENTA: i32 = 51;
    pub const LIGHT_PINK: i32 = 52;
    pub const PINK: i32 = 53;
    pub const DEEP_PINK: i32 = 54;
    pub const DARK_PINK: i32 = 55;
    pub const RED_PINK: i32 = 56;
    pub const HOT_PINK: i32 = 57;
    pub const BROWN_2: i32 = 58;
    pub const DARK_BROWN_2: i32 = 59;
    pub const LIGHT_ORANGE: i32 = 60;
    pub const BROWN_3: i32 = 61;
    pub const DARK_BROWN_3: i32 = 62;
    pub const OLIVE_2: i32 = 63;
    pub const DARK_GREEN_3: i32 = 64;
    pub const LIGHT_GREEN_2: i32 = 65;
    pub const TEAL: i32 = 66;
    pub const LIGHT_GRAY: i32 = 67;
    pub const GRAY_2: i32 = 68;
    pub const LIGHT_GRAY_2: i32 = 69;
    pub const GRAY_3: i32 = 70;
    pub const DARK_GRAY: i32 = 71;
    pub const BRIGHT_RED: i32 = 72;
    pub const BRIGHT_GREEN_2: i32 = 73;
    pub const BRIGHT_YELLOW: i32 = 74;
    pub const BRIGHT_LIME: i32 = 75;
    pub const BRIGHT_TEAL: i32 = 76;
    pub const BRIGHT_CYAN: i32 = 77;
    pub const LIGHT_BLUE_2: i32 = 78;
    pub const DEEP_BLUE_3: i32 = 79;
    pub const MEDIUM_BLUE_2: i32 = 80;
    pub const VIOLET: i32 = 81;
    pub const MAGENTA_2: i32 = 82;
    pub const DARK_ORANGE: i32 = 83;
    pub const ORANGE_2: i32 = 84;
    pub const LIGHT_ORANGE_2: i32 = 85;
    pub const SOFT_GREEN: i32 = 86;
    pub const FRESH_GREEN: i32 = 87;
    pub const SOFT_YELLOW: i32 = 88;
    pub const LIGHT_YELLOW: i32 = 89;
    pub const LIGHT_GOLD: i32 = 90;
    pub const PASTEL_GREEN: i32 = 91;
    pub const PASTEL_BLUE: i32 = 92;
    pub const PASTEL_VIOLET: i32 = 93;
    pub const PASTEL_PINK: i32 = 94;
    pub const SOFT_RED: i32 = 95;
    pub const DEEP_ORANGE: i32 = 96;
    pub const SOFT_BROWN: i32 = 97;
    pub const LIGHT_BROWN: i32 = 98;
    pub const SOFT_GRAY: i32 = 99;
    pub const LIGHT_GRAY_4: i32 = 100;
    pub const DEEP_GRAY: i32 = 101;
    pub const DARK_GRAY_2: i32 = 102;
    pub const MEDIUM_GRAY: i32 = 103;
    pub const DARK_GRAY_3: i32 = 104;
    pub const RICH_BROWN: i32 = 105;
    pub const BRIGHT_RED_2: i32 = 106;
    pub const VIBRANT_RED: i32 = 107;
    pub const RICH_ORANGE: i32 = 108;
    pub const RICH_YELLOW: i32 = 109;
    pub const FRESH_YELLOW: i32 = 110;
    pub const LUSH_GREEN: i32 = 111;
    pub const LIGHT_TEAL_2: i32 = 112;
    pub const SOFT_BLUE: i32 = 113;
    pub const FRESH_BLUE: i32 = 114;
    pub const LIGHT_PURPLE: i32 = 115;
    pub const LIGHT_VIOLET_2: i32 = 116;
    pub const BRIGHT_VIOLET: i32 = 117;
    pub const MID_GRAY: i32 = 118;
    pub const LIGHT_MAGENTA: i32 = 119;
    pub const DEEP_MAGENTA_2: i32 = 120;
    pub const DARK_MAGENTA_2: i32 = 121;
    pub const LIGHT_PINK_2: i32 = 122;
    pub const SOFT_PINK: i32 = 123;
    pub const LIGHT_RED_2: i32 = 124;
    pub const LIGHT_MAROON: i32 = 125;
    pub const LIGHT_BROWN_2: i32 = 126;
    pub const DARK_BROWN_4: i32 = 127;
}

fn rgb(c: ColorEx) -> (f64, f64, f64) {
    (c.red(), c.green(), c.blue())
}

/// Create the color manager of the Mk1 (former `APCminiMk1ColorManager`).
pub fn create_mk1_color_manager() -> ColorManager {
    use mk1::*;
    let mut cm = ColorManager::new();

    cm.register_color_index(Scales::SCALE_COLOR_OFF, APC_COLOR_BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, APC_COLOR_YELLOW);
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, APC_COLOR_BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, APC_COLOR_BLACK);

    cm.register_color_index(BUTTON_STATE_OFF, APC_COLOR_BLACK);
    cm.register_color_index(BUTTON_STATE_ON, APC_COLOR_GREEN);
    cm.register_color_index(BUTTON_STATE_HI, APC_COLOR_GREEN_BLINK);

    cm.register_color_index(abstract_session_view::COLOR_SCENE, APC_COLOR_GREEN);
    cm.register_color_index(abstract_session_view::COLOR_SELECTED_SCENE, APC_COLOR_GREEN_BLINK);
    cm.register_color_index(abstract_session_view::COLOR_SCENE_OFF, APC_COLOR_BLACK);

    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, APC_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, APC_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED, APC_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, APC_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_SELECTED, APC_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT, APC_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT_4, APC_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT, APC_COLOR_RED);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT_CONT, APC_COLOR_RED);
    cm.register_color_index(abstract_sequencer_view::COLOR_PAGE, APC_COLOR_YELLOW);
    cm.register_color_index(abstract_sequencer_view::COLOR_ACTIVE_PAGE, APC_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_SELECTED_PAGE, APC_COLOR_RED);

    cm.register_color_index(abstract_drum_view::COLOR_PAD_OFF, APC_COLOR_BLACK);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_RECORD, APC_COLOR_RED);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_PLAY, APC_COLOR_GREEN);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_SELECTED, APC_COLOR_YELLOW_BLINK);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_MUTED, APC_COLOR_YELLOW);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT, APC_COLOR_YELLOW);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_NO_CONTENT, APC_COLOR_BLACK);

    cm.register_color_index(abstract_play_view::COLOR_PLAY, APC_COLOR_GREEN);
    cm.register_color_index(abstract_play_view::COLOR_RECORD, APC_COLOR_RED);
    cm.register_color_index(abstract_play_view::COLOR_OFF, APC_COLOR_BLACK);

    register_browser_colors(&mut cm, APC_COLOR_BLACK, APC_COLOR_RED_BLINK, APC_COLOR_GREEN_BLINK, APC_COLOR_YELLOW, APC_COLOR_GREEN, APC_COLOR_RED);

    cm.register_color_index(GRID_OFF, APC_COLOR_BLACK);

    for i in 0..128 {
        cm.register_color(i, rgb(ColorEx::BLACK));
    }
    cm.register_color(APC_COLOR_BLACK, rgb(ColorEx::BLACK));
    cm.register_color(APC_COLOR_GREEN, rgb(ColorEx::GREEN));
    cm.register_color(APC_COLOR_GREEN_BLINK, rgb(ColorEx::GREEN));
    cm.register_color(APC_COLOR_RED, rgb(ColorEx::RED));
    cm.register_color(APC_COLOR_RED_BLINK, rgb(ColorEx::RED));
    cm.register_color(APC_COLOR_YELLOW, rgb(ColorEx::YELLOW));
    cm.register_color(APC_COLOR_YELLOW_BLINK, rgb(ColorEx::YELLOW));
    cm
}

fn register_browser_colors(cm: &mut ColorManager, off: i32, discard: i32, confirm: i32, play: i32, green: i32, red: i32) {
    cm.register_color_index(browser_view::OFF, off);
    cm.register_color_index(browser_view::DISCARD, discard);
    cm.register_color_index(browser_view::CONFIRM, confirm);
    cm.register_color_index(browser_view::PLAY, play);
    cm.register_color_index(browser_view::COLUMN1, green);
    cm.register_color_index(browser_view::COLUMN2, red);
    cm.register_color_index(browser_view::COLUMN3, green);
    cm.register_color_index(browser_view::COLUMN4, red);
    cm.register_color_index(browser_view::COLUMN5, green);
    cm.register_color_index(browser_view::COLUMN6, red);
    cm.register_color_index(browser_view::COLUMN7, off);
    cm.register_color_index(browser_view::COLUMN8, play);
}

/// Create the color manager of the Mk2 (former `APCminiMk2ColorManager`).
pub fn create_mk2_color_manager() -> ColorManager {
    use mk2::*;
    let mut cm = ColorManager::new();

    cm.register_color_index(BUTTON_STATE_OFF, SIMPLE_OFF);
    cm.register_color_index(BUTTON_STATE_ON, SIMPLE_ON);
    cm.register_color_index(BUTTON_STATE_HI, SIMPLE_BLINK);

    cm.register_color_index(abstract_session_view::COLOR_SCENE_OFF, SIMPLE_OFF);
    cm.register_color_index(abstract_session_view::COLOR_SCENE, SIMPLE_ON);
    cm.register_color_index(abstract_session_view::COLOR_SELECTED_SCENE, SIMPLE_BLINK);

    cm.register_color_index(Scales::SCALE_COLOR_OFF, BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, YELLOW);
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, BLACK);

    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT, BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT_4, BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT, BLUE);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT_CONT, LIGHT_BLUE);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, BRIGHT_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED, GRAY);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, DARK_GRAY);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_SELECTED, YELLOW);
    cm.register_color_index(abstract_sequencer_view::COLOR_PAGE, YELLOW);
    cm.register_color_index(abstract_sequencer_view::COLOR_ACTIVE_PAGE, GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_SELECTED_PAGE, RED);

    cm.register_color_index(abstract_drum_view::COLOR_PAD_OFF, BLACK);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_RECORD, RED);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_PLAY, GREEN);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_SELECTED, BLUE);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_MUTED, DARK_ORANGE);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT, LIGHT_YELLOW);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_NO_CONTENT, BLACK);

    cm.register_color_index(abstract_play_view::COLOR_PLAY, GREEN);
    cm.register_color_index(abstract_play_view::COLOR_RECORD, RED);
    cm.register_color_index(abstract_play_view::COLOR_OFF, BLACK);

    register_browser_colors(&mut cm, BLACK, RED, GREEN, ORANGE, GREEN, RED);

    cm.register_color_index(GRID_OFF, BLACK);

    cm.register_color_index(DawColor::GrayHalf.id(), DARKEST_GRAY);
    cm.register_color_index(DawColor::DarkGray.id(), DARK_GRAY);
    cm.register_color_index(DawColor::Gray.id(), GRAY);
    cm.register_color_index(DawColor::LightGray.id(), LIGHT_BLUE);
    cm.register_color_index(DawColor::Silver.id(), MID_GRAY);
    cm.register_color_index(DawColor::DarkBrown.id(), DARK_BROWN);
    cm.register_color_index(DawColor::Brown.id(), BROWN);
    cm.register_color_index(DawColor::DarkBlue.id(), DARK_BLUE);
    cm.register_color_index(DawColor::PurpleBlue.id(), LIGHT_PURPLE);
    cm.register_color_index(DawColor::Purple.id(), PURPLE);
    cm.register_color_index(DawColor::Pink.id(), PINK);
    cm.register_color_index(DawColor::Red.id(), RED);
    cm.register_color_index(DawColor::Orange.id(), LIGHT_ORANGE);
    cm.register_color_index(DawColor::LightOrange.id(), ORANGE);
    cm.register_color_index(DawColor::MossGreen.id(), LIGHT_GREEN);
    cm.register_color_index(DawColor::Green.id(), GREEN);
    cm.register_color_index(DawColor::ColdGreen.id(), TURQUOISE);
    cm.register_color_index(DawColor::Blue.id(), BLUE);
    cm.register_color_index(DawColor::LightPurple.id(), LIGHT_PURPLE);
    cm.register_color_index(DawColor::LightPink.id(), LIGHT_PINK);
    cm.register_color_index(DawColor::Rose.id(), SOFT_RED);
    cm.register_color_index(DawColor::ReddishBrown.id(), RICH_BROWN);
    cm.register_color_index(DawColor::LightBrown.id(), LIGHT_BROWN);
    cm.register_color_index(DawColor::LightGreen.id(), PASTEL_BLUE);
    cm.register_color_index(DawColor::BluishGreen.id(), CYAN);
    cm.register_color_index(DawColor::GreenBlue.id(), FOREST_GREEN);
    cm.register_color_index(DawColor::LightBlue.id(), LIGHT_GRAY);

    // Register colors
    for i in 0..128 {
        cm.register_color(i, rgb(ColorEx::BLACK));
    }
    cm.register_color_index(DawColor::ColorOff.id(), BLACK);
    for color in DawColor::ALL {
        if cm.has_color_index(color.id()) {
            let index = cm.color_index(color.id());
            cm.register_color(index, rgb(color.color()));
        }
    }
    cm.register_color(BLACK, rgb(ColorEx::BLACK));
    cm.register_color(WHITE, rgb(ColorEx::WHITE));
    cm.register_color(RED, rgb(ColorEx::RED));
    cm.register_color(YELLOW, rgb(ColorEx::YELLOW));
    cm.register_color(LIGHT_YELLOW, rgb(ColorEx::brighter(ColorEx::YELLOW)));
    cm.register_color(DARK_ORANGE, rgb(ColorEx::DARK_ORANGE));
    cm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_managers() {
        let mk1 = create_mk1_color_manager();
        assert_eq!(mk1.color_index(BUTTON_STATE_HI), mk1::APC_COLOR_GREEN_BLINK);
        assert_eq!(mk1.color_index(abstract_drum_view::COLOR_PAD_SELECTED), mk1::APC_COLOR_YELLOW_BLINK);
        assert_eq!(mk1.color_index(browser_view::COLUMN8), mk1::APC_COLOR_YELLOW);

        let mk2 = create_mk2_color_manager();
        assert_eq!(mk2.color_index(abstract_session_view::COLOR_SELECTED_SCENE), 2);
        assert_eq!(mk2.color_index(DawColor::LightBlue.id()), mk2::LIGHT_GRAY);
        assert_eq!(mk2.color_index(DawColor::ColorOff.id()), mk2::BLACK);
        assert_eq!(mk2.color(mk2::RED), (1.0, 0.0, 0.0));
    }
}
