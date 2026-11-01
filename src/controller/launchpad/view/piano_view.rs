// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The piano view of the Launchpad (former `PianoView`).

use super::play_controls::PlayControls;
use crate::config::SettingId;
use crate::controller::launchpad::colors::*;
use crate::daw::events::ModelEvent;
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::is_scene_button;
use crate::framework::view::play_view::{draw_piano_grid, PlayViewBase};
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::NoteMatrix;

/// The name of the piano view.
pub const NAME_PIANO: &str = "Piano";

fn piano_matrix(ctx: &Ctx) -> NoteMatrix {
    ctx.model.scales().piano_matrix(8, 8)
}

/// The piano view.
pub struct PianoView {
    play: PlayViewBase,
    play_controls: PlayControls,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl PianoView {
    pub fn new(_ctx: &Ctx) -> Self {
        PianoView { play: PlayViewBase::new(NAME_PIANO, true), play_controls: PlayControls::new(), aftertouch: Box::new(AftertouchViewCommand::new()) }
    }
}

impl View for PianoView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.play.on_activate(ctx, piano_matrix(ctx));
    }

    fn button_color(&self, _ctx: &Ctx, button: ButtonId) -> i32 {
        if button == ButtonId::Scene4 {
            self.play_controls.toggle_button_color()
        } else {
            LAUNCHPAD_COLOR_BLACK
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down || !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        if button == ButtonId::Scene4 {
            self.play_controls.toggle();
            self.play.set_blocked_notes(if self.play_controls.is_active() { 8 } else { 0 });
            self.update_note_mapping(ctx);
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let is_active = self.play_controls.is_active();
        draw_piano_grid(ctx, &self.play.base.key_manager, if is_active { 36 + 8 } else { 36 }, if is_active { 7 } else { 8 }, 8);
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
        self.play.update_note_mapping(ctx, piano_matrix(ctx));
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.play.on_model_event(ctx, event);
    }

    fn on_setting_changed(&mut self, ctx: &mut Ctx, id: SettingId) {
        if id == SettingId::ACTIVATE_FIXED_ACCENT || id == SettingId::FIXED_ACCENT_VALUE {
            self.play.init_max_velocity(ctx);
        }
    }

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let piano_octave = ctx.model.scales().piano_octave();
        Some(ScrollStates { can_scroll_left: false, can_scroll_right: false, can_scroll_up: piano_octave < 3, can_scroll_down: piano_octave > -3 })
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PianoView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
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
        self.play.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.play.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        // Not overridden in Java: resets the octave of the scales
        self.play.reset_octave(ctx, piano_matrix);
    }
}
