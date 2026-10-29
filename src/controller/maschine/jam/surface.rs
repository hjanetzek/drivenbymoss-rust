// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Maschine JAM specific parts of the control surface (former `MaschineJamControlSurface`
//! and `FaderConfig`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::controller::maschine::core::encoder_mode_manager::{EncoderModeManager, SharedEncoderModeManager};
use crate::framework::ctx::Ctx;
use crate::midi::MidiOutput;

pub const FOOTSWITCH_TIP: i32 = 3;
pub const FOOTSWITCH_RING: i32 = 4;

pub const FADER1: i32 = 8;
pub const FADER_TOUCH1: i32 = 20;

pub const SONG: i32 = 30;
pub const STEP: i32 = 31;
pub const PAD_MODE: i32 = 32;
pub const CLEAR: i32 = 33;
pub const DUPLICATE: i32 = 34;
pub const NOTE_REPEAT: i32 = 35;
pub const MACRO: i32 = 36;
pub const LEVEL: i32 = 37;
pub const AUX: i32 = 38;
pub const CONTROL: i32 = 39;
pub const AUTO: i32 = 40;
pub const NAV_UP: i32 = 41;
pub const NAV_DOWN: i32 = 42;
pub const NAV_LEFT: i32 = 43;
pub const NAV_RIGHT: i32 = 44;

pub const SELECT: i32 = 50;
pub const SWING: i32 = 51;
pub const TUNE: i32 = 52;
pub const LOCK: i32 = 53;
pub const NOTES: i32 = 54;
pub const PERFORM: i32 = 55;
pub const BROWSE: i32 = 56;
pub const KNOB_TURN: i32 = 57;
pub const KNOB_PUSH: i32 = 58;
pub const KNOB_TOUCH: i32 = 59;
pub const IN: i32 = 60;
pub const HEADPHONE: i32 = 61;
pub const MASTER: i32 = 62;
pub const GROUP: i32 = 63;
pub const STRIP_LEFT: i32 = 64;
pub const STRIP_RIGHT: i32 = 65;
pub const FOOTSWITCH: i32 = 66;

pub const GROUP_A: i32 = 70;
pub const SCENE1: i32 = 80;

pub const PLAY: i32 = 90;
pub const RECORD: i32 = 91;
pub const LEFT: i32 = 92;
pub const RIGHT: i32 = 93;
pub const TEMPO: i32 = 94;
pub const GRID: i32 = 95;
pub const SOLO: i32 = 96;
pub const MUTE: i32 = 97;

const STRIP_VALUE_OFFSET: usize = 11;

const STRIP_SETUP: [u8; 28] = [
    0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xF7,
];

const STRIP_DUAL_VALUE: [u8; 20] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4d, 0x50, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7];

/// The configuration of a touch strip (former `FaderConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaderConfig {
    pub fader_type: i32,
    pub color: i32,
    pub value: i32,
    pub dual_value: i32,
}

impl FaderConfig {
    pub const TYPE_SINGLE: i32 = 0;
    pub const TYPE_DOT: i32 = 1;
    pub const TYPE_PAN: i32 = 2;
    pub const TYPE_DUAL: i32 = 3;

    pub const fn new(fader_type: i32, color: i32, value: i32) -> Self {
        FaderConfig { fader_type, color, value, dual_value: -1 }
    }

    pub const fn dual(fader_type: i32, color: i32, value: i32, dual_value: i32) -> Self {
        FaderConfig { fader_type, color, value, dual_value }
    }
}

/// The Maschine JAM specific state of a controller instance (stored in [`Ctx::ext`]).
pub struct JamExt {
    current_fader_configs: [FaderConfig; 8],
    pub encoder_manager: SharedEncoderModeManager,
    /// The last seen active state of the browser (replaces the browser observer).
    pub browser_active: bool,
}

impl Default for JamExt {
    fn default() -> Self {
        Self::new()
    }
}

impl JamExt {
    pub fn new() -> Self {
        JamExt {
            current_fader_configs: [FaderConfig::new(-1, -1, -1); 8],
            encoder_manager: Rc::new(RefCell::new(EncoderModeManager::new())),
            browser_active: false,
        }
    }

    /// Send the configuration of the touch strips if it changed (former `setupFaders`).
    pub fn setup_faders(&mut self, output: &MidiOutput, configurations: &[FaderConfig; 8]) {
        let mut strip_setup = STRIP_SETUP;
        let mut strip_dual_values = STRIP_DUAL_VALUE;
        let mut has_setup_changed = false;
        let mut has_dual_value_changed = false;
        for (i, fader_config) in configurations.iter().enumerate() {
            let current = self.current_fader_configs[i];
            let is_dual = fader_config.fader_type == FaderConfig::TYPE_DUAL;
            if fader_config.fader_type != current.fader_type || fader_config.color != current.color {
                has_setup_changed = true;
            }
            let pos = i * 2;
            strip_setup[STRIP_VALUE_OFFSET + pos] = fader_config.fader_type as u8;
            strip_setup[STRIP_VALUE_OFFSET + 1 + pos] = fader_config.color as u8;
            if is_dual && (fader_config.dual_value != current.dual_value || has_setup_changed) {
                has_dual_value_changed = true;
            }
            if fader_config.value != current.value || has_setup_changed {
                output.send_cc(8 + i as i32, fader_config.value);
            }
            strip_dual_values[STRIP_VALUE_OFFSET + i] = fader_config.dual_value.max(0) as u8;
            self.current_fader_configs[i] = *fader_config;
        }
        if has_setup_changed {
            output.send_sysex(&strip_setup);
        }
        if has_dual_value_changed {
            output.send_sysex(&strip_dual_values);
        }
    }
}

/// The configuration to turn off all touch strips (former `EMPTY`).
pub const EMPTY: [FaderConfig; 8] = [FaderConfig::new(FaderConfig::TYPE_SINGLE, 0, 0); 8];

/// Get the Maschine JAM state of the context.
pub fn ext(ctx: &Ctx) -> &JamExt {
    ctx.ext::<JamExt>()
}

pub fn ext_mut(ctx: &mut Ctx) -> &mut JamExt {
    ctx.ext_mut::<JamExt>()
}

/// Get the shared encoder manager.
pub fn encoder_manager(ctx: &Ctx) -> SharedEncoderModeManager {
    ext(ctx).encoder_manager.clone()
}

/// Send the fader configuration via the JAM state of the context.
pub fn setup_faders(ctx: &mut Ctx, configurations: &[FaderConfig; 8]) {
    let output = ctx.surface.output.clone();
    ext_mut(ctx).setup_faders(&output, configurations);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};

    #[test]
    fn faders() {
        let backend = Rc::new(RecordingBackend::default());
        let output = MidiOutput::new(Sender::new(backend.clone()), Some(1));
        let mut ext = JamExt::new();
        let mut configs = EMPTY;
        configs[1] = FaderConfig::dual(FaderConfig::TYPE_DUAL, 5, 10, 20);
        ext.setup_faders(&output, &configs);
        let calls = backend.take_calls();
        // 8 CC values, the setup and the dual values
        assert_eq!(calls.len(), 10);
        assert!(matches!(&calls[1], BackendCall::SendMidi(1, d) if d == &vec![0xB0, 9, 10]));
        match &calls[8] {
            BackendCall::SendMidi(1, d) => {
                assert_eq!(d.len(), 28);
                assert_eq!(d[13], 3);
                assert_eq!(d[14], 5);
            }
            other => panic!("{other:?}"),
        }
        match &calls[9] {
            BackendCall::SendMidi(1, d) => assert_eq!(d[12], 20),
            other => panic!("{other:?}"),
        }
        // Nothing changed
        ext.setup_faders(&output, &configs);
        assert!(backend.take_calls().is_empty());
    }
}
