// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device modes of the Push (former `mode.device`).

pub mod browser;
pub mod layer;
pub mod layer_details;
pub mod params;
pub mod user;

use crate::framework::ctx::Ctx;
use crate::framework::ids::Modes;

/// Register the device modes: DEVICE_PARAMS, DEVICE_CHAINS, DEVICE_LAYER(_VOLUME/_PAN/_SEND1-8/_DETAILS), BROWSER, USER.
pub fn register_device_modes(ctx: &mut Ctx) {
    let params = params::DeviceParamsMode::new(ctx, false);
    ctx.register_mode(Modes::DeviceParams, params);
    let chains = params::DeviceParamsMode::new(ctx, true);
    ctx.register_mode(Modes::DeviceChains, chains);
    let layer = layer::DeviceLayerMode::new(ctx, layer::LayerKind::Layer);
    ctx.register_mode(Modes::DeviceLayer, layer);
    let volume = layer::DeviceLayerMode::new(ctx, layer::LayerKind::Volume);
    ctx.register_mode(Modes::DeviceLayerVolume, volume);
    let pan = layer::DeviceLayerMode::new(ctx, layer::LayerKind::Pan);
    ctx.register_mode(Modes::DeviceLayerPan, pan);
    for i in 0..8 {
        let send = layer::DeviceLayerMode::new(ctx, layer::LayerKind::Send(i));
        ctx.register_mode(Modes::get(Modes::DeviceLayerSend1, i), send);
    }
    ctx.register_mode(Modes::DeviceLayerDetails, layer_details::DeviceLayerDetailsMode::new());
    ctx.register_mode(Modes::Browser, browser::DeviceBrowserMode::new());
    let user = user::UserMode::new(ctx);
    ctx.register_mode(Modes::User, user);
}
