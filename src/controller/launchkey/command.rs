// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the Launchkey (former `ButtonAreaCommand`, `DeviceLockCommand` and
//! `LaunchkeyMk3PlayCommand`).

use super::colors::{LAUNCHKEY_COLOR_BLACK, LAUNCHKEY_COLOR_GREY_LO, LAUNCHKEY_COLOR_RED};
use super::surface::{state, state_mut};
use crate::daw::bank::BankItem;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::channel::RecArmCommand;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// The flag which marks a selected track in the color of [`ButtonAreaCommand::button_color`].
pub const SELECTED_FLAG: i32 = 0x1000;

/// The buttons below the faders of the Mk3: select or record arm a track, with Undo delete it
/// (former `ButtonAreaCommand`).
pub struct ButtonAreaCommand {
    column: usize,
    rec_arm_command: RecArmCommand,
}

impl ButtonAreaCommand {
    pub fn new(column: usize) -> Self {
        ButtonAreaCommand { column, rec_arm_command: RecArmCommand::new(column as i32) }
    }

    /// The color of the button; [`SELECTED_FLAG`] is added if the track is selected (former
    /// `getButtonColor`).
    pub fn button_color(ctx: &Ctx, column: usize) -> i32 {
        let t = ctx.model.current_track_bank().item(column);
        let color = if !t.does_exist() {
            LAUNCHKEY_COLOR_BLACK
        } else if state(ctx).is_select {
            ctx.colors.index_for_color(t.color())
        } else if t.is_rec_arm() {
            LAUNCHKEY_COLOR_RED
        } else {
            LAUNCHKEY_COLOR_GREY_LO
        };
        if t.is_selected() {
            SELECTED_FLAG + color
        } else {
            color
        }
    }

    /// Former static `toggleSelect`.
    pub fn toggle_select(ctx: &mut Ctx) {
        let s = state_mut(ctx);
        s.is_select = !s.is_select;
    }

    /// Former static `isSelect`.
    pub fn is_select(ctx: &Ctx) -> bool {
        state(ctx).is_select
    }
}

impl TriggerCommand for ButtonAreaCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if ctx.surface.is_pressed(ButtonId::Undo) {
            if event == ButtonEvent::Down {
                ctx.model.current_track_bank_mut().item_mut(self.column).remove();
                ctx.surface.set_trigger_consumed(ButtonId::Undo);
            }
            return;
        }
        if Self::is_select(ctx) {
            if event == ButtonEvent::Down {
                ctx.model.current_track_bank_mut().item_mut(self.column).select_or_expand_group();
            }
        } else {
            self.rec_arm_command.execute(ctx, event, velocity);
        }
    }
}

/// Pin the cursor device, with Shift open its window (former `DeviceLockCommand`).
pub struct DeviceLockCommand;

impl TriggerCommand for DeviceLockCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.cursor_device_mut().toggle_pinned();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.cursor_device_mut().toggle_window_open();
        }
    }
}

/// The play command of the Mk3: with Shift return to the arrangement (former
/// `LaunchkeyMk3PlayCommand`; `transport returnToArrangement`, which only Bitwig has).
pub struct LaunchkeyMk3PlayCommand {
    play: PlayCommand,
}

impl LaunchkeyMk3PlayCommand {
    pub fn new() -> Self {
        LaunchkeyMk3PlayCommand { play: PlayCommand::new() }
    }
}

impl Default for LaunchkeyMk3PlayCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for LaunchkeyMk3PlayCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().return_to_arrangement();
        }
    }
}
