// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The play, piano and chord views of the Maschine JAM (former `PlayView`, `PianoView` and
//! `ChordsView`).

use super::{change_scale_option, update_scale_properties, MaschineView};
use crate::config::SettingId;
use crate::controller::maschine::core::encoder_mode_manager::EncoderMode;
use crate::daw::clip::SessionAlternative;
use crate::daw::events::ModelEvent;
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::play_view::{draw_piano_grid, play_scale_matrix, ChordViewBase, PlayViewBase};
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::{NoteMatrix, Scales};

pub const NAME_PLAY: &str = "Play";
pub const NAME_PIANO: &str = "Piano";
pub const NAME_CHORDS: &str = "Chords";

fn piano_matrix(ctx: &Ctx) -> NoteMatrix {
    ctx.model.scales().piano_matrix(8, 8)
}

/// The play view and the piano view (former `PlayView` and its sub-class `PianoView`).
pub struct PlayView {
    play: PlayViewBase,
    is_piano: bool,
}

impl PlayView {
    pub fn new(_ctx: &Ctx) -> Self {
        PlayView { play: PlayViewBase::new(NAME_PLAY, true), is_piano: false }
    }

    pub fn new_piano(_ctx: &Ctx) -> Self {
        PlayView { play: PlayViewBase::new(NAME_PIANO, true), is_piano: true }
    }

    fn matrix(&self, ctx: &Ctx) -> NoteMatrix {
        if self.is_piano {
            piano_matrix(ctx)
        } else {
            play_scale_matrix(ctx)
        }
    }

    /// Former `updateScale`.
    fn update_scale(&mut self, ctx: &mut Ctx) {
        self.update_note_mapping(ctx);
        update_scale_properties(ctx);
    }
}

impl View for PlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        let matrix = self.matrix(ctx);
        self.play.on_activate(ctx, matrix);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if self.is_piano {
            draw_piano_grid(ctx, &self.play.base.key_manager, 36, 8, 8);
        } else {
            self.play.draw_grid(ctx);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if ctx.is_button_combination(ButtonId::Delete) {
            let edit_midi_channel = ctx.config.midi_edit_channel();
            let row = self.play.base.key_manager.map(note);
            ctx.model.note_clip(8, 128).clear_row(edit_midi_channel, row);
            return;
        }
        self.play.on_grid_note(ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        match button {
            ButtonId::ArrowLeft | ButtonId::ArrowRight => {
                if button == ButtonId::ArrowLeft {
                    ctx.model.scales_mut().prev_scale_layout();
                } else {
                    ctx.model.scales_mut().next_scale_layout();
                }
                self.update_scale(ctx);
                delay_display(ctx, |ctx| format!("Scale Layout: {}", ctx.model.scales().scale_layout().name()));
            }
            ButtonId::ArrowUp => self.on_octave_up(ctx, event),
            ButtonId::ArrowDown => self.on_octave_down(ctx, event),
            _ => {}
        }
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let chord = self.play.chord_name();
        self.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let matrix = self.matrix(ctx);
        self.play.update_note_mapping(ctx, matrix);
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.play.on_model_event(ctx, event);
    }

    fn on_setting_changed(&mut self, ctx: &mut Ctx, id: SettingId) {
        if id == SettingId::ACTIVATE_FIXED_ACCENT || id == SettingId::FIXED_ACCENT_VALUE {
            self.play.init_max_velocity(ctx);
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let scales = ctx.model.scales();
        Some(ScrollStates {
            can_scroll_left: scales.has_prev_scale_layout(),
            can_scroll_right: scales.has_next_scale_layout(),
            can_scroll_up: self.is_octave_up_button_on(ctx),
            can_scroll_down: self.is_octave_down_button_on(ctx),
        })
    }

    fn session_alternative(&self) -> Option<&dyn SessionAlternative> {
        Some(self)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn SessionAlternative> {
        Some(self)
    }

    fn as_transpose_view(&self) -> Option<&dyn TransposeView> {
        Some(self)
    }

    fn as_transpose_view_mut(&mut self) -> Option<&mut dyn TransposeView> {
        Some(self)
    }

    impl_as_any!();
}

impl SessionAlternative for PlayView {
    fn was_alternate_interaction_used(&self) -> bool {
        false
    }

    fn set_alternate_interaction_used(&mut self, _was_used: bool) {}
}

impl MaschineView for PlayView {
    fn change_option(&mut self, ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32) {
        self.play.base.key_manager.clear_pressed_keys();
        let increase = ctx.model.value_changer().is_increase(control);
        if !change_scale_option(ctx, temporary_encoder_mode, increase) && temporary_encoder_mode == EncoderMode::TemporaryTune {
            if increase {
                ctx.model.scales_mut().inc_octave();
            } else {
                ctx.model.scales_mut().dec_octave();
            }
            delay_display(ctx, |ctx| format!("Octave: {}", ctx.model.scales().octave()));
        }
        self.update_scale(ctx);
    }
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
            ctx.model.scales().piano_octave() < Scales::PIANO_OCTAVE_RANGE
        } else {
            self.play.is_octave_up_button_on(ctx)
        }
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        if self.is_piano {
            ctx.model.scales().piano_octave() > -Scales::PIANO_OCTAVE_RANGE
        } else {
            self.play.is_octave_down_button_on(ctx)
        }
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        if self.is_piano {
            self.play.reset_octave(ctx, piano_matrix);
        } else {
            self.play.reset_octave(ctx, play_scale_matrix);
        }
    }
}

/// The chord view (former `ChordsView`).
pub struct ChordsView {
    chords: ChordViewBase,
}

impl ChordsView {
    pub fn new(_ctx: &Ctx) -> Self {
        ChordsView { chords: ChordViewBase::new(NAME_CHORDS, true) }
    }

    fn update_scale(&mut self, ctx: &mut Ctx) {
        self.update_note_mapping(ctx);
        update_scale_properties(ctx);
    }
}

impl View for ChordsView {
    fn name(&self) -> &str {
        &self.chords.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.chords.play.on_activate(ctx, ChordViewBase::scale_matrix(ctx));
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.chords.play.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.chords.on_grid_note(ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        match button {
            ButtonId::ArrowUp => self.on_octave_up(ctx, event),
            ButtonId::ArrowDown => self.on_octave_down(ctx, event),
            _ => {}
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
        Some(super::up_down_scroll_states(self.is_octave_up_button_on(ctx), self.is_octave_down_button_on(ctx)))
    }

    fn session_alternative(&self) -> Option<&dyn SessionAlternative> {
        Some(self)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn SessionAlternative> {
        Some(self)
    }

    fn as_transpose_view(&self) -> Option<&dyn TransposeView> {
        Some(self)
    }

    fn as_transpose_view_mut(&mut self) -> Option<&mut dyn TransposeView> {
        Some(self)
    }

    impl_as_any!();
}

impl SessionAlternative for ChordsView {
    fn was_alternate_interaction_used(&self) -> bool {
        false
    }

    fn set_alternate_interaction_used(&mut self, _was_used: bool) {}
}

impl MaschineView for ChordsView {
    fn change_option(&mut self, ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32) {
        self.chords.play.base.key_manager.clear_pressed_keys();
        let increase = ctx.model.value_changer().is_increase(control);
        if !change_scale_option(ctx, temporary_encoder_mode, increase) && temporary_encoder_mode == EncoderMode::TemporaryTune {
            if increase {
                ctx.model.scales_mut().inc_octave();
            } else {
                ctx.model.scales_mut().dec_octave();
            }
            delay_display(ctx, |ctx| format!("Octave: {}", ctx.model.scales().octave()));
        }
        self.update_scale(ctx);
    }
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
