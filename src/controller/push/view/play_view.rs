// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The play view and the piano view of the Push (former `PlayView` and `PianoView`, which extends
//! the play view). Both are expression views on the Push 3 (former `IExpressionView`).

use super::{on_scene_button_with_repeat, scene_button_color_id};
use crate::config::SettingId;
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::is_scene_button;
use crate::framework::view::play_view::{draw_piano_grid, play_scale_matrix, PlayViewBase, ScaleMatrixFn};
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::{NoteMatrix, Scales};

/// The name of the play view.
pub const NAME_PLAY: &str = "Play";
/// The name of the piano view.
pub const NAME_PIANO: &str = "Piano";

fn piano_matrix(ctx: &Ctx) -> NoteMatrix {
    ctx.model.scales().piano_matrix(8, 8)
}

/// The play view (or the piano view).
pub struct PlayView {
    pub play: PlayViewBase,
    is_piano: bool,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl PlayView {
    /// Former `PlayView`.
    pub fn new_play() -> Self {
        PlayView { play: PlayViewBase::new(NAME_PLAY, true), is_piano: false, aftertouch: Box::new(AftertouchViewCommand::new()) }
    }

    /// Former `PianoView`.
    pub fn new_piano() -> Self {
        PlayView { play: PlayViewBase::new(NAME_PIANO, true), is_piano: true, aftertouch: Box::new(AftertouchViewCommand::new()) }
    }

    /// True for the piano view.
    pub fn is_piano(&self) -> bool {
        self.is_piano
    }

    fn scale_matrix_fn(&self) -> ScaleMatrixFn {
        if self.is_piano {
            piano_matrix
        } else {
            play_scale_matrix
        }
    }
}

/// Former `PlayView.onGridNote` (and the same of `ChordsView`): Delete + pad clears the notes of
/// the row in the clip. Returns true if handled.
pub fn handle_delete_pad(ctx: &mut Ctx, key_manager: &crate::scale::KeyManager, note: i32) -> bool {
    if !ctx.is_button_combination(ButtonId::Delete) {
        return false;
    }
    let edit_midi_channel = ctx.config.midi_edit_channel();
    let row = key_manager.map(note);
    ctx.model.note_clip(8, 128).clear_row(edit_midi_channel, row);
    true
}

impl View for PlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        let matrix = (self.scale_matrix_fn())(ctx);
        self.play.on_activate(ctx, matrix);
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
        if self.is_piano {
            draw_piano_grid(ctx, &self.play.base.key_manager, 36, 8, 8);
        } else {
            self.play.draw_grid(ctx);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if handle_delete_pad(ctx, &self.play.base.key_manager, note) {
            return;
        }
        self.play.on_grid_note(ctx, note, velocity);
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let chord = self.play.chord_name();
        self.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let matrix = (self.scale_matrix_fn())(ctx);
        self.play.update_note_mapping(ctx, matrix);
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

    fn is_expression_view(&self) -> bool {
        true
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PlayView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if !self.is_piano {
            self.play.on_octave_down(ctx, event, play_scale_matrix);
            return;
        }
        if event != ButtonEvent::Down {
            return;
        }
        self.play.base.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().dec_piano_octave();
        self.update_note_mapping(ctx);
        let text = ctx.model.scales().piano_range_text();
        ctx.notify(&text);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if !self.is_piano {
            self.play.on_octave_up(ctx, event, play_scale_matrix);
            return;
        }
        if event != ButtonEvent::Down {
            return;
        }
        self.play.base.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().inc_piano_octave();
        self.update_note_mapping(ctx);
        let text = ctx.model.scales().piano_range_text();
        ctx.notify(&text);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        if self.is_piano {
            return ctx.model.scales().piano_octave() < Scales::PIANO_OCTAVE_RANGE;
        }
        self.play.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        if self.is_piano {
            return ctx.model.scales().piano_octave() > -Scales::PIANO_OCTAVE_RANGE;
        }
        self.play.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        let matrix_fn = self.scale_matrix_fn();
        self.play.reset_octave(ctx, matrix_fn);
    }
}
