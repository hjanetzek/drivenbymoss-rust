// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Transport commands (former `framework.command.trigger.transport` and
//! `AbstractDoubleTriggerCommand`).

use std::cell::Cell;
use std::rc::Rc;

use super::application::selected_slot_index;
use super::clip::NewCommand;
use crate::config::{RecordFunction, TransportBehavior};
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// The time in which a second click counts as a double click.
pub const DOUBLE_CLICK_DELAY: u64 = 250;

/// Detects double clicks (former `AbstractDoubleTriggerCommand`). The flag is shared with the
/// scheduled task which resets it.
#[derive(Default, Clone)]
pub struct DoubleClick {
    restart_flag: Rc<Cell<bool>>,
}

impl DoubleClick {
    /// Returns true and resets the state if the button was clicked shortly before.
    pub fn take(&self) -> bool {
        self.restart_flag.replace(false)
    }

    /// Start the double click test (former `doubleClickTest`).
    pub fn start(&self, ctx: &mut Ctx) {
        self.restart_flag.set(true);
        let flag = self.restart_flag.clone();
        ctx.schedule(DOUBLE_CLICK_DELAY, move |_| flag.set(false));
    }
}

/// Command to start/stop playback (former `PlayCommand`). With the Select button punch in/out
/// is toggled, with Shift the loop. A double click returns to zero (if enabled in the settings).
pub struct PlayCommand {
    select_button: ButtonId,
    double_click: DoubleClick,
}

impl Default for PlayCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayCommand {
    pub fn new() -> Self {
        Self::with_select_button(ButtonId::Select)
    }

    pub fn with_select_button(select_button: ButtonId) -> Self {
        PlayCommand { select_button, double_click: DoubleClick::default() }
    }

    fn execute_single_click(&mut self, ctx: &mut Ctx) {
        // Handle the different options when the playback is stopped
        let transport = ctx.model.transport_mut();
        match ctx.config.behaviour_on_pause() {
            TransportBehavior::ReturnToZero => {
                if transport.is_playing() {
                    transport.stop_and_rewind();
                } else {
                    transport.play();
                }
            }
            TransportBehavior::Stop => {
                if transport.is_playing() {
                    transport.stop();
                } else {
                    transport.play();
                }
                self.double_click.start(ctx);
            }
            TransportBehavior::Pause => {
                transport.play();
                self.double_click.start(ctx);
            }
        }
    }

    fn execute_double_click(&mut self, ctx: &mut Ctx) {
        if ctx.config.is_play_double_click_return_to_zero() {
            ctx.model.transport_mut().stop_and_rewind();
        }
    }

    fn handle_button_combinations(&mut self, ctx: &mut Ctx) -> bool {
        if ctx.surface.is_pressed(self.select_button) {
            ctx.surface.set_trigger_consumed(self.select_button);
            ctx.model.transport_mut().toggle_punch_in();
            return true;
        }
        false
    }
}

impl TriggerCommand for PlayCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up || self.handle_button_combinations(ctx) {
            return;
        }
        if self.double_click.take() {
            self.execute_double_click(ctx);
            return;
        }
        self.execute_single_click(ctx);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_pressed(self.select_button) {
            ctx.surface.set_trigger_consumed(self.select_button);
            ctx.model.transport_mut().toggle_punch_out();
        } else {
            ctx.model.transport_mut().toggle_loop();
        }
    }
}

/// Command to toggle the metronome (former `MetronomeCommand`). With Shift either tap tempo or
/// toggle the metronome ticks.
pub struct MetronomeCommand {
    tap_with_shift: bool,
}

impl MetronomeCommand {
    pub fn new(tap_with_shift: bool) -> Self {
        MetronomeCommand { tap_with_shift }
    }
}

impl TriggerCommand for MetronomeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.transport_mut().toggle_metronome();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if self.tap_with_shift {
            ctx.model.transport_mut().tap_tempo();
        } else {
            ctx.model.transport_mut().toggle_metronome_ticks();
        }
    }
}

/// Whether the record command uses the shifted or the normal function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShiftMode {
    NoShift,
    Shifted,
    CalcShift,
}

/// Command to execute the record function configured in the settings (former
/// `ConfiguredRecordCommand`).
pub struct ConfiguredRecordCommand {
    shift_mode: ShiftMode,
}

impl Default for ConfiguredRecordCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfiguredRecordCommand {
    /// Uses the shifted function if Shift is pressed.
    pub fn new() -> Self {
        ConfiguredRecordCommand { shift_mode: ShiftMode::CalcShift }
    }

    /// Always uses the shifted or the normal function.
    pub fn with_shift(is_shifted: bool) -> Self {
        ConfiguredRecordCommand { shift_mode: if is_shifted { ShiftMode::Shifted } else { ShiftMode::NoShift } }
    }

    fn is_shifted(&self, ctx: &Ctx) -> bool {
        match self.shift_mode {
            ShiftMode::Shifted => true,
            ShiftMode::NoShift => false,
            ShiftMode::CalcShift => ctx.surface.is_shift_pressed(),
        }
    }

    fn record_function(ctx: &Ctx, is_shifted: bool) -> RecordFunction {
        if is_shifted {
            ctx.config.shifted_record_button_function()
        } else {
            ctx.config.record_button_function()
        }
    }

    fn create_clip(ctx: &mut Ctx) {
        let Some(index) = selected_slot_index(ctx) else {
            return;
        };
        let slot = ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index);
        if !slot.is_recording() {
            slot.start_recording();
        }
        slot.launch(true, false);
    }

    /// Is the button lit with the current Shift state?
    pub fn is_lit_now(&self, ctx: &Ctx) -> bool {
        Self::is_lit(ctx, self.is_shifted(ctx))
    }

    /// Is the button lit for the normal or shifted function?
    pub fn is_lit(ctx: &Ctx, is_shifted: bool) -> bool {
        match Self::record_function(ctx, is_shifted) {
            RecordFunction::RecordArranger | RecordFunction::RecordArrangerAndEnableAutomation => ctx.model.transport().is_recording(),
            RecordFunction::NewClip
            | RecordFunction::NewClipAndEnableAutomation
            | RecordFunction::RecordClip
            | RecordFunction::RecordClipAndEnableAutomation => ctx.model.selected_slot().is_some_and(|slot| slot.is_recording()),
            RecordFunction::ToggleArrangerOverdub => ctx.model.is_arranger_overdub(),
            RecordFunction::ToggleClipOverdub => ctx.model.is_launcher_overdub(),
            RecordFunction::ToggleRecArm => ctx.model.current_track_bank().selected_item().is_some_and(|track| track.is_rec_arm()),
        }
    }
}

impl TriggerCommand for ConfiguredRecordCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }

        match Self::record_function(ctx, self.is_shifted(ctx)) {
            RecordFunction::RecordArranger => ctx.model.transport_mut().start_recording(),
            RecordFunction::RecordArrangerAndEnableAutomation => {
                let transport = ctx.model.transport_mut();
                if !transport.is_writing_arranger_automation() {
                    transport.toggle_write_arranger_automation();
                }
                transport.start_recording();
            }
            RecordFunction::RecordClip => Self::create_clip(ctx),
            RecordFunction::RecordClipAndEnableAutomation => {
                Self::create_clip(ctx);
                let transport = ctx.model.transport_mut();
                if !transport.is_writing_clip_launcher_automation() {
                    transport.toggle_write_clip_launcher_automation();
                }
            }
            RecordFunction::NewClip => NewCommand::new().execute_now(ctx),
            RecordFunction::NewClipAndEnableAutomation => {
                NewCommand::new().execute_now(ctx);
                let transport = ctx.model.transport_mut();
                if !transport.is_writing_clip_launcher_automation() {
                    transport.toggle_write_clip_launcher_automation();
                }
            }
            RecordFunction::ToggleArrangerOverdub => ctx.model.toggle_overdub(),
            RecordFunction::ToggleClipOverdub => ctx.model.toggle_launcher_overdub(),
            RecordFunction::ToggleRecArm => {
                if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
                    track.toggle_rec_arm();
                }
            }
        }
    }
}
