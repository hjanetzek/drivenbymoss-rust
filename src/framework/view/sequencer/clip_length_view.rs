// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Edit the loop of a clip with the pads (former `ClipLengthView`).

use super::abstract_sequencer_view::SequencerBase;
use crate::daw::color::DawColor;
use crate::framework::color_ids::abstract_sequencer_view::{COLOR_RESOLUTION, COLOR_RESOLUTION_OFF, COLOR_RESOLUTION_SELECTED};
use crate::framework::color_ids::clip_length_view::{COLOR_OUTSIDE, COLOR_PART};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::midi::ButtonEvent;

/// Former `Views.NAME_CLIP_LENGTH`.
pub const NAME_CLIP_LENGTH: &str = "Clip Length";

const PAD_RESOLUTIONS: [i32; 3] = [1, 4, 16];

/// Edit the loop of a clip with the pads: each pad is a measure (or 1/4 or 1/16 of it,
/// selectable with the first 3 scene buttons).
pub struct ClipLengthView {
    pub seq: SequencerBase,
    pad_resolution: usize,
    first_pressed_pad: i32,
    has_second_pressed: bool,
}

impl ClipLengthView {
    pub fn new(_ctx: &Ctx, use_daw_colors: bool) -> Self {
        ClipLengthView { seq: SequencerBase::new(NAME_CLIP_LENGTH, 0, 0, 0, use_daw_colors), pad_resolution: 0, first_pressed_pad: -1, has_second_pressed: false }
    }

    fn quarters_per_pad(&self, ctx: &Ctx) -> f64 {
        ctx.model.transport().quarters_per_measure() as f64 / PAD_RESOLUTIONS[self.pad_resolution] as f64
    }
}

impl View for ClipLengthView {
    // AbstractSequencerView
    crate::forward_view!(seq => name, update_control_surface);

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.seq.clip(ctx);
        self.update_note_mapping(ctx);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.seq.view.clear_note_mapping(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let cols = ctx.surface.grid_ref().cols();
        let index = note - 36;
        let x = index % cols;
        let y = index / cols;

        // Invert to top/left to bottom/right
        let pad = (cols - 1 - y) * cols + x;

        // On button press...
        if velocity > 0 {
            // Not yet a button pressed, store it
            if self.first_pressed_pad == -1 {
                self.first_pressed_pad = pad;
            }
            return;
        }

        // On button release...
        let is_second_pad = self.first_pressed_pad != pad;
        if is_second_pad {
            self.has_second_pressed = true;
        }
        if is_second_pad || !self.has_second_pressed {
            let start = self.first_pressed_pad.min(pad);
            let end = self.first_pressed_pad.max(pad) + 1;
            let quarters_per_pad = self.quarters_per_pad(ctx);

            // Set a new loop between the 2 selected pads
            let new_start = start as f64 * quarters_per_pad;
            let clip = self.seq.clip(ctx);
            clip.set_loop_start(new_start);
            clip.set_loop_length((end - start) as f64 * quarters_per_pad);
            clip.set_play_range(new_start, end as f64 * quarters_per_pad);
        }

        if self.first_pressed_pad == pad {
            self.first_pressed_pad = -1;
            self.has_second_pressed = false;
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let quarters_per_pad = self.quarters_per_pad(ctx);
        let clip = self.seq.clip(ctx);
        let start = clip.loop_start();
        let max_quarters = quarters_per_pad * 64.0;
        let loop_start_pad = (start.max(0.0) / quarters_per_pad).floor() as i32;
        let loop_end_pad = ((start + clip.loop_length()).min(max_quarters) / quarters_per_pad).ceil() as i32;
        let clip_color = if self.seq.use_daw_colors { DawColor::color_id(clip.color()) } else { COLOR_PART };

        let Ctx { surface, colors, .. } = ctx;
        let grid = surface.grid();
        let cols = grid.cols();
        for pad in 0..64 {
            let color = if pad >= loop_start_pad && pad < loop_end_pad { clip_color } else { COLOR_OUTSIDE };
            grid.light_ex_id(colors, pad % cols, pad / cols, color);
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down || index >= 3 {
            return;
        }
        self.pad_resolution = index;
        ctx.notify(&format!("1/{}", PAD_RESOLUTIONS[self.pad_resolution]));
    }

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        match scene_index(button) {
            None => BUTTON_COLOR_OFF,
            Some(scene) if scene < 3 => {
                if scene == self.pad_resolution {
                    COLOR_RESOLUTION_SELECTED
                } else {
                    COLOR_RESOLUTION
                }
            }
            Some(_) => COLOR_RESOLUTION_OFF,
        }
    }

    crate::impl_as_any!();
}
