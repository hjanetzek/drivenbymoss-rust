// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The note repeat mode (former `NoteRepeatMode`, registered as REPEAT_NOTE).
//!
//! Differences to Java: the note repeat of the model supports the period, the note length, the
//! mode and "use pressure" (the capabilities of the Reaper version). Latch, octaves, free
//! running and shuffle are missing in the model (host feature gap); their UI is kept and hidden
//! as in Java when the host does not support them. The mode is the one of the configuration.

use super::{add_plain_parameter, forward_base, row_index, set_cell_value};
use crate::config::values::ArpeggiatorMode;
use crate::controller::push::display1::{create_menu_list, SELECT_ARROW};
use crate::controller::push::mode::{BaseModeState, PushDisplayMode};
use crate::daw::constants::{GrooveParameterId, Resolution};
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::{Mode, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The note repeat capabilities of the host (former `Capability.NOTE_REPEAT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capability {
    Length,
    Mode,
    UsePressureToVelocity,
    Latch,
    Octaves,
    IsFreeRunning,
    Swing,
}

/// Former `host.supports (capability)`: the capabilities of the note repeat of the model.
fn supports(capability: Capability) -> bool {
    match capability {
        Capability::Length | Capability::Mode | Capability::UsePressureToVelocity => true,
        // Host feature gap: not in the model
        Capability::Latch | Capability::Octaves | Capability::IsFreeRunning | Capability::Swing => false,
    }
}

fn resolution_names() -> Vec<String> {
    Resolution::names().iter().map(|n| n.to_string()).collect()
}

/// Edit the note repeat settings.
pub struct NoteRepeatMode {
    base: ParameterModeBase,
    state: BaseModeState,
}

impl NoteRepeatMode {
    pub fn new() -> Self {
        NoteRepeatMode { base: super::base("Note Repeat"), state: BaseModeState::default() }
    }

    fn selected_period_index(ctx: &Ctx) -> i32 {
        Resolution::get_match(ctx.surface.input.default_note_input().note_repeat.period) as i32
    }

    fn selected_note_length_index(ctx: &Ctx) -> i32 {
        Resolution::get_match(ctx.surface.input.default_note_input().note_repeat.note_length) as i32
    }

    fn use_pressure(ctx: &Ctx) -> bool {
        ctx.surface.input.default_note_input().note_repeat.use_pressure
    }

    /// The name of the mode and its value in the range of the value changer.
    fn mode_value(ctx: &Ctx) -> (String, i32) {
        let mode = ctx.config.note_repeat_mode();
        let num_modes = ctx.config.arpeggiator_modes().len().max(2) - 1;
        let mode_index = ctx.config.lookup_arpeggiator_mode_index(mode);
        let value = mode_index as i32 * ctx.model.value_changer().upper_bound() / num_modes as i32;
        (optimize_name(mode.map_or("", |m| m.name()), 8), value)
    }
}

impl Default for NoteRepeatMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for NoteRepeatMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_cell(0, 0, "Period:");
        let names = resolution_names();
        for (pos, (name, selected)) in create_menu_list(4, &names, Self::selected_period_index(ctx)).into_iter().enumerate() {
            display.set_cell(pos, 1, &format!("{}{name}", if selected { SELECT_ARROW } else { ' ' }));
        }
        if supports(Capability::Length) {
            display.set_cell(0, 2, "Length:");
            for (pos, (name, selected)) in create_menu_list(4, &names, Self::selected_note_length_index(ctx)).into_iter().enumerate() {
                display.set_cell(pos, 3, &format!("{}{name}", if selected { SELECT_ARROW } else { ' ' }));
            }
        }
        if supports(Capability::Latch) {
            display.set_cell(3, 4, " Latch");
        }
        let upper_bound = ctx.model.value_changer().upper_bound();
        if supports(Capability::Mode) {
            let bottom_menu = if supports(Capability::UsePressureToVelocity) { "Use Pressure" } else { "" };
            let (name, value) = Self::mode_value(ctx);
            display.set_cell(0, 5, "Mode");
            display.set_cell(1, 5, &name);
            set_cell_value(ctx, display, 2, 5, value, Format::Value);
            display.set_cell(3, 5, bottom_menu);
        }
        if supports(Capability::Octaves) {
            let bottom_menu = if supports(Capability::IsFreeRunning) { "  Sync" } else { "" };
            let octaves = ctx.config.note_repeat_octave();
            display.set_cell(0, 6, "Octaves");
            display.set_cell(1, 6, &octaves.to_string());
            set_cell_value(ctx, display, 2, 6, octaves * upper_bound / 8, Format::Value);
            display.set_cell(3, 6, bottom_menu);
        }
        if supports(Capability::Swing) {
            let shuffle = ctx.model.groove().parameter(GrooveParameterId::ShuffleAmount);
            let (name, value_str, value) = (shuffle.name_limited(10), shuffle.displayed_value_limited(8), shuffle.value());
            display.set_cell(0, 7, &name);
            display.set_cell(1, 7, &value_str);
            set_cell_value(ctx, display, 2, 7, value, Format::Value);
            display.set_cell(3, 7, "Shuffle");
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let names = resolution_names();
        display.add_option_element("Period", "", false, "", "", false, false);
        display.add_list_element_sized(6, &names, Self::selected_period_index(ctx));
        if supports(Capability::Length) {
            display.add_option_element("  Length", "", false, "", "", false, false);
            display.add_list_element_sized(6, &names, Self::selected_note_length_index(ctx));
        } else {
            display.add_empty_element();
            display.add_empty_element();
        }
        if supports(Capability::Latch) {
            display.add_option_element("", "", false, "", "Latch", false, false);
        } else {
            display.add_empty_element();
        }
        let upper_bound = ctx.model.value_changer().upper_bound();
        if supports(Capability::Mode) {
            let bottom_menu = if supports(Capability::UsePressureToVelocity) { "Use Pressure" } else { "" };
            let (name, value) = Self::mode_value(ctx);
            add_plain_parameter(display, "", false, bottom_menu, None, Self::use_pressure(ctx), "Mode", value, &name, self.base.is_knob_touched(5), -1);
        } else {
            display.add_empty_element();
        }
        if supports(Capability::Octaves) {
            let bottom_menu = if supports(Capability::IsFreeRunning) { "Sync" } else { "" };
            let octaves = ctx.config.note_repeat_octave();
            add_plain_parameter(display, "", false, bottom_menu, None, true, "Octaves", octaves * upper_bound / 8, &octaves.to_string(), self.base.is_knob_touched(6), -1);
        } else {
            display.add_empty_element();
        }
        if supports(Capability::Swing) {
            let groove = ctx.model.groove();
            let shuffle = groove.parameter(GrooveParameterId::ShuffleAmount);
            let enabled = groove.parameter(GrooveParameterId::Enabled);
            let top = format!("Groove {}", enabled.displayed_value_limited(8));
            add_plain_parameter(display, &top, enabled.value() != 0, "Shuffle", None, false, &shuffle.name_limited(10), shuffle.value(), &shuffle.displayed_value_limited(8), self.base.is_knob_touched(7), -1);
        } else {
            display.add_empty_element();
        }
    }
}

impl Mode for NoteRepeatMode {
    forward_base!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if index != 7 && !self.state.increase_knob_movement() {
            return;
        }
        let value_changer = ctx.model.value_changer().clone();
        match index {
            0 | 1 => {
                let sel = Resolution::change(Resolution::get_match(ctx.config.note_repeat_period().value()), value_changer.is_increase(value));
                ctx.config.set_note_repeat_period(Resolution::ALL[sel]);
            }
            2 | 3 => {
                if supports(Capability::Length) {
                    let sel = Resolution::change(Resolution::get_match(ctx.config.note_repeat_length().value()), value_changer.calc_knob_change(value) > 0.0);
                    ctx.config.set_note_repeat_length(Resolution::ALL[sel]);
                }
            }
            5 => {
                if supports(Capability::Mode) {
                    ctx.config.set_prev_next_note_repeat_mode(value_changer.is_increase(value));
                }
            }
            6 => {
                if supports(Capability::Octaves) {
                    let octave = ctx.config.note_repeat_octave() + if value_changer.calc_knob_change(value) > 0.0 { 1 } else { -1 };
                    ctx.config.set_note_repeat_octave(octave);
                }
            }
            7 if supports(Capability::Swing) => ctx.model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount).change_value(value),
            _ => {}
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        if !is_touched || !ctx.surface.is_delete_pressed() {
            return;
        }
        ctx.surface.set_trigger_consumed(ButtonId::Delete);
        match index {
            5 => {
                if supports(Capability::Mode) {
                    ctx.config.set_note_repeat_mode(ArpeggiatorMode::Up);
                }
            }
            6 => {
                if supports(Capability::Octaves) {
                    ctx.config.set_note_repeat_octave(1);
                }
            }
            7 if supports(Capability::Swing) => ctx.model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount).reset_value(),
            _ => {}
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if row != 0 {
            if index == 7 && supports(Capability::Swing) {
                let upper_bound = ctx.model.value_changer().upper_bound();
                let enabled = ctx.model.groove_mut().parameter_mut(GrooveParameterId::Enabled);
                let value = if enabled.value() == 0 { upper_bound } else { 0 };
                enabled.set_value(value);
            }
            return;
        }
        match index {
            0 | 1 => {
                let sel = Resolution::change(Self::selected_period_index(ctx) as usize, index == 1);
                ctx.config.set_note_repeat_period(Resolution::ALL[sel]);
            }
            2 | 3 => {
                if supports(Capability::Length) {
                    let sel = Resolution::change(Self::selected_note_length_index(ctx) as usize, index == 3);
                    ctx.config.set_note_repeat_length(Resolution::ALL[sel]);
                }
            }
            5 if supports(Capability::UsePressureToVelocity) => ctx.surface.input.default_note_input_mut().note_repeat.toggle_use_pressure(),
            // Latch (4), free running (6) and shuffle (7) are not supported by the model
            _ => {}
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let off_color = ctx.colors.color_index(BUTTON_COLOR_OFF);
        let on_color = ctx.colors.color_index(BUTTON_COLOR_ON);
        let hi_color = ctx.colors.color_index(BUTTON_COLOR_HI);
        if let Some(index) = row_index(0, button) {
            let supported = match index {
                2 | 3 => supports(Capability::Length),
                4 => supports(Capability::Latch),
                5 => supports(Capability::UsePressureToVelocity),
                6 => supports(Capability::IsFreeRunning),
                7 => supports(Capability::Swing),
                _ => true,
            };
            if !supported {
                return off_color;
            }
            return if index == 5 && Self::use_pressure(ctx) { hi_color } else { on_color };
        }
        if let Some(index) = row_index(1, button) {
            if index < 7 {
                return off_color;
            }
            let is_enabled = ctx.model.groove().parameter(GrooveParameterId::Enabled).value() > 0;
            return if is_enabled { hi_color } else { on_color };
        }
        off_color
    }

    impl_as_any!();
}
