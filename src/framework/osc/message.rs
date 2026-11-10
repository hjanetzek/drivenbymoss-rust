// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! OSC 1.0 messages and bundles and their binary encoding (former `IOpenSoundControlMessage`;
//! the Java version uses the OSC implementation of the host, Bitwig's `OscModule`).
//!
//! Supported argument types: `i` (int32), `f` (float32), `s` (string), `S` (symbol, decoded as
//! a string), `b` (blob), `h` (int64), `d` (float64), `t` (time tag), `c` (character), `r` (RGBA
//! color), `m` (MIDI message), `T` (true), `F` (false), `N` (nil) and `I` (impulse). Arrays (`[`
//! and `]`) are not supported. Strings are decoded as UTF-8 (invalid sequences are replaced),
//! although OSC 1.0 only allows ASCII.

use std::fmt;

use super::java;

/// The time tag "immediately" of a bundle.
pub const IMMEDIATELY: u64 = 1;

/// An argument of an OSC message.
#[derive(Debug, Clone)]
pub enum OscArg {
    /// `i`: a 32 bit integer (Java `Integer`).
    Int(i32),
    /// `f`: a 32 bit float (Java `Float`).
    Float(f32),
    /// `s` or `S`: a string (Java `String`).
    Str(String),
    /// `b`: binary data (Java `byte []`).
    Blob(Vec<u8>),
    /// `h`: a 64 bit integer (Java `Long`).
    Long(i64),
    /// `d`: a 64 bit float (Java `Double`).
    Double(f64),
    /// `t`: an OSC time tag (NTP format).
    TimeTag(u64),
    /// `c`: a character (sent as a 32 bit integer).
    Char(char),
    /// `r`: a 32 bit RGBA color.
    Color(u32),
    /// `m`: a MIDI message (port ID, status, data 1, data 2).
    Midi([u8; 4]),
    /// `T` or `F`: a boolean without data (Java `Boolean`).
    Bool(bool),
    /// `N`: nil (Java `null`).
    Nil,
    /// `I`: impulse (infinitum), without data.
    Impulse,
}

impl OscArg {
    /// The type tag of the argument.
    pub fn type_tag(&self) -> char {
        match self {
            OscArg::Int(_) => 'i',
            OscArg::Float(_) => 'f',
            OscArg::Str(_) => 's',
            OscArg::Blob(_) => 'b',
            OscArg::Long(_) => 'h',
            OscArg::Double(_) => 'd',
            OscArg::TimeTag(_) => 't',
            OscArg::Char(_) => 'c',
            OscArg::Color(_) => 'r',
            OscArg::Midi(_) => 'm',
            OscArg::Bool(true) => 'T',
            OscArg::Bool(false) => 'F',
            OscArg::Nil => 'N',
            OscArg::Impulse => 'I',
        }
    }

    /// The value as a number if it is one (Java `instanceof Number`: Integer, Float, Long,
    /// Double). Booleans, strings and the other types are no numbers.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            OscArg::Int(v) => Some(f64::from(*v)),
            OscArg::Float(v) => Some(f64::from(*v)),
            OscArg::Long(v) => Some(*v as f64),
            OscArg::Double(v) => Some(*v),
            _ => None,
        }
    }

    /// The value formatted like Java's `Object.toString` of the value which Bitwig's OSC module
    /// creates for the argument (e.g. `1.0` for a float, `true` for `T`, `null` for `N`).
    pub fn java_string(&self) -> String {
        match self {
            OscArg::Int(v) => v.to_string(),
            OscArg::Float(v) => java::float_to_string(*v),
            OscArg::Str(v) => v.clone(),
            OscArg::Blob(v) => format!("[B@{}", v.len()),
            OscArg::Long(v) => v.to_string(),
            OscArg::Double(v) => java::double_to_string(*v),
            OscArg::TimeTag(v) => v.to_string(),
            OscArg::Char(v) => v.to_string(),
            OscArg::Color(v) => v.to_string(),
            OscArg::Midi(v) => format!("{v:?}"),
            OscArg::Bool(v) => v.to_string(),
            OscArg::Nil => "null".to_string(),
            OscArg::Impulse => "Impulse".to_string(),
        }
    }

    /// Java `equals` of the values: floats compare their bits (as `Float.equals`).
    pub fn java_equals(&self, other: &OscArg) -> bool {
        match (self, other) {
            (OscArg::Float(a), OscArg::Float(b)) => a.to_bits() == b.to_bits(),
            (OscArg::Double(a), OscArg::Double(b)) => a.to_bits() == b.to_bits(),
            (OscArg::Int(a), OscArg::Int(b)) => a == b,
            (OscArg::Str(a), OscArg::Str(b)) => a == b,
            (OscArg::Blob(a), OscArg::Blob(b)) => a == b,
            (OscArg::Long(a), OscArg::Long(b)) => a == b,
            (OscArg::TimeTag(a), OscArg::TimeTag(b)) => a == b,
            (OscArg::Char(a), OscArg::Char(b)) => a == b,
            (OscArg::Color(a), OscArg::Color(b)) => a == b,
            (OscArg::Midi(a), OscArg::Midi(b)) => a == b,
            (OscArg::Bool(a), OscArg::Bool(b)) => a == b,
            (OscArg::Nil, OscArg::Nil) | (OscArg::Impulse, OscArg::Impulse) => true,
            _ => false,
        }
    }
}

impl PartialEq for OscArg {
    fn eq(&self, other: &Self) -> bool {
        self.java_equals(other)
    }
}

impl fmt::Display for OscArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.java_string())
    }
}

/// An OSC message: an address and its arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct OscMessage {
    pub address: String,
    pub args: Vec<OscArg>,
}

impl OscMessage {
    pub fn new(address: impl Into<String>, args: Vec<OscArg>) -> Self {
        OscMessage { address: address.into(), args }
    }

    /// Format the message as in the log of the Java version: `/address [ 1, 0.5, text ]`.
    pub fn log_text(&self) -> String {
        let values: Vec<String> = self.args.iter().map(OscArg::java_string).collect();
        format!("{} [ {} ]", self.address, values.join(", "))
    }

    /// Encode the message.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_into(&mut out);
        out
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        write_string(out, &self.address);
        let mut tags = String::with_capacity(self.args.len() + 1);
        tags.push(',');
        for arg in &self.args {
            tags.push(arg.type_tag());
        }
        write_string(out, &tags);
        for arg in &self.args {
            match arg {
                OscArg::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::Str(v) => write_string(out, v),
                OscArg::Blob(v) => {
                    out.extend_from_slice(&(v.len() as u32).to_be_bytes());
                    out.extend_from_slice(v);
                    pad(out);
                }
                OscArg::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::TimeTag(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::Char(v) => out.extend_from_slice(&(*v as u32).to_be_bytes()),
                OscArg::Color(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::Midi(v) => out.extend_from_slice(v),
                OscArg::Bool(_) | OscArg::Nil | OscArg::Impulse => {}
            }
        }
    }
}

/// An OSC packet: a message or a bundle of packets.
#[derive(Debug, Clone, PartialEq)]
pub enum OscPacket {
    Message(OscMessage),
    Bundle { time_tag: u64, content: Vec<OscPacket> },
}

impl OscPacket {
    /// A bundle of messages which are executed immediately.
    pub fn bundle(messages: Vec<OscMessage>) -> Self {
        OscPacket::Bundle { time_tag: IMMEDIATELY, content: messages.into_iter().map(OscPacket::Message).collect() }
    }

    /// Encode the packet.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_into(&mut out);
        out
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        match self {
            OscPacket::Message(message) => message.encode_into(out),
            OscPacket::Bundle { time_tag, content } => {
                write_string(out, "#bundle");
                out.extend_from_slice(&time_tag.to_be_bytes());
                for element in content {
                    let start = out.len();
                    out.extend_from_slice(&[0; 4]);
                    element.encode_into(out);
                    let size = (out.len() - start - 4) as u32;
                    out[start..start + 4].copy_from_slice(&size.to_be_bytes());
                }
            }
        }
    }

    /// Decode a packet (the content of a UDP datagram).
    pub fn decode(data: &[u8]) -> Result<OscPacket, OscDecodeError> {
        decode_packet(data, 0)
    }

    /// All messages of the packet, the ones of (nested) bundles in their order. Time tags are
    /// ignored: the messages are executed when they arrive.
    pub fn into_messages(self) -> Vec<OscMessage> {
        let mut messages = Vec::new();
        self.collect_messages(&mut messages);
        messages
    }

    fn collect_messages(self, messages: &mut Vec<OscMessage>) {
        match self {
            OscPacket::Message(message) => messages.push(message),
            OscPacket::Bundle { content, .. } => content.into_iter().for_each(|p| p.collect_messages(messages)),
        }
    }
}

/// Why a packet could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscDecodeError {
    /// The data ended before the end of an element.
    Truncated,
    /// The packet is neither a message (starts with '/') nor a bundle (`#bundle`).
    NotOsc,
    /// The size of an element of a bundle is not a multiple of 4 or exceeds the bundle.
    BadSize,
    /// A type tag which is not supported.
    UnsupportedType(char),
    /// Bundles are nested too deep.
    TooDeep,
}

impl fmt::Display for OscDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OscDecodeError::Truncated => write!(f, "truncated packet"),
            OscDecodeError::NotOsc => write!(f, "not an OSC packet"),
            OscDecodeError::BadSize => write!(f, "bad bundle element size"),
            OscDecodeError::UnsupportedType(c) => write!(f, "unsupported type tag '{c}'"),
            OscDecodeError::TooDeep => write!(f, "bundles nested too deep"),
        }
    }
}

/// The maximum nesting of bundles.
const MAX_DEPTH: usize = 16;

fn decode_packet(data: &[u8], depth: usize) -> Result<OscPacket, OscDecodeError> {
    if depth > MAX_DEPTH {
        return Err(OscDecodeError::TooDeep);
    }
    match data.first() {
        Some(b'/') => decode_message(data).map(OscPacket::Message),
        Some(b'#') => {
            let mut reader = Reader { data, pos: 0 };
            if reader.string()? != "#bundle" {
                return Err(OscDecodeError::NotOsc);
            }
            let time_tag = u64::from_be_bytes(reader.bytes::<8>()?);
            let mut content = Vec::new();
            while reader.pos < data.len() {
                let size = u32::from_be_bytes(reader.bytes::<4>()?) as usize;
                if !size.is_multiple_of(4) || reader.pos + size > data.len() {
                    return Err(OscDecodeError::BadSize);
                }
                content.push(decode_packet(&data[reader.pos..reader.pos + size], depth + 1)?);
                reader.pos += size;
            }
            Ok(OscPacket::Bundle { time_tag, content })
        }
        Some(_) => Err(OscDecodeError::NotOsc),
        None => Err(OscDecodeError::Truncated),
    }
}

fn decode_message(data: &[u8]) -> Result<OscMessage, OscDecodeError> {
    let mut reader = Reader { data, pos: 0 };
    let address = reader.string()?;
    // Old implementations send no type tag string: no arguments
    if reader.pos >= data.len() || data[reader.pos] != b',' {
        return Ok(OscMessage { address, args: Vec::new() });
    }
    let tags = reader.string()?;
    let mut args = Vec::with_capacity(tags.len().saturating_sub(1));
    for tag in tags.chars().skip(1) {
        args.push(match tag {
            'i' => OscArg::Int(i32::from_be_bytes(reader.bytes::<4>()?)),
            'f' => OscArg::Float(f32::from_be_bytes(reader.bytes::<4>()?)),
            's' | 'S' => OscArg::Str(reader.string()?),
            'b' => {
                let size = u32::from_be_bytes(reader.bytes::<4>()?) as usize;
                let blob = reader.take(size)?.to_vec();
                reader.align()?;
                OscArg::Blob(blob)
            }
            'h' => OscArg::Long(i64::from_be_bytes(reader.bytes::<8>()?)),
            'd' => OscArg::Double(f64::from_be_bytes(reader.bytes::<8>()?)),
            't' => OscArg::TimeTag(u64::from_be_bytes(reader.bytes::<8>()?)),
            'c' => OscArg::Char(char::from_u32(u32::from_be_bytes(reader.bytes::<4>()?)).unwrap_or(char::REPLACEMENT_CHARACTER)),
            'r' => OscArg::Color(u32::from_be_bytes(reader.bytes::<4>()?)),
            'm' => OscArg::Midi(reader.bytes::<4>()?),
            'T' => OscArg::Bool(true),
            'F' => OscArg::Bool(false),
            'N' => OscArg::Nil,
            'I' => OscArg::Impulse,
            other => return Err(OscDecodeError::UnsupportedType(other)),
        });
    }
    Ok(OscMessage { address, args })
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], OscDecodeError> {
        let end = self.pos.checked_add(count).ok_or(OscDecodeError::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(OscDecodeError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], OscDecodeError> {
        let slice = self.take(N)?;
        let mut result = [0; N];
        result.copy_from_slice(slice);
        Ok(result)
    }

    /// Skip the padding to the next multiple of 4.
    fn align(&mut self) -> Result<(), OscDecodeError> {
        let padding = (4 - self.pos % 4) % 4;
        self.take(padding).map(|_| ())
    }

    /// A null terminated string, padded to a multiple of 4.
    fn string(&mut self) -> Result<String, OscDecodeError> {
        let rest = &self.data[self.pos.min(self.data.len())..];
        let end = rest.iter().position(|&b| b == 0).ok_or(OscDecodeError::Truncated)?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        self.align()?;
        Ok(text)
    }
}

/// Write a null terminated string, padded with nulls to a multiple of 4.
fn write_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    pad(out);
}

fn pad(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(message: OscMessage) {
        let encoded = message.encode();
        assert!(encoded.len().is_multiple_of(4));
        assert_eq!(OscPacket::decode(&encoded), Ok(OscPacket::Message(message)));
    }

    #[test]
    fn encodes_the_specification_examples() {
        // The examples of the OSC 1.0 specification
        let message = OscMessage::new("/oscillator/4/frequency", vec![OscArg::Float(440.0)]);
        let expected: Vec<u8> = [
            b"/osc".as_slice(),
            b"illa",
            b"tor/",
            b"4/fr",
            b"eque",
            b"ncy\0",
            b",f\0\0",
            &[0x43, 0xdc, 0x00, 0x00],
        ]
        .concat();
        assert_eq!(message.encode(), expected);

        let message = OscMessage::new(
            "/foo",
            vec![OscArg::Int(1000), OscArg::Int(-1), OscArg::Str("hello".into()), OscArg::Float(1.234), OscArg::Float(5.678)],
        );
        let expected: Vec<u8> = [
            b"/foo".as_slice(),
            &[0, 0, 0, 0],
            b",iis",
            b"ff\0\0",
            &[0, 0, 0x03, 0xe8],
            &[0xff, 0xff, 0xff, 0xff],
            b"hell",
            b"o\0\0\0",
            &[0x3f, 0x9d, 0xf3, 0xb6],
            &[0x40, 0xb5, 0xb2, 0x2d],
        ]
        .concat();
        assert_eq!(message.encode(), expected);
    }

    #[test]
    fn round_trips_all_types() {
        round_trip(OscMessage::new("/a", vec![]));
        round_trip(OscMessage::new(
            "/track/1/volume",
            vec![
                OscArg::Int(i32::MIN),
                OscArg::Float(-0.25),
                OscArg::Str(String::new()),
                OscArg::Str("abc".into()),
                OscArg::Str("abcd".into()),
                OscArg::Blob(vec![]),
                OscArg::Blob(vec![1, 2, 3, 4, 5]),
                OscArg::Long(i64::MAX),
                OscArg::Double(1e300),
                OscArg::TimeTag(IMMEDIATELY),
                OscArg::Char('x'),
                OscArg::Color(0xFF00_80FF),
                OscArg::Midi([0, 0x90, 60, 127]),
                OscArg::Bool(true),
                OscArg::Bool(false),
                OscArg::Nil,
                OscArg::Impulse,
                OscArg::Str("ümlaut".into()),
            ],
        ));
    }

    #[test]
    fn bundles() {
        let inner = OscPacket::bundle(vec![OscMessage::new("/c", vec![OscArg::Int(3)])]);
        let bundle = OscPacket::Bundle {
            time_tag: 0x1234_5678_9ABC_DEF0,
            content: vec![
                OscPacket::Message(OscMessage::new("/a", vec![OscArg::Int(1)])),
                inner,
                OscPacket::Message(OscMessage::new("/b", vec![OscArg::Str("x".into())])),
            ],
        };
        let encoded = bundle.encode();
        assert_eq!(&encoded[..8], b"#bundle\0");
        assert_eq!(&encoded[8..16], &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);
        let decoded = OscPacket::decode(&encoded).unwrap();
        assert_eq!(decoded, bundle);
        let addresses: Vec<String> = decoded.into_messages().into_iter().map(|m| m.address).collect();
        assert_eq!(addresses, ["/a", "/c", "/b"]);
        // An empty bundle
        let empty = OscPacket::bundle(vec![]);
        assert_eq!(OscPacket::decode(&empty.encode()), Ok(empty));
    }

    #[test]
    fn decode_errors() {
        assert_eq!(OscPacket::decode(b""), Err(OscDecodeError::Truncated));
        assert_eq!(OscPacket::decode(b"abc\0"), Err(OscDecodeError::NotOsc));
        assert_eq!(OscPacket::decode(b"/abc"), Err(OscDecodeError::Truncated));
        assert_eq!(OscPacket::decode(b"/a\0\0,i\0\0\0\0"), Err(OscDecodeError::Truncated));
        assert_eq!(OscPacket::decode(b"/a\0\0,[\0\0"), Err(OscDecodeError::UnsupportedType('[')));
        assert_eq!(OscPacket::decode(b"#bundle\0\0\0\0\0\0\0\0\x01\0\0\0\x05/a\0\0"), Err(OscDecodeError::BadSize));
        assert_eq!(OscPacket::decode(b"#bundlX\0\0\0\0\0\0\0\0\x01"), Err(OscDecodeError::NotOsc));
        // A bundle with a huge element size
        assert_eq!(OscPacket::decode(b"#bundle\0\0\0\0\0\0\0\0\x01\xff\xff\xff\xfc"), Err(OscDecodeError::BadSize));
        // Without a type tag string: no arguments
        assert_eq!(OscPacket::decode(b"/play\0\0\0"), Ok(OscPacket::Message(OscMessage::new("/play", vec![]))));
        // A blob which exceeds the data
        assert_eq!(OscPacket::decode(b"/a\0\0,b\0\0\0\0\0\x09abcd"), Err(OscDecodeError::Truncated));
    }

    #[test]
    fn java_strings() {
        assert_eq!(OscArg::Int(-3).java_string(), "-3");
        assert_eq!(OscArg::Float(1.0).java_string(), "1.0");
        assert_eq!(OscArg::Double(0.5).java_string(), "0.5");
        assert_eq!(OscArg::Bool(true).java_string(), "true");
        assert_eq!(OscArg::Nil.java_string(), "null");
        assert_eq!(OscArg::Long(7).java_string(), "7");
        let message = OscMessage::new("/tempo/raw", vec![OscArg::Float(120.5), OscArg::Str("x".into()), OscArg::Int(1)]);
        assert_eq!(message.log_text(), "/tempo/raw [ 120.5, x, 1 ]");
        assert_eq!(OscMessage::new("/refresh", vec![]).log_text(), "/refresh [  ]");
    }

    #[test]
    fn numbers_and_equality() {
        assert_eq!(OscArg::Int(2).as_number(), Some(2.0));
        assert_eq!(OscArg::Float(0.5).as_number(), Some(0.5));
        assert_eq!(OscArg::Long(-2).as_number(), Some(-2.0));
        assert_eq!(OscArg::Double(0.25).as_number(), Some(0.25));
        assert_eq!(OscArg::Bool(true).as_number(), None);
        assert_eq!(OscArg::Str("1".into()).as_number(), None);
        assert_ne!(OscArg::Int(1), OscArg::Float(1.0));
        assert_ne!(OscArg::Float(0.0), OscArg::Float(-0.0));
        assert_eq!(OscArg::Float(f32::NAN), OscArg::Float(f32::NAN));
    }
}
