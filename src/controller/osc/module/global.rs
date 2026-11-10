// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Undo and redo (former `GlobalModule`).

use super::super::address::GlobalIn;
use super::OscResult;
use crate::framework::ctx::Ctx;

/// Former `GlobalModule`.
#[derive(Default)]
pub struct GlobalModule;

impl GlobalModule {
    pub fn execute(&mut self, ctx: &mut Ctx, command: GlobalIn) -> OscResult {
        match command {
            GlobalIn::Undo {} => ctx.model.application_mut().undo(),
            GlobalIn::Redo {} => ctx.model.application_mut().redo(),
        }
        Ok(())
    }
}
