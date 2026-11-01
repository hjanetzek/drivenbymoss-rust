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
    // AbstractParameterMode
    crate::forward_mode!(base => name, on_activate, on_deactivate, knob_value, select_item,
        select_previous_item, select_next_item, select_previous_item_page, select_next_item_page,
        has_previous_item, has_next_item, has_previous_item_page, has_next_item_page);

    crate::impl_as_any!();
}
