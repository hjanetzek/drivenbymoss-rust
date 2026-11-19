// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The color palette of the Push 2/3 (former `ColorPalette` and `ColorPaletteEntry`).
//!
//! The 128 entries of the device palette are read one after the other (sysex `04 <index>`); every
//! entry which differs from the DrivenByMoss palette ([`super::colors::palette_color_rgb`]) is
//! written (sysex `03 <index> <r> <g> <b> <white>`, keeping the white value of the device) and read
//! again to confirm it. When all entries are done and at least one was written, the palette is
//! re-applied (sysex `05`) one second later.
//!
//! Scheduling (Java: `surface.scheduleTask`): [`ColorPalette::update_palette`] returns what the
//! caller has to schedule next:
//! - [`PaletteUpdate::Continue`]: call `update_palette` again after [`UPDATE_INTERVAL_MS`].
//! - [`PaletteUpdate::Reapply`]: call [`ColorPalette::send_reapply`] after [`REAPPLY_DELAY_MS`].
//! - [`PaletteUpdate::Done`]: nothing to do.
//!
//! The device answers are passed to [`ColorPalette::handle_color_palette_message`] by the sysex
//! handler of the surface (for Push 2/3 messages for which [`is_push2_data`] is true).

use std::time::{Duration, Instant};

use super::colors::palette_color_rgb;
use crate::midi::MidiOutput;

/// The sysex header of the Push 2/3.
pub const SYSEX_HEADER: [u8; 6] = [0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01];

/// The maximum number of read/write attempts.
pub const MAX_NUMBER_OF_RETRIES: u32 = 10;
/// The delay until `update_palette` must be called again.
pub const UPDATE_INTERVAL_MS: u64 = 10;
/// The delay until the palette must be re-applied after all entries are written.
pub const REAPPLY_DELAY_MS: u64 = 1000;
/// The time after which a read request is sent again.
const READ_TIMEOUT: Duration = Duration::from_millis(1000);

const PALETTE_MESSAGE_OUT_ID: u8 = 0x03;
const PALETTE_MESSAGE_IN_ID: u8 = 0x04;
const PALETTE_REAPPLY_ID: u8 = 0x05;
const MESSAGE_LENGTH: usize = 17;

/// Test if the data is a complete Push 2/3 sysex message (former
/// `PushControlSurface.isPush2Data`).
pub fn is_push2_data(data: &[u8]) -> bool {
    data.len() > SYSEX_HEADER.len() && data.starts_with(&SYSEX_HEADER) && data.last() == Some(&0xF7)
}

/// Wrap the command bytes into a Push 2/3 sysex message (former `PushControlSurface.sendSysex`).
pub fn create_sysex(parameters: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(SYSEX_HEADER.len() + parameters.len() + 1);
    data.extend_from_slice(&SYSEX_HEADER);
    data.extend_from_slice(parameters);
    data.push(0xF7);
    data
}

/// What the caller of [`ColorPalette::update_palette`] has to schedule next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteUpdate {
    /// Call `update_palette` again after [`UPDATE_INTERVAL_MS`].
    Continue,
    /// All entries are done and some were written: call `send_reapply` after
    /// [`REAPPLY_DELAY_MS`].
    Reapply,
    /// All entries are done, nothing was written.
    Done,
}

/// The state of a palette entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Read,
    ReadRequested,
    Write,
    Done,
}

/// A color palette entry of the Push 2/3 (former `ColorPaletteEntry`).
#[derive(Debug, Clone)]
pub struct ColorPaletteEntry {
    index: u8,
    red: i32,
    green: i32,
    blue: i32,
    white: i32,
    state: EntryState,
    read_retries: u32,
    write_retries: u32,
    send_timestamp: Option<Instant>,
}

impl ColorPaletteEntry {
    /// Create an entry with the default palette color (RGB 0-255).
    pub fn new(index: u8, color: [i32; 3]) -> Self {
        ColorPaletteEntry {
            index,
            red: color[0],
            green: color[1],
            blue: color[2],
            white: 0,
            state: EntryState::Read,
            read_retries: 0,
            write_retries: 0,
            send_timestamp: None,
        }
    }

    /// Test if the given data is a valid palette entry message (complete sysex message).
    pub fn is_valid(data: &[u8]) -> bool {
        data.len() == MESSAGE_LENGTH && data[6] == PALETTE_MESSAGE_IN_ID
    }

    /// Test if the received color differs from the stored one. Stores the white value of the
    /// device. `data` must be a valid message (see [`Self::is_valid`]).
    pub fn requires_update(&mut self, data: &[u8]) -> bool {
        let value = |i: usize| data[i] as i32 + ((data[i + 1] as i32) << 7);
        self.white = value(14);
        self.red != value(8) || self.green != value(10) || self.blue != value(12)
    }

    /// Increase the read requests. Returns false if the maximum number of retries has been
    /// reached (the entry is then done).
    pub fn inc_read_retries(&mut self, now: Instant) -> bool {
        self.read_retries += 1;
        if self.read_retries > MAX_NUMBER_OF_RETRIES {
            self.state = EntryState::Done;
            return false;
        }
        self.state = EntryState::ReadRequested;
        self.send_timestamp = Some(now);
        true
    }

    /// Increase the write requests. Returns false if the maximum number of retries has been
    /// reached (the entry is then done).
    pub fn inc_write_retries(&mut self, now: Instant) -> bool {
        self.write_retries += 1;
        if self.write_retries > MAX_NUMBER_OF_RETRIES {
            self.state = EntryState::Done;
            return false;
        }
        // Set to read for confirmation check
        self.state = EntryState::Read;
        self.send_timestamp = Some(now);
        true
    }

    pub fn set_write(&mut self) {
        self.state = EntryState::Write;
    }

    /// Set the entry to be OK, which means identical to the device.
    pub fn set_done(&mut self) {
        self.state = EntryState::Done;
    }

    pub fn state(&self) -> EntryState {
        self.state
    }

    /// The command bytes (without sysex header) which write the current color.
    pub fn create_update_message(&self) -> [u8; 10] {
        let lo = |v: i32| (v % 128) as u8;
        let hi = |v: i32| (v / 128) as u8;
        [
            PALETTE_MESSAGE_OUT_ID,
            self.index,
            lo(self.red),
            hi(self.red),
            lo(self.green),
            hi(self.green),
            lo(self.blue),
            hi(self.blue),
            lo(self.white),
            hi(self.white),
        ]
    }

    /// The time when the last read/write request was sent.
    pub fn send_timestamp(&self) -> Option<Instant> {
        self.send_timestamp
    }
}

/// The color palette of the Push 2/3 (former `ColorPalette`).
#[derive(Debug, Clone)]
pub struct ColorPalette {
    entries: Vec<ColorPaletteEntry>,
    entries_has_update: bool,
    errors: Vec<String>,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorPalette {
    pub fn new() -> Self {
        let entries = (0..128u8).map(|i| ColorPaletteEntry::new(i, palette_color_rgb(i as i32))).collect();
        ColorPalette { entries, entries_has_update: false, errors: Vec::new() }
    }

    /// Check the next entry of the device palette and send a read or write request if necessary
    /// (former `updatePalette`). `now` is the current core time (`ctx.host.now()`). The result
    /// says what the caller must schedule next (see the module documentation).
    pub fn update_palette(&mut self, output: &MidiOutput, now: Instant) -> PaletteUpdate {
        let (message, update) = self.next_step(now);
        if let Some(message) = message {
            output.send_sysex(&create_sysex(&message));
        }
        update
    }

    /// Re-apply the color palette on the device (sysex `05`).
    pub fn send_reapply(output: &MidiOutput) {
        output.send_sysex(&create_sysex(&[PALETTE_REAPPLY_ID]));
    }

    /// The command bytes to send (if any) and what to schedule next.
    fn next_step(&mut self, now: Instant) -> (Option<Vec<u8>>, PaletteUpdate) {
        // All done?
        let Some(entry_index) = self.find_next_entry() else {
            // Re-apply the color palette, if necessary
            return (None, if self.entries_has_update { PaletteUpdate::Reapply } else { PaletteUpdate::Done });
        };

        let message = match self.entries[entry_index].state() {
            EntryState::Read => self.color_entry_request(entry_index, now),
            EntryState::ReadRequested => {
                // 2nd attempt after 1 second...
                let timestamp = self.entries[entry_index].send_timestamp();
                if timestamp.is_none_or(|t| now.saturating_duration_since(t) > READ_TIMEOUT) {
                    self.color_entry_request(entry_index, now)
                } else {
                    None
                }
            }
            EntryState::Write => {
                let entry = &mut self.entries[entry_index];
                if entry.inc_write_retries(now) {
                    Some(entry.create_update_message().to_vec())
                } else {
                    self.errors.push(format!("Failed writing color palette entry #{entry_index}."));
                    None
                }
            }
            EntryState::Done => return (None, PaletteUpdate::Done),
        };
        (message, PaletteUpdate::Continue)
    }

    /// Handle a color palette system exclusive message (former `handleColorPaletteMessage`).
    /// `data` is the complete sysex message including the header and `F7`; other messages are
    /// ignored.
    pub fn handle_color_palette_message(&mut self, data: &[u8]) {
        if !ColorPaletteEntry::is_valid(data) {
            return;
        }
        let Some(entry) = self.entries.get_mut(data[7] as usize) else {
            return;
        };

        // Is an update of the color palette entry necessary?
        if !entry.requires_update(data) {
            entry.set_done();
            return;
        }

        self.entries_has_update = true;
        entry.set_write();
    }

    /// Take the error messages collected since the last call (Java logs them with
    /// `surface.errorln`); the caller logs them with `host.error`.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// Get the state of an entry.
    pub fn entry_state(&self, index: usize) -> EntryState {
        self.entries[index].state()
    }

    /// Get the next palette entry which needs to be updated.
    fn find_next_entry(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.state() != EntryState::Done)
    }

    /// The request to the Push 2/3 to send the values of an entry of the current color palette.
    fn color_entry_request(&mut self, entry_index: usize, now: Instant) -> Option<Vec<u8>> {
        if !self.entries[entry_index].inc_read_retries(now) {
            self.errors.push(format!("Failed reading color palette entry #{entry_index}."));
            return None;
        }
        Some(vec![PALETTE_MESSAGE_IN_ID, entry_index as u8])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use std::rc::Rc;

    fn output() -> (MidiOutput, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        (MidiOutput::new(Sender::new(backend.clone()), Some(1)), backend)
    }

    fn sysex_calls(backend: &RecordingBackend) -> Vec<Vec<u8>> {
        backend
            .take_calls()
            .into_iter()
            .filter_map(|call| match call {
                BackendCall::SendMidi(_, data) if data.first() == Some(&0xF0) => Some(data),
                _ => None,
            })
            .collect()
    }

    /// The answer of the device for an entry.
    fn entry_message(index: u8, rgbw: [i32; 4]) -> Vec<u8> {
        let mut data = vec![0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x04, index];
        for v in rgbw {
            data.push((v % 128) as u8);
            data.push((v / 128) as u8);
        }
        data.push(0xF7);
        data
    }

    fn answer_all_matching(palette: &mut ColorPalette, from: u8) {
        for i in from..128 {
            let [r, g, b] = palette_color_rgb(i as i32);
            palette.handle_color_palette_message(&entry_message(i, [r, g, b, 0]));
        }
    }

    #[test]
    fn push2_data() {
        assert!(is_push2_data(&[0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x04, 0xF7]));
        assert!(!is_push2_data(&[0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01]));
        assert!(!is_push2_data(&[0xF0, 0x47, 0x7F, 0x15, 0x01, 0x01, 0x04, 0xF7]));
        assert_eq!(create_sysex(&[0x05]), vec![0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x05, 0xF7]);
    }

    #[test]
    fn read_write_confirm_reapply() {
        let (out, backend) = output();
        let mut palette = ColorPalette::new();
        let t0 = Instant::now();

        // Request entry 0
        assert_eq!(palette.update_palette(&out, t0), PaletteUpdate::Continue);
        assert_eq!(sysex_calls(&backend), vec![vec![0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x04, 0x00, 0xF7]]);
        assert_eq!(palette.entry_state(0), EntryState::ReadRequested);

        // Waiting for the answer: nothing is sent
        assert_eq!(palette.update_palette(&out, t0 + Duration::from_millis(10)), PaletteUpdate::Continue);
        assert!(sysex_calls(&backend).is_empty());

        // Entry 0 is black on the DrivenByMoss palette: the device has a different color (white
        // value 0x123 is kept)
        palette.handle_color_palette_message(&entry_message(0, [1, 2, 300, 0x123]));
        assert_eq!(palette.entry_state(0), EntryState::Write);
        assert_eq!(palette.update_palette(&out, t0 + Duration::from_millis(20)), PaletteUpdate::Continue);
        assert_eq!(
            sysex_calls(&backend),
            vec![vec![0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x03, 0x00, 0, 0, 0, 0, 0, 0, 0x23, 0x02, 0xF7]]
        );
        // Read again for confirmation
        assert_eq!(palette.entry_state(0), EntryState::Read);
        palette.update_palette(&out, t0 + Duration::from_millis(30));
        assert_eq!(sysex_calls(&backend), vec![vec![0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x04, 0x00, 0xF7]]);
        palette.handle_color_palette_message(&entry_message(0, [0, 0, 0, 0x123]));
        assert_eq!(palette.entry_state(0), EntryState::Done);

        // Entry 1 (0x1E 0x1E 0x1E) is requested next; a DAW color entry is written with its RGB
        palette.update_palette(&out, t0 + Duration::from_millis(40));
        assert_eq!(sysex_calls(&backend), vec![vec![0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x04, 0x01, 0xF7]]);
        palette.handle_color_palette_message(&entry_message(81, [0, 0, 0, 0]));
        assert_eq!(palette.entry_state(81), EntryState::Write);
        let [r, g, b] = palette_color_rgb(81);
        assert_eq!([r, g, b], [217, 46, 36]);

        answer_all_matching(&mut palette, 1);
        assert_eq!(palette.update_palette(&out, t0 + Duration::from_millis(50)), PaletteUpdate::Reapply);
        assert!(sysex_calls(&backend).is_empty());
        ColorPalette::send_reapply(&out);
        assert_eq!(sysex_calls(&backend), vec![vec![0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x05, 0xF7]]);
    }

    #[test]
    fn nothing_to_write() {
        let (out, _backend) = output();
        let mut palette = ColorPalette::new();
        answer_all_matching(&mut palette, 0);
        assert_eq!(palette.update_palette(&out, Instant::now()), PaletteUpdate::Done);
    }

    #[test]
    fn invalid_messages_are_ignored() {
        let mut palette = ColorPalette::new();
        let mut data = entry_message(0, [1, 1, 1, 0]);
        data[6] = 0x03;
        palette.handle_color_palette_message(&data);
        palette.handle_color_palette_message(&[0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x04, 0x00, 0xF7]);
        assert_eq!(palette.entry_state(0), EntryState::Read);
    }

    #[test]
    fn read_retries() {
        let (out, backend) = output();
        let mut palette = ColorPalette::new();
        let mut t = Instant::now();
        for _ in 0..MAX_NUMBER_OF_RETRIES {
            palette.update_palette(&out, t);
            // Not repeated within a second
            palette.update_palette(&out, t + Duration::from_millis(500));
            t += Duration::from_millis(1001);
        }
        assert_eq!(sysex_calls(&backend).len(), MAX_NUMBER_OF_RETRIES as usize);
        assert!(palette.take_errors().is_empty());

        // The 11th attempt fails, the entry is given up
        palette.update_palette(&out, t);
        assert!(sysex_calls(&backend).is_empty());
        assert_eq!(palette.entry_state(0), EntryState::Done);
        assert_eq!(palette.take_errors(), vec!["Failed reading color palette entry #0.".to_string()]);
    }
}
