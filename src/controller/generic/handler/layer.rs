// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for the layer commands of the cursor device (former `LayerHandler`).
//!
//! Reaper has no device layers (the commands do nothing there); hosts with layers report them
//! (see `LayerBank`). The send commands are ordered by send, then by layer, as the track
//! commands.

use super::{increase_knob_movement, is_absolute, is_button_pressed, is_increase, set_or_change, to_midi_value, unsupported, value_changers};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::bank::BankItem;
use crate::daw::channel::Channel;
use crate::daw::device::LayerBank;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

/// A state of a layer which can be toggled or set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerState {
    Active,
    Mute,
    Solo,
}

impl LayerState {
    fn get(self, layer: &Channel) -> bool {
        match self {
            LayerState::Active => layer.is_activated(),
            LayerState::Mute => layer.is_mute(),
            LayerState::Solo => layer.is_solo(),
        }
    }

    fn toggle(self, layer: &mut Channel) {
        match self {
            LayerState::Active => layer.toggle_is_activated(),
            LayerState::Mute => layer.toggle_mute(),
            LayerState::Solo => layer.toggle_solo(),
        }
    }

    fn set(self, layer: &mut Channel, enable: bool) {
        match self {
            LayerState::Active => layer.set_is_activated(enable),
            LayerState::Mute => layer.set_mute(enable),
            LayerState::Solo => layer.set_solo(enable),
        }
    }
}

/// The commands of a layer state: the first toggle command (layer 1), the toggle command of the
/// selected layer, the first set command, the set command of the selected layer.
const STATE_COMMANDS: [(LayerState, FlexiCommand, FlexiCommand, FlexiCommand, FlexiCommand); 3] = [
    (LayerState::Active, FlexiCommand::Layer1ToggleActive, FlexiCommand::LayerSelectedToggleActive, FlexiCommand::Layer1SetActive, FlexiCommand::LayerSelectedSetActive),
    (LayerState::Mute, FlexiCommand::Layer1ToggleMute, FlexiCommand::LayerSelectedToggleMute, FlexiCommand::Layer1SetMute, FlexiCommand::LayerSelectedSetMute),
    (LayerState::Solo, FlexiCommand::Layer1ToggleSolo, FlexiCommand::LayerSelectedToggleSolo, FlexiCommand::Layer1SetSolo, FlexiCommand::LayerSelectedSetSolo),
];

/// The state, the layer index (-1 for the selected layer) and if it is a toggle command.
fn state_command(command: FlexiCommand) -> Option<(LayerState, i32, bool)> {
    for (state, first_toggle, selected_toggle, first_set, selected_set) in STATE_COMMANDS {
        if let Some(index) = command.index_in(first_toggle, 8) {
            return Some((state, index as i32, true));
        }
        if command == selected_toggle {
            return Some((state, -1, true));
        }
        if let Some(index) = command.index_in(first_set, 8) {
            return Some((state, index as i32, false));
        }
        if command == selected_set {
            return Some((state, -1, false));
        }
    }
    None
}

/// The layer and send index of a send command, the layer index is -1 for the selected layer.
fn send_command(command: FlexiCommand) -> Option<(i32, usize)> {
    if let Some(index) = command.index_in(FlexiCommand::Layer1SetSend1, 64) {
        return Some(((index % 8) as i32, index / 8));
    }
    command.index_in(FlexiCommand::LayerSelectedSetSend1, 8).map(|send| (-1, send))
}

/// The layer bank of the cursor device if it has layers (former `getLayerBank`).
fn layer_bank(ctx: &Ctx) -> Option<&LayerBank> {
    let cursor_device = ctx.model.cursor_device();
    cursor_device.has_layers().then(|| cursor_device.layer_bank())
}

fn layer_bank_mut(ctx: &mut Ctx) -> Option<&mut LayerBank> {
    let cursor_device = ctx.model.cursor_device_mut();
    if cursor_device.has_layers() {
        Some(cursor_device.layer_bank_mut())
    } else {
        None
    }
}

/// The index of a layer in the page: the selected layer for a negative index, None if the layer
/// does not exist (former `getLayer`).
fn layer_index(layer_bank: &LayerBank, index: i32) -> Option<usize> {
    if index < 0 {
        return layer_bank.selected_index();
    }
    let index = index as usize;
    layer_bank.item(index).does_exist().then_some(index)
}

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let Some(layer_bank) = layer_bank(ctx) else {
        return -1;
    };
    let selected_layer = layer_bank.selected_item();

    if let Some((state, index, _)) = state_command(command) {
        return to_midi_value(if index < 0 { selected_layer.is_some_and(|layer| state.get(layer)) } else { state.get(layer_bank.item(index as usize)) });
    }
    if let Some(index) = command.index_in(FlexiCommand::Layer1Select, 8) {
        return to_midi_value(layer_bank.item(index).is_selected());
    }
    if let Some(index) = command.index_in(FlexiCommand::Layer1SetVolume, 8) {
        return layer_bank.item(index).volume();
    }
    if let Some(index) = command.index_in(FlexiCommand::Layer1SetPanning, 8) {
        return layer_bank.item(index).pan();
    }
    if let Some((layer, send)) = send_command(command) {
        return send_value(layer_bank, layer, send);
    }
    match command {
        FlexiCommand::LayerSelectedSetVolumeLayer => selected_layer.map_or(0, |layer| layer.volume()),
        FlexiCommand::LayerSelectedSetPanning => selected_layer.map_or(0, |layer| layer.pan()),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    if layer_bank(ctx).is_none() {
        return;
    }
    let is_button_pressed = is_button_pressed(knob_mode, value);

    if let Some((state, index, is_toggle)) = state_command(command) {
        if !is_button_pressed {
            return;
        }
        let Some(layer_bank) = layer_bank_mut(ctx) else {
            return;
        };
        let index = if index < 0 { layer_bank.selected_index() } else { Some(index as usize) };
        if let Some(index) = index {
            let layer = layer_bank.item_mut(index);
            if is_toggle {
                state.toggle(layer);
            } else {
                state.set(layer, value.is_positive());
            }
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::Layer1Select, 8) {
        if is_button_pressed {
            if let Some(layer_bank) = layer_bank_mut(ctx) {
                layer_bank.item_mut(index).select();
            }
            mv_helper::notify_selected_layer(ctx);
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::Layer1SetVolume, 8) {
        change_layer_parameter(ctx, knob_mode, index as i32, value, LayerParameter::Volume);
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::Layer1SetPanning, 8) {
        change_layer_parameter(ctx, knob_mode, index as i32, value, LayerParameter::Pan);
        return;
    }
    if let Some((layer, send)) = send_command(command) {
        change_layer_parameter(ctx, knob_mode, layer, value, LayerParameter::Send(send));
        return;
    }

    match command {
        FlexiCommand::LayerSelectPreviousBankPage => {
            if is_button_pressed {
                scroll_layer_left(ctx, true);
            }
        }
        FlexiCommand::LayerSelectNextBankPage => {
            if is_button_pressed {
                scroll_layer_right(ctx, true);
            }
        }
        FlexiCommand::LayerSelectPreviousLayer => {
            if is_button_pressed {
                scroll_layer_left(ctx, false);
            }
        }
        FlexiCommand::LayerSelectNextLayer => {
            if is_button_pressed {
                scroll_layer_right(ctx, false);
            }
        }
        FlexiCommand::LayerScrollLayers => scroll_layer(ctx, knob_mode, value),
        FlexiCommand::LayerSelectedSetVolumeLayer => change_layer_parameter(ctx, knob_mode, -1, value, LayerParameter::Volume),
        FlexiCommand::LayerSelectedSetPanning => change_layer_parameter(ctx, knob_mode, -1, value, LayerParameter::Pan),
        _ => unsupported(ctx, command),
    }
}

/// Former `getSendValue`.
fn send_value(layer_bank: &LayerBank, layer_index: i32, send_index: usize) -> i32 {
    let Some(index) = self::layer_index(layer_bank, layer_index) else {
        return 0;
    };
    let send_bank = layer_bank.item(index).send_bank();
    if send_index >= send_bank.page_size() {
        return 0;
    }
    send_bank.item(send_index).value()
}

/// The parameter of a layer.
enum LayerParameter {
    Volume,
    Pan,
    Send(usize),
}

/// Former `changeLayerVolume`, `changeLayerPanning` and `changeSendVolume`.
fn change_layer_parameter(ctx: &mut Ctx, knob_mode: KnobMode, layer_index: i32, value: MidiValue, parameter: LayerParameter) {
    let changers = value_changers(ctx, knob_mode, value);
    let Some(layer_bank) = layer_bank_mut(ctx) else {
        return;
    };
    let Some(index) = self::layer_index(layer_bank, layer_index) else {
        return;
    };
    let layer = layer_bank.item_mut(index);
    match parameter {
        LayerParameter::Volume => set_or_change(layer.volume_parameter_mut(), &changers, knob_mode, value),
        LayerParameter::Pan => set_or_change(layer.pan_parameter_mut(), &changers, knob_mode, value),
        LayerParameter::Send(send_index) => {
            let send_bank = layer.send_bank_mut();
            if send_index < send_bank.page_size() {
                set_or_change(send_bank.item_mut(send_index), &changers, knob_mode, value);
            }
        }
    }
}

fn scroll_layer(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) || !increase_knob_movement(ctx, CommandCategory::Layer) {
        return;
    }
    if is_increase(ctx, knob_mode, value) {
        scroll_layer_right(ctx, false);
    } else {
        scroll_layer_left(ctx, false);
    }
}

/// Former `scrollLayerLeft`.
fn scroll_layer_left(ctx: &mut Ctx, switch_bank: bool) {
    let Some(layer_bank) = layer_bank_mut(ctx) else {
        return;
    };
    let index = layer_bank.selected_item().map_or(0, |layer| layer.index() - 1);
    if index == -1 || switch_bank {
        layer_bank.select_previous_page();
        return;
    }
    if let Ok(index) = usize::try_from(index) {
        layer_bank.item_mut(index).select();
    }
}

/// Former `scrollLayerRight`.
fn scroll_layer_right(ctx: &mut Ctx, switch_bank: bool) {
    let Some(layer_bank) = layer_bank_mut(ctx) else {
        return;
    };
    let index = layer_bank.selected_item().map_or(0, |layer| layer.index() + 1);
    if index == 8 || switch_bank {
        layer_bank.select_next_page();
        return;
    }
    if let Ok(index) = usize::try_from(index) {
        layer_bank.item_mut(index).select();
    }
}
