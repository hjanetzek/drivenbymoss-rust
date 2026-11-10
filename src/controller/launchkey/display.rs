// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The display of the Launchkey Mk3 (former `LaunchkeyMk3Display`): 36 rows of 16 characters.
//!
//! - Rows 0 and 1 are the base screen.
//! - Rows 2 to 17 are the name and value of the 8 knob screens (pots).
//! - Rows 18 to 35 are the name and value of the 9 fader screens (8 faders and the master).
//!
//! The text is encoded in ISO-8859-2: characters above 127 are sent as `0x11` followed by the
//! character minus 0x80, characters which are not in ISO-8859-2 are left out. Difference to
//! Java: the byte of the ISO-8859-2 code page is sent instead of the Unicode value of the
//! character (Java sends `char - 0x80`, which is only right for the characters which ISO-8859-2
//! shares with Latin-1 and creates invalid sysex bytes for the others, e.g. 'Ł').

use std::cell::Cell;
use std::rc::Rc;

use crate::framework::display::{LineWriter, TextDisplay};
use crate::midi::MidiOutput;

/// The rows of the base screen.
pub const SCREEN_ROW_BASE: usize = 0;
/// The first row of the knob screens.
pub const SCREEN_ROW_POTS: usize = 2;
/// The first row of the fader screens.
pub const SCREEN_ROW_FADERS: usize = 18;

const SCREEN_ID_POT1: u8 = 56;
const SCREEN_ID_FADER1: u8 = 80;

/// The number of rows of the display.
pub const NUM_ROWS: usize = 2 * 18;
/// The number of characters of a row.
pub const NUM_CHARACTERS: usize = 16;

/// The device ID in the sysex header of the 25 to 61 key models.
const DEVICE_ID_DEFAULT: u8 = 0x0F;
/// The device ID in the sysex header of the 88 key model.
const DEVICE_ID_88: u8 = 0x12;

/// The Unicode characters of the upper half (0xA0 - 0xFF) of ISO-8859-2.
const ISO_8859_2_UPPER: [u16; 96] = [
    0x00A0, 0x0104, 0x02D8, 0x0141, 0x00A4, 0x013D, 0x015A, 0x00A7, 0x00A8, 0x0160, 0x015E, 0x0164, 0x0179, 0x00AD, 0x017D, 0x017B, //
    0x00B0, 0x0105, 0x02DB, 0x0142, 0x00B4, 0x013E, 0x015B, 0x02C7, 0x00B8, 0x0161, 0x015F, 0x0165, 0x017A, 0x02DD, 0x017E, 0x017C, //
    0x0154, 0x00C1, 0x00C2, 0x0102, 0x00C4, 0x0139, 0x0106, 0x00C7, 0x010C, 0x00C9, 0x0118, 0x00CB, 0x011A, 0x00CD, 0x00CE, 0x010E, //
    0x0110, 0x0143, 0x0147, 0x00D3, 0x00D4, 0x0150, 0x00D6, 0x00D7, 0x0158, 0x016E, 0x00DA, 0x0170, 0x00DC, 0x00DD, 0x0162, 0x00DF, //
    0x0155, 0x00E1, 0x00E2, 0x0103, 0x00E4, 0x013A, 0x0107, 0x00E7, 0x010D, 0x00E9, 0x0119, 0x00EB, 0x011B, 0x00ED, 0x00EE, 0x010F, //
    0x0111, 0x0144, 0x0148, 0x00F3, 0x00F4, 0x0151, 0x00F6, 0x00F7, 0x0159, 0x016F, 0x00FA, 0x0171, 0x00FC, 0x00FD, 0x0163, 0x02D9,
];

/// Encode a character in ISO-8859-2, `None` if it is not part of it.
pub fn encode_iso_8859_2(c: char) -> Option<u8> {
    let code = c as u32;
    if code < 0xA0 {
        return Some(code as u8);
    }
    ISO_8859_2_UPPER.iter().position(|&u| u as u32 == code).map(|i| 0xA0 + i as u8)
}

/// Encode a text in the Launchkey format: ISO-8859-2, characters above 127 are prefixed with
/// `0x11` and reduced by 0x80.
pub fn encode_text(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len());
    for c in text.chars() {
        let Some(byte) = encode_iso_8859_2(c) else {
            continue;
        };
        if byte > 127 {
            bytes.push(0x11);
            bytes.push(byte - 0x80);
        } else {
            bytes.push(byte);
        }
    }
    bytes
}

/// Sends the rows of the display (former `writeLine`).
pub struct LaunchkeyMk3Display {
    /// True for the 88 key model, which has a different device ID (former `setModel`).
    is_88_key_version: Rc<Cell<bool>>,
}

impl LaunchkeyMk3Display {
    /// Create the text display. The returned flag selects the header of the 88 key model.
    pub fn create(output: MidiOutput) -> (TextDisplay, Rc<Cell<bool>>) {
        let is_88_key_version = Rc::new(Cell::new(false));
        let writer = LaunchkeyMk3Display { is_88_key_version: is_88_key_version.clone() };
        let mut display = TextDisplay::new(output, Box::new(writer), NUM_ROWS, 1, NUM_CHARACTERS);
        display.set_center_notification(false);
        (display, is_88_key_version)
    }

    /// The sysex message of a row.
    pub fn message(is_88_key_version: bool, row: usize, text: &str) -> Vec<u8> {
        let device_id = if is_88_key_version { DEVICE_ID_88 } else { DEVICE_ID_DEFAULT };
        let mut data = vec![0xF0, 0x00, 0x20, 0x29, 0x02, device_id];
        if row < SCREEN_ROW_POTS {
            // Base screen
            data.push(0x04);
            data.push(row as u8);
        } else {
            data.push(if row.is_multiple_of(2) { 0x07 } else { 0x08 });
            if row < SCREEN_ROW_FADERS {
                // Pot screens
                data.push(SCREEN_ID_POT1 + ((row - SCREEN_ROW_POTS) / 2) as u8);
            } else {
                // Fader screens
                data.push(SCREEN_ID_FADER1 + ((row - SCREEN_ROW_FADERS) / 2) as u8);
            }
        }
        data.extend(encode_text(text));
        data.push(0xF7);
        data
    }
}

impl LineWriter for LaunchkeyMk3Display {
    fn write_line(&mut self, output: &MidiOutput, row: usize, text: &str, _previous_text: Option<&str>) {
        output.send_sysex(&Self::message(self.is_88_key_version.get(), row, text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages() {
        assert_eq!(LaunchkeyMk3Display::message(false, 0, "Hi"), vec![0xF0, 0x00, 0x20, 0x29, 0x02, 0x0F, 0x04, 0x00, b'H', b'i', 0xF7]);
        assert_eq!(LaunchkeyMk3Display::message(false, 1, "")[6..8], [0x04, 0x01]);
        // Pot 1 name and value, pot 8 value
        assert_eq!(LaunchkeyMk3Display::message(false, 2, "")[6..8], [0x07, 56]);
        assert_eq!(LaunchkeyMk3Display::message(false, 3, "")[6..8], [0x08, 56]);
        assert_eq!(LaunchkeyMk3Display::message(false, 17, "")[6..8], [0x08, 63]);
        // Fader 1 and the master fader
        assert_eq!(LaunchkeyMk3Display::message(false, 18, "")[6..8], [0x07, 80]);
        assert_eq!(LaunchkeyMk3Display::message(false, 35, "")[6..8], [0x08, 88]);
        assert_eq!(LaunchkeyMk3Display::message(true, 0, "")[5], 0x12);
    }

    #[test]
    fn encoding() {
        assert_eq!(encode_text("A1"), vec![b'A', b'1']);
        // ä is 0xE4 in ISO-8859-2 (as in Latin-1)
        assert_eq!(encode_text("ä"), vec![0x11, 0x64]);
        // Ł is 0xA3 in ISO-8859-2
        assert_eq!(encode_text("Ł"), vec![0x11, 0x23]);
        // Not in ISO-8859-2: left out
        assert_eq!(encode_text("a€b"), vec![b'a', b'b']);
        assert_eq!(encode_iso_8859_2('˙'), Some(0xFF));
    }
}
