// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The APC40 specific parts of the control surface (former `APCControlSurface` and
//! `APCPadGrid`) and the controller state, which is stored in `ctx.ext` ([`ApcState`]).

use std::rc::Rc;

use super::definition::ApcDefinition;
use crate::framework::color::ColorManager;
use crate::framework::commands::channel::Timeout;
use crate::framework::ctx::Ctx;
use crate::framework::grid::PadGrid;

// MIDI Notes
pub const APC_BUTTON_RECORD_ARM: i32 = 0x30;
pub const APC_BUTTON_SOLO: i32 = 0x31;
pub const APC_BUTTON_ACTIVATOR: i32 = 0x32;
pub const APC_BUTTON_TRACK_SELECTION: i32 = 0x33;
pub const APC_BUTTON_CLIP_STOP: i32 = 0x34;
pub const APC_BUTTON_CLIP_LAUNCH_1: i32 = 0x35;
pub const APC_BUTTON_CLIP_LAUNCH_5: i32 = 0x39;
pub const APC_BUTTON_CLIP_TRACK: i32 = 0x3A;
pub const APC_BUTTON_DEVICE_ON_OFF: i32 = 0x3B;
pub const APC_BUTTON_DEVICE_LEFT: i32 = 0x3C;
pub const APC_BUTTON_DEVICE_RIGHT: i32 = 0x3D;
pub const APC_BUTTON_DETAIL_VIEW: i32 = 0x3E;
pub const APC_BUTTON_REC_QUANT: i32 = 0x3F;
pub const APC_BUTTON_MIDI_OVERDUB: i32 = 0x40;
pub const APC_BUTTON_METRONOME: i32 = 0x41;
/// Mk2 only.
pub const APC_BUTTON_A_B: i32 = 0x42;
pub const APC_BUTTON_MASTER: i32 = 0x50;
pub const APC_BUTTON_STOP_ALL_CLIPS: i32 = 0x51;
pub const APC_BUTTON_SCENE_LAUNCH_1: i32 = 0x52;
pub const APC_BUTTON_PAN: i32 = 0x57;
pub const APC_BUTTON_SEND_A: i32 = 0x58;
pub const APC_BUTTON_SEND_B: i32 = 0x59;
pub const APC_BUTTON_SEND_C: i32 = 0x5A;
pub const APC_BUTTON_PLAY: i32 = 0x5B;
pub const APC_BUTTON_STOP: i32 = 0x5C;
pub const APC_BUTTON_RECORD: i32 = 0x5D;
pub const APC_BUTTON_UP: i32 = 0x5E;
pub const APC_BUTTON_DOWN: i32 = 0x5F;
pub const APC_BUTTON_RIGHT: i32 = 0x60;
pub const APC_BUTTON_LEFT: i32 = 0x61;
pub const APC_BUTTON_SHIFT: i32 = 0x62;
pub const APC_BUTTON_TAP_TEMPO: i32 = 0x63;
pub const APC_BUTTON_NUDGE_PLUS: i32 = 0x64;
pub const APC_BUTTON_NUDGE_MINUS: i32 = 0x65;
/// Mk2 only.
pub const APC_BUTTON_SESSION: i32 = 0x66;
/// Mk2 only.
pub const APC_BUTTON_BANK: i32 = 0x67;

// MIDI CC
pub const APC_KNOB_TRACK_LEVEL: i32 = 0x07;
/// Mk2 only.
pub const APC_KNOB_TEMPO: i32 = 0x0D;
pub const APC_KNOB_MASTER_LEVEL: i32 = 0x0E;
pub const APC_KNOB_CROSSFADER: i32 = 0x0F;
pub const APC_KNOB_DEVICE_KNOB_1: i32 = 0x10;
pub const APC_KNOB_DEVICE_KNOB_LED_1: i32 = 0x18;
pub const APC_KNOB_CUE_LEVEL: i32 = 0x2F;
pub const APC_KNOB_TRACK_KNOB_1: i32 = 0x30;
pub const APC_KNOB_TRACK_KNOB_LED_MODE_1: i32 = 0x38;
pub const APC_FOOTSWITCH_1: i32 = 0x40;
pub const APC_FOOTSWITCH_2: i32 = 0x43;

pub const LED_MODE_SINGLE: i32 = 1;
pub const LED_MODE_VOLUME: i32 = 2;
pub const LED_MODE_PAN: i32 = 3;

/// The controller specific state.
pub struct ApcState {
    pub definition: ApcDefinition,
    /// The last values sent to the knob LEDs (former `knobCache`), -1 if not sent yet.
    knob_cache: Rc<std::cell::RefCell<[i32; 128]>>,
    /// The timeout to close the tempo view (former `APCTapTempoCommand.getTimeout`).
    pub tempo_timeout: Timeout,
}

impl ApcState {
    pub fn new(definition: ApcDefinition) -> Self {
        ApcState { definition, knob_cache: Rc::new(std::cell::RefCell::new([-1; 128])), tempo_timeout: Timeout::new(500) }
    }
}

pub fn state(ctx: &Ctx) -> &ApcState {
    ctx.ext::<ApcState>()
}

/// Former `isMkII`.
pub fn is_mk2(ctx: &Ctx) -> bool {
    state(ctx).definition.is_mk2
}

/// Set the value of a knob LED; the value is sent after 100ms, if it did not change in between
/// (former `setLED`).
pub fn set_led(ctx: &mut Ctx, knob: i32, value: i32) {
    let cache = state(ctx).knob_cache.clone();
    let index = (knob & 0x7F) as usize;
    if cache.borrow()[index] == value {
        return;
    }
    cache.borrow_mut()[index] = value;
    ctx.schedule(100, move |ctx| {
        if cache.borrow()[index] == value {
            ctx.surface.output.send_cc(knob, value);
        }
    });
}

/// The grid note to controller translation of the Mk1 (former `APCPadGrid.translateToController`):
/// the tracks are on the MIDI channels 1-8, the rows are the notes 0x35-0x39.
fn translate_mk1(note: i32) -> (i32, i32) {
    let n = note - 36;
    (n % 8, 0x39 - n / 8)
}

/// The grid note to controller translation of the Mk2: the notes 0-39 on MIDI channel 1.
fn translate_mk2(note: i32) -> (i32, i32) {
    (0, note - 36)
}

/// Create the 5x8 pad grid (former `APCPadGrid`).
pub fn create_pad_grid(colors: &ColorManager, is_mk2: bool) -> PadGrid {
    PadGrid::new(colors, 5, 8, 36).with_translation(if is_mk2 { translate_mk2 } else { translate_mk1 })
}

/// Create the pads (former `createPads` with `APCPadGrid.sendBlinkState`): the Mk2 sends the
/// blink color on MIDI channel 11 (slow) or 13 (fast), the Mk1 on the channel of the pad.
pub fn create_pads(ctx: &mut Ctx) {
    let is_mk2 = is_mk2(ctx);
    super::super::create_grid_pads(
        ctx,
        Rc::new(move |ctx: &Ctx, channel, note, info| {
            let output = &ctx.surface.output;
            output.send_note_ex(channel, note, info.color.max(0));
            let blink_color = info.blink_color;
            if blink_color > 0 && blink_color < 128 {
                if is_mk2 {
                    output.send_note_ex(if info.fast { 12 } else { 10 }, note, blink_color);
                } else {
                    output.send_note_ex(channel, note, blink_color);
                }
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation() {
        assert_eq!(translate_mk1(36), (0, 0x39));
        assert_eq!(translate_mk1(36 + 39), (7, 0x35));
        assert_eq!(translate_mk2(36 + 39), (0, 39));
    }
}
