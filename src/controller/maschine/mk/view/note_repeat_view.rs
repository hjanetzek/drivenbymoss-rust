// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note repeat view (former `NoteRepeatView`).

use super::{base_on_grid_note, ExecuteFunction};
use crate::controller::maschine::core::colors::{COLOR_BLACK, COLOR_BLUE, COLOR_GREEN, COLOR_ROSE, COLOR_YELLOW};
use crate::daw::constants::Resolution;
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Edit the note repeat settings with the pads.
pub struct NoteRepeatView;

impl NoteRepeatView {
    pub fn new(_ctx: &Ctx) -> Self {
        NoteRepeatView
    }
}

impl ExecuteFunction for NoteRepeatView {
    fn execute_function(&mut self, ctx: &mut Ctx, pad_index: i32, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match pad_index {
            // Reaper supports the note repeat mode
            6 | 7 => {
                let modes = ctx.config.arpeggiator_modes().to_vec();
                if modes.is_empty() {
                    return;
                }
                let mode_index = ctx.config.lookup_arpeggiator_mode_index(ctx.config.note_repeat_mode()) as i64;
                let increase = pad_index == 7;
                let new_index = (mode_index + if increase { 1 } else { -1 }).clamp(0, modes.len() as i64 - 1) as usize;
                ctx.config.set_note_repeat_mode(modes[new_index]);
                delay_display(ctx, |ctx| format!("Mode: {}", ctx.config.note_repeat_mode().map(|m| m.name()).unwrap_or_default()));
            }
            // Reaper supports the note repeat length
            8 | 9 => {
                let sel = Resolution::change(Resolution::get_match(ctx.config.note_repeat_length().value()), pad_index == 9);
                ctx.config.set_note_repeat_length(Resolution::ALL[sel]);
                ctx.schedule(100, move |ctx| ctx.notify(&format!("Note Length: {}", Resolution::name_at(sel))));
            }
            12 | 13 => {
                let sel = Resolution::change(Resolution::get_match(ctx.config.note_repeat_period().value()), pad_index == 13);
                ctx.config.set_note_repeat_period(Resolution::ALL[sel]);
                delay_display(ctx, move |_ctx| format!("Period: {}", Resolution::name_at(sel)));
            }
            // Note repeat octaves (14, 15) are not supported by Reaper
            _ => {}
        }
    }
}

impl View for NoteRepeatView {
    fn name(&self) -> &str {
        "Note Repeat"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let grid = ctx.surface.grid();
        let colors = [(36, 42, COLOR_BLACK), (42, 44, COLOR_BLUE), (44, 46, COLOR_GREEN), (46, 48, COLOR_BLACK), (48, 50, COLOR_ROSE), (50, 52, COLOR_YELLOW)];
        for (from, to, color) in colors {
            for i in from..to {
                grid.light(i, color);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        base_on_grid_note(self, ctx, note, velocity);
    }

    impl_as_any!();
}
