// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Generic commands first needed by the Mackie MCU and HUI: `NopCommand`, `ShiftCommand`,
//! `SaveCommand`, `MarkerCommand`, `RecordCommand` (the simple one with the modifier buttons),
//! `AutomationModeCommand`, `PunchInCommand`, `PunchOutCommand`, `ToggleTrackBanksCommand`, the
//! track commands `SelectCommand`, `RecArmCommand`, `SoloCommand`, `MuteCommand`,
//! `RecArmAllCommand`, and the continuous `JogWheelCommand`.

use std::cell::Cell;
use std::rc::Rc;

use super::clip::NewCommand;
use crate::daw::bank::BankItem;
use crate::daw::constants::AutomationMode;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::parameterprovider::ParamRef;
use crate::framework::value_changer::ValueChanger;
use crate::midi::ButtonEvent;

/// Does nothing (former `NopCommand`), e.g. for modifier buttons.
pub struct NopCommand;

impl TriggerCommand for NopCommand {
    fn execute(&mut self, _ctx: &mut Ctx, _event: ButtonEvent, _velocity: i32) {}
}

/// Switches the knob sensitivity to slow while Shift is pressed (former `ShiftCommand`).
pub struct ShiftCommand;

impl TriggerCommand for ShiftCommand {
    fn execute(&mut self, ctx: &mut Ctx, _event: ButtonEvent, _velocity: i32) {
        let is_slow = ctx.surface.is_shift_pressed();
        ctx.surface.set_knob_sensitivity_is_slow(is_slow);
    }
}

/// Saves the project (former `SaveCommand`).
pub struct SaveCommand;

impl TriggerCommand for SaveCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            ctx.model.project_mut().save();
        }
    }
}

/// Toggles the marker mode, adds a marker with Select, toggles the cue markers with Shift
/// (former `MarkerCommand`).
pub struct MarkerCommand;

impl TriggerCommand for MarkerCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_select_pressed() {
            ctx.model.marker_bank_mut().add_marker();
            return;
        }
        if ctx.modes.is_active(Modes::Markers) {
            ctx.restore_mode();
        } else {
            ctx.set_active_mode(Modes::Markers);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.arranger_mut().toggle_cue_marker_visibility();
        }
    }
}

/// Starts the arranger recording, toggles the launcher overdub with Shift or creates a new clip
/// with Select (former `RecordCommand` with the default triggers: none for record, Shift for
/// launcher overdub, Select for new).
pub struct RecordCommand {
    new_command: NewCommand,
}

impl Default for RecordCommand {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq)]
enum ActiveAction {
    Record,
    LauncherOverdub,
    NewClip,
}

impl RecordCommand {
    pub fn new() -> Self {
        RecordCommand { new_command: NewCommand::new() }
    }

    fn active_action(ctx: &Ctx) -> ActiveAction {
        if ctx.surface.is_pressed(ButtonId::Shift) {
            return ActiveAction::LauncherOverdub;
        }
        if ctx.surface.is_pressed(ButtonId::Select) {
            return ActiveAction::NewClip;
        }
        // No record trigger: record is the default
        ActiveAction::Record
    }

    /// Former `handleExecute`.
    pub fn handle_execute(ctx: &mut Ctx, is_shift_pressed: bool) {
        let flip_record = ctx.config.is_flip_record();
        if is_shift_pressed != flip_record {
            // If the selected clip is recording, stop the recording instead of toggling the
            // launcher overdub
            let recording_slot = ctx.model.current_track_bank().selected_index().and_then(|track| {
                let slots = ctx.model.current_track_bank().item(track).slot_bank();
                slots.selected_item().filter(|s| s.is_recording()).map(|s| (track, s.index().max(0) as usize))
            });
            if let Some((track, slot)) = recording_slot {
                ctx.model.current_track_bank_mut().item_mut(track).slot_bank_mut().item_mut(slot).launch(true, false);
                return;
            }
            ctx.model.toggle_launcher_overdub();
        } else {
            ctx.model.transport_mut().start_recording();
        }
    }

    /// Former `isActive`: the state of the active action.
    pub fn is_active(ctx: &Ctx) -> bool {
        match Self::active_action(ctx) {
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
        match Self::active_action(ctx) {
            ActiveAction::Record => Self::handle_execute(ctx, false),
            ActiveAction::LauncherOverdub => Self::handle_execute(ctx, true),
            ActiveAction::NewClip => self.new_command.execute_normal(ctx, event),
        }
    }
}

/// Sets an automation write mode, resets the automation overrides with Select (former
/// `AutomationModeCommand`).
pub struct AutomationModeCommand {
    mode: AutomationMode,
    notify: bool,
}

impl AutomationModeCommand {
    pub fn new(mode: AutomationMode, notify: bool) -> Self {
        AutomationModeCommand { mode, notify }
    }
}

impl TriggerCommand for AutomationModeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_select_pressed() {
            ctx.model.transport_mut().reset_automation_overrides();
            return;
        }
        ctx.model.transport_mut().set_automation_write_mode(self.mode);
        if self.notify {
            let name: String = self.mode.name().chars().take(4).collect();
            ctx.notify(&name);
        }
    }
}

/// Toggles punch in (former `PunchInCommand`).
pub struct PunchInCommand;

impl TriggerCommand for PunchInCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().toggle_punch_in();
        }
    }
}

/// Toggles punch out (former `PunchOutCommand`).
pub struct PunchOutCommand;

impl TriggerCommand for PunchOutCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().toggle_punch_out();
        }
    }
}

/// Switches between the instrument/audio and the effect track bank (former
/// `ToggleTrackBanksCommand`).
pub struct ToggleTrackBanksCommand;

impl ToggleTrackBanksCommand {
    /// Former `executeNormal`.
    pub fn toggle(ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        ctx.model.toggle_current_track_bank();
        let message = if ctx.model.is_effect_track_bank_active() { "Effect Tracks" } else { "Audio & Instrument Tracks" };
        ctx.notify(message);
        if ctx.model.current_track_bank().selected_item().is_none() {
            ctx.model.current_track_bank_mut().item_mut(0).select();
        }
    }
}

impl TriggerCommand for ToggleTrackBanksCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        Self::toggle(ctx, event);
    }
}

/// Selects the track at the index of the current track bank page (former
/// `track.SelectCommand`).
pub struct SelectTrackCommand {
    pub index: usize,
}

impl TriggerCommand for SelectTrackCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.current_track_bank_mut().item_mut(self.index).select();
        }
    }
}

/// The track state a [`TrackStateCommand`] toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackState {
    RecArm,
    Solo,
    Mute,
}

/// Toggles rec arm, solo or mute of the track at the index of the current track bank page on
/// release (former `track.RecArmCommand`, `SoloCommand`, `MuteCommand`; with Shift solo toggles
/// the auto monitor, mute the monitor and rec arm does nothing).
pub struct TrackStateCommand {
    pub index: usize,
    pub state: TrackState,
}

impl TriggerCommand for TrackStateCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let track = ctx.model.current_track_bank_mut().item_mut(self.index);
        match self.state {
            TrackState::RecArm => track.toggle_rec_arm(),
            TrackState::Solo => track.toggle_solo(),
            TrackState::Mute => track.toggle_mute(),
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let track = ctx.model.current_track_bank_mut().item_mut(self.index);
        match self.state {
            TrackState::RecArm => {}
            TrackState::Solo => track.toggle_auto_monitor(),
            TrackState::Mute => track.toggle_monitor(),
        }
    }
}

/// Toggles rec arm of all tracks of the page (former `RecArmAllCommand`).
pub struct RecArmAllCommand;

impl TriggerCommand for RecArmAllCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.current_track_bank_mut().toggle_rec_arm();
        }
    }
}

/// The last touched parameter of the cursor device (former `IFocusedParameter` of the Reaper
/// model: the unpaged parameter at `getLastTouchedParameterIndex`), `ParamRef::Empty` if there
/// is none.
pub fn focused_parameter(ctx: &Ctx) -> ParamRef {
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return ParamRef::Empty;
    }
    let index = device.last_touched_parameter_index();
    let count = device.parameter_bank().map(|b| b.unpaged_item_count()).unwrap_or(0);
    if index >= 0 && (index as usize) < count {
        ParamRef::BankPosition { bank: crate::daw::events::ParameterBankId::CursorDevice, position: index as usize }
    } else {
        ParamRef::Empty
    }
}

/// Moves the play position, controls the last touched parameter or changes tempo, loop, zoom
/// and track height with modifier buttons (former `JogWheelCommand`).
pub struct JogWheelCommand {
    /// Shared with the commands which toggle it (e.g. the MCU assignable buttons).
    pub control_last_param_active: Rc<Cell<bool>>,
    value_changer: ValueChanger,
    slow_down_counter: i32,
}

impl JogWheelCommand {
    pub fn new(value_changer: ValueChanger, control_last_param_active: Rc<Cell<bool>>) -> Self {
        JogWheelCommand { control_last_param_active, value_changer, slow_down_counter: 1 }
    }
}

impl ContinuousCommand for JogWheelCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        // Slow down option for fast encoders
        let slow_down = ctx.config.encoder_knob_slow_down();
        if slow_down > 0 {
            if self.slow_down_counter % (slow_down + 1) == 0 {
                self.slow_down_counter = 1;
            } else {
                self.slow_down_counter += 1;
                return;
            }
        }

        // Control the last touched/clicked parameter
        if self.control_last_param_active.get() {
            let parameter = focused_parameter(ctx);
            if parameter.does_exist(ctx) {
                let factor = if ctx.surface.is_shift_pressed() { 10.0 } else { 50.0 };
                let increment = self.value_changer.calc_knob_change(value) * factor;
                if let Some(param) = crate::framework::parameterprovider::resolve_mut(&mut ctx.model, &parameter) {
                    param.inc(increment);
                }
            }
            return;
        }

        let increase = self.value_changer.is_increase(value);
        let slow = ctx.surface.is_knob_sensitivity_slow();

        // Scroll results in Browser
        if let Some(browser) = ctx.model.browser_mut().filter(|b| b.is_active()) {
            if increase {
                browser.select_next_result();
            } else {
                browser.select_previous_result();
            }
            return;
        }

        // Change tempo with Select button
        if ctx.surface.is_pressed(ButtonId::Select) {
            ctx.model.transport_mut().change_tempo(increase, slow);
            return;
        }
        // Change Loop start with Control button
        if ctx.surface.is_pressed(ButtonId::Control) {
            ctx.model.transport_mut().change_loop_start(increase, slow);
            return;
        }
        // Change Loop length with ALT button
        if ctx.surface.is_pressed(ButtonId::Alt) {
            ctx.model.transport_mut().change_loop_length(increase, slow);
            return;
        }
        // Zoom Arranger with Left/Right Cursor button
        if ctx.surface.is_pressed(ButtonId::ArrowLeft) || ctx.surface.is_pressed(ButtonId::ArrowRight) {
            ctx.surface.set_trigger_consumed(ButtonId::ArrowLeft);
            ctx.surface.set_trigger_consumed(ButtonId::ArrowRight);
            if increase {
                ctx.model.application_mut().zoom_in();
            } else {
                ctx.model.application_mut().zoom_out();
            }
            return;
        }
        // Increase/decrease track heights with Up/Down Cursor button
        if ctx.surface.is_pressed(ButtonId::ArrowUp) || ctx.surface.is_pressed(ButtonId::ArrowDown) {
            ctx.surface.set_trigger_consumed(ButtonId::ArrowUp);
            ctx.surface.set_trigger_consumed(ButtonId::ArrowDown);
            if increase {
                ctx.model.application_mut().inc_track_height();
            } else {
                ctx.model.application_mut().dec_track_height();
            }
            return;
        }
        ctx.model.transport_mut().change_position(increase, slow);
    }
}

/// The last touched parameter of the cursor device, resolved at each access (former
/// `FocusedParameterImpl` of the Reaper model), e.g. to bind it to a fader.
pub struct FocusedParameter;

impl crate::framework::parameterprovider::CustomParameter for FocusedParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool {
        focused_parameter(ctx).does_exist(ctx)
    }

    fn name(&self, ctx: &Ctx) -> String {
        focused_parameter(ctx).name(ctx)
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        focused_parameter(ctx).value(ctx)
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        focused_parameter(ctx).displayed_value(ctx)
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        focused_parameter(ctx).set_value(ctx, value);
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        focused_parameter(ctx).set_normalized_value(ctx, value);
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        focused_parameter(ctx).change_value(ctx, control);
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        focused_parameter(ctx).reset_value(ctx);
    }

    fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        focused_parameter(ctx).touch_value(ctx, is_touched);
    }
}

/// Zooms the arranger horizontally with a relative value (former `ZoomParameter` with
/// `isHorizontal`). Difference to Java: setting an absolute value does nothing (Java throws an
/// `UnsupportedOperationException`).
pub struct ZoomParameter;

impl crate::framework::parameterprovider::CustomParameter for ZoomParameter {
    fn does_exist(&self, _ctx: &Ctx) -> bool {
        true
    }

    fn name(&self, _ctx: &Ctx) -> String {
        "Zoom In/Out".to_string()
    }

    fn value(&self, _ctx: &Ctx) -> i32 {
        0
    }

    fn set_value(&self, _ctx: &mut Ctx, _value: i32) {}

    fn set_normalized_value(&self, _ctx: &mut Ctx, _value: f64) {}

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        if ctx.model.value_changer().is_increase(control) {
            ctx.model.application_mut().zoom_in();
        } else {
            ctx.model.application_mut().zoom_out();
        }
    }
}
