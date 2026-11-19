// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum sequencer view of the Push (former `DrumView`), an expression view on the Push 3.
//!
//! The drum pad area is handled here (former `handleNoteArea` + `handleNoteAreaButtonCombinations`
//! of `AbstractDrumView`) since the Push overrides `handleSelectButton`, which the generic
//! [`DrumBase`] does not offer as a hook.

use super::sequencer_view::handle_push_step_combinations;
use super::{handle_drum_scene_repeat, note_repeat_button_color_id, select_drum_pad};
use crate::daw::bank::BankItem;
use crate::daw::clip::NotePosition;
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::view::base::is_scene_button;
use crate::framework::view::sequencer::abstract_drum_view::{DrumBase, DrumHooks, GRID_COLUMNS};
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The name of the drum view.
pub const NAME_DRUM: &str = "Drum";

struct Hooks;

impl DrumHooks for Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        handle_push_step_combinations(&mut base.seq, ctx, note_position, velocity, false) || base.handle_sequencer_area_button_combinations(ctx, note_position, velocity)
    }
}

/// Select a pad of a drum view and mark it as played (former `handleNoteArea` without the button
/// combinations). Returns the selected pad.
pub fn select_played_pad(base: &mut DrumBase, ctx: &mut Ctx, pad: i32, offset_y: i32, velocity: i32) -> i32 {
    if velocity > 0 {
        base.set_selected_pad(ctx, pad);
    } else if base.selected_pad != pad {
        base.selected_pad = pad;
        base.seq.clear_edit_notes(ctx);
    }
    // Mark selected note
    let key = offset_y + base.selected_pad;
    if (0..128).contains(&key) {
        base.seq.view.key_manager.set_key_pressed(key, velocity);
    }
    base.selected_pad
}

/// The drum view.
pub struct DrumView {
    pub base: DrumBase,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl Default for DrumView {
    fn default() -> Self {
        Self::new()
    }
}

impl DrumView {
    pub fn new() -> Self {
        DrumView { base: DrumBase::new(NAME_DRUM, 4, 4, true), aftertouch: Box::new(AftertouchViewCommand::new()) }
    }

    pub fn clip_key(&self) -> ClipKey {
        self.base.seq.clip_key()
    }

    /// Former `handleNoteArea` with `handleNoteAreaButtonCombinations`.
    fn handle_note_area(&mut self, ctx: &mut Ctx, x: i32, y: i32, offset_y: i32, velocity: i32) {
        let pad = self.base.play_columns * y + x;
        let played_pad = select_played_pad(&mut self.base, ctx, pad, offset_y, velocity);
        if velocity == 0 {
            return;
        }

        if self.base.is_delete_trigger(ctx) {
            // Delete all of the notes on that 'pad'
            ctx.surface.set_trigger_consumed(self.base.button_delete);
            let channel = ctx.config.midi_edit_channel();
            let row = ctx.model.scales().drum_offset() + played_pad;
            self.base.seq.clip(ctx).clear_row(channel, row);
            return;
        }
        if self.base.is_mute_trigger(ctx) {
            ctx.surface.set_trigger_consumed(self.base.button_mute);
            if let Some(device) = ctx.model.drum_device_mut() {
                device.drum_pad_bank_mut().item_mut(played_pad as usize).toggle_mute();
            }
            return;
        }
        if self.base.is_solo_trigger(ctx) {
            ctx.surface.set_trigger_consumed(self.base.button_solo);
            if let Some(device) = ctx.model.drum_device_mut() {
                device.drum_pad_bank_mut().item_mut(played_pad as usize).toggle_solo();
            }
            return;
        }
        if self.base.is_browse_trigger(ctx) {
            ctx.surface.set_trigger_consumed(self.base.button_browse);
            let Some(primary) = ctx.model.drum_device() else {
                return;
            };
            if !primary.has_drum_pads() {
                return;
            }
            let bank = primary.drum_pad_bank();
            self.base.scroll_position = bank.scroll_position();
            let pad = bank.item(played_pad as usize);
            let (owner, name, position) = (bank.owner(), pad.name().to_string(), pad.position());
            if let Some(browser) = ctx.model.browser_mut() {
                browser.replace_drum_pad(owner, &name, position);
            }
            return;
        }
        if self.base.is_select_trigger(ctx) || ctx.config.is_auto_select_drum() {
            // Former handleSelectButton
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            if select_drum_pad(ctx, played_pad, None, true) {
                self.update_note_mapping(ctx);
            }
        }
    }
}

impl View for DrumView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, on_sequencer_left, on_sequencer_right, sequencer_resolution_index,
        set_sequencer_resolution_index);
    // AbstractDrumView
    crate::forward_view!(base => on_activate, on_deactivate, draw_grid, update_control_surface,
        update_note_mapping, on_model_event);

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    fn is_expression_view(&self) -> bool {
        true
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if is_scene_button(button) && ctx.surface.is_pressed(ButtonId::Repeat) {
            return note_repeat_button_color_id(ctx, button);
        }
        self.base.seq.button_color_id(ctx, button)
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down {
            return;
        }
        if handle_drum_scene_repeat(ctx, button, event) {
            return;
        }
        self.base.seq.on_button(ctx, button, event);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let x = index % self.base.num_columns;
        let y = index / self.base.num_columns;
        if y < self.base.play_rows && x < self.base.play_columns {
            let offset_y = ctx.model.scales().drum_offset();
            self.handle_note_area(ctx, x, y, offset_y, velocity);
            return;
        }
        self.base.on_grid_note(&mut Hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.seq.is_active() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, index.max(0) as usize));
        let y = index / GRID_COLUMNS;
        // Sequencer steps?
        if y < self.base.play_rows {
            return;
        }
        let x = index % GRID_COLUMNS;
        let step_x = GRID_COLUMNS * (self.base.all_rows - 1 - y) + x;
        let step_y = ctx.model.scales().drum_offset() + self.base.selected_pad;
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), step_x, step_y);
        self.base.seq.edit_note(ctx, &note_position, false);
    }

    fn sequencer_clip_key(&self) -> Option<ClipKey> {
        Some(self.base.seq.clip_key())
    }

    fn sequencer_edit_note(&mut self, ctx: &mut Ctx, note_position: &crate::daw::clip::NotePosition, add_note: bool) {
        self.base.seq.edit_note(ctx, note_position, add_note);
    }

    crate::impl_transpose_view!();


    crate::impl_as_any!();
}

impl TransposeView for DrumView {
    // AbstractDrumView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
