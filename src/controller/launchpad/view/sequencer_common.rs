// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Functions shared by the Launchpad sequencer views (the parts which are duplicated in the
//! Java sequencer view classes).

use crate::controller::launchpad::colors::{LAUNCHPAD_COLOR_BLACK, LAUNCHPAD_COLOR_GREEN, LAUNCHPAD_COLOR_YELLOW};
use crate::daw::clip::NotePosition;
use crate::framework::ctx::Ctx;
use crate::framework::feature::ScrollStates;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::scale::Scales;

/// If the UP or DOWN button is pressed, it is consumed and `Some(is_up)` is returned. Used to
/// change the repeat count of a step.
pub fn up_down_combination(ctx: &mut Ctx) -> Option<bool> {
    let is_up_pressed = ctx.surface.is_pressed(ButtonId::Up);
    if is_up_pressed || ctx.surface.is_pressed(ButtonId::Down) {
        ctx.surface.set_trigger_consumed(if is_up_pressed { ButtonId::Up } else { ButtonId::Down });
        return Some(is_up_pressed);
    }
    None
}

/// Handles the UP/DOWN + pad combination (repeat operator). Returns true if handled.
pub fn handle_repeat_combination(seq: &SequencerBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
    let Some(is_up) = up_down_combination(ctx) else {
        return false;
    };
    if velocity > 0 {
        seq.handle_sequencer_area_repeat_operator(ctx, note_position, velocity, is_up);
    }
    true
}

/// The color of the scene buttons, which select the resolution.
pub fn resolution_button_color(seq: &SequencerBase, ctx: &Ctx, button: ButtonId, non_scene_color: i32) -> i32 {
    if !seq.is_active() {
        return LAUNCHPAD_COLOR_BLACK;
    }
    match scene_index(button) {
        None => non_scene_color,
        Some(scene) if scene == 7 - seq.resolution_index(ctx) => LAUNCHPAD_COLOR_YELLOW,
        Some(_) => LAUNCHPAD_COLOR_GREEN,
    }
}

/// The scroll states of the note sequencers: left/right scroll the clip pages, up/down the
/// octaves.
pub fn note_sequencer_scroll_states(seq: &SequencerBase, ctx: &Ctx) -> ScrollStates {
    let seq_octave = ctx.model.scales().octave();
    let (left, right) = clip_scroll_states(seq, ctx);
    ScrollStates { can_scroll_left: left, can_scroll_right: right, can_scroll_up: seq_octave < Scales::OCTAVE_RANGE, can_scroll_down: seq_octave > -Scales::OCTAVE_RANGE }
}

/// The scroll states of the drum sequencers: left/right scroll the clip pages, up/down the drum
/// pads.
pub fn drum_scroll_states(seq: &SequencerBase, ctx: &Ctx) -> ScrollStates {
    let scales = ctx.model.scales();
    let (left, right) = clip_scroll_states(seq, ctx);
    ScrollStates { can_scroll_left: left, can_scroll_right: right, can_scroll_up: scales.can_scroll_drum_octave_up(), can_scroll_down: scales.can_scroll_drum_octave_down() }
}

fn clip_scroll_states(seq: &SequencerBase, ctx: &Ctx) -> (bool, bool) {
    match seq.clip_ref(ctx) {
        Some(clip) => (clip.can_scroll_steps_backwards(), clip.can_scroll_steps_forwards()),
        None => (false, false),
    }
}
