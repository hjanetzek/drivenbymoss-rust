// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum views of the Launchkey Mk3 and Mini Mk3 (former `DrumView` and `DrumConfigView` of
//! both packages).
//!
//! The drum view has two modes, toggled with the first scene button: play the 16 drum pads, or
//! edit the 16 steps of the selected pad. The second scene button shows the drum configuration
//! view while held (only with a clip): the upper row selects the resolution, the lower row
//! scrolls the steps (Mini: also the metronome and the drum octave).

use std::cell::RefCell;
use std::rc::Rc;

use super::super::colors::*;
use super::super::surface::after_view;
use crate::daw::clip::NotePosition;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::sequencer::abstract_drum_view::{DefaultDrumHooks, DrumBase, GRID_COLUMNS};
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

/// The number of steps of the sequencer.
const NUM_DISPLAY_COLS: i32 = 16;

/// The drum view (former `DrumView`).
pub struct DrumView {
    pub base: DrumBase,
    is_mini: bool,
    is_play_mode: bool,
}

impl DrumView {
    pub fn new(is_mini: bool) -> Self {
        let mut base = DrumBase::new("Drum", 2, NUM_DISPLAY_COLS, true);
        base.all_rows = 2;
        base.first_pad = ButtonId::Pad17;
        DrumView { base, is_mini, is_play_mode: true }
    }

    pub fn is_play_mode(&self) -> bool {
        self.is_play_mode
    }

    /// Former `setSelectedPad (selectedPad, velocity)`.
    fn set_selected_pad(&mut self, ctx: &mut Ctx, selected_pad: i32, velocity: i32) {
        if self.base.selected_pad == selected_pad {
            return;
        }
        self.base.selected_pad = selected_pad;
        self.base.seq.clear_edit_notes(ctx);
        if velocity > 0 {
            if let Some(device) = ctx.model.drum_device_mut() {
                device.drum_pad_bank_mut().select_pad(selected_pad.max(0) as usize);
            }
        }
    }
}

impl View for DrumView {
    crate::forward_view!(base.seq => name);
    crate::forward_view!(base => on_activate, on_deactivate, update_control_surface, on_model_event);

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        match button {
            ButtonId::Scene1 => {
                if self.is_play_mode {
                    LAUNCHKEY_COLOR_GREEN
                } else {
                    LAUNCHKEY_COLOR_BLUE
                }
            }
            _ if self.is_mini => LAUNCHKEY_COLOR_GREY_LO,
            ButtonId::Scene2 => LAUNCHKEY_COLOR_GREY_LO,
            ButtonId::ArrowUp | ButtonId::ArrowDown => {
                if ctx.surface.is_shift_pressed() {
                    return LAUNCHKEY_COLOR_BLACK;
                }
                let is_on = if button == ButtonId::ArrowUp { self.base.is_octave_up_button_on(ctx) } else { self.base.is_octave_down_button_on(ctx) };
                if is_on {
                    LAUNCHKEY_COLOR_GREEN
                } else {
                    LAUNCHKEY_COLOR_GREY_LO
                }
            }
            _ => LAUNCHKEY_COLOR_BLACK,
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let offset_y = ctx.model.scales().drum_offset();
        if self.is_play_mode {
            // 0-16
            self.set_selected_pad(ctx, index, velocity);
            // Mark selected note
            let key = offset_y + self.base.selected_pad;
            if (0..128).contains(&key) {
                self.base.seq.view.key_manager.set_key_pressed(key, velocity);
            }
        } else if self.base.seq.is_active() {
            let x = index % self.base.num_columns;
            let y = index / self.base.num_columns;
            self.base.handle_sequencer_area(&mut DefaultDrumHooks, ctx, index, x, y, offset_y, velocity);
        }
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let matrix = if ctx.model.can_selected_track_hold_notes() && self.is_play_mode { ctx.model.scales().drum_matrix() } else { Scales::empty_matrix() };
        self.base.seq.view.update_note_mapping(ctx, matrix);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if self.is_play_mode {
            let mut lights = Vec::with_capacity(16);
            if let Some(primary) = ctx.model.drum_device() {
                let drum_pad_bank = primary.drum_pad_bank();
                for y in 0..2 {
                    for x in 0..8 {
                        let index = 8 * y + x;
                        lights.push((x, 1 - y, self.base.drum_pad_color(ctx, index, drum_pad_bank, false)));
                    }
                }
            }
            let Ctx { surface, colors, .. } = ctx;
            let grid = surface.grid();
            for (x, y, color) in lights {
                grid.light_ex_id(colors, x, y, color);
            }
            return;
        }

        if !self.base.seq.is_active() {
            ctx.surface.grid().turn_off();
            return;
        }

        // Paint the sequencer steps
        self.base.seq.clip(ctx);
        let mut lights = Vec::with_capacity(16);
        if let Some(clip) = self.base.seq.clip_ref(ctx) {
            let step = clip.current_step();
            let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % self.base.sequencer_steps } else { -1 };
            let offset_y = ctx.model.scales().drum_offset();
            let channel = ctx.config.midi_edit_channel();
            let sel_pad = self.base.selected_pad;
            let edit_notes = self.base.seq.edit_notes(ctx);
            let row_color = ctx.model.drum_device().and_then(|primary| self.base.pad_color(primary, sel_pad));
            let note_row = offset_y + sel_pad;
            for col in 0..NUM_DISPLAY_COLS {
                let step_info = clip.step(&NotePosition::new(channel, col, note_row));
                let x = col % GRID_COLUMNS;
                let y = col / GRID_COLUMNS;
                lights.push((x, y, self.base.seq.step_color(Some(step_info), col == hi_step, row_color, channel, col, note_row, &edit_notes)));
            }
        }
        let Ctx { surface, colors, .. } = ctx;
        let grid = surface.grid();
        for (x, y, color) in lights {
            grid.light_ex_id(colors, x, y, color);
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        match button {
            ButtonId::Scene1 => {
                self.is_play_mode = !self.is_play_mode;
                self.update_note_mapping(ctx);
                ctx.notify(if self.is_play_mode { "Play / Select" } else { "Steps" });
            }
            ButtonId::Scene2 => {
                if self.base.seq.is_active() {
                    after_view(ctx, |ctx| ctx.set_active_view(Views::Shift));
                }
            }
            ButtonId::ArrowUp if !self.is_mini => self.base.on_octave_up(ctx, ButtonEvent::Down),
            ButtonId::ArrowDown if !self.is_mini => self.base.on_octave_down(ctx, ButtonEvent::Down),
            _ => {}
        }
    }

    crate::impl_as_any!();
}

/// Run a function with the drum view.
fn with_drum_view<R>(ctx: &mut Ctx, f: impl FnOnce(&mut DrumView, &mut Ctx) -> R) -> Option<R> {
    let view: Rc<RefCell<dyn View>> = ctx.views.get(Views::Drum)?;
    let mut view = view.try_borrow_mut().ok()?;
    let drum = view.as_any_mut().downcast_mut::<DrumView>()?;
    Some(f(drum, ctx))
}

/// The drum configuration view (former `DrumConfigView`).
pub struct DrumConfigView {
    is_mini: bool,
}

impl DrumConfigView {
    pub fn new(is_mini: bool) -> Self {
        DrumConfigView { is_mini }
    }
}

impl View for DrumConfigView {
    fn name(&self) -> &str {
        "Drum Configuration"
    }

    fn button_color(&self, _ctx: &Ctx, button: ButtonId) -> i32 {
        if button == ButtonId::Scene1 {
            LAUNCHKEY_COLOR_BLACK
        } else {
            LAUNCHKEY_COLOR_WHITE
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let index = note - 36;
        let col = index % 8;
        let is_mini = self.is_mini;
        with_drum_view(ctx, |view, ctx| {
            if index / 8 == 1 {
                view.base.seq.set_resolution_index(ctx, col);
                return;
            }
            let clip_key = view.base.seq.clip_key();
            match col {
                0 => {
                    view.base.seq.clip(ctx).scroll_steps_page_backwards();
                    mv_helper::notify_edit_page(ctx, clip_key);
                }
                1 => {
                    view.base.seq.clip(ctx).scroll_steps_page_forward();
                    mv_helper::notify_edit_page(ctx, clip_key);
                }
                3 if is_mini => {
                    ctx.model.transport_mut().toggle_metronome();
                    ctx.schedule(100, |ctx| {
                        let text = format!("Metronome: {}", if ctx.model.transport().is_metronome_on() { "On" } else { "Off" });
                        ctx.notify(&text);
                    });
                }
                6 if is_mini => view.base.on_octave_down(ctx, ButtonEvent::Down),
                7 if is_mini => view.base.on_octave_up(ctx, ButtonEvent::Down),
                _ => {}
            }
        });
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let is_mini = self.is_mini;
        let colors = with_drum_view(ctx, |view, ctx| {
            let resolution = view.base.seq.resolution_index(ctx) as i32;
            let clip = view.base.seq.clip(ctx);
            let (can_backwards, can_forwards) = (clip.can_scroll_steps_backwards(), clip.can_scroll_steps_forwards());
            (resolution, can_backwards, can_forwards)
        });
        let Some((resolution, can_backwards, can_forwards)) = colors else {
            return;
        };
        let pick = |on: bool, hi: i32, lo: i32| if on { hi } else { lo };
        let mut lower = [LAUNCHKEY_COLOR_BLACK; 8];
        lower[0] = pick(can_backwards, LAUNCHKEY_COLOR_LIME_HI, LAUNCHKEY_COLOR_LIME_LO);
        lower[1] = pick(can_forwards, LAUNCHKEY_COLOR_LIME_HI, LAUNCHKEY_COLOR_LIME_LO);
        if is_mini {
            let scales = ctx.model.scales();
            lower[3] = pick(ctx.model.transport().is_metronome_on(), LAUNCHKEY_COLOR_RED_HI, LAUNCHKEY_COLOR_RED_LO);
            // Note: Java shows the state of octave up on the octave down pad and vice versa
            lower[6] = pick(scales.can_scroll_drum_octave_up(), LAUNCHKEY_COLOR_CYAN_HI, LAUNCHKEY_COLOR_CYAN_LO);
            lower[7] = pick(scales.can_scroll_drum_octave_down(), LAUNCHKEY_COLOR_CYAN_HI, LAUNCHKEY_COLOR_CYAN_LO);
        }
        let grid = ctx.surface.grid();
        for x in 0..8 {
            grid.light_ex(x, 0, if resolution == x { LAUNCHKEY_COLOR_BLUE_HI } else { LAUNCHKEY_COLOR_BLUE_LO });
        }
        for (x, color) in lower.iter().enumerate() {
            grid.light_ex(x as i32, 1, *color);
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if button == ButtonId::Scene2 && event == ButtonEvent::Up {
            after_view(ctx, |ctx| ctx.restore_view());
        }
    }

    crate::impl_as_any!();
}
