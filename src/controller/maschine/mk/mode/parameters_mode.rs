// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Edit the parameters of the cursor device (former `MaschineParametersMode`).

use super::{shorten_and_fix_ascii, with_display, TouchStates, MAIN_ENCODER};
use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::maschine::mk::surface::maschine;
use crate::daw::events::ParameterBankId;
use crate::framework::commands::mv_helper::{notify_selected_device, notify_selected_parameter_page};
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ButtonId;
use crate::framework::mode::device::SelectedDeviceMode;
use crate::framework::mode::parameter_mode::default_knob_ids;
use crate::framework::parameterprovider::{provider, BankParameterProvider};
use crate::impl_as_any;

fn never(_ctx: &Ctx) -> bool {
    false
}

/// Translate the touch index of the main encoder (8) to the index of the selected parameter.
fn touch_index(index: usize) -> usize {
    if index == 8 {
        MAIN_ENCODER
    } else {
        index
    }
}

/// Edit the parameters of the cursor device.
pub struct MaschineParametersMode {
    inner: SelectedDeviceMode,
    touch: TouchStates,
}

impl MaschineParametersMode {
    pub fn new(ctx: &Ctx, has_mcu_display: bool) -> Self {
        let knobs = if has_mcu_display { default_knob_ids() } else { Vec::new() };
        let mut inner = SelectedDeviceMode::new(ctx, knobs).with_alternative_function(never);
        if has_mcu_display {
            inner.base.set_parameter_provider(ctx, provider(BankParameterProvider::new(ctx, ParameterBankId::CursorDevice)));
        }
        MaschineParametersMode { inner, touch: TouchStates::default() }
    }

    pub fn select_parameter(&mut self, index: i32) {
        self.inner.select_parameter(index);
    }

    pub fn selected_parameter(&self) -> usize {
        self.inner.selected_parameter()
    }

    fn is_studio_track_combination(ctx: &Ctx) -> bool {
        maschine(ctx) == Maschine::Studio && ctx.surface.is_pressed(ButtonId::Track)
    }
}

impl Mode for MaschineParametersMode {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.inner.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.inner.on_deactivate(ctx);
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.inner.on_knob_value(ctx, index, value);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.inner.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.touch.set(index, is_touched);
        self.inner.on_knob_touch(ctx, touch_index(index), is_touched);
    }

    fn is_any_knob_touched(&self) -> bool {
        self.touch.is_any_touched()
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let device = ctx.model.cursor_device();
        if !device.does_exist() {
            ctx.notify("Please select a device...");
            return;
        }
        let selected = self.selected_parameter();
        let cells: Vec<(String, String)> = (0..8)
            .map(|i| match device.parameter_bank() {
                Some(bank) => {
                    let param = bank.item(i);
                    let mut name = if param.does_exist() { shorten_and_fix_ascii(param.name(), 6) } else { String::new() };
                    if i == selected {
                        name = format!(">{name}");
                    }
                    (name, shorten_and_fix_ascii(&param.displayed_value_limited(8), 8))
                }
                None => (if i == selected { ">".to_string() } else { String::new() }, String::new()),
            })
            .collect();
        with_display(ctx, |d| {
            d.clear();
            for (i, (name, value)) in cells.iter().enumerate() {
                d.set_cell(0, i, name).set_cell(1, i, value);
            }
            d.all_done();
        });
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.inner.select_item(ctx, index);
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if Self::is_studio_track_combination(ctx) {
            ctx.model.cursor_device_mut().select_previous();
            notify_selected_device(ctx);
            return;
        }
        let selected = self.selected_parameter();
        if selected == 0 {
            self.inner.select_previous_item(ctx);
            self.select_parameter(7);
        } else {
            self.select_parameter(selected as i32 - 1);
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        if Self::is_studio_track_combination(ctx) {
            ctx.model.cursor_device_mut().select_next();
            notify_selected_device(ctx);
            return;
        }
        let selected = self.selected_parameter();
        if selected == 7 {
            self.inner.select_next_item(ctx);
            self.select_parameter(0);
        } else {
            self.select_parameter(selected as i32 + 1);
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.inner.select_previous_item(ctx);
        notify_selected_parameter_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.inner.select_next_item(ctx);
        notify_selected_parameter_page(ctx);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.selected_parameter() > 0 || self.inner.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.selected_parameter() < 7 || self.inner.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.inner.has_previous_item(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.inner.has_next_item(ctx)
    }

    impl_as_any!();
}
