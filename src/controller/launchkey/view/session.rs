// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The session view of the Launchkey Mk3 and Mini Mk3 (former `SessionView` of both packages):
//! 2 scenes of 8 clips. With a pad mode (selected in the pad mode select view) the lower row
//! controls rec arm, select, mute, solo or stop of the tracks.
//!
//! Mk3: Repeat + pad launches the clip with the alternative function, Stop + pad stops the
//! track, Undo + pad deletes the clip, Play + pad returns the track to the arrangement; the
//! arrow buttons scroll the scenes. Mini: Shift + scene button scrolls the scenes.

use super::super::colors::*;
use super::super::surface::{after_view, state};
use crate::daw::bank::BankItem;
use crate::daw::scene::Scene;
use crate::framework::color_ids::abstract_session_view::{COLOR_SCENE, COLOR_SCENE_OFF, COLOR_SELECTED_SCENE};
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::grid::LightInfo;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::view::base::scene_index;
use crate::framework::view::session_view::{SessionColors, SessionHooks, SessionViewBase};
use crate::midi::ButtonEvent;

/// The pad modes of the lower row (former `PAD_MODES`).
pub const PAD_MODES: [Modes; 5] = [Modes::RecArm, Modes::TrackSelect, Modes::Mute, Modes::Solo, Modes::StopClip];

/// The hooks of the session base.
struct LaunchkeySessionHooks {
    is_mini: bool,
}

impl SessionHooks for LaunchkeySessionHooks {
    fn is_alternate_function(&self, ctx: &Ctx) -> bool {
        // Mk3: the Repeat button (consumed in SessionView::is_alternate_function)
        if self.is_mini {
            ctx.surface.is_shift_pressed()
        } else {
            ctx.surface.is_pressed(ButtonId::Repeat)
        }
    }
}

/// The session view.
pub struct SessionView {
    base: SessionViewBase,
    hooks: LaunchkeySessionHooks,
}

impl SessionView {
    pub fn new(is_mini: bool) -> Self {
        let mut base = SessionViewBase::new(2, 8, true);
        base.clip_colors = SessionColors {
            is_recording: LightInfo::new(LAUNCHKEY_COLOR_RED, LAUNCHKEY_COLOR_RED, false),
            is_recording_queued: LightInfo::new(LAUNCHKEY_COLOR_RED_LO, LAUNCHKEY_COLOR_RED_LO, true),
            is_playing: LightInfo::new(LAUNCHKEY_COLOR_GREEN_SPRING, LAUNCHKEY_COLOR_GREEN, false),
            is_playing_queued: LightInfo::new(LAUNCHKEY_COLOR_GREEN_SPRING, LAUNCHKEY_COLOR_GREEN, true),
            is_stop_queued: LightInfo::new(LAUNCHKEY_COLOR_GREEN_SPRING, LAUNCHKEY_COLOR_GREEN, true),
            has_content: LightInfo::new(LAUNCHKEY_COLOR_AMBER, -1, false),
            has_no_content: LightInfo::new(LAUNCHKEY_COLOR_BLACK, -1, false),
            is_rec_armed: LightInfo::new(LAUNCHKEY_COLOR_RED_LO, -1, false),
            is_muted: LightInfo::new(LAUNCHKEY_COLOR_GREY_LO, -1, false),
        };
        SessionView { base, hooks: LaunchkeySessionHooks { is_mini } }
    }

    fn pad_mode(ctx: &Ctx) -> Option<Modes> {
        state(ctx).session_pad_mode
    }

    /// Get the track (x) and slot (y) index of a pad; with a pad mode the upper row is slot 0
    /// and the lower row is -1 (former `getPad`).
    fn pad(&self, ctx: &Ctx, note: i32) -> (i32, i32) {
        let index = note - ctx.surface.grid_ref().start_note();
        let t = index % self.base.columns;
        let s = self.base.rows - 1 - index / self.base.columns;
        let (x, y) = if ctx.config.is_flip_session() { (s, t) } else { (t, s) };
        if Self::pad_mode(ctx).is_some() {
            return (x, if y == 0 { -1 } else { 0 });
        }
        (x, y)
    }

    /// Former `isAlternateFunction`: Repeat on the Mk3 (marked as consumed), Shift on the Mini.
    fn is_alternate_function(&self, ctx: &mut Ctx) -> bool {
        if self.hooks.is_mini {
            return ctx.surface.is_shift_pressed();
        }
        let is_pressed = ctx.surface.is_pressed(ButtonId::Repeat);
        if is_pressed {
            ctx.surface.set_trigger_consumed(ButtonId::Repeat);
        }
        is_pressed
    }

    /// Former `AbstractSessionView.onGridNote` with the `handleButtonCombinations` of the Mk3.
    fn launch_or_record(&mut self, ctx: &mut Ctx, track_index: usize, slot_index: usize, velocity: i32) {
        let is_alternate_function = self.is_alternate_function(ctx);
        if is_alternate_function {
            self.base.set_alternate_interaction_used(true);
        }

        let is_pressed = velocity != 0;
        if is_pressed {
            let mut handled = self.base.handle_button_combinations(ctx, &self.hooks, track_index, slot_index);
            if !handled && !self.hooks.is_mini && ctx.is_button_combination(ButtonId::Play) {
                ctx.model.current_track_bank_mut().item_mut(track_index).return_to_arrangement();
                handled = true;
            }
            if handled {
                ctx.surface.consume_pads();
                return;
            }
            if self.hooks.do_select_clip_on_launch(ctx) {
                ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).select();
            }
        }

        let track = ctx.model.current_track_bank().item(track_index);
        if !track.is_rec_arm() || track.slot_bank().item(slot_index).has_content() {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).launch(is_pressed, is_alternate_function);
            return;
        }
        SessionViewBase::handle_recording(ctx, track_index, slot_index);
    }

    /// The function of a pad of the lower row with a pad mode (former `handleFirstRowModes`).
    fn handle_first_row_modes(&mut self, ctx: &mut Ctx, column: usize) {
        let Some(pad_mode) = Self::pad_mode(ctx) else {
            return;
        };
        match pad_mode {
            Modes::RecArm => ctx.model.current_track_bank_mut().item_mut(column).toggle_rec_arm(),
            Modes::TrackSelect => self.select_track(ctx, column),
            Modes::Mute => ctx.model.current_track_bank_mut().item_mut(column).toggle_mute(),
            Modes::Solo => ctx.model.current_track_bank_mut().item_mut(column).toggle_solo(),
            Modes::StopClip => ctx.model.current_track_bank_mut().item_mut(column).stop(false),
            _ => {}
        }
    }

    /// The color index of the state of a scene (former `getSceneColor`).
    fn scene_color(ctx: &Ctx, scene: &Scene) -> i32 {
        ctx.colors.color_index(if !scene.does_exist() {
            COLOR_SCENE_OFF
        } else if scene.is_selected() {
            COLOR_SELECTED_SCENE
        } else {
            COLOR_SCENE
        })
    }

    /// The color of the second scene button with a pad mode.
    fn pad_mode_color(pad_mode: Modes) -> i32 {
        match pad_mode {
            Modes::RecArm => LAUNCHKEY_COLOR_RED_HI,
            Modes::TrackSelect => LAUNCHKEY_COLOR_WHITE,
            Modes::Mute => LAUNCHKEY_COLOR_AMBER_HI,
            Modes::Solo => LAUNCHKEY_COLOR_YELLOW_HI,
            Modes::StopClip => LAUNCHKEY_COLOR_ROSE,
            _ => LAUNCHKEY_COLOR_BLACK,
        }
    }

    fn launch_scene(ctx: &mut Ctx, index: usize) {
        let mut scene_bank = ctx.model.scene_bank_mut();
        let scene = scene_bank.item_mut(index);
        scene.select();
        scene.launch(true, false);
    }
}

impl View for SessionView {
    fn name(&self) -> &str {
        "Session"
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let scene_bank = ctx.model.scene_bank();
        if !self.hooks.is_mini {
            if button == ButtonId::ArrowUp || button == ButtonId::ArrowDown {
                let is_on = if button == ButtonId::ArrowUp { scene_bank.can_scroll_page_backwards() } else { scene_bank.can_scroll_page_forwards() };
                return if is_on { LAUNCHKEY_COLOR_GREEN } else { LAUNCHKEY_COLOR_GREY_LO };
            }
            if scene_index(button).is_none() {
                return LAUNCHKEY_COLOR_BLACK;
            }
        }
        if button == ButtonId::Scene1 {
            return Self::scene_color(ctx, scene_bank.item(0));
        }
        // SCENE 2
        match Self::pad_mode(ctx) {
            None => Self::scene_color(ctx, scene_bank.item(1)),
            Some(pad_mode) => Self::pad_mode_color(pad_mode),
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let pad_mode = Self::pad_mode(ctx);
        self.base.rows = if pad_mode.is_none() { 2 } else { 1 };
        self.base.draw_grid(ctx, &self.hooks, false);
        let Some(pad_mode) = pad_mode else {
            return;
        };

        for x in 0..self.base.columns {
            let track = ctx.model.current_track_bank().item(x as usize);
            let exists = track.does_exist();
            let color = match pad_mode {
                // The Mk3 turns the pads of not existing tracks off for all modes
                _ if !exists => LAUNCHKEY_COLOR_BLACK,
                Modes::RecArm => {
                    if track.is_rec_arm() {
                        LAUNCHKEY_COLOR_RED_HI
                    } else {
                        LAUNCHKEY_COLOR_RED_LO
                    }
                }
                Modes::TrackSelect => {
                    if track.is_selected() {
                        LAUNCHKEY_COLOR_WHITE
                    } else {
                        LAUNCHKEY_COLOR_GREY_LO
                    }
                }
                Modes::Mute => {
                    if track.is_mute() {
                        LAUNCHKEY_COLOR_AMBER_HI
                    } else {
                        LAUNCHKEY_COLOR_AMBER_LO
                    }
                }
                Modes::Solo => {
                    if track.is_solo() {
                        LAUNCHKEY_COLOR_YELLOW_HI
                    } else {
                        LAUNCHKEY_COLOR_YELLOW_LO
                    }
                }
                Modes::StopClip => LAUNCHKEY_COLOR_ROSE,
                _ => continue,
            };
            ctx.surface.grid().light_ex(x, 1, color);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let (column, row) = self.pad(ctx, note);
        if row == 0 || Self::pad_mode(ctx).is_none() {
            if row < 0 || column < 0 {
                return;
            }
            let (column, row) = (column as usize, row as usize);
            if !self.hooks.is_mini {
                // Stop clip with normal stop button
                if ctx.is_button_combination(ButtonId::Stop) {
                    ctx.model.current_track_bank_mut().item_mut(column).stop(false);
                    return;
                }
                // Use Undo button for delete
                if ctx.is_button_combination(ButtonId::Undo) {
                    ctx.model.current_track_bank_mut().item_mut(column).slot_bank_mut().item_mut(row).remove();
                    return;
                }
            }
            self.launch_or_record(ctx, column, row, velocity);
        } else if velocity != 0 && column >= 0 {
            self.handle_first_row_modes(ctx, column as usize);
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !self.hooks.is_mini && (button == ButtonId::ArrowUp || button == ButtonId::ArrowDown) {
            if event != ButtonEvent::Up {
                return;
            }
            let mut scene_bank = ctx.model.scene_bank_mut();
            if button == ButtonId::ArrowUp {
                scene_bank.scroll_backwards();
            } else {
                scene_bank.scroll_forwards();
            }
            return;
        }

        let Some(index) = scene_index(button) else {
            return;
        };

        if self.hooks.is_mini && ctx.surface.is_shift_pressed() {
            if event != ButtonEvent::Up {
                return;
            }
            let has_pad_mode = Self::pad_mode(ctx).is_some();
            let mut scene_bank = ctx.model.scene_bank_mut();
            match (index == 0, has_pad_mode) {
                (true, false) => scene_bank.select_previous_page(),
                (true, true) => scene_bank.scroll_backwards(),
                (false, false) => scene_bank.select_next_page(),
                (false, true) => scene_bank.scroll_forwards(),
            }
            return;
        }

        match event {
            ButtonEvent::Down => {
                if index == 1 {
                    after_view(ctx, |ctx| ctx.set_active_view(Views::Control));
                }
            }
            ButtonEvent::Up => Self::launch_scene(ctx, index),
            ButtonEvent::Long => {}
        }
    }

    crate::impl_as_any!();
}
