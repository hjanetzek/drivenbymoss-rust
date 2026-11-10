// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Executes the actions assigned in the configuration (former `ActionModule`): `/action/{1-20}`.

use super::super::address::ActionIn;
use super::OscResult;
use crate::config::osc::assignable_action;
use crate::framework::ctx::Ctx;

/// Former `ActionModule`.
#[derive(Default)]
pub struct ActionModule;

impl ActionModule {
    pub fn execute(&mut self, ctx: &mut Ctx, command: ActionIn) -> OscResult {
        let ActionIn::Action { action } = command;
        let action_id = assignable_action(&ctx.config, action);
        // Bitwig ignores an unknown (or empty) action ID
        if !action_id.is_empty() {
            ctx.model.application_mut().invoke_action(&action_id);
        }
        Ok(())
    }
}
