// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Scenes (former `IScene`/`SceneImpl` and `ISceneBank`/`SceneBankImpl`). In Reaper scenes are
//! defined by regions.

use std::ops::{Deref, DerefMut};

use crate::backend::Processor;
use crate::protocol::{Cmd, Rgb, SceneAction};

use super::bank::{BankItem, PagedBank};
use super::color::ColorEx;
use super::context::DawContext;
use super::events::ModelEvent;
use super::util;

/// A scene.
pub struct Scene {
    ctx: DawContext,
    index: i32,
    position: i32,
    exists: bool,
    selected: bool,
    name: String,
    color: ColorEx,
    begin_position: f64,
    end_position: f64,
}

impl Scene {
    pub fn new(ctx: DawContext, index: i32, position: i32) -> Self {
        Scene {
            ctx,
            index,
            position,
            exists: false,
            selected: false,
            name: String::new(),
            color: ColorEx::new(0.2, 0.2, 0.2),
            begin_position: 0.0,
            end_position: 0.0,
        }
    }

    pub fn empty(ctx: &DawContext) -> Self {
        let mut scene = Scene::new(ctx.muted(), -1, -1);
        scene.color = ColorEx::BLACK;
        scene
    }

    /// The name of the scene; "Scene N" if the name is blank.
    pub fn name(&self) -> String {
        if !self.exists {
            return String::new();
        }
        if self.name.trim().is_empty() {
            format!("Scene {}", self.position + 1)
        } else {
            self.name.clone()
        }
    }

    pub fn name_limited(&self, limit: usize) -> String {
        util::optimize_name(&self.name(), limit)
    }

    pub fn color(&self) -> ColorEx {
        self.color
    }

    pub fn begin_position(&self) -> f64 {
        self.begin_position
    }

    pub fn end_position(&self) -> f64 {
        self.end_position
    }

    fn send(&self, action: SceneAction) {
        self.ctx.send(Cmd::SceneAction { scene: self.position, action });
    }

    /// Rename the scene.
    pub fn set_name(&mut self, name: &str) {
        self.ctx.send(Cmd::SceneName { scene: self.position, name });
    }

    /// Set the color of the scene.
    pub fn set_color(&mut self, color: ColorEx) {
        self.ctx.send(Cmd::SceneColor { scene: self.position, value: Rgb(color.to_int_rgb255()) });
    }

    /// Launch the scene on press and release, normally or alternatively (as `SceneImpl.launch` of
    /// the Java version). Reaper only uses the press.
    pub fn launch(&mut self, is_pressed: bool, is_alternative: bool) {
        self.send(match (is_pressed, is_alternative) {
            (true, false) => SceneAction::Launch,
            (true, true) => SceneAction::LaunchAlt,
            (false, false) => SceneAction::LaunchRelease,
            (false, true) => SceneAction::LaunchReleaseAlt,
        });
    }

    pub fn remove(&mut self) {
        self.send(SceneAction::Remove);
    }

    pub fn duplicate(&mut self) {
        self.send(SceneAction::Duplicate);
    }

    // Internal setters

    pub fn set_exists(&mut self, exists: bool) {
        self.exists = exists;
        self.ctx.update_scene_range(self.position, |r| r.0 = exists);
    }

    pub fn set_position(&mut self, position: i32) {
        if position >= 0 {
            self.position = position;
        }
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_color_state(&mut self, color: ColorEx) {
        self.color = color;
    }

    pub fn set_begin_position(&mut self, begin: f64) {
        self.begin_position = begin;
        self.ctx.update_scene_range(self.position, |r| r.1 = begin);
    }

    pub fn set_end_position(&mut self, end: f64) {
        self.end_position = end;
        self.ctx.update_scene_range(self.position, |r| r.2 = end);
    }
}

impl BankItem for Scene {
    fn does_exist(&self) -> bool {
        self.exists
    }

    fn is_selected(&self) -> bool {
        self.exists && self.selected
    }

    fn index(&self) -> i32 {
        self.index
    }

    fn position(&self) -> i32 {
        self.position
    }

    fn select(&mut self) {
        self.send(SceneAction::Select);
    }
}

/// A bank of scenes. Derefs (mutably) to [`PagedBank<Scene>`].
pub struct SceneBank {
    bank: PagedBank<Scene>,
}

impl SceneBank {
    /// The host reported the offset of its scene window (see `PagedBank::enable_host_paging`).
    pub(crate) fn enable_host_paging(&mut self) {
        self.bank.enable_host_paging();
    }

    pub fn new(ctx: DawContext, num_scenes: usize) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let mut bank = PagedBank::new(
            ctx,
            num_scenes,
            move |position| {
                let index = if num_scenes == 0 { 0 } else { position % num_scenes };
                Scene::new(item_ctx.clone(), index as i32, position as i32)
            },
            move || Scene::empty(&empty_ctx),
        );
        bank.set_host_bank(Processor::Scene);
        bank.set_page_event(ModelEvent::SceneBankPageChanged);
        SceneBank { bank }
    }

    pub fn enable_observers(&self, enable: bool) {
        self.bank.ctx.send(Cmd::enable_updates(Processor::Session, enable));
    }

    /// Scenes scroll by one (SceneBankImpl of the Java version for Reaper). With host paging
    /// (Bitwig) as AbstractItemBank.canScrollBackwards: a selected scene after the first one or
    /// a previous page.
    pub fn can_scroll_backwards(&self) -> bool {
        if self.bank.ctx.is_host_paging() {
            return self.bank.can_scroll_backwards();
        }
        self.bank.bank_offset > 0
    }

    /// Scenes scroll by one (Reaper). With host paging as AbstractItemBank.canScrollForwards: an
    /// existing scene after the selected one on the page or a next page.
    pub fn can_scroll_forwards(&self) -> bool {
        if self.bank.ctx.is_host_paging() {
            return self.bank.can_scroll_forwards();
        }
        self.bank.bank_offset + 1 < self.bank.item_count
    }

    /// Scroll so that the scene at the given position is the first one of the page.
    pub fn scroll_to(&mut self, position: usize, _adjust_page: bool) {
        if position < self.bank.item_count {
            self.bank.set_bank_offset(position as i64);
        }
    }

    /// Add a new scene (former `IProject.createScene`).
    pub fn create_scene(&self, length_in_beats: i32) {
        self.bank.ctx.send(Cmd::ProjectCreateScene { value: length_in_beats });
    }
}

impl Deref for SceneBank {
    type Target = PagedBank<Scene>;
    fn deref(&self) -> &PagedBank<Scene> {
        &self.bank
    }
}

impl DerefMut for SceneBank {
    fn deref_mut(&mut self) -> &mut PagedBank<Scene> {
        &mut self.bank
    }
}
