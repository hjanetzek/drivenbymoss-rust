// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Maschine Mk2/Mk3/Mikro/Plus/Studio specific parts of the control surface (former
//! `MaschineControlSurface`).

use std::rc::Rc;

use crate::controller::maschine::core::maschine::Maschine;
use crate::framework::ctx::Ctx;
use crate::midi::{BindType, MidiOutput};

// MIDI CC
pub const TOUCHSTRIP: i32 = 1;
pub const TOUCHSTRIP_TOUCH: i32 = 2;

pub const FOOTSWITCH1_TIP: i32 = 3;
pub const FOOTSWITCH1_RING: i32 = 4;
pub const FOOTSWITCH2_TIP: i32 = 5;
pub const FOOTSWITCH2_RING: i32 = 6;

pub const ENCODER: i32 = 7;
pub const ENCODER_PUSH: i32 = 8;
pub const ENCODER_TOUCH: i32 = 9;

/// Mode buttons are from CC 22 to 29.
pub const MODE_BUTTON_1: i32 = 22;
pub const MODE_BUTTON_2: i32 = 23;
pub const MODE_BUTTON_3: i32 = 24;
pub const MODE_BUTTON_4: i32 = 25;
pub const MODE_BUTTON_5: i32 = 26;
pub const MODE_BUTTON_6: i32 = 27;
pub const MODE_BUTTON_7: i32 = 28;
pub const MODE_BUTTON_8: i32 = 29;

pub const CURSOR_UP: i32 = 30;
pub const CURSOR_RIGHT: i32 = 31;
pub const CURSOR_DOWN: i32 = 32;
pub const CURSOR_LEFT: i32 = 33;

pub const GROUP: i32 = 34;
pub const AUTO: i32 = 35;
pub const LOCK: i32 = 36;
pub const NOTE_REPEAT: i32 = 37;

pub const PROJECT: i32 = 38;
pub const FAVORITES: i32 = 39;
pub const BROWSER: i32 = 40;

pub const CHANNEL: i32 = 41;
pub const ARRANGER: i32 = 42;
pub const MIXER: i32 = 43;

pub const VOLUME: i32 = 44;
pub const PLUGIN: i32 = 45;
pub const SWING: i32 = 46;
pub const SAMPLING: i32 = 47;
pub const TEMPO: i32 = 48;

pub const PITCH: i32 = 49;
pub const MOD: i32 = 50;
pub const PERFORM: i32 = 51;
pub const NOTES: i32 = 52;

pub const RESTART: i32 = 53;
pub const ERASE: i32 = 54;
pub const TAP_METRO: i32 = 55;
pub const FOLLOW: i32 = 56;
pub const PLAY: i32 = 57;
pub const REC: i32 = 58;
pub const STOP: i32 = 59;

/// Mode button touch events are from CC 60 to 67.
pub const MODE_KNOB_TOUCH_1: i32 = 60;

/// Mode knobs are from CC 70 to 77.
pub const MODE_KNOB_1: i32 = 70;

pub const FIXED_VEL: i32 = 80;
pub const PAD_MODE: i32 = 81;
pub const KEYBOARD: i32 = 82;
pub const CHORDS: i32 = 84;
pub const STEP: i32 = 83;
pub const SCENE: i32 = 85;
pub const PATTERN: i32 = 86;
pub const EVENTS: i32 = 87;
pub const VARIATION: i32 = 88;
pub const DUPLICATE: i32 = 89;
pub const SELECT: i32 = 90;
pub const SOLO: i32 = 91;
pub const MUTE: i32 = 92;

/// Banks are from CC 100 to 107.
pub const BANK_1: i32 = 100;

pub const PAGE_LEFT: i32 = 110;
pub const PAGE_RIGHT: i32 = 111;

// Specific Maschine Studio controls

pub const EDIT_COPY: i32 = 112;
pub const EDIT_PASTE: i32 = 113;
pub const EDIT_NOTE: i32 = 114;
pub const EDIT_NUDGE: i32 = 115;
pub const EDIT_UNDO: i32 = 116;
pub const EDIT_REDO: i32 = 117;
pub const EDIT_QUANTIZE: i32 = 118;
pub const EDIT_CLEAR: i32 = 119;

pub const MONITOR_IN1: i32 = 120;
pub const MONITOR_IN2: i32 = 121;
pub const MONITOR_IN3: i32 = 122;
pub const MONITOR_IN4: i32 = 123;
pub const MONITOR_MST: i32 = 124;
pub const MONITOR_GRP: i32 = 125;
pub const MONITOR_SND: i32 = 126;
pub const MONITOR_CUE: i32 = 127;
pub const MONITOR_ENCODER: i32 = 10;

pub const NAVIGATE_BACK: i32 = 11;
pub const METRO: i32 = 14;

/// The Maschine specific state of a controller instance (stored in [`Ctx::ext`]).
pub struct MaschineExt {
    pub maschine: Maschine,
    /// The last value sent to the touch strip LEDs (former `MaschineControlSurface.ribbonValue`).
    surface_ribbon_value: i32,
    /// The value of the touch strip (former `TouchstripCommand.ribbonValue`).
    pub ribbon_value: i32,
    /// The stop function should not be called on button release (former
    /// `MaschineStopCommand.consumed`).
    pub stop_consumed: bool,
    /// The main encoder controls the last touched parameter (former
    /// `MainKnobRowModeCommand.controlLastParamActive`).
    pub control_last_param_active: bool,
}

impl MaschineExt {
    pub fn new(maschine: Maschine) -> Self {
        MaschineExt { maschine, surface_ribbon_value: -1, ribbon_value: 0, stop_consumed: false, control_last_param_active: false }
    }
}

/// Get the Maschine state of the context.
pub fn ext(ctx: &Ctx) -> &MaschineExt {
    ctx.ext::<MaschineExt>()
}

pub fn ext_mut(ctx: &mut Ctx) -> &mut MaschineExt {
    ctx.ext_mut::<MaschineExt>()
}

/// Get the Maschine model.
pub fn maschine(ctx: &Ctx) -> Maschine {
    ext(ctx).maschine
}

/// Signal that the stop function should not be called on button release (former
/// `setStopConsumed`).
pub fn set_stop_consumed(ctx: &mut Ctx) {
    ext_mut(ctx).stop_consumed = true;
}

/// Install the Maschine specific trigger sender: the 7th mode button is lit with a note, all
/// other buttons with CCs (former `setTrigger`).
pub fn install_trigger_sender(ctx: &mut Ctx) {
    ctx.surface.trigger_sender = Rc::new(|output: &MidiOutput, _bind_type: BindType, channel, cc, state| {
        if cc == MODE_BUTTON_7 {
            output.send_note_ex(channel, cc, state);
        } else {
            output.send_cc_ex(channel, cc, state);
        }
    });
}

/// Set the display value of the ribbon on the controller (former `setRibbonValue`).
pub fn set_ribbon_value(ctx: &mut Ctx, value: i32) {
    if ext(ctx).surface_ribbon_value == value {
        return;
    }
    ext_mut(ctx).surface_ribbon_value = value;
    ctx.surface.output.send_cc(TOUCHSTRIP, value);
}
