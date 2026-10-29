// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Application commands (former `framework.command.trigger.application`).

use crate::daw::bank::BankItem;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// Command to undo the last action, redo with Shift (former `UndoCommand`).
#[derive(Default)]
pub struct UndoCommand;

impl UndoCommand {
    pub fn new() -> Self {
        UndoCommand
    }
}

impl TriggerCommand for UndoCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.application_mut().undo();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.application_mut().redo();
        }
    }
}

/// Command to duplicate the clip in the selected slot (former `DuplicateCommand`). With Select a
/// new scene is created, with Shift a scene from the playing launcher clips.
#[derive(Default)]
pub struct DuplicateCommand;

impl DuplicateCommand {
    pub fn new() -> Self {
        DuplicateCommand
    }

    /// Handles the Select combination, returns true if it was handled.
    pub fn handle_select(ctx: &mut Ctx, event: ButtonEvent) -> bool {
        if !ctx.surface.is_select_pressed() {
            return false;
        }
        ctx.surface.set_trigger_consumed(ButtonId::Select);
        if event == ButtonEvent::Up {
            let length = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
            ctx.model.project_mut().create_scene(length);
        }
        true
    }

    /// The normal duplicate function (former `executeNormal`).
    pub fn duplicate(ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }

        // Is there a selected track?
        if !ctx.model.cursor_track().does_exist() {
            return;
        }

        // Is there a selected slot?
        let Some(index) = selected_slot_index(ctx) else {
            return;
        };
        let slot = ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index);
        let is_playing = slot.is_playing();

        // Duplicate the clip in the selected slot
        slot.duplicate();

        if !is_playing {
            return;
        }

        // Need to wait a bit with starting the duplicated clip until it is selected
        ctx.schedule(200, |ctx| {
            if let Some(index) = selected_slot_index(ctx) {
                ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index).launch(true, false);
                return;
            }

            // Try to find the clip in the next page (clips are scrolled via the scene bank)...
            ctx.model.scene_bank_mut().scroll_page_forwards();
            ctx.schedule(200, |ctx| {
                if let Some(index) = selected_slot_index(ctx) {
                    ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index).launch(true, false);
                }
            });
        });
    }
}

/// The index of the selected slot in the page of the cursor track.
pub fn selected_slot_index(ctx: &Ctx) -> Option<usize> {
    ctx.model.cursor_track().slot_bank().selected_item().and_then(|slot| usize::try_from(slot.index()).ok())
}

impl TriggerCommand for DuplicateCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if Self::handle_select(ctx, event) {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            self.execute_shifted(ctx, event);
        } else {
            self.execute_normal(ctx, event);
        }
    }

    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        Self::duplicate(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.project_mut().create_scene_from_playing_launcher_clips();
        }
    }
}
