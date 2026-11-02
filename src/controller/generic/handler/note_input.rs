// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for note input commands: note repeat and octave transposition (former
//! `NoteInputHandler`).

use super::{is_absolute, is_button_pressed, is_increase, to_midi_value, unsupported};
use crate::config::NoteRepeatCapabilities;
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::constants::Resolution;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

/// The note repeat capabilities of the host (former `host.supports (Capability.NOTE_REPEAT_*)`).
const CAPABILITIES: NoteRepeatCapabilities = NoteRepeatCapabilities::REAPER;

/// The delay before the key translation table is updated after an octave change.
const OCTAVE_UPDATE_DELAY_MS: u64 = 6;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
    match command {
        FlexiCommand::NoteInputRepeatActive => to_midi_value(note_repeat.is_active),
        FlexiCommand::NoteInputRepeatPeriod => Resolution::get_match(note_repeat.period) as i32,
        FlexiCommand::NoteInputRepeatLength => Resolution::get_match(note_repeat.note_length) as i32,
        // The mode of the note repeat is the index in the arpeggiator modes of the host
        FlexiCommand::NoteInputRepeatMode => note_repeat.mode as i32,
        // Octaves are not supported (Reaper NoteRepeatImpl.getOctaves)
        FlexiCommand::NoteInputRepeatOctave => 0,
        FlexiCommand::NoteInputTransposeOctaveUp | FlexiCommand::NoteInputTransposeOctaveDown => ctx.model.scales().octave(),
        _ => -1,
    }
}

/// The resolution index of an absolute value.
fn absolute_resolution(val: f64) -> usize {
    let count = Resolution::ALL.len();
    ((val / 127.0 * count as f64).round() as usize).min(count - 1)
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);
    let val = if value.is_high_res() { value.value() as f64 / 128.0 } else { value.value() as f64 };

    match command {
        FlexiCommand::NoteInputRepeatActive => {
            if is_button_pressed {
                ctx.config.toggle_note_repeat_active();
                mv_helper::delay_display(ctx, |ctx| format!("Repeat: {}", if ctx.config.is_note_repeat_active() { "On" } else { "Off" }));
            }
        }
        FlexiCommand::NoteInputRepeatPeriod => {
            let sel_period = if is_absolute(knob_mode) {
                absolute_resolution(val)
            } else {
                Resolution::change(Resolution::get_match(ctx.config.note_repeat_period().value()), is_increase(ctx, knob_mode, value))
            };
            ctx.config.set_note_repeat_period(Resolution::ALL[sel_period]);
            mv_helper::delay_display(ctx, |ctx| format!("Repeat Period: {}", ctx.config.note_repeat_period().name()));
        }
        FlexiCommand::NoteInputRepeatLength => {
            if CAPABILITIES.length {
                let sel_length = if is_absolute(knob_mode) {
                    absolute_resolution(val)
                } else {
                    Resolution::change(Resolution::get_match(ctx.config.note_repeat_length().value()), is_increase(ctx, knob_mode, value))
                };
                ctx.config.set_note_repeat_length(Resolution::ALL[sel_length]);
                mv_helper::delay_display(ctx, |ctx| format!("Repeat Length: {}", ctx.config.note_repeat_length().name()));
            }
        }
        FlexiCommand::NoteInputRepeatMode => {
            if CAPABILITIES.mode {
                let modes = ctx.config.arpeggiator_modes().to_vec();
                if modes.is_empty() {
                    return;
                }
                let new_index = if is_absolute(knob_mode) {
                    if val >= modes.len() as f64 {
                        return;
                    }
                    val as usize
                } else {
                    let mode_index = ctx.config.lookup_arpeggiator_mode_index(ctx.config.note_repeat_mode()) as i64;
                    let increase = is_increase(ctx, knob_mode, value);
                    (mode_index + if increase { 1 } else { -1 }).clamp(0, modes.len() as i64 - 1) as usize
                };
                let mode = modes[new_index];
                ctx.config.set_note_repeat_mode(mode);
                mv_helper::delay_display(ctx, move |_| format!("Repeat Mode: {}", mode.name()));
            }
        }
        FlexiCommand::NoteInputRepeatOctave => {
            if CAPABILITIES.octaves {
                let octave = if is_absolute(knob_mode) {
                    val as i32
                } else {
                    ctx.config.note_repeat_octave() + if is_increase(ctx, knob_mode, value) { 1 } else { -1 }
                };
                ctx.config.set_note_repeat_octave(octave);
                mv_helper::delay_display(ctx, move |_| format!("Repeat Octave: {octave}"));
            }
        }
        FlexiCommand::NoteInputTransposeOctaveUp => {
            if is_button_pressed {
                ctx.model.scales_mut().inc_octave();
                update_octave(ctx);
            }
        }
        FlexiCommand::NoteInputTransposeOctaveDown => {
            if is_button_pressed {
                ctx.model.scales_mut().dec_octave();
                update_octave(ctx);
            }
        }
        _ => unsupported(ctx, command),
    }
}

/// Former `updateOctave`: note that this replaces the key translation table of the note map.
fn update_octave(ctx: &mut Ctx) {
    ctx.schedule(OCTAVE_UPDATE_DELAY_MS, |ctx| {
        let matrix = ctx.model.scales().note_matrix().to_vec();
        ctx.surface.set_key_translation_table(matrix);
        mv_helper::delay_display(ctx, |ctx| format!("Octave: {}", ctx.model.scales().octave()));
    });
}
