// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Helpers of the former `MVHelper` which are only used by the Maschine controllers (the
//! notifications are in [`crate::framework::commands::mv_helper`]).

use crate::daw::bank::BankItem;
use crate::daw::track::Track;
use crate::framework::ctx::Ctx;

/// Get the selected track of the track bank or the first track if none is selected and it
/// exists.
pub fn selected_or_first_track(ctx: &Ctx) -> Option<&Track> {
    let track_bank = ctx.model.track_bank();
    match track_bank.selected_item() {
        Some(track) => Some(track),
        None => {
            let item = track_bank.item(0);
            if item.does_exist() {
                Some(item)
            } else {
                None
            }
        }
    }
}
