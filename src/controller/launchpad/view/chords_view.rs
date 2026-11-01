// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The chords view of the Launchpad (former `ChordsView`).

use super::play_controls::PlayControls;
use super::play_view::{handle_scale_scene_button, scale_scroll_states};
use crate::config::SettingId;
use crate::controller::launchpad::colors::*;
use crate::daw::events::ModelEvent;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::base::is_scene_button;
use crate::framework::view::deferred::activate_preferred_view_later;
use crate::framework::view::play_view::ChordViewBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The name of the chords view.
pub const NAME_CHORDS: &str = "Chords";

/// The chords view. Each row plays a different chord type.
///
/// Note: the Java version has its own copy of the play controls code; it is the same as
/// [`PlayControls`], which is used here.
pub struct ChordsView {
    chords: ChordViewBase,
    play_controls: PlayControls,
}

impl ChordsView {
    pub fn new(_ctx: &Ctx) -> Self {
        ChordsView { chords: ChordViewBase::new(NAME_CHORDS, true), play_controls: PlayControls::new() }
    }
}

impl View for ChordsView {
    fn name(&self) -> &str {
        &self.chords.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.chords.on_activate(ctx, ChordViewBase::scale_matrix(ctx));
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if ctx.model.can_selected_track_hold_notes() {
            match button {
                ButtonId::Scene1 | ButtonId::Scene2 | ButtonId::Scene7 | ButtonId::Scene8 => return LAUNCHPAD_COLOR_OCEAN_HI,
                ButtonId::Scene4 => return self.play_controls.toggle_button_color(),
                ButtonId::Scene5 => return LAUNCHPAD_COLOR_AMBER_HI,
                _ => {}
            }
        }
        LAUNCHPAD_COLOR_BLACK
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down || !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        match button {
            ButtonId::Scene4 => {
                self.play_controls.toggle();
                self.chords.play.set_blocked_notes(if self.play_controls.is_active() { 8 } else { 0 });
            }
            ButtonId::Scene5 => {
                activate_preferred_view_later(ctx, Views::Play);
                ctx.notify("Chords: Off");
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
        self.chords.play.draw_grid(ctx);
        self.play_controls.draw(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !self.play_controls.handle_grid_notes(ctx, note, velocity) {
            self.chords.on_grid_note(ctx, note, velocity);
        }
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let chord = self.chords.chord_name();
        self.chords.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.chords.play.update_note_mapping(ctx, ChordViewBase::scale_matrix(ctx));
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.chords.play.on_model_event(ctx, event);
    }

    fn on_setting_changed(&mut self, ctx: &mut Ctx, id: SettingId) {
        if id == SettingId::ACTIVATE_FIXED_ACCENT || id == SettingId::FIXED_ACCENT_VALUE {
            self.chords.play.init_max_velocity(ctx);
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(scale_scroll_states(ctx))
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for ChordsView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.chords.play.on_octave_down(ctx, event, ChordViewBase::scale_matrix);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.chords.play.on_octave_up(ctx, event, ChordViewBase::scale_matrix);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        self.chords.play.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.chords.play.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.chords.play.reset_octave(ctx, ChordViewBase::scale_matrix);
    }
}
