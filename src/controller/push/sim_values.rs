// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The encoder state of the Push 2/3 for the Push simulator (`support/push_simulator`): a
//! simulator-only side channel, not part of the Java version and never sent to a real device.
//!
//! The real Push has endless encoders without LED rings; the values are only on the display. The
//! simulator (and a touchscreen controller built from it) draws them around its encoders. The
//! state is taken from what the display shows: the components of the 8 columns
//! ([`encoder_states`]), plus the master, tempo and play position encoders from the model, so no
//! mode needs to know about it.
//!
//! The core builds the messages only if the host takes them
//! ([`crate::backend::Backend::simulator_data_enabled`], `dbm_set_simulator_callback`) and hands
//! them to [`crate::backend::Backend::simulator_data_out`]; hosts write them to the simulator's
//! display socket between the frames. Only on change ([`SimValues::update`]); the host replays
//! the last message after it (re)connected to the socket.
//!
//! # The message (`doc/HOST_BRIDGE_GUIDE.md`, "Simulator side channel")
//!
//! ```text
//! "DBMV" version(1) kind(1) length(3: 21 bits, 7 bits per byte, least significant first) payload
//! ```
//!
//! The payload of kind [`KIND_ENCODERS`] (version 1): the number of encoders (1 byte), then per
//! encoder: its index (the touch note: 0-7 the encoders over the display, [`MASTER`],
//! [`POSITION`], [`TEMPO`]), flags ([`FLAG_HAS_VALUE`], [`FLAG_TOUCHED`], [`FLAG_BIPOLAR`]),
//! the value and the maximum (2 bytes each: 14 bits, 7 bits per byte, least significant first),
//! the name and the value text (each a length byte, 0-127, and that many UTF-8 bytes).
//!
//! Every byte outside the texts is below 0x80 and UTF-8 never contains 0xFF, so a message never
//! contains the frame header (`FF CC AA 88` + 12 zero bytes): a reader which scans for the frame
//! header (older simulators) skips it like garbage. A reader skips a message of an unknown
//! version or kind by its length, and rejects a "message" with a 0xFF byte (garbage which
//! happens to contain the magic). Messages of future kinds keep this rule.

use crate::framework::graphics::canvas::ChannelEditType;
use crate::framework::graphics::Component;

/// The magic of a message.
pub const MAGIC: [u8; 4] = *b"DBMV";
/// The version of the message format.
pub const VERSION: u8 = 1;
/// The kind of the encoder state message.
pub const KIND_ENCODERS: u8 = 1;
/// The size of the header: magic, version, kind, length.
pub const HEADER_SIZE: usize = 9;
/// The longest payload (3 bytes of 7 bits).
pub const MAX_PAYLOAD: usize = (1 << 21) - 1;
/// The longest name or value text in bytes (longer texts are cut at a character boundary).
pub const MAX_TEXT: usize = 127;
/// The largest value or maximum (14 bits).
pub const MAX_VALUE: u16 = (1 << 14) - 1;

/// The index of the master encoder (its touch note).
pub const MASTER: u8 = 8;
/// The index of the small right encoder (Push 2: "Swing", which DrivenByMoss uses for the play
/// position; its touch note).
pub const POSITION: u8 = 9;
/// The index of the tempo encoder (its touch note).
pub const TEMPO: u8 = 10;

/// The encoder has a value (the ring is drawn).
pub const FLAG_HAS_VALUE: u8 = 1;
/// The encoder is touched (or the display shows it as edited).
pub const FLAG_TOUCHED: u8 = 2;
/// The value is centred (e.g. panorama): the ring starts at the middle.
pub const FLAG_BIPOLAR: u8 = 4;

/// The state of one encoder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncoderState {
    pub index: u8,
    pub has_value: bool,
    pub value: u16,
    pub max: u16,
    pub touched: bool,
    pub bipolar: bool,
    pub name: String,
    pub text: String,
}

impl EncoderState {
    /// An encoder without a value, with a name and a text.
    pub fn text(index: u8, name: &str, text: &str, touched: bool) -> Self {
        EncoderState { index, touched, name: name.to_string(), text: text.to_string(), ..Default::default() }
    }

    /// An encoder with a value (clamped to 0..=max).
    pub fn value(index: u8, name: &str, value: i32, max: i32, text: &str, touched: bool) -> Self {
        let max = max.clamp(1, i32::from(MAX_VALUE));
        EncoderState { index, has_value: true, value: value.clamp(0, max) as u16, max: max as u16, touched, bipolar: false, name: name.to_string(), text: text.to_string() }
    }

    fn flags(&self) -> u8 {
        let mut flags = 0;
        if self.has_value {
            flags |= FLAG_HAS_VALUE;
        }
        if self.touched {
            flags |= FLAG_TOUCHED;
        }
        if self.bipolar {
            flags |= FLAG_BIPOLAR;
        }
        flags
    }
}

/// The text cut to [`MAX_TEXT`] bytes at a character boundary.
fn cut(text: &str) -> &str {
    if text.len() <= MAX_TEXT {
        return text;
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn put_u14(out: &mut Vec<u8>, value: u16) {
    let value = value.min(MAX_VALUE);
    out.push((value & 0x7F) as u8);
    out.push((value >> 7) as u8);
}

fn put_text(out: &mut Vec<u8>, text: &str) {
    let text = cut(text);
    out.push(text.len() as u8);
    out.extend_from_slice(text.as_bytes());
}

/// A message with the header.
pub fn encode_message(kind: u8, payload: &[u8]) -> Vec<u8> {
    let length = payload.len().min(MAX_PAYLOAD);
    let mut out = Vec::with_capacity(HEADER_SIZE + length);
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(kind);
    out.push((length & 0x7F) as u8);
    out.push(((length >> 7) & 0x7F) as u8);
    out.push(((length >> 14) & 0x7F) as u8);
    out.extend_from_slice(&payload[..length]);
    out
}

/// The encoder state message (at most 127 encoders).
pub fn encode_encoders(states: &[EncoderState]) -> Vec<u8> {
    let states = &states[..states.len().min(127)];
    let mut payload = vec![states.len() as u8];
    for state in states {
        payload.push(state.index & 0x7F);
        payload.push(state.flags());
        put_u14(&mut payload, state.value);
        put_u14(&mut payload, state.max);
        put_text(&mut payload, &state.name);
        put_text(&mut payload, &state.text);
    }
    encode_message(KIND_ENCODERS, &payload)
}

/// A decoded message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Encoders(Vec<EncoderState>),
    /// A message of another version or kind (skipped).
    Unknown { version: u8, kind: u8 },
}

/// Decode a message at the start of `data`: the message and its size, None if `data` does not
/// start with a complete and valid message.
pub fn decode_message(data: &[u8]) -> Option<(Message, usize)> {
    if data.len() < HEADER_SIZE || data[..4] != MAGIC || data[6..9].iter().any(|&b| b >= 0x80) {
        return None;
    }
    let (version, kind) = (data[4], data[5]);
    let length = usize::from(data[6]) | usize::from(data[7]) << 7 | usize::from(data[8]) << 14;
    let size = HEADER_SIZE + length;
    let payload = data.get(HEADER_SIZE..size)?;
    // No message contains 0xFF: garbage which happens to contain the magic
    if payload.contains(&0xFF) {
        return None;
    }
    if version != VERSION || kind != KIND_ENCODERS {
        return Some((Message::Unknown { version, kind }, size));
    }
    Some((Message::Encoders(decode_encoders(payload)?), size))
}

fn decode_encoders(payload: &[u8]) -> Option<Vec<EncoderState>> {
    let (&count, mut rest) = payload.split_first()?;
    let mut states = Vec::with_capacity(usize::from(count));
    let text = |rest: &mut &[u8]| -> Option<String> {
        let (&length, tail) = rest.split_first()?;
        let bytes = tail.get(..usize::from(length))?;
        *rest = &tail[usize::from(length)..];
        String::from_utf8(bytes.to_vec()).ok()
    };
    for _ in 0..count {
        let fixed = rest.get(..6)?;
        rest = &rest[6..];
        let flags = fixed[1];
        let value = u16::from(fixed[2]) | u16::from(fixed[3]) << 7;
        let max = u16::from(fixed[4]) | u16::from(fixed[5]) << 7;
        let name = text(&mut rest)?;
        let value_text = text(&mut rest)?;
        states.push(EncoderState {
            index: fixed[0],
            has_value: flags & FLAG_HAS_VALUE != 0,
            value,
            max,
            touched: flags & FLAG_TOUCHED != 0,
            bipolar: flags & FLAG_BIPOLAR != 0,
            name,
            text: value_text,
        });
    }
    Some(states)
}

/// The state of the encoder over a display column, from the column's component.
/// `max` is the largest value of the display (the upper bound - 1).
pub fn column_state(index: u8, component: Option<&Component>, max: i32) -> EncoderState {
    let empty = EncoderState { index, ..Default::default() };
    let Some(component) = component else {
        return empty;
    };
    match component {
        Component::Parameter(p) => {
            if p.param_name.is_empty() {
                return empty;
            }
            if p.param_value < 0 {
                return EncoderState::text(index, &p.param_name, &p.param_value_text, p.is_touched);
            }
            EncoderState::value(index, &p.param_name, p.param_value, max, &p.param_value_text, p.is_touched)
        }
        Component::Channel(c) => {
            let name = &c.menu.footer.text;
            // The modes show the value text only while the encoder is touched
            match c.edit_type {
                ChannelEditType::Volume => EncoderState::value(index, name, c.volume_value, max, &c.volume_text, !c.volume_text.is_empty()),
                ChannelEditType::Pan => EncoderState { bipolar: true, ..EncoderState::value(index, name, c.pan_value, max, &c.pan_text, !c.pan_text.is_empty()) },
                ChannelEditType::Crossfader if c.crossfade_mode >= 0 => {
                    let text = ["A", "AB", "B"].get(c.crossfade_mode as usize).copied().unwrap_or("");
                    EncoderState { bipolar: true, ..EncoderState::value(index, name, c.crossfade_mode * max / 2, max, text, false) }
                }
                // One column with all the values of a channel: no single value
                _ => EncoderState::text(index, name, "", false),
            }
        }
        Component::Sends(s) if !s.is_ex_mode => {
            let name = &s.channel.menu.footer.text;
            match s.send_data.iter().find(|send| send.edited) {
                Some(send) if send.value >= 0 && !send.name.is_empty() => EncoderState::value(index, name, send.value, max, &send.text, !send.text.is_empty()),
                _ => EncoderState::text(index, name, "", false),
            }
        }
        _ => empty,
    }
}

/// The states of the 8 encoders over the display, from the components of the 8 columns (fewer
/// components: the missing columns have no value).
pub fn encoder_states(components: &[Component], max: i32) -> Vec<EncoderState> {
    // The columns only match the encoders if there are 8 of them
    let columns = if components.len() == 8 { components } else { &[] };
    (0..8u8).map(|i| column_state(i, columns.get(usize::from(i)), max)).collect()
}

/// The sender state: the last sent message.
#[derive(Debug, Default)]
pub struct SimValues {
    last: Option<Vec<u8>>,
}

impl SimValues {
    pub fn new() -> Self {
        SimValues::default()
    }

    /// The message to send for the states: Some if it differs from the last one (the first
    /// call always returns one).
    pub fn update(&mut self, states: &[EncoderState]) -> Option<&[u8]> {
        let message = encode_encoders(states);
        if self.last.as_ref() == Some(&message) {
            return None;
        }
        self.last = Some(message);
        self.last.as_deref()
    }

    /// Forget the last message: the next [`Self::update`] sends.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The states of the shared test message (`support/push_simulator/tests/data/dbmv_encoders.hex`,
    /// also decoded and encoded by the simulator's and the binding's tests).
    pub fn fixture_states() -> Vec<EncoderState> {
        vec![
            EncoderState::value(0, "Cutoff", 512, 1023, "50.0 %", true),
            EncoderState { bipolar: true, ..EncoderState::value(1, "Pan", 255, 1023, "", false) },
            EncoderState::text(2, "Größe", "", false),
            EncoderState::default(),
            EncoderState::value(MASTER, "Master", 1023, 1023, "0.0 dB", false),
            EncoderState::text(TEMPO, "Tempo", "120.00", false),
        ]
    }

    fn fixture_bytes() -> Vec<u8> {
        let hex: String = include_str!("../../../support/push_simulator/tests/data/dbmv_encoders.hex").split_whitespace().collect();
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn the_fixture_round_trips() {
        let bytes = encode_encoders(&fixture_states());
        assert_eq!(bytes, fixture_bytes(), "{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        assert_eq!(decode_message(&bytes), Some((Message::Encoders(fixture_states()), bytes.len())));
        assert_eq!(&bytes[..6], b"DBMV\x01\x01");
    }

    #[test]
    fn no_frame_header_and_no_0xff() {
        let long = "x".repeat(300) + "é";
        let states: Vec<EncoderState> = (0..11).map(|i| EncoderState::value(i, &long, 16383, 16383, "ÿ\u{FFFF}", true)).collect();
        let bytes = encode_encoders(&states);
        assert!(!bytes.contains(&0xFF));
        let Some((Message::Encoders(decoded), size)) = decode_message(&bytes) else { panic!() };
        assert_eq!(size, bytes.len());
        // Cut at 127 bytes, the values in 14 bits
        assert_eq!(decoded[0].name.len(), 127);
        assert_eq!(decoded[3].value, 16383);
        // A value or maximum out of range is clamped
        assert_eq!(EncoderState::value(0, "", 2000, 1023, "", false).value, 1023);
        assert_eq!(EncoderState::value(0, "", -5, 99_999, "", false), EncoderState { has_value: true, max: MAX_VALUE, ..Default::default() });
    }

    #[test]
    fn decoding_partial_unknown_and_invalid() {
        let bytes = encode_encoders(&fixture_states());
        for end in 0..bytes.len() {
            assert_eq!(decode_message(&bytes[..end]), None, "{end}");
        }
        // Another kind is skipped by its length
        let other = encode_message(9, &[0xFE, 1, 2]);
        assert_eq!(decode_message(&other), Some((Message::Unknown { version: 1, kind: 9 }, 12)));
        // Not a message: it contains 0xFF
        assert_eq!(decode_message(&encode_message(9, &[0xFF])), None);
        // An invalid length byte
        let mut invalid = bytes.clone();
        invalid[6] = 0x80;
        assert_eq!(decode_message(&invalid), None);
        // A payload which does not match its count
        assert_eq!(decode_message(&encode_message(KIND_ENCODERS, &[2, 0, 0])), None);
    }

    #[test]
    fn change_detection() {
        let mut values = SimValues::new();
        let mut states = fixture_states();
        assert!(values.update(&states).is_some());
        assert!(values.update(&states).is_none());
        states[0].touched = false;
        assert!(values.update(&states).is_some());
        assert!(values.update(&states).is_none());
        values.reset();
        assert!(values.update(&states).is_some());
    }
}
