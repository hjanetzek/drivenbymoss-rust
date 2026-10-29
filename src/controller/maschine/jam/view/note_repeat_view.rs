// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note repeat configuration view of the Maschine JAM (former `NoteRepeatView`). Reaper
//! supports the note repeat length and mode but not the octaves.

use crate::controller::maschine::core::colors::*;
use crate::daw::constants::Resolution;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::impl_as_any;

/// Reaper does not support the note repeat octaves (former `Capability.NOTE_REPEAT_OCTAVES`).
const SUPPORTS_OCTAVES: bool = false;
/// Reaper supports the note repeat length (former `Capability.NOTE_REPEAT_LENGTH`).
const SUPPORTS_LENGTH: bool = true;
/// Reaper supports the note repeat mode (former `Capability.NOTE_REPEAT_MODE`).
const SUPPORTS_MODE: bool = true;

/// The pads which select the period / length (index of the resolution).
const PERIOD_PADS: [i32; 8] = [79, 80, 71, 72, 63, 64, 55, 56];
const LENGTH_PADS: [i32; 8] = [81, 82, 73, 74, 65, 66, 57, 58];

pub struct NoteRepeatView {
    name: String,
}

impl NoteRepeatView {
    pub fn new(_ctx: &Ctx) -> Self {
        NoteRepeatView { name: "Note Repeat".to_string() }
    }

    fn set_period(ctx: &mut Ctx, index: usize) {
        ctx.config.set_note_repeat_period(Resolution::ALL[index]);
        ctx.schedule(100, move |ctx| ctx.notify(&format!("Period: {}", Resolution::name_at(index))));
    }

    fn set_note_length(ctx: &mut Ctx, index: usize) {
        if SUPPORTS_LENGTH {
            ctx.config.set_note_repeat_length(Resolution::ALL[index]);
            ctx.schedule(100, move |ctx| ctx.notify(&format!("Note Length: {}", Resolution::name_at(index))));
        }
    }

    fn resolution_color(index: usize, selected: usize) -> i32 {
        let is_selected = index == selected;
        if index.is_multiple_of(2) {
            if is_selected {
                COLOR_SKY
            } else {
                COLOR_SKY_LO
            }
        } else if is_selected {
            COLOR_PINK
        } else {
            COLOR_PINK_LO
        }
    }
}

impl View for NoteRepeatView {
    fn name(&self) -> &str {
        &self.name
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let (period, length) = {
            let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
            (note_repeat.period, note_repeat.note_length)
        };
        let octaves = ctx.config.note_repeat_octave();
        let grid = ctx.surface.grid();

        for i in 0..8 {
            let color = if SUPPORTS_OCTAVES {
                if i == octaves {
                    COLOR_GREEN
                } else {
                    COLOR_GREY
                }
            } else {
                COLOR_BLACK
            };
            grid.light(36 + i, color);
        }
        for i in 44..52 {
            grid.light(i, COLOR_BLACK);
        }

        let period_index = Resolution::get_match(period);
        for (i, pad) in PERIOD_PADS.iter().enumerate() {
            grid.light(*pad, Self::resolution_color(i, period_index));
        }

        let length_index = Resolution::get_match(length);
        for (i, pad) in LENGTH_PADS.iter().enumerate() {
            grid.light(*pad, if SUPPORTS_LENGTH { Self::resolution_color(i, length_index) } else { COLOR_BLACK });
        }

        for i in (52..55).chain(59..63).chain(67..71) {
            grid.light(i, COLOR_BLACK);
        }
        grid.light(75, COLOR_BLACK);
        let mode_color = if SUPPORTS_MODE { COLOR_LIME } else { COLOR_BLACK };
        grid.light(76, mode_color);
        grid.light(77, mode_color);
        grid.light(78, COLOR_BLACK);
        for i in 83..100 {
            grid.light(i, COLOR_BLACK);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        match note {
            36..=43 => {
                if SUPPORTS_OCTAVES {
                    let octave = note - 36;
                    ctx.config.set_note_repeat_octave(octave);
                    ctx.schedule(100, move |ctx| ctx.notify(&format!("Note Repeat Octave: {octave}")));
                }
            }
            76 | 77 => {
                if SUPPORTS_MODE {
                    ctx.config.set_prev_next_note_repeat_mode(note == 77);
                    ctx.schedule(100, |ctx| {
                        let name = ctx.config.note_repeat_mode().map(|m| m.name().to_string()).unwrap_or_default();
                        ctx.notify(&format!("Note Repeat Mode: {name}"));
                    });
                }
            }
            _ => {
                if let Some(index) = PERIOD_PADS.iter().position(|p| *p == note) {
                    Self::set_period(ctx, index);
                } else if let Some(index) = LENGTH_PADS.iter().position(|p| *p == note) {
                    Self::set_note_length(ctx, index);
                }
            }
        }
    }

    impl_as_any!();
}
