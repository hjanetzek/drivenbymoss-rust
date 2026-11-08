// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The groove / shuffle settings (former `IGroove` / `GrooveImpl`). Only the enabled state and
//! the shuffle amount are supported by Reaper. Bitwig has six parameters (GrooveImpl of the Java
//! version for Bitwig), which exist once the host reports them (`/groove/<name>`).

use crate::protocol::GrooveParam;

use super::constants::GrooveParameterId;
use super::context::DawContext;
use super::parameter::{Parameter, ParameterKind};

/// The names of the groove parameters of the host by [`GrooveParameterId`] (the order of
/// GrooveImpl: enabled, shuffle amount, shuffle rate, accent amount, accent rate, accent phase).
pub const HOST_GROOVE_PARAMETERS: [&str; 6] = ["enabled", "shuffleAmount", "shuffleRate", "accentAmount", "accentRate", "accentPhase"];

/// The groove.
pub struct Groove {
    enabled: Parameter,
    shuffle_amount: Parameter,
    empty: Parameter,
    /// The parameters of the host's groove (Bitwig), in the order of [`HOST_GROOVE_PARAMETERS`];
    /// used once the host reports one of them.
    host: Vec<Parameter>,
    host_reported: bool,
}

fn host_index(id: GrooveParameterId) -> usize {
    match id {
        GrooveParameterId::Enabled => 0,
        GrooveParameterId::ShuffleAmount => 1,
        GrooveParameterId::ShuffleRate => 2,
        GrooveParameterId::AccentAmount => 3,
        GrooveParameterId::AccentRate => 4,
        GrooveParameterId::AccentPhase => 5,
    }
}

impl Groove {
    pub fn new(ctx: DawContext) -> Self {
        Groove {
            enabled: Parameter::new(ctx.clone(), ParameterKind::Groove(GrooveParameterId::Enabled), 0, 0.0),
            shuffle_amount: Parameter::new(ctx.clone(), ParameterKind::Groove(GrooveParameterId::ShuffleAmount), 1, 0.0),
            empty: Parameter::empty(&ctx),
            host: GrooveParam::ALL.iter().enumerate().map(|(i, param)| Parameter::new(ctx.clone(), ParameterKind::HostGroove(*param), i as i32, 0.0)).collect(),
            host_reported: false,
        }
    }

    /// The host's groove parameter with the given name, for the parser (Bitwig). From now on
    /// the host's parameters are used.
    pub fn host_parameter_mut(&mut self, name: &str) -> Option<&mut Parameter> {
        let index = HOST_GROOVE_PARAMETERS.iter().position(|n| *n == name)?;
        self.host_reported = true;
        Some(&mut self.host[index])
    }

    /// Get a groove parameter. Unsupported parameters return an empty parameter.
    pub fn parameter(&self, id: GrooveParameterId) -> &Parameter {
        if self.host_reported {
            return &self.host[host_index(id)];
        }
        match id {
            GrooveParameterId::Enabled => &self.enabled,
            GrooveParameterId::ShuffleAmount => &self.shuffle_amount,
            _ => &self.empty,
        }
    }

    /// Get a groove parameter for modification. Unsupported parameters return an empty parameter.
    pub fn parameter_mut(&mut self, id: GrooveParameterId) -> &mut Parameter {
        if self.host_reported {
            return &mut self.host[host_index(id)];
        }
        match id {
            GrooveParameterId::Enabled => &mut self.enabled,
            GrooveParameterId::ShuffleAmount => &mut self.shuffle_amount,
            _ => &mut self.empty,
        }
    }

    /// GrooveImpl.setIndication: all parameters (Bitwig).
    pub fn set_indication(&mut self, enable: bool) {
        if self.host_reported {
            for parameter in &mut self.host {
                parameter.set_indication(enable);
            }
        }
    }
}
