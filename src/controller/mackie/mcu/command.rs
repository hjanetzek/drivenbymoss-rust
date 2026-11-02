// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the MCU (former package `mcu.command.trigger` and
//! `MCUMultiModeSwitcherCommand`).

use std::cell::Cell;
use std::rc::Rc;

use super::mode::{is_layer_mode, with_master_volume, McuMode};
use super::surface::{extender_offset, mcu, mcu_mut};
use crate::config::mcu::{self as cfg, MainDisplay, McuConfig};
use crate::daw::application::{PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_EDIT, PANEL_LAYOUT_MIX};
use crate::daw::bank::BankItem;
use crate::daw::constants::DeviceId;
use crate::daw::events::ParameterBankId;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::general::ToggleTrackBanksCommand;
use crate::framework::commands::misc::FootswitchCommand;
use crate::framework::commands::mode::ModeMultiSelectCommand;
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::commands::transport_ext::WindCommand;
use crate::framework::ctx::Ctx;
use crate::framework::display::pad;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::parameter_mode::ModeBank;
use crate::midi::ButtonEvent;

const PREV_MODES: [(Modes, Modes); 4] = [(Modes::Pan, Modes::Volume), (Modes::Volume, Modes::Track), (Modes::Track, Modes::DeviceParams), (Modes::DeviceParams, Modes::Pan)];
const NEXT_MODES: [(Modes, Modes); 4] = [(Modes::Track, Modes::Volume), (Modes::Volume, Modes::Pan), (Modes::Pan, Modes::DeviceParams), (Modes::DeviceParams, Modes::Track)];

fn lookup(map: &[(Modes, Modes)], mode: Option<Modes>, default: Modes) -> Modes {
    map.iter().find(|(from, _)| Some(*from) == mode).map_or(default, |(_, to)| *to)
}

fn active_mode_name(ctx: &Ctx) -> String {
    ctx.active_mode().and_then(|m| m.try_borrow().ok().map(|m| m.name().to_string())).unwrap_or_default()
}

/// Switches through the main modes (former `ModeSwitcher`).
pub struct ModeSwitcher;

impl ModeSwitcher {
    pub fn scroll_up(ctx: &mut Ctx) {
        let mode = lookup(&PREV_MODES, ctx.modes.active_id(), Modes::Pan);
        ctx.set_active_mode(mode);
        let name = active_mode_name(ctx);
        ctx.host.show_notification(&name);
    }

    pub fn scroll_down(ctx: &mut Ctx) {
        let mode = lookup(&NEXT_MODES, ctx.modes.active_id(), Modes::Track);
        ctx.set_active_mode(mode);
        let name = active_mode_name(ctx);
        ctx.host.show_notification(&name);
    }
}

/// Former `MCUMultiModeSwitcherCommand`: a mode multi select command which does not notify on
/// an Asparion display.
pub struct McuMultiModeSwitcherCommand {
    inner: ModeMultiSelectCommand,
}

impl McuMultiModeSwitcherCommand {
    pub fn new(modes: &[Modes]) -> Self {
        McuMultiModeSwitcherCommand { inner: ModeMultiSelectCommand::new(modes) }
    }
}

impl TriggerCommand for McuMultiModeSwitcherCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        self.inner.notify = ctx.config.main_display_type() != MainDisplay::Asparion;
        self.inner.execute(ctx, event, velocity);
    }
}

/// Selects the track or layer modes (former `TracksCommand`); toggles the pinning of the
/// cursor track with Select.
pub struct TracksCommand {
    track_modes: McuMultiModeSwitcherCommand,
    layer_modes: McuMultiModeSwitcherCommand,
}

impl TracksCommand {
    pub fn new() -> Self {
        TracksCommand {
            track_modes: McuMultiModeSwitcherCommand::new(&[Modes::Volume, Modes::Track]),
            layer_modes: McuMultiModeSwitcherCommand::new(&[Modes::DeviceLayerVolume, Modes::DeviceLayer]),
        }
    }
}

impl Default for TracksCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for TracksCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if ctx.surface.is_pressed(ButtonId::Select) {
            if event == ButtonEvent::Down {
                ctx.model.toggle_cursor_track_pinned();
            }
            return;
        }
        if is_layer_mode(ctx.modes.active_id()) {
            self.layer_modes.execute(ctx, event, velocity);
        } else {
            self.track_modes.execute(ctx, event, velocity);
        }
    }
}

/// Selects the panorama or track mode (former `PanCommand`).
pub struct PanCommand {
    track_modes: McuMultiModeSwitcherCommand,
    layer_modes: McuMultiModeSwitcherCommand,
}

impl PanCommand {
    pub fn new() -> Self {
        PanCommand {
            track_modes: McuMultiModeSwitcherCommand::new(&[Modes::Pan, Modes::Track]),
            layer_modes: McuMultiModeSwitcherCommand::new(&[Modes::DeviceLayerPan, Modes::DeviceLayer]),
        }
    }
}

impl Default for PanCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for PanCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if is_layer_mode(ctx.modes.active_id()) {
            self.layer_modes.execute(ctx, event, velocity);
        } else {
            self.track_modes.execute(ctx, event, velocity);
        }
    }
}

/// Selects the send modes (former `SendSelectCommand`).
pub struct SendSelectCommand {
    track_modes: McuMultiModeSwitcherCommand,
    layer_modes: McuMultiModeSwitcherCommand,
}

impl SendSelectCommand {
    pub fn new() -> Self {
        let sends: Vec<Modes> = (0..8).map(|i| Modes::get(Modes::Send1, i)).collect();
        let layer_sends: Vec<Modes> = (0..8).map(|i| Modes::get(Modes::DeviceLayerSend1, i)).collect();
        SendSelectCommand { track_modes: McuMultiModeSwitcherCommand::new(&sends), layer_modes: McuMultiModeSwitcherCommand::new(&layer_sends) }
    }
}

impl Default for SendSelectCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for SendSelectCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if is_layer_mode(ctx.modes.active_id()) {
            self.layer_modes.execute(ctx, event, velocity);
        } else {
            self.track_modes.execute(ctx, event, velocity);
        }
    }
}

/// Selects the device, project and track parameter modes, toggles the pinning of the cursor
/// device with Select (former `MCUDevicesCommand`).
pub struct McuDevicesCommand {
    inner: McuMultiModeSwitcherCommand,
}

impl McuDevicesCommand {
    pub fn new() -> Self {
        McuDevicesCommand { inner: McuMultiModeSwitcherCommand::new(&[Modes::DeviceParams, Modes::ProjectParameters, Modes::TrackParameters]) }
    }
}

impl Default for McuDevicesCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for McuDevicesCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if ctx.surface.is_pressed(ButtonId::Select) {
            if event == ButtonEvent::Down {
                ctx.model.cursor_device_mut().toggle_pinned();
            }
            return;
        }
        self.inner.execute(ctx, event, velocity);
    }
}

/// Rewind / fast forward, select the previous / next marker with Select (former
/// `MCUWindCommand`).
pub struct McuWindCommand {
    pub inner: WindCommand,
    is_fast_forward: bool,
}

impl McuWindCommand {
    pub fn new(is_fast_forward: bool) -> Self {
        McuWindCommand { inner: WindCommand::new(is_fast_forward), is_fast_forward }
    }
}

impl TriggerCommand for McuWindCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if ctx.surface.is_select_pressed() {
            if event != ButtonEvent::Down {
                return;
            }
            if self.is_fast_forward {
                ctx.model.marker_bank_mut().select_next_item();
            } else {
                ctx.model.marker_bank_mut().select_previous_item();
            }
            return;
        }
        self.inner.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_shifted(ctx, event);
    }
}

/// The cursor keys: browser navigation, zoom or arrow keys (former `MCUCursorCommand`).
pub struct McuCursorCommand {
    direction: crate::framework::commands::cursor::Direction,
}

impl McuCursorCommand {
    pub fn new(direction: crate::framework::commands::cursor::Direction) -> Self {
        McuCursorCommand { direction }
    }
}

impl TriggerCommand for McuCursorCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        use crate::framework::commands::cursor::Direction;
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.model.browser().is_some_and(|b| b.is_active()) {
            let device = ctx.model.cursor_device();
            let (name, position) = (device.name().to_string(), device.position());
            if let Some(browser) = ctx.model.browser_mut() {
                match self.direction {
                    Direction::Left => browser.insert_before_cursor_device(&name, position),
                    Direction::Right => browser.insert_after_cursor_device(&name, position),
                    Direction::Up => browser.next_content_type(),
                    Direction::Down => browser.previous_content_type(),
                }
            }
            return;
        }
        let zoom = ctx.config.is_zoom_state();
        let vertical_zoom_modes = ctx.config.use_vert_zoom_for_modes();
        match self.direction {
            Direction::Left => {
                if zoom {
                    ctx.model.application_mut().zoom_out();
                } else {
                    ctx.model.application_mut().arrow_key_left();
                }
            }
            Direction::Right => {
                if zoom {
                    ctx.model.application_mut().zoom_in();
                } else {
                    ctx.model.application_mut().arrow_key_right();
                }
            }
            Direction::Up => {
                if !zoom {
                    ctx.model.application_mut().arrow_key_up();
                } else if vertical_zoom_modes {
                    ModeSwitcher::scroll_up(ctx);
                } else {
                    ctx.model.application_mut().dec_track_height();
                }
            }
            Direction::Down => {
                if !zoom {
                    ctx.model.application_mut().arrow_key_down();
                } else if vertical_zoom_modes {
                    ModeSwitcher::scroll_down(ctx);
                } else {
                    ctx.model.application_mut().inc_track_height();
                }
            }
        }
    }
}

/// Replaces the cursor device with the browser selection or toggles the zoom state (former
/// `ZoomCommand`).
pub struct ZoomCommand;

impl TriggerCommand for ZoomCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.model.browser().is_some_and(|b| b.is_active()) {
            let device = ctx.model.cursor_device();
            if device.does_exist() {
                let (name, position) = (device.name().to_string(), device.position());
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.replace_device(&name, position);
                }
            }
            return;
        }
        ctx.config.toggle_zoom_state();
    }
}

/// Toggles between track names and parameter names in the first display (former
/// `ToggleDisplayCommand`).
pub struct ToggleDisplayCommand;

impl TriggerCommand for ToggleDisplayCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.config.toggle_display_track_names();
        }
    }
}

/// Toggles between ticks and tempo, or time and beats with Select (former
/// `TempoTicksCommand`).
pub struct TempoTicksCommand;

impl TriggerCommand for TempoTicksCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_select_pressed() {
            ctx.config.toggle_display_time();
        } else {
            ctx.config.toggle_display_ticks();
        }
        ctx.schedule(200, |ctx| {
            let message = format!("{}{}", if ctx.config.is_display_time() { "TIME - " } else { "BEATS - " }, if ctx.config.is_display_ticks() { "TICKS" } else { "TEMPO" });
            ctx.notify(&message);
        });
    }
}

/// Toggles "use faders like knobs", the track banks with Shift (former `MCUFlipCommand`).
pub struct McuFlipCommand;

impl McuFlipCommand {
    /// Former `executeNormal`.
    pub fn flip(ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        ctx.config.toggle_use_faders_as_knobs();
        delay_display(ctx, |ctx| format!("Use faders as knobs: {}", if ctx.config.use_faders_as_knobs() { "On" } else { "Off" }));
    }
}

impl TriggerCommand for McuFlipCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        Self::flip(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        ToggleTrackBanksCommand::toggle(ctx, event);
    }
}

/// The keys Enter and Escape (former `KeyCommand`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    Escape,
}

/// Commits or cancels the browser, otherwise Enter / Escape (former `KeyCommand`).
pub struct KeyCommand(pub Key);

impl TriggerCommand for KeyCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let is_enter = self.0 == Key::Enter;
        if ctx.modes.is_active(Modes::Browser) {
            if let Some(browser) = ctx.model.browser_mut() {
                browser.stop_browsing(is_enter);
            }
            ctx.restore_mode();
        } else if is_enter {
            ctx.model.application_mut().enter();
        } else {
            ctx.model.application_mut().escape();
        }
    }
}

/// Moves the bank of the active mode by 1 or by a page (former `MCUMoveTrackBankCommand`).
pub struct McuMoveTrackBankCommand {
    move_by_1: bool,
    move_left: bool,
}

impl McuMoveTrackBankCommand {
    pub fn new(move_by_1: bool, move_left: bool) -> Self {
        McuMoveTrackBankCommand { move_by_1, move_left }
    }

    fn handle_bank_movement(&self, ctx: &mut Ctx, bank: ModeBank) {
        match (self.move_by_1, self.move_left) {
            (true, true) => bank.scroll_backwards(ctx),
            (true, false) => bank.scroll_forwards(ctx),
            (false, true) => bank.select_previous_page(ctx),
            (false, false) => bank.select_next_page(ctx),
        }
    }

    fn handle_mode_movement(&self, ctx: &mut Ctx) {
        let Some(mode) = ctx.active_mode() else {
            return;
        };
        let Ok(mut mode) = mode.try_borrow_mut() else {
            return;
        };
        match (self.move_by_1, self.move_left) {
            (true, true) => mode.select_previous_item(ctx),
            (true, false) => mode.select_next_item(ctx),
            (false, true) => mode.select_previous_item_page(ctx),
            (false, false) => mode.select_next_item_page(ctx),
        }
    }

    fn notify_selected_device_and_parameter_page(ctx: &mut Ctx) {
        delay_display(ctx, |ctx| {
            let device = ctx.model.cursor_device();
            if !device.does_exist() {
                return "No device selected".to_string();
            }
            let mut text = pad(&format!("{} ", crate::controller::mackie::shorten_and_fix_ascii(device.name(), 27)), 28);
            if let Some(bank) = device.parameter_bank() {
                let page_bank = bank.page_bank();
                if page_bank.item_count() > 0 {
                    let page_name = page_bank.selected_item();
                    text.push_str(if page_name.trim().is_empty() { "None" } else { &page_name });
                }
            }
            text
        });
    }

    fn notify_selected_parameter_page(ctx: &mut Ctx, is_project_parameters: bool) {
        delay_display(ctx, move |ctx| {
            let mut text = if is_project_parameters {
                "Project".to_string()
            } else {
                let track = ctx.model.cursor_track();
                format!("Track: {}", if track.does_exist() { track.name() } else { "None" })
            };
            text = pad(&text, 28);
            let bank = if is_project_parameters { ParameterBankId::Project } else { ParameterBankId::CursorTrack };
            if let Some(bank) = crate::framework::parameterprovider::parameter_bank(&ctx.model, bank) {
                let page_bank = bank.page_bank();
                if page_bank.item_count() > 0 {
                    let page_name = page_bank.selected_item();
                    text.push_str(if page_name.trim().is_empty() { "None" } else { &page_name });
                }
            }
            text
        });
    }
}

impl TriggerCommand for McuMoveTrackBankCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_select_pressed() {
            if self.move_by_1 {
                if self.move_left {
                    ctx.model.cursor_track_swap_with_previous();
                } else {
                    ctx.model.cursor_track_swap_with_next();
                }
            } else if self.move_left {
                ctx.model.cursor_device_mut().swap_with_previous();
            } else {
                ctx.model.cursor_device_mut().swap_with_next();
            }
            return;
        }

        let should_notify = ctx.config.main_display_type() != MainDisplay::Asparion;
        let active = ctx.modes.active_id();
        match active {
            Some(Modes::EqDeviceParams | Modes::InstrumentDeviceParams | Modes::DeviceParams) => {
                let bank = match active {
                    Some(Modes::EqDeviceParams) => ParameterBankId::Eq,
                    Some(Modes::InstrumentDeviceParams) => ParameterBankId::FirstInstrument,
                    _ => ParameterBankId::CursorDevice,
                };
                if self.move_by_1 {
                    self.handle_bank_movement(ctx, ModeBank::ParameterBank(bank));
                } else if active == Some(Modes::DeviceParams) {
                    // Only the cursor device can select its siblings
                    if self.move_left {
                        ctx.model.cursor_device_mut().select_previous();
                    } else {
                        ctx.model.cursor_device_mut().select_next();
                    }
                }
                if should_notify {
                    Self::notify_selected_device_and_parameter_page(ctx);
                }
            }
            Some(Modes::ProjectParameters) => {
                self.handle_bank_movement(ctx, ModeBank::ParameterBank(ParameterBankId::Project));
                if should_notify {
                    Self::notify_selected_parameter_page(ctx, true);
                }
            }
            Some(Modes::TrackParameters) => {
                self.handle_bank_movement(ctx, ModeBank::ParameterBank(ParameterBankId::CursorTrack));
                if should_notify {
                    Self::notify_selected_parameter_page(ctx, false);
                }
            }
            Some(Modes::Markers) => {
                let bank = ctx.model.marker_bank_mut();
                match (self.move_by_1, self.move_left) {
                    (true, true) => bank.scroll_backwards(),
                    (true, false) => bank.scroll_forwards(),
                    (false, true) => bank.select_previous_page(),
                    (false, false) => bank.select_next_page(),
                }
            }
            mode if is_layer_mode(mode) => self.handle_mode_movement(ctx),
            _ => self.handle_bank_movement(ctx, ModeBank::TrackBank),
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        // Only for the effect track bank pinned to the last device, which Reaper has not
        if event != ButtonEvent::Down || !ctx.config.should_pin_fx_tracks_to_last_controller() {
            return;
        }
        if ctx.model.effect_track_bank().is_some() {
            self.handle_bank_movement(ctx, ModeBank::TrackBank);
        }
    }
}

/// The functions of the footswitches and the function keys (former `AssignableCommand`).
pub struct AssignableCommand {
    index: usize,
    inner: FootswitchCommand,
    previous_track: McuMoveTrackBankCommand,
    next_track: McuMoveTrackBankCommand,
    jog_wheel_last_param: Rc<Cell<bool>>,
}

impl AssignableCommand {
    pub fn new(index: usize, jog_wheel_last_param: Rc<Cell<bool>>) -> Self {
        AssignableCommand {
            index,
            inner: FootswitchCommand::new(index),
            previous_track: McuMoveTrackBankCommand::new(true, true),
            next_track: McuMoveTrackBankCommand::new(true, false),
            jog_wheel_last_param,
        }
    }

    /// Former `isActive`: the state of the light.
    pub fn is_active(ctx: &Ctx, index: usize, jog_wheel_last_param: bool) -> bool {
        use crate::config::values::*;
        let transport = ctx.model.transport();
        match ctx.config.assignable(index) {
            FOOTSWITCH_TOGGLE_PLAY => transport.is_playing(),
            FOOTSWITCH_TOGGLE_RECORD => transport.is_recording(),
            FOOTSWITCH_TOGGLE_CLIP_OVERDUB => ctx.model.is_launcher_overdub(),
            FOOTSWITCH_PANEL_LAYOUT_ARRANGE => ctx.model.application().panel_layout() == PANEL_LAYOUT_ARRANGE,
            FOOTSWITCH_PANEL_LAYOUT_MIX => ctx.model.application().panel_layout() == PANEL_LAYOUT_MIX,
            FOOTSWITCH_PANEL_LAYOUT_EDIT => ctx.model.application().panel_layout() == PANEL_LAYOUT_EDIT,
            cfg::FOOTSWITCH_SHOW_MARKER_MODE => ctx.modes.is_active(Modes::Markers),
            cfg::FOOTSWITCH_USE_FADERS_LIKE_EDIT_KNOBS => ctx.config.use_faders_as_knobs(),
            cfg::FOOTSWITCH_TOGGLE_MOTOR_FADERS_ON_OFF => ctx.config.has_motor_faders(),
            cfg::FOOTSWITCH_PUNCH_IN => transport.is_punch_in_enabled(),
            cfg::FOOTSWITCH_PUNCH_OUT => transport.is_punch_out_enabled(),
            cfg::FOOTSWITCH_DEVICE_ON_OFF => ctx.model.cursor_device().is_enabled(),
            cfg::CONTROL_LAST_PARAM_ENCODER => jog_wheel_last_param,
            cfg::CONTROL_LAST_PARAM_MASTER_FADER => super::mode::is_master_control_last_param_active(ctx),
            _ => false,
        }
    }
}

impl TriggerCommand for AssignableCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        let is_down = event == ButtonEvent::Down;
        let setting = ctx.config.assignable(self.index);
        match setting {
            cfg::FOOTSWITCH_PREV_MODE => {
                if is_down {
                    ModeSwitcher::scroll_down(ctx);
                }
            }
            cfg::FOOTSWITCH_NEXT_MODE => {
                if is_down {
                    ModeSwitcher::scroll_up(ctx);
                }
            }
            cfg::FOOTSWITCH_SHOW_MARKER_MODE => {
                if !is_down {
                    return;
                }
                if ctx.modes.is_active(Modes::Markers) {
                    ctx.restore_mode();
                } else {
                    ctx.set_active_mode(Modes::Markers);
                }
                let name = active_mode_name(ctx);
                if !name.is_empty() {
                    ctx.notify(&name);
                }
            }
            cfg::FOOTSWITCH_USE_FADERS_LIKE_EDIT_KNOBS => McuFlipCommand::flip(ctx, event),
            cfg::FOOTSWITCH_TOGGLE_MOTOR_FADERS_ON_OFF => {
                if !is_down {
                    return;
                }
                ctx.config.toggle_motor_faders();
                delay_display(ctx, |ctx| format!("Motor Faders: {}", if ctx.config.has_motor_faders() { "On" } else { "Off" }));
            }
            cfg::FOOTSWITCH_PUNCH_IN => {
                if is_down {
                    ctx.model.transport_mut().toggle_punch_in();
                }
            }
            cfg::FOOTSWITCH_PUNCH_OUT => {
                if is_down {
                    ctx.model.transport_mut().toggle_punch_out();
                }
            }
            cfg::FOOTSWITCH_DEVICE_ON_OFF => {
                if is_down {
                    ctx.model.cursor_device_mut().toggle_enabled_state();
                }
            }
            cfg::PREV_CHANNEL => self.previous_track.execute(ctx, event, velocity),
            cfg::NEXT_CHANNEL => self.next_track.execute(ctx, event, velocity),
            cfg::CONTROL_LAST_PARAM_ENCODER => {
                if is_down {
                    self.jog_wheel_last_param.set(!self.jog_wheel_last_param.get());
                }
            }
            cfg::CONTROL_LAST_PARAM_MASTER_FADER => {
                if is_down {
                    with_master_volume(ctx, |mv, ctx| mv.toggle_control_last_param_active(ctx));
                }
            }
            cfg::FOOTSWITCH_ACTION => {
                if !is_down {
                    return;
                }
                let action = ctx.config.assignable_action(self.index);
                ctx.model.application_mut().invoke_action(&action);
            }
            _ => self.inner.execute_setting(ctx, event, setting),
        }
    }
}

const TRACK_LAYER_MODE_MAP: [(Modes, Modes); 11] = [
    (Modes::Track, Modes::DeviceLayer),
    (Modes::Volume, Modes::DeviceLayerVolume),
    (Modes::Pan, Modes::DeviceLayerPan),
    (Modes::Send1, Modes::DeviceLayerSend1),
    (Modes::Send2, Modes::DeviceLayerSend2),
    (Modes::Send3, Modes::DeviceLayerSend3),
    (Modes::Send4, Modes::DeviceLayerSend4),
    (Modes::Send5, Modes::DeviceLayerSend5),
    (Modes::Send6, Modes::DeviceLayerSend6),
    (Modes::Send7, Modes::DeviceLayerSend7),
    (Modes::Send8, Modes::DeviceLayerSend8),
];

/// The select buttons of the channels (former `mcu.command.trigger.SelectCommand`).
pub struct SelectCommand {
    index: usize,
}

impl SelectCommand {
    pub fn new(index: usize) -> Self {
        SelectCommand { index }
    }

    /// The channel of the button (former `channel`: the extender offset plus the index).
    fn channel(&self, ctx: &Ctx) -> usize {
        extender_offset(ctx) + self.index
    }

    fn handle_select_enter(&self, ctx: &mut Ctx) {
        let channel = self.channel(ctx);
        // If layer modes are active select the layer...
        if is_layer_mode(ctx.modes.active_id()) {
            if let Some(device) = ctx.model.specific_device_mut(DeviceId::FirstInstrument) {
                let bank = device.layer_bank_mut();
                if channel < bank.page_size() && !bank.item(channel).is_selected() {
                    BankItem::select(bank.item_mut(channel));
                }
            }
            return;
        }
        let tb = ctx.model.current_track_bank();
        if channel >= tb.page_size() {
            return;
        }
        let track = tb.item(channel);
        // Select the track if not already selected...
        if !track.is_selected() {
            ctx.model.current_track_bank_mut().item_mut(channel).select();
            return;
        }
        // If the track is a group dive in...
        if track.is_group() {
            if ctx.config.is_track_navigation_flat() {
                ctx.model.current_track_bank_mut().item_mut(channel).toggle_group_expanded();
            } else {
                ctx.model.current_track_bank_mut().item_mut(channel).set_group_expanded(true);
                ctx.model.current_track_bank_mut().enter(channel);
            }
            return;
        }
        // If the tracks cursor device has layers dive in...
        if ctx.model.specific_device(DeviceId::FirstInstrument).is_some_and(|d| d.has_layers()) {
            let active = ctx.modes.active_id();
            if let Some((_, layer_mode)) = TRACK_LAYER_MODE_MAP.iter().find(|(track_mode, _)| Some(*track_mode) == active) {
                ctx.set_active_mode(*layer_mode);
            }
        }
    }

    fn handle_leave(&self, ctx: &mut Ctx) {
        // If layer modes are active switch back to track mode...
        let active = ctx.modes.active_id();
        if is_layer_mode(active) {
            if let Some((track_mode, _)) = TRACK_LAYER_MODE_MAP.iter().find(|(_, layer_mode)| Some(*layer_mode) == active) {
                ctx.set_active_mode(*track_mode);
            }
        } else if !ctx.config.is_track_navigation_flat() {
            // ... otherwise leave the track group
            ctx.model.current_track_bank_mut().select_parent();
        }
        // Consume upcoming the button up event
        for i in 0..8 {
            ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::RowSelect1, i));
        }
    }

    fn handle_button_combinations(&self, ctx: &mut Ctx, event: ButtonEvent) -> bool {
        let channel = self.channel(ctx);
        // Select Send channels if Send button is pressed
        if ctx.surface.is_pressed(ButtonId::Sends) {
            if event == ButtonEvent::Down {
                // Reaper has no effect track bank
                ctx.notify(&format!("Send channel {} does not exist.", channel + 1));
                ctx.surface.set_trigger_consumed(ButtonId::Sends);
            }
            return true;
        }
        // Execute stop if Select is pressed
        if ctx.surface.is_select_pressed() {
            if event == ButtonEvent::Down && channel < ctx.model.current_track_bank().page_size() {
                ctx.model.current_track_bank_mut().item_mut(channel).stop(false);
            }
            return true;
        }
        if ctx.surface.is_pressed(ButtonId::Control) {
            if event == ButtonEvent::Down && channel < ctx.model.current_track_bank().page_size() && ctx.model.current_track_bank().item(channel).is_group() {
                ctx.model.current_track_bank_mut().item_mut(channel).toggle_group_expanded();
            }
            return true;
        }
        if ctx.surface.is_pressed(ButtonId::Alt) {
            ctx.notify(crate::config::values::NEW_CLIP_LENGTH_VALUES[self.index.min(7)]);
            ctx.config.set_new_clip_length(self.index);
            return true;
        }
        if ctx.config.has_only_1_fader() {
            if event != ButtonEvent::Down {
                return true;
            }
            // Select marker if marker mode is active
            if ctx.modes.is_active(Modes::Markers) {
                if channel < ctx.model.marker_bank().page_size() {
                    ctx.model.marker_bank_mut().item_mut(channel).select();
                }
                return true;
            }
            // Select parameter if device mode is active
            if ctx.modes.is_active(Modes::DeviceParams) {
                if ctx.model.cursor_device().does_exist() {
                    if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                        if channel < bank.page_size() {
                            bank.item_mut(channel).select();
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    /// Former `executeShifted`: allow multi selection.
    fn multi_select(&self, ctx: &mut Ctx, event: ButtonEvent) {
        let channel = self.channel(ctx);
        if event != ButtonEvent::Down || self.index >= 8 || channel >= ctx.model.current_track_bank().page_size() {
            return;
        }
        ctx.model.current_track_bank_mut().item_mut(channel).toggle_multi_select();
    }
}

impl TriggerCommand for SelectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if self.handle_button_combinations(ctx, event) {
            return;
        }
        match event {
            ButtonEvent::Up => self.handle_select_enter(ctx),
            ButtonEvent::Long => self.handle_leave(ctx),
            ButtonEvent::Down => {}
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.multi_select(ctx, event);
    }
}

/// Touching a fader selects the channel and activates the volume mode temporarily (former
/// `mcu.command.trigger.FaderTouchCommand`, index 8 is the master fader).
pub struct FaderTouchCommand {
    select: SelectCommand,
    index: usize,
}

impl FaderTouchCommand {
    pub fn new(index: usize) -> Self {
        FaderTouchCommand { select: SelectCommand::new(index), index }
    }

    fn touch_mode(ctx: &mut Ctx, mode: Modes, index: usize, is_touched: bool) {
        if let Some(mode) = ctx.modes.get(mode) {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.on_knob_touch(ctx, index, is_touched);
            }
        }
    }

    fn has_touched_fader(ctx: &Ctx) -> bool {
        mcu(ctx).shared.borrow().touched_faders.iter().any(|t| *t)
    }
}

impl TriggerCommand for FaderTouchCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Long {
            return;
        }
        let is_touched = event == ButtonEvent::Down;

        // Master Channel
        if self.index == 8 {
            if super::mode::is_master_control_last_param_active(ctx) {
                return;
            }
            if is_touched && ctx.config.is_touch_selects_channel() {
                ctx.model.master_track_mut().select();
            }
            Self::touch_mode(ctx, Modes::Master, 0, is_touched);
            return;
        }

        if ctx.config.use_faders_as_knobs() {
            if let Some(mode) = ctx.active_mode() {
                if let Ok(mut mode) = mode.try_borrow_mut() {
                    mode.on_knob_touch(ctx, self.index, is_touched);
                }
            }
            return;
        }

        // Select channel or layer
        let channel = extender_offset(ctx) + self.index;
        let is_layer = is_layer_mode(ctx.modes.active_id());
        if is_touched && ctx.config.is_touch_selects_channel() {
            if is_layer {
                let bank = ctx.model.cursor_device_mut().layer_bank_mut();
                if channel < bank.page_size() {
                    BankItem::select(bank.item_mut(channel));
                }
            } else if channel < ctx.model.current_track_bank().page_size() {
                ctx.model.current_track_bank_mut().item_mut(channel).select();
            }
        }

        let volume_mode = if is_layer { Modes::DeviceLayerVolume } else { Modes::Volume };
        Self::touch_mode(ctx, volume_mode, self.index, is_touched);

        let pos = mcu(ctx).index * 8 + self.index;
        // Temporarily enable (layer) volume mode
        if ctx.config.is_touch_channel_volume_mode() {
            if is_touched {
                if !Self::has_touched_fader(ctx) {
                    if ctx.modes.is_active(volume_mode) {
                        ctx.set_previous_mode_on_all_surfaces(volume_mode);
                    } else {
                        ctx.set_active_mode(volume_mode);
                    }
                }
                mcu_mut(ctx).shared.borrow_mut().touched_faders[pos.min(31)] = true;
            } else {
                mcu_mut(ctx).shared.borrow_mut().touched_faders[pos.min(31)] = false;
                if !Self::has_touched_fader(ctx) {
                    ctx.restore_mode();
                }
            }
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        // Former inherited SelectCommand.executeShifted
        self.select.multi_select(ctx, event);
    }
}

/// Former `handleSoloDefeat`: clear solo, clear mute with Shift.
pub fn handle_solo_defeat(ctx: &mut Ctx) {
    let has_drum_pads = ctx.model.cursor_device().has_drum_pads();
    if ctx.surface.is_shift_pressed() {
        ctx.model.current_track_bank_mut().clear_mute();
        if has_drum_pads {
            ctx.model.cursor_device_mut().drum_pad_bank_mut().clear_mute();
        }
        return;
    }
    ctx.model.current_track_bank_mut().clear_solo();
    if has_drum_pads {
        ctx.model.cursor_device_mut().drum_pad_bank_mut().clear_solo();
    }
}

/// Former `getSoloState`.
pub fn solo_state(ctx: &Ctx) -> bool {
    let device = ctx.model.cursor_device();
    if ctx.surface.is_shift_pressed() {
        return ctx.model.current_track_bank().has_mute() || device.has_drum_pads() && device.drum_pad_bank().has_muted_pads();
    }
    ctx.model.current_track_bank().has_solo() || device.has_drum_pads() && device.drum_pad_bank().has_soloed_pads()
}

/// The mode (of the current surface) with the given ID as a MCU mode.
pub fn with_mcu_mode<R>(ctx: &Ctx, mode: Modes, f: impl FnOnce(&McuMode) -> R) -> Option<R> {
    let mode = ctx.modes.get(mode)?;
    let mode = mode.try_borrow().ok()?;
    mode.as_any().downcast_ref::<McuMode>().map(f)
}
