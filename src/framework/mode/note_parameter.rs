// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The attributes of the notes edited by a [`NoteEditor`] as parameters (former
//! `NoteParameter`): get the value of an attribute of the first edited note, set it with an
//! absolute knob or change it with a relative one for all edited notes.
//!
//! Reaper supports only the pitch, duration, mute and velocity attributes (former
//! `IHost.supports (NoteAttribute)` of the Java version for Reaper). With the step grid of the
//! host (Bitwig) all attributes are supported ([`NoteEditor::is_host_grid`]).

use crate::daw::clip::{NotePosition, StepState};
use crate::framework::ctx::Ctx;
use crate::framework::mode::note_editor::NoteEditor;
use crate::framework::string_utils::{format_measures, format_percentage};
use crate::scale::Scales;

/// The attributes of a note (former `NoteAttribute`, without the occurrence, recurrence and the
/// repeat curves, which no ported controller edits with this parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteAttribute {
    Pitch,
    Gain,
    Panning,
    Duration,
    Velocity,
    ReleaseVelocity,
    VelocitySpread,
    Mute,
    Pressure,
    Timbre,
    Transpose,
    Chance,
    Repeat,
}

impl NoteAttribute {
    /// Former `NoteParameter.ATTRIBUTE_NAMES`.
    pub fn name(self) -> &'static str {
        match self {
            NoteAttribute::Pitch => "Pitch",
            NoteAttribute::Gain => "Gain",
            NoteAttribute::Panning => "Panning",
            NoteAttribute::Duration => "Duration",
            NoteAttribute::Velocity => "Velocity",
            NoteAttribute::ReleaseVelocity => "Release Velocity",
            NoteAttribute::VelocitySpread => "Velocity Spread",
            NoteAttribute::Mute => "Mute",
            NoteAttribute::Pressure => "Pressure",
            NoteAttribute::Timbre => "Timbre",
            NoteAttribute::Transpose => "Transpose",
            NoteAttribute::Chance => "Chance",
            NoteAttribute::Repeat => "Repeat",
        }
    }

    /// Is the attribute supported by the host (former `IHost.supports (NoteAttribute)`)?
    pub fn is_supported(self, is_host_grid: bool) -> bool {
        is_host_grid || matches!(self, NoteAttribute::Pitch | NoteAttribute::Duration | NoteAttribute::Velocity | NoteAttribute::Mute)
    }
}

fn supports(ctx: &Ctx, editor: &NoteEditor, attribute: NoteAttribute) -> bool {
    attribute.is_supported(editor.is_host_grid(&ctx.model))
}

/// The value (in the range of the value changer) of the attribute of the first edited note
/// (former `NoteParameter.getValue`).
pub fn note_value(ctx: &Ctx, editor: &NoteEditor, attribute: NoteAttribute) -> i32 {
    let Some((cols, rows)) = editor.clip() else {
        return 0;
    };
    let Some(note_position) = editor.notes().first() else {
        return 0;
    };
    if note_position.note == -1 {
        return 0;
    }
    let Some(clip) = ctx.model.note_clip_ref(cols, rows) else {
        return 0;
    };
    let step = clip.step(note_position);
    if step.state() == StepState::Off || !supports(ctx, editor, attribute) {
        return 0;
    }
    let normalized = match attribute {
        NoteAttribute::Pitch => note_position.note as f64 / 127.0,
        NoteAttribute::Gain => step.gain(),
        NoteAttribute::Panning => (step.pan() + 1.0) / 2.0,
        NoteAttribute::Duration => step.duration(),
        NoteAttribute::Velocity => step.velocity(),
        NoteAttribute::ReleaseVelocity => step.release_velocity(),
        NoteAttribute::VelocitySpread => step.velocity_spread(),
        NoteAttribute::Mute => {
            if step.is_muted() {
                1.0
            } else {
                0.0
            }
        }
        NoteAttribute::Pressure => step.pressure(),
        NoteAttribute::Timbre => step.timbre(),
        NoteAttribute::Transpose => {
            let range = clip.step_transpose_range();
            (step.transpose() + range) / (2.0 * range)
        }
        NoteAttribute::Chance => step.chance(),
        NoteAttribute::Repeat => (step.repeat_count() as f64 + 127.0) / 254.0,
    };
    ctx.model.value_changer().from_normalized_value(normalized)
}

/// Set the attribute of all edited notes with an absolute value (former
/// `NoteParameter.setValue`).
pub fn set_note_value(ctx: &mut Ctx, editor: &NoteEditor, attribute: NoteAttribute, value: i32) {
    let normalized = ctx.model.value_changer().to_normalized_value(value as f64);
    set_note_normalized_value(ctx, editor, attribute, normalized);
}

/// Set the attribute of all edited notes with a normalized value (former
/// `NoteParameter.setNormalizedValue`).
pub fn set_note_normalized_value(ctx: &mut Ctx, editor: &NoteEditor, attribute: NoteAttribute, normalized: f64) {
    if !supports(ctx, editor, attribute) {
        return;
    }
    let Some((cols, rows)) = editor.clip() else {
        return;
    };
    let quarters_per_measure = ctx.model.transport().quarters_per_measure();
    let is_chromatic = ctx.model.scales().is_chromatic();
    let notes: Vec<NotePosition> = editor.notes().to_vec();
    for note_position in notes {
        let new_note = {
            let n = (normalized * 126.0).round() as i32;
            if is_chromatic {
                n
            } else {
                ctx.model.scales().nearest_note_in_scale(n)
            }
        };
        let clip = ctx.model.note_clip(cols, rows);
        if clip.step(&note_position).state() == StepState::Off {
            return;
        }
        let message = match attribute {
            NoteAttribute::Pitch => {
                // Move (transpose) the note up and down
                clip.move_step_y(&note_position, new_note);
                format!("Note: {}", Scales::format_note_and_octave(new_note, -3))
            }
            NoteAttribute::Gain => {
                clip.update_step_gain(&note_position, normalized);
                format!("Gain: {}", format_percentage(normalized))
            }
            NoteAttribute::Panning => {
                let pan = normalized * 2.0 - 1.0;
                clip.update_step_pan(&note_position, pan);
                format!("Panning: {}", format_percentage(pan))
            }
            NoteAttribute::Duration => {
                // Since this is an absolute knob but the duration is unlimited, it is limited
                // to 4 bars (128 x 32th) and exactly on a 32th
                let duration = (normalized * 16.0 / 0.125).round() * 0.125;
                clip.update_step_duration(&note_position, duration);
                format!("Duration: {}", format_measures(quarters_per_measure, duration, 0, true))
            }
            NoteAttribute::Velocity => {
                clip.update_step_velocity(&note_position, normalized);
                format!("Velocity: {}", format_percentage(normalized))
            }
            NoteAttribute::ReleaseVelocity => {
                clip.update_step_release_velocity(&note_position, normalized);
                format!("Release Velocity: {}", format_percentage(normalized))
            }
            NoteAttribute::VelocitySpread => {
                clip.update_step_velocity_spread(&note_position, normalized);
                format!("Velocity Spread: {}%", (normalized * 100.0).round() as i32)
            }
            NoteAttribute::Mute => {
                let is_muted = normalized > 0.5;
                clip.update_step_mute_state(&note_position, is_muted);
                format!("Muted: {}", if is_muted { "Yes" } else { "No" })
            }
            NoteAttribute::Pressure => {
                clip.update_step_pressure(&note_position, normalized);
                format!("Pressure: {}", format_percentage(normalized))
            }
            NoteAttribute::Timbre => {
                clip.update_step_timbre(&note_position, normalized);
                format!("Timbre: {}", format_percentage(normalized))
            }
            NoteAttribute::Transpose => {
                let range = clip.step_transpose_range();
                let v = normalized * 2.0 * range - range;
                clip.update_step_transpose(&note_position, v);
                format!("Pitch: {v:.1}")
            }
            NoteAttribute::Chance => {
                clip.update_step_chance(&note_position, normalized);
                format!("Chance: {}", format_percentage(clip.step(&note_position).chance()))
            }
            NoteAttribute::Repeat => {
                // The range is limited to 1/16 to 16 to make it controllable with an absolute
                // knob!
                clip.update_step_repeat_count(&note_position, ((normalized - 0.5) * 30.0).round() as i32);
                format!("Repeat: {}", clip.step(&note_position).formatted_repeat_count())
            }
        };
        ctx.notify(&message);
    }
}

/// Change the attribute of all edited notes with a relative value (former
/// `NoteParameter.changeValue`).
pub fn change_note_value(ctx: &mut Ctx, editor: &NoteEditor, attribute: NoteAttribute, control: i32) {
    if !supports(ctx, editor, attribute) {
        return;
    }
    let Some((cols, rows)) = editor.clip() else {
        return;
    };
    let quarters_per_measure = ctx.model.transport().quarters_per_measure();
    let is_increase = ctx.model.value_changer().is_increase(control);
    let notes: Vec<NotePosition> = editor.notes().to_vec();
    for note_position in notes {
        if ctx.model.note_clip(cols, rows).step(&note_position).state() == StepState::Off {
            return;
        }
        if attribute == NoteAttribute::Pitch {
            // Move (transpose) the note up and down
            let offset = if is_increase { 1 } else { -1 };
            let is_chromatic = ctx.model.scales().is_chromatic();
            let mut new_note = note_position.note;
            loop {
                new_note += offset;
                // No more scale notes found?
                if !(0..=127).contains(&new_note) {
                    return;
                }
                let scales = ctx.model.scales();
                if is_chromatic || scales.is_in_scale(scales.to_note_in_octave(new_note)) {
                    break;
                }
            }
            ctx.model.note_clip(cols, rows).move_step_y(&note_position, new_note);
            ctx.notify(&format!("Note: {}", Scales::format_note_and_octave(new_note, -3)));
            continue;
        }
        let clip = ctx.model.note_clip(cols, rows);
        match attribute {
            NoteAttribute::Gain => clip.change_step_gain(&note_position, control),
            NoteAttribute::Panning => clip.change_step_pan(&note_position, control),
            NoteAttribute::Duration => clip.change_step_duration(&note_position, control),
            NoteAttribute::Velocity => clip.change_step_velocity(&note_position, control),
            NoteAttribute::ReleaseVelocity => clip.change_step_release_velocity(&note_position, control),
            NoteAttribute::VelocitySpread => clip.change_step_velocity_spread(&note_position, control),
            NoteAttribute::Mute => clip.change_step_mute_state(&note_position, control),
            NoteAttribute::Pressure => clip.change_step_pressure(&note_position, control),
            NoteAttribute::Timbre => clip.change_step_timbre(&note_position, control),
            NoteAttribute::Transpose => clip.change_step_transpose(&note_position, control),
            NoteAttribute::Chance => clip.change_step_chance(&note_position, control),
            NoteAttribute::Repeat => clip.change_step_repeat_count(&note_position, control),
            NoteAttribute::Pitch => {}
        }
        let step = clip.step(&note_position).clone();
        let message = match attribute {
            NoteAttribute::Gain => format!("Gain: {}", format_percentage(step.gain())),
            NoteAttribute::Panning => format!("Pan: {}", format_percentage(step.pan())),
            NoteAttribute::Duration => format!("Duration: {}", format_measures(quarters_per_measure, step.duration(), 0, true)),
            NoteAttribute::Velocity => format!("Velocity: {}", format_percentage(step.velocity())),
            NoteAttribute::ReleaseVelocity => format!("Release Velocity: {}", format_percentage(step.release_velocity())),
            NoteAttribute::VelocitySpread => format!("Velocity Spread: {}", format_percentage(step.velocity_spread())),
            NoteAttribute::Mute => format!("Muted: {}", if step.is_muted() { "Yes" } else { "No" }),
            NoteAttribute::Pressure => format!("Pressure: {}", format_percentage(step.pressure())),
            NoteAttribute::Timbre => format!("Timbre: {}", format_percentage(step.timbre())),
            NoteAttribute::Transpose => format!("Pitch: {:.1}", step.transpose()),
            NoteAttribute::Chance => format!("Chance: {}", format_percentage(step.chance())),
            NoteAttribute::Repeat => format!("Repeat: {}", step.formatted_repeat_count()),
            NoteAttribute::Pitch => String::new(),
        };
        ctx.notify(&message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_attributes() {
        assert!(NoteAttribute::Velocity.is_supported(false));
        assert!(!NoteAttribute::Gain.is_supported(false));
        assert!(NoteAttribute::Gain.is_supported(true));
        assert_eq!(NoteAttribute::VelocitySpread.name(), "Velocity Spread");
    }
}
