// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum view with 64 pads of the Launchpad (former `Drum64View`).

use crate::daw::events::ModelEvent;
use crate::framework::color::BUTTON_STATE_OFF;
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::drum64_view::Drum64ViewBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The drum view with 64 pads.
pub struct Drum64View {
    drum: Drum64ViewBase,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl Drum64View {
    pub fn new(_ctx: &Ctx) -> Self {
        Drum64View { drum: Drum64ViewBase::new(8, 8), aftertouch: Box::new(AftertouchViewCommand::new()) }
    }

    pub fn drum_octave(&self) -> i32 {
        self.drum.drum_octave()
    }
}

impl View for Drum64View {
    fn name(&self) -> &str {
        &self.drum.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.drum.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.drum.on_deactivate(ctx);
    }

    fn button_color_id(&self, _ctx: &Ctx, _button: ButtonId) -> &'static str {
        BUTTON_STATE_OFF
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.drum.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if let Some(played_pad) = self.drum.on_grid_note(ctx, note, velocity) {
            self.drum.handle_button_combinations(ctx, played_pad, |drum, ctx, played_pad| {
                // Former handleDeleteButton
                ctx.surface.set_trigger_consumed(ButtonId::Delete);
                let edit_midi_channel = ctx.config.midi_edit_channel();
                ctx.model.note_clip(8, 128).clear_row(edit_midi_channel, drum.offset_y + played_pad);
            });
        }
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.drum.update_note_mapping(ctx);
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.drum.on_model_event(ctx, event);
    }

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    fn scroll_states(&self, _ctx: &Ctx) -> Option<ScrollStates> {
        let drum_octave = self.drum.drum_octave();
        Some(ScrollStates { can_scroll_left: false, can_scroll_right: false, can_scroll_up: drum_octave < 1, can_scroll_down: drum_octave > -2 })
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for Drum64View {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.drum.on_octave_down(ctx, event);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.drum.on_octave_up(ctx, event);
    }

    fn is_octave_up_button_on(&self, _ctx: &Ctx) -> bool {
        self.drum.is_octave_up_button_on()
    }

    fn is_octave_down_button_on(&self, _ctx: &Ctx) -> bool {
        self.drum.is_octave_down_button_on()
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.drum.reset_octave(ctx);
    }
}
