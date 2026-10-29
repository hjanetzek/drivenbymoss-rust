// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Delayed notifications about the state of the model (the parts of the former `MVHelper` which
//! are used by the ported commands).

use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;

const SELECTED_TRACK_NONE: &str = "Selected track: None";
const NONE: &str = "None";
/// The delay after which the notification is shown (the model needs to be updated first).
pub const DISPLAY_DELAY: u64 = 200;

/// Notify a text after 200ms. Empty texts are not shown.
pub fn delay_display(ctx: &mut Ctx, supplier: impl FnOnce(&Ctx) -> String + 'static) {
    ctx.schedule(DISPLAY_DELAY, move |ctx| {
        let message = supplier(ctx);
        if !message.trim().is_empty() {
            ctx.notify(&message);
        }
    });
}

/// Display the name of the selected track.
pub fn notify_selected_track(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| match ctx.model.current_track_bank().selected_item() {
        Some(track) if track.does_exist() => format!("{}: {}", track.position() + 1, track.name()),
        _ => SELECTED_TRACK_NONE.to_string(),
    });
}

/// Display the name of the selected device.
pub fn notify_selected_device(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| {
        let device = ctx.model.cursor_device();
        format!("Selected device: {}", if device.does_exist() { device.name() } else { NONE })
    });
}

/// Display the name of the selected parameter page of the cursor device.
pub fn notify_selected_parameter_page(ctx: &mut Ctx) {
    if !ctx.model.cursor_device().does_exist() {
        return;
    }
    delay_display(ctx, |ctx| {
        let name = ctx.model.cursor_device().parameter_bank().map(|bank| bank.page_bank().selected_item()).unwrap_or_default();
        format!("Page: {}", if name.trim().is_empty() { NONE } else { &name })
    });
}

/// Display the name of the selected item of the active mode.
pub fn notify_selected_item_of_active_mode(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| {
        ctx.active_mode().and_then(|mode| mode.try_borrow().ok().and_then(|m| m.selected_item_name(ctx))).unwrap_or_else(|| NONE.to_string())
    });
}
