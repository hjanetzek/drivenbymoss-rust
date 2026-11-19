// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The ribbon configuration mode (former `RibbonMode`).

use super::{forward_base, restore_mode_later, row_index, touch_only};
use crate::config::push as config;
use crate::controller::push::display1::SELECT_ARROW;
use crate::controller::push::mode::PushDisplayMode;
use crate::controller::push::surface;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR2_ON, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

const MIDI_CCS: [i32; 4] = [1, 11, 7, 64];
const TOP_HEADERS: [&str; 8] = ["CC", "Quick Select", "", "", "", "Note Repeat", "", ""];
const BOTTOM_HEADERS: [&str; 8] = ["Function", "", "", "", "", "", "", ""];
const TOP_OPTIONS: [&str; 8] = ["", "Modulation", "Expression", "Volume", "Sustain", "Off", "Period", "Length"];
const BOTTOM_OPTIONS: [&str; 8] = ["Pitchbend", "CC", "CC/Pitch", "Pitch/CC", "Fader", "Last Touched", "", ""];
const FUNCTION_IDS: [usize; 6] = [config::RIBBON_MODE_PITCH, config::RIBBON_MODE_CC, config::RIBBON_MODE_CC_PB, config::RIBBON_MODE_PB_CC, config::RIBBON_MODE_FADER, config::RIBBON_MODE_LAST_TOUCHED];
const NOTE_REPEAT_NAMES: [&str; 3] = ["    Off", " Period", " Length"];
const FUNCTION_NAMES: [&str; 6] = ["Pitchbnd", "   CC   ", "CC/Pitch", "Pitch/CC", " Fader  ", " Touched"];

fn note_repeat_name_selected(index: usize) -> String {
    match index {
        0 => format!("    {SELECT_ARROW}Off"),
        1 => format!("{SELECT_ARROW}Period"),
        _ => format!("{SELECT_ARROW}Length"),
    }
}

fn function_name_selected(index: usize) -> String {
    match index {
        0 => format!("{SELECT_ARROW}Pitchbd"),
        1 => format!("  {SELECT_ARROW}CC   "),
        2 => format!("{SELECT_ARROW}CC/Ptch"),
        3 => format!("{SELECT_ARROW}Ptch/CC"),
        4 => format!("{SELECT_ARROW}Fader  "),
        _ => format!("{SELECT_ARROW}Touched"),
    }
}

/// Configures the function of the ribbon (touch strip).
pub struct RibbonMode {
    base: ParameterModeBase,
}

impl RibbonMode {
    pub fn new() -> Self {
        RibbonMode { base: super::base("Ribbon") }
    }
}

impl Default for RibbonMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for RibbonMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let ribbon_mode_cc = config::ribbon_mode_cc_val(&ctx.config);
        let ribbon_mode = config::ribbon_mode(&ctx.config);
        let note_repeat = config::ribbon_note_repeat(&ctx.config);
        display.set_cell(0, 0, &format!("CC: {ribbon_mode_cc}")).set_cell(0, 1, "Modulatn").set_cell(0, 2, "Expressn").set_cell(0, 3, "Volume").set_cell(0, 4, "Sustain");
        display.set_cell(1, 1, "CC Quick").set_cell(1, 2, "Select").set_cell(1, 5, "    Note").set_cell(1, 6, "Repeat");
        display.set_cell(2, 0, "Function");
        for (i, id) in FUNCTION_IDS.iter().enumerate() {
            let name = if ribbon_mode == *id { function_name_selected(i) } else { FUNCTION_NAMES[i].to_string() };
            display.set_cell(3, i, &name);
        }
        for (i, name) in NOTE_REPEAT_NAMES.iter().enumerate() {
            let name = if note_repeat == i { note_repeat_name_selected(i) } else { name.to_string() };
            display.set_cell(0, 5 + i, &name);
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let ribbon_mode = config::ribbon_mode(&ctx.config);
        let ribbon_note_repeat = config::ribbon_note_repeat(&ctx.config);
        for i in 0..8 {
            let mut is_menu_top_selected = true;
            let mut menu_top_name = TOP_OPTIONS[i].to_string();
            if i == 0 {
                menu_top_name = config::ribbon_mode_cc_val(&ctx.config).to_string();
            } else {
                is_menu_top_selected = i > 4 && ribbon_note_repeat == i - 5;
            }
            let is_bottom_selected = i < FUNCTION_IDS.len() && ribbon_mode == FUNCTION_IDS[i];
            display.add_option_element(TOP_HEADERS[i], &menu_top_name, is_menu_top_selected, BOTTOM_HEADERS[i], BOTTOM_OPTIONS[i], is_bottom_selected, false);
        }
    }
}

impl Mode for RibbonMode {
    forward_base!();
    touch_only!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if index == 0 {
            let cc = ctx.model.value_changer().change_value_with(value, config::ribbon_mode_cc_val(&ctx.config), -100.0, 128);
            config::set_ribbon_mode_cc(&mut ctx.config, cc);
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if row == 0 {
            if index < FUNCTION_IDS.len() {
                config::set_ribbon_mode(&mut ctx.config, index);
            } else {
                restore_mode_later(ctx);
            }
            return;
        }
        if index == 0 {
            return;
        }
        if index < 5 {
            config::set_ribbon_mode_cc(&mut ctx.config, MIDI_CCS[index - 1]);
        } else {
            config::set_ribbon_note_repeat(&mut ctx.config, index - 5);
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            if index < FUNCTION_IDS.len() {
                return if config::ribbon_mode(&ctx.config) == config::RIBBON_MODE_PITCH + index { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
            }
            return BUTTON_COLOR_OFF;
        }
        if let Some(index) = row_index(1, button) {
            if index == 0 {
                return BUTTON_COLOR_OFF;
            }
            if index < 5 {
                return if surface::is_modern(ctx) { BUTTON_COLOR_ON } else { BUTTON_COLOR2_ON };
            }
            return if config::ribbon_note_repeat(&ctx.config) == index - 5 { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
        }
        BUTTON_COLOR_OFF
    }

    impl_as_any!();
}
