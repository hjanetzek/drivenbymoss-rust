// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cursor clip (former `IClip`, `INoteClip` and `CursorClipImpl`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::backend::Processor;
use crate::daw::color::ColorEx;
use crate::daw::constants::transport_constants::{INC_FRACTION_TIME, INC_FRACTION_TIME_SLOW};
use crate::daw::constants::Resolution;
use crate::daw::context::DawContext;

use super::note::Note;
use super::note_occurrence::NoteOccurrenceType;
use super::note_position::NotePosition;
use super::step_info::StepInfo;
use super::step_state::StepState;

const PATH_NOTE: &str = "note/";

/// The number of MIDI channels for which step data is monitored.
pub const NUM_CHANNELS: usize = 16;

/// The delay between re-sending the data of notes which are currently edited (see
/// [`CursorClip::start_edit`]).
const EDIT_UPDATE_DELAY: Duration = Duration::from_millis(100);

/// The empty step which is returned for out of range positions.
static EMPTY_STEP: StepInfo = StepInfo::new();

thread_local! {
    static REQUESTED_NOTE_CLIPS: std::cell::RefCell<Vec<(usize, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Request a note clip with the given number of steps and rows while a view is created (former
/// `model.getNoteClip` in the constructors of the Java views). The clips are created after the
/// setup of the controller and announced to the host in the model setup (`noteClips`), since
/// Bitwig can only create them during init.
pub fn request_note_clip(cols: usize, rows: usize) {
    if cols > 0 && rows > 0 {
        REQUESTED_NOTE_CLIPS.with(|clips| clips.borrow_mut().push((cols, rows)));
    }
}

/// Take the requested note clips (see [`request_note_clip`]).
pub fn take_requested_note_clips() -> Vec<(usize, usize)> {
    REQUESTED_NOTE_CLIPS.with(|clips| std::mem::take(&mut *clips.borrow_mut()))
}

/// The cursor clip: a proxy to the MIDI clip which is currently selected in Reaper.
///
/// The clip monitors a window (page) of `num_steps` steps of `step_length` beats each and
/// `num_rows` rows (MIDI notes) for all 16 MIDI channels. Step positions in all methods which
/// take a [`NotePosition`] are relative to the current edit page.
///
/// The model receives the clip data from Reaper and forwards it via the `set_*` / `*_intern`
/// methods ([`set_notes`](Self::set_notes), [`set_exists_value`](Self::set_exists_value),
/// [`set_play_start_intern`](Self::set_play_start_intern),
/// [`set_play_end_intern`](Self::set_play_end_intern),
/// [`set_play_position`](Self::set_play_position), [`set_color_value`](Self::set_color_value),
/// [`set_loop_enabled_state`](Self::set_loop_enabled_state)). Furthermore it must call
/// [`run_due_tasks`](Self::run_due_tasks) regularly (former `host.scheduleTask`).
pub struct CursorClip {
    ctx: DawContext,
    exists: bool,
    clip_start: f64,
    clip_end: f64,
    is_looped: bool,
    color: ColorEx,
    play_position: f64,
    num_steps: usize,
    num_rows: usize,
    step_length: f64,
    notes: Vec<Note>,
    /// Sparse map of `[channel][step][row]` (flat index) to the step info. Only steps with
    /// notes are stored: a dense array would need ~400MB for the raindrops view clip.
    data: HashMap<usize, StepInfo>,
    edit_page: i32,
    max_page: i32,
    edit_steps: Vec<NotePosition>,
    /// Scheduled delayed updates of edited steps (former `host.scheduleTask`).
    pending_updates: Vec<(Instant, NotePosition)>,
    /// The host has a step grid for this clip (Bitwig, protocol v2): the steps come from the
    /// host and all step functions are commands to it (former Bitwig `CursorClipImpl`).
    host_grid: bool,
    host_playing_step: i32,
    host_loop_start: f64,
    host_loop_length: f64,
    host_shuffle: bool,
    /// The accent as reported by the host (0..1, 0.5 = no accent).
    host_accent: f64,
    host_pinned: bool,
}

impl CursorClip {
    /// Create a cursor clip.
    ///
    /// * `num_steps` - The number of steps of the clip to monitor (= 1 page)
    /// * `num_rows` - The number of note rows of the clip to monitor
    pub fn new(ctx: DawContext, num_steps: usize, num_rows: usize) -> Self {
        CursorClip {
            ctx,
            exists: false,
            clip_start: -1.0,
            clip_end: -1.0,
            is_looped: false,
            color: ColorEx::GRAY,
            play_position: -1.0,
            num_steps,
            num_rows,
            // 16th
            step_length: 1.0 / 4.0,
            notes: Vec::new(),
            data: HashMap::new(),
            edit_page: 0,
            max_page: 1,
            edit_steps: Vec::new(),
            pending_updates: Vec::new(),
            host_grid: false,
            host_playing_step: -1,
            host_loop_start: 0.0,
            host_loop_length: 0.0,
            host_shuffle: false,
            host_accent: 0.5,
            host_pinned: false,
        }
    }

    // ------------------------------------------------------------------------------------------
    // The step grid of the host (protocol v2)

    /// The ID of the clip in the commands and addresses of the host grid: "<steps>x<rows>".
    pub fn grid_id(&self) -> String {
        format!("{}x{}", self.num_steps, self.num_rows)
    }

    /// True if the host has a step grid for this clip.
    pub fn is_host_grid(&self) -> bool {
        self.host_grid
    }

    /// Switch to the step grid of the host.
    pub fn set_host_grid(&mut self) {
        if !self.host_grid {
            self.host_grid = true;
            self.data.clear();
            self.max_page = 1;
        }
    }

    /// A step of the host grid has changed (None = empty). Changes of steps which are currently
    /// edited are ignored, as in the Java version.
    pub fn set_host_step(&mut self, channel: i32, step: i32, row: i32, info: Option<StepInfo>) {
        self.set_host_grid();
        if self.edit_steps.iter().any(|p| p.channel == channel && p.step == step && p.note == row) {
            return;
        }
        let Some(index) = self.index(channel, step, row) else {
            return;
        };
        match info {
            Some(info) => {
                self.data.insert(index, info);
            }
            None => {
                self.data.remove(&index);
            }
        }
    }

    pub fn set_host_playing_step(&mut self, step: i32) {
        self.set_host_grid();
        self.host_playing_step = step;
    }

    pub fn set_loop_start_value(&mut self, start: f64) {
        self.host_loop_start = start;
    }

    pub fn set_loop_length_value(&mut self, length: f64) {
        self.host_loop_length = length;
    }

    pub fn set_shuffle_value(&mut self, shuffle: bool) {
        self.host_shuffle = shuffle;
    }

    pub fn set_accent_value(&mut self, accent: f64) {
        self.host_accent = accent;
    }

    pub fn set_pinned_value(&mut self, pinned: bool) {
        self.host_pinned = pinned;
    }

    fn send_grid(&self, command: &str) {
        self.ctx.send(Processor::Clip, &format!("grid/{}/{command}", self.grid_id()));
    }

    fn send_grid_int(&self, command: &str, value: i32) {
        self.ctx.send_int(Processor::Clip, &format!("grid/{}/{command}", self.grid_id()), value);
    }

    fn send_grid_str(&self, command: &str, value: &str) {
        self.ctx.send_str(Processor::Clip, &format!("grid/{}/{command}", self.grid_id()), value);
    }

    fn send_grid_double(&self, command: &str, value: f64) {
        self.ctx.send_double(Processor::Clip, &format!("grid/{}/{command}", self.grid_id()), value);
    }

    fn step_path(position: &NotePosition) -> String {
        format!("step/{}/{}/{}", position.channel, position.step, position.note)
    }

    // ------------------------------------------------------------------------------------------
    // Observers / pinning

    /// Enable or disable the updates of the clip data from Reaper.
    pub fn enable_observers(&self, enable: bool) {
        self.ctx.sender().enable_updates(Processor::Clip, enable);
    }

    /// Is the clip pinned? Only with a host grid, otherwise always false.
    pub fn is_pinned(&self) -> bool {
        self.host_grid && self.host_pinned
    }

    /// Toggle the pinned state. Only with a host grid.
    pub fn toggle_pinned(&mut self) {
        let pinned = !self.is_pinned();
        self.set_pinned(pinned);
    }

    /// Set the pinned state. Only with a host grid.
    pub fn set_pinned(&mut self, is_pinned: bool) {
        if self.host_grid {
            self.ctx.send_bool(Processor::Clip, "pinned", is_pinned);
        }
    }

    // ------------------------------------------------------------------------------------------
    // Updates from the model (data received from Reaper)

    /// Set the exists state (true if a selected clip exists).
    pub fn set_exists_value(&mut self, exists: bool) {
        self.exists = exists;
    }

    /// Set the clip color value (red, green, blue in the range of 0..1).
    pub fn set_color_value(&mut self, color: [f64; 3]) {
        self.color = ColorEx::from_array(color);
    }

    /// Set the loop enabled state.
    pub fn set_loop_enabled_state(&mut self, is_loop_enabled: bool) {
        self.is_looped = is_loop_enabled;
    }

    /// Set the play position in beats or -1 if the play cursor is not playing a part of the
    /// clip.
    pub fn set_play_position(&mut self, play_position: f64) {
        self.play_position = play_position;
    }

    /// Set the play start (only stores the value, does not send it to Reaper).
    pub fn set_play_start_intern(&mut self, start: f64) {
        self.clip_start = start;
        self.update_note_data();
    }

    /// Set the play end (only stores the value, does not send it to Reaper).
    pub fn set_play_end_intern(&mut self, end: f64) {
        self.clip_end = end;
        self.update_note_data();
    }

    /// Update all notes of the clip.
    pub fn set_notes(&mut self, notes: Vec<Note>) {
        self.notes = notes;
        self.update_note_data();
    }

    /// Get all notes of the clip (as last received with [`Self::set_notes`]).
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Execute the scheduled delayed updates of the edited steps which are due (former
    /// `host.scheduleTask` of `delayedUpdate`). Must be called regularly by the model.
    pub fn run_due_tasks(&mut self, now: Instant) {
        if self.pending_updates.is_empty() {
            return;
        }
        let (due, rest): (Vec<_>, Vec<_>) = self
            .pending_updates
            .drain(..)
            .partition(|(time, _)| *time <= now);
        self.pending_updates = rest;
        for (_, position) in due {
            self.delayed_update(position, now);
        }
    }

    /// True if there are scheduled delayed updates.
    pub fn has_pending_tasks(&self) -> bool {
        !self.pending_updates.is_empty()
    }

    // ------------------------------------------------------------------------------------------
    // Clip properties (former IClip)

    /// Returns true if the clip exists.
    pub fn does_exist(&self) -> bool {
        self.exists
    }

    /// Set the name of the clip.
    pub fn set_name(&self, name: &str) {
        self.ctx.send_str(Processor::Clip, "name", name);
    }

    /// Get the color of the clip (gray until a color was received).
    pub fn color(&self) -> ColorEx {
        self.color
    }

    /// Set the color of the clip.
    pub fn set_color(&self, color: ColorEx) {
        let [r, g, b] = color.to_int_rgb255();
        self.ctx
            .send_str(Processor::Clip, "color", &format!("RGB({r},{g},{b})"));
    }

    /// Returns the start of the clip in beat time (-1 if unknown).
    pub fn play_start(&self) -> f64 {
        self.clip_start
    }

    /// Set the start of the clip in beat time.
    pub fn set_play_start(&mut self, start: f64) {
        self.clip_start = start;
        self.ctx
            .send_double(Processor::Clip, "start", self.clip_start);
        self.update_note_data();
    }

    /// Change the start of the clip by a full (or with `slow` a single) beat.
    pub fn change_play_start(&mut self, control: i32, slow: bool) {
        if self.clip_start == -1.0 {
            return;
        }
        let increase = self.ctx.value_changer().is_increase(control);
        let frac = if slow {
            INC_FRACTION_TIME_SLOW
        } else {
            INC_FRACTION_TIME
        };
        let start = if increase {
            self.clip_start + frac
        } else {
            (self.clip_start - frac).max(0.0)
        };
        self.set_play_start(start);
    }

    /// Returns the end of the clip in beat time (-1 if unknown).
    pub fn play_end(&self) -> f64 {
        self.clip_end
    }

    /// Set the end of the clip in beat time.
    pub fn set_play_end(&mut self, end: f64) {
        self.clip_end = end;
        self.ctx.send_double(Processor::Clip, "end", self.clip_end);
        self.update_note_data();
    }

    /// Change the end of the clip by a full (or with `slow` a single) beat.
    pub fn change_play_end(&mut self, control: i32, slow: bool) {
        if self.clip_end == -1.0 {
            return;
        }
        let increase = self.ctx.value_changer().is_increase(control);
        let frac = if slow {
            INC_FRACTION_TIME_SLOW
        } else {
            INC_FRACTION_TIME
        };
        let end = if increase {
            self.clip_end + frac
        } else {
            (self.clip_end - frac).max(0.0)
        };
        self.set_play_end(end);
    }

    /// Sets the start and the end of the clip relative to the current play start. Ensure that
    /// the start is before the end.
    pub fn set_play_range(&mut self, start: f64, end: f64) {
        if self.host_grid {
            // As the Java version for Bitwig: absolute values, start and end cannot be the same
            if self.play_start() < start {
                self.set_play_end(end);
                self.set_play_start(start);
            } else {
                self.set_play_start(start);
                self.set_play_end(end);
            }
            return;
        }
        let play_start = self.play_start();
        self.set_play_start(play_start + start);
        self.set_play_end(play_start + end);
    }

    /// Get the start of the loop. Without a host grid always 0, the loop is identical to the
    /// play range.
    pub fn loop_start(&self) -> f64 {
        if self.host_grid {
            self.host_loop_start
        } else {
            0.0
        }
    }

    /// Set the start of the loop (without a host grid relative to the play start).
    pub fn set_loop_start(&mut self, start: f64) {
        if self.host_grid {
            self.host_loop_start = start;
            self.ctx.send_double(Processor::Clip, "loopStart", start);
            return;
        }
        self.set_play_start(self.play_start() + start);
    }

    /// Change the start of the loop (without a host grid the same as
    /// [`Self::change_play_start`]).
    pub fn change_loop_start(&mut self, control: i32, slow: bool) {
        if self.host_grid {
            let start = self.host_loop_start + self.time_change(control, slow);
            self.set_loop_start(start);
            return;
        }
        self.change_play_start(control, slow);
    }

    /// Get the length of the loop (without a host grid = play end - play start).
    pub fn loop_length(&self) -> f64 {
        if self.host_grid {
            self.host_loop_length
        } else {
            self.play_end() - self.play_start()
        }
    }

    /// Set the length of the loop.
    pub fn set_loop_length(&mut self, length: f64) {
        if self.host_grid {
            self.host_loop_length = length;
            self.ctx.send_double(Processor::Clip, "loopLength", length);
            return;
        }
        self.set_play_end(self.play_start() + length);
    }

    /// Change the length of the loop (without a host grid the same as
    /// [`Self::change_play_end`]).
    pub fn change_loop_length(&mut self, control: i32, slow: bool) {
        if self.host_grid {
            let length = self.host_loop_length + self.time_change(control, slow);
            self.set_loop_length(length);
            return;
        }
        self.change_play_end(control, slow);
    }

    /// The change of a time value by a knob (a beat, with `slow` a quarter of it).
    fn time_change(&self, control: i32, slow: bool) -> f64 {
        let frac = if slow { INC_FRACTION_TIME_SLOW } else { INC_FRACTION_TIME };
        if self.ctx.value_changer().is_increase(control) {
            frac
        } else {
            -frac
        }
    }

    /// Is the loop enabled?
    pub fn is_loop_enabled(&self) -> bool {
        self.is_looped
    }

    /// Set if the loop is enabled.
    pub fn set_loop_enabled(&self, enable: bool) {
        self.ctx.send_bool(Processor::Clip, "loop", enable);
    }

    /// Is shuffle enabled? Only with a host grid, otherwise always false.
    pub fn is_shuffle_enabled(&self) -> bool {
        self.host_grid && self.host_shuffle
    }

    /// Set if shuffle is enabled. Only with a host grid.
    pub fn set_shuffle_enabled(&mut self, enable: bool) {
        if self.host_grid {
            self.ctx.send_bool(Processor::Clip, "shuffle", enable);
        }
    }

    /// Get the accent value as a formatted string, e.g. "100%".
    pub fn formatted_accent(&self) -> String {
        if self.host_grid {
            return format!("{}%", self.accent().round() as i64);
        }
        // Java: Math.round (accent * 10000) / 100 (long division)
        format!("{}%", (self.accent() * 10000.0).round() as i64 / 100)
    }

    /// Get the accent value: with a host grid -100..100, otherwise not supported, always 1.
    pub fn accent(&self) -> f64 {
        if self.host_grid {
            self.host_accent * 200.0 - 100.0
        } else {
            1.0
        }
    }

    /// Reset the accent value to its default. Only with a host grid.
    pub fn reset_accent(&mut self) {
        if self.host_grid {
            self.ctx.send_double(Processor::Clip, "accent", 0.5);
        }
    }

    /// Change the accent value. Only with a host grid.
    pub fn change_accent(&mut self, control: i32, slow: bool) {
        if !self.host_grid {
            return;
        }
        let offset = if slow { 1.0 } else { 10.0 };
        let value = if self.ctx.value_changer().is_increase(control) { self.accent() + offset } else { self.accent() - offset };
        self.ctx.send_double(Processor::Clip, "accent", ((value + 100.0) / 200.0).clamp(0.0, 1.0));
    }

    /// Get the number of monitored steps (= 1 page).
    pub fn num_steps(&self) -> usize {
        self.num_steps
    }

    /// Duplicate the clip.
    pub fn duplicate(&self) {
        self.ctx.send(Processor::Clip, "duplicate");
    }

    /// Duplicate the content of the clip (in the clip).
    pub fn duplicate_content(&self) {
        self.ctx.send(Processor::Clip, "duplicateContent");
    }

    /// Quantizes the start time of all notes in the clip according to the given amount (0..1).
    /// The lengths remain the same as before.
    pub fn quantize(&self, amount: f64) {
        self.ctx.send_double(Processor::Quantize, "", amount);
    }

    /// Transposes the clip by the given semitones.
    pub fn transpose(&self, semitones: i32) {
        self.ctx.send_int(Processor::Clip, "transpose", semitones);
    }

    // ------------------------------------------------------------------------------------------
    // Note clip properties (former INoteClip)

    /// Get the number of monitored rows (notes).
    pub fn num_rows(&self) -> usize {
        self.num_rows
    }

    /// Get the index of the step which is currently played (relative to the clip start, not
    /// to the edit page) or -1 if the clip is not playing.
    pub fn current_step(&self) -> i32 {
        if self.host_grid {
            return self.host_playing_step;
        }
        if self.play_position == -1.0 {
            return -1;
        }
        // Should never happen but who knows...
        let offset = self.play_position - self.clip_start;
        if offset < 0.0 {
            return -1;
        }
        (offset / self.step_length).floor() as i32
    }

    /// Set the length of a step in beats (e.g. [`Resolution::value`]).
    pub fn set_step_length(&mut self, length: f64) {
        self.step_length = length;
        if self.host_grid {
            self.send_grid_double("stepSize", length);
            return;
        }
        self.update_note_data();
    }

    /// Get the length of a step in beats.
    pub fn step_length(&self) -> f64 {
        self.step_length
    }

    /// Get the range of the transpose attribute.
    pub fn step_transpose_range(&self) -> f64 {
        // Not used
        96.0
    }

    // ------------------------------------------------------------------------------------------
    // Steps

    /// Get the info of a step. Returns an empty (off) step info for out of range positions.
    pub fn step(&self, position: &NotePosition) -> &StepInfo {
        match self.index(position.channel, position.step, position.note) {
            Some(index) => self.data.get(&index).unwrap_or(&EMPTY_STEP),
            None => &EMPTY_STEP,
        }
    }

    /// Toggle a note at a step: if there is no note it is created with the current step length
    /// as duration and the given MIDI velocity, otherwise it is removed.
    pub fn toggle_step(&self, position: &NotePosition, velocity: i32) {
        if self.host_grid {
            self.send_grid_int(&format!("{}/toggle", Self::step_path(position)), velocity);
            return;
        }
        let pos = self.beat_position(position.step);
        self.ctx.send_str(
            Processor::Clip,
            &format!("{PATH_NOTE}{}/toggle", position.note),
            &format!(
                "{} {} {} {} 0",
                java_double(pos),
                java_double(self.step_length),
                velocity,
                position.channel
            ),
        );
    }

    /// Set a note at a step with the given MIDI velocity and duration (in beats).
    pub fn set_step(&self, position: &NotePosition, velocity: i32, duration: f64) {
        if self.host_grid {
            self.send_grid_str(&format!("{}/set", Self::step_path(position)), &format!("{velocity} {duration}"));
            return;
        }
        let pos = self.beat_position(position.step);
        self.ctx.send_str(
            Processor::Clip,
            &format!("{PATH_NOTE}{}/set", position.note),
            &format!(
                "{} {} {} {} 0",
                java_double(pos),
                java_double(duration),
                velocity,
                position.channel
            ),
        );
    }

    /// Set a note at a step and copy the velocity and duration from the given step info (former
    /// `setStep (NotePosition, IStepInfo)`).
    pub fn set_step_from(&self, position: &NotePosition, step_info: &StepInfo) {
        if self.host_grid {
            // The host creates the note and sets the other attributes a bit later, as the Java
            // version does
            self.send_grid_str(&format!("{}/setFrom", Self::step_path(position)), &step_info.to_wire());
            return;
        }
        self.set_step(
            position,
            (step_info.velocity() * 127.0) as i32,
            step_info.duration(),
        );
    }

    /// Update the values of an existing note.
    pub fn update_step(
        &self,
        position: &NotePosition,
        velocity: i32,
        duration: f64,
        is_muted: bool,
    ) {
        if self.host_grid {
            let mut info = self.step(position).clone();
            info.set_velocity(velocity as f64 / 127.0);
            info.set_duration(duration);
            info.set_muted(is_muted);
            self.send_grid_str(&format!("{}/update", Self::step_path(position)), &info.to_wire());
            return;
        }
        let pos = self.beat_position(position.step);
        self.ctx.send_str(
            Processor::Clip,
            &format!("{PATH_NOTE}{}/update", position.note),
            &format!(
                "{} {} {} {} {}",
                java_double(pos),
                java_double(duration),
                velocity,
                position.channel,
                if is_muted { "1" } else { "0" }
            ),
        );
    }

    /// Clear the note at a step.
    pub fn clear_step(&self, position: &NotePosition) {
        if self.host_grid {
            self.send_grid(&format!("{}/clear", Self::step_path(position)));
            return;
        }
        self.ctx.send_double(
            Processor::Clip,
            &format!("{PATH_NOTE}{}/clear/{}", position.note, position.channel),
            self.beat_position(position.step),
        );
    }

    /// Moves the note at the step to a different row.
    pub fn move_step_y(&self, position: &NotePosition, new_row: i32) {
        if self.host_grid {
            self.send_grid_int(&format!("{}/moveY", Self::step_path(position)), new_row);
            return;
        }
        self.ctx.send_double(
            Processor::Clip,
            &format!(
                "{PATH_NOTE}{}/moveY/{}/{}",
                position.note, position.channel, new_row
            ),
            self.beat_position(position.step),
        );
    }

    /// Clear all notes on all channels.
    pub fn clear_all(&self) {
        if self.host_grid {
            self.send_grid("clearAll");
            return;
        }
        self.ctx.send(Processor::Clip, "clear");
    }

    /// Clear a row (all notes with the given pitch) on the given channel.
    pub fn clear_row(&self, channel: i32, row: i32) {
        if self.host_grid {
            self.send_grid(&format!("clearRow/{channel}/{row}"));
            return;
        }
        self.ctx.send(
            Processor::Clip,
            &format!("{PATH_NOTE}{row}/clear/{channel}"),
        );
    }

    /// Clear a column (all notes at a step of the edit page) on the given channel.
    pub fn clear_column(&self, channel: i32, column: i32) {
        if self.host_grid {
            self.send_grid(&format!("clearColumn/{channel}/{column}"));
            return;
        }
        self.ctx.send_double(
            Processor::Clip,
            &format!("{PATH_NOTE}0/clearPosition/{channel}"),
            self.beat_position(column),
        );
    }

    // ------------------------------------------------------------------------------------------
    // Editing note attributes

    /// If there is a note started at this position, it will update the mute state of the note.
    pub fn update_step_mute_state(&mut self, position: &NotePosition, is_muted: bool) {
        let send_now = self.edit_steps.is_empty();
        let Some(info) = self.started_step_mut(position) else {
            return;
        };
        info.set_muted(is_muted);
        let (velocity, duration) = (info.velocity(), info.duration());
        if send_now {
            self.update_step(position, (velocity * 127.0) as i32, duration, is_muted);
        }
    }

    /// If there is a note started at this position, it will change the mute state of the note
    /// (increase = mute).
    pub fn change_step_mute_state(&mut self, position: &NotePosition, control: i32) {
        let is_muted = self.ctx.value_changer().is_increase(control);
        self.update_step_mute_state(position, is_muted);
    }

    /// If there is a note started at this position, it will update the duration of the note. A
    /// duration of 0 is ignored (it would delete the note).
    pub fn update_step_duration(&mut self, position: &NotePosition, duration: f64) {
        // Duration of 0 fully deletes the note (Reaper)
        if duration == 0.0 && !self.host_grid {
            return;
        }
        let send_now = self.edit_steps.is_empty();
        let Some(info) = self.started_step_mut(position) else {
            return;
        };
        info.set_duration(duration);
        let (velocity, is_muted) = (info.velocity(), info.is_muted());
        if send_now {
            self.update_step(position, (velocity * 127.0) as i32, duration, is_muted);
        }
    }

    /// If there is a note started at this position, it will change the duration of the note in
    /// 1/32 steps.
    pub fn change_step_duration(&mut self, position: &NotePosition, control: i32) {
        let duration = self.step(position).duration();
        let increase = self.ctx.value_changer().is_increase(control);
        let res = Resolution::Res1_32.value();
        self.update_step_duration(
            position,
            (duration + if increase { res } else { -res }).max(0.0),
        );
    }

    /// If there is a note started at this position, it will update the velocity (0..1) of the
    /// note. A resulting MIDI velocity of 0 is ignored (it would delete the note).
    pub fn update_step_velocity(&mut self, position: &NotePosition, velocity: f64) {
        let midi_velocity = (velocity * 127.0) as i32;
        // Velocity of 0 fully deletes the note (Reaper)
        if midi_velocity == 0 && !self.host_grid {
            return;
        }
        let send_now = self.edit_steps.is_empty();
        let Some(info) = self.started_step_mut(position) else {
            return;
        };
        info.set_velocity(velocity);
        let (duration, is_muted) = (info.duration(), info.is_muted());
        if send_now {
            self.update_step(position, midi_velocity, duration, is_muted);
        }
    }

    /// If there is a note started at this position, it will change the velocity of the note.
    pub fn change_step_velocity(&mut self, position: &NotePosition, control: i32) {
        let value_changer = self.ctx.value_changer();
        let change = value_changer
            .to_normalized_value(value_changer.calc_knob_change(control) as i32 as f64);
        let velocity = self.step(position).velocity() + change;
        self.update_step_velocity(position, velocity.clamp(0.0, 1.0));
    }

    /// Change an attribute of the note started (or continued) at the position; only with a host
    /// grid, Reaper does not support the other attributes. The change is sent unless notes are
    /// edited (see [`Self::start_edit`]).
    fn update_host_step(&mut self, position: &NotePosition, apply: impl FnOnce(&mut StepInfo)) {
        if !self.host_grid {
            return;
        }
        let send_now = self.edit_steps.is_empty();
        let Some(info) = self.started_step_mut(position) else {
            return;
        };
        apply(info);
        if send_now {
            self.send_clip_data(position);
        }
    }

    /// The normalized change of a knob (former `toNormalizedValue (calcKnobChange (control))`).
    fn knob_change(&self, control: i32) -> f64 {
        let value_changer = self.ctx.value_changer();
        value_changer.to_normalized_value(value_changer.calc_knob_change(control) as i32 as f64)
    }

    fn increment(&self, control: i32) -> i32 {
        if self.ctx.value_changer().is_increase(control) {
            1
        } else {
            -1
        }
    }

    /// If there is a note started at this position, it will update the velocity spread of the
    /// note.
    pub fn update_step_velocity_spread(&mut self, position: &NotePosition, velocity_spread: f64) {
        self.update_host_step(position, |i| i.set_velocity_spread(velocity_spread.clamp(0.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the velocity spread of the
    /// note.
    pub fn change_step_velocity_spread(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).velocity_spread() + self.knob_change(control);
        self.update_step_velocity_spread(position, value);
    }

    /// If there is a note started at this position, it will update the release velocity of the
    /// note.
    pub fn update_step_release_velocity(&mut self, position: &NotePosition, release_velocity: f64) {
        self.update_host_step(position, |i| i.set_release_velocity(release_velocity.clamp(0.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the release velocity of the
    /// note.
    pub fn change_step_release_velocity(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).release_velocity() + self.knob_change(control);
        self.update_step_release_velocity(position, value);
    }

    /// If there is a note started at this position, it will update the pressure of the note.
    pub fn update_step_pressure(&mut self, position: &NotePosition, pressure: f64) {
        self.update_host_step(position, |i| i.set_pressure(pressure.clamp(0.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the pressure of the note.
    pub fn change_step_pressure(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).pressure() + self.knob_change(control);
        self.update_step_pressure(position, value);
    }

    /// If there is a note started at this position, it will update the timbre of the note.
    pub fn update_step_timbre(&mut self, position: &NotePosition, timbre: f64) {
        self.update_host_step(position, |i| i.set_timbre(timbre.clamp(-1.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the timbre of the note.
    pub fn change_step_timbre(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).timbre() + 2.0 * self.knob_change(control);
        self.update_step_timbre(position, value);
    }

    /// If there is a note started at this position, it will update the panning of the note.
    pub fn update_step_pan(&mut self, position: &NotePosition, panning: f64) {
        self.update_host_step(position, |i| i.set_pan(panning.clamp(-1.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the panning of the note.
    pub fn change_step_pan(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).pan() + 2.0 * self.knob_change(control);
        self.update_step_pan(position, value);
    }

    /// If there is a note started at this position, it will update the transposition of the
    /// note.
    pub fn update_step_transpose(&mut self, position: &NotePosition, semitones: f64) {
        let range = self.step_transpose_range();
        self.update_host_step(position, |i| i.set_transpose(semitones.clamp(-range, range)));
    }

    /// If there is a note started at this position, it will change the transposition of the
    /// note: in steps of 0.1 for small and of 1 for large knob changes.
    pub fn change_step_transpose(&mut self, position: &NotePosition, control: i32) {
        let value_changer = self.ctx.value_changer();
        let change = value_changer.calc_knob_change(control) / value_changer.step_size() as f64;
        let mut v = if change > -1.0 && change < 1.0 { 0.1 } else { 1.0 };
        if !value_changer.is_increase(control) {
            v = -v;
        }
        let value = self.step(position).transpose() + v;
        self.update_step_transpose(position, value);
    }

    /// If there is a note started at this position, it will update the gain of the note.
    pub fn update_step_gain(&mut self, position: &NotePosition, gain: f64) {
        self.update_host_step(position, |i| i.set_gain(gain.clamp(0.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the gain of the note.
    pub fn change_step_gain(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).gain() + self.knob_change(control);
        self.update_step_gain(position, value);
    }

    /// If there is a note started at this position, it will update the on/off state of the
    /// chance of the note.
    pub fn update_step_is_chance_enabled(&mut self, position: &NotePosition, is_enabled: bool) {
        self.update_host_step(position, |i| i.set_is_chance_enabled(is_enabled));
    }

    /// If there is a note started at this position, it will update the chance of the note.
    pub fn update_step_chance(&mut self, position: &NotePosition, chance: f64) {
        self.update_host_step(position, |i| i.set_chance(chance.clamp(0.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the chance of the note.
    pub fn change_step_chance(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).chance() + self.knob_change(control);
        self.update_step_chance(position, value);
    }

    /// If there is a note started at this position, it will update the on/off state of the
    /// occurrence of the note.
    pub fn update_step_is_occurrence_enabled(&mut self, position: &NotePosition, is_enabled: bool) {
        self.update_host_step(position, |i| i.set_is_occurrence_enabled(is_enabled));
    }

    /// If there is a note started at this position, it will change the occurrence of the note
    /// to the next or previous.
    pub fn set_step_prev_next_occurrence(&mut self, position: &NotePosition, increase: bool) {
        let types = NoteOccurrenceType::ALL;
        let index = types.iter().position(|t| *t == self.step(position).occurrence()).unwrap_or(0) as i32;
        let new_index = (index + if increase { 1 } else { -1 }).clamp(0, types.len() as i32 - 1);
        self.set_step_occurrence(position, types[new_index as usize]);
    }

    /// If there is a note started at this position, it will set the occurrence of the note.
    pub fn set_step_occurrence(&mut self, position: &NotePosition, occurrence: NoteOccurrenceType) {
        self.update_host_step(position, |i| i.set_occurrence(occurrence));
    }

    /// If there is a note started at this position, it will update the on/off state of the
    /// recurrence of the note.
    pub fn update_step_is_recurrence_enabled(&mut self, position: &NotePosition, is_enabled: bool) {
        self.update_host_step(position, |i| i.set_is_recurrence_enabled(is_enabled));
    }

    /// If there is a note started at this position, it will update the recurrence length of the
    /// note (1..8).
    pub fn update_step_recurrence_length(&mut self, position: &NotePosition, recurrence_length: i32) {
        self.update_host_step(position, |i| i.set_recurrence_length(recurrence_length.clamp(1, 8)));
    }

    /// If there is a note started at this position, it will update the recurrence mask of the
    /// note.
    pub fn update_step_recurrence_mask(&mut self, position: &NotePosition, mask: i32) {
        self.update_host_step(position, |i| i.set_recurrence_mask(mask));
    }

    /// If there is a note started at this position, it will change the recurrence length of the
    /// note.
    pub fn change_step_recurrence_length(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).recurrence_length() + self.increment(control);
        self.update_step_recurrence_length(position, value);
    }

    /// If there is a note started at this position, it will update the on/off state of repeat
    /// of the note.
    pub fn update_step_is_repeat_enabled(&mut self, position: &NotePosition, is_enabled: bool) {
        self.update_host_step(position, |i| i.set_is_repeat_enabled(is_enabled));
    }

    /// If there is a note started at this position, it will update the repeat count of the note.
    pub fn update_step_repeat_count(&mut self, position: &NotePosition, value: i32) {
        self.update_host_step(position, |i| i.set_repeat_count(value.clamp(-127, 127)));
    }

    /// If there is a note started at this position, it will change the repeat count of the note.
    pub fn change_step_repeat_count(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).repeat_count() + self.increment(control);
        self.update_step_repeat_count(position, value);
    }

    /// If there is a note started at this position, it will update the repeat curve of the note.
    pub fn update_step_repeat_curve(&mut self, position: &NotePosition, value: f64) {
        self.update_host_step(position, |i| i.set_repeat_curve(value.clamp(-1.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the repeat curve of the note.
    pub fn change_step_repeat_curve(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).repeat_curve() + self.knob_change(control);
        self.update_step_repeat_curve(position, value);
    }

    /// If there is a note started at this position, it will update the repeat velocity curve of
    /// the note.
    pub fn update_step_repeat_velocity_curve(&mut self, position: &NotePosition, value: f64) {
        self.update_host_step(position, |i| i.set_repeat_velocity_curve(value.clamp(-1.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the repeat velocity curve of
    /// the note.
    pub fn change_step_repeat_velocity_curve(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).repeat_velocity_curve() + self.knob_change(control);
        self.update_step_repeat_velocity_curve(position, value);
    }

    /// If there is a note started at this position, it will update the repeat velocity end of
    /// the note.
    pub fn update_step_repeat_velocity_end(&mut self, position: &NotePosition, value: f64) {
        self.update_host_step(position, |i| i.set_repeat_velocity_end(value.clamp(-1.0, 1.0)));
    }

    /// If there is a note started at this position, it will change the repeat velocity end of
    /// the note.
    pub fn change_step_repeat_velocity_end(&mut self, position: &NotePosition, control: i32) {
        let value = self.step(position).repeat_velocity_end() + self.knob_change(control);
        self.update_step_repeat_velocity_end(position, value);
    }

    /// If there is a note started at this position, it will toggle the given bit (cycle) of the
    /// recurrence mask of the note (former default method of `INoteClip`).
    pub fn update_step_recurrence_mask_toggle_bit(&mut self, position: &NotePosition, step: i32) {
        let info = self.step(position);
        if info.state() == StepState::Off {
            return;
        }
        let bit = 1 << step;
        let mask = info.recurrence_mask() ^ bit;
        self.update_step_recurrence_mask(position, mask);
    }

    /// Start editing one or more notes. Prevents round-trip errors when quickly changing values,
    /// which are not set at the same time in Reaper: while editing, changes are only stored
    /// locally, incoming note data is ignored, and the edited steps are sent to Reaper every
    /// 100ms (see [`Self::run_due_tasks`]) and when the edit is stopped.
    pub fn start_edit(&mut self, edit_steps: &[NotePosition]) {
        // Is there a previous edit, which is not stopped yet?
        self.stop_edit();

        self.edit_steps.extend_from_slice(edit_steps);
        let now = Instant::now();
        for step in edit_steps {
            self.delayed_update(*step, now);
        }
    }

    /// Stop editing the note steps, which were started with [`Self::start_edit`]. Sends the
    /// final data of all edited steps.
    pub fn stop_edit(&mut self) {
        if self.edit_steps.is_empty() {
            return;
        }
        let edit_steps = std::mem::take(&mut self.edit_steps);
        for edit_step in &edit_steps {
            self.send_clip_data(edit_step);
        }
        self.pending_updates.clear();
        self.update_note_data();
    }

    /// True if notes are currently edited (see [`Self::start_edit`]).
    pub fn is_editing(&self) -> bool {
        !self.edit_steps.is_empty()
    }

    // ------------------------------------------------------------------------------------------
    // Querying data

    /// Does the row contain any notes (on the current page)?
    pub fn has_row_data(&self, channel: i32, row: i32) -> bool {
        (0..self.num_steps as i32).any(|step| self.state_at(channel, step, row) != StepState::Off)
    }

    /// Does the step (column of the current page) contain any notes?
    pub fn has_column_data(&self, channel: i32, column: i32) -> bool {
        (0..self.num_rows as i32).any(|row| self.state_at(channel, column, row) != StepState::Off)
    }

    /// Get the lowest row (note) which contains data, ignores the MIDI channel. Returns -1 if
    /// there is no data.
    pub fn lowest_row_with_data(&self) -> i32 {
        (0..NUM_CHANNELS as i32)
            .map(|channel| self.lowest_row_with_data_on(channel))
            .filter(|&row| row >= 0)
            .min()
            .unwrap_or(-1)
    }

    /// Get the highest row (note) which contains data, ignores the MIDI channel. Returns -1 if
    /// there is no data.
    pub fn highest_row_with_data(&self) -> i32 {
        (0..NUM_CHANNELS as i32)
            .map(|channel| self.highest_row_with_data_on(channel))
            .max()
            .unwrap_or(-1)
    }

    /// Get the lowest row (note) of the given channel which contains data or -1.
    pub fn lowest_row_with_data_on(&self, channel: i32) -> i32 {
        (0..self.num_rows as i32)
            .find(|&row| self.has_row_data(channel, row))
            .unwrap_or(-1)
    }

    /// Get the highest row (note) of the given channel which contains data or -1.
    pub fn highest_row_with_data_on(&self, channel: i32) -> i32 {
        (0..self.num_rows as i32)
            .rev()
            .find(|&row| self.has_row_data(channel, row))
            .unwrap_or(-1)
    }

    /// Get the highest row (note) which contains data at the given step or -1.
    pub fn highest_row(&self, channel: i32, step: i32) -> i32 {
        (0..self.num_rows as i32)
            .rev()
            .find(|&row| self.state_at(channel, step, row) != StepState::Off)
            .unwrap_or(-1)
    }

    /// Get the next note from the given position. It starts with searching the rows of the
    /// current column downwards (lower notes) and then advances to the next column. Starts at
    /// the beginning of the page if `active` is `None`. If `ignore_channel` is true, all MIDI
    /// channels are searched, otherwise only the channel of the given position.
    pub fn next_note(
        &self,
        active: Option<&NotePosition>,
        ignore_channel: bool,
    ) -> Option<NotePosition> {
        let pos = active.copied().unwrap_or(NotePosition::new(0, 0, 128));
        let channels = self.search_channels(pos.channel, ignore_channel);
        let top_row = self.num_rows as i32 - 1;
        for step in pos.step.max(0)..self.num_steps as i32 {
            let start_note = if step == pos.step { pos.note - 1 } else { 127 };
            for row in (0..=start_note.min(top_row)).rev() {
                for channel in channels.clone() {
                    if self.state_at(channel, step, row) == StepState::Start {
                        return Some(NotePosition::new(channel, step, row));
                    }
                }
            }
        }
        None
    }

    /// Get the previous note from the given position. It starts with searching the rows of the
    /// current column upwards (higher notes) and then advances to the previous column. Starts at
    /// the end of the page if `active` is `None`. If `ignore_channel` is true, all MIDI channels
    /// are searched, otherwise only the channel of the given position.
    pub fn previous_note(
        &self,
        active: Option<&NotePosition>,
        ignore_channel: bool,
    ) -> Option<NotePosition> {
        let pos = active
            .copied()
            .unwrap_or(NotePosition::new(0, self.num_steps as i32 - 1, -1));
        let channels = self.search_channels(pos.channel, ignore_channel);
        let row_end = (self.num_rows as i32).min(128);
        for step in (0..=pos.step.min(self.num_steps as i32 - 1)).rev() {
            let start_note = if step == pos.step { pos.note + 1 } else { 0 };
            for row in start_note.max(0)..row_end {
                for channel in channels.clone() {
                    if self.state_at(channel, step, row) == StepState::Start {
                        return Some(NotePosition::new(channel, step, row));
                    }
                }
            }
        }
        None
    }

    // ------------------------------------------------------------------------------------------
    // Paging

    /// Scroll the clip view to the given page. The page length depends on the number of steps
    /// and the step length.
    pub fn scroll_to_page(&mut self, page: i32) {
        if self.host_grid {
            self.edit_page = page.max(0);
            self.send_grid_int("scroll", self.edit_page * self.num_steps as i32);
            return;
        }
        self.edit_page = page.clamp(0, (self.max_page - 1).max(0));
        self.update_note_data();
    }

    /// Get the edit page.
    pub fn edit_page(&self) -> i32 {
        self.edit_page
    }

    /// Scroll the steps one page backwards.
    pub fn scroll_steps_page_backwards(&mut self) {
        if self.host_grid {
            // As the Java version for Bitwig: the page scrolling of the host
            if self.edit_page <= 0 {
                return;
            }
            self.send_grid("page/-");
            self.edit_page -= 1;
            return;
        }
        self.scroll_to_page(self.edit_page - 1);
    }

    /// Scroll the steps one page forwards.
    pub fn scroll_steps_page_forward(&mut self) {
        if self.host_grid {
            self.send_grid("page/+");
            self.edit_page += 1;
            return;
        }
        self.scroll_to_page(self.edit_page + 1);
    }

    /// Reports if the note grid steps can be scrolled backwards.
    pub fn can_scroll_steps_backwards(&self) -> bool {
        self.edit_page() > 0
    }

    /// Reports if the note grid steps can be scrolled forwards.
    pub fn can_scroll_steps_forwards(&self) -> bool {
        // The host grid can always be scrolled (as the Java version for Bitwig)
        self.host_grid || self.edit_page < self.max_page - 1
    }

    /// Get the number of pages of the clip (depends on the clip length, the number of steps and
    /// the step length).
    pub fn num_pages(&self) -> i32 {
        self.max_page
    }

    // ------------------------------------------------------------------------------------------
    // Internals

    /// The index into the data array or `None` if the position is out of range.
    fn index(&self, channel: i32, step: i32, row: i32) -> Option<usize> {
        if !(0..NUM_CHANNELS as i32).contains(&channel)
            || !(0..self.num_steps as i32).contains(&step)
            || !(0..self.num_rows as i32).contains(&row)
        {
            return None;
        }
        let (channel, step, row) = (channel as usize, step as usize, row as usize);
        Some((channel * self.num_steps + step) * self.num_rows + row)
    }

    fn state_at(&self, channel: i32, step: i32, row: i32) -> StepState {
        self.index(channel, step, row)
            .map_or(StepState::Off, |index| self.data.get(&index).map_or(StepState::Off, StepInfo::state))
    }

    /// The step info at the position if a note starts or continues there.
    fn started_step_mut(&mut self, position: &NotePosition) -> Option<&mut StepInfo> {
        let index = self.index(position.channel, position.step, position.note)?;
        let info = self.data.get_mut(&index)?;
        (info.state() != StepState::Off).then_some(info)
    }

    fn search_channels(&self, channel: i32, ignore_channel: bool) -> std::ops::Range<i32> {
        if ignore_channel {
            0..NUM_CHANNELS as i32
        } else {
            channel..channel + 1
        }
    }

    /// The position in beats of a step on the edit page.
    fn beat_position(&self, step: i32) -> f64 {
        (step + self.edit_page * self.num_steps as i32) as f64 * self.step_length
    }

    fn delayed_update(&mut self, edit_step: NotePosition, now: Instant) {
        if self.edit_steps.is_empty() {
            return;
        }
        self.send_clip_data(&edit_step);
        self.pending_updates
            .push((now + EDIT_UPDATE_DELAY, edit_step));
    }

    /// Send the locally changed step data to Reaper (or all values to the host grid).
    fn send_clip_data(&self, position: &NotePosition) {
        let info = self.step(position);
        if self.host_grid {
            if info.state() != StepState::Off {
                self.send_grid_str(&format!("{}/update", Self::step_path(position)), &info.to_wire());
            }
            return;
        }
        self.update_step(
            position,
            (info.velocity() * 127.0) as i32,
            info.duration(),
            info.is_muted(),
        );
    }

    /// Calculate the number of available pages.
    fn calc_pages(&mut self) {
        let length = self.clip_end - self.clip_start;
        let page_length = self.num_steps as f64 * self.step_length;
        self.max_page = (length / page_length).ceil() as i32;
        // Make sure the page is inside the new range
        // (saturating: a clip with 0 steps, e.g. of the clip length view, has an infinite or
        // undefined number of pages)
        self.edit_page = self.edit_page.clamp(0, self.max_page.saturating_sub(1).max(0));
    }

    fn update_note_data(&mut self) {
        // The steps of a host grid come from the host
        if self.host_grid {
            return;
        }
        // While editing the local data is kept
        if self.edit_steps.is_empty() {
            self.data.clear();
            let notes = std::mem::take(&mut self.notes);
            for note in &notes {
                self.update_note(note);
            }
            self.notes = notes;
        }
        self.calc_pages();
    }

    fn update_note(&mut self, note: &Note) {
        // Is the note on the current page window?
        let row = note.pitch();
        if row < 0 || row >= self.num_rows as i32 {
            return;
        }
        let step = (note.start() / self.step_length).floor() as i32;
        let page_offset = self.edit_page * self.num_steps as i32;
        let rel_to_page = step - page_offset;
        let channel = note.channel();
        let Some(index) = self.index(channel, rel_to_page, row) else {
            return;
        };

        let info = self.data.entry(index).or_default();
        info.set_selected(note.is_selected());
        info.set_muted(note.is_muted());
        info.set_state(StepState::Start);
        info.set_duration(note.end() - note.start());
        info.set_velocity(note.velocity() as f64 / 127.0);

        // Extend longer notes
        let end_step = ((note.end() / self.step_length).floor() as i32 - page_offset)
            .min(self.num_steps as i32);
        for i in rel_to_page + 1..end_step {
            if let Some(index) = self.index(channel, i, row) {
                let info = self.data.entry(index).or_default();
                info.set_state(StepState::Continue);
                info.set_selected(note.is_selected());
                info.set_muted(note.is_muted());
            }
        }
    }
}

/// Format a double like Java's `Double.toString` (e.g. "0.0", "0.25", "1.0E-4"), since the C++
/// side parses the strings which were created by Java.
fn java_double(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let abs = value.abs();
    if abs == 0.0 || (1e-3..1e7).contains(&abs) {
        // Debug formatting is the shortest representation which always contains a fraction
        return format!("{value:?}");
    }
    let formatted = format!("{value:e}");
    let (mantissa, exponent) = formatted.split_once('e').unwrap_or((&formatted, "0"));
    if mantissa.contains('.') {
        format!("{mantissa}E{exponent}")
    } else {
        format!("{mantissa}.0E{exponent}")
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use crate::framework::value_changer::ValueChanger;
    use crate::host::{Host, MemoryLog};

    fn setup(num_steps: usize, num_rows: usize) -> (CursorClip, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        let sender = Sender::new(backend.clone());
        let host = Host::new(Rc::new(MemoryLog::default()));
        let ctx = DawContext::new(host, sender, ValueChanger::twos_complement(1024, 1));
        (CursorClip::new(ctx, num_steps, num_rows), backend)
    }

    fn str_call(command: &str, value: &str) -> BackendCall {
        BackendCall::StringArg("clip".into(), command.into(), value.into())
    }

    fn double_call(command: &str, value: f64) -> BackendCall {
        BackendCall::DoubleArg("clip".into(), command.into(), value)
    }

    fn state(clip: &CursorClip, channel: i32, step: i32, note: i32) -> StepState {
        clip.step(&NotePosition::new(channel, step, note)).state()
    }

    #[test]
    fn java_double_format() {
        assert_eq!(java_double(0.0), "0.0");
        assert_eq!(java_double(1.0), "1.0");
        assert_eq!(java_double(0.25), "0.25");
        assert_eq!(java_double(1.0 / 3.0), "0.3333333333333333");
        assert_eq!(java_double(0.001), "0.001");
        assert_eq!(java_double(0.0001), "1.0E-4");
        assert_eq!(java_double(1.5e-5), "1.5E-5");
        assert_eq!(java_double(12345678.0), "1.2345678E7");
        assert_eq!(java_double(-2.5), "-2.5");
    }

    #[test]
    fn notes_to_step_states() {
        let (mut clip, _) = setup(8, 128);
        clip.set_play_start_intern(0.0);
        clip.set_play_end_intern(4.0);
        clip.set_notes(Note::parse_notes(
            "0:0:0.0:0.25:0:60:127;1:1:0.5:1.25:2:62:64;0:0:0.0:1.0:0:200:100",
        ));

        assert_eq!(state(&clip, 0, 0, 60), StepState::Start);
        assert_eq!(state(&clip, 0, 1, 60), StepState::Off);
        let info = clip.step(&NotePosition::new(0, 0, 60));
        assert_eq!(info.duration(), 0.25);
        assert_eq!(info.velocity(), 1.0);

        // Note from 0.5 to 1.25 -> steps 2 (start), 3, 4 (continue)
        assert_eq!(state(&clip, 2, 1, 62), StepState::Off);
        assert_eq!(state(&clip, 2, 2, 62), StepState::Start);
        assert_eq!(state(&clip, 2, 3, 62), StepState::Continue);
        assert_eq!(state(&clip, 2, 4, 62), StepState::Continue);
        assert_eq!(state(&clip, 2, 5, 62), StepState::Off);
        assert!(clip.step(&NotePosition::new(2, 3, 62)).is_muted());
        assert!(clip.step(&NotePosition::new(2, 3, 62)).is_selected());
        assert_eq!(state(&clip, 0, 2, 62), StepState::Off);

        // Out of range
        assert_eq!(state(&clip, 0, -1, 60), StepState::Off);
        assert_eq!(state(&clip, 16, 0, 60), StepState::Off);
        assert_eq!(state(&clip, 0, 8, 60), StepState::Off);

        assert!(clip.has_row_data(0, 60));
        assert!(!clip.has_row_data(0, 61));
        assert!(clip.has_column_data(2, 3));
        assert!(!clip.has_column_data(2, 5));
        assert_eq!(clip.lowest_row_with_data(), 60);
        assert_eq!(clip.highest_row_with_data(), 62);
        assert_eq!(clip.lowest_row_with_data_on(2), 62);
        assert_eq!(clip.highest_row_with_data_on(3), -1);
        assert_eq!(clip.highest_row(2, 4), 62);
        assert_eq!(clip.highest_row(0, 4), -1);

        // A different resolution: 1/8 -> the 0.5..1.25 note starts at step 1; it does not
        // continue at step 2 since only steps before the step containing the end are extended
        clip.set_step_length(Resolution::Res1_8.value());
        assert_eq!(state(&clip, 2, 1, 62), StepState::Start);
        assert_eq!(state(&clip, 2, 2, 62), StepState::Off);
        // 0..1 -> start at 0, continue at 1
        assert_eq!(state(&clip, 0, 0, 60), StepState::Start);
        clip.set_notes(vec![Note::new(false, false, 0.0, 1.0, 0, 60, 127)]);
        assert_eq!(state(&clip, 0, 1, 60), StepState::Continue);
        assert_eq!(state(&clip, 0, 2, 60), StepState::Off);

        // Clearing the notes
        clip.set_notes(Vec::new());
        assert_eq!(state(&clip, 2, 1, 62), StepState::Off);
        assert_eq!(clip.lowest_row_with_data(), -1);
        assert_eq!(clip.highest_row_with_data(), -1);
    }

    #[test]
    fn paging() {
        let (mut clip, _) = setup(8, 128);
        // 8 steps of 1/16 = 2 beats per page
        clip.set_play_start_intern(0.0);
        clip.set_play_end_intern(5.0);
        assert_eq!(clip.num_pages(), 3);
        assert_eq!(clip.edit_page(), 0);
        assert!(!clip.can_scroll_steps_backwards());
        assert!(clip.can_scroll_steps_forwards());

        clip.set_notes(vec![Note::new(false, false, 2.25, 2.5, 0, 36, 100)]);
        assert!(!clip.has_row_data(0, 36));

        clip.scroll_steps_page_forward();
        assert_eq!(clip.edit_page(), 1);
        assert_eq!(state(&clip, 0, 1, 36), StepState::Start);
        assert!(clip.can_scroll_steps_backwards());

        clip.scroll_steps_page_forward();
        clip.scroll_steps_page_forward();
        assert_eq!(clip.edit_page(), 2);
        assert!(!clip.can_scroll_steps_forwards());
        assert!(!clip.has_row_data(0, 36));

        clip.scroll_to_page(-5);
        assert_eq!(clip.edit_page(), 0);

        // Shrinking the clip moves the page into range
        clip.scroll_to_page(2);
        clip.set_play_end_intern(2.0);
        assert_eq!(clip.num_pages(), 1);
        assert_eq!(clip.edit_page(), 0);

        // No clip: must not panic (Java's Math.clamp threw)
        let (mut clip, _) = setup(8, 128);
        clip.scroll_to_page(3);
        assert_eq!(clip.edit_page(), 0);
    }

    #[test]
    fn current_step() {
        let (mut clip, _) = setup(16, 128);
        assert_eq!(clip.current_step(), -1);
        clip.set_play_start_intern(1.0);
        clip.set_play_position(2.3);
        assert_eq!(clip.current_step(), 5);
        clip.set_play_position(0.5);
        assert_eq!(clip.current_step(), -1);
    }

    #[test]
    fn step_commands() {
        let (mut clip, backend) = setup(8, 128);
        clip.set_play_start_intern(0.0);
        clip.set_play_end_intern(8.0);
        clip.scroll_to_page(1);
        backend.take_calls();

        let pos = NotePosition::new(3, 2, 60);
        clip.toggle_step(&pos, 100);
        clip.set_step(&pos, 90, 0.5);
        clip.update_step(&pos, 80, 0.75, true);
        clip.clear_step(&pos);
        clip.move_step_y(&pos, 62);
        clip.clear_row(3, 60);
        clip.clear_column(3, 1);
        clip.clear_all();
        let mut info = StepInfo::new();
        info.set_velocity(0.5);
        clip.set_step_from(&pos, &info);

        // Step 2 on page 1 with 8 steps of 0.25 -> 2.5 beats
        assert_eq!(
            backend.take_calls(),
            vec![
                str_call("note/60/toggle", "2.5 0.25 100 3 0"),
                str_call("note/60/set", "2.5 0.5 90 3 0"),
                str_call("note/60/update", "2.5 0.75 80 3 1"),
                double_call("note/60/clear/3", 2.5),
                double_call("note/60/moveY/3/62", 2.5),
                BackendCall::NoArg("clip".into(), "note/60/clear/3".into()),
                double_call("note/0/clearPosition/3", 2.25),
                BackendCall::NoArg("clip".into(), "clear".into()),
                str_call("note/60/set", "2.5 0.25 63 3 0"),
            ]
        );
    }

    #[test]
    fn clip_commands() {
        let (mut clip, backend) = setup(8, 128);
        clip.enable_observers(true);
        clip.set_name("Foo");
        clip.set_color(ColorEx::from_rgb(255, 0, 128));
        clip.set_loop_enabled(true);
        clip.duplicate();
        clip.duplicate_content();
        clip.quantize(0.5);
        clip.transpose(-12);
        clip.change_play_start(1, false); // ignored, no clip start
        clip.set_play_start_intern(2.0);
        clip.set_play_end_intern(6.0);
        clip.change_play_start(1, false);
        clip.change_play_end(127, true);
        clip.set_play_range(1.0, 2.0);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::EnableUpdates("clip".into(), true),
                str_call("name", "Foo"),
                str_call("color", "RGB(255,0,128)"),
                BackendCall::IntArg("clip".into(), "loop".into(), 1),
                BackendCall::NoArg("clip".into(), "duplicate".into()),
                BackendCall::NoArg("clip".into(), "duplicateContent".into()),
                BackendCall::DoubleArg("quantize".into(), "".into(), 0.5),
                BackendCall::IntArg("clip".into(), "transpose".into(), -12),
                double_call("start", 6.0),
                double_call("end", 5.0),
                double_call("start", 7.0),
                double_call("end", 8.0),
            ]
        );
        assert_eq!(clip.play_start(), 7.0);
        assert_eq!(clip.loop_length(), 1.0);
        assert_eq!(clip.formatted_accent(), "100%");
    }

    #[test]
    fn edit_attributes() {
        let (mut clip, backend) = setup(8, 128);
        clip.set_play_start_intern(0.0);
        clip.set_play_end_intern(2.0);
        clip.set_notes(vec![Note::new(false, false, 0.0, 0.5, 0, 60, 127)]);
        backend.take_calls();

        let pos = NotePosition::new(0, 0, 60);
        let off = NotePosition::new(0, 3, 60);
        clip.update_step_duration(&pos, 0.75);
        clip.update_step_duration(&pos, 0.0); // ignored
        clip.update_step_duration(&off, 0.75); // no note
        clip.update_step_velocity(&pos, 0.5);
        clip.update_step_velocity(&pos, 0.001); // ignored
        clip.update_step_mute_state(&pos, true);
        clip.change_step_duration(&pos, 1);
        assert_eq!(
            backend.take_calls(),
            vec![
                str_call("note/60/update", "0.0 0.75 127 0 0"),
                str_call("note/60/update", "0.0 0.75 63 0 0"),
                str_call("note/60/update", "0.0 0.75 63 0 1"),
                str_call("note/60/update", "0.0 0.875 63 0 1"),
            ]
        );
        let info = clip.step(&pos);
        assert_eq!(info.duration(), 0.875);
        assert_eq!(info.velocity(), 0.5);
        assert!(info.is_muted());

        // Velocity change: 1 step of 1024 range
        clip.change_step_velocity(&pos, 127);
        assert!((clip.step(&pos).velocity() - (0.5 - 1.0 / 1023.0)).abs() < 1e-3);
    }

    #[test]
    fn edit_session() {
        let (mut clip, backend) = setup(8, 128);
        clip.set_play_start_intern(0.0);
        clip.set_play_end_intern(2.0);
        clip.set_notes(vec![Note::new(false, false, 0.0, 0.25, 0, 60, 127)]);
        backend.take_calls();

        let pos = NotePosition::new(0, 0, 60);
        clip.start_edit(&[pos]);
        assert!(clip.is_editing());
        assert_eq!(
            backend.take_calls(),
            vec![str_call("note/60/update", "0.0 0.25 127 0 0")]
        );

        // While editing only the local data changes
        clip.update_step_duration(&pos, 0.5);
        assert!(backend.take_calls().is_empty());

        // Incoming data is ignored while editing
        clip.set_notes(Vec::new());
        assert_eq!(clip.step(&pos).state(), StepState::Start);

        // Not yet due
        clip.run_due_tasks(Instant::now());
        assert!(backend.take_calls().is_empty());
        clip.run_due_tasks(Instant::now() + Duration::from_millis(150));
        assert_eq!(
            backend.take_calls(),
            vec![str_call("note/60/update", "0.0 0.5 127 0 0")]
        );
        assert!(clip.has_pending_tasks());

        clip.stop_edit();
        assert!(!clip.is_editing());
        assert!(!clip.has_pending_tasks());
        assert_eq!(
            backend.take_calls(),
            vec![str_call("note/60/update", "0.0 0.5 127 0 0")]
        );
        // The stored notes are applied again
        assert_eq!(clip.step(&pos).state(), StepState::Off);

        clip.run_due_tasks(Instant::now() + Duration::from_secs(1));
        assert!(backend.take_calls().is_empty());
    }

    #[test]
    fn next_previous_note() {
        let (mut clip, _) = setup(4, 128);
        clip.set_play_start_intern(0.0);
        clip.set_play_end_intern(1.0);
        clip.set_notes(vec![
            Note::new(false, false, 0.0, 0.25, 0, 60, 100),
            Note::new(false, false, 0.0, 0.25, 0, 64, 100),
            Note::new(false, false, 0.5, 0.75, 1, 50, 100),
        ]);

        let first = clip.next_note(None, true).unwrap();
        assert_eq!(first, NotePosition::new(0, 0, 64));
        let second = clip.next_note(Some(&first), true).unwrap();
        assert_eq!(second, NotePosition::new(0, 0, 60));
        let third = clip.next_note(Some(&second), true).unwrap();
        assert_eq!(third, NotePosition::new(1, 2, 50));
        assert_eq!(clip.next_note(Some(&third), true), None);
        assert_eq!(clip.next_note(Some(&second), false), None);

        let last = clip.previous_note(None, true).unwrap();
        assert_eq!(last, NotePosition::new(1, 2, 50));
        let prev = clip.previous_note(Some(&last), true).unwrap();
        assert_eq!(prev, NotePosition::new(0, 0, 60));
        let prev = clip.previous_note(Some(&prev), true).unwrap();
        assert_eq!(prev, NotePosition::new(0, 0, 64));
        assert_eq!(clip.previous_note(Some(&prev), true), None);
    }

    #[test]
    fn small_row_count() {
        // Java's hasColumnData / getNextNote assumed 128 rows
        let (mut clip, _) = setup(8, 8);
        clip.set_play_start_intern(0.0);
        clip.set_play_end_intern(2.0);
        clip.set_notes(vec![
            Note::new(false, false, 0.0, 0.25, 0, 3, 100),
            Note::new(false, false, 0.0, 0.25, 0, 60, 100),
        ]);
        assert!(clip.has_column_data(0, 0));
        assert_eq!(
            clip.next_note(None, false),
            Some(NotePosition::new(0, 0, 3))
        );
        assert_eq!(clip.highest_row_with_data(), 3);
    }
}
