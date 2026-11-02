// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Track and device commands for buttons per track channel (former `SoloCommand`, `MuteCommand`,
//! `RecArmCommand`, `CrossfadeModeCommand`, `MasterCommand`, `StopClipCommand`,
//! `StopAllClipsCommand` of `framework.command.trigger.track` and `.clip`), and some small
//! commands without their own module (former `RedoCommand`, `DeviceOnOffCommand`,
//! `PlayPositionCommand`, `Timeout`).

use std::cell::Cell;
use std::rc::Rc;

use crate::daw::bank::BankItem;
use crate::daw::track::Track;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::Modes;
use crate::midi::ButtonEvent;

/// Apply a function to the track at the index of the current track bank, or to the selected
/// track if the index is -1.
fn with_track(ctx: &mut Ctx, index: i32, f: impl FnOnce(&mut Track)) {
    let bank = ctx.model.current_track_bank_mut();
    if index < 0 {
        if let Some(track) = bank.selected_item_mut() {
            f(track);
        }
    } else {
        f(bank.item_mut(index as usize));
    }
}

/// Toggles the solo state of a track, the auto monitor with Shift (former `SoloCommand`).
pub struct SoloCommand {
    index: i32,
}

impl SoloCommand {
    /// The command for the track at the index of the page, -1 for the selected track.
    pub fn new(index: i32) -> Self {
        SoloCommand { index }
    }
}

impl TriggerCommand for SoloCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            with_track(ctx, self.index, |t| t.toggle_solo());
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            with_track(ctx, self.index, |t| t.toggle_auto_monitor());
        }
    }
}

/// Toggles the mute state of a track, the monitor with Shift (former `MuteCommand`).
pub struct MuteCommand {
    index: i32,
}

impl MuteCommand {
    /// The command for the track at the index of the page, -1 for the selected track.
    pub fn new(index: i32) -> Self {
        MuteCommand { index }
    }
}

impl TriggerCommand for MuteCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            with_track(ctx, self.index, |t| t.toggle_mute());
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            with_track(ctx, self.index, |t| t.toggle_monitor());
        }
    }
}

/// Toggles the record arm state of a track (former `RecArmCommand`). Java subclasses override
/// `executeShifted`, which does nothing here; use [`RecArmCommand::with_shifted`].
pub struct RecArmCommand {
    index: i32,
    shifted: Option<ShiftedFunction>,
}

/// The function of a button with Shift.
pub type ShiftedFunction = Box<dyn FnMut(&mut Ctx, ButtonEvent)>;

impl RecArmCommand {
    /// The command for the track at the index of the page, -1 for the selected track.
    pub fn new(index: i32) -> Self {
        RecArmCommand { index, shifted: None }
    }

    /// The command with a function for Shift + button (the overridden `executeShifted`).
    pub fn with_shifted(index: i32, shifted: impl FnMut(&mut Ctx, ButtonEvent) + 'static) -> Self {
        RecArmCommand { index, shifted: Some(Box::new(shifted)) }
    }
}

impl TriggerCommand for RecArmCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            with_track(ctx, self.index, |t| t.toggle_rec_arm());
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if let Some(shifted) = &mut self.shifted {
            shifted(ctx, event);
        }
    }
}

/// Cycles the crossfade mode of a track: A, B, AB (former `CrossfadeModeCommand`). Reaper has no
/// crossfade modes for tracks (the parameter does not exist), the command does nothing there.
pub struct CrossfadeModeCommand {
    index: usize,
}

impl CrossfadeModeCommand {
    pub fn new(index: usize) -> Self {
        CrossfadeModeCommand { index }
    }

    /// Former `executeNormal (DOWN)`.
    /// Java: `value = normalized + 0.5; setNormalizedValue (value > 1.1 ? 0 : value)`. The track
    /// model has no crossfade parameter (Reaper does not support it, its parameter does not
    /// exist and ignores the value), therefore there is nothing to do.
    pub fn cycle(_ctx: &mut Ctx, _index: usize) {}

    /// The crossfade mode of the track as displayed text ("A", "B" or "AB"); "AB" if not
    /// supported (former `getCrossfadeParameter ().getDisplayedValue ()`, which is "AB" in
    /// Reaper).
    pub fn mode(ctx: &Ctx, index: usize) -> String {
        ctx.model.current_track_bank().item(index).crossfade_parameter().map_or_else(|| "AB".to_string(), |p| p.displayed_value())
    }
}

impl TriggerCommand for CrossfadeModeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            Self::cycle(ctx, self.index);
        }
    }
}

/// Selects the master track; with Shift toggles between the instrument/audio and effect track
/// banks (former `MasterCommand`).
#[derive(Default)]
pub struct MasterCommand;

impl MasterCommand {
    pub fn new() -> Self {
        MasterCommand
    }
}

impl TriggerCommand for MasterCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.master_track_mut().select();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        ctx.model.toggle_current_track_bank();
        if ctx.model.is_effect_track_bank_active() {
            // No Sends on effect tracks
            if let Some(mode) = ctx.modes.active_id() {
                if mode >= Modes::Send1 && mode <= Modes::Send8 {
                    ctx.set_active_mode(Modes::Pan);
                }
            }
        }
        // Make sure that a track is selected
        if !ctx.model.cursor_track().does_exist() {
            ctx.model.current_track_bank_mut().item_mut(0).select();
        }
    }
}

/// Stops the clip of a track, returns to the arrangement with Shift (former `StopClipCommand`).
pub struct StopClipCommand {
    pub index: i32,
}

impl StopClipCommand {
    /// The command for the track at the index of the page, -1 for the selected track.
    pub fn new(index: i32) -> Self {
        StopClipCommand { index }
    }
}

impl TriggerCommand for StopClipCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            with_track(ctx, self.index, |track| track.stop(false));
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            with_track(ctx, self.index, |t| t.return_to_arrangement());
        }
    }
}

/// Stops all clips of the current track bank on release (former `StopAllClipsCommand`).
#[derive(Default)]
pub struct StopAllClipsCommand;

impl StopAllClipsCommand {
    pub fn new() -> Self {
        StopAllClipsCommand
    }
}

impl TriggerCommand for StopAllClipsCommand {
    fn execute(&mut self, ctx: &mut Ctx, _event: ButtonEvent, velocity: i32) {
        if velocity == 0 {
            let is_shift = ctx.surface.is_shift_pressed();
            ctx.model.current_track_bank_mut().stop(is_shift);
        }
    }
}

/// Redo the last undone action (former `RedoCommand`).
#[derive(Default)]
pub struct RedoCommand;

impl RedoCommand {
    pub fn new() -> Self {
        RedoCommand
    }
}

impl TriggerCommand for RedoCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.application_mut().redo();
        }
    }
}

/// Toggles the enabled state of the cursor device (former `DeviceOnOffCommand`).
#[derive(Default)]
pub struct DeviceOnOffCommand;

impl DeviceOnOffCommand {
    pub fn new() -> Self {
        DeviceOnOffCommand
    }
}

impl TriggerCommand for DeviceOnOffCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.cursor_device_mut().toggle_enabled_state();
        }
    }
}

/// Changes the play position with a relative knob (former `PlayPositionCommand`).
#[derive(Default)]
pub struct PlayPositionCommand;

impl PlayPositionCommand {
    pub fn new() -> Self {
        PlayPositionCommand
    }

    /// Former `execute`.
    pub fn change_position(ctx: &mut Ctx, value: i32) {
        let increase = ctx.model.value_changer().is_increase(value);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_position(increase, slow);
    }
}

impl ContinuousCommand for PlayPositionCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        Self::change_position(ctx, value);
    }
}

/// Executes a function after a timeout, which is restarted if the timeout is delayed again
/// before it expired (former `Timeout`). Clones share the timeout.
#[derive(Debug, Clone)]
pub struct Timeout {
    timeout_millis: u64,
    update_count: Rc<Cell<u64>>,
}

impl Timeout {
    pub fn new(timeout_millis: u64) -> Self {
        Timeout { timeout_millis, update_count: Rc::new(Cell::new(0)) }
    }

    /// Execute the task after the timeout, unless the timeout is delayed again before (former
    /// `delay`).
    pub fn delay(&self, ctx: &mut Ctx, task: impl FnOnce(&mut Ctx) + 'static) {
        let issued = self.update_count.get().wrapping_add(1);
        self.update_count.set(issued);
        let update_count = self.update_count.clone();
        ctx.schedule(self.timeout_millis, move |ctx| {
            if update_count.get() == issued {
                task(ctx);
            }
        });
    }
}
