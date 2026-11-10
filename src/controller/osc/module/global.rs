// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Undo and redo (former `GlobalModule`).

use super::{OscError, OscModule, OscResult, OscShared, Path, Value};
use crate::framework::ctx::Ctx;

/// Former `GlobalModule`.
#[derive(Default)]
pub struct GlobalModule;

impl OscModule for GlobalModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["undo", "redo"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, _path: &mut Path, _value: Value) -> OscResult {
        match command {
            "undo" => ctx.model.application_mut().undo(),
            "redo" => ctx.model.application_mut().redo(),
            _ => return Err(OscError::UnknownCommand(command.to_string())),
        }
        Ok(())
    }
}
