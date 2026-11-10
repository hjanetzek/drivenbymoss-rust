// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Executes the actions assigned in the configuration (former `ActionModule`): `/action/{1-20}`.

use super::{parse_int, sub_command, OscError, OscModule, OscResult, OscShared, Path, Value};
use crate::config::osc::{assignable_action, NUM_ASSIGNABLE_ACTIONS};
use crate::framework::ctx::Ctx;

/// Former `ActionModule`.
#[derive(Default)]
pub struct ActionModule;

impl OscModule for ActionModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["action"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, _value: Value) -> OscResult {
        if command != "action" {
            return Err(OscError::UnknownCommand(command.to_string()));
        }
        let sub = sub_command(path)?;
        let Some(number) = parse_int(&sub) else {
            return Err(OscError::UnknownCommand(sub));
        };
        let action_no = (i64::from(number) - 1).clamp(0, NUM_ASSIGNABLE_ACTIONS as i64 - 1) as usize;
        let action_id = assignable_action(&ctx.config, action_no);
        // Bitwig ignores an unknown (or empty) action ID
        if !action_id.is_empty() {
            ctx.model.application_mut().invoke_action(&action_id);
        }
        Ok(())
    }
}
