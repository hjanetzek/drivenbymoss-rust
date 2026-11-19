// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The text display of the Push 1 (former `Push1Display`): 4 rows of 68 characters, split into
//! 8 cells which alternate between 9 and 8 characters (the display has 4 blocks of 17
//! characters), and the special characters of the display (bars, arrows).

use crate::framework::display::{pad, pad_into, Format, LineWriter, TextDisplay};
use crate::midi::MidiOutput;

pub const BARS_NON: char = '\u{6}';
pub const BARS_ONE: char = '\u{3}';
pub const BARS_TWO: char = '\u{5}';
pub const BARS_ONE_L: char = '\u{4}';
pub const SELECT_ARROW: char = '\u{7F}';
pub const DEGREE: char = '\u{9}';
pub const RIGHT_ARROW: char = '\u{1E}';
pub const FOLDER: char = '\u{7}';
pub const THREE_ROWS: char = '\u{2}';
pub const DIVISION: char = '\u{18}';

const NON_4: [char; 4] = [BARS_NON; 4];

/// The sysex headers of the 4 rows.
const SYSEX_MESSAGE: [[u8; 8]; 4] = [
    [0xF0, 0x47, 0x7F, 0x15, 0x18, 0x00, 0x45, 0x00],
    [0xF0, 0x47, 0x7F, 0x15, 0x19, 0x00, 0x45, 0x00],
    [0xF0, 0x47, 0x7F, 0x15, 0x1A, 0x00, 0x45, 0x00],
    [0xF0, 0x47, 0x7F, 0x15, 0x1B, 0x00, 0x45, 0x00],
];

/// The length of a block of the notification.
const BLOCK_LENGTH: usize = 17;

/// Writes the rows of the Push 1 display as sysex (former `writeLine` and
/// `convertCharacterset`).
pub struct Push1LineWriter;

impl LineWriter for Push1LineWriter {
    fn write_line(&mut self, output: &MidiOutput, row: usize, text: &str, _previous_text: Option<&str>) {
        let Some(header) = SYSEX_MESSAGE.get(row) else {
            return;
        };
        let mut data = header.to_vec();
        // Former StringUtils.asciiToHex: the low byte of each character
        data.extend(text.chars().map(|c| (c as u32 & 0xFF) as u8));
        data.push(0xF7);
        output.send_sysex(&data);
    }

    /// Former `setCell`: pad to 8 characters, plus a space for the even cells.
    fn format_cell(&self, value: &str, characters_of_cell: usize, out: &mut String) {
        pad_into(out, value, 8);
        if characters_of_cell == 9 {
            out.push(' ');
        }
    }

    /// The even cells have 9 characters, the odd ones 8 (former `clearCell` and `setBlock`).
    fn cell_width(&self, column: usize, _characters_of_cell: usize) -> usize {
        if column.is_multiple_of(2) {
            9
        } else {
            8
        }
    }

    /// Former `convertCharacterset`: the special characters which the display does not have
    /// and a space after each block of 17 characters.
    fn transform_line(&self, _row: usize, text: &str) -> String {
        let mut converted: Vec<char> = text
            .chars()
            .map(|c| match c {
                BARS_NON => '-',
                BARS_ONE | BARS_TWO | BARS_ONE_L => '|',
                SELECT_ARROW | RIGHT_ARROW => '>',
                c => c,
            })
            .collect();
        // The cells are only 9/8 characters wide after they were cleared once
        converted.resize(4 * BLOCK_LENGTH, ' ');
        let mut result = String::with_capacity(72);
        for i in 0..4 {
            let start = (i * BLOCK_LENGTH).min(converted.len());
            let end = ((i + 1) * BLOCK_LENGTH).min(converted.len());
            result.extend(&converted[start..end]);
            if i != 3 {
                result.push(' ');
            }
        }
        result
    }
}

/// Create the text display of the Push 1.
pub fn create(output: MidiOutput) -> TextDisplay {
    let mut display = TextDisplay::new(output, Box::new(Push1LineWriter), 4, 8, 68);
    display.set_short_notification_block(Some(BLOCK_LENGTH));
    display.clear();
    display
}

/// The text of the display when the host is shut down (former `shutdown`).
pub fn shutdown(display: &mut TextDisplay, host_name: &str) {
    display.clear().set_block(1, 1, "     Please start").set_block(1, 2, &format!("{host_name} to play...")).all_done().flush();
}

/// Format a value (former `formatStr`).
pub fn format_str(value: i32, format: Format, max_param: i32) -> String {
    match format {
        Format::Value => format_value(value, max_param),
        Format::Pan => format_pan(value, max_param),
        _ => value.to_string(),
    }
}

/// Set a cell with a formatted value (former `setCell (row, cell, value, format)`).
pub fn set_cell_value(display: &mut TextDisplay, row: usize, cell: usize, value: i32, format: Format, max_param: i32) {
    display.set_cell(row, cell, &format_str(value, format, max_param));
}

fn fill_fields(value: i32, max_param: i32) -> [char; 8] {
    let mut fields = [BARS_NON; 8];
    let no_of_bars = (16.0 * value as f64 / max_param as f64).round() as i32;
    let mut count = 0;
    while count < no_of_bars / 2 && count < 8 {
        fields[count as usize] = BARS_TWO;
        count += 1;
    }
    if no_of_bars % 2 == 1 && count < 8 {
        fields[count as usize] = BARS_ONE;
    }
    fields
}

/// A value as a bar of 8 characters (former `formatValue (value, maxParam)`).
pub fn format_value(value: i32, max_param: i32) -> String {
    fill_fields(value, max_param).iter().collect()
}

/// A value as a bar of 8 characters with the modulated value (former `formatValue (modulated,
/// value, maxParam)`).
pub fn format_value_modulated(modulated: i32, value: i32, max_param: i32) -> String {
    let mut fields = fill_fields(value, max_param);
    let no_of_bars_modulated = (16.0 * modulated as f64 / max_param as f64).round() as i32;
    let pos = (no_of_bars_modulated / 2).clamp(0, 7) as usize;
    if no_of_bars_modulated % 2 == 0 {
        if pos > 0 {
            if fields[pos - 1] == BARS_NON {
                fields[pos - 1] = BARS_ONE_L;
            } else if fields[pos] == BARS_ONE {
                fields[pos - 1] = BARS_TWO;
            }
        }
    } else if fields[pos] == BARS_NON {
        fields[pos] = BARS_ONE;
    } else {
        fields[pos] = BARS_TWO;
    }
    fields.iter().collect()
}

/// A panorama value as a bar of 8 characters, centered (former `formatPan`).
pub fn format_pan(pan: i32, max_param: i32) -> String {
    let middle = max_param / 2;
    if pan == middle {
        return NON_4.iter().chain(NON_4.iter()).collect();
    }
    let is_left = pan < middle;
    let pos = if is_left { middle - pan } else { pan - middle };
    let no_of_bars = 16 * pos / max_param;
    let mut n = String::new();
    for _ in 0..no_of_bars / 2 {
        n.push(BARS_TWO);
    }
    if pan >= max_param - 1 {
        n.push(BARS_TWO);
    } else if no_of_bars % 2 == 1 {
        n.push(if is_left { BARS_ONE_L } else { BARS_ONE });
    }
    // Former StringUtils.pad (n, 4, BARS_NON)
    let mut padded: String = n.chars().take(4).collect();
    while padded.chars().count() < 4 {
        padded.push(BARS_NON);
    }
    let result: String = NON_4.iter().collect::<String>() + &padded;
    if is_left {
        result.chars().rev().collect()
    } else {
        result
    }
}

/// The items of a menu with `display_size` entries around the selected one (former
/// `createMenuList`).
pub fn create_menu_list(display_size: usize, elements: &[String], selected_index: i32) -> Vec<(String, bool)> {
    let start_index = selected_index.clamp(0, (elements.len() as i32 - display_size as i32).max(0));
    (0..display_size)
        .map(|i| {
            let pos = start_index + i as i32;
            (elements.get(pos as usize).cloned().unwrap_or_default(), pos == selected_index)
        })
        .collect()
}

/// Pad a text to the given length (former `StringUtils.pad`).
pub fn pad_text(text: &str, length: usize) -> String {
    pad(text, length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use std::rc::Rc;

    #[test]
    fn cells_and_blocks() {
        let backend = Rc::new(RecordingBackend::default());
        let output = MidiOutput::new(Sender::new(backend.clone()), Some(1));
        let mut display = create(output);
        display.set_cell(0, 0, "Volume").set_cell(0, 1, "Pan").set_block(0, 1, "A block of text").all_done().flush();
        let calls = backend.take_calls();
        let line = calls.iter().find_map(|c| match c {
            BackendCall::SendMidi(_, data) if data.starts_with(&SYSEX_MESSAGE[0]) => Some(data.clone()),
            _ => None,
        });
        let line = line.expect("row 0 is sent");
        let text: String = line[8..line.len() - 1].iter().map(|b| *b as char).collect();
        // 4 blocks of 17 characters separated by a space
        assert_eq!(text.len(), 71);
        assert_eq!(&text[..17], "Volume   Pan     ");
        assert_eq!(&text[18..35], "A block of text  ");
    }

    #[test]
    fn values() {
        assert_eq!(format_value(0, 1024), "\u{6}".repeat(8));
        assert_eq!(format_value(1024, 1024), "\u{5}".repeat(8));
        assert_eq!(format_value(64, 1024).chars().next(), Some(BARS_ONE));
        assert_eq!(format_pan(512, 1024), "\u{6}".repeat(8));
        let right = format_pan(1023, 1024);
        assert_eq!(right.chars().last(), Some(BARS_TWO));
        let left = format_pan(0, 1024);
        assert_eq!(left.chars().next(), Some(BARS_TWO));
        let menu = create_menu_list(3, &["a".into(), "b".into(), "c".into(), "d".into()], 3);
        assert_eq!(menu, vec![("b".into(), false), ("c".into(), false), ("d".into(), true)]);
    }

    #[test]
    fn transform() {
        let line = Push1LineWriter.transform_line(0, &format!("{}{}{}", BARS_NON, SELECT_ARROW, "x".repeat(66)));
        assert!(line.starts_with("->x"));
        assert_eq!(line.chars().nth(17), Some(' '));
    }
}
