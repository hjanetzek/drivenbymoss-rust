// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The touchstrip (ribbon) of the Push (former `pitchbend.TouchstripCommand`).
//!
//! The note repeat settings are read from and written to the configuration, which is the source
//! of the note repeat settings of the note input in the port (Java uses `INoteRepeat`).

use crate::config::push as config;
use crate::controller::push::surface as push;
use crate::daw::constants::Resolution;
use crate::framework::commands::general::focused_parameter;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Views};
use crate::midi::{CMD_CC, CMD_PITCHBEND};

/// The touchstrip command.
#[derive(Default)]
pub struct TouchstripCommand {
    pitch_value: i32,
}

impl TouchstripCommand {
    /// Former `onPitchbend`.
    pub fn on_pitchbend(&mut self, ctx: &mut Ctx, data1: i32, data2: i32) {
        if ctx.views.is_active(Views::Session) {
            let value = if ctx.surface.is_shift_pressed() { 63 } else { data2 };
            let daw_value = ctx.model.value_changer().to_daw_value(value);
            ctx.model.set_crossfade(daw_value);
            push::set_ribbon_value(ctx, value);
            return;
        }

        // Don't get in the way of configuration
        if ctx.surface.is_shift_pressed() {
            return;
        }

        let scaled = data2 as f64 / 127.0;

        // Check if Note Repeat is active and its settings should be changed
        let ribbon_note_repeat = config::ribbon_note_repeat(&ctx.config);
        if ctx.config.is_note_repeat_active() && ribbon_note_repeat > config::NOTE_REPEAT_OFF {
            let values = Resolution::ALL;
            let index = (scaled * (values.len() - 1) as f64).round() as usize;
            let value = values[values.len() - 1 - index.min(values.len() - 1)];
            if ribbon_note_repeat == config::NOTE_REPEAT_PERIOD {
                ctx.config.set_note_repeat_period(value);
            } else {
                ctx.config.set_note_repeat_length(value);
            }
            return;
        }

        self.handle_ribbon_mode(ctx, data1, data2);
    }

    /// Former `handleRibbonMode`.
    fn handle_ribbon_mode(&mut self, ctx: &mut Ctx, data1: i32, data2: i32) {
        let is_reset = ctx.surface.is_delete_pressed();
        if is_reset {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
        }

        let cc = config::ribbon_mode_cc_val(&ctx.config);
        match config::ribbon_mode(&ctx.config) {
            config::RIBBON_MODE_PITCH => ctx.surface.send_midi_event(CMD_PITCHBEND, data1, data2),
            config::RIBBON_MODE_CC => {
                ctx.surface.send_midi_event(CMD_CC, cc, data2);
                self.pitch_value = data2;
            }
            config::RIBBON_MODE_CC_PB => {
                if data2 > 64 {
                    ctx.surface.send_midi_event(CMD_PITCHBEND, data1, data2);
                } else if data2 < 64 {
                    ctx.surface.send_midi_event(CMD_CC, cc, 127 - data2 * 2);
                } else {
                    ctx.surface.send_midi_event(CMD_PITCHBEND, data1, data2);
                    ctx.surface.send_midi_event(CMD_CC, cc, 0);
                }
            }
            config::RIBBON_MODE_PB_CC => {
                if data2 > 64 {
                    ctx.surface.send_midi_event(CMD_CC, cc, (data2 - 64) * 2);
                } else if data2 < 64 {
                    ctx.surface.send_midi_event(CMD_PITCHBEND, data1, data2);
                } else {
                    ctx.surface.send_midi_event(CMD_PITCHBEND, data1, data2);
                    ctx.surface.send_midi_event(CMD_CC, cc, 0);
                }
            }
            config::RIBBON_MODE_FADER => {
                if is_reset {
                    ctx.model.cursor_track_mut().reset_volume();
                } else {
                    let value = ctx.model.value_changer().to_daw_value(data2);
                    ctx.model.cursor_track_mut().set_volume(value);
                }
                return;
            }
            config::RIBBON_MODE_LAST_TOUCHED => {
                let parameter = focused_parameter(ctx);
                if !parameter.is_empty() && parameter.does_exist(ctx) {
                    if is_reset {
                        parameter.reset_value(ctx);
                    } else {
                        let value = ctx.model.value_changer().to_daw_value(data2);
                        parameter.set_value(ctx, value);
                    }
                } else {
                    ctx.surface.output.send_pitchbend(0, 0, 0);
                }
                return;
            }
            // Not used
            _ => {}
        }

        ctx.surface.output.send_pitchbend(0, data1, data2);
    }

    /// Former `updateValue`: the ribbon LEDs, called at the end of each flush.
    pub fn update_value(&self, ctx: &mut Ctx) {
        if ctx.views.is_active(Views::Session) {
            let value = ctx.model.value_changer().to_midi_value(ctx.model.crossfade());
            push::set_ribbon_value(ctx, value);
            return;
        }

        // Check if Note Repeat is active and its settings should be changed
        let ribbon_note_repeat = config::ribbon_note_repeat(&ctx.config);
        if ctx.config.is_note_repeat_active() && ribbon_note_repeat > config::NOTE_REPEAT_OFF {
            let values_len = Resolution::ALL.len();
            let value = if ribbon_note_repeat == config::NOTE_REPEAT_PERIOD { ctx.config.note_repeat_period() } else { ctx.config.note_repeat_length() };
            let index = Resolution::get_match(value.value());
            push::set_ribbon_value(ctx, 127 - (index as f64 * 127.0 / (values_len - 1) as f64).round() as i32);
            return;
        }

        match config::ribbon_mode(&ctx.config) {
            config::RIBBON_MODE_CC => push::set_ribbon_value(ctx, self.pitch_value),
            config::RIBBON_MODE_FADER => {
                let value = ctx.model.value_changer().to_midi_value(ctx.model.cursor_track().volume());
                push::set_ribbon_value(ctx, value);
            }
            config::RIBBON_MODE_LAST_TOUCHED => {
                let parameter = focused_parameter(ctx);
                let value = if !parameter.is_empty() && parameter.does_exist(ctx) { ctx.model.value_changer().to_midi_value(parameter.value(ctx)) } else { 0 };
                push::set_ribbon_value(ctx, value);
            }
            _ => push::set_ribbon_value(ctx, 64),
        }
    }
}
