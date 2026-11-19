// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The layer details mode (former `DeviceLayerDetailsMode`).

use std::cell::Cell;

use crate::daw::channel::Channel;
use crate::daw::device::SpecificDevice;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR2_ON, BUTTON_COLOR_OFF};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::mode::parameter_mode::button_row_index;
use crate::framework::view::color_select_mode::ColorSelectMode;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::surface as push;
use super::super::track::{color, select_color_view};
use super::super::{update_display, PushDisplayMode};

/// The bank of the mode, depending on the active view (former view change listener which
/// calls `switchBanks`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailsBank {
    /// The layers of the cursor device.
    Layers,
    /// The drum pads of the cursor device (the drum view).
    CursorDrumPads,
    /// The drum pads of the drum device with 64 pads (the drum 64 view).
    Drum64Pads,
}

/// The layer details mode (former `DeviceLayerDetailsMode`).
pub struct DeviceLayerDetailsMode {
    bank: Cell<DetailsBank>,
}

impl Default for DeviceLayerDetailsMode {
    fn default() -> Self {
        Self::new()
    }
}

fn drum64(ctx: &Ctx) -> Option<&SpecificDevice> {
    ctx.model.drum_device_sized(64)
}

impl DeviceLayerDetailsMode {
    pub fn new() -> Self {
        DeviceLayerDetailsMode { bank: Cell::new(DetailsBank::Layers) }
    }

    fn bank(&self, ctx: &Ctx) -> DetailsBank {
        let bank = match ctx.views.active_id() {
            Some(Views::Drum) => DetailsBank::CursorDrumPads,
            Some(Views::Drum64) => DetailsBank::Drum64Pads,
            // Do not switch banks if view is left for color selection
            Some(Views::Color) => return self.bank.get(),
            _ => DetailsBank::Layers,
        };
        self.bank.set(bank);
        bank
    }

    /// Former `bank instanceof IDrumPadBank`.
    fn is_drum_pad_bank(&self, ctx: &Ctx) -> bool {
        self.bank(ctx) != DetailsBank::Layers
    }

    /// The selected layer or drum pad (former `bank.getSelectedItem ()`).
    fn selected<'a>(&self, ctx: &'a Ctx) -> Option<&'a Channel> {
        match self.bank(ctx) {
            DetailsBank::Layers => ctx.model.cursor_device().layer_bank().selected_item(),
            DetailsBank::CursorDrumPads => ctx.model.cursor_device().drum_pad_bank().selected_item().map(|p| &**p),
            DetailsBank::Drum64Pads => drum64(ctx)?.drum_pad_bank().selected_item().map(|p| &**p),
        }
    }

    fn with_selected_mut(&self, ctx: &mut Ctx, f: impl FnOnce(&mut Channel)) {
        match self.bank(ctx) {
            DetailsBank::Layers => {
                if let Some(layer) = ctx.model.cursor_device_mut().layer_bank_mut().selected_item_mut() {
                    f(layer);
                }
            }
            DetailsBank::CursorDrumPads => {
                let bank = ctx.model.cursor_device_mut().drum_pad_bank_mut();
                if let Some(index) = bank.selected_index() {
                    f(bank.item_mut(index));
                }
            }
            DetailsBank::Drum64Pads => {
                if let Some(device) = ctx.model.drum_device_sized_mut(64) {
                    let bank = device.drum_pad_bank_mut();
                    if let Some(index) = bank.selected_index() {
                        f(bank.item_mut(index));
                    }
                }
            }
        }
    }

    fn clear(&self, ctx: &mut Ctx, mute: bool) {
        let bank = match self.bank(ctx) {
            DetailsBank::Layers => return,
            DetailsBank::CursorDrumPads => ctx.model.cursor_device_mut().drum_pad_bank_mut(),
            DetailsBank::Drum64Pads => match ctx.model.drum_device_sized_mut(64) {
                Some(device) => device.drum_pad_bank_mut(),
                None => return,
            },
        };
        if mute {
            bank.clear_mute();
        } else {
            bank.clear_solo();
        }
    }

    fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up || self.selected(ctx).is_none() {
            return;
        }
        match index {
            0 => self.with_selected_mut(ctx, |c| c.toggle_is_activated()),
            2 => self.with_selected_mut(ctx, |c| c.toggle_mute()),
            3 => self.with_selected_mut(ctx, |c| c.toggle_solo()),
            7 => select_color_view(ctx, ColorSelectMode::Layer),
            _ => {}
        }
    }

    fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        match index {
            6 => self.clear(ctx, true),
            7 => self.clear(ctx, false),
            _ => {}
        }
    }
}

impl PushDisplayMode for DeviceLayerDetailsMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let Some(channel) = self.selected(ctx) else {
            display.set_row(1, "                     Please selecta layer...                        ");
            return;
        };

        let layer_name = channel.name();
        let first_block = format!("Layer: {layer_name}");
        display.set_block(0, 0, &first_block);
        if layer_name.chars().count() > 17 {
            let rest: String = first_block.chars().skip(17).collect();
            display.set_block(0, 1, rest.trim());
        }

        let on_off = |b: bool| if b { "On" } else { "Off" };
        display.set_cell(2, 0, "Active").set_cell(3, 0, on_off(channel.is_activated()));
        display.set_cell(2, 1, "");
        display.set_cell(3, 1, "");
        display.set_cell(2, 2, "Mute").set_cell(3, 2, on_off(channel.is_mute()));
        display.set_cell(2, 3, "Solo").set_cell(3, 3, on_off(channel.is_solo()));
        display.set_cell(2, 4, "");
        display.set_cell(3, 4, "");
        display.set_cell(2, 5, "");
        display.set_cell(3, 5, "");
        if self.is_drum_pad_bank(ctx) {
            display.set_cell(0, 6, "Clr Mute");
            display.set_cell(0, 7, "Clr Solo");
        }
        display.set_cell(2, 7, "Select");
        display.set_cell(3, 7, "Color");
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let Some(channel) = self.selected(ctx) else {
            display.set_message(3, "Please select a layer...");
            return;
        };

        display.add_option_element(&format!("Layer: {}", channel.name()), "", false, "", "Active", channel.is_activated(), false);
        display.add_empty_element();
        display.add_option_element("", "", false, "", "Mute", channel.is_mute(), false);
        display.add_option_element("", "", false, "", "Solo", channel.is_solo(), false);
        display.add_empty_element();
        display.add_empty_element();
        if self.is_drum_pad_bank(ctx) {
            display.add_option_element("", "Clear Mute", false, "", "", false, false);
            display.add_option_element("", "Clear Solo", false, "", "Select Color", false, false);
        } else {
            display.add_empty_element();
            display.add_empty_element();
        }
    }
}

impl Mode for DeviceLayerDetailsMode {
    fn name(&self) -> &str {
        "Layer details"
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(channel) = self.selected(ctx) else {
            return color(ctx, BUTTON_COLOR_OFF);
        };

        if let Some(index) = button_row_index(0, button) {
            let color_id = match index {
                0 => {
                    if channel.is_activated() {
                        PUSH_YELLOW_MD
                    } else {
                        PUSH_YELLOW_LO
                    }
                }
                2 => {
                    if channel.is_mute() {
                        PUSH_ORANGE_HI
                    } else {
                        PUSH_ORANGE_LO
                    }
                }
                3 => {
                    if channel.is_solo() {
                        PUSH_ORANGE_HI
                    } else {
                        PUSH_ORANGE_LO
                    }
                }
                7 => PUSH_GREEN_HI,
                _ => PUSH_BLACK,
            };
            return color(ctx, color_id);
        }

        if let Some(index) = button_row_index(1, button) {
            if index >= 6 {
                return color(ctx, if self.is_drum_pad_bank(ctx) { BUTTON_COLOR2_ON } else { BUTTON_COLOR_OFF });
            }
            return if push::is_modern(ctx) { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };
        }

        color(ctx, BUTTON_COLOR_OFF)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    impl_as_any!();
}
