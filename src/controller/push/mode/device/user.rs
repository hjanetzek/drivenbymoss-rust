// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The project and track parameters mode (former `UserMode`).

use crate::daw::color::ColorEx;
use crate::daw::events::ParameterBankId;
use crate::daw::parameter_bank::ParameterBank;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::Mode;
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::{button_row_index, default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{parameter_bank, provider, BankParameterProvider, SharedProvider};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::display1;
use super::super::super::surface as push;
use super::super::track::{color_off, limit};
use super::super::{check_stop_automation_on_knob_release, update_display, PushDisplayMode};

const TOP_MENU: [&str; 8] = ["Project", " ", " ", " ", " ", " ", " ", " "];

/// The project and track parameters mode (former `UserMode`).
pub struct UserMode {
    pub base: ParameterModeBase,
    project_parameter_provider: SharedProvider,
    track_parameter_provider: SharedProvider,
    is_project_mode: bool,
}

impl UserMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = ParameterModeBase::new("Project/Track Controls", true, ModeBank::ParameterBank(ParameterBankId::Project), default_knob_ids());
        let project_parameter_provider = provider(BankParameterProvider::new(ctx, ParameterBankId::Project));
        let track_parameter_provider = provider(BankParameterProvider::new(ctx, ParameterBankId::CursorTrack));
        base.set_parameter_provider(ctx, project_parameter_provider.clone());
        UserMode { base, project_parameter_provider, track_parameter_provider, is_project_mode: true }
    }

    fn bank_id(&self) -> ParameterBankId {
        if self.is_project_mode {
            ParameterBankId::Project
        } else {
            ParameterBankId::CursorTrack
        }
    }

    fn bank<'a>(&self, ctx: &'a Ctx) -> Option<&'a ParameterBank> {
        parameter_bank(&ctx.model, self.bank_id())
    }

    /// Former `setMode`.
    fn set_mode(&mut self, ctx: &mut Ctx, is_project_mode: bool) {
        self.is_project_mode = is_project_mode;
        self.base.switch_banks(ModeBank::ParameterBank(self.bank_id()));
        let parameter_provider = if is_project_mode { self.project_parameter_provider.clone() } else { self.track_parameter_provider.clone() };
        self.base.set_parameter_provider(ctx, parameter_provider);
        self.base.bind_controls(ctx);
    }

    fn track_header(ctx: &Ctx) -> String {
        ctx.model.current_track_bank().selected_item().map_or_else(|| "None".to_string(), |t| t.name().to_string())
    }
}

impl PushDisplayMode for UserMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let Some(bank) = self.bank(ctx) else {
            return;
        };
        let track_header = Self::track_header(ctx);

        // Row 1 & 2
        for i in 0..bank.page_size() {
            let param = bank.item(i);
            display.set_cell(0, i, &if param.does_exist() { crate::config::values::fix_ascii(param.name()) } else { String::new() });
            display.set_cell(1, i, &param.displayed_value_limited(8));
        }

        // Row 3
        display.set_block(2, 0, if self.is_project_mode { "Params : Project" } else { "Params : Track ->" });
        if !self.is_project_mode {
            display.set_block(2, 1, &track_header);
        }

        // Row 4
        let page_bank = bank.page_bank();
        let selected_page = page_bank.selected_item_index();
        for i in 0..page_bank.page_size() {
            let page_name = page_bank.item(i);
            if !page_name.trim().is_empty() {
                let prefix = if i == selected_page { display1::SELECT_ARROW.to_string() } else { String::new() };
                display.set_cell(3, i, &format!("{prefix}{page_name}"));
            }
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let Some(bank) = self.bank(ctx) else {
            return;
        };
        let page_bank = bank.page_bank();
        let vc = ctx.model.value_changer();
        let selected_page = page_bank.selected_item_index();
        let selected_track = ctx.model.current_track_bank().selected_item();
        let track_header = Self::track_header(ctx);
        let track_color = selected_track.map_or(ColorEx::BLACK, |t| t.color());

        for (i, top) in TOP_MENU.iter().enumerate().take(bank.page_size()) {
            let is_bottom_menu_on = i == selected_page;
            let param = bank.item(i);
            let exists = param.does_exist();
            let parameter_name = if exists { param.name_limited(9) } else { String::new() };
            let parameter_value = vc.to_display_value(if exists { param.value() } else { 0 });
            let parameter_value_str = if exists { param.displayed_value_limited(8) } else { String::new() };
            let parameter_is_active = self.base.is_knob_touched(i);
            let parameter_modulated_value = vc.to_display_value(if exists { param.modulated_value() } else { -1 });

            let bottom_menu = limit(&page_bank.item(i), 12);
            let bottom_menu_icon = if self.is_project_mode { "PROJECT" } else { "TRACK" };
            let is_top_menu_selected = i == 0 && self.is_project_mode || i == 1 && !self.is_project_mode;
            let bottom_menu_color = if self.is_project_mode {
                if is_bottom_menu_on {
                    ColorEx::WHITE
                } else {
                    ColorEx::GRAY
                }
            } else {
                track_color
            };

            let top_menu = if i == 1 { track_header.as_str() } else { top };
            display.add_parameter_element_with_device_name(
                top_menu,
                is_top_menu_selected,
                &bottom_menu,
                bottom_menu_icon,
                bottom_menu_color,
                is_bottom_menu_on,
                &parameter_name,
                parameter_value,
                &parameter_value_str,
                parameter_is_active,
                parameter_modulated_value,
            );
        }
    }
}

impl Mode for UserMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let is_modern = push::is_modern(ctx);
        let off_color = if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };

        if let Some(index) = button_row_index(0, button) {
            let selected_color = if is_modern { PUSH2_COLOR_ORANGE_HI } else { PUSH1_COLOR_ORANGE_HI };
            let exists_color = if is_modern { PUSH2_COLOR_YELLOW_LO } else { PUSH1_COLOR_YELLOW_LO };
            let Some(bank) = self.bank(ctx) else {
                return off_color;
            };
            let page_bank = bank.page_bank();
            if page_bank.item(index).trim().is_empty() {
                return off_color;
            }
            return if index == page_bank.selected_item_index() { selected_color } else { exists_color };
        }

        if let Some(index) = button_row_index(1, button) {
            if index > 1 {
                return off_color;
            }
            let selected_color = if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR2_WHITE };
            let exists_color = if is_modern { PUSH2_COLOR2_GREY_LO } else { PUSH1_COLOR2_GREY_LO };
            return if index == 0 && self.is_project_mode || index == 1 && !self.is_project_mode { selected_color } else { exists_color };
        }

        color_off(ctx)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        let delete = is_touched && ctx.surface.is_delete_pressed();
        if delete {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
        }
        if let Some(bank) = crate::framework::parameterprovider::parameter_bank_mut(&mut ctx.model, self.bank_id()) {
            let param = bank.item_mut(index);
            if delete {
                param.reset_value();
            }
            param.touch_value(is_touched);
        }
        check_stop_automation_on_knob_release(ctx, is_touched);
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            if event != ButtonEvent::Up {
                return;
            }
            let id = self.bank_id();
            if let Some(bank) = crate::framework::parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                if !bank.page_bank().item(index).trim().is_empty() {
                    bank.select_page(index);
                }
            }
        } else if event == ButtonEvent::Up && index <= 1 {
            self.set_mode(ctx, index == 0);
        }
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item_page(ctx);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item_page(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item_page(ctx)
    }

    impl_as_any!();
}
