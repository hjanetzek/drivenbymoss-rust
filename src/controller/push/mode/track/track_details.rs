// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The track details mode (former `TrackDetailsMode`).

use crate::daw::constants::Capability;
use crate::daw::track::Track;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::{button_row_index, default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::view::color_select_mode::ColorSelectMode;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::surface as push;
use super::super::{update_display, PushDisplayMode};
use super::{color, host_supports, select_color_view};

/// The track details mode (former `TrackDetailsMode`).
pub struct TrackDetailsMode {
    pub base: ParameterModeBase,
}

impl Default for TrackDetailsMode {
    fn default() -> Self {
        Self::new()
    }
}

/// Former `getTrackTitle`.
fn track_title(track: &Track) -> String {
    format!("{} Track: ", track.channel_type().label())
}

impl TrackDetailsMode {
    pub fn new() -> Self {
        TrackDetailsMode { base: ParameterModeBase::new("Track details", true, ModeBank::TrackBank, default_knob_ids()) }
    }

    fn change_midi_edit_channel(ctx: &mut Ctx, delta: i32) {
        let channel = ctx.config.midi_edit_channel();
        ctx.config.set_midi_edit_channel(channel + delta);
    }

    fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let is_master = ctx.model.master_track().is_selected();
        if !is_master && ctx.model.current_track_bank().selected_item().is_none() {
            return;
        }
        if index == 7 {
            select_color_view(ctx, ColorSelectMode::Track);
            return;
        }
        if index == 6 {
            // Not used for the master track
            if !is_master {
                ctx.model.toggle_cursor_track_pinned();
            }
            return;
        }
        let track = if is_master {
            ctx.model.master_track_mut()
        } else {
            match ctx.model.current_track_bank_mut().selected_item_mut() {
                Some(track) => track,
                None => return,
            }
        };
        match index {
            0 => track.toggle_is_activated(),
            1 => track.toggle_rec_arm(),
            2 => track.toggle_mute(),
            3 => track.toggle_solo(),
            4 => track.toggle_monitor(),
            5 => track.toggle_auto_monitor(),
            _ => {}
        }
    }

    fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match index {
            6 => Self::change_midi_edit_channel(ctx, -1),
            7 => Self::change_midi_edit_channel(ctx, 1),
            _ => {}
        }
    }
}

impl PushDisplayMode for TrackDetailsMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let cursor_track = ctx.model.cursor_track();
        if !cursor_track.does_exist() {
            display.set_row(1, "                     Please selecta track...                        ");
            return;
        }

        let first_block = format!("{}{}", track_title(cursor_track), cursor_track.name());
        display.set_block(0, 0, &first_block);
        if first_block.chars().count() > 17 {
            let rest: String = first_block.chars().skip(17).collect();
            display.set_block(0, 1, rest.trim());
        }

        let on_off = |b: bool| if b { "On" } else { "Off" };
        display.set_cell(2, 0, "Active").set_cell(3, 0, on_off(cursor_track.is_activated()));
        display.set_cell(2, 1, "Rec Arm");
        display.set_cell(3, 1, on_off(cursor_track.is_rec_arm()));
        display.set_cell(2, 2, "Mute").set_cell(3, 2, on_off(cursor_track.is_mute()));
        display.set_cell(2, 3, "Solo").set_cell(3, 3, on_off(cursor_track.is_solo()));
        display.set_cell(2, 4, "Monitor");
        display.set_cell(3, 4, on_off(cursor_track.is_monitor()));
        display.set_cell(2, 5, "Auto Monitor");
        display.set_cell(3, 5, on_off(cursor_track.is_auto_monitor()));
        if host_supports(ctx, Capability::HasPinning) {
            display.set_cell(2, 6, "Pin Trck");
            display.set_cell(3, 6, on_off(ctx.model.is_cursor_track_pinned()));
        }
        display.set_cell(2, 7, "Select").set_cell(3, 7, "Color");

        display.set_cell(0, 5, "Midi Ins");
        display.set_block(0, 3, &format!("/Edit Channel: {}", ctx.config.midi_edit_channel() + 1));
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let cursor_track = ctx.model.cursor_track();
        if !cursor_track.does_exist() {
            display.set_message(3, "Please select a track...");
            return;
        }

        display.add_option_element(&format!("{}{}", track_title(cursor_track), cursor_track.name()), "", false, "", "Active", cursor_track.is_activated(), false);
        display.add_option_element("", "", false, "", "Rec Arm", cursor_track.is_rec_arm(), false);
        display.add_option_element("", "", false, "", "Mute", cursor_track.is_mute(), false);
        display.add_option_element("", "", false, "", "Solo", cursor_track.is_solo(), false);
        display.add_option_element("", "", false, "", "Monitor", cursor_track.is_monitor(), false);
        display.add_option_element("Midi Insert/Edit Channel:", "", false, "", "Auto Monitor", cursor_track.is_auto_monitor(), false);
        if host_supports(ctx, Capability::HasPinning) {
            display.add_option_element("", "", false, "", "Pin Track", ctx.model.is_cursor_track_pinned(), false);
        } else {
            display.add_empty_element();
        }
        display.add_option_element(&format!("        {}", ctx.config.midi_edit_channel() + 1), "", false, "", "Select Color", false, false);
    }
}

impl Mode for TrackDetailsMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let is_modern = push::is_modern(ctx);
        if let Some(index) = button_row_index(0, button) {
            let cursor_track = ctx.model.cursor_track();
            if !cursor_track.does_exist() {
                return color(ctx, BUTTON_COLOR_OFF);
            }
            return match index {
                0 => color(ctx, if cursor_track.is_activated() { PUSH_YELLOW_MD } else { PUSH_YELLOW_LO }),
                1 => color(ctx, if cursor_track.is_rec_arm() { PUSH_RED_HI } else { PUSH_RED_LO }),
                2 => color(ctx, if cursor_track.is_mute() { PUSH_ORANGE_HI } else { PUSH_ORANGE_LO }),
                3 => color(ctx, if cursor_track.is_solo() { PUSH_ORANGE_HI } else { PUSH_ORANGE_LO }),
                4 => color(ctx, if cursor_track.is_monitor() { PUSH_GREEN_HI } else { PUSH_GREEN_LO }),
                5 => color(ctx, if cursor_track.is_auto_monitor() { PUSH_GREEN_HI } else { PUSH_GREEN_LO }),
                6 => {
                    if !host_supports(ctx, Capability::HasPinning) {
                        return if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };
                    }
                    color(ctx, if ctx.model.is_cursor_track_pinned() { PUSH_GREEN_HI } else { PUSH_GREEN_LO })
                }
                _ => {
                    if is_modern {
                        PUSH2_COLOR_GREEN_HI
                    } else {
                        PUSH1_COLOR_GREEN_HI
                    }
                }
            };
        }

        if let Some(index) = button_row_index(1, button) {
            return color(ctx, if index < 6 { BUTTON_COLOR_OFF } else { BUTTON_COLOR_ON });
        }

        color(ctx, BUTTON_COLOR_OFF)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if index < 6 {
            return;
        }
        let speed = if ctx.model.value_changer().is_increase(value) { 1 } else { -1 };
        Self::change_midi_edit_channel(ctx, speed);
    }

    fn on_knob_touch(&mut self, _ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item_page(ctx);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item_page(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item_page(ctx)
    }

    impl_as_any!();
}
