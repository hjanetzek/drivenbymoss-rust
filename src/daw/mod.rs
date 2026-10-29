// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The DAW model: tracks, devices, clips, transport, etc. (former `framework.daw` interfaces
//! merged with their Reaper implementations).

pub mod application;
pub mod bank;
pub mod browser;
pub mod clip;
pub mod channel;
pub mod color;
pub mod constants;
pub mod context;
pub mod device;
pub mod events;
pub mod groove;
pub mod model;
pub mod model_setup;
pub mod parameter;
pub mod marker;
pub mod parameter_bank;
pub mod project;
pub mod scene;
pub mod slot;
pub mod track;
pub mod track_bank;
pub mod transport;
pub mod util;
