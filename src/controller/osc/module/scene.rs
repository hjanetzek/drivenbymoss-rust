// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The scenes (former `SceneModule`).

use super::{index_of, is_trigger, match_color, parse_int_or_throw, send_color, sub_command, to_string, OscError, OscModule, OscResult, OscShared, Path, Value};
use super::{TAG_COLOR, TAG_DUPLICATE, TAG_EXISTS, TAG_NAME, TAG_REMOVE, TAG_SELECT, TAG_SELECTED};
use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;

/// Former `SceneModule`.
#[derive(Default)]
pub struct SceneModule;

impl OscModule for SceneModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["scene"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        if command != "scene" {
            return Err(OscError::UnknownCommand(command.to_string()));
        }
        let scene_command = sub_command(path)?;
        match scene_command.as_str() {
            "bank" => {
                let sub = sub_command(path)?;
                match sub.as_str() {
                    "+" => {
                        if is_trigger(value) {
                            ctx.model.current_track_bank_mut().scene_bank_mut().select_next_page();
                        }
                    }
                    "-" => {
                        if is_trigger(value) {
                            ctx.model.current_track_bank_mut().scene_bank_mut().select_previous_page();
                        }
                    }
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            "+" => {
                if is_trigger(value) {
                    ctx.model.current_track_bank_mut().scene_bank_mut().scroll_forwards();
                }
            }
            "-" => {
                if is_trigger(value) {
                    ctx.model.current_track_bank_mut().scene_bank_mut().scroll_backwards();
                }
            }
            "create" => {
                if is_trigger(value) {
                    ctx.model.project_mut().create_scene_from_playing_launcher_clips();
                }
            }
            "add" => {
                if is_trigger(value) {
                    let length = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
                    ctx.model.project_mut().create_scene(length);
                }
            }
            _ => {
                // Java: Integer.parseInt, its NumberFormatException is not caught
                let index = index_of(parse_int_or_throw(&scene_command)?)?;
                let sub = sub_command(path)?;
                let mut scene_bank = ctx.model.current_track_bank_mut().scene_bank_mut();
                let scene = scene_bank.item_mut(index);
                match sub.as_str() {
                    TAG_SELECT => {
                        if is_trigger(value) {
                            scene.select();
                        }
                    }
                    // Java launches the scene not alternatively with launchAlt as well
                    "launch" | "launchAlt" => scene.launch(is_trigger(value), false),
                    TAG_DUPLICATE => scene.duplicate(),
                    TAG_REMOVE => scene.remove(),
                    TAG_NAME => {
                        if let Some(name) = value.text() {
                            if scene.does_exist() {
                                scene.set_name(&name);
                            }
                        }
                    }
                    TAG_COLOR => {
                        if let Some(color) = match_color(&to_string(value)?) {
                            if scene.does_exist() {
                                scene.set_color(color);
                            }
                        }
                    }
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let scene_bank = ctx.model.scene_bank();
        for i in 0..scene_bank.page_size() {
            let scene = scene_bank.item(i);
            let address = format!("/scene/{}/", i + 1);
            shared.writer.send_bool(&format!("{address}{TAG_EXISTS}"), scene.does_exist(), dump);
            shared.writer.send_str(&format!("{address}{TAG_NAME}"), &scene.name(), dump);
            shared.writer.send_bool(&format!("{address}{TAG_SELECTED}"), scene.is_selected(), dump);
            send_color(&mut shared.writer, &format!("{address}{TAG_COLOR}"), scene.color(), dump);
        }
    }
}
