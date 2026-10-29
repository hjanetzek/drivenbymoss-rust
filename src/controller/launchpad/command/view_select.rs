// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The view selection buttons of the Launchpad (former `SelectDeviceViewCommand`,
//! `SelectMainViewCommand`, `SelectSessionViewCommand` and `ProjectCommand`).

use std::collections::HashSet;

use crate::controller::launchpad::colors::{LAUNCHPAD_COLOR_GREY_LO, LAUNCHPAD_COLOR_TURQUOISE};
use crate::controller::launchpad::surface::{self as lp};
use crate::daw::bank::BankItem;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Views};
use crate::midi::ButtonEvent;

/// Notify the name of the active view.
fn notify_active_view_name(ctx: &mut Ctx) {
    let name = ctx.active_view().and_then(|view| view.try_borrow().ok().map(|v| v.name().to_string()));
    if let Some(name) = name {
        ctx.notify(&name);
    }
}

/// Toggles between the Device and the User view; the Tempo view with Shift on the Pro models
/// (former `SelectDeviceViewCommand`).
#[derive(Default)]
pub struct SelectDeviceViewCommand;

impl SelectDeviceViewCommand {
    pub fn new() -> Self {
        SelectDeviceViewCommand
    }

    /// The color of the Device button.
    pub fn button_color(ctx: &Ctx) -> i32 {
        if ctx.views.is_any_active(&[Views::Device, Views::User]) {
            LAUNCHPAD_COLOR_TURQUOISE
        } else {
            LAUNCHPAD_COLOR_GREY_LO
        }
    }
}

impl TriggerCommand for SelectDeviceViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }

        // Switch to tempo view on Pro with Shift+Device
        if lp::is_pro(ctx) && ctx.surface.is_shift_pressed() {
            if ctx.views.is_active(Views::Shift) {
                ctx.restore_view();
            }
            ctx.set_temporary_view(Views::Tempo);
            notify_active_view_name(ctx);
            return;
        }

        // Toggle between device and user parameters mode
        let view = if ctx.views.is_active(Views::Device) { Views::User } else { Views::Device };
        ctx.set_active_view(view);
        notify_active_view_name(ctx);
    }
}

/// Selects the note/sequencer view selection or recalls the preferred view of the track (former
/// `SelectMainViewCommand`).
pub struct SelectMainViewCommand {
    all_view_ids: HashSet<Views>,
}

impl SelectMainViewCommand {
    pub fn new(all_view_ids: &[Views]) -> Self {
        SelectMainViewCommand { all_view_ids: all_view_ids.iter().copied().collect() }
    }
}

impl TriggerCommand for SelectMainViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }

        if ctx.views.is_active(Views::Control) {
            ctx.restore_view();
            notify_active_view_name(ctx);
            return;
        }

        if ctx.views.active_id().is_some_and(|id| self.all_view_ids.contains(&id)) {
            ctx.set_active_view(Views::Control);
            ctx.notify("Note / sequencer mode selection");
            return;
        }

        if !ctx.model.cursor_track().does_exist() {
            ctx.set_active_view(Views::Session);
            ctx.notify("Session");
            return;
        }

        ctx.recall_preferred_view();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemporaryMode {
    Off,
    Possible,
    Active,
}

/// Selects the Session view; with Shift the Mix/Scene Play view. A long press activates the
/// birds-eye view, pressing it again flips the session (former `SelectSessionViewCommand`).
///
/// `SelectSessionViewCommand.setTemporary` is [`set_session_view_temporary`]: the flag is stored
/// in the Launchpad state of the context, since the session view cannot access the command.
pub struct SelectSessionViewCommand {
    temporary_mode: TemporaryMode,
    last_view: Views,
}

impl Default for SelectSessionViewCommand {
    fn default() -> Self {
        Self::new()
    }
}

/// Former `SelectSessionViewCommand.setTemporary`: the session view was used while the Session
/// button was held, the previous view is restored when the button is released.
pub fn set_session_view_temporary(ctx: &mut Ctx) {
    lp::ext_mut(ctx).session_temporary = true;
}

impl SelectSessionViewCommand {
    pub fn new() -> Self {
        SelectSessionViewCommand { temporary_mode: TemporaryMode::Off, last_view: Views::Mix }
    }

    fn sync_temporary(&mut self, ctx: &mut Ctx) {
        let ext = lp::ext_mut(ctx);
        if ext.session_temporary {
            ext.session_temporary = false;
            self.temporary_mode = TemporaryMode::Active;
        }
    }

    fn activated_view(&mut self, ctx: &mut Ctx) {
        // Activate session view
        self.temporary_mode = TemporaryMode::Possible;
        ctx.set_active_view(Views::Session);
        let is_flipped = ctx.config.is_flip_session();
        Self::notify_view_name(ctx, Views::Session, false, is_flipped);
    }

    fn notify_view_name(ctx: &mut Ctx, active_view: Views, is_birds_eye: bool, is_flipped: bool) {
        // Launchpad sometimes do not update correctly, force a flush
        if let Some(grid) = &mut ctx.surface.pad_grid {
            grid.force_flush();
        }

        if active_view == Views::Mix {
            ctx.notify("Mix");
            return;
        }

        if is_birds_eye {
            ctx.notify("Session - Birds Eye");
        } else {
            ctx.notify(if is_flipped { "Session - Flipped" } else { "Session" });
        }
    }

    fn session_view_birds_eye(ctx: &Ctx) -> bool {
        ctx.views.get(Views::Session).is_some_and(|view| view.try_borrow().is_ok_and(|v| v.is_birds_eye_active()))
    }

    fn set_session_view_birds_eye(ctx: &Ctx, is_active: bool) {
        if let Some(view) = ctx.views.get(Views::Session) {
            if let Ok(mut view) = view.try_borrow_mut() {
                view.set_birds_eye_active(is_active);
            }
        }
    }
}

impl TriggerCommand for SelectSessionViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        match event {
            ButtonEvent::Down => {
                self.temporary_mode = TemporaryMode::Off;
                lp::ext_mut(ctx).session_temporary = false;

                // Display Mix or Scene Play view with Shift
                if ctx.surface.is_shift_pressed() {
                    ctx.surface.set_trigger_consumed(ButtonId::Session);
                    if ctx.views.is_any_active(&[Views::Mix, Views::ScenePlay]) {
                        self.last_view = if ctx.views.is_active(Views::Mix) { Views::ScenePlay } else { Views::Mix };
                    }
                    ctx.set_active_view(self.last_view);
                    Self::notify_view_name(ctx, self.last_view, false, false);
                }
            }

            ButtonEvent::Long => {
                self.sync_temporary(ctx);

                // Only trigger birds-eye-view if session view is already active
                if ctx.views.is_active(Views::Session) {
                    if self.temporary_mode != TemporaryMode::Possible {
                        ctx.surface.set_trigger_consumed(ButtonId::Session);
                        Self::set_session_view_birds_eye(ctx, true);
                        let is_flipped = ctx.config.is_flip_session();
                        Self::notify_view_name(ctx, Views::Session, true, is_flipped);
                    }
                    return;
                }
                self.activated_view(ctx);
            }

            ButtonEvent::Up => {
                self.sync_temporary(ctx);

                if self.temporary_mode == TemporaryMode::Active {
                    ctx.restore_view();
                    return;
                }

                if self.temporary_mode == TemporaryMode::Possible {
                    return;
                }

                if ctx.views.is_active(Views::Session) {
                    // Disable birds-eye-view if active
                    if Self::session_view_birds_eye(ctx) {
                        Self::set_session_view_birds_eye(ctx, false);
                        let is_flipped = ctx.config.is_flip_session();
                        Self::notify_view_name(ctx, Views::Session, false, is_flipped);
                        return;
                    }

                    // Flip clips
                    let flipped = !ctx.config.is_flip_session();
                    ctx.config.set_flip_session(flipped);
                    Self::notify_view_name(ctx, Views::Session, false, flipped);
                    return;
                }

                self.activated_view(ctx);
            }
        }
    }
}

/// Selects the Project view, saves the project with Shift (former `ProjectCommand`).
#[derive(Default)]
pub struct ProjectCommand;

impl ProjectCommand {
    pub fn new() -> Self {
        ProjectCommand
    }
}

impl TriggerCommand for ProjectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.set_active_view(Views::Project);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.project_mut().save();
        }
    }
}
