// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Edit the attributes of the selected notes (former `EditNoteMode` and the used parts of
//! `NoteParameter`).
//!
//! The mode has the pages 'Note', 'Expressions' and 'Repeat'. Reaper supports the note
//! attributes pitch, duration, mute and velocity, therefore only the 'Note' page with length and
//! velocity is available there (the 'Expressions' and 'Repeat' pages require timbre support).
//! With the step grid of the host (Bitwig) all attributes and pages are supported, as in the Java
//! version for Bitwig (see [`NoteEditor::is_host_grid`]). The knobs are not bound to note
//! parameters, the knob functions of the mode change the attributes directly (which has the same
//! effect).

use super::{format_measures, format_percentage, with_display, BaseMode, MAIN_ENCODER};
use crate::controller::maschine::mk::surface::maschine;
use crate::daw::clip::{NoteOccurrenceType, StepState};
use crate::daw::util::optimize_name;
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
    Repeat,
    RepeatCurve,
    RepeatVelocityCurve,
    RepeatVelocityEnd,
}

impl NoteAttribute {
    /// Is the attribute supported by the host (former `IHost.supports (NoteAttribute)`)? With
    /// the step grid of the host (Bitwig) all attributes are supported, Reaper supports only
    /// pitch, duration, mute and velocity.
    pub fn is_supported(self, is_host_grid: bool) -> bool {
        is_host_grid || matches!(self, NoteAttribute::Duration | NoteAttribute::Velocity)
    }
}

/// The pages of the mode (former `EditNoteMode.Page`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Note,
    Expressions,
    Repeat,
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

/// The attributes of the 8 knobs on the 'Expressions' page (the first knob has no function).
const EXPRESSIONS_PAGE: [Option<NoteAttribute>; 8] = [
    None,
    Some(NoteAttribute::Duration),
    Some(NoteAttribute::Duration),
    Some(NoteAttribute::Gain),
    Some(NoteAttribute::Panning),
    Some(NoteAttribute::Transpose),
    Some(NoteAttribute::Timbre),
    Some(NoteAttribute::Pressure),
];

/// The attributes of the 8 knobs on the 'Repeat' page (the first knob has no function).
const REPEAT_PAGE: [Option<NoteAttribute>; 8] = [
    None,
    Some(NoteAttribute::Duration),
    Some(NoteAttribute::Duration),
    Some(NoteAttribute::Velocity),
    Some(NoteAttribute::Repeat),
    Some(NoteAttribute::RepeatCurve),
    Some(NoteAttribute::RepeatVelocityCurve),
    Some(NoteAttribute::RepeatVelocityEnd),
];

/// Edit the attributes of the selected notes.
pub struct EditNoteMode {
    base: BaseMode,
    note_editor: NoteEditor,
    page: Page,
    active_note_attribute: NoteAttribute,
    /// True if the knobs are used (Maschine with display), otherwise only the main encoder
    /// edits the active note attribute.
    has_controls: bool,
}

impl EditNoteMode {
    pub fn new(ctx: &Ctx) -> Self {
        let has_controls = maschine(ctx).has_mcu_display();
        EditNoteMode { base: BaseMode::new(2), note_editor: NoteEditor::new(), page: Page::Note, active_note_attribute: NoteAttribute::Velocity, has_controls }
    }

    pub fn active_parameter(&self) -> NoteAttribute {
        self.active_note_attribute
    }

    pub fn select_active_parameter(&mut self, attribute: NoteAttribute) {
        self.active_note_attribute = attribute;
    }

    /// Former `host.supports (NoteAttribute)`.
    fn supports(&self, ctx: &Ctx, attribute: NoteAttribute) -> bool {
        attribute.is_supported(self.note_editor.is_host_grid(&ctx.model))
    }

    /// Change the attribute of all edited notes (former `NoteParameter.changeValue`).
    fn change_value(&self, ctx: &mut Ctx, attribute: Option<NoteAttribute>, value: i32) {
        let Some(attribute) = attribute.filter(|a| self.supports(ctx, *a)) else {
            return;
        };
        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        let increase = ctx.model.value_changer().is_increase(value);
        let clip = ctx.model.note_clip(cols, rows);
        for note_position in self.note_editor.notes() {
            if clip.step(note_position).state() == StepState::Off {
                return;
            }
            match attribute {
                NoteAttribute::Duration => clip.change_step_duration(note_position, value),
                NoteAttribute::Velocity => clip.change_step_velocity(note_position, value),
                NoteAttribute::VelocitySpread => clip.change_step_velocity_spread(note_position, value),
                NoteAttribute::ReleaseVelocity => clip.change_step_release_velocity(note_position, value),
                NoteAttribute::Chance => clip.change_step_chance(note_position, value),
                NoteAttribute::Occurrence => clip.set_step_prev_next_occurrence(note_position, increase),
                NoteAttribute::Gain => clip.change_step_gain(note_position, value),
                NoteAttribute::Panning => clip.change_step_pan(note_position, value),
                NoteAttribute::Transpose => clip.change_step_transpose(note_position, value),
                NoteAttribute::Timbre => clip.change_step_timbre(note_position, value),
                NoteAttribute::Pressure => clip.change_step_pressure(note_position, value),
                NoteAttribute::Repeat => clip.change_step_repeat_count(note_position, value),
                NoteAttribute::RepeatCurve => clip.change_step_repeat_curve(note_position, value),
                NoteAttribute::RepeatVelocityCurve => clip.change_step_repeat_velocity_curve(note_position, value),
                NoteAttribute::RepeatVelocityEnd => clip.change_step_repeat_velocity_end(note_position, value),
            }
        }
    }

    /// Reset the attribute of all edited notes (former `NoteParameter.resetValue`).
    fn reset_value(&self, ctx: &mut Ctx, attribute: Option<NoteAttribute>) {
        let Some(attribute) = attribute.filter(|a| self.supports(ctx, *a)) else {
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
                NoteAttribute::VelocitySpread => clip.update_step_velocity_spread(note_position, 0.0),
                NoteAttribute::ReleaseVelocity => clip.update_step_release_velocity(note_position, 1.0),
                NoteAttribute::Chance => clip.update_step_chance(note_position, 1.0),
                NoteAttribute::Occurrence => clip.set_step_occurrence(note_position, NoteOccurrenceType::Always),
                NoteAttribute::Gain => clip.update_step_gain(note_position, 0.5),
                NoteAttribute::Panning => clip.update_step_pan(note_position, 0.0),
                NoteAttribute::Transpose => clip.update_step_transpose(note_position, 0.0),
                NoteAttribute::Timbre => clip.update_step_timbre(note_position, 0.0),
                NoteAttribute::Pressure => clip.update_step_pressure(note_position, 0.0),
                NoteAttribute::Repeat => clip.update_step_repeat_count(note_position, 0),
                NoteAttribute::RepeatCurve => clip.update_step_repeat_curve(note_position, 0.0),
                NoteAttribute::RepeatVelocityCurve => clip.update_step_repeat_velocity_curve(note_position, 0.0),
                NoteAttribute::RepeatVelocityEnd => clip.update_step_repeat_velocity_end(note_position, 0.0),
            }
        }
    }

    /// The attribute of the knob on the current page (former `pageParamProviders`).
    fn knob_attribute(&self, index: usize) -> Option<NoteAttribute> {
        let idx = self.base.index(index);
        let page = match self.page {
            Page::Note => &NOTE_PAGE,
            Page::Expressions => &EXPRESSIONS_PAGE,
            Page::Repeat => &REPEAT_PAGE,
        };
        page.get(idx.max(0) as usize).copied().flatten()
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
        let base = self.base;
        let page = self.page;
        let supports = |attribute: NoteAttribute| self.supports(ctx, attribute);
        let (supports_spread, supports_release_velocity, supports_chance, supports_occurrence) =
            (supports(NoteAttribute::VelocitySpread), supports(NoteAttribute::ReleaseVelocity), supports(NoteAttribute::Chance), supports(NoteAttribute::Occurrence));
        with_display(ctx, |d| {
            d.clear();
            d.set_cell(0, 0, "Note");
            d.set_cell(1, 0, &note_text);
            d.set_cell(0, 1, &base.mark("Length", 2));
            d.set_cell(1, 1, &length_head);
            d.set_cell(1, 2, &length_tail);

            match page {
                Page::Note => {
                    d.set_cell(0, 3, &base.mark("Velocity", 3)).set_cell(1, 3, &format_percentage(step_info.velocity()));
                    if supports_spread {
                        d.set_cell(0, 4, &base.mark("Spread", 4)).set_cell(1, 4, &format_percentage(step_info.velocity_spread()));
                    }
                    if supports_release_velocity {
                        d.set_cell(0, 5, &base.mark("R-Vel", 5)).set_cell(1, 5, &format_percentage(step_info.release_velocity()));
                    }
                    if supports_chance {
                        d.set_cell(0, 6, &base.mark("Chance", 6)).set_cell(1, 6, &format_percentage(step_info.chance()));
                    }
                    if supports_occurrence {
                        d.set_cell(0, 7, &base.mark("Occurnce", 7)).set_cell(1, 7, &optimize_name(step_info.occurrence().name(), 8));
                    }
                }
                Page::Expressions => {
                    d.set_cell(0, 3, &base.mark("Gain", 3)).set_cell(1, 3, &format_percentage(step_info.gain()));
                    d.set_cell(0, 4, &base.mark("Pan", 4)).set_cell(1, 4, &format_percentage(step_info.pan()));
                    d.set_cell(0, 5, &base.mark("Pitch", 5)).set_cell(1, 5, &format!("{:.1}", step_info.transpose()));
                    d.set_cell(0, 6, &base.mark("Timbre", 6)).set_cell(1, 6, &format_percentage(step_info.timbre()));
                    d.set_cell(0, 7, &base.mark("Pressure", 7)).set_cell(1, 7, &format_percentage(step_info.pressure()));
                }
                Page::Repeat => {
                    d.set_cell(0, 3, &base.mark("Velocity", 3)).set_cell(1, 3, &format_percentage(step_info.velocity()));
                    d.set_cell(0, 4, &base.mark("Count", 4)).set_cell(1, 4, &step_info.formatted_repeat_count());
                    d.set_cell(0, 5, &base.mark("Curve", 5)).set_cell(1, 5, &format_percentage(step_info.repeat_curve()));
                    d.set_cell(0, 6, &base.mark("V-Curve", 6)).set_cell(1, 6, &format_percentage(step_info.repeat_velocity_curve()));
                    d.set_cell(0, 7, &base.mark("V-End", 7)).set_cell(1, 7, &format_percentage(step_info.repeat_velocity_end()));
                }
            }
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

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        // The 'Expressions' and 'Repeat' pages require the timbre attribute, which is not
        // supported by Reaper
        if self.supports(ctx, NoteAttribute::Timbre) {
            match self.page {
                Page::Expressions => self.page = Page::Note,
                Page::Repeat => self.page = Page::Expressions,
                Page::Note => {}
            }
        }
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        if self.supports(ctx, NoteAttribute::Timbre) {
            match self.page {
                Page::Note => self.page = Page::Expressions,
                Page::Expressions => self.page = Page::Repeat,
                Page::Repeat => {}
            }
        }
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.supports(ctx, NoteAttribute::Timbre) && self.page != Page::Note
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.supports(ctx, NoteAttribute::Timbre) && self.page != Page::Repeat
    }

    fn note_editor(&self) -> Option<&NoteEditor> {
        Some(&self.note_editor)
    }

    fn note_editor_mut(&mut self) -> Option<&mut NoteEditor> {
        Some(&mut self.note_editor)
    }

    impl_as_any!();
}
