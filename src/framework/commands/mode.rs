// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Mode commands (former `command.continuous.KnobRowModeCommand` and
//! `command.trigger.mode`: `ButtonRowModeCommand`, `ModeSelectCommand`,
//! `ModeMultiSelectCommand`). The Java `ModeCursorCommand` is
//! [`super::cursor::CursorCommand::new_mode_cursor`].
//!
//! All commands work on a mode manager of the context (see [`Ctx::mode_manager`]); the default
//! is the main mode manager.

use super::mv_helper;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::ctx::{Ctx, ModeManagerId, MAIN_MODE_MANAGER};
use crate::framework::ids::Modes;
use crate::midi::ButtonEvent;

/// Hands the value of a knob to the active mode (former `KnobRowModeCommand`).
pub struct KnobRowModeCommand {
    pub index: usize,
    pub manager: ModeManagerId,
}

impl KnobRowModeCommand {
    pub fn new(index: usize) -> Self {
        KnobRowModeCommand { index, manager: MAIN_MODE_MANAGER }
    }

    pub fn with_manager(manager: ModeManagerId, index: usize) -> Self {
        KnobRowModeCommand { index, manager }
    }
}

impl ContinuousCommand for KnobRowModeCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if let Some(mode) = ctx.active_mode_in(self.manager) {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.on_knob_value(ctx, self.index, value);
            }
        }
    }
}

/// Hands a button event to the active mode (former `ButtonRowModeCommand`).
pub struct ButtonRowModeCommand {
    pub row: usize,
    pub index: usize,
    pub manager: ModeManagerId,
}

impl ButtonRowModeCommand {
    /// Uses the main mode manager.
    pub fn new(row: usize, index: usize) -> Self {
        ButtonRowModeCommand { row, index, manager: MAIN_MODE_MANAGER }
    }

    /// Former `ButtonRowModeCommand (modeManager, row, index, ..)`.
    pub fn with_manager(manager: ModeManagerId, row: usize, index: usize) -> Self {
        ButtonRowModeCommand { row, index, manager }
    }
}

impl TriggerCommand for ButtonRowModeCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if let Some(mode) = ctx.active_mode_in(self.manager) {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.on_button(ctx, self.row, self.index, event);
            }
        }
    }
}

/// The name of the active mode of the manager.
fn active_mode_name(ctx: &Ctx, manager: ModeManagerId) -> Option<String> {
    let mode = ctx.active_mode_in(manager)?;
    let name = mode.try_borrow().ok()?.name().to_string();
    Some(name)
}

/// Selects a mode (former `ModeSelectCommand`).
pub struct ModeSelectCommand {
    pub manager: ModeManagerId,
    pub mode_id: Modes,
    pub toggle: bool,
    pub notify: bool,
    pub is_temporary: bool,
}

impl ModeSelectCommand {
    /// Former `ModeSelectCommand (model, surface, modeId)`.
    pub fn new(mode_id: Modes) -> Self {
        Self::with_options(MAIN_MODE_MANAGER, mode_id, false, true, false)
    }

    /// Former `ModeSelectCommand (model, surface, modeId, toggle)`.
    pub fn with_toggle(mode_id: Modes, toggle: bool) -> Self {
        Self::with_options(MAIN_MODE_MANAGER, mode_id, toggle, true, false)
    }

    /// Former `ModeSelectCommand (modeManager, model, surface, modeId, toggle, notify, isTemporary)`.
    pub fn with_options(manager: ModeManagerId, mode_id: Modes, toggle: bool, notify: bool, is_temporary: bool) -> Self {
        ModeSelectCommand { manager, mode_id, toggle, notify, is_temporary }
    }

    /// Former `displayMode`.
    pub fn display_mode(&self, ctx: &mut Ctx) {
        if self.notify {
            if let Some(name) = active_mode_name(ctx, self.manager) {
                ctx.notify(&name);
            }
        }
    }
}

impl TriggerCommand for ModeSelectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.mode_manager(self.manager).is_active(self.mode_id) {
            if !self.toggle {
                return;
            }
            ctx.restore_mode_in(self.manager);
        } else if self.is_temporary {
            ctx.set_temporary_mode_in(self.manager, self.mode_id);
        } else {
            ctx.set_active_mode_in(self.manager, self.mode_id);
        }
        self.display_mode(ctx);
    }
}

/// Is the mode a send mode (former `Modes.isSendMode`)?
pub fn is_send_mode(mode: Modes) -> bool {
    mode >= Modes::Send1 && mode <= Modes::Send8
}

/// Is the mode a layer send mode (former `Modes.isLayerSendMode`)?
pub fn is_layer_send_mode(mode: Modes) -> bool {
    mode >= Modes::DeviceLayerSend1 && mode <= Modes::DeviceLayerSend8
}

/// Selects the next (previous with Shift) mode of a list on button release (former
/// `ModeMultiSelectCommand`).
pub struct ModeMultiSelectCommand {
    pub manager: ModeManagerId,
    mode_ids: Vec<Modes>,
    current_mode_id: Modes,
    /// If false, the selected mode is not notified (former `shouldNotify`).
    pub notify: bool,
}

impl ModeMultiSelectCommand {
    pub fn new(mode_ids: &[Modes]) -> Self {
        Self::with_manager(MAIN_MODE_MANAGER, mode_ids)
    }

    pub fn with_manager(manager: ModeManagerId, mode_ids: &[Modes]) -> Self {
        assert!(!mode_ids.is_empty(), "ModeMultiSelectCommand needs at least one mode");
        ModeMultiSelectCommand { manager, mode_ids: mode_ids.to_vec(), current_mode_id: mode_ids[0], notify: true }
    }

    pub fn current_mode_id(&self) -> Modes {
        self.current_mode_id
    }

    /// Former `switchMode`.
    pub fn switch_mode(&mut self, ctx: &mut Ctx, select_next: bool, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let active_mode_id = ctx.mode_manager(self.manager).active_id();
        let mut new_mode = self.current_mode_id;
        // If coming from a mode not on the list, activate the last one
        if Some(self.current_mode_id) == active_mode_id {
            let mut index = self.mode_ids.iter().position(|m| *m == self.current_mode_id).unwrap_or(0);
            let start_index = index;
            // Select the previous/next mode. Skips send modes for which no send exists.
            loop {
                index = self.change_index(select_next, index);
                new_mode = self.mode_ids[index];
                if !Self::send_does_not_exist(ctx, new_mode) || index == start_index {
                    break;
                }
            }
        }
        self.activate_mode(ctx, new_mode);
    }

    fn send_does_not_exist(ctx: &Ctx, mode: Modes) -> bool {
        if is_send_mode(mode) {
            return !ctx.model.track_bank().can_edit_send(mode.ordinal() - Modes::Send1.ordinal());
        }
        if is_layer_send_mode(mode) {
            return !ctx.model.track_bank().can_edit_send(mode.ordinal() - Modes::DeviceLayerSend1.ordinal());
        }
        false
    }

    fn change_index(&self, select_next: bool, index: usize) -> usize {
        let num = self.mode_ids.len();
        if select_next {
            if index + 1 >= num {
                0
            } else {
                index + 1
            }
        } else if index == 0 || index > num {
            num - 1
        } else {
            index - 1
        }
    }

    /// Former `activateMode`.
    pub fn activate_mode(&mut self, ctx: &mut Ctx, mode_id: Modes) {
        self.current_mode_id = mode_id;
        ctx.set_active_mode_in(self.manager, mode_id);
        if !self.notify {
            return;
        }
        if is_send_mode(mode_id) {
            mv_helper::notify_selected_send(ctx, mode_id.ordinal() - Modes::Send1.ordinal());
        } else if let Some(mode) = ctx.mode_manager(self.manager).get(mode_id) {
            let name = mode.try_borrow().map(|m| m.name().to_string()).unwrap_or_default();
            ctx.notify(&name);
        }
    }
}

impl TriggerCommand for ModeMultiSelectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.switch_mode(ctx, true, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.switch_mode(ctx, false, event);
    }
}
