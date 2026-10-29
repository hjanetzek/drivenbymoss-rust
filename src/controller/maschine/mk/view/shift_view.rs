// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The shift view (former `ShiftView`): undo/redo, quantize, clear and transpose.

use super::ExecuteFunction;
use crate::controller::maschine::core::colors::{COLOR_AMBER, COLOR_BLACK, COLOR_RED, COLOR_TURQUOISE, COLOR_YELLOW};
use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::maschine::mk::surface::{maschine, set_stop_consumed};
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::view::shift_view::ShiftViewBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The shift view.
pub struct ShiftView {
    base: ShiftViewBase,
}

impl ShiftView {
    pub fn new(_ctx: &Ctx) -> Self {
        ShiftView { base: ShiftViewBase::new("Shift") }
    }

    /// Signal that the view was used (e.g. the encoder was turned).
    pub fn set_was_used(&mut self) {
        self.base.set_was_used();
    }

    pub fn was_used(&self) -> bool {
        self.base.was_used()
    }
}

impl ExecuteFunction for ShiftView {
    fn execute_function(&mut self, ctx: &mut Ctx, pad_index: i32, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }

        set_stop_consumed(ctx);

        if pad_index == 0 || pad_index == 1 {
            if pad_index == 0 {
                ctx.model.application_mut().undo();
                ctx.notify("Undo");
            } else {
                ctx.model.application_mut().redo();
                ctx.notify("Redo");
            }
        }

        let Some(clip) = ctx.model.cursor_clip() else {
            return;
        };
        if !clip.does_exist() {
            return;
        }
        let message = match pad_index {
            4 => {
                clip.quantize(1.0);
                "Quantize: 100%"
            }
            5 => {
                clip.quantize(0.5);
                "Quantize: 50%"
            }
            8 => {
                clip.clear_all();
                "Clear all notes"
            }
            12 => {
                clip.transpose(-1);
                "Transpose: -1"
            }
            13 => {
                clip.transpose(1);
                "Transpose: 1"
            }
            14 => {
                clip.transpose(-12);
                "Transpose: -12"
            }
            15 => {
                clip.transpose(12);
                "Transpose: 12"
            }
            _ => return,
        };
        ctx.notify(message);
    }
}

impl View for ShiftView {
    fn name(&self) -> &str {
        "Shift"
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        if maschine(ctx) == Maschine::Mk3 {
            ctx.surface.set_knob_sensitivity_is_slow(true);
        }
        self.base.on_activate();
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        if maschine(ctx) == Maschine::Mk3 {
            ctx.surface.set_knob_sensitivity_is_slow(false);
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let exists = ctx.model.cursor_clip().is_some_and(|clip| clip.does_exist());
        let on = |color: i32| if exists { color } else { COLOR_BLACK };
        let grid = ctx.surface.grid();
        let rows = [
            [on(COLOR_TURQUOISE), on(COLOR_TURQUOISE), on(COLOR_TURQUOISE), on(COLOR_TURQUOISE)],
            [on(COLOR_AMBER), COLOR_BLACK, COLOR_BLACK, COLOR_BLACK],
            [on(COLOR_YELLOW), on(COLOR_YELLOW), COLOR_BLACK, COLOR_BLACK],
            [COLOR_RED, COLOR_YELLOW, COLOR_BLACK, COLOR_BLACK],
        ];
        for (y, row) in rows.iter().enumerate() {
            for (x, color) in row.iter().enumerate() {
                grid.light_ex(x as i32, y as i32, *color);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        self.base.set_was_used();
        self.execute_function(ctx, note - 36, ButtonEvent::Down);
    }

    fn was_shift_view_used(&self) -> bool {
        self.base.was_used()
    }

    impl_as_any!();
}
