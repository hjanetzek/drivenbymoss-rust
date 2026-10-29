// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Edit the attributes of the selected notes (former `EditNoteMode` and the used parts of
//! `NoteParameter`).
//!
//! Reaper supports the note attributes pitch, duration, mute and velocity, therefore only the
//! 'Note' page with length and velocity is available (the 'Expressions' and 'Repeat' pages
//! require timbre support). The knobs are not bound to note parameters, the knob functions of
//! the mode change the attributes directly (which has the same effect).

use super::{format_measures, format_percentage, with_display, BaseMode, MAIN_ENCODER};
use crate::controller::maschine::mk::surface::maschine;
use crate::daw::clip::StepState;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::NoteEditor;
use crate::impl_as_any;
use crate::scale::Scales;

/// The note attributes which can be edited (subset of the former `NoteAttribute`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteAttribute {
    Duration,
    Velocity,
    VelocitySpread,
    ReleaseVelocity,
    Chance,
    Occurrence,
    Gain,
    Panning,
    Transpose,
    Timbre,
    Pressure,
}

impl NoteAttribute {
    /// Is the attribute supported by Reaper (former `IHost.supports (NoteAttribute)`)?
    pub fn is_supported(self) -> bool {
        matches!(self, NoteAttribute::Duration | NoteAttribute::Velocity)
    }
}

/// The attributes of the 8 knobs on the 'Note' page (the first knob has no function).
const NOTE_PAGE: [Option<NoteAttribute>; 8] = [
    None,
    Some(NoteAttribute::Duration),
    Some(NoteAttribute::Duration),
    Some(NoteAttribute::Velocity),
    Some(NoteAttribute::VelocitySpread),
    Some(NoteAttribute::ReleaseVelocity),
    Some(NoteAttribute::Chance),
    Some(NoteAttribute::Occurrence),
];

/// Edit the attributes of the selected notes.
pub struct EditNoteMode {
    base: BaseMode,
    note_editor: NoteEditor,
    active_note_attribute: NoteAttribute,
    /// True if the knobs are used (Maschine with display), otherwise only the main encoder
    /// edits the active note attribute.
    has_controls: bool,
}

impl EditNoteMode {
    pub fn new(ctx: &Ctx) -> Self {
        let has_controls = maschine(ctx).has_mcu_display();
        EditNoteMode { base: BaseMode::new(2), note_editor: NoteEditor::new(), active_note_attribute: NoteAttribute::Velocity, has_controls }
    }

    pub fn active_parameter(&self) -> NoteAttribute {
        self.active_note_attribute
    }

    pub fn select_active_parameter(&mut self, attribute: NoteAttribute) {
        self.active_note_attribute = attribute;
    }

    /// Change the attribute of all edited notes (former `NoteParameter.changeValue`).
    fn change_value(&self, ctx: &mut Ctx, attribute: Option<NoteAttribute>, value: i32) {
        let Some(attribute) = attribute.filter(|a| a.is_supported()) else {
            return;
        };
        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        let clip = ctx.model.note_clip(cols, rows);
        for note_position in self.note_editor.notes() {
            if clip.step(note_position).state() == StepState::Off {
                return;
            }
            match attribute {
                NoteAttribute::Duration => clip.change_step_duration(note_position, value),
                NoteAttribute::Velocity => clip.change_step_velocity(note_position, value),
                _ => {}
            }
        }
    }

    /// Reset the attribute of all edited notes (former `NoteParameter.resetValue`).
    fn reset_value(&self, ctx: &mut Ctx, attribute: Option<NoteAttribute>) {
        let Some(attribute) = attribute.filter(|a| a.is_supported()) else {
            return;
        };
        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        let clip = ctx.model.note_clip(cols, rows);
        for note_position in self.note_editor.notes() {
            if clip.step(note_position).state() == StepState::Off {
                return;
            }
            match attribute {
                NoteAttribute::Duration => clip.update_step_duration(note_position, 1.0),
                NoteAttribute::Velocity => clip.update_step_velocity(note_position, 1.0),
                _ => {}
            }
        }
    }

    fn knob_attribute(&self, index: usize) -> Option<NoteAttribute> {
        let idx = self.base.index(index);
        NOTE_PAGE.get(idx.max(0) as usize).copied().flatten()
    }
}

impl Mode for EditNoteMode {
    fn name(&self) -> &str {
        "Edit note"
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !self.has_controls {
            self.change_value(ctx, Some(self.active_note_attribute), value);
            return;
        }
        self.change_value(ctx, self.knob_attribute(index), value);
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if self.note_editor.notes().is_empty() {
            return;
        }

        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            let index = if index == 8 { MAIN_ENCODER } else { index };
            self.reset_value(ctx, self.knob_attribute(index));
            return;
        }

        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        let notes = self.note_editor.notes().to_vec();
        let clip = ctx.model.note_clip(cols, rows);
        if is_touched {
            clip.start_edit(&notes);
        } else {
            clip.stop_edit();
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let notes = self.note_editor.notes().to_vec();
        if notes.is_empty() {
            with_display(ctx, |d| {
                d.clear();
                d.set_block(0, 0, "Please select");
                d.set_block(0, 1, "a note...");
                d.all_done();
            });
            return;
        }

        let note_position = notes[0];
        let (cols, rows) = self.note_editor.clip().unwrap_or((16, 128));
        let quarters_per_measure = ctx.model.transport().quarters_per_measure();
        let step_info = ctx.model.note_clip(cols, rows).step(&note_position).clone();
        let note_text = if notes.len() > 1 {
            format!("*:{}", notes.len())
        } else {
            format!("{}:{}", note_position.step + 1, Scales::format_note_and_octave(note_position.note, -3))
        };
        let length = format_measures(quarters_per_measure, step_info.duration(), 0, true);
        let (length_head, length_tail) = length.split_once(':').unwrap_or((&length, ""));
        let (length_head, length_tail) = (length_head.to_string(), format!(":{length_tail}"));
        let velocity = format_percentage(step_info.velocity());
        let base = self.base;
        with_display(ctx, |d| {
            d.clear();
            d.set_cell(0, 0, "Note");
            d.set_cell(1, 0, &note_text);
            d.set_cell(0, 1, &base.mark("Length", 2));
            d.set_cell(1, 1, &length_head);
            d.set_cell(1, 2, &length_tail);
            // Velocity spread, release velocity, chance and occurrence are not supported by
            // Reaper
            d.set_cell(0, 3, &base.mark("Velocity", 3)).set_cell(1, 3, &velocity);
            d.all_done();
        });
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        if self.has_next_item(ctx) {
            self.base.selected_param += 1;
        }
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if self.has_previous_item(ctx) {
            self.base.selected_param -= 1;
        }
    }

    fn select_item(&mut self, _ctx: &mut Ctx, index: usize) {
        self.base.select_item(index);
    }

    fn has_previous_item(&self, _ctx: &Ctx) -> bool {
        self.base.selected_param > 2
    }

    fn has_next_item(&self, _ctx: &Ctx) -> bool {
        self.base.selected_param < 7
    }

    // The 'Expressions' and 'Repeat' pages require the timbre attribute, which is not
    // supported by Reaper: no page switching

    fn note_editor(&self) -> Option<&NoteEditor> {
        Some(&self.note_editor)
    }

    fn note_editor_mut(&mut self) -> Option<&mut NoteEditor> {
        Some(&mut self.note_editor)
    }

    impl_as_any!();
}
