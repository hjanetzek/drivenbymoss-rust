// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The view to select the fixed accent velocity (former `AccentView`).

use super::{as_maschine_view, MaschineView};
use crate::controller::maschine::core::colors::{COLOR_BLACK, COLOR_GREEN, COLOR_LIME_LO};
use crate::controller::maschine::core::encoder_mode_manager::EncoderMode;
use crate::daw::clip::SessionAlternative;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::Views;
use crate::impl_as_any;

const BLOCK_SIZE: i32 = 4;

pub struct AccentView {
    name: String,
}

impl AccentView {
    pub fn new(_ctx: &Ctx) -> Self {
        AccentView { name: "Accent".to_string() }
    }
}

impl View for AccentView {
    fn name(&self) -> &str {
        &self.name
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let fixed_accent_value = ctx.config.fixed_accent_value();
        let grid = ctx.surface.grid();
        for i in (36..40).chain(44..48).chain(52..56).chain(60..64).chain(68..100) {
            grid.light(i, COLOR_BLACK);
        }

        let mut selected_pad = 15 - fixed_accent_value / 8;
        let sel_y = selected_pad / 4;
        let sel_x = selected_pad % 4;
        selected_pad = sel_y * 4 + 3 - sel_x;

        for pad in 0..16 {
            let x = BLOCK_SIZE + pad % BLOCK_SIZE;
            let y = BLOCK_SIZE + pad / BLOCK_SIZE;
            grid.light_ex(x, y, if pad == selected_pad { COLOR_GREEN } else { COLOR_LIME_LO });
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let index = note - 36;
        let x = index % 8;
        let y = index / 8;
        if x < 4 || y >= 4 {
            return;
        }
        let pad = (BLOCK_SIZE - 1 - y) * BLOCK_SIZE + x - BLOCK_SIZE;
        let sel_pad = (3 - pad / 4) * 4 + pad % 4;
        ctx.config.set_fixed_accent_value((sel_pad + 1) * 8 - 1);
    }

    fn session_alternative(&self) -> Option<&dyn SessionAlternative> {
        Some(self)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn SessionAlternative> {
        Some(self)
    }

    impl_as_any!();
}

impl SessionAlternative for AccentView {
    fn was_alternate_interaction_used(&self) -> bool {
        false
    }

    fn set_alternate_interaction_used(&mut self, _was_used: bool) {}
}

impl MaschineView for AccentView {
    /// Forward the option change to the view below the temporary accent view.
    fn change_option(&mut self, ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32) {
        let Some(active_id) = ctx.views.active_id_ignore_temporary() else {
            return;
        };
        if active_id == Views::Control {
            return;
        }
        let Some(view) = ctx.views.get(active_id) else {
            return;
        };
        let Ok(mut view) = view.try_borrow_mut() else {
            return;
        };
        if let Some(maschine_view) = as_maschine_view(&mut *view) {
            maschine_view.change_option(ctx, temporary_encoder_mode, control);
        }
    }
}
