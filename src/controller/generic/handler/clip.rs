// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for clip commands (former `ClipHandler`).

use super::{increase_knob_movement, is_absolute, is_button_pressed, is_increase, to_midi_value, unsupported};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::framework::clip_launcher_navigator::navigate_clips;
use crate::framework::commands::clip::NewCommand;
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let selected_slot = ctx.model.selected_slot();
    match command {
        FlexiCommand::ClipTogglePin => to_midi_value(ctx.model.cursor_clip().is_some_and(|clip| clip.is_pinned())),
        // Note: as in Java stop also returns the playing state
        FlexiCommand::ClipPlay | FlexiCommand::ClipPlayAlt | FlexiCommand::ClipStop | FlexiCommand::ClipStopAlt => to_midi_value(selected_slot.is_some_and(|slot| slot.is_playing())),
        FlexiCommand::ClipStopAll | FlexiCommand::ClipStopAllAlt => to_midi_value(true),
        FlexiCommand::ClipRecord => to_midi_value(selected_slot.is_some_and(|slot| slot.is_recording())),
        _ => -1,
    }
}

/// The index of the selected slot of the cursor track.
fn selected_slot_index(ctx: &Ctx) -> Option<usize> {
    ctx.model.cursor_track().slot_bank().selected_index()
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);
    match command {
        FlexiCommand::ClipTogglePin => {
            if is_button_pressed {
                if let Some(clip) = ctx.model.cursor_clip_mut() {
                    clip.toggle_pinned();
                }
            }
        }
        FlexiCommand::ClipPrevious => {
            if is_button_pressed {
                navigate_clips(ctx, true);
            }
        }
        FlexiCommand::ClipNext => {
            if is_button_pressed {
                navigate_clips(ctx, false);
            }
        }
        FlexiCommand::ClipScroll => scroll_clips(ctx, knob_mode, value),
        FlexiCommand::ClipPlay | FlexiCommand::ClipPlayAlt => {
            if let Some(index) = selected_slot_index(ctx) {
                ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index).launch(is_button_pressed, command == FlexiCommand::ClipPlayAlt);
            }
        }
        FlexiCommand::ClipStop | FlexiCommand::ClipStopAlt => {
            if is_button_pressed {
                ctx.model.cursor_track_mut().stop(command == FlexiCommand::ClipStopAlt);
            }
        }
        FlexiCommand::ClipStopAll | FlexiCommand::ClipStopAllAlt => {
            if is_button_pressed {
                ctx.model.track_bank_mut().stop(command == FlexiCommand::ClipStopAllAlt);
            }
        }
        FlexiCommand::ClipRecord => {
            if is_button_pressed {
                if let Some(index) = selected_slot_index(ctx) {
                    ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index).start_recording();
                }
            }
        }
        FlexiCommand::ClipNew => {
            if is_button_pressed {
                NewCommand::new().execute_now(ctx);
            }
        }
        FlexiCommand::ClipQuantize => {
            if is_button_pressed {
                if let Some(clip) = ctx.model.cursor_clip() {
                    if clip.does_exist() {
                        clip.quantize(1.0);
                    }
                }
            }
        }
        _ => unsupported(ctx, command),
    }
}

fn scroll_clips(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) || !increase_knob_movement(ctx, CommandCategory::Clip) {
        return;
    }
    let increase = is_increase(ctx, knob_mode, value);
    navigate_clips(ctx, !increase);
}
