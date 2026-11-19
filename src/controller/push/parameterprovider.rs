// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The parameter providers of the Push (former `controller.ableton.push.parameterprovider`).
//!
//! The layer providers work on the layer or drum pad bank of the cursor device (former
//! `ISpecificDevice.getLayerOrDrumPadBank`): the drum pads if the device has drum pads, otherwise
//! its layers. The generic [`crate::framework::parameterprovider::ParamRef`] only addresses the
//! drum pads, therefore the parameters of the layers are [`ParamRef::Custom`] parameters which
//! are resolved when a value arrives (no parameter observers are needed: the former
//! `IItemSelectionObserver`s which re-bind the knobs when the selected drum pad changes are not
//! needed since the offset is calculated on every access).

use std::rc::Rc;

use crate::daw::channel::Channel;
use crate::daw::color::ColorEx;
use crate::daw::model::Model;
use crate::daw::parameter::Parameter;
use crate::framework::ctx::Ctx;
use crate::framework::mode::parameter_mode::uses_layers;
use crate::framework::parameterprovider::{CustomParameter, LayerParameter, ParamRef, ParameterProvider};

use super::surface;

/// The number of knobs of the Push.
const SIZE: usize = 8;

// ------------------------------------------------------------------------------------------
// Access to the layers of the cursor device

/// The layer (drum pad or layer) at the index of the page of the layer or drum pad bank of the
/// cursor device (former `cursorDevice.getLayerOrDrumPadBank ().getItem (index)`).
pub fn cursor_layer(model: &Model, index: usize) -> &Channel {
    let device = model.cursor_device();
    if uses_layers(device) {
        device.layer_bank().item(index)
    } else {
        device.drum_pad_bank().item(index)
    }
}

/// See [`cursor_layer`], for modification.
pub fn cursor_layer_mut(model: &mut Model, index: usize) -> &mut Channel {
    let device = model.cursor_device_mut();
    if uses_layers(device) {
        device.layer_bank_mut().item_mut(index)
    } else {
        device.drum_pad_bank_mut().item_mut(index)
    }
}

/// The index (in the page) of the selected layer or drum pad of the cursor device.
pub fn selected_cursor_layer_index(model: &Model) -> Option<usize> {
    let device = model.cursor_device();
    if uses_layers(device) {
        device.layer_bank().selected_index()
    } else {
        device.drum_pad_bank().selected_index()
    }
}

/// Select the layer (drum pad or layer) at the index of the page of the cursor device (former
/// `getItem (index).select ()`).
pub fn select_cursor_layer(model: &mut Model, index: usize) {
    use crate::daw::bank::BankItem;
    let device = model.cursor_device_mut();
    if uses_layers(device) {
        device.layer_bank_mut().item_mut(index).select();
    } else {
        device.drum_pad_bank_mut().select_pad(index);
    }
}

/// The page size of the layer or drum pad bank of the cursor device.
pub fn cursor_layer_page_size(model: &Model) -> usize {
    let device = model.cursor_device();
    if uses_layers(device) {
        device.layer_bank().page_size()
    } else {
        device.drum_pad_bank().page_size()
    }
}

/// True if the layer or drum pad bank of the cursor device has existing items.
pub fn cursor_layers_exist(model: &Model) -> bool {
    let device = model.cursor_device();
    if uses_layers(device) {
        device.layer_bank().has_existing_items()
    } else {
        device.drum_pad_bank().has_existing_items()
    }
}

/// The drum pad bank has a size of 16, the layers only 8: the offset of the 8 knobs is 8 if the
/// selected drum pad is in the upper half (former `getDrumPadIndex`).
pub fn drum_pad_offset(model: &Model) -> usize {
    if model.cursor_device().has_drum_pads() {
        if let Some(index) = selected_cursor_layer_index(model) {
            if index > 7 {
                return 8;
            }
        }
    }
    0
}

fn channel_parameter(channel: &Channel, parameter: LayerParameter) -> Option<&Parameter> {
    match parameter {
        LayerParameter::Volume => Some(channel.volume_parameter()),
        LayerParameter::Pan => Some(channel.pan_parameter()),
        LayerParameter::Send(send) => {
            let send_bank = channel.send_bank();
            (send < send_bank.page_size()).then(|| &**send_bank.item(send))
        }
    }
}

fn channel_parameter_mut(channel: &mut Channel, parameter: LayerParameter) -> Option<&mut Parameter> {
    match parameter {
        LayerParameter::Volume => Some(channel.volume_parameter_mut()),
        LayerParameter::Pan => Some(channel.pan_parameter_mut()),
        LayerParameter::Send(send) => {
            let send_bank = channel.send_bank_mut();
            if send >= send_bank.page_size() {
                return None;
            }
            Some(&mut **send_bank.item_mut(send))
        }
    }
}

/// A parameter of a layer (or drum pad) of the cursor device: of the layer at the index of the
/// page or of the selected layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorLayerParameter {
    /// The index of the layer in the page, `None` for the selected layer.
    pub layer: Option<usize>,
    pub parameter: LayerParameter,
}

impl CursorLayerParameter {
    fn layer_index(&self, model: &Model) -> Option<usize> {
        match self.layer {
            Some(index) => Some(index),
            None => selected_cursor_layer_index(model),
        }
    }

    fn resolve<'a>(&self, model: &'a Model) -> Option<&'a Parameter> {
        let index = self.layer_index(model)?;
        let channel = cursor_layer(model, index);
        if !channel.does_exist() {
            return None;
        }
        channel_parameter(channel, self.parameter)
    }

    fn resolve_mut<'a>(&self, model: &'a mut Model) -> Option<&'a mut Parameter> {
        let index = self.layer_index(model)?;
        let channel = cursor_layer_mut(model, index);
        if !channel.does_exist() {
            return None;
        }
        channel_parameter_mut(channel, self.parameter)
    }

    /// True if the parameter is a send (former `instanceof ISend`).
    pub fn is_send(&self) -> bool {
        matches!(self.parameter, LayerParameter::Send(_))
    }

    /// Toggle the enabled state of the send (former `ISend.toggleEnabled`).
    pub fn toggle_send_enabled(&self, ctx: &mut Ctx) {
        let LayerParameter::Send(send) = self.parameter else {
            return;
        };
        let Some(index) = self.layer_index(&ctx.model) else {
            return;
        };
        let channel = cursor_layer_mut(&mut ctx.model, index);
        if send < channel.send_bank().page_size() {
            channel.send_bank_mut().item_mut(send).toggle_enabled();
        }
    }
}

impl CustomParameter for CursorLayerParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool {
        self.resolve(&ctx.model).is_some_and(|p| p.does_exist())
    }

    fn name(&self, ctx: &Ctx) -> String {
        self.resolve(&ctx.model).map(|p| p.name().to_string()).unwrap_or_default()
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        self.resolve(&ctx.model).map(|p| p.value()).unwrap_or(0)
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        self.resolve(&ctx.model).map(|p| p.displayed_value()).unwrap_or_default()
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        if let Some(p) = self.resolve_mut(&mut ctx.model) {
            p.set_value(value);
        }
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        if let Some(p) = self.resolve_mut(&mut ctx.model) {
            p.set_normalized_value(value);
        }
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        if let Some(p) = self.resolve_mut(&mut ctx.model) {
            p.change_value(control);
        }
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        if let Some(p) = self.resolve_mut(&mut ctx.model) {
            p.reset_value();
        }
    }

    fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        if let Some(p) = self.resolve_mut(&mut ctx.model) {
            p.touch_value(is_touched);
        }
    }
}

/// Toggle the enabled state of a send parameter (former `if (parameter instanceof ISend send)
/// send.toggleEnabled ()`). Does nothing for other parameters.
pub fn toggle_send_enabled(ctx: &mut Ctx, parameter: &ParamRef) {
    match parameter {
        ParamRef::TrackSend { index, send } => {
            let send_bank = ctx.model.current_track_bank_mut().item_mut(*index).send_bank_mut();
            if *send < send_bank.item_count() {
                send_bank.item_mut(*send).toggle_enabled();
            }
        }
        ParamRef::SelectedTrackSend { send } => {
            if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
                let send_bank = track.send_bank_mut();
                if *send < send_bank.item_count() {
                    send_bank.item_mut(*send).toggle_enabled();
                }
            }
        }
        ParamRef::Custom(custom) => {
            // The only custom send parameters are the ones of the layers
            let custom = custom.clone();
            if let Some(layer) = layer_parameter_of(&custom) {
                layer.toggle_send_enabled(ctx);
            }
        }
        _ => {}
    }
}

/// True if the parameter is a send (former `instanceof ISend`).
pub fn is_send(parameter: &ParamRef) -> bool {
    match parameter {
        ParamRef::TrackSend { .. } | ParamRef::SelectedTrackSend { .. } | ParamRef::DrumPadSend { .. } => true,
        ParamRef::Custom(custom) => layer_parameter_of(custom).is_some_and(|l| l.is_send()),
        _ => false,
    }
}

/// The custom layer parameters are created by this module, remember their descriptions to
/// recognise them (a `dyn CustomParameter` cannot be downcast).
fn layer_parameter_of(custom: &Rc<dyn CustomParameter>) -> Option<CursorLayerParameter> {
    LAYER_PARAMETERS.with(|params| params.borrow().iter().find(|(p, _)| Rc::ptr_eq(p, custom)).map(|(_, d)| *d))
}

thread_local! {
    static LAYER_PARAMETERS: std::cell::RefCell<Vec<(Rc<dyn CustomParameter>, CursorLayerParameter)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Get the shared parameter reference of a layer parameter (created once per description, so
/// that the bindings compare equal and it can be recognised as a send).
pub fn layer_param_ref(description: CursorLayerParameter) -> ParamRef {
    LAYER_PARAMETERS.with(|params| {
        if let Some((p, _)) = params.borrow().iter().find(|(_, d)| *d == description) {
            return ParamRef::Custom(p.clone());
        }
        let p: Rc<dyn CustomParameter> = Rc::new(description);
        params.borrow_mut().push((p.clone(), description));
        ParamRef::Custom(p)
    })
}

// ------------------------------------------------------------------------------------------
// The providers

/// The parameters of the selected track: volume, panorama and the sends; on the Push 2/3 the
/// crossfader, an empty one and 4 sends (former `PushTrackParameterProvider`).
///
/// Divergence: the Rust model has no crossfade mode parameter of a track (former
/// `ITrack.getCrossfadeParameter`), it is empty.
pub struct PushTrackParameterProvider;

impl ParameterProvider for PushTrackParameterProvider {
    fn size(&self) -> usize {
        SIZE
    }

    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef {
        if index >= SIZE || ctx.model.current_track_bank().selected_item().is_none() {
            return ParamRef::Empty;
        }
        match index {
            0 => ParamRef::SelectedTrackVolume,
            1 => ParamRef::SelectedTrackPan,
            _ if !surface::is_modern(ctx) => ParamRef::SelectedTrackSend { send: index - 2 },
            // The crossfade mode parameter of the track (not in the model)
            2 | 3 => ParamRef::Empty,
            _ => ParamRef::SelectedTrackSend { send: index - 4 },
        }
    }

    fn color(&self, ctx: &Ctx, _index: usize) -> Option<ColorEx> {
        let track = ctx.model.current_track_bank().selected_item()?;
        Some(track.color())
    }
}

/// The parameters of the selected layer or drum pad of the cursor device: volume, panorama and
/// the sends; on the Push 2/3 two empty ones and 4 sends (former
/// `PushSelectedLayerOrDrumPadParameterProvider`).
pub struct PushSelectedLayerOrDrumPadParameterProvider;

impl ParameterProvider for PushSelectedLayerOrDrumPadParameterProvider {
    fn size(&self) -> usize {
        SIZE
    }

    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef {
        if index >= SIZE || selected_cursor_layer_index(&ctx.model).is_none() {
            return ParamRef::Empty;
        }
        let parameter = match index {
            0 => LayerParameter::Volume,
            1 => LayerParameter::Pan,
            _ if !surface::is_modern(ctx) => LayerParameter::Send(index - 2),
            2 | 3 => return ParamRef::Empty,
            _ => LayerParameter::Send(index - 4),
        };
        layer_param_ref(CursorLayerParameter { layer: None, parameter })
    }
}

/// The volume, panorama or a send of the 8 layers or drum pads of the cursor device; with drum
/// pads the upper 8 of the 16 pads if the selected pad is one of them (former
/// `PushVolumeLayerOrDrumPadParameterProvider`, `PushPanLayerOrDrumPadParameterProvider` and
/// `PushSendLayerOrDrumPadParameterProvider`).
pub struct PushLayerOrDrumPadParameterProvider {
    parameter: LayerParameter,
}

impl PushLayerOrDrumPadParameterProvider {
    pub fn volume() -> Self {
        PushLayerOrDrumPadParameterProvider { parameter: LayerParameter::Volume }
    }

    pub fn pan() -> Self {
        PushLayerOrDrumPadParameterProvider { parameter: LayerParameter::Pan }
    }

    pub fn send(send_index: usize) -> Self {
        PushLayerOrDrumPadParameterProvider { parameter: LayerParameter::Send(send_index) }
    }
}

impl ParameterProvider for PushLayerOrDrumPadParameterProvider {
    fn size(&self) -> usize {
        SIZE
    }

    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef {
        if index >= SIZE || !ctx.model.cursor_device().has_layers() {
            return ParamRef::Empty;
        }
        let layer = drum_pad_offset(&ctx.model) + index;
        layer_param_ref(CursorLayerParameter { layer: Some(layer), parameter: self.parameter })
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        Some(cursor_layer(&ctx.model, drum_pad_offset(&ctx.model) + index).color())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_param_refs_are_shared() {
        let a = layer_param_ref(CursorLayerParameter { layer: Some(1), parameter: LayerParameter::Send(2) });
        let b = layer_param_ref(CursorLayerParameter { layer: Some(1), parameter: LayerParameter::Send(2) });
        let c = layer_param_ref(CursorLayerParameter { layer: Some(1), parameter: LayerParameter::Volume });
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(is_send(&a));
        assert!(!is_send(&c));
        assert!(is_send(&ParamRef::TrackSend { index: 0, send: 1 }));
        assert!(!is_send(&ParamRef::TrackVolume { index: 0 }));
    }
}
