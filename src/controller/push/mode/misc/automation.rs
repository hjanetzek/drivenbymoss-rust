// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The automation mode selection (former `AutomationSelectionMode`).

use super::{forward_base, row_index};
use crate::controller::push::display1::SELECT_ARROW;
use crate::controller::push::mode::PushDisplayMode;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Selects the automation write mode with the first button row.
pub struct AutomationSelectionMode {
    base: ParameterModeBase,
}

impl AutomationSelectionMode {
    pub fn new() -> Self {
        AutomationSelectionMode { base: super::base("Automation") }
    }
}

impl Default for AutomationSelectionMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for AutomationSelectionMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let transport = ctx.model.transport();
        let write_mode = transport.automation_write_mode();
        display.set_block(1, 0, "Automation Mode:");
        for (i, mode) in transport.automation_write_modes().iter().enumerate() {
            let arrow = if *mode == write_mode { SELECT_ARROW.to_string() } else { String::new() };
            display.set_cell(3, i, &format!("{arrow}{}", mode.label()));
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let transport = ctx.model.transport();
        let write_mode = transport.automation_write_mode();
        let modes = transport.automation_write_modes();
        for i in 0..8 {
            let label = modes.get(i).map_or("", |m| m.label());
            let selected = modes.get(i).is_some_and(|m| *m == write_mode);
            display.add_option_element("", "", false, if i == 0 { "Automation Mode" } else { "" }, label, selected, false);
        }
    }
}

impl Mode for AutomationSelectionMode {
    forward_base!();

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 {
            return;
        }
        if event != ButtonEvent::Up {
            return;
        }
        let modes = ctx.model.transport().automation_write_modes();
        if let Some(mode) = modes.get(index).copied() {
            ctx.model.transport_mut().set_automation_write_mode(mode);
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            let transport = ctx.model.transport();
            if let Some(mode) = transport.automation_write_modes().get(index) {
                return if *mode == transport.automation_write_mode() { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
            }
        }
        BUTTON_COLOR_OFF
    }

    impl_as_any!();
}
