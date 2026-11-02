// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for cursor device commands (former `DeviceHandler`), and the device functions
//! which are shared with the first instrument handler.

use super::{increase_knob_movement, is_absolute, is_button_pressed, is_increase, set_or_change, to_midi_value, toggle_parameter, unsupported, value_changers};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::constants::DeviceId;
use crate::daw::device::SpecificDevice;
use crate::daw::parameter::Parameter;
use crate::daw::parameter_bank::ParameterBank;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

/// The device of a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRef {
    /// The cursor device.
    Cursor,
    /// A specific device (e.g. the first instrument).
    Specific(DeviceId),
}

pub fn device(ctx: &Ctx, device: DeviceRef) -> Option<&SpecificDevice> {
    match device {
        DeviceRef::Cursor => Some(ctx.model.cursor_device()),
        DeviceRef::Specific(id) => ctx.model.specific_device(id),
    }
}

pub fn device_mut(ctx: &mut Ctx, device: DeviceRef) -> Option<&mut SpecificDevice> {
    match device {
        DeviceRef::Cursor => Some(ctx.model.cursor_device_mut()),
        DeviceRef::Specific(id) => ctx.model.specific_device_mut(id),
    }
}

pub fn parameter_bank(ctx: &Ctx, device: DeviceRef) -> Option<&ParameterBank> {
    self::device(ctx, device).and_then(|d| d.parameter_bank())
}

pub fn parameter_bank_mut(ctx: &mut Ctx, device: DeviceRef) -> Option<&mut ParameterBank> {
    device_mut(ctx, device).and_then(|d| d.parameter_bank_mut())
}

/// The value of the parameter of the device with the index in the page, -1 if there is none.
pub fn parameter_value(ctx: &Ctx, device: DeviceRef, index: usize) -> i32 {
    parameter_bank(ctx, device).map_or(-1, |bank| bank.item(index).value())
}

/// The 'toggle' value of the parameter of the device: on if > 0.
pub fn parameter_toggle_value(ctx: &Ctx, device: DeviceRef, index: usize) -> i32 {
    parameter_bank(ctx, device).map_or(-1, |bank| to_midi_value(bank.item(index).value() > 0))
}

/// Former `getParameterBank ().getItem (index).isSelected ()` of the select parameter page
/// commands (note: as in Java it checks the parameter, not the page).
pub fn parameter_page_value(ctx: &Ctx, device: DeviceRef, index: usize) -> i32 {
    parameter_bank(ctx, device).map_or(-1, |bank| if bank.item(index).is_selected() { 127 } else { 0 })
}

/// Set or change a parameter of the device (former `handleParameter`).
pub fn handle_parameter(ctx: &mut Ctx, device: DeviceRef, knob_mode: KnobMode, index: usize, value: MidiValue) {
    let changers = value_changers(ctx, knob_mode, value);
    if let Some(bank) = parameter_bank_mut(ctx, device) {
        set_or_change(bank.item_mut(index), &changers, knob_mode, value);
    }
}

/// Toggle a parameter of the device between 0 and the maximum.
pub fn toggle_device_parameter(ctx: &mut Ctx, device: DeviceRef, index: usize) {
    let upper_bound = ctx.model.value_changer().upper_bound();
    if let Some(bank) = parameter_bank_mut(ctx, device) {
        toggle_parameter(upper_bound, bank.item_mut(index));
    }
}

/// Reset a parameter of the device.
pub fn reset_device_parameter(ctx: &mut Ctx, device: DeviceRef, index: usize) {
    if let Some(bank) = parameter_bank_mut(ctx, device) {
        bank.item_mut(index).reset_value();
    }
}

/// Select a parameter page of the device.
pub fn select_parameter_page(ctx: &mut Ctx, device: DeviceRef, index: usize) {
    if let Some(bank) = parameter_bank_mut(ctx, device) {
        bank.select_page(index);
    }
    mv_helper::notify_selected_device_and_parameter_page(ctx);
}

/// Apply a function to the parameter bank of the device.
pub fn with_parameter_bank(ctx: &mut Ctx, device: DeviceRef, function: impl FnOnce(&mut ParameterBank)) {
    if let Some(bank) = parameter_bank_mut(ctx, device) {
        function(bank);
    }
}

/// Scroll the parameter pages with a relative knob (former `scrollParameterPage`).
pub fn scroll_parameter_page(ctx: &mut Ctx, device: DeviceRef, category: CommandCategory, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) || !increase_knob_movement(ctx, category) {
        return;
    }
    let increase = is_increase(ctx, knob_mode, value);
    with_parameter_bank(ctx, device, |bank| if increase { bank.scroll_forwards() } else { bank.scroll_backwards() });
    mv_helper::notify_selected_parameter_page(ctx);
}

/// Scroll the parameter banks with a relative knob (former `scrollParameterBank`).
pub fn scroll_parameter_bank(ctx: &mut Ctx, device: DeviceRef, category: CommandCategory, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) || !increase_knob_movement(ctx, category) {
        return;
    }
    let increase = is_increase(ctx, knob_mode, value);
    with_parameter_bank(ctx, device, |bank| if increase { bank.select_next_page() } else { bank.select_previous_page() });
    mv_helper::notify_selected_parameter_page(ctx);
}

/// The focused (last touched) parameter of the cursor device (former
/// `FocusedParameterImpl.getParam`).
fn focused_parameter(ctx: &Ctx) -> Option<&Parameter> {
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return None;
    }
    let index = usize::try_from(device.last_touched_parameter_index()).ok()?;
    let bank = device.parameter_bank()?;
    (index < bank.unpaged_item_count()).then(|| bank.unpaged_item(index))
}

fn focused_parameter_mut(ctx: &mut Ctx) -> Option<&mut Parameter> {
    let device = ctx.model.cursor_device_mut();
    if !device.does_exist() {
        return None;
    }
    let index = usize::try_from(device.last_touched_parameter_index()).ok()?;
    let bank = device.parameter_bank_mut()?;
    (index < bank.unpaged_item_count()).then(|| bank.unpaged_item_mut(index))
}

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let cursor_device = ctx.model.cursor_device();
    let device = DeviceRef::Cursor;
    if let Some(index) = command.index_in(FlexiCommand::DeviceSetParameter1, 8) {
        return parameter_value(ctx, device, index);
    }
    if let Some(index) = command.index_in(FlexiCommand::DeviceToggleParameter1, 8) {
        return parameter_toggle_value(ctx, device, index);
    }
    if let Some(index) = command.index_in(FlexiCommand::DeviceSelectParameterPage1, 8) {
        return parameter_page_value(ctx, device, index);
    }
    match command {
        FlexiCommand::DeviceToggleWindow => to_midi_value(cursor_device.is_window_open()),
        FlexiCommand::DeviceToggleBypass => to_midi_value(!cursor_device.is_enabled()),
        FlexiCommand::DeviceTogglePin => to_midi_value(cursor_device.is_pinned()),
        FlexiCommand::DeviceToggleExpand => to_midi_value(cursor_device.is_expanded()),
        FlexiCommand::DeviceToggleParameters => to_midi_value(cursor_device.is_parameter_page_section_visible()),
        FlexiCommand::DeviceLastParam => focused_parameter(ctx).filter(|p| p.does_exist()).map_or(0, |p| p.value()),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);
    let device = DeviceRef::Cursor;

    if let Some(index) = command.index_in(FlexiCommand::DeviceSetParameter1, 8) {
        handle_parameter(ctx, device, knob_mode, index, value);
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::DeviceToggleParameter1, 8) {
        if is_button_pressed {
            toggle_device_parameter(ctx, device, index);
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::DeviceResetParameter1, 8) {
        if is_button_pressed {
            reset_device_parameter(ctx, device, index);
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::DeviceSelectParameterPage1, 8) {
        if is_button_pressed {
            select_parameter_page(ctx, device, index);
        }
        return;
    }

    match command {
        FlexiCommand::DeviceScrollDevices => scroll_device(ctx, knob_mode, value),
        FlexiCommand::DeviceScrollParameterPages => scroll_parameter_page(ctx, device, CommandCategory::Device, knob_mode, value),
        FlexiCommand::DeviceScrollParameterBanks => scroll_parameter_bank(ctx, device, CommandCategory::Device, knob_mode, value),
        FlexiCommand::DeviceLastParam => {
            let changers = value_changers(ctx, knob_mode, value);
            if let Some(parameter) = focused_parameter_mut(ctx).filter(|p| p.does_exist()) {
                set_or_change(parameter, &changers, knob_mode, value);
            }
        }
        _ => {
            if !is_button_pressed {
                return;
            }
            let cursor_device = ctx.model.cursor_device_mut();
            match command {
                FlexiCommand::DeviceToggleWindow => cursor_device.toggle_window_open(),
                FlexiCommand::DeviceToggleBypass => cursor_device.toggle_enabled_state(),
                FlexiCommand::DeviceTogglePin => cursor_device.toggle_pinned(),
                FlexiCommand::DeviceToggleExpand => cursor_device.toggle_expanded(),
                FlexiCommand::DeviceToggleParameters => cursor_device.toggle_parameter_page_section_visible(),
                FlexiCommand::DeviceSelectPrevious => cursor_device.select_previous(),
                FlexiCommand::DeviceSelectNext => cursor_device.select_next(),
                FlexiCommand::DeviceSelectPreviousParameterPage => with_parameter_bank(ctx, device, |bank| bank.scroll_backwards()),
                FlexiCommand::DeviceSelectNextParameterPage => with_parameter_bank(ctx, device, |bank| bank.scroll_forwards()),
                FlexiCommand::DeviceSelectPreviousParameterBank => with_parameter_bank(ctx, device, |bank| bank.select_previous_page()),
                FlexiCommand::DeviceSelectNextParameterBank => with_parameter_bank(ctx, device, |bank| bank.select_next_page()),
                _ => unsupported(ctx, command),
            }
        }
    }
}

fn scroll_device(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) || !increase_knob_movement(ctx, CommandCategory::Device) {
        return;
    }
    if is_increase(ctx, knob_mode, value) {
        ctx.model.cursor_device_mut().select_next();
    } else {
        ctx.model.cursor_device_mut().select_previous();
    }
    mv_helper::notify_selected_device(ctx);
}
