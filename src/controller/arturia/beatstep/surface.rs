// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Beatstep specific parts of the control surface (former `BeatstepControlSurface` and
//! `BeatstepPadGrid`).
//!
//! Differences to Java:
//! - `BeatstepPadGrid.lightPad` sleeps 1 ms after each sysex message to slow down the sending;
//!   the port does not (see BITWIG_DIVERGENCES.md).
//! - `BeatstepControlSurface.isShiftPressed` returns a flag which is never set, so it is always
//!   false; the only user (`TrackEditing`, tempo and play position) passes `false` (see
//!   `view::TrackEditing`).

use std::rc::Rc;

use super::definition::{self, BEATSTEP_CHANNEL, NUM_COLUMNS, START_NOTE};
use crate::framework::color::ColorManager;
use crate::framework::ctx::Ctx;
use crate::framework::grid::PadGrid;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// The grid note to controller translation (former `BeatstepPadGrid.translateToController`):
/// the pads send their notes on MIDI channel 3.
fn translate_to_controller(note: i32) -> (i32, i32) {
    (BEATSTEP_CHANNEL, note)
}

/// Create the pad grid: 2 rows of 8 pads, starting at note 36 (former `BeatstepPadGrid`).
pub fn create_pad_grid(colors: &ColorManager) -> PadGrid {
    PadGrid::new(colors, 2, NUM_COLUMNS, START_NOTE).with_translation(translate_to_controller)
}

/// Light the pad of a grid note with a color (0, 1, 16, 17) by sysex (former
/// `BeatstepPadGrid.lightPad`).
pub fn light_pad(ctx: &Ctx, note: i32, color: i32) {
    ctx.surface.output.send_sysex(&definition::light_pad_message(note, color));
}

/// Send the stored color of the pad of a grid note again. The pad LED is turned off when the
/// pad is released, which is a different color than the one of the light (former
/// `handleGridNote` and the Shift button command).
pub fn restore_pad_color(ctx: &Ctx, note: i32) {
    let color = ctx.surface.grid_ref().light_info(note).color;
    light_pad(ctx, note, color);
}

/// Create the pad buttons with their lights and MIDI bindings. The pad states are sent as sysex
/// (former `BeatstepPadGrid.sendNoteState`, the Beatstep has no blinking). The color of a
/// released pad is restored (former `BeatstepControlSurface.handleGridNote`).
pub fn create_pads(ctx: &mut Ctx) {
    crate::controller::akai::create_grid_pads(ctx, Rc::new(|ctx: &Ctx, _channel, note, info| light_pad(ctx, note, info.color)));
    let size = (2 * NUM_COLUMNS) as usize;
    for i in 0..size {
        let note = START_NOTE + i as i32;
        ctx.surface.add_button_event_handler(ButtonId::get(ButtonId::Pad1, i), ButtonEvent::Up, Rc::new(move |ctx: &mut Ctx, _event| restore_pad_color(ctx, note)));
    }
}
