// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A drum view which plays 64 drum pads arranged in 4x4 blocks (former `AbstractDrum64View`).

use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::daw::device::DrumPadBank;
use crate::daw::events::ModelEvent;
use crate::framework::color_ids::abstract_drum_view::{COLOR_PAD_MUTED, COLOR_PAD_NO_CONTENT, COLOR_PAD_OFF, COLOR_PAD_PLAY, COLOR_PAD_RECORD, COLOR_PAD_SELECTED};
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::ViewBase;
use crate::midi::ButtonEvent;
use crate::scale::{NoteMatrix, Scales};

const DRUM_START_KEY: i32 = 36;
const BLOCK_SIZE: i32 = 16;

/// The state of the drum 64 view.
pub struct Drum64ViewBase {
    pub base: ViewBase,
    pub offset_y: i32,
    pub selected_pad: i32,
    pub pressed_keys: [i32; 128],
    pub columns: i32,
    pub rows: i32,
    pub drum_octave: i32,
    pub drum_start_key: i32,
    xblocks: i32,
    yblocks: i32,
    num_pad_grid: i32,
}

impl Default for Drum64ViewBase {
    fn default() -> Self {
        Self::new(8, 8)
    }
}

impl Drum64ViewBase {
    pub fn new(columns: i32, rows: i32) -> Self {
        Drum64ViewBase {
            base: ViewBase::new(&format!("Drum {}", columns * rows)),
            offset_y: DRUM_START_KEY,
            selected_pad: 0,
            pressed_keys: [0; 128],
            columns,
            rows,
            drum_octave: 0,
            drum_start_key: DRUM_START_KEY,
            // The number of 4x4 blocks in x and y direction
            xblocks: columns / 4,
            yblocks: rows / 4,
            num_pad_grid: columns * rows,
        }
    }

    /// The drum pad bank of the drum device with the size of the grid.
    pub fn drum_pad_bank<'a>(&self, ctx: &'a Ctx) -> Option<&'a DrumPadBank> {
        ctx.model.drum_device_sized(self.num_pad_grid as usize).or_else(|| ctx.model.drum_device()).map(|d| d.drum_pad_bank())
    }

    pub fn drum_pad_bank_mut<'a>(&self, ctx: &'a mut Ctx) -> Option<&'a mut DrumPadBank> {
        let size = self.num_pad_grid as usize;
        if ctx.model.drum_device_sized(size).is_some() {
            return ctx.model.drum_device_sized_mut(size).map(|d| d.drum_pad_bank_mut());
        }
        ctx.model.drum_device_mut().map(|d| d.drum_pad_bank_mut())
    }

    /// Replaces the track bank observers.
    pub fn on_model_event(&mut self, ctx: &Ctx, event: &ModelEvent) {
        match event {
            ModelEvent::TrackSelectionChanged { .. } => self.clear_pressed_keys(),
            ModelEvent::TrackNote { track_index, note, velocity } => {
                let selected = ctx.model.current_track_bank().selected_item().map(|t| t.index());
                if selected == Some(*track_index) && (0..128).contains(note) {
                    self.pressed_keys[*note as usize] = *velocity;
                }
            }
            _ => {}
        }
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.update_note_mapping(ctx);
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.set_indication(true);
        }
        // Bring everything in sync...
        self.reset_octave(ctx);
    }

    pub fn on_deactivate(&mut self, ctx: &mut Ctx) {
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.set_indication(false);
        }
    }

    /// A pad was played. Returns the played pad (in the 64 pads) if a pad was pressed, the
    /// caller then handles the button combinations (see [`Self::handle_button_combinations`]).
    pub fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) -> Option<i32> {
        if !ctx.model.can_selected_track_hold_notes() {
            return None;
        }

        let index = note - self.drum_start_key;
        let x = index % self.columns;
        let y = index / self.columns;
        let xblock_pos = x / 4;
        let yblock_pos = y / 4;
        let blocks = xblock_pos * self.yblocks * 16 + yblock_pos * 16;
        self.selected_pad = blocks + y % 4 * 4 + x % 4;
        let played_pad = if velocity == 0 { -1 } else { self.selected_pad };

        // Mark selected note
        let key = self.offset_y + self.selected_pad;
        if (0..128).contains(&key) {
            self.pressed_keys[key as usize] = velocity;
        }

        if played_pad < 0 {
            return None;
        }
        Some(played_pad)
    }

    /// Draw the 4x4 blocks.
    pub fn draw_grid(&self, ctx: &mut Ctx) {
        if !ctx.model.can_selected_track_hold_notes() {
            ctx.surface.grid().turn_off();
            return;
        }

        let is_recording = ctx.model.has_recording_state();
        let mut block_offset = 0;

        // Draw all blocks
        for xblock in 0..self.xblocks {
            for yblock in 0..self.yblocks {
                // Draw a 4x4 square
                for block_x in 0..4 {
                    for block_y in 0..4 {
                        let index = block_offset + block_y * 4 + block_x;
                        let x = xblock * 4 + block_x;
                        let y = yblock * 4 + block_y;
                        let color_id = self.drum_pad_color(ctx, index, is_recording);
                        let color = ctx.colors.color_index(color_id);
                        ctx.surface.grid().light_ex(x, self.rows - 1 - y, color);
                    }
                }
                block_offset += 16;
            }
        }
    }

    fn drum_pad_color(&self, ctx: &Ctx, index: i32, is_recording: bool) -> &'static str {
        // Playing note?
        let key = self.offset_y + index;
        if (0..128).contains(&key) && self.pressed_keys[key as usize] > 0 {
            return if is_recording { COLOR_PAD_RECORD } else { COLOR_PAD_PLAY };
        }

        // Selected?
        if self.selected_pad == index {
            return COLOR_PAD_SELECTED;
        }

        // Exists and active?
        let empty_color = if ctx.config.is_turn_off_empty_drum_pads() { COLOR_PAD_OFF } else { COLOR_PAD_NO_CONTENT };
        let Some(bank) = self.drum_pad_bank(ctx) else {
            return empty_color;
        };
        let drum_pad = bank.item(index as usize);
        if !drum_pad.does_exist() || !drum_pad.is_activated() {
            return empty_color;
        }

        // Muted or soloed?
        if drum_pad.is_mute() || bank.has_soloed_pads() && !drum_pad.is_solo() {
            return COLOR_PAD_MUTED;
        }

        // Former getPadContentColor
        DawColor::color_id(drum_pad.color())
    }

    pub fn clear_pressed_keys(&mut self) {
        self.pressed_keys = [0; 128];
    }

    /// Update the note mapping. Blocks the pads if a combination button is pressed.
    pub fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let surface = &ctx.surface;
        let turn_on = ctx.model.can_selected_track_hold_notes()
            && !surface.is_select_pressed()
            && !surface.is_delete_pressed()
            && !surface.is_mute_pressed()
            && !surface.is_solo_pressed();
        let matrix = if turn_on { self.drum_matrix() } else { Scales::empty_matrix() };
        self.base.update_note_mapping(ctx, matrix);
    }

    pub fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            self.set_octave(ctx, self.drum_octave - 1);
        }
    }

    pub fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            self.set_octave(ctx, self.drum_octave + 1);
        }
    }

    pub fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.set_octave(ctx, 0);
    }

    fn set_octave(&mut self, ctx: &mut Ctx, octave: i32) {
        self.clear_pressed_keys();
        self.drum_octave = octave.clamp(-2, 1);
        self.offset_y = self.drum_start_key + self.drum_octave * BLOCK_SIZE;
        self.update_note_mapping(ctx);
        let text = self.drum_range_text();
        ctx.notify(&text);
        let offset = self.offset_y.max(0) as usize;
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.scroll_to(offset, true);
        }
    }

    pub fn is_octave_up_button_on(&self) -> bool {
        self.drum_octave < 1
    }

    pub fn is_octave_down_button_on(&self) -> bool {
        self.drum_octave > Scales::DRUM_OCTAVE_LOWER
    }

    /// Handle the combination buttons when a pad is played. `handle_delete` is called if Delete
    /// is pressed (former overridable `handleDeleteButton`).
    pub fn handle_button_combinations(&mut self, ctx: &mut Ctx, played_pad: i32, handle_delete: impl FnOnce(&mut Self, &mut Ctx, i32)) {
        if ctx.surface.is_delete_pressed() {
            // Delete all of the notes on that "pad"
            handle_delete(self, ctx, played_pad);
        } else if ctx.surface.is_mute_pressed() {
            // Mute that "pad"
            ctx.surface.set_trigger_consumed(ButtonId::Mute);
            if let Some(bank) = self.drum_pad_bank_mut(ctx) {
                bank.item_mut(played_pad as usize).toggle_mute();
            }
        } else if ctx.surface.is_solo_pressed() {
            // Solo that "pad"
            ctx.surface.set_trigger_consumed(ButtonId::Solo);
            if let Some(bank) = self.drum_pad_bank_mut(ctx) {
                bank.item_mut(played_pad as usize).toggle_solo();
            }
        } else if ctx.surface.is_select_pressed() || ctx.config.is_auto_select_drum() {
            // Also select the matching device layer channel of the pad (former
            // handleSelectButton, which is empty)
        }
        self.update_note_mapping(ctx);
    }

    fn drum_matrix(&self) -> NoteMatrix {
        let mut note_map = Scales::empty_matrix();
        let mut block_offset = 0;

        // All blocks
        for xblock in 0..self.xblocks {
            for yblock in 0..self.yblocks {
                // One 4x4 square
                for block_x in 0..4 {
                    for block_y in 0..4 {
                        let index = block_offset + block_y * 4 + block_x;
                        let x = xblock * 4 + block_x;
                        let y = yblock * 4 + block_y;
                        let note = (self.drum_start_key + y * self.columns + x) as usize;
                        let value = index + self.offset_y;
                        note_map[note] = if !(-1..=127).contains(&value) { -1 } else { value };
                    }
                }
                block_offset += 16;
            }
        }
        note_map
    }

    fn drum_range_text(&self) -> String {
        let s = self.drum_start_key + self.drum_octave * self.num_pad_grid;
        format!("{} to {}", Scales::format_drum_note(s), Scales::format_drum_note(s + self.num_pad_grid - 1))
    }

    pub fn drum_octave(&self) -> i32 {
        self.drum_octave
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix() {
        let view = Drum64ViewBase::default();
        let m = view.drum_matrix();
        // Lower left 4x4 block contains the pads 36..51
        assert_eq!(m[36], 36);
        assert_eq!(m[37], 37);
        assert_eq!(m[44], 40);
        // Upper left block
        assert_eq!(m[36 + 32], 36 + 16);
        // Lower right block
        assert_eq!(m[40], 36 + 32);
    }
}
