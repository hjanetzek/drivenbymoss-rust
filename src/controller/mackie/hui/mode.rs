// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the HUI (former `hui.mode`: `TrackMode`, `VolumeMode`, `PanMode`, `SendMode`
//! on `AbstractTrackMode`, and `DeviceParamsMode`). The knobs are not bound to parameters, the
//! modes handle their values.
//!
//! Differences to Java:
//! - All mode classes are one type [`HuiMode`] with a [`HuiModeKind`] (the setup updates the
//!   knob LEDs of the active mode, former `instanceof HUIMode`).
//! - The name of a send mode contains the name of the send of the selected track (former
//!   dynamic `getName`); it is updated when the mode is activated and when its display is
//!   updated.

use super::definition::{HUI_BUTTON_STATE_OFF, HUI_BUTTON_STATE_ON, KNOB_LED_MODE_BOOST_CUT, KNOB_LED_MODE_OFF, KNOB_LED_MODE_WRAP};
use super::surface::{extender_offset, hui, set_knob_led};
use crate::config::hui::HuiConfig;
use crate::controller::mackie::shorten_and_fix_ascii;
use crate::daw::constants::DeviceId;
use crate::daw::events::ParameterBankId;
use crate::daw::track::Track;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::ParamRef;
use crate::midi::ButtonEvent;

const SEND_NAMES: [&str; 6] = ["Send A", "Send B", "Send C", "Send D", "Send E", "Send F"];
const SEND_NAMES_SHORT: [&str; 6] = ["SndA", "SndB", "SndC", "SndD", "SndE", "SndF"];

/// Which Java mode class a [`HuiMode`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HuiModeKind {
    Track,
    Volume,
    Pan,
    Send(usize),
    /// The parameters of the cursor device.
    DeviceParams,
    /// The parameters of the equalizer.
    EqParams,
}

/// A HUI mode.
pub struct HuiMode {
    pub kind: HuiModeKind,
    pub base: ParameterModeBase,
    name: String,
}

/// The track of the page at the index if it exists (former `DefaultTrackMode.getTrack`).
fn existing_track(ctx: &Ctx, index: usize) -> Option<&Track> {
    let bank = ctx.model.current_track_bank();
    (index < bank.page_size()).then(|| bank.item(index)).filter(|t| t.does_exist())
}

impl HuiMode {
    pub fn new(kind: HuiModeKind) -> Self {
        let (name, bank) = match kind {
            HuiModeKind::Track => ("Track", ModeBank::TrackBank),
            HuiModeKind::Volume => ("Volume", ModeBank::TrackBank),
            HuiModeKind::Pan => ("Panning", ModeBank::TrackBank),
            HuiModeKind::Send(send) => (SEND_NAMES[send], ModeBank::TrackBank),
            HuiModeKind::DeviceParams => ("Parameters", ModeBank::ParameterBank(ParameterBankId::CursorDevice)),
            HuiModeKind::EqParams => ("Equalizer", ModeBank::ParameterBank(ParameterBankId::Eq)),
        };
        HuiMode { kind, base: ParameterModeBase::without_controls(name, false, bank), name: name.to_string() }
    }

    fn parameter_bank(&self) -> ParameterBankId {
        if self.kind == HuiModeKind::EqParams {
            ParameterBankId::Eq
        } else {
            ParameterBankId::CursorDevice
        }
    }

    /// The track of the surface at the index (former `AbstractTrackMode.getTrack`).
    fn track(ctx: &Ctx, index: usize) -> Option<&Track> {
        existing_track(ctx, extender_offset(ctx) + index)
    }

    fn track_index(ctx: &Ctx, index: usize) -> Option<usize> {
        let position = extender_offset(ctx) + index;
        existing_track(ctx, position).map(|_| position)
    }

    /// Former `SendMode.getName`.
    fn update_name(&mut self, ctx: &Ctx) {
        if let HuiModeKind::Send(send) = self.kind {
            let name = ctx
                .model
                .track_bank()
                .selected_item()
                .map(|t| t.send_bank())
                .filter(|b| send < b.item_count() && b.item(send).does_exist())
                .map_or_else(|| "-".to_string(), |b| b.item(send).name().to_string());
            self.name = format!("{}: {name}", SEND_NAMES[send]);
        }
    }

    /// Former `drawTrackHeader`: the track names and a label in the last cell.
    fn draw_track_header(ctx: &mut Ctx, label: &str) {
        let names: Vec<Option<String>> = (0..8).map(|i| Self::track(ctx, i).map(|t| shorten_and_fix_ascii(t.name(), 4))).collect();
        if let Some(d) = ctx.text_display.as_mut() {
            d.clear();
            for (i, name) in names.iter().enumerate() {
                if let Some(name) = name {
                    d.set_cell(0, i, name);
                }
            }
            d.set_cell(0, 8, label).done(0);
        }
    }

    /// Former `resetParameter`.
    pub fn reset_parameter(&self, ctx: &mut Ctx, index: usize) {
        let offset = extender_offset(ctx);
        match self.kind {
            HuiModeKind::Track => {
                let Some(selected) = ctx.model.track_bank().selected_index() else {
                    return;
                };
                if offset > 0 {
                    return;
                }
                let track = ctx.model.track_bank_mut().item_mut(selected);
                match index {
                    0 => track.reset_volume(),
                    1 => track.reset_pan(),
                    _ => ParamRef::SelectedTrackSend { send: index - 2 }.reset_value(ctx),
                }
            }
            HuiModeKind::Volume => {
                if let Some(t) = Self::track_index(ctx, index) {
                    ctx.model.current_track_bank_mut().item_mut(t).reset_volume();
                }
            }
            HuiModeKind::Pan => {
                if let Some(t) = Self::track_index(ctx, index) {
                    ctx.model.current_track_bank_mut().item_mut(t).reset_pan();
                }
            }
            HuiModeKind::Send(send) => {
                if let Some(t) = Self::track_index(ctx, index) {
                    ParamRef::TrackSend { index: t, send }.reset_value(ctx);
                }
            }
            HuiModeKind::DeviceParams | HuiModeKind::EqParams => {
                if offset > 0 {
                    return;
                }
                let parameter = ParamRef::BankItem { bank: self.parameter_bank(), index };
                if parameter.does_exist(ctx) {
                    parameter.reset_value(ctx);
                }
            }
        }
    }

    /// Update the LED rings of the knobs (former `HUIMode.updateKnobLEDs`).
    pub fn update_knob_leds(&self, ctx: &mut Ctx) {
        let upper_bound = ctx.model.value_changer().upper_bound();
        let offset = extender_offset(ctx);
        match self.kind {
            HuiModeKind::Track => {
                let values = ctx.model.track_bank().selected_item().filter(|_| offset == 0).map(|track| {
                    let sends = track.send_bank();
                    let send = |i: usize| if i < sends.item_count() { sends.item(i).value() } else { 0 };
                    [track.volume(), track.pan(), send(0), send(1), send(2), send(3), send(4), send(5)]
                });
                match values {
                    Some(values) => {
                        for (i, value) in values.iter().enumerate() {
                            let mode = if i == 1 { KNOB_LED_MODE_BOOST_CUT } else { KNOB_LED_MODE_WRAP };
                            set_knob_led(ctx, i, mode, (*value).max(1), upper_bound);
                        }
                    }
                    None => {
                        for i in 0..8 {
                            set_knob_led(ctx, i, KNOB_LED_MODE_OFF, 0, 0);
                        }
                    }
                }
            }
            HuiModeKind::Volume => {
                for i in 0..8 {
                    let value = Self::track(ctx, i).map_or(0, |t| t.volume());
                    set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, value, upper_bound);
                }
            }
            HuiModeKind::Pan => {
                for i in 0..8 {
                    match Self::track(ctx, i).map(|t| t.pan()) {
                        Some(pan) => set_knob_led(ctx, i, KNOB_LED_MODE_BOOST_CUT, pan.max(1), upper_bound),
                        None => set_knob_led(ctx, i, KNOB_LED_MODE_OFF, 0, 0),
                    }
                }
            }
            HuiModeKind::Send(send) => {
                for i in 0..8 {
                    let value = Self::track(ctx, i).map(|t| t.send_bank()).filter(|b| send < b.item_count()).map_or(0, |b| b.item(send).value());
                    set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, value, upper_bound);
                }
            }
            HuiModeKind::DeviceParams | HuiModeKind::EqParams => {
                for i in 0..8 {
                    let parameter = ParamRef::BankItem { bank: self.parameter_bank(), index: i };
                    if offset == 0 && parameter.does_exist(ctx) {
                        let value = parameter.value(ctx).max(1);
                        set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, value, upper_bound);
                    } else {
                        set_knob_led(ctx, i, KNOB_LED_MODE_OFF, 0, 0);
                    }
                }
            }
        }
    }

    /// Former `DeviceParamsMode.trackSelectionChanged`: add an equalizer to the selected track if
    /// not present and this is the main device (no extender).
    pub fn track_selection_changed(&self, ctx: &mut Ctx, is_selected: bool) {
        if !is_selected || self.kind != HuiModeKind::EqParams || !self.base.is_active() || hui(ctx).index != 0 {
            return;
        }
        if ctx.model.specific_device(DeviceId::Eq).is_some_and(|d| d.does_exist()) {
            return;
        }
        if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
            track.add_equalizer_device();
        }
    }
}

impl Mode for HuiMode {
    crate::forward_mode!(base => select_item, has_previous_item, has_next_item, has_previous_item_page, has_next_item_page, select_previous_item,
        select_next_item, select_item_page);

    fn name(&self) -> &str {
        &self.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
        self.update_name(ctx);
        self.track_selection_changed(ctx, true);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let offset = extender_offset(ctx);
        match self.kind {
            HuiModeKind::Track => {
                if offset > 0 {
                    return;
                }
                let Some(selected) = ctx.model.track_bank().selected_index() else {
                    return;
                };
                match index {
                    0 => ctx.model.track_bank_mut().item_mut(selected).change_volume(value),
                    1 => ctx.model.track_bank_mut().item_mut(selected).change_pan(value),
                    _ => ParamRef::SelectedTrackSend { send: index - 2 }.change_value(ctx, value),
                }
            }
            HuiModeKind::Volume => {
                if let Some(t) = Self::track_index(ctx, index) {
                    ctx.model.current_track_bank_mut().item_mut(t).change_volume(value);
                }
            }
            HuiModeKind::Pan => {
                if let Some(t) = Self::track_index(ctx, index) {
                    ctx.model.current_track_bank_mut().item_mut(t).change_pan(value);
                }
            }
            HuiModeKind::Send(send) => {
                if let Some(t) = Self::track_index(ctx, index) {
                    ParamRef::TrackSend { index: t, send }.change_value(ctx, value);
                }
            }
            HuiModeKind::DeviceParams | HuiModeKind::EqParams => {
                if offset > 0 {
                    return;
                }
                let parameter = ParamRef::BankItem { bank: self.parameter_bank(), index };
                if parameter.does_exist(ctx) {
                    parameter.change_value(ctx, value);
                }
            }
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, _event: ButtonEvent) {
        // Same as Java: on all events
        if row == 0 {
            self.reset_parameter(ctx, index);
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if matches!(self.kind, HuiModeKind::DeviceParams | HuiModeKind::EqParams) {
            return HUI_BUTTON_STATE_OFF;
        }
        let state = |b: bool| if b { HUI_BUTTON_STATE_ON } else { HUI_BUTTON_STATE_OFF };
        for i in 0..8 {
            let track = Self::track(ctx, i);
            // HUI_INSERT1 is used on icon for selection
            if button == ButtonId::get(ButtonId::RowSelect1, i) || button == ButtonId::get(ButtonId::Row51, i) {
                return state(track.is_some_and(|t| t.is_selected()));
            }
            if button == ButtonId::get(ButtonId::Row21, i) {
                return state(track.is_some_and(|t| t.is_rec_arm()));
            }
            if button == ButtonId::get(ButtonId::Row31, i) {
                return state(track.is_some_and(|t| t.is_solo()));
            }
            if button == ButtonId::get(ButtonId::Row41, i) {
                return state(track.is_some_and(|t| t.is_mute()));
            }
        }
        HUI_BUTTON_STATE_OFF
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        self.update_name(ctx);
        let has_display = ctx.config.has_display1();
        match self.kind {
            HuiModeKind::Volume if has_display => Self::draw_track_header(ctx, "Volu"),
            HuiModeKind::Pan if has_display => Self::draw_track_header(ctx, "Pans"),
            HuiModeKind::Send(send) if has_display => Self::draw_track_header(ctx, SEND_NAMES_SHORT[send]),
            HuiModeKind::Track if has_display => {
                let show = ctx.model.track_bank().selected_item().is_some() && extender_offset(ctx) == 0;
                if let Some(d) = ctx.text_display.as_mut() {
                    d.clear();
                    if show {
                        for (i, label) in ["Vol", "Pan", "S1", "S2", "S3", "S4", "S5", "S6"].iter().enumerate() {
                            d.set_cell(0, i, label);
                        }
                    } else {
                        d.set_row(0, "Please select a track.");
                    }
                    d.done(0);
                }
            }
            HuiModeKind::DeviceParams | HuiModeKind::EqParams => {
                let bank = self.parameter_bank();
                let names: Vec<Option<String>> = (0..8)
                    .map(|i| {
                        let parameter = ParamRef::BankItem { bank, index: i };
                        parameter.does_exist(ctx).then(|| shorten_and_fix_ascii(&parameter.name(ctx), 4))
                    })
                    .collect();
                if let Some(d) = ctx.text_display.as_mut() {
                    d.clear();
                    for (i, name) in names.iter().enumerate() {
                        if let Some(name) = name {
                            d.set_cell(0, i, name);
                        }
                    }
                    d.done(0);
                }
            }
            _ => {}
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        match self.kind {
            HuiModeKind::DeviceParams | HuiModeKind::EqParams => ctx.model.cursor_device_mut().select_previous(),
            _ => self.base.select_previous_item_page(ctx),
        }
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        match self.kind {
            HuiModeKind::DeviceParams | HuiModeKind::EqParams => ctx.model.cursor_device_mut().select_next(),
            _ => self.base.select_next_item_page(ctx),
        }
    }

    crate::impl_as_any!();
}
