// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Maschine JAM (former `MaschineJamTrackMode`, `MaschineJamVolumeMode`,
//! `MaschineJamPanMode`, `MaschineJamSendMode`, `MaschineJamParameterMode`,
//! `MaschineJamUserMode`, `FaderSlowChange` and `IMaschineJamMode`).
//!
//! The modes wrap the generic modes and forward all functions except `on_knob_value` (the
//! faders support the slow change) and the navigation functions they override. The faders are
//! not bound to parameters (the Java modes bind the default knob IDs which do not exist on the
//! JAM), the values are handed to `on_knob_value` by the fader commands.

use crate::config::maschine_jam::MaschineJamConfiguration;
use crate::controller::maschine::core::colors::{COLOR_AMBER, COLOR_DARK_GREY, COLOR_WHITE, PARAM_COLORS};
use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::daw::events::ParameterBankId;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::Modes;
use crate::framework::mode::device::{ParameterMode, ProjectParamsMode};
use crate::framework::mode::track::{TrackMode, TrackPanMode, TrackSendMode, TrackVolumeMode};
use crate::framework::parameterprovider::{resolve_mut, ParamRef};

use super::surface::FaderConfig;

/// Changes a parameter either absolute or slowly in steps of 1 (former `FaderSlowChange`).
#[derive(Debug, Clone)]
pub struct FaderSlowChange {
    last_change_value: i32,
}

impl Default for FaderSlowChange {
    fn default() -> Self {
        FaderSlowChange { last_change_value: -1 }
    }
}

impl FaderSlowChange {
    pub fn change_value(&mut self, ctx: &mut Ctx, parameter: &ParamRef, value: i32) {
        if ctx.surface.is_shift_pressed() || MaschineJamConfiguration::is_slow_fader_change(&ctx.config) {
            if self.last_change_value == 0 {
                self.last_change_value = value;
                return;
            }
            let inc = value - self.last_change_value > 0;
            if let Some(p) = resolve_mut(&mut ctx.model, parameter) {
                p.inc(if inc { 1.0 } else { -1.0 });
            }
            self.last_change_value = value;
        } else {
            parameter.set_value(ctx, value);
        }
    }
}

/// Former `IMaschineJamMode`: a mode which configures the touch strips.
pub trait MaschineJamMode {
    fn setup_fader(&self, ctx: &Ctx, index: usize) -> FaderConfig;
}

fn track_color(ctx: &Ctx, color: crate::daw::color::ColorEx) -> i32 {
    ctx.colors.color_index(DawColor::color_id(color))
}

const FADER_OFF_DOT: FaderConfig = FaderConfig::new(FaderConfig::TYPE_DOT, 0, 0);
const FADER_OFF_SINGLE: FaderConfig = FaderConfig::new(FaderConfig::TYPE_SINGLE, 0, 0);

/// The track volume mode (former `MaschineJamVolumeMode`).
pub struct MaschineJamVolumeMode {
    inner: TrackVolumeMode,
    slow_change: FaderSlowChange,
}

impl MaschineJamVolumeMode {
    pub fn new(ctx: &Ctx) -> Self {
        MaschineJamVolumeMode { inner: TrackVolumeMode::new(ctx, true), slow_change: FaderSlowChange::default() }
    }
}

impl Mode for MaschineJamVolumeMode {
    // TrackVolumeMode
    crate::forward_mode!(inner => name, on_activate, on_deactivate, knob_value, is_any_knob_touched,
        on_button, select_item, selected_item_name, select_previous_item, select_next_item,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page,
        has_next_item_page, select_previous_item_page, select_next_item_page, on_knob_touch);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.slow_change.change_value(ctx, &ParamRef::TrackVolume { index }, value);
    }

    crate::impl_as_any!();
}

impl MaschineJamMode for MaschineJamVolumeMode {
    fn setup_fader(&self, ctx: &Ctx, index: usize) -> FaderConfig {
        let track = ctx.model.current_track_bank().item(index);
        if !track.does_exist() {
            return FADER_OFF_DOT;
        }
        let color = track_color(ctx, track.color());
        let value_changer = ctx.model.value_changer();
        let value = value_changer.to_midi_value(track.volume());
        if !ctx.model.transport().is_playing() {
            return FaderConfig::new(FaderConfig::TYPE_SINGLE, color, value);
        }
        let vu = value_changer.to_midi_value(track.vu());
        FaderConfig::dual(FaderConfig::TYPE_DUAL, color, vu, value)
    }
}

/// The track panorama mode (former `MaschineJamPanMode`).
pub struct MaschineJamPanMode {
    inner: TrackPanMode,
    slow_change: FaderSlowChange,
}

impl MaschineJamPanMode {
    pub fn new(ctx: &Ctx) -> Self {
        MaschineJamPanMode { inner: TrackPanMode::new(ctx, true), slow_change: FaderSlowChange::default() }
    }
}

impl Mode for MaschineJamPanMode {
    // TrackPanMode
    crate::forward_mode!(inner => name, on_activate, on_deactivate, knob_value, is_any_knob_touched,
        on_button, select_item, selected_item_name, select_previous_item, select_next_item,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page,
        has_next_item_page, select_previous_item_page, select_next_item_page, on_knob_touch);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.slow_change.change_value(ctx, &ParamRef::TrackPan { index }, value);
    }

    crate::impl_as_any!();
}

impl MaschineJamMode for MaschineJamPanMode {
    fn setup_fader(&self, ctx: &Ctx, index: usize) -> FaderConfig {
        let track = ctx.model.current_track_bank().item(index);
        if !track.does_exist() {
            return FADER_OFF_DOT;
        }
        let color = track_color(ctx, track.color());
        let value = ctx.model.value_changer().to_midi_value(track.pan());
        FaderConfig::new(FaderConfig::TYPE_PAN, color, value)
    }
}

/// The send mode (former `MaschineJamSendMode`).
pub struct MaschineJamSendMode {
    inner: TrackSendMode,
    send_index: usize,
    slow_change: FaderSlowChange,
}

impl MaschineJamSendMode {
    pub fn new(send_index: usize, ctx: &Ctx) -> Self {
        MaschineJamSendMode { inner: TrackSendMode::new(send_index as i32, ctx, true), send_index, slow_change: FaderSlowChange::default() }
    }
}

impl Mode for MaschineJamSendMode {
    // TrackSendMode
    crate::forward_mode!(inner => name, on_activate, on_deactivate, knob_value, is_any_knob_touched,
        on_button, select_item, selected_item_name, select_previous_item, select_next_item,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page,
        has_next_item_page, select_previous_item_page, select_next_item_page, on_knob_touch);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let parameter = ParamRef::TrackSend { index, send: self.send_index };
        if parameter.does_exist(ctx) {
            self.slow_change.change_value(ctx, &parameter, value);
        }
    }

    crate::impl_as_any!();
}

impl MaschineJamMode for MaschineJamSendMode {
    fn setup_fader(&self, ctx: &Ctx, index: usize) -> FaderConfig {
        let track = ctx.model.current_track_bank().item(index);
        if !track.does_exist() {
            return FADER_OFF_DOT;
        }
        let send = track.send_bank().item(self.send_index);
        if !send.does_exist() {
            return FADER_OFF_DOT;
        }
        let value = ctx.model.value_changer().to_midi_value(send.value());
        FaderConfig::new(FaderConfig::TYPE_SINGLE, COLOR_DARK_GREY, value)
    }
}

/// The track mode which controls volume, panorama and the sends of the selected track (former
/// `MaschineJamTrackMode`).
pub struct MaschineJamTrackMode {
    inner: TrackMode,
    slow_change: FaderSlowChange,
}

impl MaschineJamTrackMode {
    pub fn new(ctx: &Ctx) -> Self {
        MaschineJamTrackMode { inner: TrackMode::new(ctx, true), slow_change: FaderSlowChange::default() }
    }
}

impl Mode for MaschineJamTrackMode {
    // TrackMode
    crate::forward_mode!(inner => name, on_activate, on_deactivate, knob_value, is_any_knob_touched,
        on_button, select_item, selected_item_name, select_previous_item, select_next_item,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page,
        has_next_item_page, select_previous_item_page, select_next_item_page, on_knob_touch);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let Some(track) = ctx.model.current_track_bank().selected_item() else {
            return;
        };
        let parameter = match index {
            0 => ParamRef::SelectedTrackVolume,
            1 => ParamRef::SelectedTrackPan,
            _ => {
                if !track.send_bank().has_existing_items() {
                    return;
                }
                ParamRef::SelectedTrackSend { send: index - 2 }
            }
        };
        self.slow_change.change_value(ctx, &parameter, value);
    }

    crate::impl_as_any!();
}

impl MaschineJamMode for MaschineJamTrackMode {
    fn setup_fader(&self, ctx: &Ctx, index: usize) -> FaderConfig {
        let Some(track) = ctx.model.current_track_bank().selected_item() else {
            return FADER_OFF_DOT;
        };
        if !track.does_exist() {
            return FADER_OFF_DOT;
        }
        let color = track_color(ctx, track.color());
        let value_changer = ctx.model.value_changer();
        match index {
            0 => {
                let value = value_changer.to_midi_value(track.volume());
                if !ctx.model.transport().is_playing() {
                    return FaderConfig::new(FaderConfig::TYPE_SINGLE, color, value);
                }
                let vu = value_changer.to_midi_value(track.vu());
                FaderConfig::dual(FaderConfig::TYPE_DUAL, color, vu, value)
            }
            1 => FaderConfig::new(FaderConfig::TYPE_PAN, color, value_changer.to_midi_value(track.pan())),
            _ => {
                let send_bank = track.send_bank();
                if !send_bank.has_existing_items() {
                    return FADER_OFF_DOT;
                }
                let send = send_bank.item(index - 2);
                if !send.does_exist() {
                    return FADER_OFF_DOT;
                }
                FaderConfig::new(FaderConfig::TYPE_SINGLE, COLOR_DARK_GREY, value_changer.to_midi_value(send.value()))
            }
        }
    }
}

/// The device parameter mode (former `MaschineJamParameterMode`).
pub struct MaschineJamParameterMode {
    inner: ParameterMode,
    slow_change: FaderSlowChange,
}

impl MaschineJamParameterMode {
    pub fn new(ctx: &Ctx) -> Self {
        MaschineJamParameterMode { inner: ParameterMode::new(ctx, true, Vec::new()), slow_change: FaderSlowChange::default() }
    }
}

impl Mode for MaschineJamParameterMode {
    // ParameterMode
    crate::forward_mode!(inner => name, on_activate, on_deactivate, knob_value, is_any_knob_touched,
        on_button, select_item, selected_item_name, select_previous_item, select_next_item,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page,
        has_next_item_page, on_knob_touch);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !ctx.model.cursor_device().does_exist() {
            return;
        }
        let item = ParamRef::BankItem { bank: ParameterBankId::CursorDevice, index };
        if item.does_exist(ctx) {
            self.slow_change.change_value(ctx, &item, value);
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        if ctx.surface.is_select_pressed() {
            ctx.model.cursor_device_mut().select_previous();
        } else if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            bank.scroll_backwards();
        }
        mv_helper::notify_selected_device_and_parameter_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        if ctx.surface.is_select_pressed() {
            ctx.model.cursor_device_mut().select_next();
        } else if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            bank.scroll_forwards();
        }
        mv_helper::notify_selected_device_and_parameter_page(ctx);
    }

    crate::impl_as_any!();
}

impl MaschineJamMode for MaschineJamParameterMode {
    fn setup_fader(&self, ctx: &Ctx, index: usize) -> FaderConfig {
        let item = ParamRef::BankItem { bank: ParameterBankId::CursorDevice, index };
        if !ctx.model.cursor_device().does_exist() || !item.does_exist(ctx) {
            return FADER_OFF_SINGLE;
        }
        let value = ctx.model.value_changer().to_midi_value(item.value(ctx));
        FaderConfig::new(FaderConfig::TYPE_SINGLE, PARAM_COLORS[index], value)
    }
}

/// The user mode which controls the project or track parameters (former
/// `MaschineJamUserMode`).
pub struct MaschineJamUserMode {
    inner: ProjectParamsMode,
    slow_change: FaderSlowChange,
}

impl MaschineJamUserMode {
    pub fn new(ctx: &Ctx) -> Self {
        MaschineJamUserMode { inner: ProjectParamsMode::new(ctx, true, Vec::new()), slow_change: FaderSlowChange::default() }
    }

    fn bank_id(&self) -> ParameterBankId {
        if self.inner.is_project_mode {
            ParameterBankId::Project
        } else {
            ParameterBankId::CursorTrack
        }
    }

    /// Toggle between project and track parameters.
    pub fn toggle_mode(&mut self, ctx: &mut Ctx) {
        self.inner.toggle_mode(ctx);
    }

    fn notify_page(&self, ctx: &mut Ctx) {
        if self.inner.is_project_mode {
            mv_helper::notify_selected_project_parameter_page(ctx);
        } else {
            mv_helper::notify_selected_track_parameter_page(ctx);
        }
    }
}

impl Mode for MaschineJamUserMode {
    // ProjectParamsMode
    crate::forward_mode!(inner => name, on_activate, on_deactivate, knob_value, is_any_knob_touched,
        on_button, select_item, selected_item_name, select_previous_item, select_next_item,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page,
        has_next_item_page, on_knob_touch);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let item = ParamRef::BankItem { bank: self.bank_id(), index };
        if item.does_exist(ctx) {
            self.slow_change.change_value(ctx, &item, value);
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.inner.select_previous_item(ctx);
        self.notify_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.inner.select_next_item(ctx);
        self.notify_page(ctx);
    }

    crate::impl_as_any!();
}

impl MaschineJamMode for MaschineJamUserMode {
    fn setup_fader(&self, ctx: &Ctx, index: usize) -> FaderConfig {
        let item = ParamRef::BankItem { bank: self.bank_id(), index };
        if !item.does_exist(ctx) {
            return FADER_OFF_SINGLE;
        }
        let value = ctx.model.value_changer().to_midi_value(item.value(ctx));
        FaderConfig::new(FaderConfig::TYPE_SINGLE, if self.inner.is_project_mode { COLOR_WHITE } else { COLOR_AMBER }, value)
    }
}

/// Get the configuration of the touch strips from the active mode (former `instanceof
/// IMaschineJamMode` in `updateFaders`).
pub fn setup_faders_of_active_mode(ctx: &Ctx) -> Option<[FaderConfig; 8]> {
    let mode = ctx.active_mode()?;
    let mode = mode.try_borrow().ok()?;
    let any = mode.as_any();
    let jam_mode: &dyn MaschineJamMode = if let Some(m) = any.downcast_ref::<MaschineJamVolumeMode>() {
        m
    } else if let Some(m) = any.downcast_ref::<MaschineJamPanMode>() {
        m
    } else if let Some(m) = any.downcast_ref::<MaschineJamSendMode>() {
        m
    } else if let Some(m) = any.downcast_ref::<MaschineJamTrackMode>() {
        m
    } else if let Some(m) = any.downcast_ref::<MaschineJamParameterMode>() {
        m
    } else {
        any.downcast_ref::<MaschineJamUserMode>()?
    };
    Some(std::array::from_fn(|i| jam_mode.setup_fader(ctx, i)))
}

/// Toggle the project/track mode of the user mode (former cast in `MaschineJamControlCommand`).
pub fn toggle_user_mode(ctx: &mut Ctx) {
    let Some(mode) = ctx.modes.get(Modes::User) else {
        return;
    };
    let Ok(mut mode) = mode.try_borrow_mut() else {
        return;
    };
    if let Some(user_mode) = mode.as_any_mut().downcast_mut::<MaschineJamUserMode>() {
        user_mode.toggle_mode(ctx);
    }
}

