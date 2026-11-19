// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum lane sequencer views of the Push (former `Drum4View` and `Drum8View`).

use super::sequencer_view::handle_push_step_combinations;
use super::{handle_drum_scene_repeat, note_repeat_button_color_id};
use crate::daw::clip::NotePosition;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::view::base::is_scene_button;
use crate::framework::view::sequencer::abstract_drum_lane_view::{DrumLaneBase, DrumLaneHooks};
use crate::framework::view::transpose_view::TransposeView;
use crate::midi::ButtonEvent;

struct Hooks;

impl DrumLaneHooks for Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumLaneBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32, accent_velocity: i32) -> bool {
        handle_push_step_combinations(&mut base.drum.seq, ctx, note_position, velocity, true) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity, accent_velocity)
    }
}

macro_rules! drum_lane_view {
    ($name:ident, $constructor:ident, $repeat_colors:expr) => {
        /// A drum lane view.
        pub struct $name {
            pub base: DrumLaneBase,
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                $name { base: DrumLaneBase::$constructor(true) }
            }

            pub fn clip_key(&self) -> ClipKey {
                self.base.drum.seq.clip_key()
            }
        }

        impl View for $name {
            // AbstractSequencerView
            crate::forward_view!(base.drum.seq => name, on_sequencer_left, on_sequencer_right,
                sequencer_resolution_index, set_sequencer_resolution_index);
            // AbstractDrumLaneView
            crate::forward_view!(base => on_activate, on_deactivate, draw_grid, update_control_surface,
                update_note_mapping, on_model_event);

            fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
                if $repeat_colors && is_scene_button(button) && ctx.surface.is_pressed(ButtonId::Repeat) {
                    return note_repeat_button_color_id(ctx, button);
                }
                self.base.drum.seq.button_color_id(ctx, button)
            }

            fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
                self.base.on_grid_note(&mut Hooks, ctx, note, velocity);
            }

            fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
                if !self.base.seq().is_active() {
                    return;
                }
                let index = note - ctx.surface.grid_ref().start_note();
                ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, index.max(0) as usize));
                let (sound, step) = self.base.sound_and_step(ctx, index);
                let note_position = NotePosition::new(ctx.config.midi_edit_channel(), step, sound);
                self.base.drum.seq.edit_note(ctx, &note_position, false);
            }

            fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
                if !is_scene_button(button) || event != ButtonEvent::Down {
                    return;
                }
                if handle_drum_scene_repeat(ctx, button, event) {
                    return;
                }
                self.base.drum.seq.on_button(ctx, button, event);
            }

            fn sequencer_clip_key(&self) -> Option<ClipKey> {
                Some(self.base.drum.seq.clip_key())
            }

            fn sequencer_edit_note(&mut self, ctx: &mut Ctx, note_position: &crate::daw::clip::NotePosition, add_note: bool) {
                self.base.drum.seq.edit_note(ctx, note_position, add_note);
            }

            crate::impl_transpose_view!();


            crate::impl_as_any!();
        }

        impl TransposeView for $name {
            // AbstractDrumLaneView
            crate::forward_transpose_view!(base => on_octave_down, on_octave_up);
            // AbstractDrumView
            crate::forward_transpose_view!(base.drum => is_octave_up_button_on, is_octave_down_button_on,
                reset_octave);
        }
    };
}

// Former Drum4View (with the note repeat colors of the scene buttons)
drum_lane_view!(Drum4View, drum4, true);
// Former Drum8View (Java does not override getButtonColorID)
drum_lane_view!(Drum8View, drum8, false);
