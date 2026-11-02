// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The display of the MPC/Force (former `ACVSDisplay`) and the ACVS system exclusive messages
//! (former `ACVSMidiOutput` and `ACVSMessage`).
//!
//! The texts of the screen are addressed by item IDs. The display keeps the texts per item ID
//! (instead of the 4672 lines of the Java text display) and sends only the changed ones; rows
//! without a new text are not sent (former override of `done`). The display cannot show
//! notifications.

use std::collections::{BTreeMap, HashMap};

use super::definition::AcvsDevice;
use super::screen_item::ScreenItem;
use crate::config::values::fix_ascii;
use crate::daw::color::ColorEx;
use crate::midi::MidiOutput;

pub const ITEM_ID_TRACK_HEADER_FIRST: usize = 0x00;
pub const ITEM_ID_CLIPS_FIRST: usize = 0x10;
pub const ITEM_ID_SCENES_FIRST: usize = 0x50;
pub const ITEM_ID_TRACK_FADER_LEVEL_FIRST: usize = 0x100;
pub const ITEM_ID_TRACK_PEAK_LEVEL_FIRST: usize = 0x108;
pub const ITEM_ID_TRACK_PAN_FIRST: usize = 0x110;
pub const ITEM_ID_TRACK_SEND1_FIRST: usize = 0x118;
pub const ITEM_ID_TRACK_SEND2_FIRST: usize = 0x120;
pub const ITEM_ID_TRACK_SEND3_FIRST: usize = 0x128;
pub const ITEM_ID_TRACK_SEND4_FIRST: usize = 0x130;
pub const ITEM_ID_DEVICE_BANK_NAME: usize = 0x200;
pub const ITEM_ID_DEVICE_NAME: usize = 0x201;
pub const ITEM_ID_DEVICE_PARAMETER_NAME_FIRST: usize = 0x210;
pub const ITEM_ID_DEVICE_PARAMETER_VALUE_FIRST: usize = 0x220;
pub const ITEM_ID_TEMPO: usize = 0x300;
pub const ITEM_ID_ARRANGEMENT_POSITION: usize = 0x310;
pub const ITEM_ID_LOOP_START: usize = 0x311;
pub const ITEM_ID_LOOP_LENGTH: usize = 0x312;
pub const ITEM_ID_DEVICE_PARAM_NAME1: usize = 0x1210;
pub const ITEM_ID_DEVICE_PARAM_VALUE1: usize = 0x122F;

pub const MESSAGE_ID_PING: u8 = 0x00;
pub const MESSAGE_ID_PONG: u8 = 0x01;
pub const MESSAGE_ID_TEXT: u8 = 0x10;
pub const MESSAGE_ID_COLOR: u8 = 0x11;

/// A received ACVS message (former `ACVSMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcvsMessage {
    pub device: AcvsDevice,
    pub message_id: u8,
    pub content: Vec<u8>,
}

/// Parse a received system exclusive message: F0 47 00 device-ID message-ID content F7 (former
/// `ACVSMidiOutput.getMessageContent`).
pub fn parse_message(data: &[u8]) -> Option<AcvsMessage> {
    const HEADER_LENGTH: usize = 4;
    let content_length = data.len() as i32 - HEADER_LENGTH as i32 - 1;
    if content_length <= 0 || data[data.len() - 1] != 0xF7 {
        return None;
    }
    if data[0] != 0xF0 || data[1] != 0x47 || data[2] != 0x00 {
        return None;
    }
    let device = AcvsDevice::from_id(data[3])?;
    let content = data[HEADER_LENGTH + 1..data.len() - 1].to_vec();
    Some(AcvsMessage { device, message_id: data[HEADER_LENGTH], content })
}

/// Create a system exclusive message (former `ACVSMidiOutput.sendSysex (messageTypeID, data)`).
pub fn create_sysex(device: AcvsDevice, message_id: u8, data: &[u8]) -> Vec<u8> {
    let mut message = vec![0xF0, 0x47, 0x00, device.id(), message_id];
    message.extend_from_slice(data);
    message.push(0xF7);
    message
}

/// The ping message (former `pingMessage`).
pub fn ping_message(device: AcvsDevice) -> Vec<u8> {
    vec![0xF0, 0x47, 0x00, device.id(), MESSAGE_ID_PING, 0xF7]
}

/// Create the text message of an item (former `sendText`).
pub fn text_message(device: AcvsDevice, item_id: usize, text: &str) -> Vec<u8> {
    let text_bytes = text.as_bytes();
    let mut data = Vec::with_capacity(4 + text_bytes.len());
    // Item ID MSB / LSB
    data.push((item_id >> 8 & 0x7F) as u8);
    data.push((item_id & 0x7F) as u8);
    // Text length MSB / LSB
    data.push((text_bytes.len() >> 8 & 0x7F) as u8);
    data.push((text_bytes.len() & 0x7F) as u8);
    data.extend_from_slice(text_bytes);
    create_sysex(device, MESSAGE_ID_TEXT, &data)
}

/// Create the color message of an item (former `sendColor`).
pub fn color_message(device: AcvsDevice, item_id: usize, color: ColorEx) -> Vec<u8> {
    let rgb = color.to_int_rgb127();
    let data = [(item_id >> 8 & 0x7F) as u8, (item_id & 0x7F) as u8, rgb[0] as u8, rgb[1] as u8, rgb[2] as u8];
    create_sysex(device, MESSAGE_ID_COLOR, &data)
}

/// The display of the MPC/Force (former `ACVSDisplay`).
pub struct AcvsDisplay {
    device: AcvsDevice,
    /// The texts set since the last `all_done` (former `fullRows`).
    full_rows: BTreeMap<usize, String>,
    /// The texts to send (former `message`).
    message: BTreeMap<usize, String>,
    /// The texts which were sent (former `currentMessage`).
    current_message: HashMap<usize, String>,
    color: BTreeMap<usize, ColorEx>,
    current_color: HashMap<usize, ColorEx>,
    screen_item_cache: HashMap<ScreenItem, i32>,
}

impl AcvsDisplay {
    pub fn new(device: AcvsDevice) -> Self {
        AcvsDisplay {
            device,
            full_rows: BTreeMap::new(),
            message: BTreeMap::new(),
            current_message: HashMap::new(),
            color: BTreeMap::new(),
            current_color: HashMap::new(),
            screen_item_cache: HashMap::new(),
        }
    }

    pub fn device(&self) -> AcvsDevice {
        self.device
    }

    /// Set the text of an item.
    pub fn set_row(&mut self, row: usize, text: &str) -> &mut Self {
        self.full_rows.insert(row, text.to_string());
        self
    }

    /// Set the color of an item.
    pub fn set_color(&mut self, row: usize, color: ColorEx) {
        self.color.insert(row, color);
    }

    /// Finish the editing of all rows (only the rows with a new text are updated).
    pub fn all_done(&mut self) {
        let rows = std::mem::take(&mut self.full_rows);
        self.message.extend(rows);
    }

    /// Send the changed texts and colors.
    pub fn flush(&mut self, output: &MidiOutput) {
        for (row, text) in &self.message {
            // Has anything changed?
            if self.current_message.get(row) == Some(text) {
                continue;
            }
            self.current_message.insert(*row, text.clone());
            output.send_sysex(&text_message(self.device, *row, &fix_ascii(text)));
        }
        for (row, color) in &self.color {
            if self.current_color.get(row) == Some(color) {
                continue;
            }
            self.current_color.insert(*row, *color);
            output.send_sysex(&color_message(self.device, *row, *color));
        }
    }

    /// Send all texts, colors and screen items again with the next flush.
    pub fn force_flush(&mut self) {
        self.current_message.clear();
        self.current_color.clear();
        self.screen_item_cache.clear();
    }

    /// Set the value of a screen item; it is only sent if it changed (former `setScreenItem`).
    pub fn set_screen_item(&mut self, output: &MidiOutput, screen_item: ScreenItem, value: i32) {
        if self.screen_item_cache.get(&screen_item) == Some(&value) {
            return;
        }
        self.screen_item_cache.insert(screen_item, value);
        let (channel, is_note, note_cc) = screen_item.midi();
        if is_note {
            output.send_note_ex(channel, note_cc, value);
        } else {
            output.send_cc_ex(channel, note_cc, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages() {
        assert_eq!(ping_message(AcvsDevice::Force), vec![0xF0, 0x47, 0x00, 0x40, 0x00, 0xF7]);
        assert_eq!(text_message(AcvsDevice::MpcLiveOne, 0x300, "120"), vec![0xF0, 0x47, 0x00, 0x3B, 0x10, 0x03, 0x00, 0x00, 0x03, b'1', b'2', b'0', 0xF7]);
        assert_eq!(color_message(AcvsDevice::MpcX, 0x10, ColorEx::RED), vec![0xF0, 0x47, 0x00, 0x3A, 0x11, 0x00, 0x10, 127, 0, 0, 0xF7]);

        let pong = parse_message(&[0xF0, 0x47, 0x00, 0x40, 0x01, 0x00, 0xF7]).unwrap();
        assert_eq!(pong.device, AcvsDevice::Force);
        assert_eq!(pong.message_id, MESSAGE_ID_PONG);
        assert_eq!(pong.content, vec![0x00]);
        assert!(parse_message(&[0xF0, 0x47, 0x00, 0x33, 0x01, 0x00, 0xF7]).is_none());
        assert!(parse_message(&[0xF0, 0x47, 0x00, 0x40, 0xF7]).is_none());
    }
}
