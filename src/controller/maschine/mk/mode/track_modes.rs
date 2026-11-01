// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The track modes of the Maschine with display (former `MaschineVolumeMode`,
//! `MaschinePanMode` and `MaschineSendMode`).

use super::{shorten_and_fix_ascii, with_display, TouchStates, MAIN_ENCODER};
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::mode::parameter_mode::default_knob_ids;
use crate::framework::mode::track::{TrackPanMode, TrackSendMode, TrackVolumeMode};
use crate::impl_as_any;

/// Translate the knob index: the main encoder (and its touch index 8) controls the selected
/// track (former `getTrack (-1)`).
fn track_index(ctx: &Ctx, index: usize) -> Option<usize> {
    if index == MAIN_ENCODER || index == 8 {
        ctx.model.current_track_bank().selected_index()
    } else {
        Some(index)
    }
}

/// Forwards the functions of the [`Mode`] trait to the wrapped track mode; the knob functions
/// map the main encoder to the selected track, the touch state is kept here.
macro_rules! forward_track_mode {
    () => {
        // TrackVolumeMode, TrackPanMode, TrackSendMode
        crate::forward_mode!(inner => name, on_activate, on_deactivate, on_button, select_item,
            selected_item_name, select_previous_item, select_next_item, select_previous_item_page,
            select_next_item_page, select_item_page, has_previous_item, has_next_item,
            has_previous_item_page, has_next_item_page);

        fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
            if let Some(index) = track_index(ctx, index) {
                self.inner.on_knob_value(ctx, index, value);
            }
        }

        fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
            match track_index(ctx, index) {
                Some(index) => self.inner.knob_value(ctx, index),
                None => -1,
            }
        }

        fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
            self.touch.set(index, is_touched);
            if let Some(index) = track_index(ctx, index) {
                self.inner.on_knob_touch(ctx, index, is_touched);
            }
        }

        fn is_any_knob_touched(&self) -> bool {
            self.touch.is_any_touched()
        }

        impl_as_any!();
    };
}

/// The name of the track for the display: shortened and marked if selected.
fn track_label(ctx: &Ctx, index: usize) -> String {
    let track = ctx.model.current_track_bank().item(index);
    let name = shorten_and_fix_ascii(track.name(), 6);
    if track.is_selected() {
        format!(">{name}")
    } else {
        name
    }
}

/// Edit the volume of the tracks (former `MaschineVolumeMode`).
pub struct MaschineVolumeMode {
    inner: TrackVolumeMode,
    touch: TouchStates,
    display_vu: bool,
}

impl MaschineVolumeMode {
    pub fn new(ctx: &Ctx, has_mcu_display: bool) -> Self {
        let inner = if has_mcu_display { TrackVolumeMode::new_with_controls(ctx, false, default_knob_ids()) } else { TrackVolumeMode::new(ctx, false) };
        MaschineVolumeMode { inner, touch: TouchStates::default(), display_vu: false }
    }

    pub fn toggle_display_vu(&mut self) {
        self.display_vu = !self.display_vu;
    }

    pub fn is_display_vu(&self) -> bool {
        self.display_vu
    }
}

impl Mode for MaschineVolumeMode {
    forward_track_mode!();

    fn update_display(&mut self, ctx: &mut Ctx) {
        let mut cells = Vec::with_capacity(8);
        for i in 0..8 {
            let track = ctx.model.current_track_bank().item(i);
            let value = if self.display_vu && !self.touch.is_touched(i) && !(self.touch.is_touched(8) && track.is_selected()) {
                let steps = (ctx.model.value_changer().to_normalized_value(track.vu() as f64) * 6.0).round() as usize;
                ">".repeat(steps.min(16))
            } else {
                track.volume_str_limited(6)
            };
            cells.push((track_label(ctx, i), value));
        }
        with_display(ctx, |d| {
            for (i, (name, value)) in cells.iter().enumerate() {
                d.set_cell(0, i, name);
                d.set_cell(1, i, value);
            }
            d.all_done();
        });
    }
}

/// Edit the panorama of the tracks (former `MaschinePanMode`).
pub struct MaschinePanMode {
    inner: TrackPanMode,
    touch: TouchStates,
}

impl MaschinePanMode {
    pub fn new(ctx: &Ctx, has_mcu_display: bool) -> Self {
        let inner = if has_mcu_display { TrackPanMode::new_with_controls(ctx, false, default_knob_ids()) } else { TrackPanMode::new(ctx, false) };
        MaschinePanMode { inner, touch: TouchStates::default() }
    }
}

impl Mode for MaschinePanMode {
    forward_track_mode!();

    fn update_display(&mut self, ctx: &mut Ctx) {
        let cells: Vec<(String, String)> = (0..8).map(|i| (track_label(ctx, i), ctx.model.current_track_bank().item(i).pan_str_limited(6))).collect();
        with_display(ctx, |d| {
            for (i, (name, value)) in cells.iter().enumerate() {
                d.set_cell(0, i, name);
                d.set_cell(1, i, value);
            }
            d.all_done();
        });
    }
}

/// Edit a send of the tracks (former `MaschineSendMode`).
pub struct MaschineSendMode {
    inner: TrackSendMode,
    touch: TouchStates,
    send_index: usize,
}

impl MaschineSendMode {
    pub fn new(send_index: usize, ctx: &Ctx, has_mcu_display: bool) -> Self {
        let inner = if has_mcu_display {
            TrackSendMode::new_with_controls(send_index as i32, ctx, false, default_knob_ids())
        } else {
            TrackSendMode::new(send_index as i32, ctx, false)
        };
        MaschineSendMode { inner, touch: TouchStates::default(), send_index }
    }
}

impl Mode for MaschineSendMode {
    forward_track_mode!();

    fn update_display(&mut self, ctx: &mut Ctx) {
        let send_index = self.send_index;
        let cells: Vec<(String, String)> = (0..8)
            .map(|i| {
                let send = ctx.model.current_track_bank().item(i).send_bank().item(send_index);
                (track_label(ctx, i), send.displayed_value_limited(6))
            })
            .collect();
        with_display(ctx, |d| {
            for (i, (name, value)) in cells.iter().enumerate() {
                d.set_cell(0, i, name);
                d.set_cell(1, i, value);
            }
            d.all_done();
        });
    }
}
