// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The data about a note in a sequencer step (former `IStepInfo`, `DefaultStepInfo`,
//! `StepInfoImpl` and `EmptyStepInfo`).

use super::note_occurrence::NoteOccurrenceType;
use super::step_state::StepState;

/// The data about a note in a sequencer step.
///
/// Use [`StepInfo::new`] (or `Default`) for the default step info (former `DefaultStepInfo`)
/// and [`StepInfo::EMPTY`] for the "empty" info (former `EmptyStepInfo`), which differs only in
/// a zero duration and the formatted repeat count (`"-"`). Copies are made with `Clone` (former
/// `createCopy`).
#[derive(Debug, Clone, PartialEq)]
pub struct StepInfo {
    state: StepState,
    is_selected: bool,
    is_muted: bool,
    duration: f64,
    velocity: f64,
    velocity_spread: f64,
    release_velocity: f64,
    pressure: f64,
    timbre: f64,
    pan: f64,
    transpose: f64,
    gain: f64,
    is_chance_enabled: bool,
    chance: f64,
    is_occurrence_enabled: bool,
    occurrence: NoteOccurrenceType,
    is_recurrence_enabled: bool,
    recurrence_length: i32,
    recurrence_mask: i32,
    is_repeat_enabled: bool,
    repeat_count: i32,
    repeat_curve: f64,
    repeat_velocity_curve: f64,
    repeat_velocity_end: f64,
    /// True for the empty step info (former `EmptyStepInfo`).
    empty: bool,
}

impl Default for StepInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl StepInfo {
    /// The empty step info (former `EmptyStepInfo.INSTANCE`): everything is off/zero.
    pub const EMPTY: StepInfo = StepInfo {
        state: StepState::Off,
        is_selected: false,
        is_muted: false,
        duration: 0.0,
        velocity: 0.0,
        velocity_spread: 0.0,
        release_velocity: 0.0,
        pressure: 0.0,
        timbre: 0.0,
        pan: 0.0,
        transpose: 0.0,
        gain: 0.0,
        is_chance_enabled: false,
        chance: 0.0,
        is_occurrence_enabled: false,
        occurrence: NoteOccurrenceType::Always,
        is_recurrence_enabled: false,
        recurrence_length: 0,
        recurrence_mask: 0,
        is_repeat_enabled: false,
        repeat_count: 0,
        repeat_curve: 0.0,
        repeat_velocity_curve: 0.0,
        repeat_velocity_end: 0.0,
        empty: true,
    };

    /// Create a default step info: state off, a duration of 1/16 (0.25 beats) and all other
    /// values zero/disabled.
    pub const fn new() -> Self {
        StepInfo {
            state: StepState::Off,
            is_selected: false,
            is_muted: false,
            duration: 0.25,
            velocity: 0.0,
            velocity_spread: 0.0,
            release_velocity: 0.0,
            pressure: 0.0,
            timbre: 0.0,
            pan: 0.0,
            transpose: 0.0,
            gain: 0.0,
            is_chance_enabled: false,
            chance: 0.0,
            is_occurrence_enabled: false,
            occurrence: NoteOccurrenceType::Always,
            is_recurrence_enabled: false,
            recurrence_length: 0,
            recurrence_mask: 0,
            is_repeat_enabled: false,
            repeat_count: 0,
            repeat_curve: 0.0,
            repeat_velocity_curve: 0.0,
            repeat_velocity_end: 0.0,
            empty: false,
        }
    }

    /// The state of the step (off, start or continuation of a note).
    pub fn state(&self) -> StepState {
        self.state
    }

    /// Is the note selected?
    pub fn is_selected(&self) -> bool {
        self.is_selected
    }

    /// Is the note muted?
    pub fn is_muted(&self) -> bool {
        self.is_muted
    }

    /// The duration (length) of the note in beats.
    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// The velocity of the note (0..1).
    pub fn velocity(&self) -> f64 {
        self.velocity
    }

    /// The velocity spread of the note.
    pub fn velocity_spread(&self) -> f64 {
        self.velocity_spread
    }

    /// The release velocity of the note.
    pub fn release_velocity(&self) -> f64 {
        self.release_velocity
    }

    /// The pressure of the note from 0 to +1.
    pub fn pressure(&self) -> f64 {
        self.pressure
    }

    /// The timbre of the note from -1 to +1.
    pub fn timbre(&self) -> f64 {
        self.timbre
    }

    /// The panning of the note, -1 for left, +1 for right.
    pub fn pan(&self) -> f64 {
        self.pan
    }

    /// The transposition of the note in semi-tones, from -24 to +24.
    pub fn transpose(&self) -> f64 {
        self.transpose
    }

    /// The gain of the note.
    pub fn gain(&self) -> f64 {
        self.gain
    }

    /// Is chance (a random value to play the note) enabled?
    pub fn is_chance_enabled(&self) -> bool {
        self.is_chance_enabled
    }

    /// The chance (random value) that the note is played, in the range of [0..1].
    pub fn chance(&self) -> f64 {
        self.chance
    }

    /// Is occurrence (condition if the note is played) enabled?
    pub fn is_occurrence_enabled(&self) -> bool {
        self.is_occurrence_enabled
    }

    /// The occurrence condition.
    pub fn occurrence(&self) -> NoteOccurrenceType {
        self.occurrence
    }

    /// Is recurrence enabled?
    pub fn is_recurrence_enabled(&self) -> bool {
        self.is_recurrence_enabled
    }

    /// The length of the recurrence, [1..8], 1=Off.
    pub fn recurrence_length(&self) -> i32 {
        self.recurrence_length
    }

    /// The mask for the recurrence: field of bits, cycle N -> bit N; max 8 cycles.
    pub fn recurrence_mask(&self) -> i32 {
        self.recurrence_mask
    }

    /// Are repeats enabled?
    pub fn is_repeat_enabled(&self) -> bool {
        self.is_repeat_enabled
    }

    /// The number of repeats in the range of [-127..127].
    pub fn repeat_count(&self) -> i32 {
        self.repeat_count
    }

    /// Get the number of repeats formatted as a text: "Off" for 0, "1/N" for negative values
    /// and the number of played notes for positive values ("-" for [`StepInfo::EMPTY`]).
    pub fn formatted_repeat_count(&self) -> String {
        if self.empty {
            return "-".to_string();
        }
        let count = self.repeat_count;
        if count == 0 {
            "Off".to_string()
        } else if count < 0 {
            format!("1/{}", (count - 1).abs())
        } else {
            (count + 1).to_string()
        }
    }

    /// The repeat curve.
    pub fn repeat_curve(&self) -> f64 {
        self.repeat_curve
    }

    /// The repeat velocity curve between the start and end velocity.
    pub fn repeat_velocity_curve(&self) -> f64 {
        self.repeat_velocity_curve
    }

    /// The repeat velocity end.
    pub fn repeat_velocity_end(&self) -> f64 {
        self.repeat_velocity_end
    }

    /// Set the value returned by [`Self::state`].
    pub fn set_state(&mut self, state: StepState) {
        self.state = state;
    }

    /// Set the value returned by [`Self::is_selected`].
    pub fn set_selected(&mut self, selected: bool) {
        self.is_selected = selected;
    }

    /// Set the value returned by [`Self::is_muted`].
    pub fn set_muted(&mut self, muted: bool) {
        self.is_muted = muted;
    }

    /// Set the value returned by [`Self::duration`].
    pub fn set_duration(&mut self, duration: f64) {
        self.duration = duration;
    }

    /// Set the value returned by [`Self::velocity`].
    pub fn set_velocity(&mut self, velocity: f64) {
        self.velocity = velocity;
    }

    /// Set the value returned by [`Self::velocity_spread`].
    pub fn set_velocity_spread(&mut self, velocity_spread: f64) {
        self.velocity_spread = velocity_spread;
    }

    /// Set the value returned by [`Self::release_velocity`].
    pub fn set_release_velocity(&mut self, release_velocity: f64) {
        self.release_velocity = release_velocity;
    }

    /// Set the value returned by [`Self::pressure`].
    pub fn set_pressure(&mut self, pressure: f64) {
        self.pressure = pressure;
    }

    /// Set the value returned by [`Self::timbre`].
    pub fn set_timbre(&mut self, timbre: f64) {
        self.timbre = timbre;
    }

    /// Set the value returned by [`Self::pan`].
    pub fn set_pan(&mut self, pan: f64) {
        self.pan = pan;
    }

    /// Set the value returned by [`Self::transpose`].
    pub fn set_transpose(&mut self, transpose: f64) {
        self.transpose = transpose;
    }

    /// Set the value returned by [`Self::gain`].
    pub fn set_gain(&mut self, gain: f64) {
        self.gain = gain;
    }

    /// Set the value returned by [`Self::is_chance_enabled`].
    pub fn set_is_chance_enabled(&mut self, chance_enabled: bool) {
        self.is_chance_enabled = chance_enabled;
    }

    /// Set the value returned by [`Self::chance`].
    pub fn set_chance(&mut self, chance: f64) {
        self.chance = chance;
    }

    /// Set the value returned by [`Self::is_occurrence_enabled`].
    pub fn set_is_occurrence_enabled(&mut self, occurrence_enabled: bool) {
        self.is_occurrence_enabled = occurrence_enabled;
    }

    /// Set the value returned by [`Self::occurrence`].
    pub fn set_occurrence(&mut self, occurrence: NoteOccurrenceType) {
        self.occurrence = occurrence;
    }

    /// Set the value returned by [`Self::is_recurrence_enabled`].
    pub fn set_is_recurrence_enabled(&mut self, recurrence_enabled: bool) {
        self.is_recurrence_enabled = recurrence_enabled;
    }

    /// Set the value returned by [`Self::recurrence_length`].
    pub fn set_recurrence_length(&mut self, recurrence_length: i32) {
        self.recurrence_length = recurrence_length;
    }

    /// Set the value returned by [`Self::recurrence_mask`].
    pub fn set_recurrence_mask(&mut self, recurrence_mask: i32) {
        self.recurrence_mask = recurrence_mask;
    }

    /// Set the value returned by [`Self::is_repeat_enabled`].
    pub fn set_is_repeat_enabled(&mut self, repeat_enabled: bool) {
        self.is_repeat_enabled = repeat_enabled;
    }

    /// Set the value returned by [`Self::repeat_count`].
    pub fn set_repeat_count(&mut self, repeat_count: i32) {
        self.repeat_count = repeat_count;
    }

    /// Set the value returned by [`Self::repeat_curve`].
    pub fn set_repeat_curve(&mut self, repeat_curve: f64) {
        self.repeat_curve = repeat_curve;
    }

    /// Set the value returned by [`Self::repeat_velocity_curve`].
    pub fn set_repeat_velocity_curve(&mut self, repeat_velocity_curve: f64) {
        self.repeat_velocity_curve = repeat_velocity_curve;
    }

    /// Set the value returned by [`Self::repeat_velocity_end`].
    pub fn set_repeat_velocity_end(&mut self, repeat_velocity_end: f64) {
        self.repeat_velocity_end = repeat_velocity_end;
    }

    /// Format all values for the backend protocol (host step grid, protocol v2): state (0 off,
    /// 1 start, 2 continue), selected, muted, duration, velocity, velocity spread, release
    /// velocity, pressure, timbre, pan, transpose, gain, chance enabled, chance, occurrence
    /// enabled, occurrence (index of [`NoteOccurrenceType::ALL`]), recurrence enabled,
    /// recurrence length, recurrence mask, repeat enabled, repeat count, repeat curve, repeat
    /// velocity curve, repeat velocity end; separated by spaces, booleans as 0/1.
    pub fn to_wire(&self) -> String {
        let flag = |b: bool| if b { "1" } else { "0" };
        let state = match self.state {
            StepState::Off => 0,
            StepState::Start => 1,
            StepState::Continue => 2,
        };
        let occurrence = NoteOccurrenceType::ALL.iter().position(|o| *o == self.occurrence).unwrap_or(0);
        format!(
            "{state} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {occurrence} {} {} {} {} {} {} {} {}",
            flag(self.is_selected),
            flag(self.is_muted),
            self.duration,
            self.velocity,
            self.velocity_spread,
            self.release_velocity,
            self.pressure,
            self.timbre,
            self.pan,
            self.transpose,
            self.gain,
            flag(self.is_chance_enabled),
            self.chance,
            flag(self.is_occurrence_enabled),
            flag(self.is_recurrence_enabled),
            self.recurrence_length,
            self.recurrence_mask,
            flag(self.is_repeat_enabled),
            self.repeat_count,
            self.repeat_curve,
            self.repeat_velocity_curve,
            self.repeat_velocity_end
        )
    }

    /// Parse the values formatted by [`Self::to_wire`]. A single "0" is an empty step (None).
    pub fn from_wire(text: &str) -> Option<StepInfo> {
        let values: Vec<f64> = text.split_whitespace().map(|v| v.parse::<f64>().unwrap_or(0.0)).collect();
        Self::from_values(&values)
    }

    /// The values of [`Self::to_wire`] as numbers (the binary form of the protocol).
    pub fn to_values(&self) -> [f64; 24] {
        let flag = |b: bool| if b { 1.0 } else { 0.0 };
        let state = match self.state {
            StepState::Off => 0.0,
            StepState::Start => 1.0,
            StepState::Continue => 2.0,
        };
        let occurrence = NoteOccurrenceType::ALL.iter().position(|o| *o == self.occurrence).unwrap_or(0) as f64;
        [
            state,
            flag(self.is_selected),
            flag(self.is_muted),
            self.duration,
            self.velocity,
            self.velocity_spread,
            self.release_velocity,
            self.pressure,
            self.timbre,
            self.pan,
            self.transpose,
            self.gain,
            flag(self.is_chance_enabled),
            self.chance,
            flag(self.is_occurrence_enabled),
            occurrence,
            flag(self.is_recurrence_enabled),
            self.recurrence_length as f64,
            self.recurrence_mask as f64,
            flag(self.is_repeat_enabled),
            self.repeat_count as f64,
            self.repeat_curve,
            self.repeat_velocity_curve,
            self.repeat_velocity_end,
        ]
    }

    /// The step of the values of [`Self::to_values`] (at least 24); None for an empty step.
    pub fn from_values(values: &[f64]) -> Option<StepInfo> {
        if values.len() < 24 {
            return None;
        }
        let f = |i: usize| values[i];
        let n = |i: usize| f(i) as i32;
        let b = |i: usize| f(i) > 0.0;
        let state = match n(0) {
            1 => StepState::Start,
            2 => StepState::Continue,
            _ => return None,
        };
        let mut info = StepInfo::new();
        info.state = state;
        info.is_selected = b(1);
        info.is_muted = b(2);
        info.duration = f(3);
        info.velocity = f(4);
        info.velocity_spread = f(5);
        info.release_velocity = f(6);
        info.pressure = f(7);
        info.timbre = f(8);
        info.pan = f(9);
        info.transpose = f(10);
        info.gain = f(11);
        info.is_chance_enabled = b(12);
        info.chance = f(13);
        info.is_occurrence_enabled = b(14);
        info.occurrence = NoteOccurrenceType::ALL.get(n(15).max(0) as usize).copied().unwrap_or_default();
        info.is_recurrence_enabled = b(16);
        info.recurrence_length = n(17);
        info.recurrence_mask = n(18);
        info.is_repeat_enabled = b(19);
        info.repeat_count = n(20);
        info.repeat_curve = f(21);
        info.repeat_velocity_curve = f(22);
        info.repeat_velocity_end = f(23);
        Some(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let info = StepInfo::new();
        assert_eq!(info.state(), StepState::Off);
        assert_eq!(info.duration(), 0.25);
        assert_eq!(info.formatted_repeat_count(), "Off");
        assert_eq!(StepInfo::default(), info);
        assert_eq!(StepInfo::EMPTY.duration(), 0.0);
        assert_eq!(StepInfo::EMPTY.formatted_repeat_count(), "-");
        assert_eq!(StepInfo::EMPTY.clone(), StepInfo::EMPTY);
    }

    #[test]
    fn formatted_repeat_count() {
        let mut info = StepInfo::new();
        info.set_repeat_count(3);
        assert_eq!(info.formatted_repeat_count(), "4");
        info.set_repeat_count(-2);
        assert_eq!(info.formatted_repeat_count(), "1/3");
    }
}
