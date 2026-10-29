// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The pad grid (former `IPadGrid`, `PadGridImpl` and `LightInfo`).

use std::collections::BTreeMap;

use super::color::ColorManager;
use crate::midi::MidiOutput;

/// The color key used for pads which are off.
pub const GRID_OFF: &str = "GRID_COLOR_OFF";

/// The light state of a pad: a color, an optional blink color and the blink speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LightInfo {
    pub color: i32,
    /// -1 for off.
    pub blink_color: i32,
    pub fast: bool,
}

impl LightInfo {
    pub fn new(color: i32, blink_color: i32, fast: bool) -> Self {
        assert!((0..=127).contains(&color), "color must be in the range of 0..127.");
        assert!((-1..=127).contains(&blink_color), "blinkColor must be in the range of 0..127 or -1 for off.");
        LightInfo { color, blink_color, fast }
    }

    /// Encode all three values into one integer, used to detect changes.
    pub fn encoded(&self) -> i32 {
        let code_blink_color = if self.blink_color < 0 { 1 << 15 } else { self.blink_color << 8 };
        let code_fast = if self.fast { 1 << 16 } else { 0 };
        code_fast + code_blink_color + self.color
    }
}

/// How blinking pads are sent to the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlinkMode {
    /// Send the blink color as a note on MIDI channel 14 (fast) or 10 (slow), e.g. Push.
    Channels,
    /// Collect the blink states, the controller sends them on flush (e.g. as sysex).
    Collect,
    /// The pad states are collected and sent with [`PadGrid::flush_blinking`]; blinking is
    /// emulated by toggling between the color and the blink color every 600ms (former
    /// `BlinkingPadGrid`, e.g. Maschine).
    Emulated,
}

/// The speed of the emulated blinking in milliseconds (former `BlinkingPadGrid.BLINK_SPEED`).
pub const BLINK_SPEED: u128 = 600;

/// The state of the emulated blinking (former `BlinkingPadGrid`).
#[derive(Debug, Clone)]
pub struct BlinkingState {
    /// The pad states which were changed since the last flush (by controller note).
    pub pad_infos: BTreeMap<i32, LightInfo>,
    /// The pads which are currently blinking (by controller note).
    pub blinking_lights: BTreeMap<i32, LightInfo>,
    pub is_blink: bool,
    pub update_time: std::time::Instant,
}

impl Default for BlinkingState {
    fn default() -> Self {
        BlinkingState { pad_infos: BTreeMap::new(), blinking_lights: BTreeMap::new(), is_blink: false, update_time: std::time::Instant::now() }
    }
}

/// Translates a grid note into a (MIDI channel, MIDI note) pair of the controller.
pub type GridTranslation = fn(note: i32) -> (i32, i32);

fn default_translation(note: i32) -> (i32, i32) {
    (0, note)
}

const NUM_NOTES: usize = 128;

/// A grid of pads. The pads are addressed by notes starting at `start_note` (lower left pad).
pub struct PadGrid {
    pad_states: Vec<LightInfo>,
    rows: i32,
    columns: i32,
    start_note: i32,
    end_note: i32,
    off_color: i32,
    translation: GridTranslation,
    blink_mode: BlinkMode,
    /// The collected blink states (only used with [`BlinkMode::Collect`]).
    pub pending_blinks: BTreeMap<i32, LightInfo>,
    /// The state of the emulated blinking (only used with [`BlinkMode::Emulated`]).
    pub blinking: BlinkingState,
}

impl PadGrid {
    pub fn new(colors: &ColorManager, rows: i32, columns: i32, start_note: i32) -> Self {
        PadGrid {
            pad_states: vec![LightInfo::default(); NUM_NOTES],
            rows,
            columns,
            start_note,
            end_note: start_note + rows * columns - 1,
            off_color: if colors.has_color_index(GRID_OFF) { colors.color_index(GRID_OFF) } else { 0 },
            translation: default_translation,
            blink_mode: BlinkMode::Channels,
            pending_blinks: BTreeMap::new(),
            blinking: BlinkingState::default(),
        }
    }

    pub fn with_translation(mut self, translation: GridTranslation) -> Self {
        self.translation = translation;
        self
    }

    pub fn with_blink_mode(mut self, blink_mode: BlinkMode) -> Self {
        self.blink_mode = blink_mode;
        self
    }

    pub fn rows(&self) -> i32 {
        self.rows
    }

    pub fn cols(&self) -> i32 {
        self.columns
    }

    pub fn start_note(&self) -> i32 {
        self.start_note
    }

    pub fn end_note(&self) -> i32 {
        self.end_note
    }

    /// Light a pad addressed by its note.
    pub fn light(&mut self, note: i32, color: i32) {
        self.light_blink(note, color, -1, false);
    }

    pub fn light_blink(&mut self, note: i32, color: i32, blink_color: i32, fast: bool) {
        self.set_light(note, color, blink_color, fast);
    }

    /// Light a pad addressed by column (x) and row (y), y = 0 is the top row.
    pub fn light_ex(&mut self, x: i32, y: i32, color: i32) {
        self.light_ex_blink(x, y, color, -1, false);
    }

    pub fn light_ex_blink(&mut self, x: i32, y: i32, color: i32, blink_color: i32, fast: bool) {
        let off = (self.rows - 1) * self.columns + self.start_note;
        self.set_light(off + x - self.columns * y, color, blink_color, fast);
    }

    /// Light a pad addressed by column (x) and row (y) with color IDs.
    pub fn light_ex_id(&mut self, colors: &ColorManager, x: i32, y: i32, color_id: &str) {
        self.light_ex(x, y, colors.color_index(color_id));
    }

    pub fn light_id(&mut self, colors: &ColorManager, note: i32, color_id: &str) {
        self.light(note, colors.color_index(color_id));
    }

    pub fn light_id_blink(&mut self, colors: &ColorManager, note: i32, color_id: &str, blink_color_id: Option<&str>, fast: bool) {
        let blink = blink_color_id.map(|id| colors.color_index(id)).unwrap_or(-1);
        self.light_blink(note, colors.color_index(color_id), blink, fast);
    }

    fn set_light(&mut self, note: i32, color: i32, blink_color: i32, fast: bool) {
        if !(0..NUM_NOTES as i32).contains(&note) {
            return;
        }
        let blink = if blink_color >= 0 { blink_color } else { self.off_color };
        self.pad_states[note as usize] = LightInfo::new(color.clamp(0, 127), blink, fast);
    }

    pub fn light_info(&self, note: i32) -> LightInfo {
        self.pad_states.get(note as usize).copied().unwrap_or_default()
    }

    /// Clear all pad states which forces an update on the next flush.
    pub fn force_flush(&mut self) {
        for note in self.start_note..=self.end_note {
            self.pad_states[note as usize] = LightInfo::default();
        }
    }

    /// Turn off all pads.
    pub fn turn_off(&mut self) {
        for note in self.start_note..=self.end_note {
            self.light(note, 0);
        }
    }

    pub fn translate_to_controller(&self, note: i32) -> (i32, i32) {
        (self.translation)(note)
    }

    /// Translate a controller note back to the grid (identity in the default implementation).
    pub fn translate_to_grid(&self, note: i32) -> i32 {
        note
    }

    /// Send the state of the pad to the controller.
    pub fn send_state(&mut self, output: &MidiOutput, note: i32) {
        let state = self.light_info(note);
        let (channel, controller_note) = self.translate_to_controller(note);
        let channel = channel.max(0);
        if self.blink_mode == BlinkMode::Emulated {
            // Former BlinkingPadGrid.sendNoteState/sendBlinkState
            let info = self.blinking.pad_infos.entry(controller_note).or_insert(LightInfo { color: 0, blink_color: -1, fast: false });
            info.color = state.color.max(0);
            if state.blink_color > 0 && state.blink_color < 128 {
                info.blink_color = state.blink_color;
                info.fast = state.fast;
            }
            return;
        }
        output.send_note_ex(channel, controller_note, state.color.max(0));
        let blink_color = state.blink_color;
        if blink_color > 0 && blink_color < 128 {
            match self.blink_mode {
                BlinkMode::Channels => output.send_note_ex(if state.fast { 14 } else { 10 }, controller_note, blink_color),
                BlinkMode::Emulated => {}
                BlinkMode::Collect => {
                    let info = self.pending_blinks.entry(controller_note).or_default();
                    info.blink_color = blink_color;
                    info.fast = state.fast;
                }
            }
        }
    }
}

impl PadGrid {
    /// Send the changed pad states and toggle the colors of the blinking pads every 600ms
    /// (former `BlinkingPadGrid.flush`). Only used with [`BlinkMode::Emulated`].
    pub fn flush_blinking(&mut self, output: &MidiOutput) {
        let infos = std::mem::take(&mut self.blinking.pad_infos);
        for (note, info) in infos {
            Self::send_pad_update(output, note, info.color);
            if info.blink_color > 0 {
                self.blinking.blinking_lights.insert(note, info);
            } else {
                self.blinking.blinking_lights.remove(&note);
            }
        }

        // Toggle blink colors every 600ms
        if !self.check_blinking() {
            return;
        }
        for (note, info) in &self.blinking.blinking_lights {
            let color_index = if self.blinking.is_blink { info.blink_color } else { info.color };
            Self::send_pad_update(output, *note, color_index);
        }
    }

    fn check_blinking(&mut self) -> bool {
        let now = std::time::Instant::now();
        if now.duration_since(self.blinking.update_time).as_millis() > BLINK_SPEED {
            self.blinking.update_time = now;
            self.blinking.is_blink = !self.blinking.is_blink;
            return true;
        }
        false
    }

    fn send_pad_update(output: &MidiOutput, note: i32, color_index: i32) {
        output.send_note_ex(0, note, color_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_ex_addresses_rows_from_top() {
        let colors = ColorManager::new();
        let mut grid = PadGrid::new(&colors, 8, 8, 36);
        grid.light_ex(0, 0, 5);
        assert_eq!(grid.light_info(36 + 56).color, 5);
        grid.light_ex(7, 7, 6);
        assert_eq!(grid.light_info(43).color, 6);
    }

    #[test]
    fn emulated_blinking() {
        use crate::backend::{BackendCall, RecordingBackend, Sender};
        let backend = std::rc::Rc::new(RecordingBackend::default());
        let output = MidiOutput::new(Sender::new(backend.clone()), Some(1));
        let colors = ColorManager::new();
        let mut grid = PadGrid::new(&colors, 4, 4, 36).with_blink_mode(BlinkMode::Emulated);
        grid.light_blink(36, 5, 7, false);
        grid.light(37, 3);
        grid.send_state(&output, 36);
        grid.send_state(&output, 37);
        assert!(backend.take_calls().is_empty());
        grid.flush_blinking(&output);
        let calls = backend.take_calls();
        assert_eq!(calls.len(), 2);
        assert!(matches!(&calls[0], BackendCall::SendMidi(1, data) if data == &vec![0x90, 36, 5]));
        assert_eq!(grid.blinking.blinking_lights.len(), 1);
        grid.blinking.update_time -= std::time::Duration::from_millis(700);
        grid.flush_blinking(&output);
        let calls = backend.take_calls();
        assert!(matches!(&calls[0], BackendCall::SendMidi(1, data) if data == &vec![0x90, 36, 7]));
    }

    #[test]
    fn encoding() {
        assert_eq!(LightInfo::new(3, -1, false).encoded(), (1 << 15) + 3);
        assert_eq!(LightInfo::new(3, 2, true).encoded(), (1 << 16) + (2 << 8) + 3);
    }
}
