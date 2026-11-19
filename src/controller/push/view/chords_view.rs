// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The chords view of the Push (former `ChordsView`).

use super::play_view::handle_delete_pad;
use super::{on_scene_button_with_repeat, scene_button_color_id};
use crate::config::SettingId;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::is_scene_button;
use crate::framework::view::play_view::ChordViewBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The name of the chords view.
pub const NAME_CHORDS: &str = "Chords";

/// The chords view.
pub struct ChordsView {
    pub chords: ChordViewBase,
}

impl Default for ChordsView {
    fn default() -> Self {
        Self::new()
    }
}

impl ChordsView {
    pub fn new() -> Self {
        ChordsView { chords: ChordViewBase::new(NAME_CHORDS, true) }
    }
}

impl View for ChordsView {
    fn name(&self) -> &str {
        &self.chords.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.chords.on_activate(ctx, ChordViewBase::scale_matrix(ctx));
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        scene_button_color_id(ctx, button)
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if is_scene_button(button) {
            on_scene_button_with_repeat(ctx, button, event);
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.chords.play.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if handle_delete_pad(ctx, &self.chords.play.base.key_manager, note) {
            return;
        }
        self.chords.on_grid_note(ctx, note, velocity);
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let chord = self.chords.chord_name();
        self.chords.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.chords.play.update_note_mapping(ctx, ChordViewBase::scale_matrix(ctx));
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &crate::daw::events::ModelEvent) {
        self.chords.play.on_model_event(ctx, event);
    }

    fn on_setting_changed(&mut self, ctx: &mut Ctx, id: SettingId) {
        if id == SettingId::ACTIVATE_FIXED_ACCENT || id == SettingId::FIXED_ACCENT_VALUE {
            self.chords.play.init_max_velocity(ctx);
        }
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
