// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the APC40 (former `controller.akai.apc.command`).

use std::cell::RefCell;
use std::rc::Rc;

use super::mode::ApcMode;
use super::surface as apc;
use super::view::{as_sequencer_resolution, ApcDrumView};
use crate::config::values::NEW_CLIP_LENGTH_VALUES;
use crate::daw::bank::BankItem;
use crate::framework::command::{ContinuousCommand, SharedTrigger, TriggerCommand};
use crate::framework::commands::channel::{PlayPositionCommand, StopClipCommand};
use crate::framework::commands::clip::{NewCommand, QuantizeCommand};
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::misc::BrowserCommand;
use crate::framework::commands::mode::ModeMultiSelectCommand;
use crate::framework::commands::mv_helper;
use crate::framework::commands::transport_ext::{TapTempoCommand, TempoCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::midi::ButtonEvent;

/// Show the tempo view until the tempo was not changed for 500ms (former
/// `if (!viewManager.isActive (Views.TEMPO)) viewManager.setTemporary (Views.TEMPO);
/// this.timeout.delay (viewManager::restore)`).
pub fn show_tempo_view(ctx: &mut Ctx) {
    if !ctx.views.is_active(Views::Tempo) {
        ctx.set_temporary_view(Views::Tempo);
    }
    let timeout = apc::state(ctx).tempo_timeout.clone();
    timeout.delay(ctx, |ctx| ctx.restore_view());
}

/// The play position knob: also changes the tempo while Tap Tempo is pressed and the loop start
/// and length while the left and right arrows are pressed (former `APCPlayPositionCommand`).
#[derive(Default)]
pub struct ApcPlayPositionCommand {
    tempo_command: TempoCommand,
}

impl ContinuousCommand for ApcPlayPositionCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if ctx.surface.is_pressed(ButtonId::TapTempo) {
            ctx.surface.set_trigger_consumed(ButtonId::TapTempo);
            ContinuousCommand::execute(&mut self.tempo_command, ctx, value);
            show_tempo_view(ctx);
            return;
        }
        let increase = ctx.model.value_changer().is_increase(value);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        if ctx.surface.is_pressed(ButtonId::ArrowLeft) {
            ctx.surface.set_trigger_consumed(ButtonId::ArrowLeft);
            ctx.model.transport_mut().change_loop_start(increase, slow);
            mv_helper::delay_display(ctx, |ctx| format!("Loop Start: {}", ctx.model.transport().loop_start_beat_text()));
            return;
        }
        if ctx.surface.is_pressed(ButtonId::ArrowRight) {
            ctx.surface.set_trigger_consumed(ButtonId::ArrowRight);
            ctx.model.transport_mut().change_loop_length(increase, slow);
            mv_helper::delay_display(ctx, |ctx| format!("Loop Length: {}", ctx.model.transport().loop_length_beat_text()));
            return;
        }
        PlayPositionCommand::change_position(ctx, value);
    }
}

/// The tempo knob of the Mk2, which shows the tempo view (former `APCTempoCommand`).
#[derive(Default)]
pub struct ApcTempoCommand {
    tempo_command: TempoCommand,
}

impl ContinuousCommand for ApcTempoCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        ContinuousCommand::execute(&mut self.tempo_command, ctx, value);
        show_tempo_view(ctx);
    }
}

/// Former `APCBrowserCommand.startBrowser`: the Mk1 always commits the browser, the Mk2 discards
/// it with Shift.
pub fn start_browser(ctx: &mut Ctx, insert_device: bool, before_current: bool) {
    let Some(is_active) = ctx.model.browser().map(|b| b.is_active()) else {
        return;
    };
    if is_active {
        // Former getCommit
        let commit = !apc::is_mk2(ctx) || !ctx.surface.is_shift_pressed();
        BrowserCommand::discard_browser(ctx, commit);
        return;
    }
    BrowserCommand::start_browser(ctx, insert_device, before_current);
}

/// Opens the browser (former `APCBrowserCommand`, triggers Shift and Select).
#[derive(Default)]
pub struct ApcBrowserCommand;

impl TriggerCommand for ApcBrowserCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Shift) {
            start_browser(ctx, true, true);
        } else {
            let insert = ctx.surface.is_pressed(ButtonId::Select);
            start_browser(ctx, insert, false);
        }
    }
}

/// The cursor keys (former `APCCursorCommand`): executed on release; up/down select the browser
/// results in the browser mode, left/right the item pages of the active mode or the markers
/// with Shift.
pub struct ApcCursorCommand {
    cursor: CursorCommand,
}

impl ApcCursorCommand {
    pub fn new(direction: Direction) -> Self {
        let mut cursor = CursorCommand::new(direction, false);
        cursor.trigger_event = ButtonEvent::Up;
        ApcCursorCommand { cursor }
    }

    fn scroll(&self, ctx: &mut Ctx) {
        let is_browser_mode = ctx.modes.is_active(Modes::Browser);
        match self.cursor.direction {
            Direction::Up => {
                if is_browser_mode {
                    if let Some(browser) = ctx.model.browser_mut() {
                        browser.select_previous_result();
                    }
                } else {
                    self.cursor.scroll_up(ctx);
                }
            }
            Direction::Down => {
                if is_browser_mode {
                    if let Some(browser) = ctx.model.browser_mut() {
                        browser.select_next_result();
                    }
                } else {
                    self.cursor.scroll_down(ctx);
                }
            }
            Direction::Left | Direction::Right => {
                let is_right = self.cursor.direction == Direction::Right;
                if ctx.surface.is_shift_pressed() {
                    let markers = ctx.model.marker_bank_mut();
                    if is_right {
                        markers.select_next_item();
                    } else {
                        markers.select_previous_item();
                    }
                    return;
                }
                if let Some(mode) = ctx.active_mode() {
                    if let Ok(mut mode) = mode.try_borrow_mut() {
                        if is_right {
                            mode.select_next_item_page(ctx);
                        } else {
                            mode.select_previous_item_page(ctx);
                        }
                    }
                }
            }
        }
    }
}

impl TriggerCommand for ApcCursorCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != self.cursor.trigger_event {
            return;
        }
        self.scroll(ctx);
        self.cursor.notify(ctx);
    }
}

/// Toggles the cursor device; with Shift opens the browser (former `APCDeviceOnOffCommand`).
#[derive(Default)]
pub struct ApcDeviceOnOffCommand;

impl TriggerCommand for ApcDeviceOnOffCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.cursor_device_mut().toggle_enabled_state();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            start_browser(ctx, false, false);
        }
    }
}

/// Quantize; with Shift pins the cursor device and track (former `APCQuantizeCommand`).
#[derive(Default)]
pub struct ApcQuantizeCommand {
    quantize: QuantizeCommand,
}

impl TriggerCommand for ApcQuantizeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.quantize.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if !ctx.model.cursor_track().does_exist() || !ctx.model.cursor_device().does_exist() {
            return;
        }
        let pinned = ctx.model.cursor_device().is_pinned();
        ctx.model.cursor_device_mut().toggle_pinned();
        let cursor_track_pinned = ctx.model.is_cursor_track_pinned();
        if pinned == cursor_track_pinned {
            ctx.model.toggle_cursor_track_pinned();
        }
    }
}

/// Starts the recording; with Shift creates a new clip (former `APCRecordCommand`).
#[derive(Default)]
pub struct ApcRecordCommand {
    new_command: NewCommand,
}

impl TriggerCommand for ApcRecordCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().start_recording();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.new_command.execute_normal(ctx, event);
    }
}

/// The track stop buttons: select the resolution in the sequencers (former
/// `APCStopClipCommand`).
pub struct ApcStopClipCommand {
    stop: StopClipCommand,
}

impl ApcStopClipCommand {
    pub fn new(index: usize) -> Self {
        ApcStopClipCommand { stop: StopClipCommand::new(index as i32) }
    }

    /// Former `getButtonColor`.
    pub fn button_color(ctx: &Ctx, index: usize, stop_button: ButtonId) -> i32 {
        if let Some(view) = ctx.active_view() {
            if let Ok(view) = view.try_borrow() {
                if let Some(drum) = view.as_any().downcast_ref::<ApcDrumView>() {
                    return drum.stop_button_color(ctx, index);
                }
                if let Some(resolution) = as_sequencer_resolution(&*view, ctx) {
                    return i32::from(resolution == index);
                }
            }
        }
        i32::from(ctx.surface.is_pressed(stop_button))
    }
}

impl TriggerCommand for ApcStopClipCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        // Set the step resolution in sequencer modes
        let index = self.stop.index as usize;
        if let Some(view) = ctx.active_view() {
            if let Ok(mut view) = view.try_borrow_mut() {
                if let Some(drum) = view.as_any_mut().downcast_mut::<ApcDrumView>() {
                    drum.handle_stop_buttons(ctx, index);
                    return;
                }
                if view.set_sequencer_resolution_index(ctx, index as i32) {
                    return;
                }
            }
        }
        self.stop.execute_normal(ctx, ButtonEvent::Down);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.stop.execute_shifted(ctx, event);
    }
}

/// Tap tempo, which shows the tempo view; with Shift adds a marker (former
/// `APCTapTempoCommand`).
#[derive(Default)]
pub struct ApcTapTempoCommand {
    tap: TapTempoCommand,
}

impl TriggerCommand for ApcTapTempoCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        self.tap.execute_normal(ctx, event);
        show_tempo_view(ctx);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.marker_bank().add_marker();
        }
    }
}

/// Apply a function to the user mode (former `(UserMode) modeManager.get (Modes.USER)`).
pub fn with_user_mode<R>(ctx: &mut Ctx, f: impl FnOnce(&mut ApcMode, &mut Ctx) -> R) -> Option<R> {
    let mode = ctx.modes.get(Modes::User)?;
    let mut mode = mode.try_borrow_mut().ok()?;
    let user = mode.as_any_mut().downcast_mut::<ApcMode>()?;
    Some(f(user, ctx))
}

/// Toggles the user mode between project and track parameters (former `APCUserModeCommand`).
#[derive(Default)]
pub struct ApcUserModeCommand;

impl TriggerCommand for ApcUserModeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.modes.is_active(Modes::User) {
            with_user_mode(ctx, |mode, ctx| mode.toggle_mode(ctx));
        } else {
            ctx.set_active_mode(Modes::User);
        }
        // Former getTextDisplay ().notify (null): clears the last notification
        with_user_mode(ctx, |mode, ctx| mode.display_page_name(ctx));
    }
}

/// The track selection buttons: select a track, a send while Send is pressed, a user page while
/// User is pressed (Mk2) or the new clip length with Shift (former
/// `SelectTrackSendOrClipLengthCommand`).
pub struct SelectTrackSendOrClipLengthCommand {
    index: usize,
    /// The send mode selection command of the Mk2, to keep its selection in sync.
    send_command: Option<Rc<RefCell<ModeMultiSelectCommand>>>,
}

impl SelectTrackSendOrClipLengthCommand {
    pub fn new(index: usize, send_command: Option<Rc<RefCell<ModeMultiSelectCommand>>>) -> Self {
        SelectTrackSendOrClipLengthCommand { index, send_command }
    }
}

impl TriggerCommand for SelectTrackSendOrClipLengthCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Send1) {
            let send_mode_id = Modes::get(Modes::Send1, self.index);
            ctx.surface.set_trigger_consumed(ButtonId::Send1);
            // Keep the send step selection in sync. Note: Java casts the command of the Send
            // button, which is wrapped by the shift closer, so this throws a
            // ClassCastException on the Mk2; here the selection command is used directly.
            match &self.send_command {
                Some(command) => match command.try_borrow_mut() {
                    Ok(mut command) => command.activate_mode(ctx, send_mode_id),
                    Err(_) => ctx.set_active_mode(send_mode_id),
                },
                None => ctx.set_active_mode(send_mode_id),
            }
            // Display the sends name
            let tb = ctx.model.track_bank();
            let track = tb.selected_item().or_else(|| Some(tb.item(0)).filter(|t| t.does_exist()));
            let send_name = match track {
                Some(track) => track.send_bank().item(self.index).name().to_string(),
                None => "-".to_string(),
            };
            ctx.notify(&format!("Send {}: {send_name}", self.index + 1));
            return;
        }
        if apc::is_mk2(ctx) && ctx.surface.is_pressed(ButtonId::Send2) {
            ctx.surface.set_trigger_consumed(ButtonId::Send2);
            let index = self.index;
            with_user_mode(ctx, |mode, ctx| crate::framework::feature::Mode::select_item_page(mode, ctx, index));
            return;
        }
        ctx.model.current_track_bank_mut().item_mut(self.index).select_or_expand_group();
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if let Some(value) = NEW_CLIP_LENGTH_VALUES.get(self.index) {
            ctx.notify(value);
        }
        ctx.config.set_new_clip_length(self.index);
    }
}

/// Selects a send mode, with Shift the 3rd send higher (former `SendModeCommand`, Mk1).
pub struct SendModeCommand {
    send_index: usize,
}

impl SendModeCommand {
    pub fn new(send_index: usize) -> Self {
        SendModeCommand { send_index }
    }

    fn handle_execute(ctx: &mut Ctx, event: ButtonEvent, send_index: usize) {
        // No Sends on FX tracks
        if event != ButtonEvent::Down || ctx.model.is_effect_track_bank_active() {
            return;
        }
        ctx.set_active_mode(Modes::get(Modes::Send1, send_index));
        mv_helper::notify_selected_send(ctx, send_index);
    }
}

impl TriggerCommand for SendModeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        Self::handle_execute(ctx, event, self.send_index);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        Self::handle_execute(ctx, event, self.send_index + 3);
    }
}

/// Toggles the launcher overdub; with Shift the clip launcher automation write (former
/// `SessionRecordCommand`).
#[derive(Default)]
pub struct SessionRecordCommand;

impl TriggerCommand for SessionRecordCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.toggle_launcher_overdub();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().toggle_write_clip_launcher_automation();
        }
    }
}

/// Wraps the command of a button to close the shift view, if Shift was used in combination
/// with a button outside of the grid (former override of `addButton` in
/// `APCControllerSetup`).
pub struct ShiftCloserCommand {
    inner: SharedTrigger,
}

impl ShiftCloserCommand {
    pub fn new(inner: SharedTrigger) -> Self {
        ShiftCloserCommand { inner }
    }
}

impl TriggerCommand for ShiftCloserCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if ctx.surface.is_shift_pressed() && event == ButtonEvent::Down && ctx.views.active_id() == Some(Views::Shift) {
            ctx.restore_view();
        }
        match self.inner.try_borrow_mut() {
            Ok(mut command) => command.execute(ctx, event, velocity),
            Err(_) => ctx.host.error("Recursive execution of a command"),
        }
    }
}
