// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note editing mode (former `NoteMode`): edits the attributes of the notes which a
//! sequencer view put into its note editor.
//!
//! Differences to Java:
//! - The knobs are not bound to note parameters; the mode changes the attributes in
//!   `on_knob_value`, which has the same effect.
//! - The framework `NoteAttribute` lacks occurrence, recurrence length and the repeat curves;
//!   [`NoteKnob`] adds them for this mode (port of the former `NoteParameter` for them).
//! - `host.supports (NoteAttribute)` is [`NoteEditor::is_host_grid`]: the step grid of the host
//!   (Bitwig) supports all attributes, Reaper only duration, velocity and mute.

use super::{add_plain_parameter, forward_base, row_index, set_cell_value};
use crate::controller::push::colors::{PUSH_BLACK, PUSH_BLACK_2, PUSH_GREEN_2, PUSH_GREEN_LO, PUSH_GREY_LO_2, PUSH_ORANGE_HI, PUSH_ORANGE_LO};
use crate::controller::push::mode::PushDisplayMode;
use crate::daw::clip::{CursorClip, NotePosition, StepInfo, StepState};
use crate::daw::color::ColorEx;
use crate::daw::clip::NoteOccurrenceType;
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::Mode;
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::NoteEditor;
use crate::framework::mode::note_parameter::{change_note_value, note_value, NoteAttribute};
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::framework::string_utils::{format_measures, format_percentage};
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

const OFF: &str = "  Off";
const ON: &str = "   On";
const MENU: [&str; 8] = ["Common", "Expressions", "Repeat", " ", " ", " ", " ", "Recurr. Pattern"];
const RECURRENCE_PRESETS: [&str; 8] = ["First", "Not first", " ", "Last", "Not last", " ", "Odd", "Even"];

/// The pages of the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Note,
    RecurrencePattern,
    Expressions,
    Repeat,
}

/// The note attributes edited by the knobs (former `NoteAttribute` as used by `NoteParameter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKnob {
    Duration,
    Mute,
    Velocity,
    VelocitySpread,
    ReleaseVelocity,
    Chance,
    Occurrence,
    RecurrenceLength,
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

impl NoteKnob {
    /// The attribute of the framework, if it has it.
    fn framework(self) -> Option<NoteAttribute> {
        Some(match self {
            NoteKnob::Duration => NoteAttribute::Duration,
            NoteKnob::Mute => NoteAttribute::Mute,
            NoteKnob::Velocity => NoteAttribute::Velocity,
            NoteKnob::VelocitySpread => NoteAttribute::VelocitySpread,
            NoteKnob::ReleaseVelocity => NoteAttribute::ReleaseVelocity,
            NoteKnob::Chance => NoteAttribute::Chance,
            NoteKnob::Gain => NoteAttribute::Gain,
            NoteKnob::Panning => NoteAttribute::Panning,
            NoteKnob::Transpose => NoteAttribute::Transpose,
            NoteKnob::Timbre => NoteAttribute::Timbre,
            NoteKnob::Pressure => NoteAttribute::Pressure,
            NoteKnob::Repeat => NoteAttribute::Repeat,
            NoteKnob::Occurrence | NoteKnob::RecurrenceLength | NoteKnob::RepeatCurve | NoteKnob::RepeatVelocityCurve | NoteKnob::RepeatVelocityEnd => return None,
        })
    }

    /// Former `host.supports (NoteAttribute)`.
    fn is_supported(self, is_host_grid: bool) -> bool {
        is_host_grid || matches!(self, NoteKnob::Duration | NoteKnob::Velocity | NoteKnob::Mute)
    }
}

/// The attributes of the knobs of a page (former `pageParamProviders`).
pub fn page_knobs(page: Page) -> [Option<NoteKnob>; 8] {
    use NoteKnob::*;
    match page {
        Page::Note => [Some(Duration), Some(Mute), Some(Velocity), Some(VelocitySpread), Some(ReleaseVelocity), Some(Chance), Some(Occurrence), Some(RecurrenceLength)],
        Page::Expressions => [Some(Duration), Some(Mute), None, Some(Gain), Some(Panning), Some(Transpose), Some(Timbre), Some(Pressure)],
        Page::Repeat => [Some(Duration), Some(Mute), None, Some(Repeat), Some(RepeatCurve), Some(RepeatVelocityCurve), Some(RepeatVelocityEnd), None],
        Page::RecurrencePattern => [None, None, None, None, None, None, None, Some(RecurrenceLength)],
    }
}

fn recurrence_text(recurrence: i32) -> String {
    if recurrence < 2 {
        "Off".to_string()
    } else {
        recurrence.to_string()
    }
}

/// Edit the attributes of the selected notes.
pub struct NoteMode {
    base: ParameterModeBase,
    page: Page,
    note_editor: NoteEditor,
}

impl NoteMode {
    pub fn new() -> Self {
        NoteMode { base: super::base("Note"), page: Page::Note, note_editor: NoteEditor::new() }
    }

    /// The selected page.
    pub fn page(&self) -> Page {
        self.page
    }

    fn supports(&self, ctx: &Ctx, knob: NoteKnob) -> bool {
        knob.is_supported(self.note_editor.is_host_grid(&ctx.model))
    }

    /// The clip and the first edited note, if there is one.
    fn first_note<'a>(&self, ctx: &'a Ctx) -> Option<(&'a CursorClip, NotePosition)> {
        let (cols, rows) = self.note_editor.clip()?;
        let position = *self.note_editor.notes().first()?;
        let clip = ctx.model.note_clip_ref(cols, rows)?;
        Some((clip, position))
    }

    fn format_length(ctx: &Ctx, duration: f64) -> String {
        format_measures(ctx.model.transport().quarters_per_measure(), duration, 0, true)
    }

    fn recurrence_value(ctx: &Ctx, recurrence: i32) -> i32 {
        (recurrence - 1) * (ctx.model.value_changer().upper_bound() - 1) / 7
    }

    /// Former `NoteParameter.changeValue` for all edited notes.
    fn change_value(&self, ctx: &mut Ctx, knob: NoteKnob, control: i32) {
        let editor = self.note_editor.clone();
        if let Some(attribute) = knob.framework() {
            change_note_value(ctx, &editor, attribute, control);
            return;
        }
        if !self.supports(ctx, knob) {
            return;
        }
        let Some((cols, rows)) = editor.clip() else {
            return;
        };
        let is_increase = ctx.model.value_changer().is_increase(control);
        for position in editor.notes().to_vec() {
            let clip = ctx.model.note_clip(cols, rows);
            if clip.step(&position).state() == StepState::Off {
                return;
            }
            let message = match knob {
                NoteKnob::RepeatCurve => {
                    clip.change_step_repeat_curve(&position, control);
                    format!("Curve: {}", format_percentage(clip.step(&position).repeat_curve()))
                }
                NoteKnob::RepeatVelocityCurve => {
                    clip.change_step_repeat_velocity_curve(&position, control);
                    format!("Vel-Crve: {}", format_percentage(clip.step(&position).repeat_velocity_curve()))
                }
                NoteKnob::RepeatVelocityEnd => {
                    clip.change_step_repeat_velocity_end(&position, control);
                    format!("Vel. End: {}", format_percentage(clip.step(&position).repeat_velocity_end()))
                }
                NoteKnob::Occurrence => {
                    clip.set_step_prev_next_occurrence(&position, is_increase);
                    format!("Occurrence: {}", clip.step(&position).occurrence().name())
                }
                NoteKnob::RecurrenceLength => {
                    clip.change_step_recurrence_length(&position, control);
                    format!("Recurrence: {}", recurrence_text(clip.step(&position).recurrence_length()))
                }
                _ => continue,
            };
            ctx.notify(&message);
        }
    }

    /// Former `NoteParameter.resetValue` for all edited notes.
    fn reset_value(&self, ctx: &mut Ctx, knob: NoteKnob) {
        if !self.supports(ctx, knob) {
            return;
        }
        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        for position in self.note_editor.notes().to_vec() {
            let clip = ctx.model.note_clip(cols, rows);
            if clip.step(&position).state() == StepState::Off {
                return;
            }
            match knob {
                NoteKnob::Gain => clip.update_step_gain(&position, 0.5),
                NoteKnob::Panning => clip.update_step_pan(&position, 0.0),
                NoteKnob::Duration => clip.update_step_duration(&position, 1.0),
                NoteKnob::Velocity => clip.update_step_velocity(&position, 1.0),
                NoteKnob::ReleaseVelocity => clip.update_step_release_velocity(&position, 1.0),
                NoteKnob::VelocitySpread => clip.update_step_velocity_spread(&position, 0.0),
                NoteKnob::Mute => clip.update_step_mute_state(&position, false),
                NoteKnob::Pressure => clip.update_step_pressure(&position, 0.0),
                NoteKnob::Timbre => clip.update_step_timbre(&position, 0.0),
                NoteKnob::Transpose => clip.update_step_transpose(&position, 0.0),
                NoteKnob::Chance => clip.update_step_chance(&position, 1.0),
                NoteKnob::Repeat => clip.update_step_repeat_count(&position, 0),
                NoteKnob::RepeatCurve => clip.update_step_repeat_curve(&position, 0.0),
                NoteKnob::RepeatVelocityCurve => clip.update_step_repeat_velocity_curve(&position, 0.0),
                NoteKnob::RepeatVelocityEnd => clip.update_step_repeat_velocity_end(&position, 0.0),
                NoteKnob::Occurrence => clip.set_step_occurrence(&position, NoteOccurrenceType::Always),
                NoteKnob::RecurrenceLength => clip.update_step_recurrence_length(&position, 1),
            }
        }
    }

    /// Former `NoteParameter.getValue`.
    fn value(&self, ctx: &Ctx, knob: NoteKnob) -> i32 {
        if let Some(attribute) = knob.framework() {
            return note_value(ctx, &self.note_editor, attribute);
        }
        let Some((clip, position)) = self.first_note(ctx) else {
            return 0;
        };
        let step = clip.step(&position);
        if step.state() == StepState::Off || !self.supports(ctx, knob) {
            return 0;
        }
        let normalized = match knob {
            NoteKnob::RepeatCurve => step.repeat_curve(),
            NoteKnob::RepeatVelocityCurve => step.repeat_velocity_curve(),
            NoteKnob::RepeatVelocityEnd => (step.repeat_velocity_end() + 1.0) / 2.0,
            NoteKnob::Occurrence => {
                let index = NoteOccurrenceType::ALL.iter().position(|t| *t == step.occurrence()).unwrap_or(0);
                index as f64 / NoteOccurrenceType::ALL.len() as f64
            }
            NoteKnob::RecurrenceLength => (step.recurrence_length() - 1) as f64 / 7.0,
            _ => 0.0,
        };
        ctx.model.value_changer().from_normalized_value(normalized)
    }

    /// Former `rebind`: the knobs are handled by the mode itself.
    fn set_page(&mut self, page: Page) {
        self.page = page;
    }

    fn update_display1_page(&self, ctx: &Ctx, display: &mut TextDisplay, clip: &CursorClip, step: &StepInfo) {
        let value_changer = ctx.model.value_changer();
        let is_host_grid = self.note_editor.is_host_grid(&ctx.model);
        let supports = |knob: NoteKnob| knob.is_supported(is_host_grid);
        match self.page {
            Page::Note => {
                display.set_cell(0, 1, " COMMON:");
                let velocity = step.velocity();
                display.set_cell(0, 2, "Velocity");
                display.set_cell(1, 2, &format_percentage(velocity));
                set_cell_value(ctx, display, 2, 2, value_changer.from_normalized_value(velocity), Format::Value);
                if supports(NoteKnob::VelocitySpread) {
                    let spread = step.velocity_spread();
                    display.set_cell(0, 3, "V-Spread");
                    display.set_cell(1, 3, &format_percentage(spread));
                    set_cell_value(ctx, display, 2, 3, value_changer.from_normalized_value(spread), Format::Value);
                }
                if supports(NoteKnob::ReleaseVelocity) {
                    let release = step.release_velocity();
                    display.set_cell(0, 4, "R-Velcty");
                    display.set_cell(1, 4, &format_percentage(release));
                    set_cell_value(ctx, display, 2, 4, value_changer.from_normalized_value(release), Format::Value);
                }
                if supports(NoteKnob::Chance) {
                    let chance = step.chance();
                    display.set_cell(0, 5, "Chance");
                    display.set_cell(1, 5, &format_percentage(chance));
                    set_cell_value(ctx, display, 2, 5, value_changer.from_normalized_value(chance), Format::Value);
                    display.set_cell(3, 5, if step.is_chance_enabled() { ON } else { OFF });
                }
                if supports(NoteKnob::Occurrence) {
                    display.set_cell(0, 6, "Occurnce");
                    display.set_cell(1, 6, &optimize_name(step.occurrence().name(), 8));
                    display.set_cell(3, 6, if step.is_occurrence_enabled() { ON } else { OFF });
                }
                if supports(NoteKnob::RecurrenceLength) {
                    let recurrence = step.recurrence_length();
                    display.set_cell(0, 7, "Recurnce");
                    display.set_cell(1, 7, &recurrence_text(recurrence));
                    set_cell_value(ctx, display, 2, 7, Self::recurrence_value(ctx, recurrence), Format::Value);
                    display.set_cell(3, 7, if step.is_recurrence_enabled() { ON } else { OFF });
                }
            }
            Page::Expressions => {
                if supports(NoteKnob::Timbre) {
                    display.set_cell(0, 2, "EXPRESS:");
                    let gain = step.gain();
                    display.set_cell(0, 3, "Gain").set_cell(1, 3, &format_percentage(gain));
                    set_cell_value(ctx, display, 2, 3, value_changer.from_normalized_value(gain).min(1023), Format::Value);
                    let pan = step.pan();
                    display.set_cell(0, 4, "Pan").set_cell(1, 4, &format_percentage(pan));
                    set_cell_value(ctx, display, 2, 4, value_changer.from_normalized_value((pan + 1.0) / 2.0), Format::Pan);
                    let transpose = step.transpose();
                    let range = clip.step_transpose_range();
                    display.set_cell(0, 5, "Pitch").set_cell(1, 5, &format!("{transpose:.1}"));
                    set_cell_value(ctx, display, 2, 5, value_changer.from_normalized_value((transpose + range) / (2.0 * range)), Format::Pan);
                    let timbre = step.timbre();
                    display.set_cell(0, 6, "Timbre").set_cell(1, 6, &format_percentage(timbre));
                    set_cell_value(ctx, display, 2, 6, value_changer.from_normalized_value((timbre + 1.0) / 2.0), Format::Value);
                    let pressure = step.pressure();
                    display.set_cell(0, 7, "Pressure").set_cell(1, 7, &format_percentage(pressure));
                    set_cell_value(ctx, display, 2, 7, value_changer.from_normalized_value(pressure), Format::Value);
                }
            }
            Page::Repeat => {
                display.set_cell(0, 2, "REPEAT:");
                let rc = (step.repeat_count() + 127) * (value_changer.upper_bound() - 1) / 254;
                display.set_cell(0, 3, "Count");
                display.set_cell(1, 3, &step.formatted_repeat_count());
                set_cell_value(ctx, display, 2, 3, rc, Format::Value);
                display.set_cell(3, 3, if step.is_repeat_enabled() { ON } else { OFF });
                let curve = step.repeat_curve();
                display.set_cell(0, 4, "Curve");
                display.set_cell(1, 4, &format_percentage(curve));
                set_cell_value(ctx, display, 2, 4, value_changer.from_normalized_value((curve + 1.0) / 2.0), Format::Value);
                let velocity_curve = step.repeat_velocity_curve();
                display.set_cell(0, 5, "Vel-Crve");
                display.set_cell(1, 5, &format_percentage(velocity_curve));
                set_cell_value(ctx, display, 2, 5, value_changer.from_normalized_value((velocity_curve + 1.0) / 2.0), Format::Value);
                let velocity_end = step.repeat_velocity_end();
                display.set_cell(0, 6, "Vel. End");
                display.set_cell(1, 6, &format_percentage(velocity_end));
                set_cell_value(ctx, display, 2, 6, value_changer.from_normalized_value((velocity_end + 1.0) / 2.0), Format::Value);
            }
            Page::RecurrencePattern => {
                display.set_block(0, 1, "       Recurrence");
                display.set_block(0, 2, "Pattern");
                let recurrence_length = step.recurrence_length();
                let mask = step.recurrence_mask();
                let is_shift = ctx.surface.is_shift_pressed();
                for (i, preset) in RECURRENCE_PRESETS.iter().enumerate() {
                    let mut label = "   -";
                    if is_shift {
                        label = preset;
                    } else if (i as i32) < recurrence_length {
                        label = if mask & (1 << i) > 0 { ON } else { OFF };
                    }
                    if i == 7 {
                        display.set_cell(0, 7, "Recurnce");
                        display.set_cell(1, 7, &recurrence_text(recurrence_length));
                        set_cell_value(ctx, display, 2, 7, Self::recurrence_value(ctx, recurrence_length), Format::Value);
                    }
                    display.set_cell(3, i, label);
                }
            }
        }
    }

    fn update_display2_page(&self, ctx: &Ctx, display: &mut GraphicDisplay, clip: &CursorClip, step: &StepInfo) {
        let value_changer = ctx.model.value_changer();
        let is_host_grid = self.note_editor.is_host_grid(&ctx.model);
        let supports = |knob: NoteKnob| knob.is_supported(is_host_grid);
        let touched = |index: usize| self.base.is_knob_touched(index);
        let on_off = |on: bool| if on { "On" } else { "Off" };
        match self.page {
            Page::Note => {
                let velocity = step.velocity();
                let value = value_changer.from_normalized_value(velocity);
                add_plain_parameter(display, if supports(NoteKnob::Repeat) { MENU[2] } else { " " }, false, "", None, false, "Velocity", value, &format_percentage(velocity), touched(2), value);
                if supports(NoteKnob::VelocitySpread) {
                    let spread = step.velocity_spread();
                    let value = value_changer.from_normalized_value(spread);
                    add_plain_parameter(display, MENU[3], false, "", None, false, "Vel-Spread", value, &format_percentage(spread), touched(3), value);
                } else {
                    display.add_empty_element_with_small_menu(true);
                }
                if supports(NoteKnob::ReleaseVelocity) {
                    let release = step.release_velocity();
                    let value = value_changer.from_normalized_value(release);
                    add_plain_parameter(display, MENU[4], false, "", None, false, "R-Velocity", value, &format_percentage(release), touched(4), value);
                } else {
                    display.add_empty_element_with_small_menu(true);
                }
                if supports(NoteKnob::Chance) {
                    let chance = step.chance();
                    let value = value_changer.from_normalized_value(chance);
                    add_plain_parameter(display, MENU[5], false, on_off(step.is_chance_enabled()), None, false, "Chance", value, &format_percentage(chance), touched(5), value);
                } else {
                    display.add_empty_element_with_small_menu(true);
                }
                if supports(NoteKnob::Occurrence) {
                    let name = optimize_name(step.occurrence().name(), 9);
                    add_plain_parameter(display, MENU[6], false, on_off(step.is_occurrence_enabled()), None, false, "Occurrence", -1, &name, touched(6), -1);
                } else {
                    display.add_empty_element_with_small_menu(true);
                }
                if supports(NoteKnob::RecurrenceLength) {
                    let recurrence = step.recurrence_length();
                    let value = Self::recurrence_value(ctx, recurrence);
                    add_plain_parameter(display, MENU[7], false, on_off(step.is_recurrence_enabled()), None, false, "Recurrence", value, &recurrence_text(recurrence), touched(7), value);
                } else {
                    display.add_empty_element_with_small_menu(true);
                }
            }
            Page::Expressions => {
                add_plain_parameter(display, MENU[2], false, "", None, false, "", -1, "", false, -1);
                let gain = step.gain();
                let value = value_changer.from_normalized_value(gain).min(1023);
                add_plain_parameter(display, MENU[3], false, "", None, false, "Gain", value, &format_percentage(gain), touched(3), value);
                let pan = step.pan();
                let value = value_changer.from_normalized_value((pan + 1.0) / 2.0);
                add_plain_parameter(display, MENU[4], false, "", None, false, "Pan", value, &format_percentage(pan), touched(4), value);
                let transpose = step.transpose();
                let range = clip.step_transpose_range();
                let value = value_changer.from_normalized_value((transpose + range) / (2.0 * range));
                add_plain_parameter(display, MENU[5], false, "", None, false, "Pitch", value, &format!("{transpose:.1}"), touched(5), value);
                let timbre = step.timbre();
                let value = value_changer.from_normalized_value((timbre + 1.0) / 2.0);
                add_plain_parameter(display, MENU[6], false, "", None, false, "Timbre", value, &format_percentage(timbre), touched(6), value);
                let pressure = step.pressure();
                let value = value_changer.from_normalized_value(pressure);
                add_plain_parameter(display, MENU[7], false, "", None, false, "Pressure", value, &format_percentage(pressure), touched(7), value);
            }
            Page::Repeat => {
                add_plain_parameter(display, MENU[2], true, "", None, false, "", -1, "", false, -1);
                let rc = (step.repeat_count() + 127) * (value_changer.upper_bound() - 1) / 254;
                add_plain_parameter(display, MENU[3], false, on_off(step.is_repeat_enabled()), None, false, "Count", rc, &step.formatted_repeat_count(), touched(3), rc);
                let curve = step.repeat_curve();
                let value = value_changer.from_normalized_value((curve + 1.0) / 2.0);
                add_plain_parameter(display, MENU[4], false, "", None, false, "Curve", value, &format_percentage(curve), touched(4), value);
                let velocity_curve = step.repeat_velocity_curve();
                let value = value_changer.from_normalized_value((velocity_curve + 1.0) / 2.0);
                add_plain_parameter(display, MENU[5], false, "", None, false, "Vel. Curve", value, &format_percentage(velocity_curve), touched(5), value);
                let velocity_end = step.repeat_velocity_end();
                let value = value_changer.from_normalized_value((velocity_end + 1.0) / 2.0);
                add_plain_parameter(display, MENU[6], false, "", None, false, "Vel. End", value, &format_percentage(velocity_end), touched(6), value);
                add_plain_parameter(display, MENU[7], false, "", None, false, "", -1, "", false, -1);
            }
            Page::RecurrencePattern => {
                let recurrence_length = step.recurrence_length();
                let mask = step.recurrence_mask();
                let is_shift = ctx.surface.is_shift_pressed();
                for i in 0..8 {
                    let mut color = Some(ColorEx::BLACK);
                    let mut label = "-";
                    if is_shift {
                        color = Some(if RECURRENCE_PRESETS[i].trim().is_empty() { ColorEx::BLACK } else { ColorEx::GRAY });
                        label = RECURRENCE_PRESETS[i];
                    } else if (i as i32) < recurrence_length {
                        let is_on = mask & (1 << i) > 0;
                        color = if is_on { Some(ColorEx::ORANGE) } else { None };
                        label = if is_on { "On" } else { "Off" };
                    }
                    if i == 7 {
                        let value = Self::recurrence_value(ctx, recurrence_length);
                        add_plain_parameter(display, MENU[i], true, label, color, false, "Recurrence", value, &recurrence_text(recurrence_length), touched(7), value);
                    } else {
                        add_plain_parameter(display, MENU[i], false, label, color, false, "", -1, "", false, -1);
                    }
                }
            }
        }
    }
}

impl Default for NoteMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for NoteMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let Some((clip, position)) = self.first_note(ctx) else {
            display.set_row(1, "                     Please selecta note...                         ");
            return;
        };
        let step = clip.step(&position).clone();
        if self.page != Page::RecurrencePattern {
            display.set_cell(0, 0, "Length").set_cell(1, 0, &Self::format_length(ctx, step.duration()));
            if step.is_muted() {
                display.set_cell(2, 1, " MUTED");
            }
            let size = self.note_editor.notes().len();
            let is_one_note = size == 1;
            display.set_cell(3, 0, &if is_one_note { format!("Step: {}", position.step + 1) } else { format!("Notes: {size}") });
            display.set_cell(3, 1, &if is_one_note { Scales::format_note_and_octave(position.note, -3) } else { "*".to_string() });
        }
        self.update_display1_page(ctx, display, clip, &step);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let Some((clip, position)) = self.first_note(ctx) else {
            for i in 0..8 {
                display.add_option_element(if i == 2 { "Please select a note to edit..." } else { "" }, "", false, "", "", false, true);
            }
            return;
        };
        let step = clip.step(&position).clone();
        if self.page != Page::RecurrencePattern {
            let size = self.note_editor.notes().len();
            let is_one_note = size == 1;
            let step_bottom_menu = if is_one_note { format!("Step: {}", position.step + 1) } else { format!("Notes: {size}") };
            add_plain_parameter(display, MENU[0], self.page == Page::Note, &step_bottom_menu, None, false, "Length", -1, &Self::format_length(ctx, step.duration()), self.base.is_knob_touched(0), -1);
            let has_expressions = self.supports(ctx, NoteKnob::Timbre);
            let top_menu = if has_expressions { MENU[1] } else { " " };
            let is_top_menu_on = has_expressions && self.page == Page::Expressions;
            let bottom_menu = if is_one_note { Scales::format_note_and_octave(position.note, -3) } else { "*".to_string() };
            if self.supports(ctx, NoteKnob::Mute) {
                let value = if step.is_muted() { ctx.model.value_changer().upper_bound() } else { 0 };
                add_plain_parameter(display, top_menu, is_top_menu_on, &bottom_menu, None, false, "Is Muted?", value, if step.is_muted() { "Yes" } else { "No" }, self.base.is_knob_touched(1), value);
            } else {
                add_plain_parameter(display, top_menu, is_top_menu_on, &bottom_menu, None, false, "", -1, "", false, -1);
            }
        }
        self.update_display2_page(ctx, display, clip, &step);
    }
}

impl Mode for NoteMode {
    forward_base!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if let Some(Some(knob)) = page_knobs(self.page).get(index) {
            self.change_value(ctx, *knob, value);
        }
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        match page_knobs(self.page).get(index) {
            Some(Some(knob)) => self.value(ctx, *knob),
            _ => -1,
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        // As in Java the touch state is not stored
        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        if self.note_editor.notes().is_empty() {
            return;
        }
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            if let Some(Some(knob)) = page_knobs(self.page).get(index) {
                self.reset_value(ctx, *knob);
            }
            return;
        }
        let notes = self.note_editor.notes().to_vec();
        let clip = ctx.model.note_clip(cols, rows);
        if is_touched {
            clip.start_edit(&notes);
        } else {
            clip.stop_edit();
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if row != 0 {
            match index {
                0 => self.set_page(Page::Note),
                1 if self.supports(ctx, NoteKnob::Timbre) => self.set_page(Page::Expressions),
                2 if self.supports(ctx, NoteKnob::Repeat) => self.set_page(Page::Repeat),
                7 if self.supports(ctx, NoteKnob::RecurrenceLength) => self.set_page(Page::RecurrencePattern),
                _ => {}
            }
            return;
        }

        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        let is_shift = ctx.surface.is_shift_pressed();
        let page = self.page;
        let supports_chance = self.supports(ctx, NoteKnob::Chance);
        let supports_occurrence = self.supports(ctx, NoteKnob::Occurrence);
        let supports_recurrence = self.supports(ctx, NoteKnob::RecurrenceLength);
        let supports_repeat = self.supports(ctx, NoteKnob::Repeat);
        for position in self.note_editor.notes().to_vec() {
            let clip = ctx.model.note_clip(cols, rows);
            let step = clip.step(&position).clone();
            match page {
                Page::Note => match index {
                    5 => {
                        if supports_chance {
                            clip.update_step_is_chance_enabled(&position, !step.is_chance_enabled());
                        }
                    }
                    6 => {
                        if supports_occurrence {
                            clip.update_step_is_occurrence_enabled(&position, !step.is_occurrence_enabled());
                        }
                    }
                    7 => {
                        if supports_recurrence {
                            clip.update_step_is_recurrence_enabled(&position, !step.is_recurrence_enabled());
                        }
                    }
                    _ => return,
                },
                Page::Expressions => {}
                Page::Repeat => {
                    if index == 3 && supports_repeat {
                        clip.update_step_is_repeat_enabled(&position, !step.is_repeat_enabled());
                    }
                }
                Page::RecurrencePattern => {
                    if !supports_recurrence {
                        continue;
                    }
                    if !is_shift {
                        clip.update_step_recurrence_mask_toggle_bit(&position, index as i32);
                        continue;
                    }
                    let length = step.recurrence_length();
                    match index {
                        // First
                        0 => clip.update_step_recurrence_mask(&position, 1),
                        // Not first
                        1 => clip.update_step_recurrence_mask(&position, 254),
                        // Last
                        3 => clip.update_step_recurrence_mask(&position, 1 << (length - 1)),
                        // Not last
                        4 => clip.update_step_recurrence_mask(&position, (1 << (length - 1)) - 1),
                        // Even
                        6 => clip.update_step_recurrence_mask(&position, 85),
                        // Odd
                        7 => clip.update_step_recurrence_mask(&position, 170),
                        _ => {}
                    }
                }
            }
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let Some((clip, position)) = self.first_note(ctx) else {
            return PUSH_BLACK;
        };
        let step = clip.step(&position);
        if let Some(index) = row_index(0, button) {
            let orange = |on: bool| if on { PUSH_ORANGE_HI } else { PUSH_ORANGE_LO };
            match self.page {
                Page::Note => {
                    if index == 5 && self.supports(ctx, NoteKnob::Chance) {
                        return orange(step.is_chance_enabled());
                    }
                    if index == 6 && self.supports(ctx, NoteKnob::Occurrence) {
                        return orange(step.is_occurrence_enabled());
                    }
                    if index == 7 && self.supports(ctx, NoteKnob::RecurrenceLength) {
                        return orange(step.is_recurrence_enabled());
                    }
                }
                Page::Expressions => {}
                Page::Repeat => {
                    if index == 3 {
                        return orange(step.is_repeat_enabled());
                    }
                }
                Page::RecurrencePattern => {
                    if ctx.surface.is_shift_pressed() {
                        return if RECURRENCE_PRESETS[index].trim().is_empty() { PUSH_BLACK } else { PUSH_GREEN_LO };
                    }
                    if (index as i32) < step.recurrence_length() {
                        return orange(step.recurrence_mask() & (1 << index) > 0);
                    }
                    return PUSH_BLACK;
                }
            }
            return PUSH_BLACK;
        }
        if let Some(index) = row_index(1, button) {
            let selected = match self.page {
                Page::Note => 0,
                Page::Expressions => 1,
                Page::Repeat => 2,
                Page::RecurrencePattern => 7,
            };
            if index == selected {
                return PUSH_GREEN_2;
            }
            if index == 0 || index == 1 && self.supports(ctx, NoteKnob::Timbre) {
                return PUSH_GREY_LO_2;
            }
            if index == 2 && self.supports(ctx, NoteKnob::Repeat) || index == 7 && self.supports(ctx, NoteKnob::RecurrenceLength) {
                return PUSH_GREY_LO_2;
            }
            return PUSH_BLACK_2;
        }
        PUSH_BLACK
    }

    fn note_editor(&self) -> Option<&NoteEditor> {
        Some(&self.note_editor)
    }

    fn note_editor_mut(&mut self) -> Option<&mut NoteEditor> {
        Some(&mut self.note_editor)
    }

    impl_as_any!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_and_support() {
        assert_eq!(page_knobs(Page::Note)[7], Some(NoteKnob::RecurrenceLength));
        assert_eq!(page_knobs(Page::RecurrencePattern).iter().filter(|k| k.is_some()).count(), 1);
        assert!(NoteKnob::Velocity.is_supported(false));
        assert!(!NoteKnob::Occurrence.is_supported(false));
        assert!(NoteKnob::Occurrence.is_supported(true));
        assert_eq!(NoteKnob::Gain.framework(), Some(NoteAttribute::Gain));
        assert_eq!(NoteKnob::RepeatCurve.framework(), None);
        assert_eq!(recurrence_text(1), "Off");
        assert_eq!(recurrence_text(4), "4");
    }
}
