// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The track select view (former `SelectView`).

use super::{base_on_grid_note, pad_xy, ExecuteFunction};
use crate::controller::maschine::core::colors::COLOR_WHITE;
use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::framework::commands::mv_helper::notify_selected_track;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::ids::ButtonId;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Select the tracks with the pads.
pub struct SelectView;

impl SelectView {
    pub fn new(_ctx: &Ctx) -> Self {
        SelectView
    }
}

impl ExecuteFunction for SelectView {
    fn execute_function(&mut self, ctx: &mut Ctx, pad_index: i32, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let index = pad_index as usize;
        if ctx.is_button_combination(ButtonId::Duplicate) {
            ctx.model.current_track_bank_mut().item_mut(index).duplicate();
            return;
        }
        if ctx.is_button_combination(ButtonId::Delete) {
            ctx.model.current_track_bank_mut().item_mut(index).remove();
            return;
        }
        ctx.model.current_track_bank_mut().item_mut(index).select_or_expand_group();
        notify_selected_track(ctx);
    }
}

impl View for SelectView {
    fn name(&self) -> &str {
        "Track Select"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let Ctx { surface, colors, model, .. } = ctx;
        let grid = surface.grid();
        let tb = model.current_track_bank();
        for i in 0..16 {
            let item = tb.item(i);
            let (x, y) = pad_xy(i as i32);
            if item.does_exist() {
                let color_index = colors.color_index(DawColor::color_id(item.color()));
                if item.is_selected() {
                    grid.light_ex_blink(x, y, color_index, COLOR_WHITE, false);
                } else {
                    grid.light_ex(x, y, color_index);
                }
            } else {
                grid.light_ex_id(colors, x, y, BUTTON_COLOR_OFF);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        base_on_grid_note(self, ctx, note, velocity);
    }

    impl_as_any!();
}
