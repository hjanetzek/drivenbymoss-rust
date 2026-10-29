// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note repeat mode (former `NoteRepeatMode`). Reaper supports the note repeat length and
//! mode but not the octaves.

use super::{with_display, BaseMode};
use crate::daw::constants::Resolution;
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ButtonId;
use crate::impl_as_any;
use crate::midi::ARP_MODES;

/// Edit the note repeat settings with the knobs.
pub struct NoteRepeatMode {
    base: BaseMode,
}

impl NoteRepeatMode {
    pub fn new(_ctx: &Ctx) -> Self {
        NoteRepeatMode { base: BaseMode::new(0) }
    }
}

impl Mode for NoteRepeatMode {
    fn name(&self) -> &str {
        "Note Repeat"
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let idx = self.base.index(index);
        let value_changer = ctx.model.value_changer();
        let is_increase = value_changer.is_increase(value);
        let speed = value_changer.calc_knob_change(value);
        match idx {
            0 | 1 => {
                let sel = Resolution::change(Resolution::get_match(ctx.config.note_repeat_period().value()), is_increase);
                ctx.config.set_note_repeat_period(Resolution::ALL[sel]);
            }
            2 | 3 => {
                let sel = Resolution::change(Resolution::get_match(ctx.config.note_repeat_length().value()), speed > 0.0);
                ctx.config.set_note_repeat_length(Resolution::ALL[sel]);
            }
            4 | 5 => ctx.config.set_prev_next_note_repeat_mode(is_increase),
            // Note repeat octaves (6, 7) are not supported by Reaper
            _ => {}
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if !is_touched || !ctx.surface.is_delete_pressed() {
            return;
        }
        ctx.surface.set_trigger_consumed(ButtonId::Delete);
        match index {
            0 | 1 => ctx.config.set_note_repeat_period(Resolution::ALL[4]),
            2 | 3 => ctx.config.set_note_repeat_length(Resolution::ALL[4]),
            4 | 5 => {
                let up = ARP_MODES.iter().position(|m| *m == "Up").unwrap_or(0);
                ctx.surface.input.default_note_input_mut().note_repeat.set_mode(up);
            }
            _ => {}
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
        let names: Vec<&str> = Resolution::ALL.iter().map(|r| r.name()).collect();
        let period_index = Resolution::get_match(note_repeat.period);
        let length_index = Resolution::get_match(note_repeat.note_length);
        let mode = ARP_MODES.get(note_repeat.mode).copied().unwrap_or(ARP_MODES[0]);
        let period = names.get(period_index).copied().unwrap_or(names[0]).to_string();
        let length = names.get(length_index).copied().unwrap_or(names[0]).to_string();
        let base = self.base;
        with_display(ctx, |d| {
            d.clear();
            d.set_block(0, 0, &base.mark("Period", 0));
            d.set_block(1, 0, &period);
            d.set_block(0, 1, &base.mark("Length", 2));
            d.set_block(1, 1, &length);
            d.set_block(0, 2, &base.mark("Mode", 4));
            d.set_block(1, 2, &optimize_name(mode, 12));
            d.all_done();
        });
    }

    fn select_previous_item(&mut self, _ctx: &mut Ctx) {
        self.base.select_previous_item();
        if self.base.selected_param % 2 == 1 {
            self.base.selected_param -= 1;
        }
    }

    fn select_next_item(&mut self, _ctx: &mut Ctx) {
        self.base.select_next_item();
        if self.base.selected_param % 2 == 1 {
            self.base.selected_param += 1;
        }
        if self.base.selected_param > 6 {
            self.base.selected_param = 6;
        }
    }

    fn select_item(&mut self, _ctx: &mut Ctx, index: usize) {
        self.base.select_item(index);
    }

    impl_as_any!();
}
