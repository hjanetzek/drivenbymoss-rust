// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Device modes (former `framework.mode.device`: `ParameterMode`, `SelectedDeviceMode` and
//! `ProjectParamsMode`).
//!
//! All of them are complete [`Mode`]s but can also be used as the base of a controller specific
//! mode (the struct is public, its `base` is a [`ParameterModeBase`]). The functions of the
//! modes are also available as inherent functions (e.g. [`ParameterMode::select_previous`])
//! so that controller modes which embed them can call the "super" implementation.

use crate::daw::events::ParameterBankId;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, ContinuousId};
use crate::framework::mode::parameter_mode::{AlternativeFunction, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, BankParameterProvider, ParamRef};
use crate::midi::ButtonEvent;

// ------------------------------------------------------------------------------------------
// ParameterMode

/// Edit the parameters of the cursor device (former `ParameterMode`).
pub struct ParameterMode {
    pub base: ParameterModeBase,
}

impl ParameterMode {
    /// Former `ParameterMode (surface, model, isAbsolute, controls)`. Without controls (empty
    /// list) the knobs are not bound and `on_knob_value` changes the parameters.
    pub fn new(ctx: &Ctx, is_absolute: bool, controls: Vec<ContinuousId>) -> Self {
        Self::with_name("Parameters", ctx, is_absolute, controls)
    }

    /// Former `ParameterMode (name, surface, model, isAbsolute, controls, isAlternativeFunction)`.
    pub fn with_name(name: &str, ctx: &Ctx, is_absolute: bool, controls: Vec<ContinuousId>) -> Self {
        let has_controls = !controls.is_empty();
        let mut base = ParameterModeBase::new(name, is_absolute, ModeBank::ParameterBank(ParameterBankId::CursorDevice), controls);
        if has_controls {
            base.set_parameter_provider(ctx, provider(BankParameterProvider::new(ctx, ParameterBankId::CursorDevice)));
        }
        ParameterMode { base }
    }

    pub fn with_alternative_function(mut self, is_alternative_function: AlternativeFunction) -> Self {
        self.base.is_alternative_function = is_alternative_function;
        self
    }

    fn item(index: usize) -> ParamRef {
        ParamRef::BankItem { bank: ParameterBankId::CursorDevice, index }
    }

    pub fn knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !ctx.model.cursor_device().does_exist() {
            return;
        }
        let item = Self::item(index);
        if !item.does_exist(ctx) {
            return;
        }
        if self.base.is_absolute {
            item.set_value(ctx, value);
        } else {
            item.change_value(ctx, value);
        }
    }

    pub fn knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        if !ctx.model.cursor_device().does_exist() {
            return;
        }
        let item = Self::item(index);
        if !item.does_exist(ctx) {
            return;
        }
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            item.reset_value(ctx);
        }
        item.touch_value(ctx, is_touched);
    }

    pub fn value_of_knob(ctx: &Ctx, index: usize) -> i32 {
        if !ctx.model.cursor_device().does_exist() {
            return -1;
        }
        let item = Self::item(index);
        if item.does_exist(ctx) {
            item.value(ctx)
        } else {
            -1
        }
    }

    /// The name of the device and the selected parameter page.
    pub fn selected_name(ctx: &Ctx) -> Option<String> {
        let device = ctx.model.cursor_device();
        if !device.does_exist() {
            return None;
        }
        let mut value = device.name().to_string();
        if let Some(bank) = device.parameter_bank() {
            let page = bank.page_bank().selected_item();
            if !page.is_empty() {
                value = format!("{value} - {page}");
            }
        }
        Some(value)
    }

    pub fn select_previous(&mut self, ctx: &mut Ctx) {
        if (self.base.is_alternative_function)(ctx) {
            ctx.model.cursor_device_mut().device_bank_mut().select_previous_page();
        } else {
            ctx.model.cursor_device_mut().select_previous();
        }
    }

    pub fn select_next(&mut self, ctx: &mut Ctx) {
        if (self.base.is_alternative_function)(ctx) {
            ctx.model.cursor_device_mut().device_bank_mut().select_next_page();
        } else {
            ctx.model.cursor_device_mut().select_next();
        }
    }

    pub fn select_previous_page(ctx: &mut Ctx) {
        if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            bank.scroll_backwards();
        }
    }

    pub fn select_next_page(ctx: &mut Ctx) {
        if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            bank.scroll_forwards();
        }
    }

    pub fn has_previous(&self, ctx: &Ctx) -> bool {
        let device = ctx.model.cursor_device();
        if (self.base.is_alternative_function)(ctx) {
            device.device_bank().can_scroll_page_backwards()
        } else {
            device.can_select_previous()
        }
    }

    pub fn has_next(&self, ctx: &Ctx) -> bool {
        let device = ctx.model.cursor_device();
        if (self.base.is_alternative_function)(ctx) {
            device.device_bank().can_scroll_page_forwards()
        } else {
            device.can_select_next()
        }
    }
}

impl Mode for ParameterMode {
    // AbstractParameterMode
    crate::forward_mode!(base => name, on_activate, on_deactivate, is_any_knob_touched,
        select_item_page, has_previous_item_page, has_next_item_page);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.knob(ctx, index, value);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        Self::value_of_knob(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.knob_touch(ctx, index, is_touched);
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            bank.select_page(index);
        }
    }

    fn selected_item_name(&self, ctx: &Ctx) -> Option<String> {
        Self::selected_name(ctx)
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.select_previous(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.select_next(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        Self::select_previous_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        Self::select_next_page(ctx);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.has_previous(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.has_next(ctx)
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// SelectedDeviceMode

/// Edit the parameters of the cursor device with relative knobs; knob index -1 edits the
/// selected parameter (former `SelectedDeviceMode`).
pub struct SelectedDeviceMode {
    pub base: ParameterModeBase,
    sel_param: i32,
}

impl SelectedDeviceMode {
    pub fn new(_ctx: &Ctx, knobs: Vec<ContinuousId>) -> Self {
        SelectedDeviceMode { base: ParameterModeBase::new("Parameters", false, ModeBank::ParameterBank(ParameterBankId::CursorDevice), knobs), sel_param: 0 }
    }

    pub fn with_alternative_function(mut self, is_alternative_function: AlternativeFunction) -> Self {
        self.base.is_alternative_function = is_alternative_function;
        self
    }

    fn item(&self, index: i32) -> ParamRef {
        let index = if index < 0 { self.sel_param } else { index };
        ParamRef::BankItem { bank: ParameterBankId::CursorDevice, index: index.max(0) as usize }
    }

    /// Edit the parameter (a negative index edits the selected parameter).
    pub fn knob(&mut self, ctx: &mut Ctx, index: i32, value: i32) {
        let item = self.item(index);
        if item.does_exist(ctx) {
            item.change_value(ctx, value);
        }
    }

    pub fn knob_touch(&mut self, ctx: &mut Ctx, index: i32, is_touched: bool) {
        let item = self.item(index);
        if !item.does_exist(ctx) {
            return;
        }
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            item.reset_value(ctx);
        }
        item.touch_value(ctx, is_touched);
    }

    pub fn select_parameter(&mut self, index: i32) {
        self.sel_param = index;
    }

    pub fn selected_parameter(&self) -> usize {
        if self.sel_param < 0 {
            0
        } else {
            self.sel_param as usize
        }
    }
}

impl Mode for SelectedDeviceMode {
    // AbstractParameterMode
    crate::forward_mode!(base => name, on_activate, on_deactivate, knob_value, select_item,
        select_previous_item, select_next_item, select_previous_item_page, select_next_item_page,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page,
        has_next_item_page);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.knob(ctx, index as i32, value);
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.knob_touch(ctx, index as i32, is_touched);
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// ProjectParamsMode

/// Edit the project parameters or the parameters of the cursor track (former
/// `ProjectParamsMode`).
pub struct ProjectParamsMode {
    pub base: ParameterModeBase,
    pub is_project_mode: bool,
    pub notify_page_change: bool,
    has_providers: bool,
}

impl ProjectParamsMode {
    /// Former `ProjectParamsMode (surface, model, isAbsolute, knobs)`.
    pub fn new(ctx: &Ctx, is_absolute: bool, knobs: Vec<ContinuousId>) -> Self {
        Self::with_options(ctx, is_absolute, knobs, true)
    }

    /// Former `ProjectParamsMode (surface, model, isAbsolute, knobs, isAlternativeFunction,
    /// addParameterProvider)`.
    pub fn with_options(ctx: &Ctx, is_absolute: bool, knobs: Vec<ContinuousId>, add_parameter_provider: bool) -> Self {
        let has_providers = add_parameter_provider && !knobs.is_empty();
        let mut base = ParameterModeBase::new("Project Parameters", is_absolute, ModeBank::ParameterBank(ParameterBankId::Project), knobs);
        if has_providers {
            base.set_parameter_provider(ctx, provider(BankParameterProvider::new(ctx, ParameterBankId::Project)));
        }
        ProjectParamsMode { base, is_project_mode: true, notify_page_change: true, has_providers }
    }

    pub fn with_alternative_function(mut self, is_alternative_function: AlternativeFunction) -> Self {
        self.base.is_alternative_function = is_alternative_function;
        self
    }

    fn bank_id(&self) -> ParameterBankId {
        if self.is_project_mode {
            ParameterBankId::Project
        } else {
            ParameterBankId::CursorTrack
        }
    }

    pub fn toggle_mode(&mut self, ctx: &mut Ctx) {
        let mode = !self.is_project_mode;
        self.set_mode(ctx, mode);
    }

    pub fn set_mode(&mut self, ctx: &mut Ctx, is_project_mode: bool) {
        self.is_project_mode = is_project_mode;
        let bank = self.bank_id();
        self.base.switch_banks(ModeBank::ParameterBank(bank));
        if self.has_providers {
            self.base.set_parameter_provider(ctx, provider(BankParameterProvider::new(ctx, bank)));
        }
        self.base.bind_controls(ctx);
    }

    pub fn knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        let param = ParamRef::BankItem { bank: self.bank_id(), index };
        if !param.does_exist(ctx) {
            return;
        }
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            param.reset_value(ctx);
        }
        param.touch_value(ctx, is_touched);
    }

    fn do_notify_page(&self, ctx: &mut Ctx) {
        if self.notify_page_change {
            if self.is_project_mode {
                mv_helper::notify_selected_project_parameter_page(ctx);
            } else {
                mv_helper::notify_selected_track_parameter_page(ctx);
            }
        }
    }
}

impl Mode for ProjectParamsMode {
    fn name(&self) -> &str {
        if self.is_project_mode {
            "Project Parameters"
        } else {
            "Track Parameters"
        }
    }

    // AbstractParameterMode
    crate::forward_mode!(base => on_activate, on_deactivate, knob_value, is_any_knob_touched,
        select_previous_item_page, select_next_item_page, select_item_page, has_previous_item,
        has_next_item, has_previous_item_page, has_next_item_page, select_item);

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let param = ParamRef::BankItem { bank: self.bank_id(), index };
        if self.base.is_absolute {
            param.set_value(ctx, value);
        } else {
            param.change_value(ctx, value);
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.knob_touch(ctx, index, is_touched);
    }

    fn on_button(&mut self, _ctx: &mut Ctx, _row: usize, _index: usize, _event: ButtonEvent) {}

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item(ctx);
        self.do_notify_page(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item(ctx);
        self.do_notify_page(ctx);
    }

    crate::impl_as_any!();
}
