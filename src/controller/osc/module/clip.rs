// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cursor clip and the clip slots of the cursor track (former `ClipModule`).

use super::{is_trigger, match_color, send_color, sub_command, to_integer, to_string, OscError, OscModule, OscResult, OscShared, Path, Value};
use super::{TAG_COLOR, TAG_INSERT_FILE, TAG_NAME};
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;

/// Former `ClipModule`.
#[derive(Default)]
pub struct ClipModule;

impl OscModule for ClipModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["clip"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        if command != "clip" {
            return Err(OscError::UnknownCommand(command.to_string()));
        }
        let sub = sub_command(path)?;
        if sub == "stopall" || sub == "stopallAlt" {
            ctx.model.track_bank_mut().stop(sub == "stopallAlt");
            return Ok(());
        }

        // Cursor clip related commands
        match sub.as_str() {
            "pinned" => {
                if let Some(clip) = ctx.model.cursor_clip_mut() {
                    if value.is_null() {
                        clip.toggle_pinned();
                    } else {
                        clip.set_pinned(is_trigger(value));
                    }
                }
                return Ok(());
            }
            "quantize" => {
                if let Some(clip) = ctx.model.cursor_clip().filter(|c| c.does_exist()) {
                    clip.quantize(1.0);
                }
                return Ok(());
            }
            // Java continues with the slot commands after the name and the color (and ends with
            // an unknown command if a slot is selected)
            TAG_NAME => {
                if let Some(name) = value.text() {
                    if let Some(clip) = ctx.model.cursor_clip().filter(|c| c.does_exist()) {
                        clip.set_name(&name);
                    }
                }
            }
            TAG_COLOR => {
                if let Some(color) = match_color(&to_string(value)?) {
                    if let Some(clip) = ctx.model.cursor_clip().filter(|c| c.does_exist()) {
                        clip.set_color(color);
                    }
                }
            }
            _ => {}
        }

        // Slot bank related commands
        if !ctx.model.cursor_track().does_exist() {
            return Ok(());
        }
        if sub == "stop" || sub == "stopAlt" {
            ctx.model.cursor_track_mut().stop(sub == "stopAlt");
            return Ok(());
        }
        match sub.as_str() {
            "+" => {
                // The selection stays in the current page (see clip_launcher_navigator::select_clip)
                crate::framework::clip_launcher_navigator::select_clip(ctx, false);
                return Ok(());
            }
            "-" => {
                crate::framework::clip_launcher_navigator::select_clip(ctx, true);
                return Ok(());
            }
            _ => {}
        }
        let Some(slot_index) = ctx.model.cursor_track().slot_bank().selected_item().map(|s| s.index()) else {
            return Ok(());
        };
        let Ok(slot_index) = usize::try_from(slot_index) else {
            return Ok(());
        };
        let track_ref = ctx.model.cursor_track_ref();
        match sub.as_str() {
            "launch" => {
                let pressed = to_integer(value)? > 0;
                ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index).launch(pressed, false);
            }
            "launchAlt" => {
                let pressed = to_integer(value)? > 0;
                ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index).launch(pressed, true);
            }
            "record" => {
                if let Some(track) = track_ref {
                    ctx.model.record_note_clip(track, slot_index);
                }
            }
            "create" => {
                let length = to_integer(value)?;
                if let Some(track) = track_ref {
                    ctx.model.create_note_clip(track, slot_index, length, true);
                }
            }
            TAG_INSERT_FILE => {
                let file = to_string(value)?;
                ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index).insert_file(&file);
            }
            _ => return Err(OscError::UnknownCommand(command.to_string())),
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let clip = ctx.model.cursor_clip();
        shared.writer.send_bool("/clip/exists", clip.is_some_and(|c| c.does_exist()), dump);
        shared.writer.send_bool("/clip/pinned", clip.is_some_and(|c| c.is_pinned()), dump);
        send_color(&mut shared.writer, "/clip/color", clip.map(|c| c.color()).unwrap_or(ColorEx::BLACK), dump);
    }
}
