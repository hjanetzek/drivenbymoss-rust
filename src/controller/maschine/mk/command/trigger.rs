// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The trigger commands of the Maschine Mk2/Mk3/Mikro/Plus/Studio (former package
//! `controller.ni.maschine.mk3.command.trigger`).

use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::maschine::mk::mode::edit_note::{EditNoteMode, NoteAttribute};
use crate::controller::maschine::mk::mode::track_modes::MaschineVolumeMode;
use crate::controller::maschine::mk::mode::user_mode::MaschineUserMode;
use crate::controller::maschine::mk::surface::{ext, ext_mut, maschine, set_stop_consumed};
use crate::controller::maschine::mk::view::drum_view::DrumView;
use crate::controller::maschine::mk::view::play_view::PlayView;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::misc::{AddTrackCommand, BrowserCommand};
use crate::framework::commands::mode::{ModeMultiSelectCommand, ModeSelectCommand};
use crate::framework::commands::mv_helper::notify_selected_device;
use crate::framework::commands::transport::MetronomeCommand;
use crate::framework::commands::transport_ext::{StopCommand, TapTempoCommand};
use crate::framework::commands::view::ToggleShiftViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::view::sequencer::abstract_sequencer_view::with_note_editor;
use crate::midi::ButtonEvent;

/// Execute the function with the drum view.
pub fn with_drum_view<R>(ctx: &mut Ctx, f: impl FnOnce(&mut DrumView, &mut Ctx) -> R) -> Option<R> {
    let view = ctx.views.get(Views::Drum)?;
    let mut view = view.try_borrow_mut().ok()?;
    let drum_view = view.as_any_mut().downcast_mut::<DrumView>()?;
    Some(f(drum_view, ctx))
}

/// Execute the function with the play view.
pub fn with_play_view<R>(ctx: &mut Ctx, f: impl FnOnce(&mut PlayView, &mut Ctx) -> R) -> Option<R> {
    let view = ctx.views.get(Views::Play)?;
    let mut view = view.try_borrow_mut().ok()?;
    let play_view = view.as_any_mut().downcast_mut::<PlayView>()?;
    Some(f(play_view, ctx))
}

/// Read a value of the drum view.
pub fn drum_view_state<R>(ctx: &Ctx, f: impl FnOnce(&DrumView) -> R) -> Option<R> {
    let view = ctx.views.get(Views::Drum)?;
    let view = view.try_borrow().ok()?;
    view.as_any().downcast_ref::<DrumView>().map(f)
}

/// Read a value of the play view.
pub fn play_view_state<R>(ctx: &Ctx, f: impl FnOnce(&PlayView) -> R) -> Option<R> {
    let view = ctx.views.get(Views::Play)?;
    let view = view.try_borrow().ok()?;
    view.as_any().downcast_ref::<PlayView>().map(f)
}

/// Execute the function with the edit note mode.
fn with_edit_note_mode<R>(ctx: &mut Ctx, f: impl FnOnce(&mut EditNoteMode, &mut Ctx) -> R) -> Option<R> {
    let mode = ctx.modes.get(Modes::Note)?;
    let mut mode = mode.try_borrow_mut().ok()?;
    let edit_note_mode = mode.as_any_mut().downcast_mut::<EditNoteMode>()?;
    Some(f(edit_note_mode, ctx))
}

/// Is the note mode active in the drum or play view? Then the buttons select the edited note
/// attribute.
fn is_note_editing(ctx: &Ctx) -> bool {
    ctx.modes.is_active(Modes::Note) && ctx.views.is_any_active(&[Views::Drum, Views::Play])
}

/// Toggle between two note attributes of the edit note mode and display the name of the new one.
fn toggle_note_attribute(ctx: &mut Ctx, first: NoteAttribute, second: NoteAttribute, first_name: &str, second_name: &str) {
    let selected_first = with_edit_note_mode(ctx, |mode, _ctx| {
        let is_first = mode.active_parameter() == first;
        mode.select_active_parameter(if is_first { second } else { first });
        is_first
    });
    if let Some(is_first) = selected_first {
        ctx.notify(if is_first { second_name } else { first_name });
    }
}

// ------------------------------------------------------------------------------------------

/// Stops the playback; on the models without a Shift button, the Stop button acts as the Shift
/// button (former `MaschineStopCommand`).
#[derive(Default)]
pub struct MaschineStopCommand;

impl MaschineStopCommand {
    /// Former `executeNormal`.
    fn stop(ctx: &mut Ctx) {
        if ctx.model.transport().is_playing() {
            StopCommand::handle_stop_options(ctx);
        } else {
            ctx.model.transport_mut().stop_and_rewind();
        }
    }
}

impl TriggerCommand for MaschineStopCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if !maschine(ctx).has_shift() {
            if event == ButtonEvent::Down {
                ctx.surface.set_knob_sensitivity_is_slow(true);
                if !ctx.views.is_active(Views::Shift) {
                    ctx.set_temporary_view(Views::Shift);
                }
                return;
            }
            if event != ButtonEvent::Up {
                return;
            }
            ctx.surface.set_knob_sensitivity_is_slow(false);
            if ctx.views.is_active(Views::Shift) {
                ctx.restore_view();
            }
            if ext(ctx).stop_consumed {
                ext_mut(ctx).stop_consumed = false;
                return;
            }
        }
        // Note: as in Java the models with a Shift button stop on all button events
        Self::stop(ctx);
    }
}

/// Toggles the Shift view (former `MaschineToggleShiftViewCommand`): the knob sensitivity is
/// not changed on the Mk3.
#[derive(Default)]
pub struct MaschineToggleShiftViewCommand {
    inner: ToggleShiftViewCommand,
}

impl MaschineToggleShiftViewCommand {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TriggerCommand for MaschineToggleShiftViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        let is_slow = ctx.surface.is_knob_sensitivity_slow();
        self.inner.execute(ctx, event, velocity);
        if maschine(ctx) == Maschine::Mk3 {
            ctx.surface.set_knob_sensitivity_is_slow(is_slow);
        }
    }
}

/// Tap tempo, with Shift toggle the metronome (former `MaschineTapMetroCommand`).
pub struct MaschineTapMetroCommand {
    metro_command: MetronomeCommand,
    tap_tempo_command: TapTempoCommand,
}

impl Default for MaschineTapMetroCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl MaschineTapMetroCommand {
    pub fn new() -> Self {
        MaschineTapMetroCommand { metro_command: MetronomeCommand::new(false), tap_tempo_command: TapTempoCommand::new() }
    }
}

impl TriggerCommand for MaschineTapMetroCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            set_stop_consumed(ctx);
            self.metro_command.execute_normal(ctx, event);
        } else {
            // Former tapTempoCommand.execute (DOWN, velocity) without Shift
            self.tap_tempo_command.execute(ctx, ButtonEvent::Down, velocity);
        }
    }
}

/// Pressing the encoder confirms the browser or toggles controlling the last touched parameter
/// (former `MaschineEncoderPressCommand`).
#[derive(Default)]
pub struct MaschineEncoderPressCommand;

impl TriggerCommand for MaschineEncoderPressCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.modes.active_id() == Some(Modes::Browser) {
            if let Some(browser) = ctx.model.browser_mut() {
                browser.stop_browsing(true);
            }
            ctx.restore_mode();
        } else {
            let active = !ext(ctx).control_last_param_active;
            ext_mut(ctx).control_last_param_active = active;
            ctx.notify(&format!("Last Param: {}", if active { " ON" } else { "OFF" }));
        }
    }
}

/// Selects the play view or toggles the scale configuration (former `KeyboardCommand`).
#[derive(Default)]
pub struct KeyboardCommand;

impl KeyboardCommand {
    pub fn execute_keyboard(ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.views.is_active(Views::Play) {
            if !maschine(ctx).has_mcu_display() {
                with_play_view(ctx, |view, ctx| view.toggle_shifted(ctx));
            }
            if ctx.modes.is_active(Modes::Scales) {
                ctx.restore_mode();
            } else {
                ctx.set_temporary_mode(Modes::Scales);
            }
        } else {
            ctx.set_active_view(Views::Play);
            with_note_editor(ctx, |editor| editor.clear_notes());
            let cursor_track = ctx.model.cursor_track();
            if cursor_track.does_exist() {
                let position = cursor_track.position();
                ctx.views.set_preferred(position, Views::Play);
            }
        }
    }
}

impl TriggerCommand for KeyboardCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        Self::execute_keyboard(ctx, event);
    }
}

/// Selects the drum view or toggles the drum configuration (former `PadModeCommand`).
#[derive(Default)]
pub struct PadModeCommand {
    /// The keyboard command to execute with Shift (none on the Mk2).
    has_keyboard_command: bool,
}

impl PadModeCommand {
    pub fn new(has_keyboard_command: bool) -> Self {
        PadModeCommand { has_keyboard_command }
    }

    pub fn execute_pad_mode(ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.views.is_active(Views::Drum) {
            if !maschine(ctx).has_mcu_display() {
                with_drum_view(ctx, |view, ctx| view.toggle_shifted(ctx));
            }
            if ctx.modes.is_active(Modes::PlayOptions) {
                ctx.restore_mode();
            } else {
                ctx.set_active_mode(Modes::PlayOptions);
            }
        } else {
            ctx.set_active_view(Views::Drum);
            with_note_editor(ctx, |editor| editor.clear_notes());
            let cursor_track = ctx.model.cursor_track();
            if cursor_track.does_exist() {
                let position = cursor_track.position();
                ctx.views.set_preferred(position, Views::Drum);
            }
        }
    }
}

impl TriggerCommand for PadModeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        Self::execute_pad_mode(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        set_stop_consumed(ctx);
        if self.has_keyboard_command {
            KeyboardCommand::execute_keyboard(ctx, event);
        }
    }
}

/// Switches between the drum, play and chord view on the Mk2 and Studio (former
/// `MaschineMk2PlayViewSwitchCommand`).
#[derive(Default)]
pub struct MaschineMk2PlayViewSwitchCommand;

impl TriggerCommand for MaschineMk2PlayViewSwitchCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }

        match ctx.views.active_id() {
            Some(Views::Drum) => KeyboardCommand::execute_keyboard(ctx, event),
            Some(Views::Play) => {
                if play_view_state(ctx, |v| v.is_chord_mode()).unwrap_or(false) {
                    PadModeCommand::execute_pad_mode(ctx, event);
                }
                with_play_view(ctx, |view, _ctx| view.toggle_chord_mode());
            }
            _ => PadModeCommand::execute_pad_mode(ctx, event),
        }

        if ctx.views.active_id() == Some(Views::Drum) {
            ctx.notify("Drum");
        } else if play_view_state(ctx, |v| v.is_chord_mode()).unwrap_or(false) {
            ctx.notify("Chord");
        } else {
            ctx.notify("Play");
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match ctx.views.active_id_ignore_temporary() {
            Some(Views::Drum) => {
                if ctx.modes.is_active(Modes::PlayOptions) {
                    ctx.restore_mode();
                } else {
                    ctx.set_active_mode(Modes::PlayOptions);
                }
            }
            Some(Views::Play) => {
                if ctx.modes.is_active(Modes::Scales) {
                    ctx.restore_mode();
                } else {
                    ctx.set_temporary_mode(Modes::Scales);
                }
            }
            _ => {}
        }
    }
}

/// Updates the note mapping, with Shift toggles the note edit mode (former
/// `MaschineSelectButtonCommand`).
pub struct MaschineSelectButtonCommand {
    mode_select_command: ModeSelectCommand,
}

impl Default for MaschineSelectButtonCommand {
    fn default() -> Self {
        MaschineSelectButtonCommand { mode_select_command: ModeSelectCommand::with_toggle(Modes::Note, true) }
    }
}

impl TriggerCommand for MaschineSelectButtonCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, _event: ButtonEvent) {
        ctx.update_view_note_mapping();
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.mode_select_command.execute_normal(ctx, event);
    }
}

/// Selects the next/previous send mode and notifies the name of the send (former
/// `MaschineSendSelectCommand`).
pub struct MaschineSendSelectCommand {
    inner: ModeMultiSelectCommand,
}

impl Default for MaschineSendSelectCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl MaschineSendSelectCommand {
    pub fn new() -> Self {
        MaschineSendSelectCommand {
            inner: ModeMultiSelectCommand::new(&[Modes::Send1, Modes::Send2, Modes::Send3, Modes::Send4, Modes::Send5, Modes::Send6, Modes::Send7, Modes::Send8]),
        }
    }

    fn notify_send(ctx: &mut Ctx, active_mode: Option<Modes>) {
        let Some(new_mode) = ctx.modes.active_id() else {
            return;
        };
        if active_mode == Some(new_mode) {
            return;
        }
        let send_index = new_mode.ordinal().wrapping_sub(Modes::Send1.ordinal());
        if send_index >= 8 {
            return;
        }
        let Some(track) = ctx.model.current_track_bank().selected_item() else {
            return;
        };
        let send = track.send_bank().item(send_index);
        if send.does_exist() {
            let message = format!("Send {}: {}", send_index + 1, send.name());
            ctx.notify(&message);
        }
    }
}

impl TriggerCommand for MaschineSendSelectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        let active_mode = ctx.modes.active_id();
        self.inner.execute_normal(ctx, event);
        Self::notify_send(ctx, active_mode);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        let active_mode = ctx.modes.active_id();
        self.inner.execute_shifted(ctx, event);
        Self::notify_send(ctx, active_mode);
    }
}

/// The cursor command of the Maschine: on the Mk2 and Studio Shift + left/right emulate the
/// up/down buttons (former `MaschineCursorCommand`).
pub struct MaschineCursorCommand {
    direction: Direction,
    cursor: CursorCommand,
    mode_selector: ModeMultiSelectCommand,
}

impl MaschineCursorCommand {
    pub fn new(direction: Direction) -> Self {
        MaschineCursorCommand {
            direction,
            cursor: CursorCommand::new_mode_cursor(direction, false),
            mode_selector: ModeMultiSelectCommand::new(&[Modes::Position, Modes::Tempo, Modes::Volume]),
        }
    }

    fn select(&mut self, ctx: &mut Ctx, is_page: bool, is_next: bool) {
        self.cursor.select_item_or_page(ctx, is_page, is_next);
    }
}

impl TriggerCommand for MaschineCursorCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let model = maschine(ctx);
        let is_studio = model == Maschine::Studio;
        let is_up_down_emulated = model == Maschine::Studio || model == Maschine::Mk2;
        match self.direction {
            Direction::Left => {
                if is_studio && ctx.surface.is_pressed(ButtonId::Overdub) {
                    ctx.surface.set_trigger_consumed(ButtonId::Overdub);
                    self.mode_selector.execute_shifted(ctx, ButtonEvent::Up);
                    return;
                }
                if is_up_down_emulated && ctx.surface.is_shift_pressed() {
                    // Former scrollDown; Shift is the Stop button on the Mk2, its release must not
                    // stop (a bug of the Java version, UPSTREAM_CANDIDATES.md L)
                    set_stop_consumed(ctx);
                    self.select(ctx, true, false);
                    return;
                }
                self.select(ctx, false, false);
            }
            Direction::Right => {
                if is_studio && ctx.surface.is_pressed(ButtonId::Overdub) {
                    ctx.surface.set_trigger_consumed(ButtonId::Overdub);
                    self.mode_selector.execute_normal(ctx, ButtonEvent::Up);
                    return;
                }
                if is_up_down_emulated && ctx.surface.is_shift_pressed() {
                    // Former scrollUp; Shift is the Stop button on the Mk2, its release must not
                    // stop (a bug of the Java version, UPSTREAM_CANDIDATES.md L)
                    set_stop_consumed(ctx);
                    self.select(ctx, true, true);
                    return;
                }
                self.select(ctx, false, true);
            }
            Direction::Up => self.select(ctx, true, true),
            Direction::Down => self.select(ctx, true, false),
        }
    }
}

/// Selects the previous/next device, user parameter page, browser content type or clip (former
/// `PageCommand`).
pub struct PageCommand {
    direction: Direction,
}

impl PageCommand {
    pub fn new(direction: Direction) -> Self {
        PageCommand { direction }
    }

    /// Select the previous/next slot of the cursor track (the slot pages follow the scene
    /// bank).
    fn select_slot(ctx: &mut Ctx, is_next: bool, is_page: bool) {
        let page_size = ctx.model.cursor_track().slot_bank().page_size();
        let selected = ctx.model.cursor_track().slot_bank().selected_item().map(crate::daw::bank::BankItem::index);
        if !is_page {
            let index = match selected {
                Some(index) => index + if is_next { 1 } else { -1 },
                None => 0,
            };
            if index >= 0 && (index as usize) < page_size {
                crate::daw::bank::BankItem::select(ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index as usize));
                return;
            }
        }
        let can_scroll = {
            let scene_bank = ctx.model.scene_bank();
            if is_next {
                scene_bank.can_scroll_page_forwards()
            } else {
                scene_bank.can_scroll_page_backwards()
            }
        };
        if !can_scroll {
            return;
        }
        {
            let mut scene_bank = ctx.model.scene_bank_mut();
            if is_next {
                scene_bank.scroll_page_forwards();
            } else {
                scene_bank.scroll_page_backwards();
            }
        }
        let index = if is_next { 0 } else { page_size.saturating_sub(1) };
        crate::daw::bank::BankItem::select(ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index));
    }
}

impl TriggerCommand for PageCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let is_left = self.direction == Direction::Left;
        match ctx.modes.active_id() {
            Some(Modes::DeviceParams) => {
                if is_left {
                    ctx.model.cursor_device_mut().select_previous();
                } else {
                    ctx.model.cursor_device_mut().select_next();
                }
                notify_selected_device(ctx);
            }
            Some(Modes::User) => {
                if let Some(mode) = ctx.modes.get(Modes::User) {
                    if let Ok(mut mode) = mode.try_borrow_mut() {
                        if let Some(user_mode) = mode.as_any_mut().downcast_mut::<MaschineUserMode>() {
                            user_mode.set_mode(ctx, is_left);
                        }
                    }
                }
            }
            Some(Modes::Browser) => {
                if let Some(browser) = ctx.model.browser_mut() {
                    if is_left {
                        browser.previous_content_type();
                    } else {
                        browser.next_content_type();
                    }
                }
            }
            _ => {
                if !ctx.model.cursor_track().does_exist() {
                    return;
                }
                let is_shift = ctx.surface.is_shift_pressed();
                if is_shift {
                    set_stop_consumed(ctx);
                }
                Self::select_slot(ctx, !is_left, is_shift);
            }
        }
    }
}

/// Saves the project (with display or Shift), otherwise discards the browser or adds a track
/// (former `ProjectButtonCommand`).
#[derive(Default)]
pub struct ProjectButtonCommand {
    add_track: AddTrackCommand,
}

impl TriggerCommand for ProjectButtonCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if maschine(ctx).has_mcu_display() || ctx.surface.is_shift_pressed() {
            ctx.model.project_mut().save();
            return;
        }
        if ctx.model.browser().is_some_and(|b| b.is_active()) {
            BrowserCommand::discard_browser(ctx, false);
        } else {
            self.add_track.execute(ctx, event, velocity);
        }
    }
}

/// Opens the browser to add a device (former `AddDeviceCommand`). The Reaper browser has no
/// filter columns, therefore nothing happens if the browser is already active.
pub struct AddDeviceCommand;

impl TriggerCommand for AddDeviceCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.model.browser().is_some_and(|b| b.is_active()) {
            // Former: select the next item of the collection filter column / reset the filter
            return;
        }
        // Former AddEffectCommand.executeNormal
        if !ctx.surface.is_shift_pressed() {
            BrowserCommand::start_browser(ctx, true, false);
        }
    }
}

/// Selects the transport modes; in the note mode it toggles between panning and duration
/// (former `SwingCommand`).
pub struct SwingCommand {
    inner: ModeMultiSelectCommand,
}

impl Default for SwingCommand {
    fn default() -> Self {
        SwingCommand { inner: ModeMultiSelectCommand::new(&[Modes::Position, Modes::LoopStart, Modes::LoopLength]) }
    }
}

impl TriggerCommand for SwingCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if is_note_editing(ctx) {
            if event == ButtonEvent::Down {
                toggle_note_attribute(ctx, NoteAttribute::Panning, NoteAttribute::Duration, "Panning", "Duration");
            }
            return;
        }
        self.inner.execute(ctx, event, velocity);
    }
}

/// Selects the tempo mode; in the note mode it toggles between pitch and pressure (former
/// `TempoCommand`).
pub struct TempoCommand {
    inner: ModeSelectCommand,
}

impl Default for TempoCommand {
    fn default() -> Self {
        TempoCommand { inner: ModeSelectCommand::new(Modes::Tempo) }
    }
}

impl TriggerCommand for TempoCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if is_note_editing(ctx) {
            if event == ButtonEvent::Down {
                toggle_note_attribute(ctx, NoteAttribute::Transpose, NoteAttribute::Pressure, "Pitch", "Pressure");
            }
            return;
        }
        self.inner.execute(ctx, event, velocity);
    }
}

/// Selects the volume, panorama and send modes; in the note mode it toggles between velocity
/// and gain; with Shift it toggles the VU display of the volume mode (former
/// `VolumePanSendCommand`).
pub struct VolumePanSendCommand {
    inner: ModeMultiSelectCommand,
}

impl Default for VolumePanSendCommand {
    fn default() -> Self {
        VolumePanSendCommand {
            inner: ModeMultiSelectCommand::new(&[
                Modes::Volume,
                Modes::Pan,
                Modes::Send1,
                Modes::Send2,
                Modes::Send3,
                Modes::Send4,
                Modes::Send5,
                Modes::Send6,
                Modes::Send7,
                Modes::Send8,
            ]),
        }
    }
}

impl TriggerCommand for VolumePanSendCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if is_note_editing(ctx) {
            if event == ButtonEvent::Down {
                toggle_note_attribute(ctx, NoteAttribute::Velocity, NoteAttribute::Gain, "Velocity", "Gain");
            }
            return;
        }

        if ctx.surface.is_shift_pressed() {
            if event == ButtonEvent::Down {
                set_stop_consumed(ctx);
                if let Some(mode) = ctx.modes.get(Modes::Volume) {
                    if let Ok(mut mode) = mode.try_borrow_mut() {
                        if let Some(volume_mode) = mode.as_any_mut().downcast_mut::<MaschineVolumeMode>() {
                            volume_mode.toggle_display_vu();
                        }
                    }
                }
            }
            return;
        }

        self.inner.execute(ctx, event, velocity);
    }
}

/// Toggles the fixed velocity (former `ToggleFixedVelCommand`).
pub struct ToggleFixedVelCommand;

impl TriggerCommand for ToggleFixedVelCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        let enabled = !ctx.config.is_accent_active();
        ctx.config.set_accent_enabled(enabled);
        ctx.notify(&format!("Fixed Velocity: {}", if enabled { "On" } else { "Off" }));
    }
}

/// Toggles the grid editor of the drum view (the FOLLOW/GRID button).
pub fn toggle_grid_editor(ctx: &mut Ctx, event: ButtonEvent) {
    if event != ButtonEvent::Up {
        return;
    }
    if ctx.views.is_any_active(&[Views::Drum, Views::Play]) {
        with_drum_view(ctx, |view, ctx| view.toggle_grid_editor(ctx));
    }
}

/// Is the grid editor of the drum view active (in the drum or play view)?
pub fn is_grid_editor(ctx: &Ctx) -> bool {
    ctx.views.is_any_active(&[Views::Drum, Views::Play]) && drum_view_state(ctx, |v| v.is_grid_editor()).unwrap_or(false)
}
