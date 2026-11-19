// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Edit commands: delete the selection and create scenes (former `DeleteCommand` and
//! `CreateSceneCommand`).

use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// Deletes the selection of the DAW on release (former generic `DeleteCommand`). The note
/// mapping of the active view is updated for the key combinations.
#[derive(Default)]
pub struct DeleteCommand;

impl DeleteCommand {
    pub fn new() -> Self {
        DeleteCommand
    }
}

impl TriggerCommand for DeleteCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        // Update for key combinations
        ctx.update_view_note_mapping();
        if event == ButtonEvent::Up {
            ctx.model.application_mut().delete_selection();
        }
    }
}

/// Creates a new scene, with Shift a scene from the playing launcher clips (former
/// `CreateSceneCommand`).
#[derive(Default)]
pub struct CreateSceneCommand;

impl CreateSceneCommand {
    pub fn new() -> Self {
        CreateSceneCommand
    }
}

impl TriggerCommand for CreateSceneCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            let length = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
            ctx.model.project_mut().create_scene(length);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.project_mut().create_scene_from_playing_launcher_clips();
        }
    }
}
