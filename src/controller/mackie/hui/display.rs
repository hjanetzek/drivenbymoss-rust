// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The displays of the HUI protocol (former `HUIDisplay` and `HUISegmentDisplay`). Difference to
//! Java: the display lines are sent synchronously (no latest-task executor), only ASCII.

use crate::framework::display::{to_ascii_bytes, LineWriter, TextDisplay};
use crate::midi::MidiOutput;

const SYSEX_DISPLAY_HEADER: [u8; 7] = [0xF0, 0x00, 0x00, 0x66, 0x05, 0x00, 0x10];
const SYSEX_SEGMENT_HEADER: [u8; 7] = [0xF0, 0x00, 0x00, 0x66, 0x05, 0x00, 0x11];

/// The text display: 1 row with 9 cells of 4 characters (former `HUIDisplay`).
pub struct HuiDisplayWriter;

/// Create the text display.
pub fn create_text_display(output: MidiOutput) -> TextDisplay {
    let mut display = TextDisplay::new(output, Box::new(HuiDisplayWriter), 1, 9, 36);
    display.set_center_notification(false);
    display
}

impl LineWriter for HuiDisplayWriter {
    fn write_line(&mut self, output: &MidiOutput, _row: usize, text: &str, _previous_text: Option<&str>) {
        let bytes = to_ascii_bytes(text);
        for cell in 0..9 {
            let mut msg = SYSEX_DISPLAY_HEADER.to_vec();
            msg.push(cell as u8);
            for i in 0..4 {
                msg.push(bytes.get(cell * 4 + i).copied().unwrap_or(b' '));
            }
            msg.push(0xF7);
            output.send_sysex(&msg);
        }
    }
}

/// The position display of 8 digits (former `HUISegmentDisplay`).
pub struct HuiSegmentDisplayWriter {
    old_transport_buffer: [i32; 8],
}

/// Create the segment display.
pub fn create_segment_display(output: MidiOutput) -> TextDisplay {
    TextDisplay::new(output, Box::new(HuiSegmentDisplayWriter { old_transport_buffer: [-1; 8] }), 1, 1, 8)
}

impl LineWriter for HuiSegmentDisplayWriter {
    fn write_line(&mut self, output: &MidiOutput, _row: usize, text: &str, _previous_text: Option<&str>) {
        // Sets the position string. Must only contain numbers and ':'
        let lower_text = text.to_lowercase();
        let mut transport_buffer = [0i32; 8];
        // Convert string to display character codes
        let mut index = 0;
        for c in lower_text.chars().rev() {
            // Set a dot
            if c == ':' {
                transport_buffer[index] += 0x10;
            } else {
                let value = c.to_digit(10).map_or(0, |d| d as i32);
                transport_buffer[index] += value;
                index += 1;
                if index >= 8 {
                    break;
                }
            }
        }

        // Lookup number of changed digits
        let mut pos: i32 = 7;
        for i in (0..8).rev() {
            if transport_buffer[i] == self.old_transport_buffer[i] {
                pos -= 1;
            } else {
                break;
            }
        }
        // Nothing has changed
        if pos == -1 {
            return;
        }
        // Store the changes
        let count = pos as usize + 1;
        self.old_transport_buffer[..count].copy_from_slice(&transport_buffer[..count]);
        // Create and send the message with changed digits
        let mut msg = SYSEX_SEGMENT_HEADER.to_vec();
        msg.extend(transport_buffer[..count].iter().map(|v| *v as u8));
        msg.push(0xF7);
        output.send_sysex(&msg);
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};

    fn sent(backend: &RecordingBackend) -> Vec<Vec<u8>> {
        backend.take_calls().into_iter().filter_map(|c| if let BackendCall::SendMidi(_, d) = c { Some(d) } else { None }).collect()
    }

    #[test]
    fn displays() {
        let backend = Rc::new(RecordingBackend::default());
        let output = MidiOutput::new(Sender::new(backend.clone()), Some(1));
        let mut display = create_text_display(output.clone());
        display.set_cell(0, 0, "Drums").set_cell(0, 8, "Volu").all_done().flush();
        let calls = sent(&backend);
        assert_eq!(calls.len(), 9);
        assert_eq!(calls[0], vec![0xF0, 0x00, 0x00, 0x66, 0x05, 0x00, 0x10, 0, b'D', b'r', b'u', b'm', 0xF7]);
        assert_eq!(&calls[8][7..12], &[8, b'V', b'o', b'l', b'u']);

        let mut segment = create_segment_display(output);
        segment.set_row(0, "1:02").all_done().flush();
        // From the right: 2, 0, 1 with the dot of the ':', then zeros
        assert_eq!(sent(&backend), vec![vec![0xF0, 0x00, 0x00, 0x66, 0x05, 0x00, 0x11, 2, 0, 0x11, 0, 0, 0, 0, 0, 0xF7]]);
        segment.set_row(0, "1:03").all_done().flush();
        assert_eq!(sent(&backend), vec![vec![0xF0, 0x00, 0x00, 0x66, 0x05, 0x00, 0x11, 3, 0xF7]]);
    }
}
