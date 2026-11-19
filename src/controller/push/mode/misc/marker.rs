// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The marker mode (former `MarkerMode`).
//!
//! Differences to Java: the bank of the mode is the marker bank of the model, which the
//! `ModeBank` of the parameter base cannot address; the navigation functions use it directly.
//! The name of a marker without a name is the one of the model ("Marker <index + 1>") instead of
//! "Mark <position>", the model has no access to the unnamed state.

use super::{forward_base, row_index, shorten_and_fix_ascii};
use crate::controller::push::display1::SELECT_ARROW;
use crate::controller::push::mode::PushDisplayMode;
use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR2_ON, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::framework::parameterprovider::{provider, VolumeParameterProvider};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

const EDIT_MENU: [&str; 8] = ["Add Marker", "", "", "", "", "", "Select", "Launch"];

/// Launch, select, add and remove markers. The knobs control the track volumes.
pub struct MarkerMode {
    base: ParameterModeBase,
    action_mode_launch: bool,
}

impl MarkerMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = super::base("Marker");
        base.set_parameter_provider(ctx, provider(VolumeParameterProvider::new(ctx)));
        MarkerMode { base, action_mode_launch: true }
    }

    /// Is the action of the first row buttons launching the marker (or selecting it)?
    pub fn is_action_mode_launch(&self) -> bool {
        self.action_mode_launch
    }

    fn marker_name(ctx: &Ctx, index: usize) -> String {
        let marker = ctx.model.marker_bank().item(index);
        if marker.does_exist() {
            marker.name()
        } else {
            String::new()
        }
    }
}

impl PushDisplayMode for MarkerMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_cell(2, 0, "Markers:");
        for (i, menu) in EDIT_MENU.iter().enumerate() {
            if i == 0 {
                display.set_block(0, i, menu);
            }
            if i == 6 {
                let arrow = if self.action_mode_launch { String::new() } else { SELECT_ARROW.to_string() };
                display.set_cell(0, i, &format!("{arrow}{menu}"));
            }
            if i == 7 {
                let arrow = if self.action_mode_launch { SELECT_ARROW.to_string() } else { String::new() };
                display.set_cell(0, i, &format!("{arrow}{menu}"));
            }
            if ctx.model.marker_bank().item(i).does_exist() {
                display.set_cell(3, i, &shorten_and_fix_ascii(&Self::marker_name(ctx, i), 8));
            }
        }
        display.set_cell(0, 5, "Action:");
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        for (i, menu) in EDIT_MENU.iter().enumerate() {
            let color = ctx.model.marker_bank().item(i).color();
            let header_bottom_name = if i == 0 { "Markers" } else { "" };
            let header_top_name = if i == 6 { "Action" } else { "" };
            let is_menu_top_selected = i == 6 && !self.action_mode_launch || i == 7 && self.action_mode_launch;
            let menu_bottom_name = shorten_and_fix_ascii(&Self::marker_name(ctx, i), 12);
            display.add_option_element_colored(header_top_name, menu, is_menu_top_selected, None, header_bottom_name, &menu_bottom_name, false, Some(color), false);
        }
    }
}

impl Mode for MarkerMode {
    forward_base!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.base.on_knob_value(ctx, index, value);
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.on_knob_touch(ctx, index, is_touched);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if row == 0 {
            if !ctx.model.marker_bank().item(index).does_exist() {
                return;
            }
            if ctx.is_button_combination(ButtonId::Delete) {
                ctx.model.marker_bank_mut().item_mut(index).remove_marker();
                return;
            }
            if self.action_mode_launch {
                ctx.model.marker_bank_mut().item_mut(index).launch(true);
            } else {
                ctx.model.marker_bank_mut().item_mut(index).select();
            }
            return;
        }
        match index {
            0 => ctx.model.marker_bank().add_marker(),
            6 => self.action_mode_launch = false,
            7 => self.action_mode_launch = true,
            _ => {}
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            return if ctx.model.marker_bank().item(index).does_exist() { BUTTON_COLOR_ON } else { BUTTON_COLOR_OFF };
        }
        if let Some(index) = row_index(1, button) {
            return if EDIT_MENU[index].is_empty() { BUTTON_COLOR_OFF } else { BUTTON_COLOR2_ON };
        }
        BUTTON_COLOR_OFF
    }

    // Since markers do not have a selected state we can only scroll the page
    fn select_next_item(&mut self, ctx: &mut Ctx) {
        ctx.model.marker_bank_mut().select_next_page();
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        ctx.model.marker_bank_mut().select_previous_page();
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        ctx.model.marker_bank_mut().select_next_page();
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        ctx.model.marker_bank_mut().select_previous_page();
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        ctx.model.marker_bank().can_scroll_backwards()
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        ctx.model.marker_bank().can_scroll_forwards()
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        ctx.model.marker_bank().can_scroll_page_backwards()
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        ctx.model.marker_bank().can_scroll_page_forwards()
    }

    impl_as_any!();
}
