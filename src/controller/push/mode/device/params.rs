// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device parameters mode and the device chains mode (former `DeviceParamsMode` and its
//! sub-class `DeviceChainsMode`).

use crate::config::push as config;
use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::daw::constants::Capability;
use crate::daw::events::ParameterBankId;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_OFF};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::parameter_mode::{button_row_index, default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, BankParameterProvider};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::display1;
use super::super::super::surface as push;
use super::super::track::{color, host_supports, limit, set_mode_later, trigger_button_later};
use super::super::{check_stop_automation_on_knob_release, update_display, PushDisplayMode};

const MENU: [&str; 8] = ["On", "Parameters", "Expanded", "Chains", "Banks", "Pin Device", "Window", "Up"];

/// Set if the devices (instead of the parameter pages) are shown by the DEVICE_PARAMS mode
/// (former `((DeviceParamsMode) modeManager.get (Modes.DEVICE_PARAMS)).setShowDevices`). Does
/// nothing if the mode is currently in use (call it from outside of the mode).
pub fn set_show_devices(ctx: &Ctx, enable: bool) {
    if let Some(mode) = ctx.modes.get(Modes::DeviceParams) {
        if let Ok(mut mode) = mode.try_borrow_mut() {
            if let Some(mode) = mode.as_any_mut().downcast_mut::<DeviceParamsMode>() {
                mode.set_show_devices(enable);
            }
        }
    }
}

/// True if the DEVICE_PARAMS mode shows the devices (former `isShowDevices`).
pub fn is_show_devices(ctx: &Ctx) -> bool {
    ctx.modes
        .get(Modes::DeviceParams)
        .and_then(|mode| mode.try_borrow().ok().and_then(|m| m.as_any().downcast_ref::<DeviceParamsMode>().map(|m| m.is_show_devices())))
        .unwrap_or(true)
}

/// The device parameters mode (former `DeviceParamsMode`); with `is_chains` the device chains
/// mode (former `DeviceChainsMode`).
pub struct DeviceParamsMode {
    pub base: ParameterModeBase,
    host_menu: [&'static str; 8],
    show_devices: bool,
    is_chains: bool,
}

impl DeviceParamsMode {
    pub fn new(ctx: &Ctx, is_chains: bool) -> Self {
        let mut base = ParameterModeBase::new("Parameters", true, ModeBank::ParameterBank(ParameterBankId::CursorDevice), default_knob_ids());
        base.set_parameter_provider(ctx, provider(BankParameterProvider::new(ctx, ParameterBankId::CursorDevice)));
        let mut host_menu = MENU;
        if !host_supports(ctx, Capability::HasParameterPageSection) {
            host_menu[1] = "";
        }
        if !host_supports(ctx, Capability::HasPinning) {
            host_menu[5] = "";
        }
        if !host_supports(ctx, Capability::HasSlotChains) {
            host_menu[3] = "";
        }
        DeviceParamsMode { base, host_menu, show_devices: true, is_chains }
    }

    /// Former `setShowDevices`.
    pub fn set_show_devices(&mut self, enable: bool) {
        self.show_devices = enable;
    }

    /// Former `isShowDevices`.
    pub fn is_show_devices(&self) -> bool {
        self.show_devices
    }

    /// Set the show devices state of the DEVICE_PARAMS mode, which might be this one.
    fn params_set_show_devices(&mut self, ctx: &Ctx, enable: bool) {
        if self.is_chains {
            set_show_devices(ctx, enable);
        } else {
            self.show_devices = enable;
        }
    }

    fn params_is_show_devices(&self, ctx: &Ctx) -> bool {
        if self.is_chains {
            is_show_devices(ctx)
        } else {
            self.show_devices
        }
    }

    fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            return;
        }

        if event == ButtonEvent::Up {
            if !ctx.model.cursor_device().does_exist() {
                return;
            }

            if self.is_chains {
                // Divergence: the Rust model has no slot chains (always empty)
                let slot_chains = ctx.model.cursor_device().slot_chains();
                if index >= slot_chains.len() {
                    return;
                }
                ctx.model.cursor_device_mut().select_slot_chain(&slot_chains[index]);
                set_mode_later(ctx, Modes::DeviceParams);
                return;
            }

            // Select parameter bank if parameter banks are visible
            if !self.show_devices {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    bank.select_page(index);
                }
                return;
            }

            // Duplicate device
            if ctx.is_button_combination(ButtonId::Duplicate) {
                ctx.model.cursor_device_mut().duplicate();
                return;
            }

            // Delete device
            if ctx.is_button_combination(ButtonId::Delete) {
                ctx.model.cursor_device_mut().device_bank_mut().item_mut(index).remove();
                return;
            }

            // Disable/Enable device
            if ctx.is_button_combination(ButtonId::Mute) {
                ctx.model.cursor_device_mut().device_bank_mut().item_mut(index).toggle_enabled_state();
                return;
            }

            // Select device
            if ctx.model.cursor_device().index() != index as i32 {
                ctx.model.cursor_device_mut().device_bank_mut().item_mut(index).select();
                return;
            }

            // No layers, show devices
            if !ctx.model.cursor_device().has_layers() {
                self.params_set_show_devices(ctx, false);
                return;
            }

            // If there are layers, make sure one is selected
            let layer_bank = ctx.model.cursor_device_mut().layer_bank_mut();
            if layer_bank.selected_item().is_none() {
                layer_bank.item_mut(0).select();
            }
            let mode = push::current_layer_mix_mode(ctx);
            set_mode_later(ctx, mode);
            return;
        }

        // LONG press - move upwards
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Row11, index));
        self.move_up(ctx);
    }

    /// Former `moveUp`.
    fn move_up(&mut self, ctx: &mut Ctx) {
        if ctx.modes.is_active(Modes::DeviceChains) {
            set_mode_later(ctx, Modes::DeviceParams);
            return;
        }

        // There is no device on the track move upwards to the track view
        if !ctx.model.cursor_device().does_exist() {
            trigger_button_later(ctx, ButtonId::Track);
            return;
        }

        // Parameter banks are shown -> show devices
        if !self.params_is_show_devices(ctx) {
            self.params_set_show_devices(ctx, true);
            return;
        }

        // Devices are shown, if nested show the layers otherwise move up to the tracks
        if ctx.model.cursor_device().is_nested() {
            ctx.model.cursor_device_mut().select_parent();
            ctx.schedule(300, |ctx| {
                let mode = if ctx.model.cursor_device().has_layers() { push::current_layer_mix_mode(ctx) } else { Modes::DeviceParams };
                ctx.set_active_mode(mode);
                set_show_devices(ctx, false);
                ctx.model.cursor_device_mut().select_channel();
            });
            return;
        }

        // Move up to the track
        if ctx.model.is_cursor_device_on_master_track() {
            trigger_button_later(ctx, ButtonId::Mastertrack);
        } else {
            trigger_button_later(ctx, ButtonId::Track);
        }
    }

    fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let exists = ctx.model.cursor_device().does_exist();
        match index {
            0 => {
                if exists {
                    ctx.model.cursor_device_mut().toggle_enabled_state();
                }
            }
            1 => {
                if exists {
                    ctx.model.cursor_device_mut().toggle_parameter_page_section_visible();
                }
            }
            2 => {
                if exists {
                    ctx.model.cursor_device_mut().toggle_expanded();
                }
            }
            3 => {
                if !host_supports(ctx, Capability::HasSlotChains) {
                    return;
                }
                set_mode_later(ctx, if ctx.modes.is_active(Modes::DeviceChains) { Modes::DeviceParams } else { Modes::DeviceChains });
            }
            4 => {
                if !exists {
                    return;
                }
                if !ctx.modes.is_active(Modes::DeviceParams) {
                    set_mode_later(ctx, Modes::DeviceParams);
                }
                // As Java: toggles the state of this mode (also if it is the chains mode)
                self.show_devices = !self.show_devices;
            }
            5 => {
                if exists {
                    ctx.model.cursor_device_mut().toggle_pinned();
                }
            }
            6 => {
                if exists {
                    ctx.model.cursor_device_mut().toggle_window_open();
                }
            }
            7 => self.move_up(ctx),
            _ => {}
        }
    }

    /// Former `checkExists1`.
    fn check_exists1(ctx: &Ctx, display: &mut TextDisplay) -> bool {
        if ctx.model.cursor_device().does_exist() {
            return true;
        }
        display.set_block(1, 0, "           Select").set_block(1, 1, "a device or press").set_block(1, 2, "'Add Effect'...  ");
        false
    }

    /// Former `checkExists2`.
    pub fn check_exists2(ctx: &Ctx, display: &mut GraphicDisplay) -> bool {
        if ctx.model.cursor_device().does_exist() {
            return true;
        }
        for i in 0..8 {
            display.add_option_element(if i == 2 { "Please select a device or press 'Add Device'..." } else { "" }, if i == 7 { "Up" } else { "" }, true, "", "", false, true);
        }
        false
    }

    /// Former `getTopMenuEnablement`.
    fn top_menu_enablement(&self, ctx: &Ctx, has_pinning: bool, index: usize) -> bool {
        let cd = ctx.model.cursor_device();
        match index {
            0 => cd.is_enabled(),
            1 => cd.is_parameter_page_section_visible(),
            2 => cd.is_expanded(),
            3 => ctx.modes.is_active(Modes::DeviceChains),
            4 => !ctx.modes.is_active(Modes::DeviceChains) && !self.show_devices,
            5 => has_pinning && cd.is_pinned(),
            6 => cd.is_window_open(),
            7 => true,
            _ => false,
        }
    }

    fn update_chains_display1(&self, ctx: &Ctx, display: &mut TextDisplay) {
        let cd = ctx.model.cursor_device();
        display.set_block(2, 0, "Device Chains:").set_block(2, 1, cd.name());
        let slot_chains = cd.slot_chains();
        for i in 0..8 {
            display.set_cell(3, i, slot_chains.get(i).map_or("", |s| s.as_str()));
        }
    }

    fn update_chains_display2(&self, ctx: &Ctx, display: &mut GraphicDisplay) {
        let cd = ctx.model.cursor_device();
        let bottom_menu_color = DawColor::color_entry(ctx.model.current_track_bank().selected_channel_color_entry());
        let has_pinning = host_supports(ctx, Capability::HasPinning);
        let slot_chains = cd.slot_chains();
        for i in 0..8 {
            let is_top_menu_on = self.top_menu_enablement(ctx, has_pinning, i);
            let bottom_menu = slot_chains.get(i).map_or("", |s| s.as_str());
            display.add_parameter_element_with_device_name(self.host_menu[i], is_top_menu_on, bottom_menu, "", bottom_menu_color, i < slot_chains.len(), "", 0, "", false, -1);
        }
    }
}

impl PushDisplayMode for DeviceParamsMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let ctx = &*ctx;
        if !Self::check_exists1(ctx, display) {
            return;
        }
        if self.is_chains {
            self.update_chains_display1(ctx, display);
            return;
        }

        let cd = ctx.model.cursor_device();

        // Row 1 & 2
        if let Some(parameter_bank) = cd.parameter_bank() {
            for i in 0..8 {
                let param = parameter_bank.item(i);
                display.set_cell(0, i, &if param.does_exist() { crate::config::values::fix_ascii(param.name()) } else { String::new() });
                display.set_cell(1, i, &crate::config::values::fix_ascii(&param.displayed_value_limited(8)));
            }
        }

        // Row 3
        display.set_block(2, 0, "Selected Device:").set_block(2, 1, cd.name());

        // Row 4
        if self.show_devices {
            let device_bank = cd.device_bank();
            for i in 0..8 {
                let device = device_bank.item(i);
                let mut sb = String::new();
                if device.does_exist() {
                    if i as i32 == cd.index() {
                        sb.push(display1::SELECT_ARROW);
                    }
                    if !device.is_enabled() {
                        sb.push(display1::DIVISION);
                    }
                    sb.push_str(device.name());
                }
                display.set_cell(3, i, &sb);
            }
            return;
        }

        if let Some(parameter_bank) = cd.parameter_bank() {
            let bank = parameter_bank.page_bank();
            let selected_item_index = bank.selected_item_index();
            for i in 0..bank.page_size() {
                let item = bank.item(i);
                let text = if item.is_empty() {
                    String::new()
                } else if i == selected_item_index {
                    format!("{}{item}", display1::SELECT_ARROW)
                } else {
                    item
                };
                display.set_cell(3, i, &text);
            }
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let ctx = &*ctx;
        if !Self::check_exists2(ctx, display) {
            return;
        }
        if self.is_chains {
            self.update_chains_display2(ctx, display);
            return;
        }

        let cd = ctx.model.cursor_device();
        let bottom_menu_color = DawColor::color_entry(ctx.model.current_track_bank().selected_channel_color_entry());
        let color_background = config::color(&ctx.config, config::COLOR_BACKGROUND);
        let device_bank = cd.device_bank();
        let Some(parameter_bank) = cd.parameter_bank() else {
            return;
        };
        let page_bank = parameter_bank.page_bank();
        let selected_page = page_bank.selected_item_index();
        let has_pinning = host_supports(ctx, Capability::HasPinning);
        let vc = ctx.model.value_changer();

        for i in 0..parameter_bank.page_size().min(8) {
            let is_top_menu_on = self.top_menu_enablement(ctx, has_pinning, i);

            let mut color = bottom_menu_color;
            let (bottom_menu, bottom_menu_icon, is_bottom_menu_on) = if self.show_devices {
                let device = device_bank.item(i);
                if !device.is_enabled() {
                    color = color_background;
                }
                (if device.does_exist() { device.name_limited(12) } else { String::new() }, device.name().to_string(), i as i32 == cd.index())
            } else {
                (limit(&page_bank.item(i), 12), cd.name().to_string(), i == selected_page)
            };

            let param = parameter_bank.item(i);
            let exists = param.does_exist();
            let parameter_name = if exists { param.name_limited(12) } else { String::new() };
            let parameter_value = vc.to_display_value(if exists { param.value() } else { 0 });
            let parameter_value_str = if exists { param.displayed_value_limited(8) } else { String::new() };
            let parameter_is_active = self.base.is_knob_touched(i);
            let parameter_modulated_value = vc.to_display_value(if exists { param.modulated_value() } else { -1 });

            display.add_parameter_element_with_device_name(
                self.host_menu[i],
                is_top_menu_on,
                &bottom_menu,
                &bottom_menu_icon,
                color,
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

impl Mode for DeviceParamsMode {
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
        let cd = ctx.model.cursor_device();

        if let Some(index) = button_row_index(0, button) {
            if !cd.does_exist() {
                return color(ctx, BUTTON_COLOR_OFF);
            }
            let selected_color = color(ctx, PUSH_ORANGE_HI);
            let exists_color = color(ctx, PUSH_YELLOW_LO);
            let off_color = color(ctx, PUSH_BLACK);
            if self.show_devices {
                if !cd.device_bank().item(index).does_exist() {
                    return off_color;
                }
                return if index as i32 == cd.index() { selected_color } else { exists_color };
            }
            let Some(parameter_bank) = cd.parameter_bank() else {
                return off_color;
            };
            let bank = parameter_bank.page_bank();
            if bank.item(index).is_empty() {
                return off_color;
            }
            return if index == bank.selected_item_index() { selected_color } else { exists_color };
        }

        if let Some(index) = button_row_index(1, button) {
            if self.is_chains {
                // Former DeviceChainsMode.getButtonColor
                let exists_color = if is_modern { PUSH2_COLOR_YELLOW_LO } else { PUSH1_COLOR_YELLOW_LO };
                let off_color = if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };
                return if index < cd.slot_chains().len() { exists_color } else { off_color };
            }

            let white = if is_modern { PUSH2_COLOR2_WHITE } else { PUSH1_COLOR2_WHITE };
            if !cd.does_exist() {
                return if index == 7 { white } else { color(ctx, BUTTON_COLOR_OFF) };
            }
            let green = if is_modern { PUSH2_COLOR2_GREEN } else { PUSH1_COLOR2_GREEN };
            let grey = if is_modern { PUSH2_COLOR2_GREY_LO } else { PUSH1_COLOR2_GREY_LO };
            let orange = if is_modern { PUSH2_COLOR2_ORANGE } else { PUSH1_COLOR2_ORANGE };
            let off = if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };
            let turquoise = if is_modern { PUSH2_COLOR2_TURQUOISE_HI } else { PUSH1_COLOR2_TURQUOISE_HI };
            return match index {
                0 => {
                    if cd.is_enabled() {
                        green
                    } else {
                        grey
                    }
                }
                1 => {
                    if cd.is_parameter_page_section_visible() {
                        orange
                    } else {
                        white
                    }
                }
                2 => {
                    if cd.is_expanded() {
                        orange
                    } else {
                        white
                    }
                }
                3 => {
                    if ctx.modes.is_active(Modes::DeviceChains) {
                        orange
                    } else {
                        white
                    }
                }
                4 => {
                    if self.show_devices {
                        white
                    } else {
                        orange
                    }
                }
                5 => {
                    if !host_supports(ctx, Capability::HasPinning) {
                        off
                    } else if cd.is_pinned() {
                        turquoise
                    } else {
                        grey
                    }
                }
                6 => {
                    if cd.is_window_open() {
                        turquoise
                    } else {
                        grey
                    }
                }
                _ => white,
            };
        }

        color(ctx, BUTTON_COLOR_OFF)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if self.is_chains {
            // Former DeviceChainsMode.onKnobTouch: intentionally empty
            return;
        }
        self.base.set_touched_knob(index, is_touched);
        if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            let delete = is_touched && ctx.surface.is_delete_pressed();
            let param = bank.item_mut(index);
            if delete {
                param.reset_value();
            }
            param.touch_value(is_touched);
            if delete {
                ctx.surface.set_trigger_consumed(ButtonId::Delete);
            }
        }
        check_stop_automation_on_knob_release(ctx, is_touched);
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.base.select_item(ctx, index);
    }

    // Divergence: the device bank of the model has no item selection, the previous/next device
    // is selected with the cursor device (former `getDeviceBank ().selectPreviousItem ()`)
    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if self.show_devices {
            ctx.model.cursor_device_mut().select_previous();
            return;
        }
        self.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        if self.show_devices {
            ctx.model.cursor_device_mut().select_next();
            return;
        }
        self.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        if self.show_devices {
            if ctx.surface.is_shift_pressed() {
                ctx.model.cursor_device_mut().swap_with_previous();
            } else {
                ctx.model.cursor_device_mut().device_bank_mut().select_previous_page();
            }
            return;
        }
        self.base.select_previous_item_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        if self.show_devices {
            if ctx.surface.is_shift_pressed() {
                ctx.model.cursor_device_mut().swap_with_next();
            } else {
                ctx.model.cursor_device_mut().device_bank_mut().select_next_page();
            }
            return;
        }
        self.base.select_next_item_page(ctx);
    }

    fn select_item_page(&mut self, ctx: &mut Ctx, page: usize) {
        self.base.select_item_page(ctx, page);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        if self.show_devices {
            return ctx.model.cursor_device().device_bank().can_scroll_backwards();
        }
        self.base.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        if self.show_devices {
            return ctx.model.cursor_device().device_bank().can_scroll_forwards();
        }
        self.base.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        if self.show_devices {
            if ctx.surface.is_shift_pressed() {
                return ctx.model.cursor_device().index() > 0;
            }
            return ctx.model.cursor_device().device_bank().can_scroll_page_backwards();
        }
        self.base.has_previous_item_page(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        if self.show_devices {
            if ctx.surface.is_shift_pressed() {
                return ctx.model.cursor_device().index() < 7;
            }
            return ctx.model.cursor_device().device_bank().can_scroll_page_forwards();
        }
        self.base.has_next_item_page(ctx)
    }

    impl_as_any!();
}
