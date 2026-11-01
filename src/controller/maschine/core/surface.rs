// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The parts of the control surface which are shared by all Maschine models (former
//! `AbstractMaschineControlSurface` and `MaschinePadGrid`).

use super::maschine::Maschine;
use crate::framework::color::ColorManager;
use crate::framework::ctx::Ctx;
use crate::framework::grid::{BlinkMode, PadGrid};
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// Create the pad grid of a Maschine (former `MaschinePadGrid`), which emulates blinking.
pub fn create_pad_grid(colors: &ColorManager, rows: i32, cols: i32) -> PadGrid {
    PadGrid::new(colors, rows, cols, 36).with_blink_mode(BlinkMode::Emulated)
}

/// The button which is used as the Shift button (former `isShiftPressed`): the models without a
/// usable Shift button emulate it with the Stop button (on the Studio it is the Shift button
/// which is bound to the 'Navigate Back' button). `has_shift` is [`Maschine::has_shift`], or for
/// the Mk2 true when dbm-nihost sends its Shift button (not in the Java version).
pub fn shift_button(maschine: Maschine, has_shift: bool) -> ButtonId {
    if has_shift || maschine == Maschine::Studio {
        ButtonId::Shift
    } else {
        ButtonId::Stop
    }
}

/// Handle incoming system exclusive data (former `handleSysEx`).
pub fn handle_sysex(ctx: &mut Ctx, maschine: Maschine, data: &str) {
    if maschine.message_shift_down().eq_ignore_ascii_case(data) {
        ctx.trigger_button(ButtonId::Shift, ButtonEvent::Down, 1.0);
        return;
    }

    if maschine.message_shift_up().eq_ignore_ascii_case(data) {
        ctx.trigger_button(ButtonId::Shift, ButtonEvent::Up, 0.0);
        return;
    }

    if maschine.message_return_from_host().eq_ignore_ascii_case(data) {
        force_flush(ctx);
        return;
    }

    ctx.host.error(&format!("Unhandled Sysex message: {data}"));
}

/// Force sending all states to the device on the next flush (former `forceFlush`).
pub fn force_flush(ctx: &mut Ctx) {
    if let Some(display) = &mut ctx.text_display {
        display.force_flush();
    }
    ctx.surface.clear_cache();
}

/// Send the collected pad states (former `MaschinePadGrid.flush`, called in `flushHardware`).
pub fn flush_pad_grid(ctx: &mut Ctx) {
    let now = ctx.host.now();
    let surface = &mut ctx.surface;
    if let Some(grid) = &mut surface.pad_grid {
        grid.flush_blinking(&surface.output, now);
    }
}
