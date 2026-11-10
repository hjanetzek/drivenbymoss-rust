// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cue markers (former `MarkerModule`).

use super::super::address::{Dir, MarkerIn, MarkerKeys};
use super::{OscResult, OscShared};
use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::KeyList;

/// Former `MarkerModule`.
pub struct MarkerModule {
    markers: KeyList<MarkerKeys>,
}

impl MarkerModule {
    pub fn new() -> Self {
        MarkerModule { markers: KeyList::new("/marker/".into()) }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: MarkerIn) -> OscResult {
        match command {
            MarkerIn::Launch { marker } => ctx.model.marker_bank_mut().item_mut(marker).launch(true),
            MarkerIn::Bank { dir } => {
                let bank = ctx.model.marker_bank_mut();
                match dir {
                    Dir::Next => bank.select_next_page(),
                    Dir::Previous => bank.select_previous_page(),
                }
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let bank = ctx.model.marker_bank();
        for i in 0..bank.page_size() {
            let k = self.markers.at(w, i);
            let marker = bank.item(i);
            w.put(k.exists, marker.does_exist(), dump);
            w.put(k.name, &marker.name(), dump);
            w.put(k.color, marker.color(), dump);
        }
    }
}

impl Default for MarkerModule {
    fn default() -> Self {
        Self::new()
    }
}
