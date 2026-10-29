// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The colors of the Launch Control XL (former `LaunchControlXLColorManager`).

use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::{abstract_drum_view, abstract_sequencer_view};
use crate::framework::feature::{BUTTON_COLOR_OFF, BUTTON_COLOR_ON};

pub const LAUNCHCONTROL_COLOR_BLACK: i32 = 12;
pub const LAUNCHCONTROL_COLOR_RED: i32 = 15;
pub const LAUNCHCONTROL_COLOR_RED_LO: i32 = 13;
pub const LAUNCHCONTROL_COLOR_YELLOW: i32 = 62;
pub const LAUNCHCONTROL_COLOR_YELLOW_LO: i32 = 29;
pub const LAUNCHCONTROL_COLOR_GREEN: i32 = 60;
pub const LAUNCHCONTROL_COLOR_GREEN_LO: i32 = 28;
pub const LAUNCHCONTROL_COLOR_AMBER: i32 = 63;
pub const LAUNCHCONTROL_COLOR_AMBER_LO: i32 = 30;

pub const LAUNCHCONTROL_COLOR_ON: i32 = 127;

fn rgb(color: ColorEx) -> (f64, f64, f64) {
    (color.red(), color.green(), color.blue())
}

/// Create the color manager.
pub fn create_color_manager() -> ColorManager {
    let mut cm = ColorManager::new();
    cm.register_color_index(BUTTON_COLOR_OFF, LAUNCHCONTROL_COLOR_BLACK);
    cm.register_color_index(BUTTON_COLOR_ON, LAUNCHCONTROL_COLOR_ON);

    cm.register_color_index(BUTTON_STATE_OFF, 0);
    cm.register_color_index(BUTTON_STATE_ON, 1);
    cm.register_color_index(BUTTON_STATE_HI, 127);

    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_NO_CONTENT, LAUNCHCONTROL_COLOR_GREEN_LO);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_HILITE_CONTENT, LAUNCHCONTROL_COLOR_GREEN);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED, LAUNCHCONTROL_COLOR_RED);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_MUTED_CONT, LAUNCHCONTROL_COLOR_RED_LO);
    cm.register_color_index(abstract_sequencer_view::COLOR_STEP_SELECTED, LAUNCHCONTROL_COLOR_YELLOW);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT, LAUNCHCONTROL_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_NO_CONTENT_4, LAUNCHCONTROL_COLOR_BLACK);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT, LAUNCHCONTROL_COLOR_AMBER);
    cm.register_color_index(abstract_sequencer_view::COLOR_CONTENT_CONT, LAUNCHCONTROL_COLOR_AMBER_LO);

    cm.register_color_index(abstract_drum_view::COLOR_PAD_OFF, LAUNCHCONTROL_COLOR_BLACK);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_RECORD, LAUNCHCONTROL_COLOR_RED);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_PLAY, LAUNCHCONTROL_COLOR_GREEN);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_SELECTED, LAUNCHCONTROL_COLOR_YELLOW);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_MUTED, LAUNCHCONTROL_COLOR_AMBER_LO);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_HAS_CONTENT, LAUNCHCONTROL_COLOR_YELLOW_LO);
    cm.register_color_index(abstract_drum_view::COLOR_PAD_NO_CONTENT, LAUNCHCONTROL_COLOR_BLACK);

    for i in 0..128 {
        cm.register_color(i, (0.0, 0.0, 0.0));
    }
    cm.register_color(LAUNCHCONTROL_COLOR_BLACK, (0.0, 0.0, 0.0));
    cm.register_color(LAUNCHCONTROL_COLOR_RED, rgb(DawColor::ReddishBrown.color()));
    cm.register_color(LAUNCHCONTROL_COLOR_RED_LO, rgb(ColorEx::from_rgb(39, 4, 1)));
    cm.register_color(LAUNCHCONTROL_COLOR_AMBER, rgb(DawColor::ReddishBrown.color()));
    cm.register_color(LAUNCHCONTROL_COLOR_AMBER_LO, rgb(DawColor::DarkBrown.color()));
    cm.register_color(LAUNCHCONTROL_COLOR_YELLOW, rgb(ColorEx::from_rgb(107, 105, 1)));
    cm.register_color(LAUNCHCONTROL_COLOR_YELLOW_LO, rgb(ColorEx::from_rgb(37, 36, 1)));
    cm.register_color(LAUNCHCONTROL_COLOR_GREEN, rgb(ColorEx::from_rgb(1, 104, 1)));
    cm.register_color(LAUNCHCONTROL_COLOR_GREEN_LO, rgb(ColorEx::from_rgb(1, 36, 1)));
    cm.register_color(LAUNCHCONTROL_COLOR_ON, (1.0, 1.0, 1.0));
    cm
}
