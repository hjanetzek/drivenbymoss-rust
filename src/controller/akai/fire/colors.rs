// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the Akai Fire (former `FireColorManager`).
//!
//! The pads are RGB: the color indices are mapped to RGB values, the DAW colors start at index
//! 100 (former `DAW_COLOR_START`, registered as colors instead of the override of `getColor`).
//! The buttons have up to 4 brightness states, the light state is sent directly as the value
//! (0 off, 1 and 2 dim colors, 3 and 4 bright colors).

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::*;
use crate::framework::feature::BUTTON_COLOR_OFF;
use crate::framework::grid::GRID_OFF;
use crate::scale::Scales;

pub const BUTTON_STATE_ON2: &str = "BUTTON_STATE_ON2";
pub const BUTTON_STATE_HI2: &str = "BUTTON_STATE_HI2";

const DAW_COLOR_START: i32 = 100;
const SCALE_COLOR_OCTAVE: i32 = 50;
const SCALE_COLOR_NOTE: i32 = 51;
const SCALE_COLOR_OUT_OF_SCALE: i32 = 52;

pub const FIRE_COLOR_BLACK: i32 = 0;
pub const FIRE_COLOR_WHITE: i32 = 59;
pub const FIRE_COLOR_GREEN: i32 = 60;
pub const FIRE_COLOR_DARK_GREEN: i32 = 61;
pub const FIRE_COLOR_RED: i32 = 62;
pub const FIRE_COLOR_DARK_RED: i32 = 63;
pub const FIRE_COLOR_DARKER_RED: i32 = 64;
pub const FIRE_COLOR_BLUE: i32 = 65;
pub const FIRE_COLOR_DARK_BLUE: i32 = 66;
pub const FIRE_COLOR_DARK_OCEAN: i32 = 67;
pub const FIRE_COLOR_ORANGE: i32 = 68;
pub const FIRE_COLOR_DARK_ORANGE: i32 = 69;
pub const FIRE_COLOR_DARKER_ORANGE: i32 = 70;
pub const FIRE_COLOR_YELLOW: i32 = 71;
pub const FIRE_COLOR_DARK_YELLOW: i32 = 72;
pub const FIRE_COLOR_DARKER_YELLOW: i32 = 73;
pub const FIRE_COLOR_BROWN: i32 = 74;
pub const FIRE_COLOR_GRAY: i32 = 75;

fn rgb(c: ColorEx) -> (f64, f64, f64) {
    (c.red(), c.green(), c.blue())
}

/// Create the color manager (former `FireColorManager`).
pub fn create_color_manager() -> ColorManager {
    let mut cm = ColorManager::new();
    cm.register_color_index(Scales::SCALE_COLOR_OFF, FIRE_COLOR_BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, SCALE_COLOR_OCTAVE);
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, SCALE_COLOR_NOTE);
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, SCALE_COLOR_OUT_OF_SCALE);
    cm.register_color_index(GRID_OFF, FIRE_COLOR_BLACK);

    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, FIRE_COLOR_DARK_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, FIRE_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED, FIRE_COLOR_GRAY);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, FIRE_COLOR_GRAY);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_SELECTED, FIRE_COLOR_DARKER_YELLOW);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT, FIRE_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT_4, FIRE_COLOR_GRAY);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT, FIRE_COLOR_BLUE);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT_CONT, FIRE_COLOR_DARK_BLUE);
    cm.register_color_index(abstract_sequencer_view::COLOR_PAGE, FIRE_COLOR_GRAY);
    cm.register_color_index(abstract_sequencer_view::COLOR_ACTIVE_PAGE, FIRE_COLOR_RED);
    cm.register_color_index(abstract_sequencer_view::COLOR_SELECTED_PAGE, FIRE_COLOR_DARKER_RED);
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION, FIRE_COLOR_DARK_ORANGE);
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION_SELECTED, FIRE_COLOR_ORANGE);
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION_OFF, FIRE_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_TRANSPOSE, FIRE_COLOR_WHITE);
    cm.register_color_index(abstract_sequencer_view::COLOR_TRANSPOSE_SELECTED, FIRE_COLOR_YELLOW);

    cm.register_color_index(abstract_drum_view::COLOR_PAD_OFF, FIRE_COLOR_BLACK);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_RECORD, FIRE_COLOR_RED);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_PLAY, FIRE_COLOR_GREEN);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_SELECTED, FIRE_COLOR_BLUE);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_MUTED, FIRE_COLOR_BROWN);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT, FIRE_COLOR_YELLOW);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_NO_CONTENT, FIRE_COLOR_DARK_YELLOW);

    cm.register_color_index(abstract_play_view::COLOR_OFF, FIRE_COLOR_BLACK);
    cm.register_color_index(abstract_play_view::COLOR_PLAY, FIRE_COLOR_GREEN);
    cm.register_color_index(abstract_play_view::COLOR_RECORD, FIRE_COLOR_RED);

    cm.register_color_index(abstract_session_view::COLOR_SCENE, FIRE_COLOR_DARK_GREEN);
    cm.register_color_index(abstract_session_view::COLOR_SELECTED_SCENE, FIRE_COLOR_GREEN);
    cm.register_color_index(abstract_session_view::COLOR_SCENE_OFF, FIRE_COLOR_BLACK);

    cm.register_color_index(BUTTON_COLOR_OFF, 0);
    cm.register_color_index(BUTTON_STATE_OFF, 0);
    cm.register_color_index(BUTTON_STATE_ON, 1);
    cm.register_color_index(BUTTON_STATE_HI, 3);
    cm.register_color_index(BUTTON_STATE_ON2, 2);
    cm.register_color_index(BUTTON_STATE_HI2, 4);

    for i in 0..128 {
        cm.register_color(i, rgb(ColorEx::BLACK));
    }
    cm.register_color(FIRE_COLOR_BLACK, rgb(ColorEx::BLACK));
    cm.register_color(FIRE_COLOR_WHITE, rgb(ColorEx::WHITE));
    cm.register_color(FIRE_COLOR_GREEN, rgb(ColorEx::GREEN));
    cm.register_color(FIRE_COLOR_DARK_GREEN, rgb(ColorEx::DARK_GREEN));
    cm.register_color(FIRE_COLOR_RED, rgb(ColorEx::RED));
    cm.register_color(FIRE_COLOR_DARK_RED, rgb(ColorEx::DARK_RED));
    cm.register_color(FIRE_COLOR_DARKER_RED, rgb(ColorEx::from_rgb(20, 0, 0)));
    cm.register_color(FIRE_COLOR_BLUE, rgb(ColorEx::BLUE));
    cm.register_color(FIRE_COLOR_DARK_BLUE, rgb(ColorEx::DARK_BLUE));
    cm.register_color(FIRE_COLOR_DARK_OCEAN, rgb(ColorEx::SKY_BLUE));
    cm.register_color(FIRE_COLOR_ORANGE, rgb(ColorEx::ORANGE));
    cm.register_color(FIRE_COLOR_DARK_ORANGE, rgb(ColorEx::DARK_ORANGE));
    cm.register_color(FIRE_COLOR_DARKER_ORANGE, rgb(ColorEx::from_rgb(32, 14, 0)));
    cm.register_color(FIRE_COLOR_DARK_YELLOW, rgb(ColorEx::DARK_YELLOW));
    cm.register_color(FIRE_COLOR_YELLOW, rgb(ColorEx::YELLOW));
    cm.register_color(FIRE_COLOR_DARKER_YELLOW, rgb(ColorEx::from_rgb(20, 20, 0)));
    cm.register_color(FIRE_COLOR_BROWN, rgb(ColorEx::BROWN));
    cm.register_color(FIRE_COLOR_GRAY, rgb(ColorEx::GRAY));
    cm.register_color(SCALE_COLOR_OCTAVE, rgb(ColorEx::BLUE));
    cm.register_color(SCALE_COLOR_NOTE, rgb(ColorEx::WHITE));
    cm.register_color(SCALE_COLOR_OUT_OF_SCALE, rgb(ColorEx::DARK_GRAY));

    for (i, color) in DawColor::ALL.iter().enumerate() {
        let index = DAW_COLOR_START + i as i32;
        cm.register_color_index(color.id(), index);
        cm.register_color(index, rgb(color.color()));
    }
    cm
}

/// The color of a color index of a pad (former `getColor (colorIndex, PADx)`).
pub fn pad_color(cm: &ColorManager, color_index: i32) -> ColorEx {
    if !(0..128).contains(&color_index) {
        return ColorEx::BLACK;
    }
    let (r, g, b) = cm.color(color_index);
    ColorEx::new(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        let cm = create_color_manager();
        assert_eq!(cm.color_index(BUTTON_STATE_HI2), 4);
        assert_eq!(pad_color(&cm, FIRE_COLOR_RED), ColorEx::RED);
        assert_eq!(pad_color(&cm, cm.color_index(DawColor::ALL[0].id())), DawColor::ALL[0].color());
        assert_eq!(pad_color(&cm, -1), ColorEx::BLACK);
    }
}
