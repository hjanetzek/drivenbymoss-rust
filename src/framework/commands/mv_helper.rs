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

/// Display the name of the selected device and its parameter page (former
/// `notifySelectedDeviceAndParameterPage`).
pub fn notify_selected_device_and_parameter_page(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| {
        let device = ctx.model.cursor_device();
        if !device.does_exist() {
            return "No device selected".to_string();
        }
        let mut text = device.name().to_string();
        if let Some(bank) = device.parameter_bank() {
            let page_name = bank.page_bank().selected_item();
            text = format!("{text} - {}", if page_name.trim().is_empty() { NONE } else { &page_name });
        }
        text
    });
}

/// Display the range of the selected sends of the track at the index of the page of the
/// current track bank (former `notifySelectedSends (sendBank)`).
pub fn notify_selected_sends(ctx: &mut Ctx, track_index: usize) {
    delay_display(ctx, move |ctx| {
        let scroll_position = ctx.model.current_track_bank().item(track_index).send_bank().scroll_position() + 1;
        format!("Sends: {}-{}", scroll_position, scroll_position + 1)
    });
}

/// Display the range of the selected sends of the drum pad (layer) at the index of the page of
/// the first instrument (former `notifySelectedSends (sendBank)` for layers).
pub fn notify_selected_layer_sends(ctx: &mut Ctx, layer_index: usize) {
    delay_display(ctx, move |ctx| {
        let scroll_position = ctx
            .model
            .specific_device(crate::daw::constants::DeviceId::FirstInstrument)
            .map_or(0, |d| d.drum_pad_bank().item(layer_index).send_bank().scroll_position())
            + 1;
        format!("Sends: {}-{}", scroll_position, scroll_position + 1)
    });
}

/// Display the selected layer of the cursor device (former `notifySelectedLayer`). Reaper has
/// no device layers, therefore nothing is displayed.
pub fn notify_selected_layer(ctx: &mut Ctx) {
    let device = ctx.model.cursor_device();
    if !device.does_exist() || !device.has_layers() {
        return;
    }
    delay_display(ctx, |_| format!("Selected layer: {NONE}"));
}

/// Display the range of the scene page (former `notifyScenePage`).
pub fn notify_scene_page(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| {
        let scene_bank = ctx.model.scene_bank();
        let page_size = scene_bank.page_size();
        let Some(last_scene) = (0..page_size).rev().find(|i| scene_bank.item(*i).does_exist()) else {
            return "No scenes".to_string();
        };
        let first = scene_bank.item(0);
        let first_text = format!("{}: {}", first.position() + 1, first.name());
        if last_scene == 0 {
            return first_text;
        }
        let last = scene_bank.item(last_scene);
        format!("{first_text} - {}: {}", last.position() + 1, last.name())
    });
}

/// Display the edit page of the clip with the given key (former `notifyEditPage (clip)`).
pub fn notify_edit_page(ctx: &mut Ctx, clip: crate::framework::mode::note_editor::ClipKey) {
    let exists = ctx.model.note_clip_ref(clip.0, clip.1).is_some_and(|c| c.does_exist());
    if exists {
        delay_display(ctx, move |ctx| format!("Edit page: {}", ctx.model.note_clip_ref(clip.0, clip.1).map_or(0, |c| c.edit_page()) + 1));
    }
}

/// Display the name of the scale (former `notifyScale`).
pub fn notify_scale(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| format!("Scale: {}", ctx.model.scales().scale().name()));
}

fn notify_parameter_page_of(ctx: &mut Ctx, bank: crate::daw::events::ParameterBankId, prefix: &'static str) {
    delay_display(ctx, move |ctx| {
        let name = crate::framework::parameterprovider::parameter_bank(&ctx.model, bank).map(|b| b.page_bank().selected_item()).unwrap_or_default();
        format!("{prefix}Page: {}", if name.trim().is_empty() { NONE } else { &name })
    });
}

/// Former `notifySelectedProjectParameterPage`.
pub fn notify_selected_project_parameter_page(ctx: &mut Ctx) {
    notify_parameter_page_of(ctx, crate::daw::events::ParameterBankId::Project, "Project ");
}

/// Former `notifySelectedTrackParameterPage`.
pub fn notify_selected_track_parameter_page(ctx: &mut Ctx) {
    notify_parameter_page_of(ctx, crate::daw::events::ParameterBankId::CursorTrack, "Track ");
}

/// Display the name of the send with the given index of the selected (or first) track (former
/// `notifySelectedSend`).
pub fn notify_selected_send(ctx: &mut Ctx, send_index: usize) {
    delay_display(ctx, move |ctx| {
        let bank = ctx.model.track_bank();
        let track = bank.selected_item().or_else(|| Some(bank.item(0)).filter(|t| t.does_exist()));
        let name = match track {
            Some(track) => {
                let send = track.send_bank().item(send_index);
                if send.does_exist() {
                    send.name().to_string()
                } else {
                    "-".to_string()
                }
            }
            None => "-".to_string(),
        };
        format!("Send {}: {name}", send_index + 1)
    });
}
