// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the OSC controller (former `OSCColorManager`): the scale colors and the DAW
//! colors; the virtual keyboard sends them as RGB.

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::ColorManager;
use crate::scale::Scales;

pub const BLACK: i32 = 0;
pub const SKY_BLUE: i32 = 1;
pub const WHITE: i32 = 2;
pub const DARK_GRAY: i32 = 3;

/// The index of the first DAW color after DARK_GRAY (GRAY = 4 ... LIGHT_BLUE = 29): the DAW
/// colors keep their order.
const FIRST_DAW_COLOR: i32 = 3;

fn rgb(color: ColorEx) -> (f64, f64, f64) {
    (color.red(), color.green(), color.blue())
}

/// Create the color manager (former `OSCColorManager`).
pub fn create_color_manager() -> ColorManager {
    let mut cm = ColorManager::new();
    cm.register_color_index(Scales::SCALE_COLOR_OFF, BLACK);
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, SKY_BLUE);
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, WHITE);
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, BLACK);

    // COLOR_OFF is black, DARK_GRAY (3) ... LIGHT_BLUE (29) in the order of the DAW colors
    for (i, color) in DawColor::ALL.iter().enumerate() {
        if *color == DawColor::ColorOff {
            cm.register_color_index(color.id(), BLACK);
            continue;
        }
        let index = FIRST_DAW_COLOR + i as i32 - 1;
        cm.register_color_index(color.id(), index);
        // Java registers the dark gray of ColorEx, not the DAW color
        cm.register_color(index, if index == DARK_GRAY { rgb(ColorEx::DARK_GRAY) } else { rgb(color.color()) });
    }
    cm.register_color(BLACK, rgb(ColorEx::BLACK));
    cm.register_color(SKY_BLUE, rgb(ColorEx::SKY_BLUE));
    cm.register_color(WHITE, rgb(ColorEx::WHITE));
    cm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices() {
        let cm = create_color_manager();
        assert_eq!(cm.color_index("DAW_COLOR_DARK_GRAY"), DARK_GRAY);
        assert_eq!(cm.color_index("DAW_COLOR_GRAY"), 4);
        assert_eq!(cm.color_index("DAW_COLOR_RED"), 14);
        assert_eq!(cm.color_index("DAW_COLOR_LIGHT_BLUE"), 29);
        assert_eq!(cm.color_index("COLOR_OFF"), BLACK);
        assert_eq!(cm.color_index(Scales::SCALE_COLOR_OCTAVE), SKY_BLUE);
        assert_eq!(cm.color(DARK_GRAY), rgb(ColorEx::DARK_GRAY));
        assert_eq!(cm.color(14), rgb(DawColor::Red.color()));
        assert_eq!(cm.color(WHITE), (1.0, 1.0, 1.0));
    }
}
