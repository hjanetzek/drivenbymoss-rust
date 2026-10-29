// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note parameters of the note edit modes (the parts of the former `NoteParameter` which
//! are used by `XLBaseNoteEditMode`). The edited note is the one of the step of the knob column,
//! see [`note_position`].
//!
//! Reaper supports only the pitch, duration, mute and velocity attributes of notes (former
//! `IHost.supports (NoteAttribute)` of the Java version for Reaper); of the attributes used by
//! the Launch Control XL only the pitch is functional there. With the step grid of the host
//! (Bitwig) all attributes are supported, as in the Java version for Bitwig.

use super::super::surface as xl;
use super::CLIP_KEY;
use crate::daw::clip::{NotePosition, StepInfo, StepState};
use crate::framework::ctx::Ctx;
use crate::framework::parameterprovider::CustomParameter;
use crate::scale::Scales;

/// The note attributes which are edited by the knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XlNoteAttribute {
    Pitch,
    Chance,
    Repeat,
    VelocitySpread,
    Panning,
}

impl XlNoteAttribute {
    /// Former `IHost.supports`: all attributes with the step grid of the host (Bitwig), only the
    /// pitch on Reaper.
    pub fn is_supported(self, ctx: &Ctx) -> bool {
        self == XlNoteAttribute::Pitch || ctx.model.note_clip_ref(CLIP_KEY.0, CLIP_KEY.1).is_some_and(|clip| clip.is_host_grid())
    }

    pub fn name(self) -> &'static str {
        match self {
            XlNoteAttribute::Pitch => "Pitch",
            XlNoteAttribute::Chance => "Chance",
            XlNoteAttribute::Repeat => "Repeat",
            XlNoteAttribute::VelocitySpread => "Velocity Spread",
            XlNoteAttribute::Panning => "Panning",
        }
    }
}

/// Get the note which is edited by the parameter with the given index (former
/// `XLBaseNoteEditMode.getNotePosition`). In the drum mode it is the note of the selected pad,
/// in the note mode the highest note of the step.
pub fn note_position(ctx: &Ctx, is_drum: bool, parameter_index: usize) -> NotePosition {
    let column = (parameter_index % 8) as i32;
    let channel = ctx.config.midi_edit_channel();
    let note_row = if is_drum {
        xl::xl(ctx).drum_note
    } else {
        ctx.model.note_clip_ref(CLIP_KEY.0, CLIP_KEY.1).map_or(-1, |clip| clip.highest_row(channel, column))
    };
    NotePosition::new(channel, column, note_row)
}

/// A note attribute of the note of a step (former `NoteParameter`).
pub struct XlNoteParameter {
    index: usize,
    attribute: XlNoteAttribute,
    is_drum: bool,
}

impl XlNoteParameter {
    pub fn new(index: usize, attribute: XlNoteAttribute, is_drum: bool) -> Self {
        XlNoteParameter { index, attribute, is_drum }
    }

    fn position(&self, ctx: &Ctx) -> NotePosition {
        note_position(ctx, self.is_drum, self.index)
    }

    fn step(ctx: &Ctx, position: &NotePosition) -> StepInfo {
        ctx.model.note_clip_ref(CLIP_KEY.0, CLIP_KEY.1).map(|clip| clip.step(position).clone()).unwrap_or_default()
    }

    /// Is there a note at the position?
    fn has_note(ctx: &Ctx, position: &NotePosition) -> bool {
        ctx.model.note_clip_ref(CLIP_KEY.0, CLIP_KEY.1).is_some_and(|clip| clip.step(position).state() != StepState::Off)
    }

    fn move_note(ctx: &mut Ctx, position: &NotePosition, new_note: i32) {
        ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1).move_step_y(position, new_note);
        ctx.notify(&format!("Note: {}", Scales::format_note_and_octave(new_note, -3)));
    }
}

impl CustomParameter for XlNoteParameter {
    fn does_exist(&self, _ctx: &Ctx) -> bool {
        // There is always one note position
        true
    }

    fn name(&self, _ctx: &Ctx) -> String {
        self.attribute.name().to_string()
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        let position = self.position(ctx);
        if position.note == -1 || !Self::has_note(ctx, &position) || !self.attribute.is_supported(ctx) {
            return 0;
        }
        let step = Self::step(ctx, &position);
        let normalized_value = match self.attribute {
            XlNoteAttribute::Pitch => position.note as f64 / 127.0,
            XlNoteAttribute::Chance => step.chance(),
            XlNoteAttribute::Repeat => (step.repeat_count() as f64 + 127.0) / 254.0,
            XlNoteAttribute::VelocitySpread => step.velocity_spread(),
            XlNoteAttribute::Panning => (step.pan() + 1.0) / 2.0,
        };
        ctx.model.value_changer().from_normalized_value(normalized_value)
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        let position = self.position(ctx);
        if !self.attribute.is_supported(ctx) || !Self::has_note(ctx, &position) {
            return String::new();
        }
        let step = Self::step(ctx, &position);
        match self.attribute {
            XlNoteAttribute::Pitch => Scales::format_note_and_octave(position.note, -3),
            XlNoteAttribute::Chance => format_percentage(step.chance()),
            XlNoteAttribute::Repeat => step.formatted_repeat_count(),
            XlNoteAttribute::VelocitySpread => format_percentage(step.velocity_spread()),
            XlNoteAttribute::Panning => format_percentage(step.pan()),
        }
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        if !self.attribute.is_supported(ctx) {
            return;
        }
        let position = self.position(ctx);
        if !Self::has_note(ctx, &position) {
            return;
        }
        let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
        let message = match self.attribute {
            XlNoteAttribute::Pitch => {
                // Move (transpose) the note up and down
                let mut new_note = (value * 126.0).round() as i32;
                if !ctx.model.scales().is_chromatic() {
                    new_note = ctx.model.scales().nearest_note_in_scale(new_note);
                }
                Self::move_note(ctx, &position, new_note);
                return;
            }
            XlNoteAttribute::Chance => {
                clip.update_step_chance(&position, value);
                format!("Chance: {}", format_percentage(clip.step(&position).chance()))
            }
            XlNoteAttribute::Repeat => {
                // The range is limited to 1/16 to 16 to make it controllable with an absolute knob
                clip.update_step_repeat_count(&position, ((value - 0.5) * 30.0).round() as i32);
                format!("Repeat: {}", clip.step(&position).formatted_repeat_count())
            }
            XlNoteAttribute::VelocitySpread => {
                clip.update_step_velocity_spread(&position, value);
                format!("Velocity Spread: {}%", (value * 100.0).round() as i32)
            }
            XlNoteAttribute::Panning => {
                let pan = value * 2.0 - 1.0;
                clip.update_step_pan(&position, pan);
                format!("Panning: {}", format_percentage(pan))
            }
        };
        ctx.notify(&message);
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        if !self.attribute.is_supported(ctx) {
            return;
        }
        let position = self.position(ctx);
        if !Self::has_note(ctx, &position) {
            return;
        }
        if self.attribute != XlNoteAttribute::Pitch {
            let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
            let message = match self.attribute {
                XlNoteAttribute::Chance => {
                    clip.change_step_chance(&position, control);
                    format!("Chance: {}", format_percentage(clip.step(&position).chance()))
                }
                XlNoteAttribute::Repeat => {
                    clip.change_step_repeat_count(&position, control);
                    format!("Repeat: {}", clip.step(&position).formatted_repeat_count())
                }
                XlNoteAttribute::VelocitySpread => {
                    clip.change_step_velocity_spread(&position, control);
                    format!("Vel. Spread: {}", format_percentage(clip.step(&position).velocity_spread()))
                }
                _ => {
                    clip.change_step_pan(&position, control);
                    format!("Pan: {}", format_percentage(clip.step(&position).pan()))
                }
            };
            ctx.notify(&message);
            return;
        }
        // Move (transpose) the note up and down
        let offset = if ctx.model.value_changer().is_increase(control) { 1 } else { -1 };
        let scales = ctx.model.scales();
        let is_chromatic = scales.is_chromatic();
        let mut new_note = position.note;
        loop {
            new_note += offset;
            // No more scale notes found?
            if !(0..=127).contains(&new_note) {
                return;
            }
            if is_chromatic || scales.is_in_scale(scales.to_note_in_octave(new_note)) {
                break;
            }
        }
        Self::move_note(ctx, &position, new_note);
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        if !self.attribute.is_supported(ctx) {
            return;
        }
        let position = self.position(ctx);
        if !Self::has_note(ctx, &position) {
            return;
        }
        let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
        match self.attribute {
            XlNoteAttribute::Pitch => {}
            XlNoteAttribute::Chance => return clip.update_step_chance(&position, 1.0),
            XlNoteAttribute::Repeat => return clip.update_step_repeat_count(&position, 0),
            XlNoteAttribute::VelocitySpread => return clip.update_step_velocity_spread(&position, 0.0),
            XlNoteAttribute::Panning => return clip.update_step_pan(&position, 0.0),
        }
        let new_note = 60 + ctx.model.scales().scale_offset();
        Self::move_note(ctx, &position, new_note);
    }
}

/// Format a value as a percentage with 1 fraction digit (former `StringUtils.formatPercentage`).
fn format_percentage(value: f64) -> String {
    let percent = (value * 1000.0).round() / 10.0;
    if percent.fract() == 0.0 {
        format!("{}%", percent as i64)
    } else {
        format!("{percent:.1}%")
    }
}
