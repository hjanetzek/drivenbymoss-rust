// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The views of the Maschine Mk2/Mk3/Mikro/Plus/Studio (former package
//! `controller.ni.maschine.mk3.view`).

pub mod clip_view;
pub mod drum_view;
pub mod mute_view;
pub mod note_repeat_view;
pub mod parameter_view;
pub mod play_view;
pub mod scene_view;
pub mod select_view;
pub mod shift_view;
pub mod solo_view;

use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// Execute a function by its pad index (former `IExecuteFunction`).
pub trait ExecuteFunction {
    fn execute_function(&mut self, ctx: &mut Ctx, pad_index: i32, event: ButtonEvent);
}

/// The grid note handling of all views based on the former `BaseView`: the pads execute the
/// function with their index.
pub fn base_on_grid_note(view: &mut dyn ExecuteFunction, ctx: &mut Ctx, note: i32, velocity: i32) {
    view.execute_function(ctx, note - 36, if velocity > 0 { ButtonEvent::Down } else { ButtonEvent::Up });
}

/// The pad position (x, y) of the pad index in the 4x4 grid (y = 0 is the top row).
pub fn pad_xy(index: i32) -> (i32, i32) {
    (index % 4, 3 - index / 4)
}
