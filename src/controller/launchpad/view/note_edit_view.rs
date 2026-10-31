// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Edit the attributes of the notes selected in a sequencer with the virtual faders (former
//! `NoteEditView`, which extends `AbstractFaderView`).

use crate::controller::launchpad::colors::{
    LAUNCHPAD_COLOR_BLACK, LAUNCHPAD_COLOR_BLUE, LAUNCHPAD_COLOR_GREY_LO, LAUNCHPAD_COLOR_OCEAN, LAUNCHPAD_COLOR_ORANGE, LAUNCHPAD_COLOR_PINK, LAUNCHPAD_COLOR_RED_HI, LAUNCHPAD_COLOR_ROSE,
    LAUNCHPAD_COLOR_TURQUOISE, LAUNCHPAD_COLOR_YELLOW,
};
use super::abstract_fader_view::{self, FaderView, FaderViewBase};
use super::session_view::session_scroll_states;
use crate::controller::launchpad::surface as lp;
use crate::daw::clip::{CursorClip, SessionAlternative, StepState};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::NoteEditor;
use crate::framework::virtual_fader::VirtualFaderView;
use crate::midi::ButtonEvent;

const COLUMN_COLORS: [i32; 8] = [
    LAUNCHPAD_COLOR_BLUE,
    LAUNCHPAD_COLOR_OCEAN,
    LAUNCHPAD_COLOR_PINK,
    LAUNCHPAD_COLOR_BLUE,
    LAUNCHPAD_COLOR_ORANGE,
    LAUNCHPAD_COLOR_TURQUOISE,
    LAUNCHPAD_COLOR_YELLOW,
    LAUNCHPAD_COLOR_ROSE,
];

/// The note attributes which are edited by the faders (subset of the former `NoteAttribute`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteAttribute {
    Velocity,
    VelocitySpread,
    Chance,
    Gain,
    Panning,
    Transpose,
    Timbre,
    Pressure,
    Mute,
}

impl NoteAttribute {
    /// Is the attribute supported by the host (former `IHost.supports (NoteAttribute)`)? With
    /// the step grid of the host (Bitwig) all attributes are supported, Reaper supports only
    /// pitch, duration, mute and velocity.
    pub fn is_supported(self, is_host_grid: bool) -> bool {
        is_host_grid || matches!(self, NoteAttribute::Velocity | NoteAttribute::Mute)
    }
}

/// The attributes of the 8 faders.
const PARAMETERS: [NoteAttribute; 8] = [
    NoteAttribute::Velocity,
    NoteAttribute::VelocitySpread,
    NoteAttribute::Chance,
    NoteAttribute::Gain,
    NoteAttribute::Panning,
    NoteAttribute::Transpose,
    NoteAttribute::Timbre,
    NoteAttribute::Pressure,
];

/// Edit the attributes of the notes selected in a sequencer.
pub struct NoteEditView {
    fader: FaderViewBase,
    note_editor: NoteEditor,
}

impl NoteEditView {
    pub fn new(_ctx: &Ctx) -> Self {
        NoteEditView { fader: FaderViewBase::new("Note Edit"), note_editor: NoteEditor::new() }
    }

    /// Former `host.supports (NoteAttribute)`.
    fn supports(&self, ctx: &Ctx, attribute: NoteAttribute) -> bool {
        attribute.is_supported(self.note_editor.is_host_grid(&ctx.model))
    }

    /// Is the fader of the column a panorama fader (former `columnPan`)? The Java version
    /// decides it in the constructor, the host is only known when the clip data arrives.
    fn column_pan(&self, ctx: &Ctx, index: usize) -> bool {
        match index {
            3 => self.supports(ctx, NoteAttribute::Gain),
            4 => self.supports(ctx, NoteAttribute::Panning),
            5 => self.supports(ctx, NoteAttribute::Transpose),
            6 => self.supports(ctx, NoteAttribute::Timbre),
            _ => false,
        }
    }

    fn clip<'a>(&self, ctx: &'a Ctx) -> Option<&'a CursorClip> {
        let (cols, rows) = self.note_editor.clip()?;
        ctx.model.note_clip_ref(cols, rows)
    }

    /// Get the value (0-127) of the attribute of the first edited note (former
    /// `NoteParameter.getValue`).
    pub fn parameter_value(&self, ctx: &Ctx, attribute: NoteAttribute) -> i32 {
        let Some(note_position) = self.note_editor.note_positions(-1).first() else {
            return 0;
        };
        if note_position.note == -1 {
            return 0;
        }
        let Some(clip) = self.clip(ctx) else {
            return 0;
        };
        let step_info = clip.step(note_position);
        if step_info.state() == StepState::Off || !self.supports(ctx, attribute) {
            return 0;
        }
        let normalized_value = match attribute {
            NoteAttribute::Velocity => step_info.velocity(),
            NoteAttribute::VelocitySpread => step_info.velocity_spread(),
            NoteAttribute::Chance => step_info.chance(),
            NoteAttribute::Gain => step_info.gain(),
            NoteAttribute::Panning => (step_info.pan() + 1.0) / 2.0,
            NoteAttribute::Transpose => {
                let transpose_range = clip.step_transpose_range();
                (step_info.transpose() + transpose_range) / (2.0 * transpose_range)
            }
            NoteAttribute::Timbre => step_info.timbre(),
            NoteAttribute::Pressure => step_info.pressure(),
            NoteAttribute::Mute => {
                if step_info.is_muted() {
                    1.0
                } else {
                    0.0
                }
            }
        };
        ctx.model.value_changer().from_normalized_value(normalized_value)
    }

    /// Set the value (0-127) of the attribute of all edited notes (former
    /// `NoteParameter.setValue`).
    pub fn set_parameter_value(&self, ctx: &mut Ctx, attribute: NoteAttribute, value: i32) {
        let normalized_value = ctx.model.value_changer().to_normalized_value(value as f64);
        self.set_parameter_normalized_value(ctx, attribute, normalized_value);
    }

    /// Former `NoteParameter.setNormalizedValue`.
    pub fn set_parameter_normalized_value(&self, ctx: &mut Ctx, attribute: NoteAttribute, normalized_value: f64) {
        if !self.supports(ctx, attribute) {
            return;
        }
        let Some((cols, rows)) = self.note_editor.clip() else {
            return;
        };
        let clip = ctx.model.note_clip(cols, rows);
        for note_position in self.note_editor.note_positions(-1) {
            if clip.step(note_position).state() == StepState::Off {
                return;
            }
            match attribute {
                NoteAttribute::Velocity => clip.update_step_velocity(note_position, normalized_value),
                NoteAttribute::VelocitySpread => clip.update_step_velocity_spread(note_position, normalized_value),
                NoteAttribute::Chance => clip.update_step_chance(note_position, normalized_value),
                NoteAttribute::Gain => clip.update_step_gain(note_position, normalized_value),
                NoteAttribute::Panning => clip.update_step_pan(note_position, normalized_value * 2.0 - 1.0),
                NoteAttribute::Transpose => {
                    let transpose_range = clip.step_transpose_range();
                    clip.update_step_transpose(note_position, normalized_value * 2.0 * transpose_range - transpose_range);
                }
                NoteAttribute::Timbre => clip.update_step_timbre(note_position, normalized_value),
                NoteAttribute::Pressure => clip.update_step_pressure(note_position, normalized_value),
                NoteAttribute::Mute => clip.update_step_mute_state(note_position, normalized_value > 0.5),
            }
        }
    }

}

impl FaderView for NoteEditView {
    fn setup_fader(&self, ctx: &mut Ctx, index: usize) {
        let is_pan = self.column_pan(ctx, index);
        lp::setup_fader(ctx, index, COLUMN_COLORS[index], is_pan);
        // Prevent issue with catch mode by initializing fader value at setup
        let value = self.fader_value(ctx, index);
        self.on_value_knob(ctx, index, value);
    }
}

impl VirtualFaderView for NoteEditView {
    fn fader_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.parameter_value(ctx, PARAMETERS[index])
    }

    fn on_value_knob(&self, ctx: &mut Ctx, index: usize, value: i32) {
        // Set immediately to prevent issue with relative scaling mode
        self.set_parameter_value(ctx, PARAMETERS[index], value);
    }
}

impl View for NoteEditView {
    fn name(&self) -> &str {
        &self.fader.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.fader.on_activate(ctx);
        abstract_fader_view::setup_faders(ctx, &*self);
    }

    fn on_deactivate(&mut self, _ctx: &mut Ctx) {
        // Clear the edited notes
        self.note_editor.clear_notes();
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.fader.base.clear_note_mapping(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if button == ButtonId::Scene1 {
            return if self.parameter_value(ctx, NoteAttribute::Mute) > 0 { LAUNCHPAD_COLOR_RED_HI } else { LAUNCHPAD_COLOR_GREY_LO };
        }
        LAUNCHPAD_COLOR_BLACK
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        for i in 0..8 {
            let value = self.fader_value(ctx, i);
            lp::set_fader_value(ctx, i, value);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        abstract_fader_view::on_grid_note(ctx, &*self, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, _ctx: &mut Ctx, _note: i32) {
        // Prevent clip selection
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down && button == ButtonId::Scene1 {
            let is_muted = self.parameter_value(ctx, NoteAttribute::Mute) > 0;
            self.set_parameter_normalized_value(ctx, NoteAttribute::Mute, if is_muted { 0.0 } else { 1.0 });
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(session_scroll_states(ctx))
    }

    fn session_alternative(&self) -> Option<&dyn SessionAlternative> {
        Some(&self.fader)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn SessionAlternative> {
        Some(&mut self.fader)
    }

    fn as_virtual_fader_view(&self) -> Option<&dyn VirtualFaderView> {
        Some(self)
    }

    fn note_editor(&self) -> Option<&NoteEditor> {
        Some(&self.note_editor)
    }

    fn note_editor_mut(&mut self) -> Option<&mut NoteEditor> {
        Some(&mut self.note_editor)
    }

    crate::impl_as_any!();
}
