// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The program change view of the Push (former `PrgChangeView`): the pads send program changes
//! (0-63, 64-127 when toggled), the scene buttons select the bank (CC 32).

use crate::controller::push::colors::*;
use crate::controller::push::surface;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::impl_as_any;
use crate::midi::{ButtonEvent, CMD_CC, CMD_PROGRAM_CHANGE};

/// The program change view.
pub struct PrgChangeView {
    greens: [i32; 8],
    yellows: [i32; 8],
    bank_number: i32,
    program_number: i32,
    is_toggled: bool,
}

impl PrgChangeView {
    pub fn new(ctx: &Ctx) -> Self {
        let is_push2 = surface::is_modern(ctx);
        let pick = |p2: i32, p1: i32| if is_push2 { p2 } else { p1 };
        let green_hi = pick(PUSH2_COLOR2_GREEN_HI, PUSH1_COLOR2_GREEN_HI);
        let green = pick(PUSH2_COLOR2_GREEN, PUSH1_COLOR2_GREEN);
        let green_lo = pick(PUSH2_COLOR2_GREEN_LO, PUSH1_COLOR2_GREEN_LO);
        let green_spring = pick(PUSH2_COLOR2_GREEN_SPRING, PUSH1_COLOR2_GREEN_SPRING);
        let yellow_hi = pick(PUSH2_COLOR2_YELLOW_HI, PUSH1_COLOR2_YELLOW_HI);
        let yellow = pick(PUSH2_COLOR2_YELLOW, PUSH1_COLOR2_YELLOW);
        let yellow_lo = pick(PUSH2_COLOR2_YELLOW_LO, PUSH1_COLOR2_YELLOW_LO);
        let yellow_lime = pick(PUSH2_COLOR2_YELLOW_LIME, PUSH1_COLOR2_YELLOW_LIME);
        PrgChangeView {
            greens: [green_hi, green, green_lo, green_spring, green_hi, green, green_lo, green_spring],
            yellows: [yellow_hi, yellow, yellow_lo, yellow_lime, yellow_hi, yellow, yellow_lo, yellow_lime],
            bank_number: 0,
            program_number: -1,
            is_toggled: false,
        }
    }
}

impl View for PrgChangeView {
    fn name(&self) -> &str {
        "PrgChnge"
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down {
            return;
        }
        let new_bank = index as i32;
        if new_bank == self.bank_number {
            self.is_toggled = !self.is_toggled;
        } else {
            self.bank_number = new_bank;
            self.is_toggled = false;
            ctx.surface.send_midi_event(CMD_CC, 32, self.bank_number);
            // Forces the bank change
            if self.program_number != -1 {
                ctx.surface.send_midi_event(CMD_PROGRAM_CHANGE, self.program_number, 0);
            }
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let is_push2 = surface::is_modern(ctx);
        let black = if is_push2 { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };
        match scene_index(button) {
            Some(scene) if scene as i32 == self.bank_number => {
                if self.is_toggled {
                    if is_push2 {
                        PUSH2_COLOR_SCENE_YELLOW
                    } else {
                        PUSH1_COLOR_SCENE_YELLOW
                    }
                } else if is_push2 {
                    PUSH2_COLOR_SCENE_GREEN
                } else {
                    PUSH1_COLOR_SCENE_GREEN
                }
            }
            _ => black,
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let colors = if self.is_toggled { &self.yellows } else { &self.greens };
        let sel_pad = if self.is_toggled {
            if self.program_number >= 64 {
                self.program_number - 64
            } else {
                -1
            }
        } else if self.program_number < 64 {
            self.program_number
        } else {
            -1
        };
        let red = if surface::is_modern(ctx) { PUSH2_COLOR2_RED } else { PUSH1_COLOR2_RED };
        let grid = ctx.surface.grid();
        for i in 36..100 {
            let pad = i - 36;
            let row = (pad / 8) as usize;
            grid.light(i, if sel_pad == pad { red } else { colors[row] });
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        self.program_number = note - 36 + if self.is_toggled { 64 } else { 0 };
        ctx.surface.send_midi_event(CMD_PROGRAM_CHANGE, self.program_number, 0);
    }

    impl_as_any!();
}
