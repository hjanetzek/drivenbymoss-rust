// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum views of the Maschine JAM (former `DrumView`, `Drum4View`, `Drum8View` and
//! `Drum64View`).

use super::MaschineView;
use crate::controller::maschine::core::encoder_mode_manager::EncoderMode;
use crate::daw::clip::{NotePosition, SessionAlternative};
use crate::daw::events::ModelEvent;
use crate::framework::color::BUTTON_STATE_OFF;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::drum64_view::Drum64ViewBase;
use crate::framework::view::sequencer::abstract_drum_lane_view::{DrumLaneBase, DrumLaneHooks};
use crate::framework::view::sequencer::abstract_drum_view::{DrumBase, DrumHooks};
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

pub const NAME_DRUM: &str = "Drum";

/// Left/right scroll the clip pages, up/down the drum octave.
fn clip_scroll(seq: &SequencerBase, ctx: &Ctx) -> (bool, bool) {
    match seq.clip_ref(ctx) {
        Some(clip) => (clip.can_scroll_steps_backwards(), clip.can_scroll_steps_forwards()),
        None => (false, false),
    }
}

/// Former `handleSequencerAreaButtonCombinations` override: Select + pad changes the repeat
/// count. Returns true if handled.
fn handle_select_repeat(seq: &SequencerBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
    let is_select_pressed = ctx.surface.is_select_pressed();
    if is_select_pressed {
        if velocity > 0 {
            seq.handle_sequencer_area_repeat_operator(ctx, note_position, velocity, is_select_pressed);
        }
        return true;
    }
    false
}

struct DrumViewHooks;

impl DrumHooks for DrumViewHooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        handle_select_repeat(&base.seq, ctx, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, velocity)
    }
}

/// The drum sequencer (former `DrumView`).
pub struct DrumView {
    base: DrumBase,
}

impl DrumView {
    pub fn new(_ctx: &Ctx) -> Self {
        DrumView { base: DrumBase::new(NAME_DRUM, 4, 4, true) }
    }
}

impl View for DrumView {
    fn name(&self) -> &str {
        self.base.seq.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        self.base.seq.button_color_id(ctx, button)
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.base.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut DrumViewHooks, ctx, note, velocity);
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
        let (left, right) = clip_scroll(&self.base.seq, ctx);
        Some(ScrollStates {
            can_scroll_left: left,
            can_scroll_right: right,
            can_scroll_up: self.base.is_octave_up_button_on(ctx),
            can_scroll_down: self.base.is_octave_down_button_on(ctx),
        })
    }

    fn sequencer_resolution_index(&self, ctx: &Ctx) -> Option<usize> {
        Some(self.base.seq.resolution_index(ctx))
    }

    fn set_sequencer_resolution_index(&mut self, ctx: &mut Ctx, index: i32) -> bool {
        self.base.seq.set_resolution_index(ctx, index);
        true
    }

    fn on_sequencer_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq.on_left(ctx, event);
    }

    fn on_sequencer_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq.on_right(ctx, event);
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

impl SessionAlternative for DrumView {
    fn was_alternate_interaction_used(&self) -> bool {
        false
    }

    fn set_alternate_interaction_used(&mut self, _was_used: bool) {}
}

impl MaschineView for DrumView {
    fn change_option(&mut self, ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32) {
        self.base.seq.view.key_manager.clear_pressed_keys();
        let increase = ctx.model.value_changer().is_increase(control);
        if temporary_encoder_mode == EncoderMode::TemporaryTune {
            if increase {
                self.base.on_octave_up(ctx, ButtonEvent::Down);
            } else {
                self.base.on_octave_down(ctx, ButtonEvent::Down);
            }
        }
    }
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

struct DrumLaneViewHooks;

impl DrumLaneHooks for DrumLaneViewHooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumLaneBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32, accent_velocity: i32) -> bool {
        handle_select_repeat(&base.drum.seq, ctx, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity, accent_velocity)
    }
}

/// The drum lane views (former `Drum4View` and `Drum8View`). Only the Drum 4 view supports the
/// navigation with the arrow buttons (former `IViewNavigation`).
pub struct DrumLaneView {
    base: DrumLaneBase,
    has_navigation: bool,
}

impl DrumLaneView {
    pub fn new_drum4(_ctx: &Ctx) -> Self {
        DrumLaneView { base: DrumLaneBase::drum4(true), has_navigation: true }
    }

    pub fn new_drum8(_ctx: &Ctx) -> Self {
        DrumLaneView { base: DrumLaneBase::drum8(true), has_navigation: false }
    }
}

impl View for DrumLaneView {
    fn name(&self) -> &str {
        self.base.seq().name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        self.base.seq().button_color_id(ctx, button)
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.base.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut DrumLaneViewHooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        match button {
            ButtonId::ArrowLeft => self.base.seq_mut().on_left(ctx, event),
            ButtonId::ArrowRight => self.base.seq_mut().on_right(ctx, event),
            ButtonId::ArrowUp => self.base.on_octave_up(ctx, event),
            ButtonId::ArrowDown => self.base.on_octave_down(ctx, event),
            _ => self.base.seq_mut().on_button(ctx, button, event),
        }
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
        if !self.has_navigation {
            return None;
        }
        let (left, right) = clip_scroll(self.base.seq(), ctx);
        Some(ScrollStates {
            can_scroll_left: left,
            can_scroll_right: right,
            can_scroll_up: self.base.drum.is_octave_up_button_on(ctx),
            can_scroll_down: self.base.drum.is_octave_down_button_on(ctx),
        })
    }

    fn sequencer_resolution_index(&self, ctx: &Ctx) -> Option<usize> {
        Some(self.base.seq().resolution_index(ctx))
    }

    fn set_sequencer_resolution_index(&mut self, ctx: &mut Ctx, index: i32) -> bool {
        self.base.seq_mut().set_resolution_index(ctx, index);
        true
    }

    fn on_sequencer_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq_mut().on_left(ctx, event);
    }

    fn on_sequencer_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.seq_mut().on_right(ctx, event);
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

impl SessionAlternative for DrumLaneView {
    fn was_alternate_interaction_used(&self) -> bool {
        false
    }

    fn set_alternate_interaction_used(&mut self, _was_used: bool) {}
}

impl MaschineView for DrumLaneView {
    fn change_option(&mut self, ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32) {
        self.base.drum.seq.view.key_manager.clear_pressed_keys();
        let increase = ctx.model.value_changer().is_increase(control);
        if temporary_encoder_mode == EncoderMode::TemporaryTune {
            if increase {
                self.base.on_octave_up(ctx, ButtonEvent::Down);
            } else {
                self.base.on_octave_down(ctx, ButtonEvent::Down);
            }
        }
    }
}

impl TransposeView for DrumLaneView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_down(ctx, event);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.base.on_octave_up(ctx, event);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        self.base.drum.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.base.drum.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.base.drum.reset_octave(ctx);
    }
}

/// The 64 drum pads view (former `Drum64View`).
pub struct Drum64View {
    drum: Drum64ViewBase,
}

impl Drum64View {
    pub fn new(_ctx: &Ctx) -> Self {
        Drum64View { drum: Drum64ViewBase::new(8, 8) }
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
            // Former handleButtonCombinations override: browsing the presets of a drum pad is
            // not supported by Reaper
            if ctx.is_button_combination(ButtonId::Browse) {
                return;
            }
            self.drum.handle_button_combinations(ctx, played_pad, |drum, ctx, played_pad| {
                // Former handleDeleteButton
                ctx.surface.set_trigger_consumed(ButtonId::Delete);
                let edit_midi_channel = ctx.config.midi_edit_channel();
                ctx.model.note_clip(8, 128).clear_row(edit_midi_channel, drum.offset_y + played_pad);
            });
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        match button {
            ButtonId::ArrowUp => self.drum.on_octave_up(ctx, event),
            ButtonId::ArrowDown => self.drum.on_octave_down(ctx, event),
            _ => {}
        }
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.drum.update_note_mapping(ctx);
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        self.drum.on_model_event(ctx, event);
    }

    fn scroll_states(&self, _ctx: &Ctx) -> Option<ScrollStates> {
        Some(super::up_down_scroll_states(self.drum.is_octave_up_button_on(), self.drum.is_octave_down_button_on()))
    }

    fn as_transpose_view(&self) -> Option<&dyn TransposeView> {
        Some(self)
    }

    fn as_transpose_view_mut(&mut self) -> Option<&mut dyn TransposeView> {
        Some(self)
    }

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
