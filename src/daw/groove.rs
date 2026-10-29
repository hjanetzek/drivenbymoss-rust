// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The groove / shuffle settings (former `IGroove` / `GrooveImpl`). Only the enabled state and
//! the shuffle amount are supported by Reaper.

use super::constants::GrooveParameterId;
use super::context::DawContext;
use super::parameter::{Parameter, ParameterKind};

/// The groove.
pub struct Groove {
    enabled: Parameter,
    shuffle_amount: Parameter,
    empty: Parameter,
}

impl Groove {
    pub fn new(ctx: DawContext) -> Self {
        Groove {
            enabled: Parameter::new(ctx.clone(), ParameterKind::Groove(GrooveParameterId::Enabled), 0, 0.0),
            shuffle_amount: Parameter::new(ctx.clone(), ParameterKind::Groove(GrooveParameterId::ShuffleAmount), 1, 0.0),
            empty: Parameter::empty(&ctx),
        }
    }

    /// Get a groove parameter. Unsupported parameters return an empty parameter.
    pub fn parameter(&self, id: GrooveParameterId) -> &Parameter {
        match id {
            GrooveParameterId::Enabled => &self.enabled,
            GrooveParameterId::ShuffleAmount => &self.shuffle_amount,
            _ => &self.empty,
        }
    }

    /// Get a groove parameter for modification. Unsupported parameters return an empty parameter.
    pub fn parameter_mut(&mut self, id: GrooveParameterId) -> &mut Parameter {
        match id {
            GrooveParameterId::Enabled => &mut self.enabled,
            GrooveParameterId::ShuffleAmount => &mut self.shuffle_amount,
            _ => &mut self.empty,
        }
    }

    pub fn set_indication(&mut self, _enable: bool) {}
}
