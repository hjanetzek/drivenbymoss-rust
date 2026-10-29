// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Maschine Mk2/Mk3/Mikro/Plus/Studio (former package
//! `controller.ni.maschine.mk3.mode`). The modes write to the text display (MCU protocol) of
//! the Maschine, if the model has one.
//!
//! The knob index `usize::MAX` stands for the Java index -1 (the main encoder, which edits the
//! selected parameter).

pub mod browse;
pub mod drum_configuration;
pub mod edit_note;
pub mod note_repeat;
pub mod parameters_mode;
pub mod play_configuration;
pub mod track_modes;
pub mod transport_modes;
pub mod user_mode;

use crate::config::values::fix_ascii;
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;

/// The knob index of the main encoder (Java -1).
pub const MAIN_ENCODER: usize = usize::MAX;

/// The number of knobs of which the touch state is stored (8 mode knobs and the main encoder).
pub const NUM_TOUCH_STATES: usize = 9;

/// Convert the knob index of the mode functions to the Java index (-1 for the main encoder).
pub fn knob_index(index: usize) -> i32 {
    if index == MAIN_ENCODER {
        -1
    } else {
        index as i32
    }
}

/// Execute the function with the text display (if the Maschine has one).
pub fn with_display(ctx: &mut Ctx, f: impl FnOnce(&mut TextDisplay)) {
    if let Some(display) = &mut ctx.text_display {
        f(display);
    }
}

/// The state of all modes based on the former `BaseMode`: the selected parameter.
#[derive(Debug, Clone, Copy, Default)]
pub struct BaseMode {
    pub selected_param: i32,
}

impl BaseMode {
    pub fn new(selected_param: i32) -> Self {
        BaseMode { selected_param }
    }

    /// Mark the label with a '>' if the parameter is selected.
    pub fn mark(&self, label: &str, index: i32) -> String {
        if self.selected_param == index {
            format!(">{label}")
        } else {
            label.to_string()
        }
    }

    pub fn select_previous_item(&mut self) {
        self.selected_param = (self.selected_param - 1).max(0);
    }

    pub fn select_next_item(&mut self) {
        self.selected_param = (self.selected_param + 1).min(7);
    }

    pub fn select_item(&mut self, index: usize) {
        self.selected_param = index as i32;
    }

    /// The index of the knob: the selected parameter for the main encoder.
    pub fn index(&self, index: usize) -> i32 {
        if index == MAIN_ENCODER {
            self.selected_param
        } else {
            index as i32
        }
    }
}

/// The touch states of the 8 knobs and the main encoder (former `initTouchedStates (9)`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TouchStates {
    touched: [bool; NUM_TOUCH_STATES],
}

impl TouchStates {
    pub fn set(&mut self, index: usize, is_touched: bool) {
        if index < NUM_TOUCH_STATES {
            self.touched[index] = is_touched;
        }
    }

    pub fn is_touched(&self, index: usize) -> bool {
        index < NUM_TOUCH_STATES && self.touched[index]
    }

    pub fn is_any_touched(&self) -> bool {
        self.touched.iter().any(|t| *t)
    }
}

/// First replaces umlauts with alternative writing, then shortens a text to the given length
/// (former `StringUtils.shortenAndFixASCII`).
pub fn shorten_and_fix_ascii(text: &str, length: usize) -> String {
    optimize_name(&fix_ascii(text), length)
}

/// Format a value (0..1) as a percentage with 1 fraction digit (former
/// `StringUtils.formatPercentage`).
pub fn format_percentage(value: f64) -> String {
    let percent = (value * 1000.0).round() / 10.0;
    if percent.fract() == 0.0 {
        format!("{}%", percent as i64)
    } else {
        format!("{percent:.1}%")
    }
}

/// Format the given time as measure.quarters.eights(:ticks) (former
/// `StringUtils.formatMeasures`).
pub fn format_measures(quarters_per_measure: i32, beats: f64, start_offset: i32, include_frames: bool) -> String {
    let quarters_per_measure = quarters_per_measure.max(1) as f64;
    let measure = (beats / quarters_per_measure).floor() as i32;
    let mut t = beats - measure as f64 * quarters_per_measure;
    let quarters = t.floor() as i32;
    t -= quarters as f64;
    let eights = (t / 0.25).floor() as i32;
    if !include_frames {
        return format!("{}.{}.{}", measure + start_offset, quarters + start_offset, eights + start_offset);
    }
    t -= eights as f64 * 0.25;
    let frames = (t / 0.25 * 100.0).floor() as i32;
    format!("{}.{}.{}:{frames:03}", measure + start_offset, quarters + start_offset, eights + start_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(format_percentage(0.5), "50%");
        assert_eq!(format_percentage(0.1234), "12.3%");
        assert_eq!(format_measures(4, 0.0, 0, true), "0.0.0:000");
        assert_eq!(format_measures(4, 5.5, 0, true), "1.1.2:000");
        // Same floating point result as in Java
        assert_eq!(format_measures(4, 0.3, 0, true), "0.0.1:019");
        let mut base = BaseMode::new(0);
        base.select_previous_item();
        assert_eq!(base.selected_param, 0);
        assert_eq!(base.mark("Scale", 0), ">Scale");
        assert_eq!(base.index(MAIN_ENCODER), 0);
    }
}
