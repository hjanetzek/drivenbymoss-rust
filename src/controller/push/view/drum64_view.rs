// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum view with 64 pads of the Push (former `Drum64View`), an expression view on the
//! Push 3.

use super::{on_scene_button_with_repeat, scene_button_color_id, select_drum_pad};
use crate::daw::bank::BankItem;
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::is_scene_button;
use crate::framework::view::drum64_view::Drum64ViewBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The drum view with 64 pads.
pub struct Drum64View {
    pub drum: Drum64ViewBase,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl Default for Drum64View {
    fn default() -> Self {
        Self::new()
    }
}

impl Drum64View {
    pub fn new() -> Self {
        Drum64View { drum: Drum64ViewBase::new(8, 8), aftertouch: Box::new(AftertouchViewCommand::new()) }
    }

    /// Former `handleButtonCombinations` (with the overrides of the Push: Browse, Delete and
    /// Select).
    fn handle_button_combinations(&mut self, ctx: &mut Ctx, played_pad: i32) {
        if ctx.is_button_combination(ButtonId::Browse) {
            let primary = ctx.model.drum_device_sized(64).or_else(|| ctx.model.drum_device());
            if let Some(primary) = primary {
                if primary.has_drum_pads() {
                    let bank = primary.drum_pad_bank();
                    let pad = bank.item(played_pad.max(0) as usize);
                    let (owner, name, position) = (bank.owner(), pad.name().to_string(), pad.position());
                    if let Some(browser) = ctx.model.browser_mut() {
                        browser.replace_drum_pad(owner, &name, position);
                    }
                }
            }
            return;
        }

        if ctx.surface.is_delete_pressed() {
            // Former handleDeleteButton
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            let edit_midi_channel = ctx.config.midi_edit_channel();
            let row = self.drum.offset_y + played_pad;
            ctx.model.note_clip(8, 128).clear_row(edit_midi_channel, row);
        } else if ctx.surface.is_mute_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Mute);
            if let Some(bank) = self.drum.drum_pad_bank_mut(ctx) {
                bank.item_mut(played_pad as usize).toggle_mute();
            }
        } else if ctx.surface.is_solo_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Solo);
            if let Some(bank) = self.drum.drum_pad_bank_mut(ctx) {
                bank.item_mut(played_pad as usize).toggle_solo();
            }
        } else if ctx.surface.is_select_pressed() || ctx.config.is_auto_select_drum() {
            // Former handleSelectButton
            select_drum_pad(ctx, played_pad, Some(64), false);
        }
        self.drum.update_note_mapping(ctx);
    }
}

impl View for Drum64View {
    fn name(&self) -> &str {
        &self.drum.base.name
    }

    // AbstractDrum64View
    crate::forward_view!(drum => on_activate, on_deactivate, draw_grid, update_note_mapping, on_model_event);

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        scene_button_color_id(ctx, button)
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if is_scene_button(button) {
            on_scene_button_with_repeat(ctx, button, event);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if let Some(played_pad) = self.drum.on_grid_note(ctx, note, velocity) {
            self.handle_button_combinations(ctx, played_pad);
        }
    }

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    fn is_expression_view(&self) -> bool {
        true
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for Drum64View {
    // AbstractDrum64View
    crate::forward_transpose_view!(drum => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
