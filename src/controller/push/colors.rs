// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Different colors to use for the pads and buttons of Push 1, Push 2 and Push 3 (former
//! `PushColorManager`).
//!
//! The color indices are sent directly as the velocity / CC value of the pads and buttons:
//! - Push 2/3: pads and RGB buttons use an index into the (programmable) color palette. The
//!   DrivenByMoss palette is the Push 1 palette ([`DEFAULT_PALETTE`]) with the DAW colors at the
//!   indices 70 to 96; it is written to the device by `super::palette::ColorPalette`.
//! - Push 1: pads use the fixed Push 1 palette (same as [`DEFAULT_PALETTE`]), the first row and the
//!   scene buttons use the `PUSH1_COLOR_*` / `PUSH1_COLOR_SCENE_*` values, the transport buttons
//!   only have the light states 0 (off), 1 (dim), 4 (bright) and the blink variants.
//!
//! The RGB values registered for the 128 indices are only used for display purposes (simulator,
//! button colors on screen); see [`button_color`] and [`pad_color`] for the former override of
//! `getColor (colorIndex, buttonID)`.

use crate::config::push::PushVersion;
use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::*;
use crate::framework::feature::{BUTTON_COLOR2_HI, BUTTON_COLOR2_ON, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::grid::GRID_OFF;
use crate::framework::ids::ButtonId;
use crate::scale::Scales;

/// ID for color when button signals a recording state.
pub const PUSH_BUTTON_STATE_REC_ON: &str = "PUSH_BUTTON_STATE_REC_ON";
/// ID for color when button signals an activated recording state.
pub const PUSH_BUTTON_STATE_REC_HI: &str = "PUSH_BUTTON_STATE_REC_HI";
/// ID for color when button signals an overwrite state.
pub const PUSH_BUTTON_STATE_OVR_ON: &str = "PUSH_BUTTON_STATE_OVR_ON";
/// ID for color when button signals an activated overwrite state.
pub const PUSH_BUTTON_STATE_OVR_HI: &str = "PUSH_BUTTON_STATE_OVR_HI";
/// ID for color when button signals a play state.
pub const PUSH_BUTTON_STATE_PLAY_ON: &str = "PUSH_BUTTON_STATE_PLAY_ON";
/// ID for color when button signals an activated play state.
pub const PUSH_BUTTON_STATE_PLAY_HI: &str = "PUSH_BUTTON_STATE_PLAY_HI";
/// ID for color when button signals a mute state.
pub const PUSH_BUTTON_STATE_MUTE_ON: &str = "PUSH_BUTTON_STATE_MUTE_ON";
/// ID for color when button signals an activated mute state.
pub const PUSH_BUTTON_STATE_MUTE_HI: &str = "PUSH_BUTTON_STATE_MUTE_HI";
/// ID for color when button signals a solo state.
pub const PUSH_BUTTON_STATE_SOLO_ON: &str = "PUSH_BUTTON_STATE_SOLO_ON";
/// ID for color when button signals an activated solo state.
pub const PUSH_BUTTON_STATE_SOLO_HI: &str = "PUSH_BUTTON_STATE_SOLO_HI";
/// ID for color when button signals a stop clip state.
pub const PUSH_BUTTON_STATE_STOP_ON: &str = "PUSH_BUTTON_STATE_STOP_ON";
/// ID for color when button signals an activated stop clip state.
pub const PUSH_BUTTON_STATE_STOP_HI: &str = "PUSH_BUTTON_STATE_STOP_HI";

/// ID for the color to use for note repeat resolution.
pub const NOTE_REPEAT_PERIOD_OFF: &str = "NOTE_REPEAT_PERIOD_OFF";
/// ID for the color to use for note repeat resolution selected.
pub const NOTE_REPEAT_PERIOD_HI: &str = "NOTE_REPEAT_PERIOD_HI";
/// ID for the color to use for note repeat length.
pub const NOTE_REPEAT_LENGTH_OFF: &str = "NOTE_REPEAT_LENGTH_OFF";
/// ID for the color to use for note repeat length selected.
pub const NOTE_REPEAT_LENGTH_HI: &str = "NOTE_REPEAT_LENGTH_HI";

/// ID for color when button signals a master state.
pub const PUSH_BUTTON_STATE_MASTER_ON: &str = "PUSH_BUTTON_STATE_MASTER_ON";
/// ID for color when button signals an activated master state.
pub const PUSH_BUTTON_STATE_MASTER_HI: &str = "PUSH_BUTTON_STATE_MASTER_HI";

// ----------------------------------------------------------------
// Only Push 3

/// ID for color when button signals a lock state.
pub const PUSH_BUTTON_STATE_LOCK_ON: &str = "PUSH_BUTTON_STATE_LOCK_ON";
/// ID for color when button signals an activated lock state.
pub const PUSH_BUTTON_STATE_LOCK_HI: &str = "PUSH_BUTTON_STATE_LOCK_HI";
/// ID for color when button signals a insert scene state.
pub const PUSH_BUTTON_STATE_INSERT_SCENE_ON: &str = "PUSH_BUTTON_STATE_INSERT_SCENE_ON";
/// ID for color when button signals an activated insert scene state.
pub const PUSH_BUTTON_STATE_INSERT_SCENE_HI: &str = "PUSH_BUTTON_STATE_INSERT_SCENE_HI";

/// The default color palette as RGB (0-255), like fixed on Push 1. The indices 70 to 96 are
/// replaced by the DAW colors in the Push 2/3 palette (see [`palette_color_rgb`]).
#[rustfmt::skip]
pub const DEFAULT_PALETTE: [[i32; 3]; 128] = [
    [0x00, 0x00, 0x00], [0x1E, 0x1E, 0x1E], [0x7F, 0x7F, 0x7F],
    [0xFF, 0xFF, 0xFF], [0xFF, 0x4C, 0x4C], [0xFF, 0x00, 0x00],
    [0x59, 0x00, 0x00], [0x19, 0x00, 0x00], [0xFF, 0xBD, 0x6C],
    [0xFF, 0x54, 0x00], [0x59, 0x1D, 0x00], [0x27, 0x1B, 0x00],
    [0xFF, 0xFF, 0x4C], [0xFF, 0xFF, 0x00], [0x59, 0x59, 0x00],
    [0x19, 0x19, 0x00], [0x88, 0xFF, 0x4C], [0x54, 0xFF, 0x00],
    [0x1D, 0x59, 0x00], [0x14, 0x2B, 0x00], [0x4C, 0xFF, 0x4C],
    [0x00, 0xFF, 0x00], [0x00, 0x59, 0x00], [0x00, 0x19, 0x00],
    [0x4C, 0xFF, 0x5E], [0x00, 0xFF, 0x19], [0x00, 0x59, 0x0D],
    [0x00, 0x19, 0x02], [0x4C, 0xFF, 0x88], [0x00, 0xFF, 0x55],
    [0x00, 0x59, 0x1D], [0x00, 0x1F, 0x12], [0x4C, 0xFF, 0xB7],
    [0x00, 0xFF, 0x99], [0x00, 0x59, 0x35], [0x00, 0x19, 0x12],
    [0x4C, 0xC3, 0xFF], [0x00, 0xA9, 0xFF], [0x00, 0x41, 0x52],
    [0x00, 0x10, 0x19], [0x4C, 0x88, 0xFF], [0x00, 0x55, 0xFF],
    [0x00, 0x1D, 0x59], [0x00, 0x08, 0x19], [0x4C, 0x4C, 0xFF],
    [0x00, 0x00, 0xFF], [0x00, 0x00, 0x59], [0x00, 0x00, 0x19],
    [0x87, 0x4C, 0xFF], [0x54, 0x00, 0xFF], [0x19, 0x00, 0x64],
    [0x0F, 0x00, 0x30], [0xFF, 0x4C, 0xFF], [0xFF, 0x00, 0xFF],
    [0x59, 0x00, 0x59], [0x19, 0x00, 0x19], [0xFF, 0x4C, 0x87],
    [0xFF, 0x00, 0x54], [0x59, 0x00, 0x1D], [0x22, 0x00, 0x13],
    [0xFF, 0x15, 0x00], [0x99, 0x35, 0x00], [0x79, 0x51, 0x00],
    [0x43, 0x64, 0x00], [0x03, 0x39, 0x00], [0x00, 0x57, 0x35],
    [0x00, 0x54, 0x7F], [0x00, 0x00, 0xFF], [0x00, 0x45, 0x4F],
    [0x25, 0x00, 0xCC], [0x7F, 0x7F, 0x7F], [0x20, 0x20, 0x20],
    [0xFF, 0x00, 0x00], [0xBD, 0xFF, 0x2D], [0xAF, 0xED, 0x06],
    [0x64, 0xFF, 0x09], [0x10, 0x8B, 0x00], [0x00, 0xFF, 0x87],
    [0x00, 0xA9, 0xFF], [0x00, 0x2A, 0xFF], [0x3F, 0x00, 0xFF],
    [0x7A, 0x00, 0xFF], [0xB2, 0x1A, 0x7D], [0x40, 0x21, 0x00],
    [0xFF, 0x4A, 0x00], [0x88, 0xE1, 0x06], [0x72, 0xFF, 0x15],
    [0x00, 0xFF, 0x00], [0x3B, 0xFF, 0x26], [0x59, 0xFF, 0x71],
    [0x38, 0xFF, 0xCC], [0x5B, 0x8A, 0xFF], [0x31, 0x51, 0xC6],
    [0x87, 0x7F, 0xE9], [0xD3, 0x1D, 0xFF], [0xFF, 0x00, 0x5D],
    [0xFF, 0x7F, 0x00], [0xB9, 0xB0, 0x00], [0x90, 0xFF, 0x00],
    [0x83, 0x5D, 0x07], [0x39, 0x2B, 0x00], [0x14, 0x4C, 0x10],
    [0x0D, 0x50, 0x38], [0x15, 0x15, 0x2A], [0x16, 0x20, 0x5A],
    [0x69, 0x3C, 0x1C], [0xA8, 0x00, 0x0A], [0xDE, 0x51, 0x3D],
    [0xD8, 0x6A, 0x1C], [0xFF, 0xE1, 0x26], [0x9E, 0xE1, 0x2F],
    [0x67, 0xB5, 0x0F], [0x1E, 0x1E, 0x30], [0xDC, 0xFF, 0x6B],
    [0x80, 0xFF, 0xBD], [0x9A, 0x99, 0xFF], [0x8E, 0x66, 0xFF],
    [0x40, 0x40, 0x40], [0x75, 0x75, 0x75], [0xE0, 0xFF, 0xFF],
    [0xA0, 0x00, 0x00], [0x35, 0x00, 0x00], [0x1A, 0xD0, 0x00],
    [0x07, 0x42, 0x00], [0xB9, 0xB0, 0x00], [0x3F, 0x31, 0x00],
    [0xB3, 0x5F, 0x00], [0x4B, 0x15, 0x02]
];

// Second row & Pad button colors
pub const PUSH2_COLOR2_BLACK: i32 = 0;
pub const PUSH2_COLOR2_GREY_LO: i32 = 1;
pub const PUSH2_COLOR2_GREY_MD: i32 = 103;
pub const PUSH2_COLOR2_GREY_LT: i32 = 2;
pub const PUSH2_COLOR2_WHITE: i32 = 3;
pub const PUSH2_COLOR2_ROSE: i32 = 4;
pub const PUSH2_COLOR2_RED_HI: i32 = 5;
pub const PUSH2_COLOR2_RED: i32 = 6;
pub const PUSH2_COLOR2_RED_LO: i32 = 7;
pub const PUSH2_COLOR2_RED_AMBER: i32 = 8;
pub const PUSH2_COLOR2_AMBER_HI: i32 = 9;
pub const PUSH2_COLOR2_AMBER: i32 = 10;
pub const PUSH2_COLOR2_AMBER_LO: i32 = 11;
pub const PUSH2_COLOR2_AMBER_YELLOW: i32 = 12;
pub const PUSH2_COLOR2_YELLOW_HI: i32 = 13;
pub const PUSH2_COLOR2_YELLOW: i32 = 14;
pub const PUSH2_COLOR2_YELLOW_LO: i32 = 15;
pub const PUSH2_COLOR2_YELLOW_LIME: i32 = 16;
pub const PUSH2_COLOR2_LIME_HI: i32 = 17;
pub const PUSH2_COLOR2_LIME: i32 = 18;
pub const PUSH2_COLOR2_LIME_LO: i32 = 19;
pub const PUSH2_COLOR2_LIME_GREEN: i32 = 20;
pub const PUSH2_COLOR2_GREEN_HI: i32 = 21;
pub const PUSH2_COLOR2_GREEN: i32 = 22;
pub const PUSH2_COLOR2_GREEN_LO: i32 = 23;
pub const PUSH2_COLOR2_GREEN_SPRING: i32 = 24;
pub const PUSH2_COLOR2_SPRING_HI: i32 = 25;
pub const PUSH2_COLOR2_SPRING: i32 = 26;
pub const PUSH2_COLOR2_SPRING_LO: i32 = 27;
pub const PUSH2_COLOR2_SPRING_TURQUOISE: i32 = 28;
pub const PUSH2_COLOR2_TURQUOISE_LO: i32 = 29;
pub const PUSH2_COLOR2_TURQUOISE: i32 = 30;
pub const PUSH2_COLOR2_TURQUOISE_HI: i32 = 31;
pub const PUSH2_COLOR2_TURQUOISE_CYAN: i32 = 32;
pub const PUSH2_COLOR2_CYAN_HI: i32 = 33;
pub const PUSH2_COLOR2_CYAN: i32 = 34;
pub const PUSH2_COLOR2_CYAN_LO: i32 = 35;
pub const PUSH2_COLOR2_CYAN_SKY: i32 = 36;
pub const PUSH2_COLOR2_SKY_HI: i32 = 37;
pub const PUSH2_COLOR2_SKY: i32 = 38;
pub const PUSH2_COLOR2_SKY_LO: i32 = 39;
pub const PUSH2_COLOR2_SKY_OCEAN: i32 = 40;
pub const PUSH2_COLOR2_OCEAN_HI: i32 = 41;
pub const PUSH2_COLOR2_OCEAN: i32 = 42;
pub const PUSH2_COLOR2_OCEAN_LO: i32 = 43;
pub const PUSH2_COLOR2_OCEAN_BLUE: i32 = 44;
pub const PUSH2_COLOR2_BLUE_HI: i32 = 45;
pub const PUSH2_COLOR2_BLUE: i32 = 46;
pub const PUSH2_COLOR2_BLUE_LO: i32 = 47;
pub const PUSH2_COLOR2_BLUE_ORCHID: i32 = 48;
pub const PUSH2_COLOR2_ORCHID_HI: i32 = 49;
pub const PUSH2_COLOR2_ORCHID: i32 = 50;
pub const PUSH2_COLOR2_ORCHID_LO: i32 = 51;
pub const PUSH2_COLOR2_ORCHID_MAGENTA: i32 = 52;
pub const PUSH2_COLOR2_MAGENTA_HI: i32 = 53;
pub const PUSH2_COLOR2_MAGENTA: i32 = 54;
pub const PUSH2_COLOR2_MAGENTA_LO: i32 = 55;
pub const PUSH2_COLOR2_MAGENTA_PINK: i32 = 56;
pub const PUSH2_COLOR2_PINK_HI: i32 = 57;
pub const PUSH2_COLOR2_PINK: i32 = 58;
pub const PUSH2_COLOR2_PINK_LO: i32 = 59;
pub const PUSH2_COLOR2_SILVER: i32 = 118;
pub const PUSH2_COLOR2_ORANGE: i32 = 65;
pub const PUSH2_COLOR2_ORANGE_LIGHT: i32 = 3;
pub const PUSH2_COLOR2_LIGHT_BROWN: i32 = 69;


// First row colors
pub const PUSH2_COLOR_BLACK: i32 = 0;
pub const PUSH2_COLOR_RED_LO: i32 = PUSH2_COLOR2_RED_LO;
pub const PUSH2_COLOR_RED_LO_SBLINK: i32 = 2;
pub const PUSH2_COLOR_RED_LO_FBLINK: i32 = 3;
pub const PUSH2_COLOR_RED_HI: i32 = PUSH2_COLOR2_RED_HI;
pub const PUSH2_COLOR_RED_HI_SBLINK: i32 = 5;
pub const PUSH2_COLOR_RED_HI_FBLINK: i32 = 6;
pub const PUSH2_COLOR_ORANGE_LO: i32 = PUSH2_COLOR2_AMBER_LO;
pub const PUSH2_COLOR_ORANGE_LO_SBLINK: i32 = 8;
pub const PUSH2_COLOR_ORANGE_LO_FBLINK: i32 = 9;
pub const PUSH2_COLOR_ORANGE_HI: i32 = PUSH2_COLOR2_AMBER_HI;
pub const PUSH2_COLOR_ORANGE_HI_SBLINK: i32 = 11;
pub const PUSH2_COLOR_ORANGE_HI_FBLINK: i32 = 12;
pub const PUSH2_COLOR_YELLOW_LO: i32 = PUSH2_COLOR2_YELLOW_LO;
pub const PUSH2_COLOR_YELLOW_LO_SBLINK: i32 = 14;
pub const PUSH2_COLOR_YELLOW_LO_FBLINK: i32 = 15;
pub const PUSH2_COLOR_YELLOW_MD: i32 = PUSH2_COLOR2_YELLOW_HI;
pub const PUSH2_COLOR_YELLOW_MD_SBLINK: i32 = 17;
pub const PUSH2_COLOR_YELLOW_MD_FBLINK: i32 = 18;
pub const PUSH2_COLOR_GREEN_LO: i32 = PUSH2_COLOR2_GREEN_LO;
pub const PUSH2_COLOR_GREEN_LO_SBLINK: i32 = 20;
pub const PUSH2_COLOR_GREEN_LO_FBLINK: i32 = 21;
pub const PUSH2_COLOR_GREEN_HI: i32 = PUSH2_COLOR2_GREEN_HI;
pub const PUSH2_COLOR_GREEN_HI_SBLINK: i32 = 23;
pub const PUSH2_COLOR_GREEN_HI_FBLINK: i32 = 24;


// Scene button colors
pub const PUSH2_COLOR_SCENE_RED: i32 = PUSH2_COLOR2_RED;
pub const PUSH2_COLOR_SCENE_RED_BLINK: i32 = 2;
pub const PUSH2_COLOR_SCENE_RED_BLINK_FAST: i32 = 3;
pub const PUSH2_COLOR_SCENE_RED_HI: i32 = PUSH2_COLOR2_RED_HI;
pub const PUSH2_COLOR_SCENE_RED_HI_BLINK: i32 = 5;
pub const PUSH2_COLOR_SCENE_RED_HI_BLINK_FAST: i32 = 6;
pub const PUSH2_COLOR_SCENE_ORANGE: i32 = PUSH2_COLOR2_AMBER;
pub const PUSH2_COLOR_SCENE_ORANGE_BLINK: i32 = 8;
pub const PUSH2_COLOR_SCENE_ORANGE_BLINK_FAST: i32 = 9;
pub const PUSH2_COLOR_SCENE_ORANGE_HI: i32 = PUSH2_COLOR2_AMBER_HI;
pub const PUSH2_COLOR_SCENE_ORANGE_HI_BLINK: i32 = 11;
pub const PUSH2_COLOR_SCENE_ORANGE_HI_BLINK_FAST: i32 = 12;
pub const PUSH2_COLOR_SCENE_YELLOW: i32 = PUSH2_COLOR2_YELLOW;
pub const PUSH2_COLOR_SCENE_YELLOW_BLINK: i32 = 14;
pub const PUSH2_COLOR_SCENE_YELLOW_BLINK_FAST: i32 = 15;
pub const PUSH2_COLOR_SCENE_YELLOW_HI: i32 = PUSH2_COLOR2_YELLOW_HI;
pub const PUSH2_COLOR_SCENE_YELLOW_HI_BLINK: i32 = 17;
pub const PUSH2_COLOR_SCENE_YELLOW_HI_BLINK_FAST: i32 = 18;
pub const PUSH2_COLOR_SCENE_GREEN: i32 = PUSH2_COLOR2_GREEN;
pub const PUSH2_COLOR_SCENE_GREEN_BLINK: i32 = 20;
pub const PUSH2_COLOR_SCENE_GREEN_BLINK_FAST: i32 = 21;
pub const PUSH2_COLOR_SCENE_GREEN_HI: i32 = PUSH2_COLOR2_GREEN_HI;
pub const PUSH2_COLOR_SCENE_GREEN_HI_BLINK: i32 = 23;
pub const PUSH2_COLOR_SCENE_GREEN_HI_BLINK_FAST: i32 = 24;
pub const PUSH2_COLOR_SCENE_WHITE: i32 = 60;


// First row colors
pub const PUSH1_COLOR_BLACK: i32 = 0;
pub const PUSH1_COLOR_RED_LO: i32 = 1;
pub const PUSH1_COLOR_RED_LO_SBLINK: i32 = 2;
pub const PUSH1_COLOR_RED_LO_FBLINK: i32 = 3;
pub const PUSH1_COLOR_RED_HI: i32 = 4;
pub const PUSH1_COLOR_RED_HI_SBLINK: i32 = 5;
pub const PUSH1_COLOR_RED_HI_FBLINK: i32 = 6;
pub const PUSH1_COLOR_ORANGE_LO: i32 = 7;
pub const PUSH1_COLOR_ORANGE_LO_SBLINK: i32 = 8;
pub const PUSH1_COLOR_ORANGE_LO_FBLINK: i32 = 9;
pub const PUSH1_COLOR_ORANGE_HI: i32 = 10;
pub const PUSH1_COLOR_ORANGE_HI_SBLINK: i32 = 11;
pub const PUSH1_COLOR_ORANGE_HI_FBLINK: i32 = 12;
pub const PUSH1_COLOR_YELLOW_LO: i32 = 13;
pub const PUSH1_COLOR_YELLOW_LO_SBLINK: i32 = 14;
pub const PUSH1_COLOR_YELLOW_LO_FBLINK: i32 = 15;
pub const PUSH1_COLOR_YELLOW_MD: i32 = 16;
pub const PUSH1_COLOR_YELLOW_MD_SBLINK: i32 = 17;
pub const PUSH1_COLOR_YELLOW_MD_FBLINK: i32 = 18;
pub const PUSH1_COLOR_GREEN_LO: i32 = 19;
pub const PUSH1_COLOR_GREEN_LO_SBLINK: i32 = 20;
pub const PUSH1_COLOR_GREEN_LO_FBLINK: i32 = 21;
pub const PUSH1_COLOR_GREEN_HI: i32 = 22;
pub const PUSH1_COLOR_GREEN_HI_SBLINK: i32 = 23;
pub const PUSH1_COLOR_GREEN_HI_FBLINK: i32 = 24;


// Second row & Pad button colors
pub const PUSH1_COLOR2_BLACK: i32 = 0;
pub const PUSH1_COLOR2_GREY_LO: i32 = 1;
pub const PUSH1_COLOR2_GREY_MD: i32 = 103;
pub const PUSH1_COLOR2_GREY_LT: i32 = 2;
pub const PUSH1_COLOR2_WHITE: i32 = 3;
pub const PUSH1_COLOR2_ROSE: i32 = 4;
pub const PUSH1_COLOR2_RED_HI: i32 = 5;
pub const PUSH1_COLOR2_RED: i32 = 6;
pub const PUSH1_COLOR2_RED_LO: i32 = 7;
pub const PUSH1_COLOR2_RED_AMBER: i32 = 8;
pub const PUSH1_COLOR2_AMBER_HI: i32 = 9;
pub const PUSH1_COLOR2_AMBER: i32 = 10;
pub const PUSH1_COLOR2_AMBER_LO: i32 = 11;
pub const PUSH1_COLOR2_AMBER_YELLOW: i32 = 12;
pub const PUSH1_COLOR2_YELLOW_HI: i32 = 13;
pub const PUSH1_COLOR2_YELLOW: i32 = 14;
pub const PUSH1_COLOR2_YELLOW_LO: i32 = 15;
pub const PUSH1_COLOR2_YELLOW_LIME: i32 = 16;
pub const PUSH1_COLOR2_LIME_HI: i32 = 17;
pub const PUSH1_COLOR2_LIME: i32 = 18;
pub const PUSH1_COLOR2_LIME_LO: i32 = 19;
pub const PUSH1_COLOR2_LIME_GREEN: i32 = 20;
pub const PUSH1_COLOR2_GREEN_HI: i32 = 21;
pub const PUSH1_COLOR2_GREEN: i32 = 22;
pub const PUSH1_COLOR2_GREEN_LO: i32 = 23;
pub const PUSH1_COLOR2_GREEN_SPRING: i32 = 24;
pub const PUSH1_COLOR2_SPRING_HI: i32 = 25;
pub const PUSH1_COLOR2_SPRING: i32 = 26;
pub const PUSH1_COLOR2_SPRING_LO: i32 = 27;
pub const PUSH1_COLOR2_SPRING_TURQUOISE: i32 = 28;
pub const PUSH1_COLOR2_TURQUOISE_LO: i32 = 29;
pub const PUSH1_COLOR2_TURQUOISE: i32 = 30;
pub const PUSH1_COLOR2_TURQUOISE_HI: i32 = 31;
pub const PUSH1_COLOR2_TURQUOISE_CYAN: i32 = 32;
pub const PUSH1_COLOR2_CYAN_HI: i32 = 33;
pub const PUSH1_COLOR2_CYAN: i32 = 34;
pub const PUSH1_COLOR2_CYAN_LO: i32 = 35;
pub const PUSH1_COLOR2_CYAN_SKY: i32 = 36;
pub const PUSH1_COLOR2_SKY_HI: i32 = 37;
pub const PUSH1_COLOR2_SKY: i32 = 38;
pub const PUSH1_COLOR2_SKY_LO: i32 = 39;
pub const PUSH1_COLOR2_SKY_OCEAN: i32 = 40;
pub const PUSH1_COLOR2_OCEAN_HI: i32 = 41;
pub const PUSH1_COLOR2_OCEAN: i32 = 42;
pub const PUSH1_COLOR2_OCEAN_LO: i32 = 43;
pub const PUSH1_COLOR2_OCEAN_BLUE: i32 = 44;
pub const PUSH1_COLOR2_BLUE_HI: i32 = 45;
pub const PUSH1_COLOR2_BLUE: i32 = 46;
pub const PUSH1_COLOR2_BLUE_LO: i32 = 47;
pub const PUSH1_COLOR2_BLUE_ORCHID: i32 = 48;
pub const PUSH1_COLOR2_ORCHID_HI: i32 = 49;
pub const PUSH1_COLOR2_ORCHID: i32 = 50;
pub const PUSH1_COLOR2_ORCHID_LO: i32 = 51;
pub const PUSH1_COLOR2_ORCHID_MAGENTA: i32 = 52;
pub const PUSH1_COLOR2_MAGENTA_HI: i32 = 53;
pub const PUSH1_COLOR2_MAGENTA: i32 = 54;
pub const PUSH1_COLOR2_MAGENTA_LO: i32 = 55;
pub const PUSH1_COLOR2_MAGENTA_PINK: i32 = 56;
pub const PUSH1_COLOR2_PINK_HI: i32 = 57;
pub const PUSH1_COLOR2_PINK: i32 = 58;
pub const PUSH1_COLOR2_PINK_LO: i32 = 59;
pub const PUSH1_COLOR2_SILVER: i32 = 118;
pub const PUSH1_COLOR2_ORANGE: i32 = 65;
pub const PUSH1_COLOR2_ORANGE_LIGHT: i32 = 3;
pub const PUSH1_COLOR2_LIGHT_BROWN: i32 = 69;


// Scene button colors
pub const PUSH1_COLOR_SCENE_RED: i32 = 1;
pub const PUSH1_COLOR_SCENE_RED_BLINK: i32 = 2;
pub const PUSH1_COLOR_SCENE_RED_BLINK_FAST: i32 = 3;
pub const PUSH1_COLOR_SCENE_RED_HI: i32 = 4;
pub const PUSH1_COLOR_SCENE_RED_HI_BLINK: i32 = 5;
pub const PUSH1_COLOR_SCENE_RED_HI_BLINK_FAST: i32 = 6;
pub const PUSH1_COLOR_SCENE_ORANGE: i32 = 7;
pub const PUSH1_COLOR_SCENE_ORANGE_BLINK: i32 = 8;
pub const PUSH1_COLOR_SCENE_ORANGE_BLINK_FAST: i32 = 9;
pub const PUSH1_COLOR_SCENE_ORANGE_HI: i32 = 10;
pub const PUSH1_COLOR_SCENE_ORANGE_HI_BLINK: i32 = 11;
pub const PUSH1_COLOR_SCENE_ORANGE_HI_BLINK_FAST: i32 = 12;
pub const PUSH1_COLOR_SCENE_YELLOW: i32 = 13;
pub const PUSH1_COLOR_SCENE_YELLOW_BLINK: i32 = 14;
pub const PUSH1_COLOR_SCENE_YELLOW_BLINK_FAST: i32 = 15;
pub const PUSH1_COLOR_SCENE_YELLOW_HI: i32 = 16;
pub const PUSH1_COLOR_SCENE_YELLOW_HI_BLINK: i32 = 17;
pub const PUSH1_COLOR_SCENE_YELLOW_HI_BLINK_FAST: i32 = 18;
pub const PUSH1_COLOR_SCENE_GREEN: i32 = 19;
pub const PUSH1_COLOR_SCENE_GREEN_BLINK: i32 = 20;
pub const PUSH1_COLOR_SCENE_GREEN_BLINK_FAST: i32 = 21;
pub const PUSH1_COLOR_SCENE_GREEN_HI: i32 = 22;
pub const PUSH1_COLOR_SCENE_GREEN_HI_BLINK: i32 = 23;
pub const PUSH1_COLOR_SCENE_GREEN_HI_BLINK_FAST: i32 = 24;

pub const PUSH_BLACK: &str = "PUSH_BLACK";
pub const PUSH_RED: &str = "PUSH_RED";
pub const PUSH_RED_LO: &str = "PUSH_RED_LO";
pub const PUSH_RED_HI: &str = "PUSH_RED_HI";
pub const PUSH_ORANGE_LO: &str = "PUSH_ORANGE_LO";
pub const PUSH_ORANGE_HI: &str = "PUSH_ORANGE_HI";
pub const PUSH_YELLOW_LO: &str = "PUSH_YELLOW_LO";
pub const PUSH_YELLOW_MD: &str = "PUSH_YELLOW_MD";
pub const PUSH_GREEN_LO: &str = "PUSH_GREEN_LO";
pub const PUSH_GREEN_HI: &str = "PUSH_GREEN_HI";

pub const PUSH_BLACK_2: &str = "PUSH_BLACK_2";
pub const PUSH_WHITE_2: &str = "PUSH_WHITE_2";
pub const PUSH_GREY_LO_2: &str = "PUSH_GREY_LO_2";
pub const PUSH_GREEN_2: &str = "PUSH_GREEN_2";

/// The buttons which have only a white LED on Push 2/3 (and a single color on Push 1). Only used
/// for the display color of a button (see [`button_color`]).
pub const MONOCHROME_BUTTONS: &[ButtonId] = &[
    ButtonId::Accent,
    ButtonId::AddEffect,
    ButtonId::AddTrack,
    ButtonId::ArrowDown,
    ButtonId::ArrowLeft,
    ButtonId::ArrowRight,
    ButtonId::ArrowUp,
    ButtonId::Browse,
    ButtonId::Clip,
    ButtonId::Delete,
    ButtonId::Device,
    ButtonId::Double,
    ButtonId::Duplicate,
    ButtonId::FixedLength,
    ButtonId::Help,
    ButtonId::InsertScene,
    ButtonId::Layout,
    ButtonId::Load,
    ButtonId::LockMode,
    ButtonId::Mastertrack,
    ButtonId::Metronome,
    ButtonId::New,
    ButtonId::Note,
    ButtonId::OctaveDown,
    ButtonId::OctaveUp,
    ButtonId::PageLeft,
    ButtonId::PageRight,
    ButtonId::PanSend,
    ButtonId::Quantize,
    ButtonId::Repeat,
    ButtonId::Save,
    ButtonId::Scales,
    ButtonId::Select,
    ButtonId::Session,
    ButtonId::Setup,
    ButtonId::Shift,
    ButtonId::TapTempo,
    ButtonId::Track,
    ButtonId::Undo,
    ButtonId::User,
    ButtonId::Volume,
];

fn rgb(c: ColorEx) -> (f64, f64, f64) {
    (c.red(), c.green(), c.blue())
}

/// Create the color manager for the given Push version (former `PushColorManager` constructor).
pub fn create_color_manager(version: PushVersion) -> ColorManager {
    let is_modern = version.is_modern();
    let is_push3 = version == PushVersion::Version3;

    let mut cm = ColorManager::new();

    cm.register_color_index(PUSH_BLACK, if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK });
    cm.register_color_index(PUSH_RED, if is_modern { PUSH2_COLOR_RED_HI } else { PUSH1_COLOR_RED_HI });
    cm.register_color_index(PUSH_RED_LO, if is_modern { PUSH2_COLOR_RED_LO } else { PUSH1_COLOR_RED_LO });
    cm.register_color_index(PUSH_RED_HI, if is_modern { PUSH2_COLOR_RED_HI } else { PUSH1_COLOR_RED_HI });
    cm.register_color_index(PUSH_ORANGE_LO, if is_modern { PUSH2_COLOR_ORANGE_LO } else { PUSH1_COLOR_ORANGE_LO });
    cm.register_color_index(PUSH_ORANGE_HI, if is_modern { PUSH2_COLOR_ORANGE_HI } else { PUSH1_COLOR_ORANGE_HI });
    cm.register_color_index(PUSH_YELLOW_LO, if is_modern { PUSH2_COLOR_YELLOW_LO } else { PUSH1_COLOR_YELLOW_LO });
    cm.register_color_index(PUSH_YELLOW_MD, if is_modern { PUSH2_COLOR_YELLOW_MD } else { PUSH1_COLOR_YELLOW_MD });
    cm.register_color_index(PUSH_GREEN_LO, if is_modern { PUSH2_COLOR_GREEN_LO } else { PUSH1_COLOR_GREEN_LO });
    cm.register_color_index(PUSH_GREEN_HI, if is_modern { PUSH2_COLOR_GREEN_HI } else { PUSH1_COLOR_GREEN_HI });

    cm.register_color_index(PUSH_BLACK_2, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });
    cm.register_color_index(PUSH_WHITE_2, if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR2_WHITE });
    cm.register_color_index(PUSH_GREY_LO_2, if is_modern { PUSH2_COLOR2_GREY_LO } else { PUSH1_COLOR2_GREY_LO });
    cm.register_color_index(PUSH_GREEN_2, if is_modern { PUSH2_COLOR2_GREEN } else { PUSH1_COLOR2_GREEN });

    cm.register_color_index(Scales::SCALE_COLOR_OFF, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });
    cm.register_color_index(Scales::SCALE_COLOR_OCTAVE, if is_modern { PUSH2_COLOR2_OCEAN_HI } else { PUSH1_COLOR2_OCEAN_HI });
    cm.register_color_index(Scales::SCALE_COLOR_NOTE, if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR2_WHITE });
    cm.register_color_index(Scales::SCALE_COLOR_OUT_OF_SCALE, if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK });

    cm.register_color_index(BUTTON_COLOR_OFF, if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK });
    cm.register_color_index(BUTTON_COLOR_ON, if is_modern { PUSH2_COLOR2_GREY_LO } else { PUSH1_COLOR_GREEN_LO });
    cm.register_color_index(BUTTON_COLOR_HI, if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR_YELLOW_MD });
    cm.register_color_index(BUTTON_COLOR2_ON, if is_modern { PUSH2_COLOR2_GREY_LO } else { PUSH1_COLOR2_GREEN_LO });
    cm.register_color_index(BUTTON_COLOR2_HI, if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR2_YELLOW_HI });

    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT_4, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT, if is_modern { PUSH2_COLOR2_BLUE_HI } else { PUSH1_COLOR2_BLUE_HI });
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT_CONT, if is_modern { PUSH2_COLOR2_BLUE_LO } else { PUSH1_COLOR2_BLUE_LO });
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, if is_modern { PUSH2_COLOR2_GREEN_LO } else { PUSH1_COLOR2_GREEN_LO });
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, if is_modern { PUSH2_COLOR2_GREEN_HI } else { PUSH1_COLOR2_GREEN_HI });
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED, if is_modern { PUSH2_COLOR2_GREY_MD } else { PUSH1_COLOR2_GREY_MD });
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, if is_modern { PUSH2_COLOR2_GREY_LO } else { PUSH1_COLOR2_GREY_LO });
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_SELECTED, if is_modern { PUSH2_COLOR2_YELLOW_HI } else { PUSH1_COLOR2_YELLOW_HI });
    cm.register_color_index(abstract_sequencer_view::COLOR_PAGE, if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR2_WHITE });
    cm.register_color_index(abstract_sequencer_view::COLOR_ACTIVE_PAGE, if is_modern { PUSH2_COLOR2_GREEN } else { PUSH1_COLOR2_GREEN });
    cm.register_color_index(abstract_sequencer_view::COLOR_SELECTED_PAGE, if is_modern { PUSH2_COLOR2_OCEAN_HI } else { PUSH1_COLOR2_OCEAN_HI });
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION, if is_modern { PUSH2_COLOR_SCENE_ORANGE } else { PUSH1_COLOR_SCENE_ORANGE });
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION_SELECTED, if is_modern { PUSH2_COLOR_SCENE_ORANGE_HI } else { PUSH1_COLOR_SCENE_ORANGE_HI });
    cm.register_color_index(abstract_sequencer_view::COLOR_RESOLUTION_OFF, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });
    cm.register_color_index(abstract_sequencer_view::COLOR_TRANSPOSE, if is_modern { PUSH2_COLOR_SCENE_WHITE } else { PUSH1_COLOR_SCENE_YELLOW });
    cm.register_color_index(abstract_sequencer_view::COLOR_TRANSPOSE_SELECTED, if is_modern { PUSH2_COLOR_SCENE_YELLOW_HI } else { PUSH1_COLOR_SCENE_YELLOW_HI });

    cm.register_color_index(abstract_drum_view::COLOR_PAD_OFF, if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK });
    cm.register_color_index(abstract_drum_view::COLOR_PAD_RECORD, if is_modern { PUSH2_COLOR2_RED_HI } else { PUSH1_COLOR2_RED_HI });
    cm.register_color_index(abstract_drum_view::COLOR_PAD_PLAY, if is_modern { PUSH2_COLOR2_GREEN_HI } else { PUSH1_COLOR2_GREEN_HI });
    cm.register_color_index(abstract_drum_view::COLOR_PAD_SELECTED, if is_modern { PUSH2_COLOR2_BLUE_HI } else { PUSH1_COLOR2_BLUE_HI });
    cm.register_color_index(abstract_drum_view::COLOR_PAD_MUTED, if is_modern { PUSH2_COLOR2_AMBER_LO } else { PUSH1_COLOR2_AMBER_LO });
    cm.register_color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT, if is_modern { PUSH2_COLOR2_YELLOW_HI } else { PUSH1_COLOR2_YELLOW_HI });
    cm.register_color_index(abstract_drum_view::COLOR_PAD_NO_CONTENT, if is_modern { PUSH2_COLOR2_YELLOW_LO } else { PUSH1_COLOR2_YELLOW_LO });

    cm.register_color_index(abstract_play_view::COLOR_PLAY, if is_modern { PUSH2_COLOR2_GREEN_HI } else { PUSH1_COLOR2_GREEN_HI });
    cm.register_color_index(abstract_play_view::COLOR_RECORD, if is_modern { PUSH2_COLOR2_RED_HI } else { PUSH1_COLOR2_RED_HI });
    cm.register_color_index(abstract_play_view::COLOR_OFF, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });

    cm.register_color_index(clip_length_view::COLOR_OUTSIDE, if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK });
    cm.register_color_index(clip_length_view::COLOR_PART, if is_modern { PUSH2_COLOR2_OCEAN_HI } else { PUSH1_COLOR2_OCEAN_HI });

    cm.register_color_index(abstract_session_view::COLOR_SCENE, if is_modern { PUSH2_COLOR_SCENE_GREEN } else { PUSH1_COLOR_SCENE_GREEN });
    cm.register_color_index(abstract_session_view::COLOR_SELECTED_SCENE, if is_modern { PUSH2_COLOR_SCENE_GREEN_HI } else { PUSH1_COLOR_SCENE_GREEN_HI });
    cm.register_color_index(abstract_session_view::COLOR_SCENE_OFF, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });

    cm.register_color_index(scene_play_view::COLOR_SELECTED_PLAY_SCENE, if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR2_WHITE });

    cm.register_color_index(GRID_OFF, if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });

    cm.register_color_index(NOTE_REPEAT_PERIOD_OFF, if is_modern { PUSH2_COLOR_SCENE_YELLOW } else { PUSH1_COLOR_SCENE_YELLOW });
    cm.register_color_index(NOTE_REPEAT_PERIOD_HI, if is_modern { PUSH2_COLOR_SCENE_YELLOW_HI } else { PUSH1_COLOR_SCENE_YELLOW_HI });
    cm.register_color_index(NOTE_REPEAT_LENGTH_OFF, if is_modern { PUSH2_COLOR_SCENE_RED } else { PUSH1_COLOR_SCENE_RED });
    cm.register_color_index(NOTE_REPEAT_LENGTH_HI, if is_modern { PUSH2_COLOR_SCENE_RED_HI } else { PUSH1_COLOR_SCENE_RED_HI });

    // Push 2 DAW colors are set in the color palette from indices 70 to 96
    cm.register_color_index(DawColor::ColorOff.id(), if is_modern { PUSH2_COLOR2_BLACK } else { PUSH1_COLOR2_BLACK });
    cm.register_color_index(DawColor::GrayHalf.id(), if is_modern { 70 } else { PUSH1_COLOR2_GREY_MD });
    cm.register_color_index(DawColor::DarkGray.id(), if is_modern { 71 } else { 1 });
    cm.register_color_index(DawColor::Gray.id(), if is_modern { 72 } else { PUSH1_COLOR2_GREY_MD });
    cm.register_color_index(DawColor::LightGray.id(), if is_modern { 73 } else { PUSH1_COLOR2_GREY_LT });
    cm.register_color_index(DawColor::Silver.id(), if is_modern { 74 } else { 40 });
    cm.register_color_index(DawColor::DarkBrown.id(), if is_modern { 75 } else { 11 });
    cm.register_color_index(DawColor::Brown.id(), if is_modern { 76 } else { 12 });
    cm.register_color_index(DawColor::DarkBlue.id(), if is_modern { 77 } else { 42 });
    cm.register_color_index(DawColor::PurpleBlue.id(), if is_modern { 78 } else { 44 });
    cm.register_color_index(DawColor::Purple.id(), if is_modern { 79 } else { 81 });
    cm.register_color_index(DawColor::Pink.id(), if is_modern { 80 } else { 57 });
    cm.register_color_index(DawColor::Red.id(), if is_modern { 81 } else { 6 });
    cm.register_color_index(DawColor::Orange.id(), if is_modern { 82 } else { 60 });
    cm.register_color_index(DawColor::LightOrange.id(), if is_modern { 83 } else { 62 });
    cm.register_color_index(DawColor::MossGreen.id(), if is_modern { 84 } else { 19 });
    cm.register_color_index(DawColor::Green.id(), if is_modern { 85 } else { 26 });
    cm.register_color_index(DawColor::ColdGreen.id(), if is_modern { 86 } else { 30 });
    cm.register_color_index(DawColor::Blue.id(), if is_modern { 87 } else { 37 });
    cm.register_color_index(DawColor::LightPurple.id(), if is_modern { 88 } else { 48 });
    cm.register_color_index(DawColor::LightPink.id(), if is_modern { 89 } else { 56 });
    cm.register_color_index(DawColor::Rose.id(), if is_modern { 90 } else { 4 });
    cm.register_color_index(DawColor::ReddishBrown.id(), if is_modern { 91 } else { 10 });
    cm.register_color_index(DawColor::LightBrown.id(), if is_modern { 92 } else { 61 });
    cm.register_color_index(DawColor::LightGreen.id(), if is_modern { 93 } else { 18 });
    cm.register_color_index(DawColor::BluishGreen.id(), if is_modern { 94 } else { 25 });
    cm.register_color_index(DawColor::GreenBlue.id(), if is_modern { 95 } else { 32 });
    cm.register_color_index(DawColor::LightBlue.id(), if is_modern { 96 } else { 41 });

    cm.register_color_index(BUTTON_STATE_OFF, 0);
    cm.register_color_index(BUTTON_STATE_ON, if is_modern { 30 } else { 1 });
    cm.register_color_index(BUTTON_STATE_HI, if is_modern { 127 } else { 4 });
    cm.register_color_index(PUSH_BUTTON_STATE_REC_ON, if is_modern { PUSH2_COLOR2_GREY_LO } else { 1 });
    cm.register_color_index(PUSH_BUTTON_STATE_REC_HI, if is_modern { PUSH2_COLOR2_RED_HI } else { 4 });
    cm.register_color_index(PUSH_BUTTON_STATE_OVR_ON, if is_modern { PUSH2_COLOR2_GREY_LO } else { 1 });
    cm.register_color_index(PUSH_BUTTON_STATE_OVR_HI, if is_modern { PUSH2_COLOR2_AMBER } else { 2 });
    cm.register_color_index(PUSH_BUTTON_STATE_PLAY_ON, if is_modern { PUSH2_COLOR2_GREY_LO } else { 1 });
    cm.register_color_index(PUSH_BUTTON_STATE_PLAY_HI, if is_modern { PUSH2_COLOR2_GREEN_HI } else { 4 });
    cm.register_color_index(PUSH_BUTTON_STATE_MUTE_ON, if is_modern { PUSH2_COLOR2_GREY_LO } else { 1 });
    cm.register_color_index(PUSH_BUTTON_STATE_MUTE_HI, if is_modern { PUSH2_COLOR2_AMBER_LO } else { 4 });
    cm.register_color_index(PUSH_BUTTON_STATE_SOLO_ON, if is_modern { PUSH2_COLOR2_GREY_LO } else { 1 });
    cm.register_color_index(PUSH_BUTTON_STATE_SOLO_HI, if is_modern { PUSH2_COLOR2_YELLOW } else { 4 });
    cm.register_color_index(PUSH_BUTTON_STATE_STOP_ON, if is_modern { PUSH2_COLOR2_RED_LO } else { 1 });
    cm.register_color_index(PUSH_BUTTON_STATE_STOP_HI, if is_modern { PUSH2_COLOR2_RED_HI } else { 4 });

    cm.register_color_index(PUSH_BUTTON_STATE_MASTER_ON, if is_push3 { PUSH2_COLOR2_GREY_LO } else { 30 });
    cm.register_color_index(PUSH_BUTTON_STATE_MASTER_HI, if is_push3 { PUSH2_COLOR2_WHITE } else { 127 });

    // Only Push 3
    cm.register_color_index(PUSH_BUTTON_STATE_LOCK_ON, PUSH2_COLOR2_GREY_LO);
    cm.register_color_index(PUSH_BUTTON_STATE_LOCK_HI, PUSH2_COLOR2_RED_LO);
    cm.register_color_index(PUSH_BUTTON_STATE_INSERT_SCENE_ON, PUSH2_COLOR2_GREY_LO);
    cm.register_color_index(PUSH_BUTTON_STATE_INSERT_SCENE_HI, PUSH2_COLOR2_WHITE);

    for i in 0..128 {
        cm.register_color(i, rgb(palette_color(i)));
    }
    cm
}

/// Get a color entry of the default Push color palette as RGB (0-255) (former
/// `getPaletteColorRGB`). The indices 70 to 96 contain the DAW colors.
///
/// Panics if the index is not in the range of 0..127.
pub fn palette_color_rgb(index: i32) -> [i32; 3] {
    if (70..=96).contains(&index) {
        return DawColor::color_entry_at(index - 69).to_int_rgb255();
    }
    DEFAULT_PALETTE[index as usize]
}

/// Get a color of the default Push color palette (former `getPaletteColor`).
///
/// Panics if the index is not in the range of 0..127.
pub fn palette_color(index: i32) -> ColorEx {
    if (70..=96).contains(&index) {
        return DawColor::color_entry_at(index - 69);
    }
    let [r, g, b] = DEFAULT_PALETTE[index as usize];
    ColorEx::from_rgb(r, g, b)
}

fn registered_color(cm: &ColorManager, color_index: i32) -> ColorEx {
    let (r, g, b) = cm.color(color_index);
    ColorEx::new(r, g, b)
}

/// The display color of a pad for a color index (former `getColor (colorIndex, PADx)`): the
/// palette color, black for negative indices.
pub fn pad_color(cm: &ColorManager, color_index: i32) -> ColorEx {
    if color_index < 0 {
        return ColorEx::BLACK;
    }
    registered_color(cm, color_index)
}

/// The display color of a button for a color index (former override of
/// `getColor (colorIndex, buttonID)`). Monochrome buttons are shown in gray tones, the Push 1
/// transport buttons with their fixed LED colors, all others with the palette color.
///
/// Java throws a `ColorIndexException` for unregistered indices; all indices 0..127 are
/// registered, others return black here.
pub fn button_color(cm: &ColorManager, version: PushVersion, color_index: i32, button: ButtonId) -> ColorEx {
    if color_index < 0 {
        return ColorEx::BLACK;
    }

    if MONOCHROME_BUTTONS.contains(&button) {
        if color_index == cm.color_index(BUTTON_STATE_OFF) {
            return ColorEx::BLACK;
        }
        if color_index == cm.color_index(BUTTON_STATE_ON) {
            return ColorEx::DARK_GRAY;
        }
        // BUTTON_STATE_HI
        return ColorEx::LIGHT_GRAY;
    }

    if version == PushVersion::Version1 {
        match button {
            ButtonId::Play => return registered_color(cm, if color_index == 1 { PUSH2_COLOR2_GREY_LO } else { PUSH2_COLOR2_GREEN_HI }),
            ButtonId::Automation | ButtonId::Record => {
                return match color_index {
                    1 => ColorEx::DARK_GRAY,
                    4 => ColorEx::RED,
                    _ => ColorEx::DARK_RED,
                };
            }
            ButtonId::Mute => return if color_index == 1 { ColorEx::DARK_ORANGE } else { ColorEx::ORANGE },
            ButtonId::Solo => return if color_index == 1 { ColorEx::DARK_BLUE } else { ColorEx::BLUE },
            ButtonId::StopClip => return if color_index == 1 { ColorEx::DARK_RED } else { ColorEx::RED },
            _ => {}
        }
    }

    registered_color(cm, color_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push1_color_indices() {
        let cm = create_color_manager(PushVersion::Version1);
        assert_eq!(cm.color_index(PUSH_RED), PUSH1_COLOR_RED_HI);
        assert_eq!(cm.color_index(BUTTON_STATE_ON), 1);
        assert_eq!(cm.color_index(BUTTON_STATE_HI), 4);
        assert_eq!(cm.color_index(BUTTON_COLOR_ON), PUSH1_COLOR_GREEN_LO);
        assert_eq!(cm.color_index(PUSH_BUTTON_STATE_OVR_HI), 2);
        assert_eq!(cm.color_index(PUSH_BUTTON_STATE_MASTER_ON), 30);
        assert_eq!(cm.color_index(abstract_sequencer_view::COLOR_TRANSPOSE), PUSH1_COLOR_SCENE_YELLOW);
        assert_eq!(cm.color_index(DawColor::Red.id()), 6);
        assert_eq!(cm.color_index(DawColor::LightBlue.id()), 41);
        assert_eq!(cm.color_index(DawColor::GrayHalf.id()), PUSH1_COLOR2_GREY_MD);
        assert_eq!(cm.color_index(GRID_OFF), 0);
    }

    #[test]
    fn push2_and_push3_color_indices() {
        for version in [PushVersion::Version2, PushVersion::Version3] {
            let cm = create_color_manager(version);
            assert_eq!(cm.color_index(PUSH_RED), PUSH2_COLOR2_RED_HI);
            assert_eq!(cm.color_index(BUTTON_STATE_ON), 30);
            assert_eq!(cm.color_index(BUTTON_STATE_HI), 127);
            assert_eq!(cm.color_index(BUTTON_COLOR_ON), PUSH2_COLOR2_GREY_LO);
            assert_eq!(cm.color_index(abstract_sequencer_view::COLOR_TRANSPOSE), PUSH2_COLOR_SCENE_WHITE);
            assert_eq!(cm.color_index(abstract_session_view::COLOR_SCENE), PUSH2_COLOR2_GREEN);
            assert_eq!(cm.color_index(DawColor::ColorOff.id()), 0);
            assert_eq!(cm.color_index(DawColor::GrayHalf.id()), 70);
            assert_eq!(cm.color_index(DawColor::Red.id()), 81);
            assert_eq!(cm.color_index(DawColor::LightBlue.id()), 96);
            assert_eq!(cm.color_index(PUSH_BUTTON_STATE_LOCK_HI), PUSH2_COLOR2_RED_LO);
        }
        let push2 = create_color_manager(PushVersion::Version2);
        assert_eq!(push2.color_index(PUSH_BUTTON_STATE_MASTER_HI), 127);
        let push3 = create_color_manager(PushVersion::Version3);
        assert_eq!(push3.color_index(PUSH_BUTTON_STATE_MASTER_HI), PUSH2_COLOR2_WHITE);
    }

    #[test]
    fn palette() {
        assert_eq!(palette_color_rgb(5), [0xFF, 0x00, 0x00]);
        assert_eq!(palette_color_rgb(127), [0x4B, 0x15, 0x02]);
        // The DAW colors are at 70..96 (index - 69 into the DAW colors)
        assert_eq!(palette_color(81), DawColor::Red.color());
        assert_eq!(palette_color_rgb(96), DawColor::LightBlue.color().to_int_rgb255());
        assert_eq!(palette_color_rgb(69), [0x25, 0x00, 0xCC]);
        assert_eq!(palette_color_rgb(97), [0xB9, 0xB0, 0x00]);
    }

    #[test]
    fn display_colors() {
        let push1 = create_color_manager(PushVersion::Version1);
        let push2 = create_color_manager(PushVersion::Version2);
        assert_eq!(pad_color(&push2, 5), ColorEx::from_rgb(0xFF, 0, 0));
        assert_eq!(pad_color(&push2, -1), ColorEx::BLACK);
        assert_eq!(pad_color(&push1, 81), DawColor::Red.color());

        // Monochrome buttons
        assert_eq!(button_color(&push2, PushVersion::Version2, 30, ButtonId::Shift), ColorEx::DARK_GRAY);
        assert_eq!(button_color(&push2, PushVersion::Version2, 127, ButtonId::Shift), ColorEx::LIGHT_GRAY);
        assert_eq!(button_color(&push1, PushVersion::Version1, 1, ButtonId::Shift), ColorEx::DARK_GRAY);
        assert_eq!(button_color(&push1, PushVersion::Version1, 0, ButtonId::Shift), ColorEx::BLACK);

        // Push 1 transport buttons
        assert_eq!(button_color(&push1, PushVersion::Version1, 4, ButtonId::Record), ColorEx::RED);
        assert_eq!(button_color(&push1, PushVersion::Version1, 1, ButtonId::Mute), ColorEx::DARK_ORANGE);
        assert_eq!(button_color(&push1, PushVersion::Version1, 4, ButtonId::Play), palette_color(PUSH2_COLOR2_GREEN_HI));

        // RGB buttons on Push 2 use the palette
        assert_eq!(button_color(&push2, PushVersion::Version2, PUSH2_COLOR2_AMBER_LO, ButtonId::Mute), palette_color(PUSH2_COLOR2_AMBER_LO));
        assert_eq!(button_color(&push2, PushVersion::Version2, 4, ButtonId::Record), palette_color(4));
    }
}
