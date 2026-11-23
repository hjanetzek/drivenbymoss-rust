// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The parts of the control surface which are shared by all Maschine models (former
//! `AbstractMaschineControlSurface` and `MaschinePadGrid`).

use std::rc::Rc;

use super::brightness::{LedBrightness, SharedBrightness};
use super::maschine::Maschine;
use crate::config::SettingId;
use crate::framework::color::ColorManager;
use crate::framework::ctx::Ctx;
use crate::framework::grid::{BlinkMode, PadGrid};
use crate::framework::ids::ButtonId;
use crate::midi::{BindType, ButtonEvent, MidiOutput};

/// Create the pad grid of a Maschine (former `MaschinePadGrid`), which emulates blinking. The
/// pad colors are sent with the LED brightness (not in the Java version).
pub fn create_pad_grid(colors: &ColorManager, rows: i32, cols: i32, brightness: &SharedBrightness) -> PadGrid {
    let brightness = brightness.clone();
    PadGrid::new(colors, rows, cols, 36).with_blink_mode(BlinkMode::Emulated).with_color_mapper(Rc::new(move |color| brightness.get().rgb(color)))
}

/// Create the shared LED brightness with the value of the setting.
pub fn create_brightness(config: &crate::config::Configuration, id: SettingId) -> SharedBrightness {
    Rc::new(std::cell::Cell::new(LedBrightness::from_config(config, id)))
}

/// Install the trigger sender of a Maschine, which sends the button LEDs with the LED brightness
/// (not in the Java version): `is_rgb` tells which buttons have an RGB LED (a palette index),
/// all others have a single-colour LED. `as_note` tells which buttons are lit with a note.
pub fn install_trigger_sender(ctx: &mut Ctx, brightness: &SharedBrightness, is_rgb: fn(i32) -> bool, as_note: fn(BindType, i32) -> bool) {
    let brightness = brightness.clone();
    ctx.surface.trigger_sender = Rc::new(move |output: &MidiOutput, bind_type: BindType, channel, cc, value| {
        let brightness = brightness.get();
        let value = if is_rgb(cc) { brightness.rgb(value) } else { brightness.mono(value) };
        if as_note(bind_type, cc) {
            output.send_note_ex(channel, cc, value);
        } else {
            output.send_cc_ex(channel, cc, value);
        }
    });
}

/// Apply a change of the LED brightness setting: re-send all LEDs if it changed. Returns true if
/// it changed (the JAM re-sends its touch strips as well).
pub fn apply_brightness(ctx: &mut Ctx, brightness: &SharedBrightness, id: SettingId) -> bool {
    let value = LedBrightness::from_config(&ctx.config, id);
    if brightness.get() == value {
        return false;
    }
    brightness.set(value);
    ctx.surface.clear_cache();
    true
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
