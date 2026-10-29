// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The shift view of the Maschine JAM (former `ShiftView`).

use crate::controller::maschine::core::colors::*;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::view::shift_view::ShiftViewBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The shift view: undo/redo, quantize and transpose on the top row.
pub struct ShiftView {
    base: ShiftViewBase,
}

impl ShiftView {
    pub fn new(_ctx: &Ctx) -> Self {
        ShiftView { base: ShiftViewBase::new("Shift") }
    }
}

impl View for ShiftView {
    fn name(&self) -> &str {
        &self.base.base.name
    }

    fn on_activate(&mut self, _ctx: &mut Ctx) {
        self.base.on_activate();
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let grid = ctx.surface.grid();
        for i in 36..92 {
            grid.light(i, COLOR_BLACK);
        }
        grid.light(92, COLOR_RED_LO);
        grid.light(93, COLOR_GREEN_LO);
        grid.light(94, COLOR_LIME_LO);
        grid.light(95, COLOR_LIME_LO);
        grid.light(96, COLOR_MAGENTA_LO);
        grid.light(97, COLOR_MAGENTA_LO);
        grid.light(98, COLOR_ROSE);
        grid.light(99, COLOR_ROSE);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        self.base.set_was_used();

        if note == 92 {
            ctx.model.application_mut().undo();
            return;
        }
        if note == 93 {
            ctx.model.application_mut().redo();
            return;
        }
        let Some(clip) = ctx.model.cursor_clip() else {
            return;
        };
        if !clip.does_exist() {
            return;
        }
        match note {
            94 => clip.quantize(1.0),
            95 => clip.quantize(0.5),
            96 => clip.transpose(-1),
            97 => clip.transpose(1),
            98 => clip.transpose(-12),
            99 => clip.transpose(12),
            _ => {}
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let flip_session = ctx.config.is_flip_session();
        if event != ButtonEvent::Down {
            return;
        }
        let track_bank_page = |ctx: &mut Ctx, is_forwards: bool| {
            let track_bank = ctx.model.current_track_bank_mut();
            if is_forwards {
                track_bank.select_next_page();
            } else {
                track_bank.select_previous_page();
            }
        };
        let scene_bank_page = |ctx: &mut Ctx, is_forwards: bool| {
            let mut scene_bank = ctx.model.current_track_bank_mut().scene_bank_mut();
            if is_forwards {
                scene_bank.select_next_page();
            } else {
                scene_bank.select_previous_page();
            }
        };
        match button {
            ButtonId::ArrowLeft | ButtonId::ArrowRight => {
                if flip_session {
                    scene_bank_page(ctx, button == ButtonId::ArrowRight);
                } else {
                    track_bank_page(ctx, button == ButtonId::ArrowRight);
                }
            }
            ButtonId::ArrowUp | ButtonId::ArrowDown => {
                if flip_session {
                    track_bank_page(ctx, button == ButtonId::ArrowDown);
                } else {
                    scene_bank_page(ctx, button == ButtonId::ArrowDown);
                }
            }
            _ => {}
        }
    }

    fn was_shift_view_used(&self) -> bool {
        self.base.was_used()
    }

    impl_as_any!();
}
