// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A view which displays one scene per pad (former `ScenePlayView`).

use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::framework::color_ids::abstract_sequencer_view::COLOR_RESOLUTION_OFF;
use crate::framework::color_ids::scene_play_view::COLOR_SELECTED_PLAY_SCENE;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::grid::GRID_OFF;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::{scene_index, ViewBase};
use crate::impl_as_any;

/// The name of the view.
pub const NAME_SCENE_PLAY: &str = "Scene Play";

/// Launches the scenes of a scene bank with the size of the pad grid.
pub struct ScenePlayView {
    base: ViewBase,
    num_scenes: usize,
}

impl ScenePlayView {
    /// Constructor. Creates the scene bank with the size of the pad grid.
    pub fn new(ctx: &mut Ctx) -> Self {
        let grid = ctx.surface.grid_ref();
        let num_scenes = (grid.rows() * grid.cols()) as usize;
        ctx.model.ensure_scene_bank(num_scenes);
        ScenePlayView { base: ViewBase::new(NAME_SCENE_PLAY), num_scenes }
    }

    fn with_scene_mut(&self, ctx: &mut Ctx, index: usize, f: impl FnOnce(&mut crate::daw::scene::Scene)) {
        if let Some(bank) = ctx.model.scene_bank_sized_mut(self.num_scenes) {
            f(bank.item_mut(index));
        } else {
            // The configured scene bank has the same size
            f(ctx.model.scene_bank_mut().item_mut(index));
        }
    }
}

impl View for ScenePlayView {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let grid = ctx.surface.grid_ref();
        let rows = grid.rows();
        let cols = grid.cols();
        for row in 0..rows {
            for col in 0..cols {
                let pad = (row * cols + col) as usize;
                let color_id = match ctx.model.scene_bank_sized(self.num_scenes) {
                    Some(bank) => {
                        let scene = bank.item(pad);
                        if scene.is_selected() {
                            COLOR_SELECTED_PLAY_SCENE
                        } else if scene.does_exist() {
                            DawColor::color_id(scene.color())
                        } else {
                            GRID_OFF
                        }
                    }
                    None => GRID_OFF,
                };
                let color = ctx.colors.color_index(color_id);
                ctx.surface.grid().light_ex(col, rows - row - 1, color);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let scene_index = (note - 36).max(0) as usize;
        let is_pressed = velocity > 0;
        if is_pressed {
            // Former handleSceneButtonCombinations
            if ctx.is_button_combination(ButtonId::Delete) {
                self.with_scene_mut(ctx, scene_index, |scene| scene.remove());
                return;
            }
            if ctx.is_button_combination(ButtonId::Duplicate) {
                self.with_scene_mut(ctx, scene_index, |scene| scene.duplicate());
                return;
            }
            self.with_scene_mut(ctx, scene_index, |scene| scene.select());
            let name = ctx.model.scene_bank_sized(self.num_scenes).map(|bank| bank.item(scene_index).name()).unwrap_or_default();
            ctx.notify(&name);
            // Only select the scene
            if ctx.surface.is_select_pressed() {
                return;
            }
        }
        let is_alternative = ctx.surface.is_shift_pressed();
        self.with_scene_mut(ctx, scene_index, |scene| scene.launch(is_pressed, is_alternative));
    }

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        if scene_index(button).is_some() {
            COLOR_RESOLUTION_OFF
        } else {
            BUTTON_COLOR_OFF
        }
    }

    impl_as_any!();
}
