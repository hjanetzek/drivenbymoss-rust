// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Further trigger commands of the Launchpad (former `DeleteCommand`,
//! `LaunchpadDuplicateCommand` and `PlayAndNewCommand`).

use crate::framework::command::TriggerCommand;
use crate::framework::commands::application::DuplicateCommand;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// The Delete button. It is only used as a combination button by the views; with Shift it
/// toggles the loop (former `DeleteCommand`).
#[derive(Default)]
pub struct DeleteCommand;

impl DeleteCommand {
    pub fn new() -> Self {
        DeleteCommand
    }
}

impl TriggerCommand for DeleteCommand {
    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().toggle_loop();
        }
    }
}

/// Duplicates the selected clip; with Shift the content of the cursor clip (former
/// `LaunchpadDuplicateCommand`).
#[derive(Default)]
pub struct LaunchpadDuplicateCommand {
    duplicate: DuplicateCommand,
}

impl LaunchpadDuplicateCommand {
    pub fn new() -> Self {
        LaunchpadDuplicateCommand { duplicate: DuplicateCommand::new() }
    }
}

impl TriggerCommand for LaunchpadDuplicateCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if DuplicateCommand::handle_select(ctx, event) {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            self.execute_shifted(ctx, event);
        } else {
            self.duplicate.execute_normal(ctx, event);
        }
    }

    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.duplicate.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            if let Some(clip) = ctx.model.cursor_clip() {
                clip.duplicate_content();
            }
        }
    }
}

/// Play; with Shift a new clip is created (former `PlayAndNewCommand`).
#[derive(Default)]
pub struct PlayAndNewCommand {
    play: PlayCommand,
    new: NewCommand,
}

impl PlayAndNewCommand {
    pub fn new() -> Self {
        PlayAndNewCommand { play: PlayCommand::new(), new: NewCommand::new() }
    }
}

impl TriggerCommand for PlayAndNewCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.new.execute_normal(ctx, event);
    }
}
