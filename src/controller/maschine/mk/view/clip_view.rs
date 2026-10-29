// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The clip view (former `ClipView`): the 16 clips of the selected track.

use super::{base_on_grid_note, pad_xy, ExecuteFunction};
use crate::controller::maschine::core::colors::{COLOR_GREEN, COLOR_GREEN_LO, COLOR_RED, COLOR_RED_LO};
use crate::daw::bank::BankItem;
use crate::daw::clip::SessionAlternative;
use crate::daw::color::DawColor;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::ids::ButtonId;
use crate::framework::view::session_view::SessionViewBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Launch the 16 clips of the selected track with the pads.
#[derive(Default)]
pub struct ClipView {
    was_alternate_interaction_used: bool,
}

impl ClipView {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::default()
    }
}

impl ExecuteFunction for ClipView {
    fn execute_function(&mut self, ctx: &mut Ctx, pad_index: i32, event: ButtonEvent) {
        let is_down = event == ButtonEvent::Down;
        let Some(track_index) = ctx.model.current_track_bank().selected_index() else {
            return;
        };
        let slot_index = pad_index as usize;
        let is_shift = ctx.surface.is_shift_pressed();

        if is_down {
            if ctx.is_button_combination(ButtonId::Duplicate) {
                if ctx.model.current_track_bank().item(track_index).does_exist() {
                    ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).duplicate();
                }
                return;
            }
            if ctx.is_button_combination(ButtonId::Clip) {
                ctx.model.current_track_bank_mut().item_mut(track_index).stop(is_shift);
                return;
            }
            if ctx.is_button_combination(ButtonId::Browse) {
                // Browse for a clip (only with a host browser, Reaper has none for slots)
                let track = ctx.model.current_track_bank().item(track_index);
                if track.does_exist() {
                    let slot = track.slot_bank().item(slot_index);
                    let (name, track, position) = (slot.name().to_string(), slot.track_index(), slot.position());
                    if let Some(browser) = ctx.model.browser_mut() {
                        browser.replace_slot(&name, track, position);
                    }
                }
                return;
            }
            if ctx.is_button_combination(ButtonId::Delete) {
                ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).remove();
                return;
            }
            if ctx.is_button_combination(ButtonId::Select) {
                ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).select();
                return;
            }
            if ctx.config.is_select_clip_on_launch() {
                ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).select();
            }
        }

        let track = ctx.model.current_track_bank().item(track_index);
        if !track.is_rec_arm() || track.slot_bank().item(slot_index).has_content() {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).launch(is_down, is_shift);
            return;
        }

        if is_down {
            SessionViewBase::handle_recording(ctx, track_index, slot_index);
        }
    }
}

impl SessionAlternative for ClipView {
    fn was_alternate_interaction_used(&self) -> bool {
        self.was_alternate_interaction_used
    }

    fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.was_alternate_interaction_used = was_used;
    }
}

impl View for ClipView {
    fn name(&self) -> &str {
        "Clip"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let Ctx { surface, colors, model, .. } = ctx;
        let Some(selected_track) = model.current_track_bank().selected_item() else {
            return;
        };
        let grid = surface.grid();
        let slot_bank = selected_track.slot_bank();
        for i in 0..16 {
            let item = slot_bank.item(i);
            let (x, y) = pad_xy(i as i32);
            if item.does_exist() {
                if item.is_recording_queued() {
                    grid.light_ex(x, y, COLOR_RED_LO);
                } else if item.is_recording() {
                    grid.light_ex(x, y, COLOR_RED);
                } else if item.is_playing_queued() {
                    grid.light_ex(x, y, COLOR_GREEN_LO);
                } else if item.is_playing() {
                    grid.light_ex(x, y, COLOR_GREEN);
                } else if item.is_stop_queued() {
                    grid.light_ex(x, y, COLOR_GREEN_LO);
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

    fn session_alternative(&self) -> Option<&dyn SessionAlternative> {
        Some(self)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn SessionAlternative> {
        Some(self)
    }

    impl_as_any!();
}
