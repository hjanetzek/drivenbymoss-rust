// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The displays of the MCU protocol (former `MCUDisplay`, `MCUSegmentDisplay` and
//! `MCUAssignmentDisplay`).
//!
//! Differences to Java:
//! - `framework::display::McuDisplay` has fixed options; the Java `MCUDisplay` reads the main
//!   and second display types from the configuration at each write and supports switching the
//!   inserted space. Therefore this writer reads its options from a shared [`DisplayOptions`],
//!   which the setup updates when the settings change.
//! - The lines are sent synchronously (no latest-task executor), only ASCII is sent.

use std::cell::Cell;
use std::rc::Rc;

use crate::framework::display::{pad_into, to_ascii_bytes, LineWriter, TextDisplay};
use crate::midi::MidiOutput;

const SYSEX_DISPLAY_HEADER_MAIN: &str = "F0 00 00 66 14 ";
const SYSEX_DISPLAY_HEADER1_MAIN: &str = "F0 00 00 66 14 12 ";
const SYSEX_DISPLAY_HEADER1_EXTENDER: &str = "F0 00 00 66 15 12 ";
const SYSEX_DISPLAY_HEADER2: &str = "F0 00 00 67 15 13 ";

/// The options of the MCU displays which depend on the configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayOptions {
    /// The main display is an Asparion display (sends only the changed part of a line).
    pub is_asparion: bool,
    /// The second display is a QCon display (with a 9th master cell).
    pub is_qcon_second_display: bool,
    /// Insert a space after each cell (not for the Mackie 7 character display).
    pub insert_space: bool,
}

impl Default for DisplayOptions {
    fn default() -> Self {
        DisplayOptions { is_asparion: false, is_qcon_second_display: false, insert_space: true }
    }
}

/// The options shared by all displays of an instance.
pub type SharedDisplayOptions = Rc<Cell<DisplayOptions>>;

/// The line writer of the two text displays (former `MCUDisplay`).
pub struct McuDisplayWriter {
    is_first_display: bool,
    is_extender: bool,
    is_main_device: bool,
    options: SharedDisplayOptions,
}

/// Create a text display: 2 rows with 8 cells (9 for the second display of the main device) of
/// 7 characters.
///
/// - `is_first`: True if it is the first display, otherwise the second
/// - `is_mcu_extender`: True if it is an original Mackie extender
/// - `is_main_device`: True if this is the main device
pub fn create_text_display(output: MidiOutput, is_first: bool, is_mcu_extender: bool, is_main_device: bool, options: SharedDisplayOptions) -> TextDisplay {
    let writer = McuDisplayWriter { is_first_display: is_first, is_extender: is_mcu_extender, is_main_device, options };
    let cells = if !is_first && is_main_device { 9 } else { 8 };
    let mut display = TextDisplay::new(output, Box::new(writer), 2, cells, 7 * 8);
    display.set_center_notification(false);
    display
}

impl McuDisplayWriter {
    fn is_short_second_display(&self) -> bool {
        !self.is_first_display && self.is_main_device && self.options.get().is_qcon_second_display
    }

    fn header(&self, row: usize, offset: usize) -> String {
        let mut header = String::new();
        let is_asparion = self.options.get().is_asparion;
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
        header
    }
}

impl LineWriter for McuDisplayWriter {
    fn format_cell(&self, value: &str, characters_of_cell: usize, out: &mut String) {
        if self.options.get().insert_space {
            pad_into(out, value, characters_of_cell.saturating_sub(1));
            out.push(' ');
        } else {
            pad_into(out, value, characters_of_cell)
        }
    }

    fn transform_line(&self, row: usize, text: &str) -> String {
        if self.is_short_second_display() {
            // If a 9th master cell should be added
            let mut t = text.to_string();
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
        if self.options.get().is_asparion {
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

/// The 7-segment display of the position (former `MCUSegmentDisplay`): 1 row of 20 characters,
/// only digits and ':'.
pub struct SegmentDisplayWriter {
    transport_buffer: [i32; 10],
}

/// Create the segment display.
pub fn create_segment_display(output: MidiOutput) -> TextDisplay {
    TextDisplay::new(output, Box::new(SegmentDisplayWriter { transport_buffer: [0; 10] }), 1, 1, 20)
}

impl LineWriter for SegmentDisplayWriter {
    fn write_line(&mut self, output: &MidiOutput, _row: usize, text: &str, _previous_text: Option<&str>) {
        // Sets the position string. Must only contain numbers and ':'
        let chars: Vec<char> = text.chars().collect();
        let mut add_dot = false;
        let mut pos = chars.len() as i32 - 1;
        let mut i = 0;
        while i < 10 {
            let mut c = 0x20;
            if pos >= 0 {
                let single_digit = chars[pos as usize];
                pos -= 1;
                if single_digit == ':' {
                    add_dot = true;
                    continue;
                }
                c = single_digit as i32;
                if add_dot {
                    c += 0x40;
                }
            }
            if c != self.transport_buffer[i] {
                output.send_cc(0x40 + i as i32, c);
                self.transport_buffer[i] = c;
            }
            i += 1;
            add_dot = false;
        }
    }
}

/// Clear the 7-digit displays (former `MCUSegmentDisplay.shutdown`).
pub fn shutdown_segment_display(output: &MidiOutput) {
    for i in 0..12 {
        output.send_cc(0x40 + i, 0x20);
    }
}

/// The 2 character display of the mode (former `MCUAssignmentDisplay`).
pub struct AssignmentDisplayWriter {
    assignment_buffer: [i32; 2],
}

/// Create the assignment display.
pub fn create_assignment_display(output: MidiOutput) -> TextDisplay {
    TextDisplay::new(output, Box::new(AssignmentDisplayWriter { assignment_buffer: [0; 2] }), 1, 1, 2)
}

impl LineWriter for AssignmentDisplayWriter {
    fn write_line(&mut self, output: &MidiOutput, _row: usize, text: &str, _previous_text: Option<&str>) {
        // Sets the assignment (mode) string. Must only contain 2 upper case letters
        let chars: Vec<char> = text.chars().collect();
        for i in 0..2 {
            let c = chars.get(i).copied().unwrap_or(' ') as i32;
            if self.assignment_buffer[i] != c {
                let value = if c >= 0x40 { c - 0x40 } else { c };
                output.send_cc(0x4B - i as i32, value);
                self.assignment_buffer[i] = c;
            }
        }
    }
}

/// Clear the assignment display (former `MCUAssignmentDisplay.shutdown`, which writes "  "
/// with the current buffer).
pub fn shutdown_assignment_display(display: &mut TextDisplay) {
    display.set_row(0, "  ").all_done();
    display.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};

    fn output() -> (MidiOutput, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        (MidiOutput::new(Sender::new(backend.clone()), Some(2)), backend)
    }

    fn sent(backend: &RecordingBackend) -> Vec<Vec<u8>> {
        backend.take_calls().into_iter().filter_map(|c| if let BackendCall::SendMidi(_, d) = c { Some(d) } else { None }).collect()
    }

    #[test]
    fn text_display_options() {
        let (output, backend) = output();
        let options: SharedDisplayOptions = Rc::new(Cell::new(DisplayOptions::default()));
        let mut display = create_text_display(output, true, true, false, options.clone());
        display.set_cell(0, 0, "Volume").all_done().flush();
        let calls = sent(&backend);
        assert_eq!(&calls[0][..7], &[0xF0, 0x00, 0x00, 0x66, 0x15, 0x12, 0x00]);
        assert_eq!(&calls[0][7..14], b"Volume ");

        options.set(DisplayOptions { is_asparion: true, is_qcon_second_display: false, insert_space: false });
        display.set_cell(0, 0, "Panning").all_done().flush();
        let calls = sent(&backend);
        // Asparion: only the changed part of the first row
        assert_eq!(&calls[0][..8], &[0xF0, 0x00, 0x00, 0x66, 0x14, 0x1A, 0x00, 0x01]);
        assert_eq!(&calls[0][8..15], b"Panning");
    }

    #[test]
    fn qcon_second_display() {
        let (output, backend) = output();
        let options: SharedDisplayOptions = Rc::new(Cell::new(DisplayOptions { is_qcon_second_display: true, ..DisplayOptions::default() }));
        let mut display = create_text_display(output, false, false, true, options);
        assert_eq!(display.number_of_cells(), 9);
        display.set_cell(0, 8, "Maste").all_done().flush();
        let calls = sent(&backend);
        assert_eq!(&calls[0][..7], &[0xF0, 0x00, 0x00, 0x67, 0x15, 0x13, 0x00]);
        // 2 spaces in front, the last character is replaced by 'r'
        let text = &calls[0][7..calls[0].len() - 1];
        assert_eq!(&text[..2], b"  ");
        assert_eq!(text[text.len() - 1], b'r');
    }

    #[test]
    fn segment_and_assignment() {
        let (output, backend) = output();
        let mut segment = create_segment_display(output.clone());
        segment.set_row(0, "1:02").all_done().flush();
        let calls = sent(&backend);
        // From the right: '2', '0', '1' with the dot of the ':', then spaces
        assert_eq!(calls[0], vec![0xB0, 0x40, b'2']);
        assert_eq!(calls[1], vec![0xB0, 0x41, b'0']);
        assert_eq!(calls[2], vec![0xB0, 0x42, b'1' + 0x40]);
        assert_eq!(calls.len(), 10);
        segment.set_row(0, "1:03").all_done().flush();
        assert_eq!(sent(&backend), vec![vec![0xB0, 0x40, b'3']]);

        let mut assignment = create_assignment_display(output);
        assignment.set_row(0, "VL").all_done().flush();
        assert_eq!(sent(&backend), vec![vec![0xB0, 0x4B, b'V' - 0x40], vec![0xB0, 0x4A, b'L' - 0x40]]);
        shutdown_assignment_display(&mut assignment);
        assert_eq!(sent(&backend), vec![vec![0xB0, 0x4B, 0x20], vec![0xB0, 0x4A, 0x20]]);
    }
}
