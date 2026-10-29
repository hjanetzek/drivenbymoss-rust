// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Text displays (former `ITextDisplay`, `AbstractTextDisplay`, `Format` and the MCU protocol
//! display `MCUDisplay`, which is also used by the Maschine Mk2/Mk3/Studio/Plus).
//!
//! Differences to Java:
//! - The display is stored in [`crate::framework::ctx::Ctx::text_display`]. Notifications are
//!   shown with [`crate::framework::ctx::Ctx::notify`], which also schedules the timer that
//!   removes the notification after [`NOTIFICATION_TIME`] ms.
//! - The hardware display proxy of the simulator GUI is not ported.
//! - The lines are sent synchronously (the Java MCU display uses a latest-task executor).
//! - Only ASCII characters are sent to the device, all other characters are replaced by '?'
//!   (Java sends the bytes of the platform encoding, which breaks the sysex message).

use crate::midi::MidiOutput;

/// Time to keep a notification displayed in milliseconds.
pub const NOTIFICATION_TIME: i32 = 700;

/// The interval in which the notification timeout is checked.
pub const NOTIFICATION_WATCH_INTERVAL: i32 = 100;

/// Different options how to format values (former `Format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// No formatting.
    Raw,
    /// Format it as a value.
    Value,
    /// Format it as a panning value.
    Pan,
}

/// Pad the given text with spaces until it reaches the given length or cut it (former
/// `StringUtils.pad`). The length is counted in characters.
pub fn pad(text: &str, length: usize) -> String {
    let count = text.chars().count();
    if count >= length {
        return text.chars().take(length).collect();
    }
    let mut result = String::with_capacity(length);
    result.push_str(text);
    result.extend(std::iter::repeat_n(' ', length - count));
    result
}

/// Sends the lines of a text display to the hardware. This is the device specific part of a
/// text display (former abstract methods of `AbstractTextDisplay`).
pub trait LineWriter {
    /// Send a row to the display (former `writeLine`).
    fn write_line(&mut self, output: &MidiOutput, row: usize, text: &str, previous_text: Option<&str>);

    /// Format the content of a cell (former `setCell`), the default pads the text to the length
    /// of the cell.
    fn format_cell(&self, value: &str, characters_of_cell: usize) -> String {
        pad(value, characters_of_cell)
    }

    /// Modify the text of a line before it is written (former override of `updateLine`).
    fn transform_line(&self, _row: usize, text: &str) -> String {
        text.to_string()
    }
}

/// A text display with a number of lines which are split into cells (former
/// `AbstractTextDisplay`).
pub struct TextDisplay {
    output: MidiOutput,
    writer: Box<dyn LineWriter>,

    no_of_lines: usize,
    no_of_cells: usize,
    no_of_characters: usize,
    characters_of_cell: usize,

    empty_line: String,
    notification_message: String,
    center_notification: bool,
    notification_timeout: i32,

    empty_cell: String,
    current_message: Vec<Option<String>>,
    message: Vec<Option<String>>,
    full_rows: Vec<Option<String>>,
    cells: Vec<String>,

    is_shutdown: bool,
}

impl TextDisplay {
    /// Constructor.
    ///
    /// - `no_of_lines`: The number of rows that the display supports
    /// - `no_of_cells`: The number of cells that the display supports
    /// - `no_of_characters`: The number of characters of 1 row that the display supports
    pub fn new(output: MidiOutput, writer: Box<dyn LineWriter>, no_of_lines: usize, no_of_cells: usize, no_of_characters: usize) -> Self {
        let mut display = TextDisplay {
            output,
            writer,
            no_of_lines: 0,
            no_of_cells: 0,
            no_of_characters: 0,
            characters_of_cell: 0,
            empty_line: String::new(),
            notification_message: String::new(),
            center_notification: true,
            notification_timeout: 0,
            empty_cell: String::new(),
            current_message: Vec::new(),
            message: Vec::new(),
            full_rows: Vec::new(),
            cells: Vec::new(),
            is_shutdown: false,
        };
        display.set_number_of_cells(no_of_lines, no_of_cells, no_of_characters);
        display
    }

    /// Set the line and cell numbers.
    pub fn set_number_of_cells(&mut self, no_of_lines: usize, no_of_cells: usize, no_of_characters: usize) {
        self.no_of_lines = no_of_lines;
        self.no_of_cells = no_of_cells;
        self.no_of_characters = no_of_characters;
        self.characters_of_cell = no_of_characters / no_of_cells.max(1);
        self.empty_cell = " ".repeat(self.characters_of_cell);
        self.empty_line = " ".repeat(no_of_characters);
        self.notification_message = self.empty_line.clone();
        self.current_message = vec![None; no_of_lines];
        self.message = vec![None; no_of_lines];
        self.full_rows = vec![None; no_of_lines];
        self.cells = vec![self.empty_cell.clone(); no_of_lines * no_of_cells];
    }

    pub fn number_of_lines(&self) -> usize {
        self.no_of_lines
    }

    pub fn number_of_cells(&self) -> usize {
        self.no_of_cells
    }

    /// Note: same as in Java this returns the number of characters of a row.
    pub fn number_of_cell_characters(&self) -> usize {
        self.no_of_characters
    }

    pub fn characters_of_cell(&self) -> usize {
        self.characters_of_cell
    }

    /// Set if notification messages should be centered in the display.
    pub fn set_center_notification(&mut self, center_notification: bool) {
        self.center_notification = center_notification;
    }

    /// Set a whole row. The text must match the exact number of characters of a row.
    pub fn set_row(&mut self, row: usize, text: &str) -> &mut Self {
        if row < self.no_of_lines {
            self.full_rows[row] = Some(text.to_string());
        }
        self
    }

    pub fn clear(&mut self) -> &mut Self {
        for row in 0..self.no_of_lines {
            self.clear_row(row);
        }
        self
    }

    pub fn clear_row(&mut self, row: usize) -> &mut Self {
        for column in 0..self.no_of_cells {
            self.clear_cell(row, column);
        }
        self
    }

    pub fn clear_block(&mut self, row: usize, block: usize) -> &mut Self {
        let cell = 2 * block;
        self.clear_cell(row, cell);
        self.clear_cell(row, cell + 1);
        self
    }

    pub fn clear_column(&mut self, column: usize) -> &mut Self {
        for row in 0..self.no_of_lines {
            self.clear_cell(row, column);
        }
        self
    }

    /// Signals that editing of a row is finished and the full text for the row can be created.
    pub fn done(&mut self, row: usize) -> &mut Self {
        if row >= self.no_of_lines {
            return self;
        }
        if let Some(full_row) = self.full_rows[row].take() {
            self.message[row] = Some(full_row);
        } else {
            let index = row * self.no_of_cells;
            self.message[row] = Some(self.cells[index..index + self.no_of_cells].concat());
        }
        self
    }

    /// Marks editing as finished and create the text for all rows.
    pub fn all_done(&mut self) -> &mut Self {
        for row in 0..self.no_of_lines {
            self.done(row);
        }
        self
    }

    pub fn clear_cell(&mut self, row: usize, column: usize) -> &mut Self {
        if let Some(cell) = self.cell_index(row, column) {
            self.cells[cell] = self.empty_cell.clone();
        }
        self
    }

    /// Set a cell with a number (former `setCell (int, int, int, Format)`).
    pub fn set_cell_value(&mut self, row: usize, column: usize, value: i32, _format: Format) -> &mut Self {
        self.set_cell(row, column, &value.to_string())
    }

    pub fn set_cell(&mut self, row: usize, column: usize, value: &str) -> &mut Self {
        match self.cell_index(row, column) {
            Some(cell) => self.cells[cell] = self.writer.format_cell(value, self.characters_of_cell),
            None => debug_assert!(false, "Display array index out of bounds."),
        }
        self
    }

    /// Set a block (2 cells).
    pub fn set_block(&mut self, row: usize, block: usize, value: &str) -> &mut Self {
        let cell = 2 * block;
        let chars = self.characters_of_cell;
        if value.chars().count() >= chars {
            let (Some(first), Some(second)) = (self.cell_index(row, cell), self.cell_index(row, cell + 1)) else {
                return self;
            };
            let head: String = value.chars().take(chars).collect();
            let tail: String = value.chars().skip(chars).collect();
            self.cells[first] = pad(&head, chars);
            self.cells[second] = pad(&tail, chars);
        } else {
            self.set_cell(row, cell, value);
            self.clear_cell(row, cell + 1);
        }
        self
    }

    fn cell_index(&self, row: usize, column: usize) -> Option<usize> {
        if row < self.no_of_lines && column < self.no_of_cells {
            Some(row * self.no_of_cells + column)
        } else {
            None
        }
    }

    /// Show a notification message on the display (former `notify`). Returns true if the timer
    /// which removes the notification needs to be started (see [`TextDisplay::watch`]).
    pub fn notify(&mut self, message: &str) -> bool {
        let mut msg = String::new();
        if self.center_notification {
            let length = message.chars().count() as i32;
            let pad_length = (self.no_of_characters as i32 - length) / 2 + 1;
            let padding = if pad_length > 0 { " ".repeat(pad_length as usize) } else { String::new() };
            msg.push_str(&padding);
            msg.push_str(message);
            msg.push_str(&padding);
        } else {
            msg.push_str(message);
        }
        // Pad enough spaces at the end to fill all lines...
        for _ in 0..self.no_of_lines {
            msg.push_str(&self.empty_line);
        }
        self.notification_message = msg;

        let is_running = self.notification_timeout > 0;
        self.notification_timeout = NOTIFICATION_TIME;
        self.flush();
        !is_running
    }

    /// Decrease the notification timeout (called every [`NOTIFICATION_WATCH_INTERVAL`] ms).
    /// Returns true if the timer needs to continue.
    pub fn watch(&mut self) -> bool {
        self.notification_timeout -= NOTIFICATION_WATCH_INTERVAL;
        if self.notification_timeout <= 0 {
            self.force_flush();
            return false;
        }
        true
    }

    pub fn cancel_notification(&mut self) {
        self.notification_timeout = 0;
    }

    pub fn is_notification_active(&self) -> bool {
        self.notification_timeout > 0
    }

    /// Flushes (only) the changed texts of all rows.
    pub fn flush(&mut self) {
        for row in 0..self.no_of_lines {
            // Has anything changed?
            if self.current_message[row].is_some() && self.current_message[row] == self.message[row] {
                continue;
            }
            let previous = self.current_message[row].take();
            self.current_message[row] = self.message[row].clone();
            if let Some(text) = self.current_message[row].clone() {
                self.update_line(row, &text, previous.as_deref());
            }
        }

        if self.notification_timeout > 0 {
            self.update_notification();
        }
    }

    /// Updates the display with the notification.
    fn update_notification(&mut self) {
        let chars: Vec<char> = self.notification_message.chars().collect();
        for row in 0..self.no_of_lines {
            let pos = row * self.no_of_characters;
            let text: String = if pos < chars.len() { chars[pos..chars.len().min(pos + self.no_of_characters)].iter().collect() } else { String::new() };
            let previous = self.current_message[row].clone();
            self.update_line(row, &pad(&text, self.no_of_characters), previous.as_deref());
        }
    }

    /// Update the line on the hardware.
    fn update_line(&mut self, row: usize, text: &str, previous_text: Option<&str>) {
        if self.is_shutdown {
            return;
        }
        let text = self.writer.transform_line(row, text);
        self.writer.write_line(&self.output, row, &text, previous_text);
    }

    /// Forces the recreation of all row texts. The next call to flush will then send all rows.
    pub fn force_flush(&mut self) {
        for row in 0..self.no_of_lines {
            self.current_message[row] = Some(String::new());
        }
    }

    /// Get the text of a row which was last sent to the device (for tests).
    pub fn current_line(&self, row: usize) -> Option<&str> {
        self.current_message.get(row).and_then(|m| m.as_deref())
    }

    /// Clear the display and prevent further sends.
    pub fn shutdown(&mut self) {
        if self.is_shutdown {
            return;
        }
        self.clear().all_done();
        self.flush();
        self.is_shutdown = true;
    }
}

/// Convert a text to the bytes sent to a display: non-ASCII characters are replaced by '?'.
pub fn to_ascii_bytes(text: &str) -> Vec<u8> {
    text.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).collect()
}

const SYSEX_DISPLAY_HEADER: &str = "F0 00 00 66 ";
const SYSEX_DISPLAY_HEADER_MAIN: &str = "F0 00 00 66 14 ";
const SYSEX_DISPLAY_HEADER1_MAIN: &str = "F0 00 00 66 14 12 ";
const SYSEX_DISPLAY_HEADER1_EXTENDER: &str = "F0 00 00 66 15 12 ";
const SYSEX_DISPLAY_HEADER2: &str = "F0 00 00 67 15 13 ";

/// The options of a MCU display which depend on the MCU configuration (main display type
/// Asparion, second display type QCon).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct McuDisplayOptions {
    /// The main display is an Asparion display (sends only the changed part of a line).
    pub is_asparion: bool,
    /// The second display is a QCon display (with a 9th master cell).
    pub is_qcon_second_display: bool,
}

/// The MCU protocol display: 2 rows with 8 cells of 7 characters (former `MCUDisplay`).
pub struct McuDisplay {
    is_first_display: bool,
    is_extender: bool,
    is_main_device: bool,
    insert_space: bool,
    options: McuDisplayOptions,
}

impl McuDisplay {
    /// Create the text display for a MCU display. 2 rows (0-1) with 4 blocks (0-3). Each block
    /// consists of 18 characters or 2 cells (0-8).
    ///
    /// - `is_first`: True if it is the first display, otherwise the second
    /// - `is_mcu_extender`: True if it is an original Mackie extender
    /// - `is_main_device`: True if this is the main device
    pub fn create(output: MidiOutput, is_first: bool, is_mcu_extender: bool, is_main_device: bool, options: McuDisplayOptions) -> TextDisplay {
        let writer = McuDisplay { is_first_display: is_first, is_extender: is_mcu_extender, is_main_device, insert_space: true, options };
        let cells = if !is_first && is_main_device && options.is_qcon_second_display { 9 } else { 8 };
        let mut display = TextDisplay::new(output, Box::new(writer), 2, cells, 7 * 8);
        display.set_center_notification(false);
        display
    }

    fn header(&self, row: usize, offset: usize) -> String {
        let mut header = String::new();
        let is_asparion = self.options.is_asparion;
        if self.is_first_display {
            if is_asparion {
                header.push_str(SYSEX_DISPLAY_HEADER_MAIN);
                header.push_str(&format!("1A {offset:02X} {}", if row == 0 { "01 " } else { "02 " }));
            } else {
                header.push_str(if self.is_extender { SYSEX_DISPLAY_HEADER1_EXTENDER } else { SYSEX_DISPLAY_HEADER1_MAIN });
            }
        } else if is_asparion {
            header.push_str(SYSEX_DISPLAY_HEADER_MAIN);
            header.push_str(&format!("19 {offset:02X} "));
        } else {
            header.push_str(SYSEX_DISPLAY_HEADER2);
        }
        if !is_asparion {
            header.push_str(if row == 0 { "00 " } else { "38 " });
        }
        debug_assert!(header.starts_with(SYSEX_DISPLAY_HEADER));
        header
    }
}

impl LineWriter for McuDisplay {
    fn format_cell(&self, value: &str, characters_of_cell: usize) -> String {
        if self.insert_space {
            let mut content = pad(value, characters_of_cell.saturating_sub(1));
            content.push(' ');
            content
        } else {
            pad(value, characters_of_cell)
        }
    }

    fn transform_line(&self, row: usize, text: &str) -> String {
        if !self.is_first_display && self.is_main_device && self.options.is_qcon_second_display {
            // If a 9th master cell should be added
            let mut t: String = text.to_string();
            if row == 0 && !t.is_empty() {
                t.pop();
                t.push('r');
            }
            return format!("  {t}");
        }
        text.to_string()
    }

    fn write_line(&mut self, output: &MidiOutput, row: usize, text: &str, previous_text: Option<&str>) {
        let mut offset = 0;
        let mut bytes = to_ascii_bytes(text);
        if self.options.is_asparion {
            if let Some(previous) = previous_text {
                let previous = to_ascii_bytes(previous);
                if previous.len() == bytes.len() && !bytes.is_empty() {
                    offset = (0..bytes.len()).find(|i| bytes[*i] != previous[*i]).unwrap_or(0);
                    let mut end = offset + 1;
                    for i in (offset + 1..previous.len()).rev() {
                        if bytes[i] != previous[i] {
                            end = i;
                            break;
                        }
                    }
                    let end = (end + 1).min(bytes.len());
                    bytes = bytes[offset..end].to_vec();
                }
            }
        }
        let mut sysex = crate::midi::from_hex_str(&self.header(row, offset));
        sysex.extend_from_slice(&bytes);
        sysex.push(0xF7);
        output.send_sysex(&sysex);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use std::rc::Rc;

    fn display() -> (TextDisplay, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        let output = MidiOutput::new(Sender::new(backend.clone()), Some(1));
        (McuDisplay::create(output, true, false, false, McuDisplayOptions::default()), backend)
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

    #[test]
    fn pad_and_cut() {
        assert_eq!(pad("abc", 5), "abc  ");
        assert_eq!(pad("abcdef", 3), "abc");
        assert_eq!(pad("abc", 3), "abc");
    }

    #[test]
    fn mcu_lines() {
        let (mut display, backend) = display();
        display.set_cell(0, 0, "Volume").set_cell(0, 1, "Panorama").set_block(1, 3, "Hello World!");
        display.all_done().flush();
        let calls = sysex_calls(&backend);
        assert_eq!(calls.len(), 2);
        let mut expected = vec![0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x00];
        let mut line = String::from("Volume Panora ");
        line.push_str(&" ".repeat(42));
        expected.extend_from_slice(line.as_bytes());
        expected.push(0xF7);
        assert_eq!(calls[0], expected);
        assert_eq!(&calls[1][..7], &[0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x38]);
        assert_eq!(calls[1].len(), 7 + 56 + 1);
        assert_eq!(&calls[1][7 + 42..7 + 56], b"Hello World!  ");

        // Nothing changed
        display.all_done().flush();
        assert!(sysex_calls(&backend).is_empty());
    }

    #[test]
    fn notification() {
        let (mut display, backend) = display();
        display.all_done().flush();
        backend.take_calls();
        assert!(display.notify("Undo"));
        assert!(display.is_notification_active());
        let calls = sysex_calls(&backend);
        assert_eq!(calls.len(), 2);
        assert_eq!(&calls[0][7..11], b"Undo");
        // A second notification does not start another timer
        assert!(!display.notify("Redo"));
        for _ in 0..6 {
            assert!(display.watch());
        }
        assert!(!display.watch());
        assert!(!display.is_notification_active());
        backend.take_calls();
        // The previous content is restored with the next flush
        display.flush();
        let calls = sysex_calls(&backend);
        assert_eq!(calls.len(), 2);
        assert_eq!(&calls[0][7..11], b"    ");
    }
}
