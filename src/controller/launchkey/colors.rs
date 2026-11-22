// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the Launchkey (former `LaunchkeyMk3ColorManager` and
//! `LaunchkeyMiniMk3ColorManager`, which only differ in the registration of `BUTTON_COLOR_OFF`).
//!
//! Not ported: the RGB values of the palette and the `getColor` override (both only used by the
//! simulator GUI).

use crate::controller::push::colors::DEFAULT_PALETTE;
use crate::framework::color::{palette_from_rgb255, ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::*;
use crate::framework::feature::BUTTON_COLOR_OFF;
use crate::framework::grid::GRID_OFF;
use crate::scale::Scales;

pub const LAUNCHKEY_COLOR_BLACK: i32 = 0;
pub const LAUNCHKEY_COLOR_GREY_LO: i32 = 1;
pub const LAUNCHKEY_COLOR_GREY_MD: i32 = 2;
pub const LAUNCHKEY_COLOR_WHITE: i32 = 3;
pub const LAUNCHKEY_COLOR_ROSE: i32 = 4;
pub const LAUNCHKEY_COLOR_RED_HI: i32 = 5;
pub const LAUNCHKEY_COLOR_RED: i32 = 6;
pub const LAUNCHKEY_COLOR_RED_LO: i32 = 7;
pub const LAUNCHKEY_COLOR_RED_AMBER: i32 = 8;
pub const LAUNCHKEY_COLOR_AMBER_HI: i32 = 9;
pub const LAUNCHKEY_COLOR_AMBER: i32 = 10;
pub const LAUNCHKEY_COLOR_AMBER_LO: i32 = 11;
pub const LAUNCHKEY_COLOR_AMBER_YELLOW: i32 = 12;
pub const LAUNCHKEY_COLOR_YELLOW_HI: i32 = 13;
pub const LAUNCHKEY_COLOR_YELLOW: i32 = 14;
pub const LAUNCHKEY_COLOR_YELLOW_LO: i32 = 15;
pub const LAUNCHKEY_COLOR_YELLOW_LIME: i32 = 16;
pub const LAUNCHKEY_COLOR_LIME_HI: i32 = 17;
pub const LAUNCHKEY_COLOR_LIME: i32 = 18;
pub const LAUNCHKEY_COLOR_LIME_LO: i32 = 19;
pub const LAUNCHKEY_COLOR_LIME_GREEN: i32 = 20;
pub const LAUNCHKEY_COLOR_GREEN_HI: i32 = 21;
pub const LAUNCHKEY_COLOR_GREEN: i32 = 22;
pub const LAUNCHKEY_COLOR_GREEN_LO: i32 = 23;
pub const LAUNCHKEY_COLOR_GREEN_SPRING: i32 = 24;
pub const LAUNCHKEY_COLOR_SPRING_HI: i32 = 25;
pub const LAUNCHKEY_COLOR_SPRING: i32 = 26;
pub const LAUNCHKEY_COLOR_SPRING_LO: i32 = 27;
pub const LAUNCHKEY_COLOR_SPRING_TURQUOISE: i32 = 28;
pub const LAUNCHKEY_COLOR_TURQUOISE_LO: i32 = 29;
pub const LAUNCHKEY_COLOR_TURQUOISE: i32 = 30;
pub const LAUNCHKEY_COLOR_TURQUOISE_HI: i32 = 31;
pub const LAUNCHKEY_COLOR_TURQUOISE_CYAN: i32 = 32;
pub const LAUNCHKEY_COLOR_CYAN_HI: i32 = 33;
pub const LAUNCHKEY_COLOR_CYAN: i32 = 34;
pub const LAUNCHKEY_COLOR_CYAN_LO: i32 = 35;
pub const LAUNCHKEY_COLOR_CYAN_SKY: i32 = 36;
pub const LAUNCHKEY_COLOR_SKY_HI: i32 = 37;
pub const LAUNCHKEY_COLOR_SKY: i32 = 38;
pub const LAUNCHKEY_COLOR_SKY_LO: i32 = 39;
pub const LAUNCHKEY_COLOR_SKY_OCEAN: i32 = 40;
pub const LAUNCHKEY_COLOR_OCEAN_HI: i32 = 41;
pub const LAUNCHKEY_COLOR_OCEAN: i32 = 42;
pub const LAUNCHKEY_COLOR_OCEAN_LO: i32 = 43;
pub const LAUNCHKEY_COLOR_OCEAN_BLUE: i32 = 44;
pub const LAUNCHKEY_COLOR_BLUE_HI: i32 = 45;
pub const LAUNCHKEY_COLOR_BLUE: i32 = 46;
pub const LAUNCHKEY_COLOR_BLUE_LO: i32 = 47;
pub const LAUNCHKEY_COLOR_BLUE_ORCHID: i32 = 48;
pub const LAUNCHKEY_COLOR_ORCHID_HI: i32 = 49;
pub const LAUNCHKEY_COLOR_ORCHID: i32 = 50;
pub const LAUNCHKEY_COLOR_ORCHID_LO: i32 = 51;
pub const LAUNCHKEY_COLOR_ORCHID_MAGENTA: i32 = 52;
pub const LAUNCHKEY_COLOR_MAGENTA_HI: i32 = 53;
pub const LAUNCHKEY_COLOR_MAGENTA: i32 = 54;
pub const LAUNCHKEY_COLOR_MAGENTA_LO: i32 = 55;
pub const LAUNCHKEY_COLOR_MAGENTA_PINK: i32 = 56;
pub const LAUNCHKEY_COLOR_PINK_HI: i32 = 57;
pub const LAUNCHKEY_COLOR_PINK: i32 = 58;
pub const LAUNCHKEY_COLOR_PINK_LO: i32 = 59;
pub const LAUNCHKEY_COLOR_ORANGE: i32 = 60;

/// Create the color manager. The Mini does not register `BUTTON_COLOR_OFF` (as Java).
pub fn create_color_manager(is_mini: bool) -> ColorManager {
    let mut cm = ColorManager::new();
    if !is_mini {
        cm.register_color_index(BUTTON_COLOR_OFF, LAUNCHKEY_COLOR_BLACK);
    }
    cm.register_color_index(Scales::SCALE_COLOR_OFF, LAUNCHKEY_COLOR_BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, LAUNCHKEY_COLOR_OCEAN_HI);
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, LAUNCHKEY_COLOR_WHITE);
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, LAUNCHKEY_COLOR_BLACK);

    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, LAUNCHKEY_COLOR_GREEN_LO);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, LAUNCHKEY_COLOR_GREEN_HI);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED, LAUNCHKEY_COLOR_GREY_MD);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, LAUNCHKEY_COLOR_GREY_LO);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_SELECTED, LAUNCHKEY_COLOR_WHITE);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT, LAUNCHKEY_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT_4, LAUNCHKEY_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT, LAUNCHKEY_COLOR_BLUE_HI);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT_CONT, LAUNCHKEY_COLOR_BLUE_ORCHID);
    cm.register_color_index(abstract_sequencer_view::COLOR_PAGE, LAUNCHKEY_COLOR_WHITE);
    cm.register_color_index(abstract_sequencer_view::COLOR_ACTIVE_PAGE, LAUNCHKEY_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_SELECTED_PAGE, LAUNCHKEY_COLOR_BLUE_ORCHID);
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION_OFF, LAUNCHKEY_COLOR_BLACK);

    cm.register_color_index(abstract_drum_view::COLOR_PAD_OFF, LAUNCHKEY_COLOR_BLACK);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_RECORD, LAUNCHKEY_COLOR_RED_HI);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_PLAY, LAUNCHKEY_COLOR_GREEN_HI);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_SELECTED, LAUNCHKEY_COLOR_BLUE_HI);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_MUTED, LAUNCHKEY_COLOR_AMBER_LO);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT, LAUNCHKEY_COLOR_YELLOW_HI);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_NO_CONTENT, LAUNCHKEY_COLOR_YELLOW_LO);

    cm.register_color_index(abstract_play_view::COLOR_PLAY, LAUNCHKEY_COLOR_GREEN_HI);
    cm.register_color_index(abstract_play_view::COLOR_RECORD, LAUNCHKEY_COLOR_RED_HI);
    cm.register_color_index(abstract_play_view::COLOR_OFF, LAUNCHKEY_COLOR_BLACK);

    cm.register_color_index(abstract_session_view::COLOR_SCENE, LAUNCHKEY_COLOR_GREEN_LO);
    cm.register_color_index(abstract_session_view::COLOR_SELECTED_SCENE, LAUNCHKEY_COLOR_GREEN_HI);
    cm.register_color_index(abstract_session_view::COLOR_SCENE_OFF, LAUNCHKEY_COLOR_BLACK);

    cm.register_color_index(GRID_OFF, LAUNCHKEY_COLOR_BLACK);

    cm.register_color_index("COLOR_OFF", LAUNCHKEY_COLOR_BLACK);
    cm.register_color_index("DAW_COLOR_GRAY_HALF", LAUNCHKEY_COLOR_GREY_MD);
    cm.register_color_index("DAW_COLOR_DARK_GRAY", LAUNCHKEY_COLOR_GREY_LO);
    cm.register_color_index("DAW_COLOR_GRAY", LAUNCHKEY_COLOR_GREY_MD);
    cm.register_color_index("DAW_COLOR_LIGHT_GRAY", LAUNCHKEY_COLOR_GREY_LO);
    cm.register_color_index("DAW_COLOR_SILVER", LAUNCHKEY_COLOR_SKY_OCEAN);
    cm.register_color_index("DAW_COLOR_DARK_BROWN", LAUNCHKEY_COLOR_AMBER_LO);
    cm.register_color_index("DAW_COLOR_BROWN", LAUNCHKEY_COLOR_AMBER_YELLOW);
    cm.register_color_index("DAW_COLOR_DARK_BLUE", LAUNCHKEY_COLOR_OCEAN);
    cm.register_color_index("DAW_COLOR_PURPLE_BLUE", LAUNCHKEY_COLOR_OCEAN_BLUE);
    cm.register_color_index("DAW_COLOR_PURPLE", LAUNCHKEY_COLOR_ORCHID_HI);
    cm.register_color_index("DAW_COLOR_PINK", LAUNCHKEY_COLOR_PINK_HI);
    cm.register_color_index("DAW_COLOR_RED", LAUNCHKEY_COLOR_RED);
    cm.register_color_index("DAW_COLOR_ORANGE", LAUNCHKEY_COLOR_ORANGE);
    cm.register_color_index("DAW_COLOR_LIGHT_ORANGE", LAUNCHKEY_COLOR_AMBER_HI);
    cm.register_color_index("DAW_COLOR_MOSS_GREEN", LAUNCHKEY_COLOR_LIME_LO);
    cm.register_color_index("DAW_COLOR_GREEN", LAUNCHKEY_COLOR_SPRING);
    cm.register_color_index("DAW_COLOR_COLD_GREEN", LAUNCHKEY_COLOR_TURQUOISE);
    cm.register_color_index("DAW_COLOR_BLUE", LAUNCHKEY_COLOR_SKY_HI);
    cm.register_color_index("DAW_COLOR_LIGHT_PURPLE", LAUNCHKEY_COLOR_BLUE_ORCHID);
    cm.register_color_index("DAW_COLOR_LIGHT_PINK", LAUNCHKEY_COLOR_MAGENTA_PINK);
    cm.register_color_index("DAW_COLOR_ROSE", LAUNCHKEY_COLOR_ROSE);
    cm.register_color_index("DAW_COLOR_REDDISH_BROWN", LAUNCHKEY_COLOR_AMBER);
    cm.register_color_index("DAW_COLOR_LIGHT_BROWN", LAUNCHKEY_COLOR_AMBER_YELLOW);
    cm.register_color_index("DAW_COLOR_LIGHT_GREEN", LAUNCHKEY_COLOR_LIME);
    cm.register_color_index("DAW_COLOR_BLUISH_GREEN", LAUNCHKEY_COLOR_SPRING_HI);
    cm.register_color_index("DAW_COLOR_GREEN_BLUE", LAUNCHKEY_COLOR_TURQUOISE_CYAN);
    cm.register_color_index("DAW_COLOR_LIGHT_BLUE", LAUNCHKEY_COLOR_OCEAN_HI);

    cm.register_color_index(BUTTON_STATE_OFF, 0);
    cm.register_color_index(BUTTON_STATE_ON, 1);
    cm.register_color_index(BUTTON_STATE_HI, 127);
    // The direct path (not Bitwig): Novation's palette of the LED velocities
    cm.set_palette_rgb(&palette_from_rgb255(&DEFAULT_PALETTE));
    cm
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daw::color::DawColor;

    #[test]
    fn color_ids() {
        let cm = create_color_manager(false);
        assert_eq!(cm.color_index(BUTTON_STATE_ON), 1);
        assert_eq!(cm.color_index(BUTTON_STATE_HI), 127);
        assert_eq!(cm.color_index(abstract_session_view::COLOR_SELECTED_SCENE), LAUNCHKEY_COLOR_GREEN_HI);
        assert!(cm.has_color_index(BUTTON_COLOR_OFF));
        // All DAW colors are registered
        for color in DawColor::ALL {
            assert!(cm.has_color_index(color.id()), "{}", color.id());
        }
        assert!(!create_color_manager(true).has_color_index(BUTTON_COLOR_OFF));
    }
}
