// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The scene view (former `SceneView`).

use super::{base_on_grid_note, pad_xy, ExecuteFunction};
use crate::controller::maschine::core::colors::COLOR_WHITE;
use crate::daw::bank::BankItem;
use crate::daw::clip::SessionAlternative;
use crate::daw::color::DawColor;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::ids::ButtonId;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Launch the 16 scenes with the pads.
#[derive(Default)]
pub struct SceneView {
    was_alternate_interaction_used: bool,
}

impl SceneView {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::default()
    }
}

impl ExecuteFunction for SceneView {
    fn execute_function(&mut self, ctx: &mut Ctx, pad_index: i32, event: ButtonEvent) {
        let is_down = event == ButtonEvent::Down;
        let index = pad_index as usize;
        let is_shift = ctx.surface.is_shift_pressed();
        if is_down {
            if ctx.is_button_combination(ButtonId::Scene1) {
                ctx.model.track_bank_mut().stop(is_shift);
                return;
            }
            if ctx.is_button_combination(ButtonId::Duplicate) {
                ctx.model.scene_bank_mut().item_mut(index).duplicate();
                return;
            }
            if ctx.is_button_combination(ButtonId::Delete) {
                ctx.model.scene_bank_mut().item_mut(index).remove();
                return;
            }
            if ctx.is_button_combination(ButtonId::Select) {
                ctx.model.scene_bank_mut().item_mut(index).select();
                return;
            }
            if ctx.config.is_select_clip_on_launch() {
                ctx.model.scene_bank_mut().item_mut(index).select();
            }
        }
        ctx.model.scene_bank_mut().item_mut(index).launch(is_down, is_shift);
    }
}

impl SessionAlternative for SceneView {
    fn was_alternate_interaction_used(&self) -> bool {
        self.was_alternate_interaction_used
    }

    fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.was_alternate_interaction_used = was_used;
    }
}

impl View for SceneView {
    fn name(&self) -> &str {
        "Scene"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let Ctx { surface, colors, model, .. } = ctx;
        let grid = surface.grid();
        let scene_bank = model.scene_bank();
        for i in 0..16 {
            let item = scene_bank.item(i);
            let (x, y) = pad_xy(i as i32);
            if item.does_exist() {
                if item.is_selected() {
                    grid.light_ex(x, y, COLOR_WHITE);
                } else {
                    grid.light_ex_id(colors, x, y, DawColor::color_id(item.color()));
                }
            } else {
                grid.light_ex_id(colors, x, y, BUTTON_COLOR_OFF);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        base_on_grid_note(self, ctx, note, velocity);
        if ctx.surface.is_shift_pressed() {
            self.was_alternate_interaction_used = true;
        }
    }

    crate::impl_session_alternative!();

    impl_as_any!();
}
