// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The groove mode (former `GrooveMode`) and the record quantization mode (former
//! `QuantizeMode`, registered as REC_ARM).
//!
//! Differences to Java:
//! - The groove parameters are not bound to the knobs (they are no `ParamRef`s); the mode
//!   changes them in `on_knob_value`, which has the same effect.
//! - The groove has no observers to enable; only the indication is switched.

use super::{add_parameter, forward_base, has_host_record_quantization, row_index, set_cell_value, set_temporary_mode_later, set_active_mode_later};
use crate::controller::push::display1::SELECT_ARROW;
use crate::controller::push::mode::PushDisplayMode;
use crate::daw::constants::{GrooveParameterId, RecordQuantization};
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::{Mode, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

const TAG_GROOVE: &str = "Groove";

/// The groove parameters of the knobs (former `params`).
fn knob_parameter(index: usize) -> Option<GrooveParameterId> {
    match index {
        2 => Some(GrooveParameterId::ShuffleAmount),
        3 => Some(GrooveParameterId::ShuffleRate),
        5 => Some(GrooveParameterId::AccentAmount),
        6 => Some(GrooveParameterId::AccentPhase),
        7 => Some(GrooveParameterId::AccentRate),
        _ => None,
    }
}

/// Edit the groove (shuffle and accent) parameters.
pub struct GrooveMode {
    base: ParameterModeBase,
}

impl GrooveMode {
    pub fn new() -> Self {
        GrooveMode { base: super::base(TAG_GROOVE) }
    }

    fn display_parameter1(ctx: &Ctx, display: &mut TextDisplay, id: GrooveParameterId, index: usize) {
        let p = ctx.model.groove().parameter(id);
        if !p.does_exist() {
            return;
        }
        let name = p.name_limited(8);
        let value_str = p.displayed_value_limited(8);
        let value = p.value();
        display.set_cell(0, index, &name);
        display.set_cell(1, index, &value_str);
        set_cell_value(ctx, display, 2, index, value, Format::Value);
    }

    fn display_parameter2(&self, ctx: &Ctx, display: &mut GraphicDisplay, id: GrooveParameterId, index: usize) {
        let p = ctx.model.groove().parameter(id);
        add_parameter(display, " ", false, "", &p.name_limited(10), p.value(), &p.displayed_value_limited(8), self.base.is_knob_touched(index), -1);
    }

    fn has_accent(ctx: &Ctx) -> bool {
        ctx.model.groove().parameter(GrooveParameterId::AccentAmount).does_exist()
    }
}

impl Default for GrooveMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for GrooveMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_cell(0, 0, "Quantize");
        display.set_cell(0, 1, &format!("{SELECT_ARROW}{TAG_GROOVE}"));
        let enabled = ctx.model.groove().parameter(GrooveParameterId::Enabled).value();
        display.set_cell(3, 0, if enabled == 0 { "  Off" } else { "Enabled" });
        display.set_cell(2, 1, "Shuffle:");
        Self::display_parameter1(ctx, display, GrooveParameterId::ShuffleAmount, 2);
        Self::display_parameter1(ctx, display, GrooveParameterId::ShuffleRate, 3);
        display.set_cell(2, 4, if Self::has_accent(ctx) { " Accent:" } else { "" });
        Self::display_parameter1(ctx, display, GrooveParameterId::AccentAmount, 5);
        Self::display_parameter1(ctx, display, GrooveParameterId::AccentPhase, 6);
        Self::display_parameter1(ctx, display, GrooveParameterId::AccentRate, 7);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let enabled = ctx.model.groove().parameter(GrooveParameterId::Enabled).value();
        let param_text = if enabled == 0 { "Off" } else { "Enabled" };
        display.add_option_element_colored("", "Quantize", false, None, "", param_text, enabled > 0, None, true);
        display.add_option_element_colored("", TAG_GROOVE, true, None, "      Shuffle", "", false, None, true);
        self.display_parameter2(ctx, display, GrooveParameterId::ShuffleAmount, 2);
        self.display_parameter2(ctx, display, GrooveParameterId::ShuffleRate, 3);
        let has_accent = Self::has_accent(ctx);
        display.add_option_element_colored("", if has_accent { "" } else { " " }, false, None, if has_accent { "      Accent" } else { "" }, "", false, None, true);
        self.display_parameter2(ctx, display, GrooveParameterId::AccentAmount, 5);
        self.display_parameter2(ctx, display, GrooveParameterId::AccentPhase, 6);
        self.display_parameter2(ctx, display, GrooveParameterId::AccentRate, 7);
    }
}

impl Mode for GrooveMode {
    forward_base!(no_activate);

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
        ctx.model.groove_mut().set_indication(true);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
        ctx.model.groove_mut().set_indication(false);
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if let Some(id) = knob_parameter(index) {
            ctx.model.groove_mut().parameter_mut(id).change_value(value);
        }
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        knob_parameter(index).map_or(-1, |id| ctx.model.groove().parameter(id).value())
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        // Java does not store the touch state (the parameters are shown as untouched)
        let Some(id) = knob_parameter(index) else {
            return;
        };
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            ctx.model.groove_mut().parameter_mut(id).reset_value();
        }
        ctx.model.groove_mut().parameter_mut(id).touch_value(is_touched);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up || index != 0 {
            return;
        }
        if row == 0 {
            let parameter = ctx.model.groove_mut().parameter_mut(GrooveParameterId::Enabled);
            let value = if parameter.value() > 0 { 0.0 } else { 1.0 };
            parameter.set_normalized_value(value);
        } else {
            set_active_mode_later(ctx, Modes::RecArm);
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(1, button) {
            match index {
                0 => return BUTTON_COLOR_ON,
                1 => return BUTTON_COLOR_HI,
                _ => {}
            }
        } else if row_index(0, button) == Some(0) {
            return if ctx.model.groove().parameter(GrooveParameterId::Enabled).value() > 0 { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
        }
        BUTTON_COLOR_OFF
    }

    impl_as_any!();
}

const MENU: [&str; 8] = ["Quantize", "Groove", " ", " ", " ", " ", " ", " "];

/// The record quantization of the cursor track (and the quantize amount).
pub struct QuantizeMode {
    base: ParameterModeBase,
}

impl QuantizeMode {
    pub fn new() -> Self {
        QuantizeMode { base: super::base("Record") }
    }

    fn record_quantization(ctx: &Ctx) -> RecordQuantization {
        let cursor_track = ctx.model.cursor_track();
        if cursor_track.does_exist() {
            cursor_track.record_quantization_grid()
        } else {
            RecordQuantization::ResOff
        }
    }

    fn is_record_quantization_note_length(ctx: &Ctx) -> bool {
        let cursor_track = ctx.model.cursor_track();
        cursor_track.does_exist() && cursor_track.is_record_quantization_note_length()
    }
}

impl Default for QuantizeMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for QuantizeMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_cell(0, 0, &format!("{SELECT_ARROW}Quantize"));
        display.set_cell(0, 1, "Groove");
        let rec_quant = Self::record_quantization(ctx);
        display.set_block(2, 0, "Record Quantize:");
        for (i, value) in RecordQuantization::ALL.iter().enumerate() {
            let arrow = if *value == rec_quant { SELECT_ARROW.to_string() } else { String::new() };
            display.set_cell(3, i, &format!("{arrow}{}", value.name()));
        }
        if has_host_record_quantization(ctx) {
            display.set_block(2, 2, "       Quant Note");
            display.set_cell(2, 6, "Length:");
            display.set_cell(3, 6, if Self::is_record_quantization_note_length(ctx) { "On" } else { "Off" });
        }
        let quantize_amount = ctx.config.quantize_amount();
        display.set_cell(0, 7, "Quant Amnt").set_cell(1, 7, &format!("{quantize_amount}%"));
        set_cell_value(ctx, display, 2, 7, quantize_amount * 1023 / 100, Format::Value);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let rec_quant = Self::record_quantization(ctx);
        for (i, value) in RecordQuantization::ALL.iter().enumerate() {
            display.add_option_element("", MENU[i], i == 0, if i == 0 { "Record Quantization" } else { "" }, value.name(), *value == rec_quant, true);
        }
        let has_host_quantization = has_host_record_quantization(ctx);
        if has_host_quantization {
            display.add_option_element_colored("", " ", false, None, "Quantize Note Length", "", false, None, true);
            let is_quant_length = Self::is_record_quantization_note_length(ctx);
            display.add_option_element("", " ", false, "", if is_quant_length { "On" } else { "Off" }, is_quant_length, true);
        } else {
            display.add_empty_element_with_small_menu(true);
            display.add_empty_element_with_small_menu(true);
        }
        if has_host_quantization {
            let quantize_amount = ctx.config.quantize_amount();
            // Java bug kept: shows the touch state of knob 1 (and the touch is never stored)
            add_parameter(display, " ", false, "", "Qunt Amnt", quantize_amount * 1023 / 100, &format!("{quantize_amount}%"), self.base.is_knob_touched(0), -1);
        } else {
            display.add_empty_element_with_small_menu(true);
        }
    }
}

impl Mode for QuantizeMode {
    forward_base!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if index == 7 {
            let value_changer = ctx.model.value_changer().clone();
            ctx.config.change_quantize_amount(&value_changer, value);
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            if index == 7 {
                ctx.config.reset_quantize_amount();
            }
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if row == 0 {
            if !ctx.model.cursor_track().does_exist() {
                return;
            }
            match index {
                0..=4 => ctx.model.cursor_track_mut().set_record_quantization_grid(RecordQuantization::ALL[index]),
                6 => ctx.model.cursor_track_mut().toggle_record_quantization_note_length(),
                _ => {}
            }
        } else if index == 1 {
            set_temporary_mode_later(ctx, Modes::Groove);
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            let rec_quant = Self::record_quantization(ctx);
            if let Some(value) = RecordQuantization::ALL.get(index) {
                return if *value == rec_quant { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
            }
            if index == 6 {
                return if ctx.model.cursor_track().is_record_quantization_note_length() { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
            }
            return BUTTON_COLOR_OFF;
        }
        if let Some(index) = row_index(1, button) {
            return match index {
                0 => BUTTON_COLOR_HI,
                1 => BUTTON_COLOR_ON,
                _ => BUTTON_COLOR_OFF,
            };
        }
        BUTTON_COLOR_OFF
    }

    impl_as_any!();
}
