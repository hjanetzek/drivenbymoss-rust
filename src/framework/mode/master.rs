// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Mode for the volumes of the effect tracks, the metronome and the master track (former
//! `MasterAndFXVolumeMode`). Reaper has no effect track bank, therefore the first controls are
//! bound to empty parameters.

use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ContinuousId;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, CombinedParameterProvider, EmptyParameterProvider, FixedParameterProvider, ParamRef};

/// The volumes of the FX tracks, the metronome and the master track (former
/// `MasterAndFXVolumeMode`).
pub struct MasterAndFXVolumeMode {
    pub base: ParameterModeBase,
}

impl MasterAndFXVolumeMode {
    /// At least 2 controls are required.
    pub fn new(ctx: &Ctx, is_absolute: bool, controls: Vec<ContinuousId>) -> Self {
        let num_fx_tracks = controls.len().saturating_sub(2);
        if controls.len() < 2 {
            ctx.host.error("Number of controls must be at least 2.");
        }
        let mut base = ParameterModeBase::new("FX and master volumes", is_absolute, ModeBank::TrackBank, controls);
        base.set_parameter_provider(
            ctx,
            provider(CombinedParameterProvider::new(vec![
                // N FX track volumes (there is no effect track bank in Reaper)
                provider(EmptyParameterProvider::new(num_fx_tracks)),
                // Metronome volume
                provider(FixedParameterProvider::single(ParamRef::MetronomeVolume)),
                // Master volume
                provider(FixedParameterProvider::single(ParamRef::MasterVolume)),
            ])),
        );
        MasterAndFXVolumeMode { base }
    }
}

impl Mode for MasterAndFXVolumeMode {
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
