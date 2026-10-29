// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Aftertouch command of play views (former `AftertouchViewCommand`).
//!
//! Java asks the key manager of the view for the MIDI note of a pad. Since the view is borrowed
//! while its aftertouch command runs, the Rust version uses the key translation table of the
//! surface instead, which the view creates from the same note matrix (the table maps the
//! controller note of a pad to the played MIDI note, which is exactly
//! `keyManager.getMidiNoteFromGrid`). The pressed keys for channel aftertouch are taken from the
//! pressed pad buttons.

use crate::config::values::{AFTERTOUCH_CONVERT_CHANNEL, AFTERTOUCH_CONVERT_OFF, AFTERTOUCH_CONVERT_POLY};
use crate::framework::command::AftertouchCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::{CMD_CC, CMD_CHANNEL_AFTERTOUCH, CMD_POLY_AFTERTOUCH};

/// Converts the aftertouch of the pads as configured in the settings.
#[derive(Default)]
pub struct AftertouchViewCommand;

impl AftertouchViewCommand {
    pub fn new() -> Self {
        AftertouchViewCommand
    }

    /// The MIDI note which is played by the pad with the given controller note, -1 if none.
    fn midi_note_from_grid(ctx: &Ctx, note: i32) -> i32 {
        usize::try_from(note).ok().and_then(|n| ctx.surface.key_translation_table().get(n).copied()).unwrap_or(-1)
    }

    /// The controller notes of all pressed pads.
    fn pressed_keys(ctx: &Ctx) -> Vec<i32> {
        let Some(grid) = &ctx.surface.pad_grid else {
            return Vec::new();
        };
        let size = grid.rows() * grid.cols();
        (0..size)
            .filter(|i| ctx.surface.is_pressed(ButtonId::get(ButtonId::Pad1, *i as usize)))
            .map(|i| grid.translate_to_controller(grid.start_note() + i).1)
            .collect()
    }
}

impl AftertouchCommand for AftertouchViewCommand {
    fn on_poly_aftertouch(&mut self, ctx: &mut Ctx, note: i32, value: i32) {
        let convert_aftertouch = ctx.config.convert_aftertouch();
        match convert_aftertouch {
            // Filter poly-aftertouch
            AFTERTOUCH_CONVERT_OFF => {}

            // Translate notes of poly-aftertouch to current note mapping and only allow
            // aftertouch for pads with notes
            AFTERTOUCH_CONVERT_POLY => {
                let n = Self::midi_note_from_grid(ctx, note);
                if n != -1 {
                    ctx.surface.send_midi_event(CMD_POLY_AFTERTOUCH, n, value);
                }
            }

            // Convert to channel-aftertouch
            AFTERTOUCH_CONVERT_CHANNEL => ctx.surface.send_midi_event(CMD_CHANNEL_AFTERTOUCH, value, 0),

            // MIDI CC
            cc => ctx.surface.send_midi_event(CMD_CC, cc, value),
        }
    }

    fn on_channel_aftertouch(&mut self, ctx: &mut Ctx, value: i32) {
        let convert_aftertouch = ctx.config.convert_aftertouch();
        if convert_aftertouch == AFTERTOUCH_CONVERT_OFF {
            return;
        }
        if convert_aftertouch == AFTERTOUCH_CONVERT_POLY {
            for key in Self::pressed_keys(ctx) {
                self.on_poly_aftertouch(ctx, key, value);
            }
        } else {
            self.on_poly_aftertouch(ctx, -1, value);
        }
    }
}
