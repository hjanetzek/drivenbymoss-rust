// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The scenes (former `SceneModule`).

use super::super::address::{Dir, SceneCmd, SceneIn, SceneKeys};
use super::{is_trigger, match_color, to_string, OscResult, OscShared, Value};
use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::KeyList;

/// Former `SceneModule`.
pub struct SceneModule {
    scenes: KeyList<SceneKeys>,
}

impl SceneModule {
    pub fn new() -> Self {
        SceneModule { scenes: KeyList::new("/scene/".into()) }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: SceneIn, value: Value) -> OscResult {
        match command {
            SceneIn::BankPage { dir } => {
                if is_trigger(value) {
                    let mut scene_bank = ctx.model.current_track_bank_mut().scene_bank_mut();
                    match dir {
                        Dir::Next => scene_bank.select_next_page(),
                        Dir::Previous => scene_bank.select_previous_page(),
                    }
                }
            }
            SceneIn::Scroll { dir } => {
                if is_trigger(value) {
                    let mut scene_bank = ctx.model.current_track_bank_mut().scene_bank_mut();
                    match dir {
                        Dir::Next => scene_bank.scroll_forwards(),
                        Dir::Previous => scene_bank.scroll_backwards(),
                    }
                }
            }
            SceneIn::Create {} => {
                if is_trigger(value) {
                    ctx.model.project_mut().create_scene_from_playing_launcher_clips();
                }
            }
            SceneIn::Add {} => {
                if is_trigger(value) {
                    let length = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
                    ctx.model.project_mut().create_scene(length);
                }
            }
            SceneIn::Scene { scene: index, cmd } => {
                let mut scene_bank = ctx.model.current_track_bank_mut().scene_bank_mut();
                let scene = scene_bank.item_mut(index);
                match cmd {
                    SceneCmd::Select => {
                        if is_trigger(value) {
                            scene.select();
                        }
                    }
                    // Java launches the scene not alternatively with launchAlt as well
                    SceneCmd::Launch | SceneCmd::LaunchAlt => scene.launch(is_trigger(value), false),
                    SceneCmd::Duplicate => scene.duplicate(),
                    SceneCmd::Remove => scene.remove(),
                    SceneCmd::Name => {
                        if let Some(name) = value.text() {
                            if scene.does_exist() {
                                scene.set_name(&name);
                            }
                        }
                    }
                    SceneCmd::Color => {
                        if let Some(color) = match_color(&to_string(value)?) {
                            if scene.does_exist() {
                                scene.set_color(color);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let scene_bank = ctx.model.scene_bank();
        for i in 0..scene_bank.page_size() {
            let scene = scene_bank.item(i);
            let k = self.scenes.at(w, i);
            w.put(k.exists, scene.does_exist(), dump);
            w.put(k.name, &scene.name(), dump);
            w.put(k.selected, scene.is_selected(), dump);
            w.put(k.color, scene.color(), dump);
        }
    }
}

impl Default for SceneModule {
    fn default() -> Self {
        Self::new()
    }
}
