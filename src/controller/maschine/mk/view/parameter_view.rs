// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The parameter view (former `ParameterView`): select the device, the parameter page and the
//! parameter which is edited with the encoder.

use super::{base_on_grid_note, ExecuteFunction};
use crate::controller::maschine::core::colors::{COLOR_BLACK, COLOR_ROSE, COLOR_SKIN, COLOR_WHITE, PARAM_COLORS};
use crate::controller::maschine::mk::mode::parameters_mode::MaschineParametersMode;
use crate::framework::commands::mv_helper::notify_selected_parameter_page;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::ids::Modes;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Select the device, the parameter page and the parameter with the pads.
pub struct ParameterView;

impl ParameterView {
    pub fn new(_ctx: &Ctx) -> Self {
        ParameterView
    }

    /// Get the selected parameter of the parameters mode.
    fn selected_parameter(ctx: &Ctx) -> usize {
        ctx.modes
            .get(Modes::DeviceParams)
            .and_then(|mode| mode.try_borrow().ok().and_then(|m| m.as_any().downcast_ref::<MaschineParametersMode>().map(|m| m.selected_parameter())))
            .unwrap_or(0)
    }
}

impl ExecuteFunction for ParameterView {
    fn execute_function(&mut self, ctx: &mut Ctx, pad_index: i32, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }

        if !ctx.model.cursor_device().does_exist() {
            ctx.host.show_notification("No device selected.");
            return;
        }

        match pad_index {
            12 => ctx.model.cursor_device_mut().select_previous(),
            13 => ctx.model.cursor_device_mut().select_next(),
            14 => {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    bank.scroll_backwards();
                }
                notify_selected_parameter_page(ctx);
            }
            15 => {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    bank.scroll_forwards();
                }
                notify_selected_parameter_page(ctx);
            }
            _ => {}
        }

        if pad_index >= 8 {
            return;
        }

        let selected_parameter = if pad_index < 4 { pad_index + 4 } else { pad_index - 4 };
        if let Some(mode) = ctx.modes.get(Modes::DeviceParams) {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                if let Some(mode) = mode.as_any_mut().downcast_mut::<MaschineParametersMode>() {
                    mode.select_parameter(selected_parameter);
                }
            }
        }

        ctx.schedule(200, move |ctx| {
            let mut message = String::new();
            match ctx.model.cursor_device().parameter_bank() {
                Some(bank) if bank.page_bank().item_count() > 0 => {
                    message.push_str(&bank.page_bank().selected_item());
                    message.push_str(": ");
                    let item = bank.item(selected_parameter as usize);
                    message.push_str(if item.does_exist() { item.name() } else { "None" });
                }
                _ => message.push_str("No parameters available."),
            }
            ctx.host.show_notification(&message);
        });
    }
}

impl View for ParameterView {
    fn name(&self) -> &str {
        "Parameter"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let selected_parameter = Self::selected_parameter(ctx);
        let Ctx { surface, colors, model, .. } = ctx;
        let grid = surface.grid();

        for i in 8..12 {
            grid.light_ex_id(colors, i % 4, 3 - i / 4, BUTTON_COLOR_OFF);
        }

        let cursor_device = model.cursor_device();
        let does_exist = cursor_device.does_exist();
        let parameter_bank = cursor_device.parameter_bank();
        grid.light_ex(0, 0, if does_exist && cursor_device.can_select_previous() { COLOR_ROSE } else { COLOR_BLACK });
        grid.light_ex(1, 0, if does_exist && cursor_device.can_select_next() { COLOR_ROSE } else { COLOR_BLACK });
        let can_backwards = parameter_bank.is_some_and(|bank| bank.can_scroll_backwards());
        let can_forwards = parameter_bank.is_some_and(|bank| bank.can_scroll_forwards());
        grid.light_ex(2, 0, if does_exist && can_backwards { COLOR_SKIN } else { COLOR_BLACK });
        grid.light_ex(3, 0, if does_exist && can_forwards { COLOR_SKIN } else { COLOR_BLACK });

        for (i, color) in PARAM_COLORS.iter().copied().enumerate() {
            let x = (i % 4) as i32;
            let y = 2 + (i / 4) as i32;
            let exists = parameter_bank.is_some_and(|bank| bank.item(i).does_exist());
            if exists {
                if i == selected_parameter {
                    grid.light_ex_blink(x, y, color, COLOR_WHITE, false);
                } else {
                    grid.light_ex(x, y, color);
                }
            } else {
                grid.light_ex_id(colors, x, y, BUTTON_COLOR_OFF);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        base_on_grid_note(self, ctx, note, velocity);
    }

    impl_as_any!();
}
