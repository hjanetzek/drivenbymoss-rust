// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! View switches which are requested by a view itself. While a function of a view is executed,
//! the view is borrowed and cannot be deactivated. Therefore these switches are executed by a
//! task which runs right after the current event was handled.

use crate::framework::ctx::Ctx;
use crate::framework::ids::Views;

/// Activate a view (and store it as the preferred view of the cursor track) after the current
/// view function has finished.
pub fn activate_preferred_view_later(ctx: &mut Ctx, view: Views) {
    ctx.schedule(0, move |ctx| ctx.activate_preferred_view(view));
}

/// Restore the previous view after the current view function has finished.
pub fn restore_view_later(ctx: &mut Ctx) {
    ctx.schedule(0, |ctx| ctx.restore_view());
}
