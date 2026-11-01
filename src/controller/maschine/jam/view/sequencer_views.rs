// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note sequencer views of the Maschine JAM (former `SequencerView`, `PolySequencerView` and
//! `RaindropsView`).

use super::{change_scale_option, update_scale_properties, MaschineView};
use crate::controller::maschine::core::encoder_mode_manager::EncoderMode;
use crate::daw::clip::{NotePosition, SessionAlternative};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::sequencer::abstract_note_sequencer_view::{NoteSequencerBase, NoteSequencerHooks};
use crate::framework::view::sequencer::abstract_poly_sequencer_view::{DefaultPolySequencerHooks, PolySequencerBase};
use crate::framework::view::sequencer::abstract_raindrops_view::RaindropsBase;
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

pub const NAME_SEQUENCER: &str = "Sequencer";
pub const NAME_RAINDROPS: &str = "Raindrops";

fn clip_scroll(seq: &SequencerBase, ctx: &Ctx) -> (bool, bool) {
    match seq.clip_ref(ctx) {
        Some(clip) => (clip.can_scroll_steps_backwards(), clip.can_scroll_steps_forwards()),
        None => (false, false),
    }
}

struct SequencerHooks;

impl NoteSequencerHooks for SequencerHooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32) -> bool {
        let is_select_pressed = ctx.surface.is_select_pressed();
        if is_select_pressed {
            if velocity > 0 {
                base.seq.handle_sequencer_area_repeat_operator(ctx, note_position, velocity, is_select_pressed);
            }
            return true;
        }
        base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity)
    }
}

/// The note sequencer (former `SequencerView`).
pub struct SequencerView {
    base: NoteSequencerBase,
}

impl SequencerView {
    pub fn new(_ctx: &Ctx) -> Self {
        SequencerView { base: NoteSequencerBase::new(NAME_SEQUENCER, true) }
    }
}

impl View for SequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, button_color_id, update_control_surface,
        sequencer_resolution_index, set_sequencer_resolution_index, on_sequencer_left,
        on_sequencer_right);
    // AbstractNoteSequencerView
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut SequencerHooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        match button {
            ButtonId::ArrowLeft => self.base.seq.on_left(ctx, event),
            ButtonId::ArrowRight => self.base.seq.on_right(ctx, event),
            ButtonId::ArrowUp => self.base.on_octave_up(ctx, event),
            ButtonId::ArrowDown => self.base.on_octave_down(ctx, event),
            _ => self.base.seq.on_button(ctx, button, event),
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let (left, right) = clip_scroll(&self.base.seq, ctx);
        Some(ScrollStates {
            can_scroll_left: left,
            can_scroll_right: right,
            can_scroll_up: self.base.is_octave_up_button_on(ctx),
            can_scroll_down: self.base.is_octave_down_button_on(ctx),
        })
    }

    crate::impl_session_alternative!();
    crate::impl_transpose_view!();

    impl_as_any!();
}

impl SessionAlternative for SequencerView {
    fn was_alternate_interaction_used(&self) -> bool {
        false
    }

    fn set_alternate_interaction_used(&mut self, _was_used: bool) {}
}

impl MaschineView for SequencerView {
    fn change_option(&mut self, ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32) {
        self.base.seq.view.key_manager.clear_pressed_keys();
        let increase = ctx.model.value_changer().is_increase(control);
        if !change_scale_option(ctx, temporary_encoder_mode, increase) && temporary_encoder_mode == EncoderMode::TemporaryTune {
            if increase {
                self.base.on_octave_up(ctx, ButtonEvent::Down);
            } else {
                self.base.on_octave_down(ctx, ButtonEvent::Down);
            }
        }
        // Former updateScaleConfig
        update_scale_properties(ctx);
        self.base.update_note_mapping(ctx);
    }
}

impl TransposeView for SequencerView {
    // AbstractNoteSequencerView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

/// The poly sequencer (former `PolySequencerView`).
pub struct PolySequencerView {
    base: PolySequencerBase,
}

impl PolySequencerView {
    pub fn new(_ctx: &Ctx) -> Self {
        PolySequencerView { base: PolySequencerBase::new(true) }
    }
}

impl View for PolySequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, button_color_id, update_control_surface,
        sequencer_resolution_index, set_sequencer_resolution_index, on_sequencer_left,
        on_sequencer_right);
    // AbstractPolySequencerView
    crate::forward_view!(base => on_activate, draw_grid, on_grid_note_long_press,
        update_note_mapping, on_model_event);

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut DefaultPolySequencerHooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        match button {
            ButtonId::ArrowLeft => self.base.seq.on_left(ctx, event),
            ButtonId::ArrowRight => self.base.seq.on_right(ctx, event),
            ButtonId::ArrowUp => self.base.on_octave_up(ctx, event),
            ButtonId::ArrowDown => self.base.on_octave_down(ctx, event),
            _ => self.base.seq.on_button(ctx, button, event),
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let (left, right) = clip_scroll(&self.base.seq, ctx);
        Some(ScrollStates {
            can_scroll_left: left,
            can_scroll_right: right,
            can_scroll_up: self.base.is_octave_up_button_on(ctx),
            can_scroll_down: self.base.is_octave_down_button_on(ctx),
        })
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PolySequencerView {
    // AbstractPolySequencerView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

/// The raindrops sequencer (former `RaindropsView`).
pub struct RaindropsView {
    base: RaindropsBase,
}

impl RaindropsView {
    pub fn new(_ctx: &Ctx) -> Self {
        RaindropsView { base: RaindropsBase::new(NAME_RAINDROPS, true) }
    }
}

impl View for RaindropsView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, button_color_id, update_control_surface,
        sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractRaindropsView
    crate::forward_view!(base => on_activate, draw_grid, on_grid_note, update_note_mapping);

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        match button {
            ButtonId::ArrowRight | ButtonId::ArrowUp => self.base.on_octave_up(ctx, event),
            ButtonId::ArrowLeft | ButtonId::ArrowDown => self.base.on_octave_down(ctx, event),
            _ => self.base.on_button(ctx, button, event),
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let up = self.base.is_octave_up_button_on(ctx);
        let down = self.base.is_octave_down_button_on(ctx);
        Some(ScrollStates { can_scroll_left: up, can_scroll_right: down, can_scroll_up: up, can_scroll_down: down })
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for RaindropsView {
    // AbstractRaindropsView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
