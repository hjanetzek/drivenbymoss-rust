// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Device navigation commands (former `DeviceLayerLeftCommand`, `DeviceLayerRightCommand`,
//! `SelectPreviousDeviceOrParamPageCommand` and `SelectNextDeviceOrParamPageCommand` of
//! `framework.command.trigger.device`).
//!
//! Reaper has no device layers (the layer bank is always empty), so the layer functions only
//! apply with a host which reports layers.

use crate::daw::bank::BankItem;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// The index of the selected layer of the cursor device, if the device has layers.
fn selected_layer(ctx: &Ctx) -> Option<usize> {
    let cd = ctx.model.cursor_device();
    if !cd.has_layers() {
        return None;
    }
    cd.layer_bank().selected_index()
}

/// Select the previous device or layer; with Shift exit the layer (former
/// `DeviceLayerLeftCommand`).
#[derive(Default)]
pub struct DeviceLayerLeftCommand;

impl DeviceLayerLeftCommand {
    pub fn new() -> Self {
        DeviceLayerLeftCommand
    }

    /// Former `canExecute`.
    pub fn can_execute(ctx: &Ctx) -> bool {
        if ctx.surface.is_shift_pressed() {
            return true;
        }
        let cd = ctx.model.cursor_device();
        match selected_layer(ctx) {
            Some(_) => cd.layer_bank().can_scroll_backwards(),
            None => cd.can_select_previous(),
        }
    }
}

impl TriggerCommand for DeviceLayerLeftCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if selected_layer(ctx).is_none() {
            ctx.model.cursor_device_mut().select_previous();
        } else {
            ctx.model.cursor_device_mut().layer_bank_mut().select_previous_item();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        // Exit layer
        match selected_layer(ctx) {
            None => {
                if ctx.model.cursor_device().is_nested() {
                    let cd = ctx.model.cursor_device_mut();
                    cd.select_parent();
                    cd.select_channel();
                }
            }
            Some(index) => ctx.model.cursor_device_mut().layer_bank_mut().item_mut(index).set_selected(false),
        }
    }
}

/// Select the next device or layer; with Shift enter the layer (former
/// `DeviceLayerRightCommand`).
#[derive(Default)]
pub struct DeviceLayerRightCommand;

impl DeviceLayerRightCommand {
    pub fn new() -> Self {
        DeviceLayerRightCommand
    }

    /// Former `canExecute`.
    pub fn can_execute(ctx: &Ctx) -> bool {
        if ctx.surface.is_shift_pressed() {
            return true;
        }
        let cd = ctx.model.cursor_device();
        match selected_layer(ctx) {
            Some(_) => cd.layer_bank().can_scroll_forwards(),
            None => cd.can_select_next(),
        }
    }
}

impl TriggerCommand for DeviceLayerRightCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if selected_layer(ctx).is_none() {
            ctx.model.cursor_device_mut().select_next();
        } else {
            ctx.model.cursor_device_mut().layer_bank_mut().select_next_item();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        // Enter layer
        if !ctx.model.cursor_device().has_layers() {
            return;
        }
        if selected_layer(ctx).is_none() {
            ctx.model.cursor_device_mut().layer_bank_mut().item_mut(0).select();
        }
        // Former layer.enter (): the model has no nested devices of layers (Reaper has none)
    }
}

/// Select the previous parameter page; with Shift the previous device (former
/// `SelectPreviousDeviceOrParamPageCommand`).
#[derive(Default)]
pub struct SelectPreviousDeviceOrParamPageCommand;

impl SelectPreviousDeviceOrParamPageCommand {
    pub fn new() -> Self {
        SelectPreviousDeviceOrParamPageCommand
    }

    /// Former `canExecute`.
    pub fn can_execute(ctx: &Ctx) -> bool {
        let cd = ctx.model.cursor_device();
        if ctx.surface.is_shift_pressed() {
            return cd.can_select_previous();
        }
        cd.parameter_bank().is_some_and(|bank| bank.can_scroll_backwards())
    }
}

impl TriggerCommand for SelectPreviousDeviceOrParamPageCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                bank.scroll_backwards();
            }
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.cursor_device_mut().select_previous();
        }
    }
}

/// Select the next parameter page; with Shift the next device (former
/// `SelectNextDeviceOrParamPageCommand`).
#[derive(Default)]
pub struct SelectNextDeviceOrParamPageCommand;

impl SelectNextDeviceOrParamPageCommand {
    pub fn new() -> Self {
        SelectNextDeviceOrParamPageCommand
    }

    /// Former `canExecute`.
    pub fn can_execute(ctx: &Ctx) -> bool {
        let cd = ctx.model.cursor_device();
        if ctx.surface.is_shift_pressed() {
            return cd.can_select_next();
        }
        cd.parameter_bank().is_some_and(|bank| bank.can_scroll_forwards())
    }
}

impl TriggerCommand for SelectNextDeviceOrParamPageCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                bank.scroll_forwards();
            }
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.cursor_device_mut().select_next();
        }
    }
}
