// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Clip commands (former `framework.command.trigger.clip`).

use super::mv_helper;
use crate::daw::bank::BankItem;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// The length of the new clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewClipLength {
    /// The length configured in the settings (former `NewCommand`).
    #[default]
    Configured,
    /// A fixed length index (former `TemporaryNewCommand`): 0 = 1 beat, 1 = 2 beats, 2 = 1 bar,
    /// 3 = 2 bars, 4 = 4 bars, ...
    Fixed(i32),
}

/// Command to create a new clip on the cursor track in the next empty slot and start recording
/// (former `NewCommand` and `TemporaryNewCommand`).
#[derive(Default)]
pub struct NewCommand {
    clip_length: NewClipLength,
}

impl NewCommand {
    pub fn new() -> Self {
        NewCommand { clip_length: NewClipLength::Configured }
    }

    /// Former `TemporaryNewCommand`.
    pub fn with_fixed_length(clip_length: i32) -> Self {
        NewCommand { clip_length: NewClipLength::Fixed(clip_length) }
    }

    /// Create the clip and enable overdub (former `execute ()`).
    pub fn execute_now(&self, ctx: &mut Ctx) {
        self.handle_execute(ctx, true);
    }

    pub fn handle_execute(&self, ctx: &mut Ctx, enable_overdub: bool) {
        let Some(track) = ctx.model.cursor_track_ref().filter(|_| ctx.model.cursor_track().does_exist()) else {
            ctx.notify("Please select an Instrument track first.");
            return;
        };

        // Reaper always has an empty slot at the given index
        let slot_index = ctx.model.cursor_track().slot_bank().selected_item().map(|slot| slot.index().max(0) as usize).unwrap_or(0);
        let length = self.clip_length(ctx);
        ctx.model.create_note_clip(track, slot_index, length, enable_overdub);
    }

    /// The length of the new clip in beats.
    pub fn clip_length(&self, ctx: &Ctx) -> i32 {
        let quarters_per_measure = ctx.model.transport().quarters_per_measure();
        match self.clip_length {
            NewClipLength::Configured => ctx.config.new_clip_length_in_beats(quarters_per_measure),
            NewClipLength::Fixed(length) => {
                if length < 2 {
                    2_f64.powi(length) as i32
                } else {
                    (2_f64.powi(length - 2) * quarters_per_measure as f64) as i32
                }
            }
        }
    }
}

impl TriggerCommand for NewCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            self.handle_execute(ctx, true);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            self.handle_execute(ctx, false);
        }
    }
}

/// Command to quantize the cursor clip (former `QuantizeCommand`). With Shift (or Select) the
/// record quantization of the cursor track is changed.
#[derive(Default)]
pub struct QuantizeCommand;

impl QuantizeCommand {
    pub fn new() -> Self {
        QuantizeCommand
    }

    pub fn quantize(ctx: &mut Ctx) {
        let amount = ctx.config.quantize_amount() as f64 / 100.0;
        if let Some(clip) = ctx.model.cursor_clip() {
            if clip.does_exist() {
                clip.quantize(amount);
            }
        }
    }

    /// Toggle through all record quantization settings.
    pub fn select_record_quantization(ctx: &mut Ctx, next: bool) {
        if !ctx.model.cursor_track().does_exist() {
            return;
        }
        let track = ctx.model.cursor_track_mut();
        if next {
            track.next_record_quantization();
        } else {
            track.previous_record_quantization();
        }
        mv_helper::delay_display(ctx, |ctx| format!("Record Quantization: {}", ctx.model.cursor_track().record_quantization_grid().name()));
    }
}

impl TriggerCommand for QuantizeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Select) {
            Self::select_record_quantization(ctx, false);
        } else {
            Self::quantize(ctx);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            Self::select_record_quantization(ctx, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_lengths() {
        // The formula of TemporaryNewCommand for 4/4
        let lengths: Vec<i32> = (0..6)
            .map(|l| if l < 2 { 2_f64.powi(l) as i32 } else { (2_f64.powi(l - 2) * 4.0) as i32 })
            .collect();
        assert_eq!(lengths, vec![1, 2, 4, 8, 16, 32]);
        assert_eq!(NewCommand::with_fixed_length(3).clip_length, NewClipLength::Fixed(3));
    }
}
