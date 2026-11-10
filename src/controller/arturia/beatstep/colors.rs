// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the pads of the Beatstep (former `BeatstepColorManager`).

use crate::daw::color::ColorEx;
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::grid::GRID_OFF;
use crate::scale::Scales;

pub const BEATSTEP_BUTTON_STATE_INVALID: i32 = -1;
pub const BEATSTEP_BUTTON_STATE_OFF: i32 = 0;
pub const BEATSTEP_BUTTON_STATE_RED: i32 = 1;
pub const BEATSTEP_BUTTON_STATE_BLUE: i32 = 16;
pub const BEATSTEP_BUTTON_STATE_PINK: i32 = 17;

/// Create the color manager.
pub fn create_color_manager() -> ColorManager {
    let mut cm = ColorManager::new();
    cm.register_color_index(Scales::SCALE_COLOR_OFF, BEATSTEP_BUTTON_STATE_OFF);
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, BEATSTEP_BUTTON_STATE_RED);
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, BEATSTEP_BUTTON_STATE_BLUE);
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, BEATSTEP_BUTTON_STATE_OFF);

    cm.register_color_index(GRID_OFF, BEATSTEP_BUTTON_STATE_OFF);
    cm.register_color_index(BUTTON_STATE_OFF, BEATSTEP_BUTTON_STATE_OFF);
    cm.register_color_index(BUTTON_STATE_ON, BEATSTEP_BUTTON_STATE_OFF);
    cm.register_color_index(BUTTON_STATE_HI, BEATSTEP_BUTTON_STATE_BLUE);

    for (index, color) in [
        (BEATSTEP_BUTTON_STATE_OFF, ColorEx::BLACK),
        (BEATSTEP_BUTTON_STATE_RED, ColorEx::RED),
        (BEATSTEP_BUTTON_STATE_BLUE, ColorEx::BLUE),
        (BEATSTEP_BUTTON_STATE_PINK, ColorEx::PINK),
    ] {
        let [red, green, blue] = color.to_double_rgb();
        cm.register_color(index, (red, green, blue));
    }
    cm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_indices() {
        let cm = create_color_manager();
        assert_eq!(cm.color_index(Scales::SCALE_COLOR_NOTE), BEATSTEP_BUTTON_STATE_BLUE);
        assert_eq!(cm.color_index(Scales::SCALE_COLOR_OCTAVE), BEATSTEP_BUTTON_STATE_RED);
        assert_eq!(cm.color_index(BUTTON_STATE_ON), BEATSTEP_BUTTON_STATE_OFF);
        assert_eq!(cm.color_index(BUTTON_STATE_HI), BEATSTEP_BUTTON_STATE_BLUE);
        assert_eq!(cm.color(BEATSTEP_BUTTON_STATE_RED), (1.0, 0.0, 0.0));
    }
}
