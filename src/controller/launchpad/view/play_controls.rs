// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Additional play controls (sustain, pitchbend, modulation) in the lowest row of the play
//! views (former `PlayControls`).

use crate::controller::launchpad::colors::*;
use crate::framework::ctx::Ctx;
use crate::midi::{CMD_CC, CMD_PITCHBEND};

const MODULATION_INTENSITIES: [i32; 5] = [0, 32, 64, 92, 127];

/// The state of the play controls.
#[derive(Debug, Default)]
pub struct PlayControls {
    is_active: bool,
    is_sustain: bool,
    is_pitch_down: bool,
    is_pitch_up: bool,
    is_modulation: usize,
}

impl PlayControls {
    pub fn new() -> Self {
        Self::default()
    }

    /// The color of the button which toggles the controls.
    pub fn toggle_button_color(&self) -> i32 {
        if self.is_active {
            LAUNCHPAD_COLOR_GREEN_HI
        } else {
            LAUNCHPAD_COLOR_GREEN_LO
        }
    }

    pub fn toggle(&mut self) {
        self.is_active = !self.is_active;
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Draw the controls into the lowest row, if active.
    pub fn draw(&self, ctx: &mut Ctx) {
        if !self.is_active {
            return;
        }
        let start_note = ctx.model.scales().start_note();
        let grid = ctx.surface.grid();
        grid.light(start_note, if self.is_sustain { LAUNCHPAD_COLOR_YELLOW_HI } else { LAUNCHPAD_COLOR_YELLOW_LO });
        grid.light(start_note + 1, if self.is_pitch_down { LAUNCHPAD_COLOR_CYAN_HI } else { LAUNCHPAD_COLOR_CYAN_LO });
        grid.light(start_note + 2, if self.is_pitch_up { LAUNCHPAD_COLOR_CYAN_HI } else { LAUNCHPAD_COLOR_CYAN_LO });
        for i in 0..5 {
            grid.light(start_note + 3 + i as i32, if self.is_modulation == i { LAUNCHPAD_COLOR_MAGENTA_HI } else { LAUNCHPAD_COLOR_MAGENTA_LO });
        }
    }

    /// Handle a pad of the controls. Returns false if the pad is not part of the controls.
    pub fn handle_grid_notes(&mut self, ctx: &mut Ctx, key: i32, velocity: i32) -> bool {
        if !self.is_active {
            return false;
        }
        let pos = key - ctx.model.scales().start_note();
        if !(0..8).contains(&pos) {
            return false;
        }

        let is_down = velocity > 0;
        let input = &ctx.surface.input;
        match pos {
            // Sustain
            0 => {
                self.is_sustain = is_down;
                input.send_raw_midi_event(CMD_CC, 64, if self.is_sustain { 127 } else { 0 });
            }
            // Pitch
            1 => {
                self.is_pitch_down = is_down;
                input.send_raw_midi_event(CMD_PITCHBEND, 0, if self.is_pitch_down { (velocity / 2 - 63).abs() } else { 64 });
            }
            2 => {
                self.is_pitch_up = is_down;
                input.send_raw_midi_event(CMD_PITCHBEND, 0, if self.is_pitch_up { 64 + velocity / 2 } else { 64 });
            }
            // Modulation
            _ => {
                if is_down {
                    self.is_modulation = (pos - 3) as usize;
                    input.send_raw_midi_event(CMD_CC, 1, MODULATION_INTENSITIES[self.is_modulation]);
                }
            }
        }
        true
    }
}
