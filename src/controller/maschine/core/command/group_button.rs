// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Command for the group buttons (former `GroupButtonCommand`).

use super::super::colors::{dim_or_highlight_color, COLOR_BLACK, COLOR_BLUE, COLOR_DARK_GREY, COLOR_GREEN, COLOR_RED, COLOR_WHITE, COLOR_YELLOW};
use super::super::mv::selected_or_first_track;
use crate::framework::commands::mv_helper::notify_selected_send;
use crate::daw::bank::BankItem;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes};
use crate::midi::ButtonEvent;

/// Command for the group buttons: selects a track or a send, sets the sequencer resolution or
/// toggles solo, mute and record arm of a track in combination with other buttons.
pub struct GroupButtonCommand {
    index: usize,
}

impl GroupButtonCommand {
    pub fn new(index: usize) -> Self {
        GroupButtonCommand { index }
    }

    /// Get the color of the button.
    pub fn button_color(ctx: &Ctx, index: usize) -> i32 {
        // Send selection
        if ctx.surface.is_pressed(ButtonId::Sends) {
            if let Some(track) = selected_or_first_track(ctx) {
                if track.send_bank().item(index).does_exist() {
                    return COLOR_WHITE;
                }
            }
            return COLOR_BLACK;
        }

        if ctx.surface.is_pressed(ButtonId::Groove) {
            return match ctx.active_view().and_then(|view| view.try_borrow().ok().and_then(|v| v.sequencer_resolution_index(ctx))) {
                Some(resolution) => {
                    if resolution == index {
                        COLOR_GREEN
                    } else {
                        COLOR_DARK_GREY
                    }
                }
                None => COLOR_BLACK,
            };
        }

        let track = ctx.model.current_track_bank().item(index);
        if !track.does_exist() {
            return COLOR_BLACK;
        }

        if ctx.surface.is_pressed(ButtonId::Solo) {
            return if track.is_solo() { COLOR_BLUE } else { COLOR_DARK_GREY };
        }
        if ctx.surface.is_pressed(ButtonId::Mute) {
            return if track.is_mute() { COLOR_YELLOW } else { COLOR_DARK_GREY };
        }
        if ctx.surface.is_pressed(ButtonId::Record) {
            return if track.is_rec_arm() { COLOR_RED } else { COLOR_DARK_GREY };
        }

        // Track selection
        dim_or_highlight_color(&ctx.colors, track.color(), track.is_selected())
    }
}

impl TriggerCommand for GroupButtonCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }

        // Send selection
        if ctx.surface.is_pressed(ButtonId::Sends) {
            ctx.surface.set_trigger_consumed(ButtonId::Sends);
            ctx.set_active_mode(Modes::get(Modes::Send1, self.index));
            notify_selected_send(ctx, self.index);
            return;
        }

        // Set sequencer resolution
        if ctx.surface.is_pressed(ButtonId::Groove) {
            if let Some(view) = ctx.active_view() {
                if let Ok(mut view) = view.try_borrow_mut() {
                    view.set_sequencer_resolution_index(ctx, self.index as i32);
                }
            }
            return;
        }

        let index = self.index;
        if !ctx.model.current_track_bank().item(index).does_exist() {
            return;
        }

        let combinations = [ButtonId::Solo, ButtonId::Mute, ButtonId::Record, ButtonId::Delete, ButtonId::Duplicate];
        for button in combinations {
            if !ctx.surface.is_pressed(button) {
                continue;
            }
            ctx.surface.set_trigger_consumed(button);
            let track = ctx.model.current_track_bank_mut().item_mut(index);
            match button {
                // Solo track
                ButtonId::Solo => track.toggle_solo(),
                // Mute track
                ButtonId::Mute => track.toggle_mute(),
                // Record arm the track
                ButtonId::Record => track.toggle_rec_arm(),
                // Delete the track
                ButtonId::Delete => track.remove(),
                // Duplicate the track
                _ => track.duplicate(),
            }
            return;
        }

        // Track selection or group expansion
        ctx.model.current_track_bank_mut().item_mut(index).select_or_expand_group();
    }
}
