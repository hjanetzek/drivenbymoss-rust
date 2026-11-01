// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Layer modes (former `framework.mode.layer`: `DefaultLayerMode`, `LayerMuteMode` and
//! `LayerSoloMode`).
//!
//! As the Java version, the modes work on the drum pads of the first instrument if it has drum
//! pads, otherwise on its layers. Reaper has no device layers but (virtual) drum pads
//! (`hasDrumPads` is always true); hosts with layers report them (protocol v2).

use std::marker::PhantomData;

use crate::daw::channel::Channel;
use crate::daw::constants::DeviceId;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::{uses_layers, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::DrumDeviceRef;
use crate::midi::ButtonEvent;

/// The mode specific function of a layer mode (former `executeMethod`).
pub trait LayerModeBehavior: 'static {
    fn execute_method(layer: &mut Channel);
}

/// Get the layer (drum pad or layer) at the index of the page of the first instrument.
pub fn layer(ctx: &Ctx, index: usize) -> Option<&Channel> {
    let device = ctx.model.specific_device(DeviceId::FirstInstrument)?;
    Some(if uses_layers(device) { device.layer_bank().item(index) } else { &**device.drum_pad_bank().item(index) })
}

/// Get the layer (drum pad or layer) at the index of the page of the first instrument for
/// modification.
pub fn layer_mut(ctx: &mut Ctx, index: usize) -> Option<&mut Channel> {
    let device = ctx.model.specific_device_mut(DeviceId::FirstInstrument)?;
    Some(if uses_layers(device) { device.layer_bank_mut().item_mut(index) } else { &mut **device.drum_pad_bank_mut().item_mut(index) })
}

/// The index of the selected layer (drum pad or layer) in the page of the first instrument.
fn selected_layer_index(ctx: &Ctx) -> Option<usize> {
    let device = ctx.model.specific_device(DeviceId::FirstInstrument)?;
    if uses_layers(device) {
        device.layer_bank().selected_index()
    } else {
        device.drum_pad_bank().selected_index()
    }
}

/// The base of the layer modes (former `DefaultLayerMode`).
pub struct DefaultLayerMode<B: LayerModeBehavior> {
    pub base: ParameterModeBase,
    behavior: PhantomData<B>,
}

impl<B: LayerModeBehavior> DefaultLayerMode<B> {
    pub fn with_name(name: &str, is_absolute: bool) -> Self {
        DefaultLayerMode { base: ParameterModeBase::without_controls(name, is_absolute, ModeBank::DrumPadBank(DrumDeviceRef::FirstInstrument)), behavior: PhantomData }
    }

    /// Get the layer at the index or the selected layer if the index is negative (former
    /// `getLayer`).
    pub fn layer_index(ctx: &Ctx, index: i32) -> Option<usize> {
        if index >= 0 {
            return Some(index as usize);
        }
        selected_layer_index(ctx)
    }

    /// Handle the Delete and Duplicate combinations (former `isButtonCombination`).
    fn is_button_combination(ctx: &mut Ctx, index: usize) -> bool {
        if ctx.is_button_combination(ButtonId::Delete) {
            if let Some(layer) = layer_mut(ctx, index) {
                layer.remove();
            }
            return true;
        }
        if ctx.is_button_combination(ButtonId::Duplicate) {
            if let Some(layer) = layer_mut(ctx, index) {
                layer.duplicate();
            }
            return true;
        }
        false
    }

    /// Former `onButton`.
    pub fn handle_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down || row != 0 {
            return;
        }
        if !Self::is_button_combination(ctx, index) {
            if let Some(layer) = layer_mut(ctx, index) {
                B::execute_method(layer);
            }
        }
    }
}

impl<B: LayerModeBehavior> Mode for DefaultLayerMode<B> {
    // AbstractParameterMode
    crate::forward_mode!(base => name, on_activate, on_deactivate, select_item,
        select_previous_item, select_next_item, select_previous_item_page, select_next_item_page,
        has_previous_item, has_next_item, has_previous_item_page, has_next_item_page);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        self.handle_button(ctx, row, index, event);
    }

    fn selected_item_name(&self, ctx: &Ctx) -> Option<String> {
        let layer = layer(ctx, selected_layer_index(ctx)?)?;
        layer.does_exist().then(|| format!("{}: {}", layer.position() + 1, layer.name()))
    }

    crate::impl_as_any!();
}

/// Toggles mute (former `LayerMuteMode`).
pub struct LayerMuteBehavior;
impl LayerModeBehavior for LayerMuteBehavior {
    fn execute_method(layer: &mut Channel) {
        layer.toggle_mute();
    }
}

/// Toggles solo (former `LayerSoloMode`).
pub struct LayerSoloBehavior;
impl LayerModeBehavior for LayerSoloBehavior {
    fn execute_method(layer: &mut Channel) {
        layer.toggle_solo();
    }
}

pub type LayerMuteMode = DefaultLayerMode<LayerMuteBehavior>;
pub type LayerSoloMode = DefaultLayerMode<LayerSoloBehavior>;

impl DefaultLayerMode<LayerMuteBehavior> {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name("Layer Mute", true)
    }
}

impl DefaultLayerMode<LayerSoloBehavior> {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name("Layer Solo", true)
    }
}
