// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Layer modes (former `framework.mode.layer`: `DefaultLayerMode`, `LayerMuteMode` and
//! `LayerSoloMode`).
//!
//! Reaper has no device layers but (virtual) drum pads, which the Java version uses instead
//! (`hasDrumPads` is always true). The bank of the modes is therefore always the drum pad bank of
//! the first instrument.

use std::marker::PhantomData;

use crate::daw::bank::BankItem;
use crate::daw::channel::Channel;
use crate::daw::constants::DeviceId;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::DrumDeviceRef;
use crate::midi::ButtonEvent;

/// The mode specific function of a layer mode (former `executeMethod`).
pub trait LayerModeBehavior: 'static {
    fn execute_method(layer: &mut Channel);
}

/// Get the layer (drum pad) at the index of the page of the first instrument.
pub fn layer(ctx: &Ctx, index: usize) -> Option<&Channel> {
    ctx.model.specific_device(DeviceId::FirstInstrument).map(|d| &**d.drum_pad_bank().item(index))
}

/// Get the layer (drum pad) at the index of the page of the first instrument for modification.
pub fn layer_mut(ctx: &mut Ctx, index: usize) -> Option<&mut Channel> {
    ctx.model.specific_device_mut(DeviceId::FirstInstrument).map(|d| &mut **d.drum_pad_bank_mut().item_mut(index))
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
        ctx.model.specific_device(DeviceId::FirstInstrument)?.drum_pad_bank().selected_index()
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
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        self.handle_button(ctx, row, index, event);
    }

    fn selected_item_name(&self, ctx: &Ctx) -> Option<String> {
        let bank = ctx.model.specific_device(DeviceId::FirstInstrument)?.drum_pad_bank();
        let layer = bank.selected_item()?;
        layer.does_exist().then(|| format!("{}: {}", layer.position() + 1, layer.name()))
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.base.select_item(ctx, index);
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item_page(ctx);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item_page(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item_page(ctx)
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
