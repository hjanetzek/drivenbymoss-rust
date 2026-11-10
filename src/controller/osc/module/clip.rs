// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cursor clip and the clip slots of the cursor track (former `ClipModule`).

use super::super::address::{ClipIn, Dir};
use super::{is_trigger, match_color, to_integer, to_string, OscError, OscResult, OscShared, Value};
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::{Key, OscWriter};

/// Former `ClipModule`.
pub struct ClipModule {
    exists: Key<bool>,
    pinned: Key<bool>,
    color: Key<ColorEx>,
}

/// The selected slot of the cursor track (the address table checks that there is one).
fn selected_slot(ctx: &Ctx) -> Option<usize> {
    ctx.model.cursor_track().slot_bank().selected_item().and_then(|s| usize::try_from(s.index()).ok())
}

impl ClipModule {
    pub fn new(w: &mut OscWriter) -> Self {
        ClipModule { exists: w.key("/clip/exists"), pinned: w.key("/clip/pinned"), color: w.key("/clip/color") }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: ClipIn, value: Value) -> OscResult {
        match command {
            ClipIn::StopAll { alt } => ctx.model.track_bank_mut().stop(alt),
            ClipIn::Pinned {} => {
                if let Some(clip) = ctx.model.cursor_clip_mut() {
                    if value.is_null() {
                        clip.toggle_pinned();
                    } else {
                        clip.set_pinned(is_trigger(value));
                    }
                }
            }
            ClipIn::Quantize {} => {
                if let Some(clip) = ctx.model.cursor_clip().filter(|c| c.does_exist()) {
                    clip.quantize(1.0);
                }
            }
            ClipIn::Name {} => {
                if let Some(name) = value.text() {
                    if let Some(clip) = ctx.model.cursor_clip().filter(|c| c.does_exist()) {
                        clip.set_name(&name);
                    }
                }
                return slot_commands_after_cursor_clip(ctx);
            }
            ClipIn::Color {} => {
                if let Some(color) = match_color(&to_string(value)?) {
                    if let Some(clip) = ctx.model.cursor_clip().filter(|c| c.does_exist()) {
                        clip.set_color(color);
                    }
                }
                return slot_commands_after_cursor_clip(ctx);
            }
            ClipIn::Stop { alt } => ctx.model.cursor_track_mut().stop(alt),
            // The selection stays in the current page (see clip_launcher_navigator::select_clip)
            ClipIn::SelectAdjacent { dir } => crate::framework::clip_launcher_navigator::select_clip(ctx, dir == Dir::Previous),
            ClipIn::Launch { alt } => {
                let pressed = to_integer(value)? > 0;
                if let Some(slot) = selected_slot(ctx) {
                    ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot).launch(pressed, alt);
                }
            }
            ClipIn::Record {} => {
                if let (Some(slot), Some(track)) = (selected_slot(ctx), ctx.model.cursor_track_ref()) {
                    ctx.model.record_note_clip(track, slot);
                }
            }
            ClipIn::Create {} => {
                let length = to_integer(value)?;
                if let (Some(slot), Some(track)) = (selected_slot(ctx), ctx.model.cursor_track_ref()) {
                    ctx.model.create_note_clip(track, slot, length, true);
                }
            }
            ClipIn::InsertFile {} => {
                let file = to_string(value)?;
                if let Some(slot) = selected_slot(ctx) {
                    ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot).insert_file(&file);
                }
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let clip = ctx.model.cursor_clip();
        w.put(self.exists, clip.is_some_and(|c| c.does_exist()), dump);
        w.put(self.pinned, clip.is_some_and(|c| c.is_pinned()), dump);
        w.put(self.color, clip.map(|c| c.color()).unwrap_or(ColorEx::BLACK), dump);
    }
}

/// Java continues with the slot commands after the name and the color of the cursor clip, and
/// ends with an unknown command if the cursor track has a selected slot.
fn slot_commands_after_cursor_clip(ctx: &Ctx) -> OscResult {
    if ctx.model.cursor_track().does_exist() && selected_slot(ctx).is_some() {
        return Err(OscError::UnknownCommand("clip".into()));
    }
    Ok(())
}
