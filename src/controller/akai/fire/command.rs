// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the Fire (former `FireModeCommand`, `FireStopCommand`, `FireRecordCommand`,
//! `FireSelectButtonCommand`, `FireBrowserCommand`, `FireViewButtonCommand`,
//! `StepSequencerSelectCommand`, `PlaySelectCommand`, `DrumSequencerSelectCommand`,
//! `SessionSelectCommand` and `SelectKnobCommand`).

use super::mode::{FireNoteMode, FireUserMode};
use super::surface::{is_alt_pressed, with_active_mode};
use super::view::{on_select_knob_value, SessionView};
use crate::config::fire as config;
use crate::daw::bank::BankItem;
use crate::daw::constants::RecordQuantization;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::misc::BrowserCommand;
use crate::framework::commands::mode::ModeMultiSelectCommand;
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::commands::transport_ext::StopCommand;
use crate::framework::commands::transport_more::RecordCommand;
use crate::framework::commands::view::{ViewButtonCommand, ViewMultiSelectCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::midi::ButtonEvent;

/// Former `MVHelper.notifyPlayPosition`.
pub fn notify_play_position(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| format!("Play Position: {}", ctx.model.transport().beat_text()));
}

/// Former `MVHelper.notifyTempo`.
pub fn notify_tempo(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| format!("Tempo: {:.2}", ctx.model.transport().tempo()));
}

fn on_off(is_on: bool) -> &'static str {
    if is_on {
        "On"
    } else {
        "Off"
    }
}

/// Former `MVHelper.notifyAutomationWriteMode`.
pub fn notify_automation_write_mode(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| format!("Automation: {}", ctx.model.transport().automation_write_mode().name()));
}

/// Is the record button shifted (Shift or the flip record setting)?
pub fn is_record_shifted(ctx: &Ctx) -> bool {
    ctx.surface.is_shift_pressed() != ctx.config.is_flip_record()
}

/// Selects the knob modes; with ALT toggles between the single channel and the mixer modes
/// (former `FireModeCommand`).
pub struct FireModeCommand {
    pub multi_select: ModeMultiSelectCommand,
}

impl FireModeCommand {
    pub fn new(modes: &[Modes]) -> Self {
        FireModeCommand { multi_select: ModeMultiSelectCommand::new(modes) }
    }
}

impl TriggerCommand for FireModeCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if !is_alt_pressed(ctx) {
            self.multi_select.execute(ctx, event, velocity);
            return;
        }
        if event == ButtonEvent::Up {
            return;
        }
        // Toggle 16 track displays in channel and mixer modes
        let next = match ctx.modes.active_id() {
            Some(Modes::DeviceLayer) => Some(Modes::DeviceLayerVolume),
            Some(Modes::DeviceLayerVolume) => Some(Modes::DeviceLayer),
            Some(Modes::Track) => Some(Modes::Volume),
            Some(Modes::Volume) => Some(Modes::Track),
            _ => None,
        };
        if let Some(mode) = next {
            ctx.set_active_mode(mode);
        }
        let name = ctx.active_mode().and_then(|m| m.try_borrow().ok().map(|m| m.name().to_string()));
        if let Some(name) = name {
            ctx.host.show_notification(&name);
        }
    }
}

/// Stop; with ALT toggles the arranger automation write, with SHIFT the clip launcher
/// automation write (former `FireStopCommand`).
#[derive(Default)]
pub struct FireStopCommand {
    stop: StopCommand,
}

impl TriggerCommand for FireStopCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if is_alt_pressed(ctx) {
            // Former executeAlt
            if event == ButtonEvent::Up {
                ctx.model.transport_mut().toggle_write_arranger_automation();
                delay_display(ctx, |ctx| format!("Arranger Automation: {}", on_off(ctx.model.transport().is_writing_arranger_automation())));
            }
            return;
        }
        if ctx.surface.is_shift_pressed() {
            if event == ButtonEvent::Up {
                ctx.model.transport_mut().toggle_write_clip_launcher_automation();
                delay_display(ctx, |ctx| format!("Clip Automation: {}", on_off(ctx.model.transport().is_writing_clip_launcher_automation())));
            }
            return;
        }
        self.stop.execute(ctx, event, velocity);
    }
}

/// Record; with ALT creates a new clip (former `FireRecordCommand`).
pub struct FireRecordCommand {
    record: RecordCommand,
    new_command: NewCommand,
}

impl FireRecordCommand {
    pub fn new() -> Self {
        FireRecordCommand { record: RecordCommand::new(), new_command: NewCommand::new() }
    }
}

impl Default for FireRecordCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for FireRecordCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if is_alt_pressed(ctx) {
            self.new_command.execute(ctx, event, velocity);
            return;
        }
        self.record.execute(ctx, event, velocity);
    }
}

/// The function of the view selection buttons (STEP, NOTE, DRUM).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewSelect {
    /// Former `StepSequencerSelectCommand`.
    Step,
    /// Former `PlaySelectCommand`.
    Play,
    /// Former `DrumSequencerSelectCommand`.
    Drum,
}

impl ViewSelect {
    pub fn views(self) -> &'static [Views] {
        match self {
            ViewSelect::Step => &[Views::Sequencer, Views::PolySequencer],
            ViewSelect::Play => &[Views::Play, Views::Piano],
            // The XoX drum view (Views::Drum) is not ported
            ViewSelect::Drum => &[Views::Drum4, Views::Drum64],
        }
    }
}

/// Selects one of several views (former `AbstractFireViewMultiSelectCommand` and its
/// sub-classes).
pub struct FireViewSelectCommand {
    kind: ViewSelect,
    multi_select: ViewMultiSelectCommand,
}

impl FireViewSelectCommand {
    pub fn new(kind: ViewSelect) -> Self {
        FireViewSelectCommand { kind, multi_select: ViewMultiSelectCommand::with_options(kind.views(), true, ButtonEvent::Up, false) }
    }

    /// The light state of the button (former `getViewActivationColor`).
    pub fn view_activation_color(ctx: &Ctx, kind: ViewSelect) -> i32 {
        let views = kind.views();
        if let Some(index) = views.iter().position(|v| ctx.views.is_active(*v)) {
            // The drum views start with 2 since the XoX view (1) is not ported
            return index as i32 + if kind == ViewSelect::Drum { 2 } else { 1 };
        }
        if kind == ViewSelect::Step && ctx.surface.is_shift_pressed() && ctx.config.is_accent_active() {
            return 1;
        }
        0
    }
}

impl TriggerCommand for FireViewSelectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if self.kind == ViewSelect::Play && is_alt_pressed(ctx) {
            if let Some(clip) = ctx.model.cursor_clip() {
                clip.quantize(1.0);
            }
            ctx.notify("Quantize");
            return;
        }
        if self.kind != ViewSelect::Play {
            // Clear the edited notes
            if let Some(mode) = ctx.modes.get(Modes::Note) {
                if let Ok(mut mode) = mode.try_borrow_mut() {
                    if let Some(editor) = mode.note_editor_mut() {
                        editor.clear_notes();
                    }
                }
            }
        }
        let cursor_track = ctx.model.cursor_track();
        let does_exist = cursor_track.does_exist();
        let position = cursor_track.position();
        if does_exist && (ctx.views.is_active(Views::Session) || ctx.views.is_active(Views::Mix)) {
            if let Some(preferred) = ctx.views.preferred(position).filter(|v| self.multi_select.view_ids.contains(v)) {
                ctx.set_active_view(preferred);
                return;
            }
        }
        self.multi_select.select_next(ctx, event);
        if does_exist {
            if let Some(active) = ctx.views.active_id() {
                ctx.views.set_preferred(position, active);
            }
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match self.kind {
            ViewSelect::Step => {
                let is_active = ctx.config.is_accent_active();
                ctx.config.set_accent_enabled(!is_active);
            }
            ViewSelect::Drum => {
                ctx.model.transport_mut().tap_tempo();
                notify_tempo(ctx);
            }
            ViewSelect::Play => {
                let track = ctx.model.cursor_track_mut();
                if !track.does_exist() {
                    return;
                }
                // Toggle through all record quantization settings...
                let values = RecordQuantization::ALL;
                let current = track.record_quantization_grid();
                let index = values.iter().position(|v| *v == current).map(|i| (i + 1) % values.len()).unwrap_or(0);
                track.set_record_quantization_grid(values[index]);
                ctx.notify(&format!("Rec. Quant.: {}", values[index].name()));
            }
        }
    }
}

/// Toggles between the session and the mix view; with SHIFT toggles the birds eye view (former
/// `SessionSelectCommand`).
pub struct SessionSelectCommand {
    session_view_id: Views,
}

impl SessionSelectCommand {
    pub fn new() -> Self {
        SessionSelectCommand { session_view_id: Views::Session }
    }

    /// The light state of the button (former `getViewActivationColor`).
    pub fn view_activation_color(ctx: &Ctx) -> i32 {
        if ctx.views.is_active(Views::Session) {
            1
        } else if ctx.views.is_active(Views::Mix) {
            2
        } else {
            0
        }
    }
}

impl Default for SessionSelectCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for SessionSelectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.views.is_active(Views::Session) {
            self.session_view_id = Views::Mix;
        } else if ctx.views.is_active(Views::Mix) {
            self.session_view_id = Views::Session;
        }
        ctx.set_active_view(self.session_view_id);
        let name = ctx.views.get(self.session_view_id).and_then(|v| v.try_borrow().ok().map(|v| v.name().to_string()));
        if let Some(name) = name {
            ctx.notify(&name);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let Some(view) = ctx.views.get(Views::Session) else {
            return;
        };
        let is_birds_eye = {
            let Ok(mut view) = view.try_borrow_mut() else {
                return;
            };
            let Some(session) = view.as_any_mut().downcast_mut::<SessionView>() else {
                return;
            };
            session.session.toggle_birds_eye_view();
            session.session.is_birds_eye_active
        };
        ctx.notify(if is_birds_eye { "Birds Eye" } else { "Clips" });
        if ctx.views.is_active(Views::Shift) {
            ctx.restore_view();
        }
        ctx.surface.set_trigger_consumed(ButtonId::Shift);
    }
}

/// The SELECT button (former `FireSelectButtonCommand`).
pub struct FireSelectButtonCommand;

impl TriggerCommand for FireSelectButtonCommand {
    fn execute(&mut self, ctx: &mut Ctx, _event: ButtonEvent, velocity: i32) {
        if velocity > 0 {
            return;
        }
        if ctx.modes.is_active(Modes::Note) {
            if let Some(mode) = ctx.modes.get(Modes::Note) {
                if let Ok(mode) = mode.try_borrow() {
                    if let Some(note_mode) = mode.as_any().downcast_ref::<FireNoteMode>() {
                        note_mode.reset_transpose(ctx);
                    }
                }
            }
            return;
        }
        if ctx.modes.is_active(Modes::Browser) {
            BrowserCommand::discard_browser(ctx, true);
            return;
        }
        if ctx.modes.is_active(Modes::DeviceParams) {
            if ctx.config.is_delete_mode_active() {
                ctx.model.cursor_device_mut().remove();
                ctx.config.toggle_delete_mode_active();
                return;
            }
            if is_alt_pressed(ctx) && ctx.surface.is_shift_pressed() {
                ctx.model.cursor_device_mut().toggle_enabled_state();
                return;
            }
        }
        if is_alt_pressed(ctx) {
            ctx.model.cursor_device_mut().toggle_window_open();
        } else {
            config::toggle_control_last_param(&mut ctx.config);
            delay_display(ctx, |ctx| format!("Last Param: {}", if config::is_control_last_param(&ctx.config) { "ON" } else { "OFF" }));
        }
    }
}

/// The BROWSER button (former `FireBrowserCommand`): SHIFT inserts before, ALT after the
/// cursor device; ALT closes the browser without committing.
pub struct FireBrowserCommand {
    browser: BrowserCommand,
}

impl FireBrowserCommand {
    pub fn new() -> Self {
        FireBrowserCommand { browser: BrowserCommand::with_triggers(ButtonId::Shift, Some(ButtonId::Alt)) }
    }
}

impl Default for FireBrowserCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for FireBrowserCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        // Former getCommit: discard without commit if ALT is pressed
        if event == ButtonEvent::Up && ctx.model.browser().is_some_and(|b| b.is_active()) {
            let commit = !is_alt_pressed(ctx);
            BrowserCommand::discard_browser(ctx, commit);
            return;
        }
        self.browser.execute(ctx, event, velocity);
    }
}

/// GRID LEFT/RIGHT: changes the content type in the browser, otherwise forwarded to the view
/// (former `FireViewButtonCommand`).
pub struct FireViewButtonCommand {
    button: ButtonId,
    view_button: ViewButtonCommand,
}

impl FireViewButtonCommand {
    pub fn new(button: ButtonId) -> Self {
        FireViewButtonCommand { button, view_button: ViewButtonCommand::new(button) }
    }
}

impl TriggerCommand for FireViewButtonCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if ctx.modes.is_active(Modes::Browser) {
            if velocity == 0 {
                return;
            }
            if let Some(browser) = ctx.model.browser_mut() {
                if self.button == ButtonId::ArrowLeft {
                    browser.previous_content_type();
                } else if self.button == ButtonId::ArrowRight {
                    browser.next_content_type();
                }
            }
            return;
        }
        self.view_button.execute(ctx, event, velocity);
    }
}

/// Selects the next/previous track; with SELECT pressed the page (former
/// `SelectKnobCommand.handleTrackSelection`).
pub fn handle_track_selection(ctx: &mut Ctx, is_inc: bool) {
    let is_select = ctx.surface.is_select_pressed();
    let track_bank = ctx.model.track_bank_mut();
    if is_inc {
        if is_select {
            if track_bank.can_scroll_page_forwards() {
                track_bank.select_next_page();
                return;
            }
            let position_of_last_item = track_bank.position_of_last_item();
            if position_of_last_item >= 0 {
                let index = position_of_last_item as usize % track_bank.page_size().max(1);
                let last_item = track_bank.item_mut(index);
                if !last_item.is_selected() {
                    last_item.select();
                }
            }
            return;
        }
        track_bank.select_next_item();
        return;
    }
    if is_select {
        if track_bank.can_scroll_page_backwards() {
            track_bank.select_previous_page();
            return;
        }
        let first_item = track_bank.item_mut(0);
        if !first_item.is_selected() {
            first_item.select();
        }
        return;
    }
    track_bank.select_previous_item();
}

/// The SELECT knob (former `SelectKnobCommand`).
#[derive(Default)]
pub struct SelectKnobCommand {
    /// The time of the last movement in the note mode (former `Timeout` of 500ms).
    last_move: Option<std::time::Instant>,
}

impl SelectKnobCommand {
    fn handle_device_page_selection(ctx: &mut Ctx, is_inc: bool) {
        if !ctx.model.has_selected_device() {
            return;
        }
        let is_shift = ctx.surface.is_shift_pressed();
        let device = ctx.model.cursor_device_mut();
        match (is_inc, is_shift) {
            (true, true) => {
                if let Some(bank) = device.parameter_bank_mut() {
                    bank.select_next_item();
                }
            }
            (true, false) => device.select_next(),
            (false, true) => {
                if let Some(bank) = device.parameter_bank_mut() {
                    bank.select_previous_item();
                }
            }
            (false, false) => device.select_previous(),
        }
    }

    fn handle_user_page_selection(ctx: &mut Ctx, is_inc: bool) {
        let Some(mode) = ctx.modes.get(Modes::User) else {
            return;
        };
        let Ok(mut mode) = mode.try_borrow_mut() else {
            return;
        };
        let is_shift = ctx.surface.is_shift_pressed();
        let mode: &mut dyn crate::framework::feature::Mode = &mut *mode;
        if is_shift {
            if is_inc {
                mode.select_next_item(ctx);
            } else {
                mode.select_previous_item(ctx);
            }
        } else if let Some(user_mode) = mode.as_any_mut().downcast_mut::<FireUserMode>() {
            user_mode.set_mode(ctx, !is_inc);
        }
    }

    /// Release the touch of the note mode after 500ms without movement (former `checkUntouch`).
    fn check_untouch(&mut self, ctx: &mut Ctx, index: usize) {
        if !ctx.modes.is_active(Modes::Note) {
            return;
        }
        let now = ctx.host.now();
        self.last_move = Some(now);
        ctx.schedule(500, move |ctx| {
            if !ctx.modes.is_active(Modes::Note) {
                return;
            }
            if let Some(mode) = ctx.modes.get(Modes::Note) {
                if let Ok(mut mode) = mode.try_borrow_mut() {
                    mode.on_knob_touch(ctx, index, false);
                }
            }
        });
    }
}

impl ContinuousCommand for SelectKnobCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if ctx.modes.is_active(Modes::Browser) {
            if let Some(mode) = ctx.modes.get(Modes::Browser) {
                if let Ok(mut mode) = mode.try_borrow_mut() {
                    mode.on_knob_touch(ctx, 8, true);
                    mode.on_knob_value(ctx, 8, value);
                }
            }
            self.check_untouch(ctx, 8);
            return;
        }

        // Bank scrolling with ALT button is always active
        if is_alt_pressed(ctx) {
            let is_inc = ctx.model.value_changer().is_increase(value);
            let active = ctx.modes.active_id();
            match active {
                Some(Modes::Track | Modes::Volume | Modes::DeviceLayer | Modes::DeviceLayerVolume | Modes::Note) => handle_track_selection(ctx, is_inc),
                Some(Modes::DeviceParams) => Self::handle_device_page_selection(ctx, is_inc),
                Some(Modes::User) => Self::handle_user_page_selection(ctx, is_inc),
                _ => {}
            }
            return;
        }

        // Change the tempo in combination with the drum button
        if ctx.surface.is_pressed(ButtonId::Drum) {
            ctx.surface.set_trigger_consumed(ButtonId::Drum);
            let mut amount = if ctx.surface.is_select_pressed() { 10.0 } else { 1.0 };
            if ctx.surface.is_shift_pressed() {
                amount /= 100.0;
            }
            if ctx.model.value_changer().calc_knob_change(value) < 0.0 {
                amount *= -1.0;
            }
            let tempo = ctx.model.transport().tempo();
            ctx.model.transport_mut().set_tempo(tempo + amount);
            notify_tempo(ctx);
            return;
        }

        // Change the play position in combination with the metronome button
        if ctx.surface.is_pressed(ButtonId::Metronome) {
            ctx.surface.set_trigger_consumed(ButtonId::Metronome);
            let increase = ctx.model.value_changer().is_increase(value);
            let slow = ctx.surface.is_shift_pressed();
            ctx.model.transport_mut().change_position(increase, slow);
            notify_play_position(ctx);
            return;
        }

        if config::is_control_last_param(&ctx.config) {
            let parameter = crate::framework::commands::general::focused_parameter(ctx);
            if parameter.does_exist(ctx) {
                // Speed it up a bit...
                let change = ctx.model.value_changer().calc_knob_change(value) * 6.0;
                let current = parameter.value(ctx);
                parameter.set_value(ctx, current + change.round() as i32);
            }
            return;
        }

        on_select_knob_value(ctx, None, value);
    }
}

/// The ALT button: notifies the active mode that the parameters changed (former ALT command in
/// `createModes`).
pub fn on_alt(ctx: &mut Ctx) {
    with_active_mode(ctx, |mode, ctx| {
        let any = mode.as_any_mut();
        if let Some(m) = any.downcast_mut::<super::mode::FireChannelMode>() {
            m.base.parameters_adjusted(ctx);
        } else if let Some(m) = any.downcast_mut::<super::mode::FireParameterMode>() {
            m.base.parameters_adjusted(ctx);
        } else if let Some(m) = any.downcast_mut::<FireUserMode>() {
            m.base.parameters_adjusted(ctx);
        }
    });
}
