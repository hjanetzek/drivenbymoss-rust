// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The mix view of the Launchpad (former `MixView`): the rows of the pad grid select the fader
//! mode or toggle stop/mute/solo/rec arm of the 8 tracks, the scene buttons are a fader for the
//! selected track.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::session_view::session_scroll_states;
use crate::controller::launchpad::colors::*;
use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::daw::track::Track;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::{scene_index, ViewBase};
use crate::framework::virtual_fader::{self, VirtualFader, VirtualFaderCallback};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// What the fader (scene buttons) controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaderMode {
    Volume,
    Pan,
    Send1,
    Send2,
}

/// The value of the selected track for the fader mode.
fn fader_value(track: &Track, mode: FaderMode) -> i32 {
    match mode {
        FaderMode::Volume => track.volume(),
        FaderMode::Pan => track.pan(),
        FaderMode::Send1 | FaderMode::Send2 => {
            let send = track.send_bank().item(if mode == FaderMode::Send1 { 0 } else { 1 });
            if send.does_exist() {
                send.value()
            } else {
                0
            }
        }
    }
}

/// The fader callback (former `IVirtualFaderCallback` implementation of the view).
struct MixFaderCallback {
    mode: Rc<Cell<FaderMode>>,
}

impl VirtualFaderCallback for MixFaderCallback {
    fn value(&self, ctx: &Ctx) -> i32 {
        ctx.model.current_track_bank().selected_item().map(|t| fader_value(t, self.mode.get())).unwrap_or(0)
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        let mode = self.mode.get();
        let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() else {
            return;
        };
        match mode {
            FaderMode::Volume => track.volume_parameter_mut().set_value_immediatly(value),
            FaderMode::Pan => track.pan_parameter_mut().set_value_immediatly(value),
            FaderMode::Send1 | FaderMode::Send2 => {
                let send = track.send_bank_mut().item_mut(if mode == FaderMode::Send1 { 0 } else { 1 });
                if send.does_exist() {
                    send.set_value_immediatly(value);
                }
            }
        }
    }
}

/// The mix view.
pub struct MixView {
    base: ViewBase,
    fader: Rc<RefCell<VirtualFader>>,
    fader_mode: Rc<Cell<FaderMode>>,
}

impl MixView {
    pub fn new(_ctx: &Ctx) -> Self {
        let fader_mode = Rc::new(Cell::new(FaderMode::Volume));
        let fader = VirtualFader::new(Rc::new(MixFaderCallback { mode: fader_mode.clone() }), None).shared();
        MixView { base: ViewBase::new("Mix"), fader, fader_mode }
    }

    fn select_track(ctx: &mut Ctx, index: usize) {
        if ctx.model.current_track_bank().item(index).is_selected() {
            return;
        }
        ctx.model.current_track_bank_mut().item_mut(index).select();
        let track = ctx.model.current_track_bank().item(index);
        let message = format!("{}: {}", track.position() + 1, track.name());
        ctx.notify(&message);
    }
}

impl View for MixView {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        for i in 0..8 {
            let track = ctx.model.current_track_bank().item(i);
            let n = i as i32;
            let colors: [i32; 8] = if track.does_exist() {
                let is_selected = track.is_selected();
                let has_sends = track.send_bank().item_count() > 0;
                let send_color = |hi: i32| {
                    if !has_sends {
                        LAUNCHPAD_COLOR_BLACK
                    } else if is_selected {
                        hi
                    } else {
                        LAUNCHPAD_COLOR_GREY_LO
                    }
                };
                [
                    // Record Arm
                    if track.is_rec_arm() { LAUNCHPAD_COLOR_RED_HI } else { LAUNCHPAD_COLOR_GREY_LO },
                    // Solo
                    if track.is_solo() { LAUNCHPAD_COLOR_BLUE_HI } else { LAUNCHPAD_COLOR_GREY_MD },
                    // Mute
                    if track.is_mute() { LAUNCHPAD_COLOR_GREY_LO } else { LAUNCHPAD_COLOR_YELLOW_HI },
                    // Stop
                    if ctx.surface.is_pressed(ButtonId::get(ButtonId::Pad25, i)) { LAUNCHPAD_COLOR_RED } else { LAUNCHPAD_COLOR_ROSE },
                    // Send 2
                    send_color(LAUNCHPAD_COLOR_LIME_HI),
                    // Send 1
                    send_color(LAUNCHPAD_COLOR_ORCHID_HI),
                    // Panning
                    if is_selected { LAUNCHPAD_COLOR_SKY_HI } else { LAUNCHPAD_COLOR_GREY_LO },
                    // Volume
                    ctx.colors.color_index(DawColor::color_id(track.color())),
                ]
            } else {
                [LAUNCHPAD_COLOR_BLACK; 8]
            };
            let grid = ctx.surface.grid();
            for (row, color) in colors.iter().enumerate() {
                grid.light(36 + row as i32 * 8 + n, *color);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let n = note - 36;
        let index = (n % 8) as usize;
        let what = n / 8;

        match what {
            7 => {
                self.fader_mode.set(FaderMode::Volume);
                ctx.notify("Volume");
                Self::select_track(ctx, index);
            }
            6 => {
                self.fader_mode.set(FaderMode::Pan);
                ctx.notify("Panning");
                Self::select_track(ctx, index);
            }
            5 | 4 => {
                let send_index = if what == 5 { 0 } else { 1 };
                self.fader_mode.set(if what == 5 { FaderMode::Send1 } else { FaderMode::Send2 });
                let send = ctx.model.current_track_bank().item(index).send_bank().item(send_index);
                let name = if send.does_exist() { send.name().to_string() } else { "None".to_string() };
                ctx.notify(&format!("Send {}: {}", send_index + 1, name));
                Self::select_track(ctx, index);
            }
            3 => {
                ctx.model.current_track_bank_mut().item_mut(index).stop(false);
                ctx.notify("Stop clip");
            }
            2 => {
                ctx.model.current_track_bank_mut().item_mut(index).toggle_mute();
                ctx.notify("Mute");
            }
            1 => {
                ctx.model.current_track_bank_mut().item_mut(index).toggle_solo();
                ctx.notify("Solo");
            }
            0 => {
                ctx.model.current_track_bank_mut().item_mut(index).toggle_rec_arm();
                ctx.notify("Rec Arm");
            }
            // Not used
            _ => {}
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(scene) = scene_index(button) else {
            return 0;
        };
        let mode = self.fader_mode.get();
        let (color, value) = match ctx.model.current_track_bank().selected_item() {
            Some(track) => {
                let color = match mode {
                    FaderMode::Volume => LAUNCHPAD_COLOR_CYAN,
                    FaderMode::Pan => LAUNCHPAD_COLOR_SKY_HI,
                    FaderMode::Send1 => LAUNCHPAD_COLOR_ORCHID_HI,
                    FaderMode::Send2 => LAUNCHPAD_COLOR_LIME_HI,
                };
                (color, fader_value(track, mode))
            }
            None => (0, 0),
        };
        let mut fader = self.fader.borrow_mut();
        fader.setup(color, mode == FaderMode::Pan);
        fader.set_value(None, value);
        fader.color_state(7 - scene)
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, velocity: i32) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down || ctx.model.current_track_bank().selected_item().is_none() {
            return;
        }
        virtual_fader::move_to(&self.fader, ctx, 7 - index as i32, velocity, None);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(session_scroll_states(ctx))
    }

    impl_as_any!();
}
