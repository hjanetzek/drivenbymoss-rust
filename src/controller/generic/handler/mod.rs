// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Flexi command handlers (former `flexihandler` package).
//!
//! Each Java handler class (`IFlexiCommandHandler`) supports exactly the commands of one
//! [`CommandCategory`], so the handlers are modules with two functions, which are dispatched by
//! the category of the command:
//!
//! - `command_value (ctx, command) -> i32`: the current value of the command in the 14 bit range
//!   (former `getCommandValue`), -1 if it has none.
//! - `handle (ctx, command, knob_mode, value)`: execute the command (former `handle`).
//!
//! The helper functions of `AbstractHandler` are the functions of this module. The state of the
//! handlers (the movement counter for the scroll commands) is stored per category in
//! [`super::surface::FlexiState`].

pub mod action;
pub mod browser;
pub mod clip;
pub mod device;
pub mod eq;
pub mod fx_track;
pub mod global;
pub mod groove;
pub mod instrument_device;
pub mod layer;
pub mod layout;
pub mod marker;
pub mod master;
pub mod midi_cc;
pub mod modes;
pub mod note_input;
pub mod project_remotes;
pub mod scene;
pub mod track;
pub mod track_remotes;
pub mod transport;

use super::command::{CommandCategory, FlexiCommand};
use super::slot::{KnobMode, MidiValue};
use super::surface::{flexi, flexi_mut};
use crate::daw::parameter::Parameter;
use crate::framework::ctx::Ctx;
use crate::framework::value_changer::ValueChanger;

/// The number of relative knob movements needed for one scroll step (former `SCROLL_RATE`).
pub const SCROLL_RATE: i32 = 6;

/// True if a handler is registered for the command (all commands except Off and the function
/// layer commands, which are handled by the surface).
pub fn has_handler(command: FlexiCommand) -> bool {
    !matches!(command.category(), None | Some(CommandCategory::FunctionLayers))
}

/// Get the current value of a command (former `getCommandValue` of the handlers).
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let Some(category) = command.category() else {
        return -1;
    };
    match category {
        CommandCategory::Browser => browser::command_value(ctx, command),
        CommandCategory::Clip => clip::command_value(ctx, command),
        CommandCategory::Device => device::command_value(ctx, command),
        CommandCategory::FunctionLayers => -1,
        CommandCategory::Groove => groove::command_value(ctx, command),
        CommandCategory::InstrumentDevice => instrument_device::command_value(ctx, command),
        CommandCategory::Eq => eq::command_value(ctx, command),
        CommandCategory::FxTrack => fx_track::command_value(ctx, command),
        CommandCategory::Global => global::command_value(ctx, command),
        CommandCategory::Layer => layer::command_value(ctx, command),
        CommandCategory::Layout => layout::command_value(ctx, command),
        CommandCategory::Marker => marker::command_value(ctx, command),
        CommandCategory::Master => master::command_value(ctx, command),
        CommandCategory::MidiCc => midi_cc::command_value(ctx, command),
        CommandCategory::Modes => modes::command_value(ctx, command),
        CommandCategory::NoteInput => note_input::command_value(ctx, command),
        CommandCategory::ProjectRemotes => project_remotes::command_value(ctx, command),
        CommandCategory::Scene => scene::command_value(ctx, command),
        CommandCategory::Track => track::command_value(ctx, command),
        CommandCategory::TrackRemotes => track_remotes::command_value(ctx, command),
        CommandCategory::Transport => transport::command_value(ctx, command),
        CommandCategory::Action => action::command_value(ctx, command),
    }
}

/// Execute a command (former `handle` of the handlers).
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let Some(category) = command.category() else {
        return;
    };
    match category {
        CommandCategory::Browser => browser::handle(ctx, command, knob_mode, value),
        CommandCategory::Clip => clip::handle(ctx, command, knob_mode, value),
        CommandCategory::Device => device::handle(ctx, command, knob_mode, value),
        CommandCategory::FunctionLayers => {}
        CommandCategory::Groove => groove::handle(ctx, command, knob_mode, value),
        CommandCategory::InstrumentDevice => instrument_device::handle(ctx, command, knob_mode, value),
        CommandCategory::Eq => eq::handle(ctx, command, knob_mode, value),
        CommandCategory::FxTrack => fx_track::handle(ctx, command, knob_mode, value),
        CommandCategory::Global => global::handle(ctx, command, knob_mode, value),
        CommandCategory::Layer => layer::handle(ctx, command, knob_mode, value),
        CommandCategory::Layout => layout::handle(ctx, command, knob_mode, value),
        CommandCategory::Marker => marker::handle(ctx, command, knob_mode, value),
        CommandCategory::Master => master::handle(ctx, command, knob_mode, value),
        CommandCategory::MidiCc => midi_cc::handle(ctx, command, knob_mode, value),
        CommandCategory::Modes => modes::handle(ctx, command, knob_mode, value),
        CommandCategory::NoteInput => note_input::handle(ctx, command, knob_mode, value),
        CommandCategory::ProjectRemotes => project_remotes::handle(ctx, command, knob_mode, value),
        CommandCategory::Scene => scene::handle(ctx, command, knob_mode, value),
        CommandCategory::Track => track::handle(ctx, command, knob_mode, value),
        CommandCategory::TrackRemotes => track_remotes::handle(ctx, command, knob_mode, value),
        CommandCategory::Transport => transport::handle(ctx, command, knob_mode, value),
        CommandCategory::Action => action::handle(ctx, command, knob_mode, value),
    }
}

/// Report a command which a handler does not support (former `FlexiHandlerException`).
pub fn unsupported(ctx: &Ctx, command: FlexiCommand) {
    ctx.host.error(&format!("Unsupported flexi command: {}", command.name()));
}

// ------------------------------------------------------------------------------------------
// Former AbstractHandler

/// The value changer for absolute values (former `getAbsoluteValueChanger`).
pub fn absolute_value_changer(ctx: &Ctx, value: MidiValue) -> ValueChanger {
    flexi(ctx).absolute_value_changer(value).clone()
}

/// The value changer for relative values (former `getRelativeValueChanger`).
pub fn relative_value_changer(ctx: &Ctx, knob_mode: KnobMode) -> ValueChanger {
    flexi(ctx).relative_value_changer(knob_mode).clone()
}

/// True if the relative value increases (former `isIncrease`).
pub fn is_increase(ctx: &Ctx, knob_mode: KnobMode, control: MidiValue) -> bool {
    flexi(ctx).relative_value_changer(knob_mode).calc_knob_change(control.value()) > 0.0
}

/// Former `isAbsolute`.
pub fn is_absolute(knob_mode: KnobMode) -> bool {
    knob_mode.is_absolute()
}

/// True if a button is pressed: always for toggle buttons, a value > 0 for push buttons (former
/// `isButtonPressed`).
pub fn is_button_pressed(knob_mode: KnobMode, value: MidiValue) -> bool {
    knob_mode == KnobMode::AbsoluteToggle || knob_mode == KnobMode::Absolute && value.is_positive()
}

/// Count the relative knob movements of the handler of a category, returns true every
/// [`SCROLL_RATE`] movements (former `increaseKnobMovement`).
pub fn increase_knob_movement(ctx: &mut Ctx, category: CommandCategory) -> bool {
    let counter = &mut flexi_mut(ctx).movement_counters[category.ordinal()];
    *counter += 1;
    if *counter < SCROLL_RATE {
        return false;
    }
    *counter = 0;
    true
}

/// Former `toMidiValue`: the maximum 14 bit value for true.
pub fn to_midi_value(state: bool) -> i32 {
    if state {
        16383
    } else {
        0
    }
}

/// The value changers for a value: the absolute and the relative one.
pub fn value_changers(ctx: &Ctx, knob_mode: KnobMode, value: MidiValue) -> (ValueChanger, ValueChanger) {
    (absolute_value_changer(ctx, value), relative_value_changer(ctx, knob_mode))
}

/// Set the value of a parameter with the absolute value changer or change it with the relative
/// one (the frequent Java pattern `if (isAbsolute (knobMode)) parameter.setValue
/// (this.getAbsoluteValueChanger (value), val); else parameter.changeValue
/// (this.getRelativeValueChanger (knobMode), val);`).
pub fn set_or_change(parameter: &mut Parameter, changers: &(ValueChanger, ValueChanger), knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) {
        parameter.set_value_with(&changers.0, value.value());
    } else {
        parameter.change_value_with(&changers.1, value.value());
    }
}

/// Toggle a parameter between 0 and the maximum (the Java pattern `setValue (v > 0 ? 0 :
/// upperBound - 1)`).
pub fn toggle_parameter(upper_bound: i32, parameter: &mut Parameter) {
    let value = parameter.value();
    parameter.set_value(if value > 0 { 0 } else { upper_bound - 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_pressed() {
        assert!(is_button_pressed(KnobMode::AbsoluteToggle, MidiValue::new(0, false)));
        assert!(is_button_pressed(KnobMode::Absolute, MidiValue::new(1, false)));
        assert!(!is_button_pressed(KnobMode::Absolute, MidiValue::new(0, false)));
        assert!(!is_button_pressed(KnobMode::RelativeSignedBit, MidiValue::new(127, false)));
        assert_eq!(to_midi_value(true), 16383);
        assert!(has_handler(FlexiCommand::TransportPlay));
        assert!(!has_handler(FlexiCommand::Off));
        assert!(!has_handler(FlexiCommand::FunctionLayer3Temp));
    }
}
