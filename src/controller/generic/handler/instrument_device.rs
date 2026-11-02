// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for the commands of the first instrument device (former
//! `InstrumentDeviceHandler`).

use super::device::{
    device, device_mut, handle_parameter, parameter_page_value, parameter_toggle_value, parameter_value, reset_device_parameter, scroll_parameter_bank, scroll_parameter_page,
    select_parameter_page, toggle_device_parameter, with_parameter_bank, DeviceRef,
};
use super::{is_button_pressed, to_midi_value, unsupported};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::constants::DeviceId;
use crate::framework::ctx::Ctx;

const DEVICE: DeviceRef = DeviceRef::Specific(DeviceId::FirstInstrument);

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    if let Some(index) = command.index_in(FlexiCommand::InstrumentDeviceSetParameter1, 8) {
        return parameter_value(ctx, DEVICE, index);
    }
    if let Some(index) = command.index_in(FlexiCommand::InstrumentDeviceToggleParameter1, 8) {
        return parameter_toggle_value(ctx, DEVICE, index);
    }
    if let Some(index) = command.index_in(FlexiCommand::InstrumentDeviceSelectParameterPage1, 8) {
        return parameter_page_value(ctx, DEVICE, index);
    }
    let Some(device) = device(ctx, DEVICE) else {
        return -1;
    };
    match command {
        FlexiCommand::InstrumentDeviceToggleWindow => to_midi_value(device.is_window_open()),
        FlexiCommand::InstrumentDeviceToggleBypass => to_midi_value(!device.is_enabled()),
        FlexiCommand::InstrumentDeviceToggleExpand => to_midi_value(device.is_expanded()),
        FlexiCommand::InstrumentDeviceToggleParameters => to_midi_value(device.is_parameter_page_section_visible()),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);

    if let Some(index) = command.index_in(FlexiCommand::InstrumentDeviceSetParameter1, 8) {
        handle_parameter(ctx, DEVICE, knob_mode, index, value);
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::InstrumentDeviceToggleParameter1, 8) {
        if is_button_pressed {
            toggle_device_parameter(ctx, DEVICE, index);
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::InstrumentDeviceResetParameter1, 8) {
        if is_button_pressed {
            reset_device_parameter(ctx, DEVICE, index);
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::InstrumentDeviceSelectParameterPage1, 8) {
        if is_button_pressed {
            select_parameter_page(ctx, DEVICE, index);
        }
        return;
    }

    match command {
        FlexiCommand::InstrumentDeviceScrollParameterPages => scroll_parameter_page(ctx, DEVICE, CommandCategory::InstrumentDevice, knob_mode, value),
        FlexiCommand::InstrumentDeviceScrollParameterBanks => scroll_parameter_bank(ctx, DEVICE, CommandCategory::InstrumentDevice, knob_mode, value),
        FlexiCommand::InstrumentDeviceSelectPreviousParameterPage => {
            if is_button_pressed {
                with_parameter_bank(ctx, DEVICE, |bank| bank.scroll_backwards());
            }
        }
        FlexiCommand::InstrumentDeviceSelectNextParameterPage => {
            if is_button_pressed {
                with_parameter_bank(ctx, DEVICE, |bank| bank.scroll_forwards());
            }
        }
        FlexiCommand::InstrumentDeviceSelectPreviousParameterBank => {
            if is_button_pressed {
                with_parameter_bank(ctx, DEVICE, |bank| bank.select_previous_page());
            }
        }
        FlexiCommand::InstrumentDeviceSelectNextParameterBank => {
            if is_button_pressed {
                with_parameter_bank(ctx, DEVICE, |bank| bank.select_next_page());
            }
        }
        FlexiCommand::InstrumentDeviceToggleWindow
        | FlexiCommand::InstrumentDeviceToggleBypass
        | FlexiCommand::InstrumentDeviceToggleExpand
        | FlexiCommand::InstrumentDeviceToggleParameters => {
            if !is_button_pressed {
                return;
            }
            let Some(device) = device_mut(ctx, DEVICE) else {
                return;
            };
            match command {
                FlexiCommand::InstrumentDeviceToggleWindow => device.toggle_window_open(),
                FlexiCommand::InstrumentDeviceToggleBypass => device.toggle_enabled_state(),
                FlexiCommand::InstrumentDeviceToggleExpand => device.toggle_expanded(),
                _ => device.toggle_parameter_page_section_visible(),
            }
        }
        _ => unsupported(ctx, command),
    }
}
