// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A mode which does nothing (former `DummyMode`). With controls, the controls are bound to
//! empty parameters which blocks their commands.

use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ContinuousId;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, EmptyParameterProvider};

/// A mode which does nothing (former `DummyMode`).
pub struct DummyMode {
    pub base: ParameterModeBase,
}

impl DummyMode {
    pub fn new(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let size = controls.len();
        let mut base = ParameterModeBase::new("Dummy", true, ModeBank::None, controls);
        if size > 0 {
            base.set_parameter_provider(ctx, provider(EmptyParameterProvider::new(size)));
        }
        DummyMode { base }
    }
}

impl Mode for DummyMode {
    // AbstractParameterMode
    crate::forward_mode!(base => name, on_activate, on_deactivate, knob_value);

    crate::impl_as_any!();
}
