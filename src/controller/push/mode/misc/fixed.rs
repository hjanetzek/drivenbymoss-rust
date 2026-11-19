// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The fixed clip length mode (former `FixedMode`).

use super::{forward_base, restore_mode_later, row_index};
use crate::config::values::NEW_CLIP_LENGTH_VALUES;
use crate::controller::push::display1::SELECT_ARROW;
use crate::controller::push::mode::PushDisplayMode;
use crate::framework::commands::clip::NewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_HI, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// First row: select the length of new clips. Second row: create a clip with the length (the
/// length is not stored).
pub struct FixedMode {
    base: ParameterModeBase,
}

impl FixedMode {
    pub fn new() -> Self {
        FixedMode { base: super::base("Fixed") }
    }
}

impl Default for FixedMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for FixedMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_block(1, 0, "Create Clip (leng").set_block(1, 1, "th not stored):");
        let new_clip_length = ctx.config.new_clip_length();
        display.set_block(2, 0, "New Clip Length:");
        for (i, value) in NEW_CLIP_LENGTH_VALUES.iter().enumerate() {
            display.set_cell(0, i, value);
            let arrow = if new_clip_length == i { SELECT_ARROW.to_string() } else { String::new() };
            display.set_cell(3, i, &format!("{arrow}{value}"));
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let new_clip_length = ctx.config.new_clip_length();
        for (i, value) in NEW_CLIP_LENGTH_VALUES.iter().enumerate() {
            display.add_option_element(if i == 0 { "Create Clip (length not stored)" } else { "" }, value, false, if i == 0 { "New Clip Length" } else { "" }, value, new_clip_length == i, false);
        }
    }
}

impl Mode for FixedMode {
    forward_base!();

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if row == 0 {
            ctx.config.set_new_clip_length(index);
        } else {
            NewCommand::with_fixed_length(index as i32).execute_now(ctx);
        }
        restore_mode_later(ctx);
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            return if ctx.config.new_clip_length() == index { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
        }
        BUTTON_COLOR_ON
    }

    impl_as_any!();
}
