// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the faders (former `launchcontrol.mode.faders`).

use crate::daw::constants::DeviceId;
use crate::daw::device::SpecificDevice;
use crate::daw::events::ParameterBankId;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ContinuousId;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, DrumDeviceRef, FixedParameterProvider, LayerOrDrumPadParameterProvider, ParamRef};

/// Edit the gains of the equalizer bands (former `XLEqGainMode`).
pub struct XlEqGainMode {
    base: ParameterModeBase,
}

impl XlEqGainMode {
    pub fn new(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let mut base = ParameterModeBase::new("EQ Gain", true, ModeBank::ParameterBank(ParameterBankId::CursorDevice), controls);
        let parameters = (0..8).map(|i| ParamRef::BankItem { bank: ParameterBankId::Eq, index: SpecificDevice::gain_parameter_index(i) }).collect();
        base.set_parameter_provider(ctx, provider(FixedParameterProvider::new(parameters)));
        XlEqGainMode { base }
    }
}

impl Mode for XlEqGainMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        if !ctx.model.specific_device(DeviceId::Eq).is_some_and(|d| d.does_exist()) {
            return -1;
        }
        self.base.knob_value(ctx, index)
    }

    crate::impl_as_any!();
}

/// Edit the volumes of the layers (drum pads) of the first instrument (former
/// `XLLayerVolumeMode`).
pub struct XlLayerVolumeMode {
    base: ParameterModeBase,
}

impl XlLayerVolumeMode {
    pub fn new(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        // Reaper always has drum pads, therefore the bank is the drum pad bank (former
        // parametersAdjusted)
        let mut base = ParameterModeBase::new("Layer Volume", true, ModeBank::DrumPadBank(DrumDeviceRef::FirstInstrument), controls);
        base.set_parameter_provider(ctx, provider(LayerOrDrumPadParameterProvider::volume(ctx)));
        XlLayerVolumeMode { base }
    }
}

impl Mode for XlLayerVolumeMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    crate::impl_as_any!();
}

/// The fader mode of the drum and note sequencer templates. In Java the note edit modes are
/// registered in the main and in the fader mode manager; their faders are not bound to
/// parameters but to `KnobRowModeCommand`s which call the main mode. Since a mode cannot be
/// active (borrowed) in two managers at the same time in Rust, the fader manager uses this
/// mode, which does nothing (it does not bind the faders).
pub struct XlNoteEditFaderMode {
    name: String,
}

impl XlNoteEditFaderMode {
    pub fn new(name: &str) -> Self {
        XlNoteEditFaderMode { name: name.to_string() }
    }
}

impl Mode for XlNoteEditFaderMode {
    fn name(&self) -> &str {
        &self.name
    }

    crate::impl_as_any!();
}
