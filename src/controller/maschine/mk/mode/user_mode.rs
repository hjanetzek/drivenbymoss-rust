// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Edit the project or track parameters (former `MaschineUserMode`).

use super::{shorten_and_fix_ascii, with_display, TouchStates, MAIN_ENCODER};
use crate::daw::events::ParameterBankId;
use crate::framework::commands::mv_helper::{notify_selected_project_parameter_page, notify_selected_track_parameter_page};
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::mode::device::ProjectParamsMode;
use crate::framework::mode::parameter_mode::default_knob_ids;
use crate::framework::parameterprovider::ParamRef;
use crate::impl_as_any;

/// Edit the project or track parameters.
pub struct MaschineUserMode {
    inner: ProjectParamsMode,
    touch: TouchStates,
    sel_param: i32,
}

impl MaschineUserMode {
    pub fn new(ctx: &Ctx) -> Self {
        MaschineUserMode { inner: ProjectParamsMode::new(ctx, false, default_knob_ids()), touch: TouchStates::default(), sel_param: 0 }
    }

    fn bank_id(&self) -> ParameterBankId {
        if self.inner.is_project_mode {
            ParameterBankId::Project
        } else {
            ParameterBankId::CursorTrack
        }
    }

    fn parameter(&self, index: usize) -> ParamRef {
        ParamRef::BankItem { bank: self.bank_id(), index }
    }

    pub fn select_parameter(&mut self, index: i32) {
        self.sel_param = index;
    }

    pub fn selected_parameter(&self) -> usize {
        if self.sel_param == -1 {
            0
        } else {
            self.sel_param as usize
        }
    }

    fn notify_page(&self, ctx: &mut Ctx) {
        if self.inner.is_project_mode {
            notify_selected_project_parameter_page(ctx);
        } else {
            notify_selected_track_parameter_page(ctx);
        }
    }

    /// Switch between the project (true) and the track parameters (former `setMode`).
    pub fn set_mode(&mut self, ctx: &mut Ctx, is_project_mode: bool) {
        self.inner.set_mode(ctx, is_project_mode);
        self.notify_page(ctx);
    }

    pub fn is_project_mode(&self) -> bool {
        self.inner.is_project_mode
    }
}

impl Mode for MaschineUserMode {
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
        let index = if index == MAIN_ENCODER { self.selected_parameter() } else { index };
        let param = self.parameter(index);
        if param.does_exist(ctx) {
            param.change_value(ctx, value);
        }
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.inner.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.touch.set(index, is_touched);
        if index < 8 {
            self.inner.on_knob_touch(ctx, index, is_touched);
        }
    }

    fn is_any_knob_touched(&self) -> bool {
        self.touch.is_any_touched()
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let selected = self.selected_parameter();
        let cells: Vec<(String, String)> = (0..8)
            .map(|i| {
                let param = self.parameter(i);
                let mut name = if param.does_exist(ctx) { shorten_and_fix_ascii(&param.name(ctx), 6) } else { String::new() };
                if i == selected {
                    name = format!(">{name}");
                }
                (name, param.displayed_value(ctx))
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
        let selected = self.selected_parameter();
        if selected == 0 {
            self.inner.select_previous_item(ctx);
            self.select_parameter(7);
        } else {
            self.select_parameter(selected as i32 - 1);
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
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
        self.notify_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.inner.select_next_item(ctx);
        self.notify_page(ctx);
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
