// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Device parameter providers (former `parameterprovider.device`: `BankParameterProvider`,
//! `VolumeLayerOrDrumPadParameterProvider`, `PanLayerOrDrumPadParameterProvider` and
//! `SendLayerOrDrumPadParameterProvider`).
//!
//! Reaper has no device layers but (virtual) drum pads for every device, therefore the layer or
//! drum pad providers always provide the drum pad parameters.

use super::{parameter_bank, DrumDeviceRef, ParamRef, ParameterProvider};
use crate::daw::color::ColorEx;
use crate::daw::events::ParameterBankId;
use crate::framework::ctx::Ctx;

/// Provides the parameters of the current page of a parameter bank (former
/// `BankParameterProvider`).
pub struct BankParameterProvider {
    bank: ParameterBankId,
    size: usize,
}

impl BankParameterProvider {
    /// The size is the page size of the bank (8 if the bank does not exist).
    pub fn new(ctx: &Ctx, bank: ParameterBankId) -> Self {
        let size = parameter_bank(&ctx.model, bank).map(|b| b.page_size()).unwrap_or(8);
        BankParameterProvider { bank, size }
    }

    pub fn bank(&self) -> ParameterBankId {
        self.bank
    }
}

impl ParameterProvider for BankParameterProvider {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, _ctx: &Ctx, index: usize) -> ParamRef {
        ParamRef::BankItem { bank: self.bank, index }
    }
}

/// Which parameter of a layer / drum pad is provided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerParameter {
    Volume,
    Pan,
    Send(usize),
}

/// Provides the volume, panorama or a send of the drum pads (layers) of a device (former
/// `VolumeLayerOrDrumPadParameterProvider`, `PanLayerOrDrumPadParameterProvider`,
/// `SendLayerOrDrumPadParameterProvider`).
pub struct LayerOrDrumPadParameterProvider {
    device: DrumDeviceRef,
    parameter: LayerParameter,
    size: usize,
}

impl LayerOrDrumPadParameterProvider {
    pub fn new(ctx: &Ctx, device: DrumDeviceRef, parameter: LayerParameter) -> Self {
        let size = super::drum_device(&ctx.model, device).map(|d| d.drum_pad_bank().page_size()).unwrap_or(8);
        LayerOrDrumPadParameterProvider { device, parameter, size }
    }

    /// Former `VolumeLayerOrDrumPadParameterProvider (firstInstrument)`.
    pub fn volume(ctx: &Ctx) -> Self {
        Self::new(ctx, DrumDeviceRef::FirstInstrument, LayerParameter::Volume)
    }

    /// Former `PanLayerOrDrumPadParameterProvider (firstInstrument)`.
    pub fn pan(ctx: &Ctx) -> Self {
        Self::new(ctx, DrumDeviceRef::FirstInstrument, LayerParameter::Pan)
    }

    /// Former `SendLayerOrDrumPadParameterProvider (firstInstrument, sendIndex)`.
    pub fn send(ctx: &Ctx, send_index: usize) -> Self {
        Self::new(ctx, DrumDeviceRef::FirstInstrument, LayerParameter::Send(send_index))
    }
}

impl ParameterProvider for LayerOrDrumPadParameterProvider {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, _ctx: &Ctx, index: usize) -> ParamRef {
        let device = self.device;
        match self.parameter {
            LayerParameter::Volume => ParamRef::DrumPadVolume { device, index },
            LayerParameter::Pan => ParamRef::DrumPadPan { device, index },
            LayerParameter::Send(send) => ParamRef::DrumPadSend { device, index, send },
        }
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        super::drum_device(&ctx.model, self.device).map(|d| d.drum_pad_bank().item(index).color())
    }
}
