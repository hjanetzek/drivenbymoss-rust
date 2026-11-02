// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! More transport and project commands (former `RecordCommand`, `RestartCommand`,
//! `LoadCommand`, `SaveCommand`, `LoopPositionCommand` and `LoopLengthCommand`).

use crate::daw::bank::BankItem;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::commands::clip::NewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// The action of the record command, which depends on the pressed modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveAction {
    Record,
    LauncherOverdub,
    NewClip,
}

/// Starts the recording, toggles the launcher overdub or creates a new clip, depending on the
/// pressed modifier (former `RecordCommand`).
pub struct RecordCommand {
    record_trigger: Option<ButtonId>,
    launcher_overdub_trigger: Option<ButtonId>,
    new_trigger: Option<ButtonId>,
    new_command: NewCommand,
}

impl Default for RecordCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordCommand {
    /// Former `RecordCommand (model, surface)`: Shift toggles the launcher overdub, Select
    /// creates a new clip.
    pub fn new() -> Self {
        Self::with_triggers(None, Some(ButtonId::Shift), Some(ButtonId::Select))
    }

    pub fn with_triggers(record_trigger: Option<ButtonId>, launcher_overdub_trigger: Option<ButtonId>, new_trigger: Option<ButtonId>) -> Self {
        RecordCommand { record_trigger, launcher_overdub_trigger, new_trigger, new_command: NewCommand::new() }
    }

    fn active_action(&self, ctx: &Ctx) -> ActiveAction {
        let pressed = |trigger: Option<ButtonId>| trigger.is_some_and(|id| ctx.surface.is_pressed(id));
        if pressed(self.record_trigger) {
            return ActiveAction::Record;
        }
        if pressed(self.launcher_overdub_trigger) {
            return ActiveAction::LauncherOverdub;
        }
        if pressed(self.new_trigger) {
            return ActiveAction::NewClip;
        }
        if self.record_trigger.is_none() {
            return ActiveAction::Record;
        }
        if self.launcher_overdub_trigger.is_none() {
            return ActiveAction::LauncherOverdub;
        }
        ActiveAction::NewClip
    }

    /// Former `handleExecute`.
    pub fn handle_execute(ctx: &mut Ctx, is_shift_pressed: bool) {
        let flip_record = ctx.config.is_flip_record();
        if is_shift_pressed != flip_record {
            // If the selected clip is recording, stop the recording instead of toggling the
            // launcher overdub
            let recording_slot = ctx
                .model
                .current_track_bank()
                .selected_index()
                .and_then(|track| ctx.model.current_track_bank().item(track).slot_bank().selected_item().filter(|s| s.is_recording()).map(|s| (track, s.index())));
            if let Some((track, slot)) = recording_slot {
                ctx.model.current_track_bank_mut().item_mut(track).slot_bank_mut().item_mut(slot.max(0) as usize).launch(true, false);
                return;
            }
            ctx.model.toggle_launcher_overdub();
        } else {
            ctx.model.transport_mut().start_recording();
        }
    }

    /// Former `isActive`.
    pub fn is_active(&self, ctx: &Ctx) -> bool {
        match self.active_action(ctx) {
            ActiveAction::Record => ctx.model.transport().is_recording(),
            ActiveAction::LauncherOverdub => ctx.model.is_launcher_overdub(),
            ActiveAction::NewClip => false,
        }
    }
}

impl TriggerCommand for RecordCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        match self.active_action(ctx) {
            ActiveAction::Record => Self::handle_execute(ctx, false),
            ActiveAction::LauncherOverdub => Self::handle_execute(ctx, true),
            ActiveAction::NewClip => self.new_command.execute_normal(ctx, event),
        }
    }

    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        // Backwards compatibility to old interface
        if event == ButtonEvent::Down {
            Self::handle_execute(ctx, false);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            Self::handle_execute(ctx, true);
        }
    }
}

/// Starts the playback from the beginning, or stops and rewinds (former `RestartCommand`).
#[derive(Default)]
pub struct RestartCommand;

impl TriggerCommand for RestartCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.model.transport().is_playing() {
            ctx.model.transport_mut().stop_and_rewind();
        } else {
            ctx.model.transport_mut().play();
        }
    }
}

/// Opens a project (former `LoadCommand`).
#[derive(Default)]
pub struct LoadCommand;

impl TriggerCommand for LoadCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            ctx.model.project_mut().load();
        }
    }
}

/// Saves the project (former `SaveCommand`).
#[derive(Default)]
pub struct SaveCommand;

impl TriggerCommand for SaveCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            ctx.model.project_mut().save();
        }
    }
}

/// Moves the loop start with a relative knob (former `LoopPositionCommand`).
#[derive(Default)]
pub struct LoopPositionCommand;

impl ContinuousCommand for LoopPositionCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        let increase = ctx.model.value_changer().is_increase(value);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_loop_start(increase, slow);
    }
}

/// Changes the loop length with a relative knob (former `LoopLengthCommand`).
#[derive(Default)]
pub struct LoopLengthCommand;

impl ContinuousCommand for LoopLengthCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        let increase = ctx.model.value_changer().is_increase(value);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_loop_length(increase, slow);
    }
}
