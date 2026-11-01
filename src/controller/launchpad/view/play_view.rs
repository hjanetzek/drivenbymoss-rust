// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The play view of the Launchpad (former `PlayView`).

use super::play_controls::PlayControls;
use crate::config::SettingId;
use crate::controller::launchpad::colors::*;
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::base::is_scene_button;
use crate::framework::view::deferred::activate_preferred_view_later;
use crate::framework::view::play_view::{play_scale_matrix, PlayViewBase};
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::{Scale, Scales};

/// The name of the play view.
pub const NAME_PLAY: &str = "Play";

/// Handle the scene buttons which change the scale settings (scene 1, 2, 6, 7 and 8) of the
/// play and chord views. Returns false if the button is none of them.
pub(crate) fn handle_scale_scene_button(ctx: &mut Ctx, button: ButtonId) -> bool {
    match button {
        ButtonId::Scene1 | ButtonId::Scene2 => {
            if button == ButtonId::Scene1 {
                ctx.model.scales_mut().next_scale_layout();
            } else {
                ctx.model.scales_mut().prev_scale_layout();
            }
            let name = ctx.model.scales().scale_layout().name();
            ctx.config.set_scale_layout(name);
            ctx.notify(name);
        }
        ButtonId::Scene6 => {
            ctx.model.scales_mut().toggle_chromatic();
            let is_chromatic = ctx.model.scales().is_chromatic();
            ctx.config.set_scale_in_key(!is_chromatic);
            ctx.notify(if is_chromatic { "Chromatic" } else { "In Key" });
        }
        ButtonId::Scene7 | ButtonId::Scene8 => {
            let index = ctx.model.scales().scale_offset_index() as i32;
            ctx.model.scales_mut().set_scale_offset_by_index(if button == ButtonId::Scene7 { index + 1 } else { index - 1 });
            let name = Scales::BASES[ctx.model.scales().scale_offset_index()];
            ctx.config.set_scale_base(name);
            ctx.notify(name);
        }
        _ => return false,
    }
    true
}

/// The scroll states of the play and chord views.
pub(crate) fn scale_scroll_states(ctx: &Ctx) -> ScrollStates {
    let octave = ctx.model.scales().octave();
    let scale = ctx.model.scales().scale().ordinal();
    ScrollStates { can_scroll_left: scale > 0, can_scroll_right: scale < Scale::ALL.len() - 1, can_scroll_up: octave < 3, can_scroll_down: octave > -3 }
}

/// The play view.
pub struct PlayView {
    pub(crate) play: PlayViewBase,
    pub(crate) play_controls: PlayControls,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl PlayView {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name(NAME_PLAY)
    }

    pub fn with_name(name: &str) -> Self {
        PlayView { play: PlayViewBase::new(name, true), play_controls: PlayControls::new(), aftertouch: Box::new(AftertouchViewCommand::new()) }
    }
}

impl View for PlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.play.on_activate(ctx, play_scale_matrix(ctx));
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if !ctx.model.can_selected_track_hold_notes() {
            return LAUNCHPAD_COLOR_BLACK;
        }
        match button {
            ButtonId::Scene1 | ButtonId::Scene2 | ButtonId::Scene7 | ButtonId::Scene8 => LAUNCHPAD_COLOR_OCEAN_HI,
            ButtonId::Scene4 => self.play_controls.toggle_button_color(),
            ButtonId::Scene5 => LAUNCHPAD_COLOR_AMBER_LO,
            ButtonId::Scene6 => {
                if ctx.model.scales().is_chromatic() {
                    LAUNCHPAD_COLOR_WHITE
                } else {
                    LAUNCHPAD_COLOR_GREY_LO
                }
            }
            _ => LAUNCHPAD_COLOR_BLACK,
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down || !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        match button {
            ButtonId::Scene4 => {
                self.play_controls.toggle();
                self.play.set_blocked_notes(if self.play_controls.is_active() { 8 } else { 0 });
            }
            ButtonId::Scene5 => {
                activate_preferred_view_later(ctx, Views::Chords);
                ctx.notify("Chords: On");
                // Do not update note map!
                return;
            }
            _ => {
                handle_scale_scene_button(ctx, button);
            }
        }
        self.update_note_mapping(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.play.draw_grid(ctx);
        self.play_controls.draw(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !self.play_controls.handle_grid_notes(ctx, note, velocity) {
            self.play.on_grid_note(ctx, note, velocity);
        }
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let chord = self.play.chord_name();
        self.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.play.update_note_mapping(ctx, play_scale_matrix(ctx));
    }

    // AbstractPlayView
    crate::forward_view!(play => on_model_event);

    fn on_setting_changed(&mut self, ctx: &mut Ctx, id: SettingId) {
        if id == SettingId::ACTIVATE_FIXED_ACCENT || id == SettingId::FIXED_ACCENT_VALUE {
            self.play.init_max_velocity(ctx);
        }
    }

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(scale_scroll_states(ctx))
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PlayView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.on_octave_down(ctx, event, play_scale_matrix);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.on_octave_up(ctx, event, play_scale_matrix);
    }

    // AbstractPlayView
    crate::forward_transpose_view!(play => is_octave_up_button_on, is_octave_down_button_on);

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.play.reset_octave(ctx, play_scale_matrix);
    }
}
