// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum sequencer view of the Launchpad (former `DrumView`).

use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use super::sequencer_common::{drum_scroll_states, handle_repeat_combination};
use crate::controller::launchpad::colors::{
    LAUNCHPAD_COLOR_BLACK, LAUNCHPAD_COLOR_GREEN, LAUNCHPAD_COLOR_PINK_HI, LAUNCHPAD_COLOR_PINK_LO, LAUNCHPAD_COLOR_SKY_HI, LAUNCHPAD_COLOR_SKY_LO, LAUNCHPAD_COLOR_YELLOW,
};
use crate::daw::clip::NotePosition;
use crate::daw::constants::Resolution;
use crate::daw::events::ModelEvent;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::framework::view::sequencer::abstract_drum_view::{DrumBase, DrumHooks};
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

/// The Launchpad specific behaviour of the drum sequencer.
#[derive(Default)]
struct Hooks {
    note_edit_position: Option<NotePosition>,
}

impl DrumHooks for Hooks {
    fn handle_sequencer_area(&mut self, base: &mut DrumBase, ctx: &mut Ctx, index: i32, x: i32, y: i32, offset_y: i32, velocity: i32) {
        // Toggle the note on up, so we can intercept the long presses
        if velocity != 0 {
            self.note_edit_position = None;
            return;
        }
        // Note: If the length of the note was changed this method will not be called since
        // button up was consumed! Therefore, always call edit note
        match self.note_edit_position {
            Some(position) => base.seq.edit_note(ctx, &position, false),
            None => base.handle_sequencer_area(self, ctx, index, x, y, offset_y, velocity),
        }
    }

    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        handle_repeat_combination(&base.seq, ctx, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, velocity)
    }

    fn handle_loop_area(&mut self, base: &mut DrumBase, ctx: &mut Ctx, pad: i32, velocity: i32) {
        if pad == 15 {
            if velocity > 0 {
                base.toggle_extra_buttons();
            }
            return;
        }
        base.handle_loop_area(ctx, pad, velocity);
    }
}

/// The drum sequencer view.
pub struct DrumView {
    base: DrumBase,
    hooks: Hooks,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl DrumView {
    pub fn new(_ctx: &Ctx) -> Self {
        let mut base = DrumBase::new("Drum", 4, 4, true).with_extra_buttons();
        base.button_select = ButtonId::Pad13;
        base.button_mute = ButtonId::Pad14;
        base.button_solo = ButtonId::Pad15;
        base.button_browse = ButtonId::Pad16;
        DrumView { base, hooks: Hooks::default(), aftertouch: Box::new(AftertouchViewCommand::new()) }
    }

    /// Get the selected drum pad (index in the page).
    pub fn selected_pad(&self) -> i32 {
        self.base.selected_pad()
    }

    /// Scroll the drum pad bank back to the page which was active when browsing started.
    pub fn reposition_bank_page(&mut self, ctx: &mut Ctx) {
        self.base.reposition_bank_page(ctx);
    }

    fn set_period(ctx: &mut Ctx, index: usize) {
        ctx.config.set_note_repeat_period(Resolution::ALL[index]);
        ctx.schedule(100, move |ctx| ctx.notify(&format!("Period: {}", Resolution::name_at(index))));
    }

    fn set_note_length(ctx: &mut Ctx, index: usize) {
        ctx.config.set_note_repeat_length(Resolution::ALL[index]);
        ctx.schedule(100, move |ctx| ctx.notify(&format!("Note Length: {}", Resolution::name_at(index))));
    }

    fn ex_flags(&self) -> (bool, bool) {
        self.base.ex.as_ref().map(|ex| (ex.note_repeat_period_on, ex.note_repeat_length_on)).unwrap_or_default()
    }
}

impl View for DrumView {
    fn name(&self) -> &str {
        self.base.seq.name()
    }

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(scene) = scene_index(button) else {
            return 0;
        };

        // The note repeat period and length are read from the configuration, which is the
        // source of the note repeat settings
        let (period_on, length_on) = self.ex_flags();
        if period_on {
            let period_index = Resolution::get_match(ctx.config.note_repeat_period().value());
            return if scene == 7 - period_index { LAUNCHPAD_COLOR_PINK_HI } else { LAUNCHPAD_COLOR_SKY_LO };
        }
        if length_on {
            let length_index = Resolution::get_match(ctx.config.note_repeat_length().value());
            return if scene == 7 - length_index { LAUNCHPAD_COLOR_SKY_HI } else { LAUNCHPAD_COLOR_PINK_LO };
        }

        if !self.base.seq.is_active() {
            return LAUNCHPAD_COLOR_BLACK;
        }
        if scene == 7 - self.base.seq.resolution_index(ctx) {
            LAUNCHPAD_COLOR_YELLOW
        } else {
            LAUNCHPAD_COLOR_GREEN
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.base.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut self.hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.seq.is_active() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let x = index % self.base.num_columns;
        let y = index / self.base.num_columns;
        let offset_y = ctx.model.scales().drum_offset();

        // Sequencer steps
        if y < self.base.play_rows {
            return;
        }

        // Remember the long pressed note to use it either for editing or for changing the
        // length of the note on pad release
        let step = self.base.num_columns * (self.base.all_rows - 1 - y) + x;
        self.hooks.note_edit_position = Some(NotePosition::new(ctx.config.midi_edit_channel(), step, offset_y + self.base.selected_pad()));
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let Some(scene) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        let (period_on, length_on) = self.ex_flags();
        if period_on {
            Self::set_period(ctx, 7 - scene);
            return;
        }
        if length_on {
            Self::set_note_length(ctx, 7 - scene);
            return;
        }
        self.base.seq.on_button(ctx, button, event);
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        self.base.update_control_surface(ctx);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.base.update_note_mapping(ctx);
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.base.on_model_event(ctx, event);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(drum_scroll_states(&self.base.seq, ctx))
    }

    fn on_sequencer_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq.on_left(ctx, event);
    }

    fn on_sequencer_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq.on_right(ctx, event);
    }

    crate::impl_transpose_view!();

    crate::impl_as_any!();
}

impl TransposeView for DrumView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_down(ctx, event);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_up(ctx, event);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        self.base.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.base.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.base.reset_octave(ctx);
    }
}
