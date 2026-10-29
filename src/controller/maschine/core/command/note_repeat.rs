// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Command to toggle the note repeat and to show its configuration (former `NoteRepeatCommand`
//! and `FillModeNoteRepeatCommand`).

use crate::framework::command::TriggerCommand;
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::midi::ButtonEvent;

/// Toggles the note repeat. A long press (or Shift + press) shows the note repeat configuration
/// as a mode (`is_mode`) or a view; Select + press toggles the fill mode.
pub struct FillModeNoteRepeatCommand {
    is_mode: bool,
}

impl FillModeNoteRepeatCommand {
    pub fn new(is_mode: bool) -> Self {
        FillModeNoteRepeatCommand { is_mode }
    }

    /// Former `handleEditModeActivation`.
    fn handle_edit_mode_activation(&self, ctx: &mut Ctx, event: ButtonEvent) -> bool {
        if event == ButtonEvent::Long || event == ButtonEvent::Down && ctx.surface.is_shift_pressed() {
            if self.is_mode {
                ctx.set_temporary_mode(Modes::RepeatNote);
            } else {
                ctx.set_active_view(Views::RepeatNote);
            }
            ctx.surface.set_trigger_consumed(ButtonId::Repeat);
            return true;
        }

        if event != ButtonEvent::Up {
            return true;
        }

        if self.is_mode {
            if ctx.modes.is_active(Modes::RepeatNote) {
                ctx.restore_mode();
                return true;
            }
        } else if ctx.views.is_active(Views::RepeatNote) {
            ctx.restore_view();
            return true;
        }
        false
    }
}

impl TriggerCommand for FillModeNoteRepeatCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.surface.is_select_pressed() {
            if event == ButtonEvent::Down {
                ctx.surface.set_trigger_consumed(ButtonId::Select);
                ctx.model.transport_mut().toggle_fill_mode_active();
                delay_display(ctx, |ctx| format!("Fill Mode: {}", if ctx.model.transport().is_fill_mode_active() { "On" } else { "Off" }));
            }
            return;
        }
        if !self.handle_edit_mode_activation(ctx, event) {
            ctx.config.toggle_note_repeat_active();
        }
    }
}
