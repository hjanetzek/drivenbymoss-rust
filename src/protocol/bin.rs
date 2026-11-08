// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The binary form of the protocol (TYPED_PROTOCOL_PLAN §2.2).
//!
//! A buffer is an 8-byte header (`DBM1`, `u16` major version, `u16` flags) followed by records.
//! A record is 24 bytes, little-endian: `u16 id`, `u8 type`, `u8 op`, `u16 idx[4]`, `u32 len`,
//! an 8-byte inline value; then `len` bytes of payload, padded to 8. The fields of a message or
//! command other than its value are the indices ([`BinIdx`], in the order of the table); the
//! value is inline or in the payload ([`BinValue`]).

use std::borrow::Cow;

use super::types::*;
use crate::backend::Processor;
use crate::daw::clip::{Note, StepInfo};
use crate::daw::constants::{ChannelType, RecordQuantization};

pub const MAGIC: [u8; 4] = *b"DBM1";
/// Layout changes (never expected).
pub const VERSION_MAJOR: u16 = 1;
/// IDs added.
pub const VERSION_MINOR: u16 = 0;
pub const HEADER_SIZE: usize = 8;
pub const RECORD_SIZE: usize = 24;

/// The value types of a record.
pub mod ty {
    pub const NONE: u8 = 0;
    pub const BOOL: u8 = 1;
    pub const I32: u8 = 2;
    pub const F64: u8 = 3;
    pub const TEXT: u8 = 4;
    pub const COLOR: u8 = 5;
    pub const STRUCT: u8 = 6;
    pub const STRUCT_ARRAY: u8 = 7;
    pub const TEXT_LIST: u8 = 8;
    pub const FEATURES: u8 = 9;

    pub const NAMES: &[&str] = &["none", "bool", "i32", "f64", "text", "color", "struct", "struct_array", "text_list", "features"];
}

/// The operations of the value commands (the suffixes of the legacy text).
pub mod op {
    pub const SET: u8 = 0;
    pub const INC: u8 = 1;
    pub const RESET: u8 = 2;
    pub const IMMEDIATELY: u8 = 3;
    pub const TOUCH: u8 = 4;
    pub const INDICATE: u8 = 5;
}

/// An error in a binary buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinError {
    /// Not a buffer of this protocol (magic or major version).
    Header,
    /// A record or its payload does not fit into the buffer.
    Truncated,
    /// The ID is not in the table (skipped by its length).
    UnknownId(u16),
    /// The type, indices or value do not fit the ID.
    Value(u16),
}

// ------------------------------------------------------------------------------------------
// Writing

/// The value of a record while it is encoded.
pub struct ValOut<'w> {
    pub ty: u8,
    pub op: u8,
    pub inline: [u8; 8],
    pub payload: &'w mut Vec<u8>,
}

/// The indices of a record while they are encoded.
#[derive(Debug, Default)]
pub struct IdxOut {
    pub idx: [u16; 4],
    n: usize,
}

impl IdxOut {
    pub fn push(&mut self, value: u16) {
        if let Some(slot) = self.idx.get_mut(self.n) {
            *slot = value;
        }
        self.n += 1;
    }

    /// Add bits to the last slot (a field packed into the slot of the field before it).
    pub fn or_last(&mut self, bits: u16) {
        if let Some(slot) = self.n.checked_sub(1).and_then(|i| self.idx.get_mut(i)) {
            *slot |= bits;
        }
    }
}

/// A buffer of records (reused: [`Writer::clear`] keeps its capacity).
#[derive(Debug, Clone)]
pub struct Writer {
    buf: Vec<u8>,
    scratch: Vec<u8>,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    pub fn new() -> Self {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_MAJOR.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        Writer { buf, scratch: Vec::new() }
    }

    /// Remove the records.
    pub fn clear(&mut self) {
        self.buf.truncate(HEADER_SIZE);
    }

    /// True if there are no records.
    pub fn is_empty(&self) -> bool {
        self.buf.len() <= HEADER_SIZE
    }

    /// The buffer with the header.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Append a record: the indices and the value are written by `encode`.
    pub fn record(&mut self, id: u16, encode: impl FnOnce(&mut IdxOut, &mut ValOut)) {
        self.scratch.clear();
        let mut idx = IdxOut::default();
        let mut val = ValOut { ty: ty::NONE, op: op::SET, inline: [0; 8], payload: &mut self.scratch };
        encode(&mut idx, &mut val);
        let (ty, op, inline) = (val.ty, val.op, val.inline);
        let buf = &mut self.buf;
        buf.extend_from_slice(&id.to_le_bytes());
        buf.push(ty);
        buf.push(op);
        for i in idx.idx {
            buf.extend_from_slice(&i.to_le_bytes());
        }
        buf.extend_from_slice(&(self.scratch.len() as u32).to_le_bytes());
        buf.extend_from_slice(&inline);
        buf.extend_from_slice(&self.scratch);
        buf.resize(buf.len().next_multiple_of(8), 0);
    }
}

// ------------------------------------------------------------------------------------------
// Reading

/// A record of a buffer (the payload borrowed from it).
#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    pub id: u16,
    pub ty: u8,
    pub op: u8,
    pub idx: [u16; 4],
    pub inline: [u8; 8],
    pub payload: &'a [u8],
}

impl Record<'_> {
    pub fn inline_i64(&self) -> i64 {
        i64::from_le_bytes(self.inline)
    }

    pub fn inline_f64(&self) -> f64 {
        f64::from_le_bytes(self.inline)
    }
}

/// The records of a buffer; checks the header.
pub fn records(buf: &[u8]) -> Result<Records<'_>, BinError> {
    if buf.len() < HEADER_SIZE || buf[0..4] != MAGIC || u16::from_le_bytes([buf[4], buf[5]]) != VERSION_MAJOR {
        return Err(BinError::Header);
    }
    Ok(Records { rest: &buf[HEADER_SIZE..] })
}

/// The iterator over the records of a buffer.
pub struct Records<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for Records<'a> {
    type Item = Result<Record<'a>, BinError>;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.rest;
        if b.is_empty() {
            return None;
        }
        if b.len() < RECORD_SIZE {
            self.rest = &[];
            return Some(Err(BinError::Truncated));
        }
        let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let len = u32::from_le_bytes([b[12], b[13], b[14], b[15]]) as usize;
        let end = RECORD_SIZE + len;
        if b.len() < end {
            self.rest = &[];
            return Some(Err(BinError::Truncated));
        }
        let record = Record {
            id: u16_at(0),
            ty: b[2],
            op: b[3],
            idx: [u16_at(4), u16_at(6), u16_at(8), u16_at(10)],
            inline: b[16..24].try_into().expect("8 bytes"),
            payload: &b[RECORD_SIZE..end],
        };
        self.rest = &b[end.next_multiple_of(8).min(b.len())..];
        Some(Ok(record))
    }
}

/// The indices of a record while they are decoded.
pub struct IdxIn {
    idx: [u16; 4],
    n: usize,
}

impl IdxIn {
    pub fn new(idx: [u16; 4]) -> Self {
        IdxIn { idx, n: 0 }
    }

    pub fn take(&mut self) -> Option<u16> {
        let value = self.idx.get(self.n).copied();
        self.n += 1;
        value
    }

    /// The last slot taken (for a field packed into the slot of the field before it).
    pub fn last(&self) -> Option<u16> {
        self.idx.get(self.n.checked_sub(1)?).copied()
    }
}

/// A part of an index slot (metadata for the generated bindings): a field, or a slot of a
/// composite field, in `bits` bits from `shift` (16 and 0 for a whole slot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdxPart {
    pub name: String,
    pub shift: u8,
    pub bits: u8,
    /// An i16 (a whole slot only).
    pub signed: bool,
}

impl IdxPart {
    pub fn whole(name: String, signed: bool) -> Self {
        IdxPart { name, shift: 0, bits: 16, signed }
    }
}

/// An index slot: one part, or several packed fields.
pub type IdxSlot = Vec<IdxPart>;

/// The name of an index slot: the field, or `field_slot` for a field with several slots.
pub fn slot_name(field: &str, slot: &str) -> String {
    if slot.is_empty() {
        field.to_string()
    } else {
        format!("{field}_{slot}")
    }
}

// ------------------------------------------------------------------------------------------
// The traits

/// A field which is written as indices.
pub trait BinIdx: Sized {
    /// The names of its index slots (empty: one slot named as the field).
    const SLOTS: &'static [&'static str] = &[""];
    /// The slots are i16.
    const SIGNED: bool = false;
    fn put(&self, idx: &mut IdxOut);
    fn get(idx: &mut IdxIn) -> Option<Self>;

    /// Append the slots of the field `field` (the metadata of [`Self::put`]).
    fn describe(field: &'static str, slots: &mut Vec<IdxSlot>) {
        slots.extend(Self::SLOTS.iter().map(|s| vec![IdxPart::whole(slot_name(field, s), Self::SIGNED)]));
    }
}

/// The value of a message or command.
pub trait BinValue<'a>: Sized {
    /// The value type of the record (for the generated bindings; a value command varies it).
    const TYPE: u8;
    fn encode(&self, out: &mut ValOut);
    fn decode(record: &Record<'a>) -> Option<Self>;
}

/// A constant value of a command (none in the binary form).
pub trait BinConst {
    fn encode(out: &mut ValOut);
    fn matches(record: &Record<'_>) -> bool;
}

/// An index which is a number (0..65535).
fn put_num(idx: &mut IdxOut, value: usize) {
    idx.push(u16::try_from(value).unwrap_or(u16::MAX));
}

impl BinIdx for usize {
    fn put(&self, idx: &mut IdxOut) {
        put_num(idx, *self);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        idx.take().map(usize::from)
    }
}

/// A signed index (-32768..32767).
impl BinIdx for i32 {
    const SIGNED: bool = true;

    fn put(&self, idx: &mut IdxOut) {
        idx.push(*self as i16 as u16);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        idx.take().map(|v| v as i16 as i32)
    }
}

/// The limit of a shortened text, 0 for the full text.
impl BinIdx for Option<usize> {
    fn put(&self, idx: &mut IdxOut) {
        put_num(idx, self.unwrap_or(0));
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        idx.take().map(|v| (v != 0).then_some(usize::from(v)))
    }
}

/// The 0-based index (-1 for none).
impl BinIdx for Pos1 {
    const SIGNED: bool = true;

    fn put(&self, idx: &mut IdxOut) {
        self.0.put(idx);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        i32::get(idx).map(Pos1)
    }
}

/// Two slots: steps, rows.
impl BinIdx for GridId {
    const SLOTS: &'static [&'static str] = &["steps", "rows"];

    fn put(&self, idx: &mut IdxOut) {
        idx.push(self.steps);
        idx.push(self.rows);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        Some(GridId { steps: idx.take()?, rows: idx.take()? })
    }
}

/// The bits of the [`GridRow`] in the slot which it shares with the [`GridChannel`].
pub const GRID_ROW_BITS: u8 = 7;
/// The bits of the [`GridChannel`] (above the row).
pub const GRID_CHANNEL_BITS: u8 = 4;

/// `channel << 7` (0–15); the [`GridRow`] which follows adds its bits.
impl BinIdx for GridChannel {
    fn put(&self, idx: &mut IdxOut) {
        idx.push((self.0.clamp(0, (1 << GRID_CHANNEL_BITS) - 1) as u16) << GRID_ROW_BITS);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        idx.take().map(|v| GridChannel(i32::from(v >> GRID_ROW_BITS) & ((1 << GRID_CHANNEL_BITS) - 1)))
    }

    fn describe(field: &'static str, slots: &mut Vec<IdxSlot>) {
        slots.push(vec![IdxPart { name: field.to_string(), shift: GRID_ROW_BITS, bits: GRID_CHANNEL_BITS, signed: false }]);
    }
}

/// The low 7 bits of the slot of the [`GridChannel`] before it (0–127); no slot of its own.
impl BinIdx for GridRow {
    const SLOTS: &'static [&'static str] = &[];

    fn put(&self, idx: &mut IdxOut) {
        idx.or_last(self.0.clamp(0, (1 << GRID_ROW_BITS) - 1) as u16);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        idx.last().map(|v| GridRow(i32::from(v) & ((1 << GRID_ROW_BITS) - 1)))
    }

    fn describe(field: &'static str, slots: &mut Vec<IdxSlot>) {
        if let Some(slot) = slots.last_mut() {
            slot.push(IdxPart { name: field.to_string(), shift: 0, bits: GRID_ROW_BITS, signed: false });
        }
    }
}

/// The number of the processor (its position in [`Processor::ALL`]; append only).
impl BinIdx for Processor {
    fn put(&self, idx: &mut IdxOut) {
        put_num(idx, Processor::ALL.iter().position(|p| p == self).unwrap_or(usize::MAX));
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        Processor::ALL.get(usize::from(idx.take()?)).copied()
    }
}

/// kind (0 master, 1 track, 2 pad, 3 layer), processor, index.
impl BinIdx for Chan {
    const SLOTS: &'static [&'static str] = &["kind", "processor", "index"];

    fn describe(field: &'static str, slots: &mut Vec<IdxSlot>) {
        for (slot, signed) in [("kind", false), ("processor", false), ("index", true)] {
            slots.push(vec![IdxPart::whole(slot_name(field, slot), signed)]);
        }
    }

    fn put(&self, idx: &mut IdxOut) {
        let (kind, processor, i) = match *self {
            Chan::Master => (0, Processor::Master, 0),
            Chan::Track { bank, p } => (1, bank, p),
            Chan::Pad { owner, i } => (2, owner, i),
            Chan::Layer { owner, i } => (3, owner, i),
        };
        idx.push(kind);
        processor.put(idx);
        i.put(idx);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        let kind = idx.take()?;
        let processor = Processor::get(idx)?;
        let i = i32::get(idx)?;
        Some(match kind {
            0 => Chan::Master,
            1 => Chan::Track { bank: processor, p: i },
            2 => Chan::Pad { owner: processor, i },
            3 => Chan::Layer { owner: processor, i },
            _ => return None,
        })
    }
}

// ------------------------------------------------------------------------------------------
// Values

fn set_i64(out: &mut ValOut, ty: u8, value: i64) {
    out.ty = ty;
    out.inline = value.to_le_bytes();
}

fn set_f64(out: &mut ValOut, value: f64) {
    out.ty = ty::F64;
    out.inline = value.to_le_bytes();
}

fn set_text(out: &mut ValOut, text: &str) {
    out.ty = ty::TEXT;
    out.payload.extend_from_slice(text.as_bytes());
}

fn is(record: &Record<'_>, ty: u8) -> bool {
    record.ty == ty && record.op == op::SET
}

impl BinValue<'_> for bool {
    const TYPE: u8 = ty::BOOL;

    fn encode(&self, out: &mut ValOut) {
        set_i64(out, ty::BOOL, *self as i64);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        is(record, ty::BOOL).then(|| record.inline_i64() != 0)
    }
}

impl BinValue<'_> for i32 {
    const TYPE: u8 = ty::I32;

    fn encode(&self, out: &mut ValOut) {
        set_i64(out, ty::I32, *self as i64);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        is(record, ty::I32).then(|| record.inline_i64() as i32)
    }
}

impl BinValue<'_> for f64 {
    const TYPE: u8 = ty::F64;

    fn encode(&self, out: &mut ValOut) {
        set_f64(out, *self);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        is(record, ty::F64).then(|| record.inline_f64())
    }
}

impl<'a> BinValue<'a> for &'a str {
    const TYPE: u8 = ty::TEXT;

    fn encode(&self, out: &mut ValOut) {
        set_text(out, self);
    }

    fn decode(record: &Record<'a>) -> Option<Self> {
        if !is(record, ty::TEXT) {
            return None;
        }
        std::str::from_utf8(record.payload).ok()
    }
}

/// `u32 count, (u32 len, bytes)…`.
impl<'a> BinValue<'a> for Vec<&'a str> {
    const TYPE: u8 = ty::TEXT_LIST;

    fn encode(&self, out: &mut ValOut) {
        out.ty = ty::TEXT_LIST;
        out.payload.extend_from_slice(&(self.len() as u32).to_le_bytes());
        for text in self {
            out.payload.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.payload.extend_from_slice(text.as_bytes());
        }
    }

    fn decode(record: &Record<'a>) -> Option<Self> {
        if !is(record, ty::TEXT_LIST) {
            return None;
        }
        let mut rest = record.payload;
        let take_u32 = |rest: &mut &'a [u8]| -> Option<usize> {
            let (head, tail) = rest.split_first_chunk::<4>()?;
            *rest = tail;
            Some(u32::from_le_bytes(*head) as usize)
        };
        let count = take_u32(&mut rest)?;
        let mut list = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let len = take_u32(&mut rest)?;
            if rest.len() < len {
                return None;
            }
            let (text, tail) = rest.split_at(len);
            list.push(std::str::from_utf8(text).ok()?);
            rest = tail;
        }
        Some(list)
    }
}

/// `r, g, b, valid` (0..255): a color of the reports (None: no color).
impl BinValue<'_> for Option<[f64; 3]> {
    const TYPE: u8 = ty::COLOR;

    fn encode(&self, out: &mut ValOut) {
        out.ty = ty::COLOR;
        if let Some(rgb) = self {
            let c = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
            out.inline[..4].copy_from_slice(&[c(rgb[0]), c(rgb[1]), c(rgb[2]), 1]);
        }
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        if !is(record, ty::COLOR) {
            return None;
        }
        let [r, g, b, valid, ..] = record.inline;
        Some((valid != 0).then(|| [f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0]))
    }
}

/// A color of the commands (`RGB(r,g,b)` in the legacy text).
impl BinValue<'_> for Rgb {
    const TYPE: u8 = ty::COLOR;

    fn encode(&self, out: &mut ValOut) {
        out.ty = ty::COLOR;
        let c = |v: i32| v.clamp(0, 255) as u8;
        out.inline[..4].copy_from_slice(&[c(self.0[0]), c(self.0[1]), c(self.0[2]), 1]);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        if !is(record, ty::COLOR) {
            return None;
        }
        let [r, g, b, ..] = record.inline;
        Some(Rgb([r.into(), g.into(), b.into()]))
    }
}

/// The size of a note in a struct array: `f64 start, f64 end, i32 channel, i32 pitch, i32
/// velocity, u32 flags` (bit 0 selected, bit 1 muted).
pub const NOTE_SIZE: usize = 32;

impl BinValue<'_> for Vec<Note> {
    const TYPE: u8 = ty::STRUCT_ARRAY;

    fn encode(&self, out: &mut ValOut) {
        out.ty = ty::STRUCT_ARRAY;
        for n in self {
            out.payload.extend_from_slice(&n.start().to_le_bytes());
            out.payload.extend_from_slice(&n.end().to_le_bytes());
            out.payload.extend_from_slice(&n.channel().to_le_bytes());
            out.payload.extend_from_slice(&n.pitch().to_le_bytes());
            out.payload.extend_from_slice(&n.velocity().to_le_bytes());
            out.payload.extend_from_slice(&(n.is_selected() as u32 | ((n.is_muted() as u32) << 1)).to_le_bytes());
        }
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        if !is(record, ty::STRUCT_ARRAY) || !record.payload.len().is_multiple_of(NOTE_SIZE) {
            return None;
        }
        let notes = record
            .payload
            .as_chunks::<NOTE_SIZE>()
            .0
            .iter()
            .map(|c| {
                let f = |i: usize| f64::from_le_bytes(c[i..i + 8].try_into().expect("8 bytes"));
                let n = |i: usize| i32::from_le_bytes(c[i..i + 4].try_into().expect("4 bytes"));
                let flags = n(28);
                Note::new(flags & 1 != 0, flags & 2 != 0, f(0), f(8), n(16), n(20), n(24))
            })
            .collect();
        Some(notes)
    }
}

/// The 24 values of [`StepInfo::to_values`] as doubles; no payload: an empty step.
impl BinValue<'_> for Option<StepInfo> {
    const TYPE: u8 = ty::STRUCT;

    fn encode(&self, out: &mut ValOut) {
        out.ty = ty::STRUCT;
        if let Some(step) = self {
            for v in step.to_values() {
                out.payload.extend_from_slice(&v.to_le_bytes());
            }
        }
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        if !is(record, ty::STRUCT) || !record.payload.len().is_multiple_of(8) {
            return None;
        }
        let values: Vec<f64> = record.payload.as_chunks::<8>().0.iter().map(|c| f64::from_le_bytes(*c)).collect();
        Some(StepInfo::from_values(&values))
    }
}

/// The channel type as its number (the order of [`ChannelType::from_name`]).
impl BinValue<'_> for ChannelType {
    const TYPE: u8 = ty::I32;

    fn encode(&self, out: &mut ValOut) {
        set_i64(out, ty::I32, CHANNEL_TYPES.iter().position(|t| t == self).unwrap_or(0) as i64);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        CHANNEL_TYPES.get(usize::try_from(i32::decode(record)?).ok()?).copied()
    }
}

/// The channel types in the order of their numbers in the binary form (append only).
pub const CHANNEL_TYPES: &[ChannelType] = &[
    ChannelType::Unknown,
    ChannelType::Audio,
    ChannelType::Instrument,
    ChannelType::Hybrid,
    ChannelType::Group,
    ChannelType::GroupOpen,
    ChannelType::Effect,
    ChannelType::Master,
    ChannelType::Layer,
    ChannelType::Cue,
];

/// The record quantization as its index in [`RecordQuantization::ALL`].
impl BinValue<'_> for RecordQuantization {
    const TYPE: u8 = ty::I32;

    fn encode(&self, out: &mut ValOut) {
        set_i64(out, ty::I32, RecordQuantization::ALL.iter().position(|q| q == self).unwrap_or(0) as i64);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        RecordQuantization::ALL.get(usize::try_from(i32::decode(record)?).ok()?).copied()
    }
}

/// A value command: the operation in `op`, the value as a double (set, inc, immediately), a
/// bool (touch, indicate) or none (reset).
impl BinValue<'_> for ValueOp {
    const TYPE: u8 = ty::F64;

    fn encode(&self, out: &mut ValOut) {
        match *self {
            ValueOp::Set(v) => set_f64(out, v),
            ValueOp::Inc(v) => {
                set_f64(out, v);
                out.op = op::INC;
            }
            ValueOp::Reset => out.op = op::RESET,
            ValueOp::Immediately(v) => {
                set_f64(out, v);
                out.op = op::IMMEDIATELY;
            }
            ValueOp::Touch(v) => {
                set_i64(out, ty::BOOL, v as i64);
                out.op = op::TOUCH;
            }
            ValueOp::Indicate(v) => {
                set_i64(out, ty::BOOL, v as i64);
                out.op = op::INDICATE;
            }
        }
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        Some(match (record.op, record.ty) {
            (op::SET, ty::F64) => ValueOp::Set(record.inline_f64()),
            (op::INC, ty::F64) => ValueOp::Inc(record.inline_f64()),
            (op::RESET, ty::NONE) => ValueOp::Reset,
            (op::IMMEDIATELY, ty::F64) => ValueOp::Immediately(record.inline_f64()),
            (op::TOUCH, ty::BOOL) => ValueOp::Touch(record.inline_i64() != 0),
            (op::INDICATE, ty::BOOL) => ValueOp::Indicate(record.inline_i64() != 0),
            _ => return None,
        })
    }
}

/// Two i32 in the inline value (low: first).
fn set_pair(out: &mut ValOut, a: i32, b: i32) {
    out.ty = ty::STRUCT;
    out.inline[..4].copy_from_slice(&a.to_le_bytes());
    out.inline[4..].copy_from_slice(&b.to_le_bytes());
}

fn get_pair(record: &Record<'_>) -> Option<(i32, i32)> {
    if !is(record, ty::STRUCT) || !record.payload.is_empty() {
        return None;
    }
    let i = record.inline;
    Some((i32::from_le_bytes([i[0], i[1], i[2], i[3]]), i32::from_le_bytes([i[4], i[5], i[6], i[7]])))
}

/// numerator, denominator (inline struct).
impl BinValue<'_> for Signature {
    const TYPE: u8 = ty::STRUCT;

    fn encode(&self, out: &mut ValOut) {
        set_pair(out, self.numerator, self.denominator);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        get_pair(record).map(|(numerator, denominator)| Signature { numerator, denominator })
    }
}

/// track, slot (inline struct).
impl BinValue<'_> for SlotRef {
    const TYPE: u8 = ty::STRUCT;

    fn encode(&self, out: &mut ValOut) {
        set_pair(out, self.track, self.slot);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        get_pair(record).map(|(track, slot)| SlotRef { track, slot })
    }
}

/// `i32 velocity, u32 0, f64 duration` (payload struct).
impl BinValue<'_> for GridNote {
    const TYPE: u8 = ty::STRUCT;

    fn encode(&self, out: &mut ValOut) {
        out.ty = ty::STRUCT;
        out.payload.extend_from_slice(&self.velocity.to_le_bytes());
        out.payload.extend_from_slice(&0u32.to_le_bytes());
        out.payload.extend_from_slice(&self.duration.to_le_bytes());
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        if !is(record, ty::STRUCT) || record.payload.len() != 16 {
            return None;
        }
        let p = record.payload;
        Some(GridNote {
            velocity: i32::from_le_bytes(p[0..4].try_into().expect("4 bytes")),
            duration: f64::from_le_bytes(p[8..16].try_into().expect("8 bytes")),
        })
    }
}

/// `u32 attribute, u32 0, f64 value` (payload struct).
impl BinValue<'_> for StepValue {
    const TYPE: u8 = ty::STRUCT;

    fn encode(&self, out: &mut ValOut) {
        out.ty = ty::STRUCT;
        out.payload.extend_from_slice(&(self.attr as u32).to_le_bytes());
        out.payload.extend_from_slice(&0u32.to_le_bytes());
        out.payload.extend_from_slice(&self.value.to_le_bytes());
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        if !is(record, ty::STRUCT) || record.payload.len() != 16 {
            return None;
        }
        let p = record.payload;
        let attr = *StepAttr::ALL.get(u32::from_le_bytes(p[0..4].try_into().expect("4 bytes")) as usize)?;
        Some(StepValue { attr, value: f64::from_le_bytes(p[8..16].try_into().expect("8 bytes")) })
    }
}

impl BinValue<'_> for IntText {
    const TYPE: u8 = ty::I32;

    fn encode(&self, out: &mut ValOut) {
        self.0.encode(out);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        i32::decode(record).map(IntText)
    }
}

impl BinValue<'_> for Pos1 {
    const TYPE: u8 = ty::I32;

    fn encode(&self, out: &mut ValOut) {
        self.0.encode(out);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        i32::decode(record).map(Pos1)
    }
}

impl BinValue<'_> for super::legacy::Enable {
    const TYPE: u8 = ty::BOOL;

    fn encode(&self, out: &mut ValOut) {
        self.0.encode(out);
    }

    fn decode(record: &Record<'_>) -> Option<Self> {
        bool::decode(record).map(super::legacy::Enable)
    }
}

impl BinConst for super::legacy::One {
    fn encode(_: &mut ValOut) {}

    fn matches(record: &Record<'_>) -> bool {
        is(record, ty::NONE)
    }
}

impl BinConst for super::legacy::Delay {
    fn encode(_: &mut ValOut) {}

    fn matches(record: &Record<'_>) -> bool {
        is(record, ty::NONE)
    }
}

/// A message or command without a value.
pub fn no_value(record: &Record<'_>) -> bool {
    is(record, ty::NONE)
}

/// Text of a value for diagnostics.
pub fn type_name(ty: u8) -> Cow<'static, str> {
    ty::NAMES.get(usize::from(ty)).map_or_else(|| Cow::Owned(format!("type {ty}")), |n| Cow::Borrowed(*n))
}

// ------------------------------------------------------------------------------------------
// The text conversions (logs, recordings, tests, tools)

/// The kinds of the legacy command lines (as the recordings name the C callbacks).
const KINDS: &[&str] = &["noArg", "int", "double", "string", "strings", "delay", "enable"];

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t")
}

fn unescape(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some(other) => result.push(other),
            None => {}
        }
    }
    result
}

/// The legacy line of a command: `<kind>\t<processor>\t<command>[\t<value>…]` with the kind
/// of the C callback (`noArg`, `int`, `double`, `string`, `strings`, `delay`, `enable`); tabs,
/// newlines and backslashes in the fields escaped (as in the recordings).
pub fn command_line(cmd: &super::Cmd<'_>) -> String {
    use super::Arg;
    let legacy = cmd.legacy();
    let (p, c) = (legacy.processor.id(), escape(&legacy.command));
    match legacy.arg {
        Arg::None => format!("noArg\t{p}\t{c}"),
        Arg::Int(v) => format!("int\t{p}\t{c}\t{v}"),
        Arg::Double(v) => format!("double\t{p}\t{c}\t{v}"),
        Arg::Str(v) => format!("string\t{p}\t{c}\t{}", escape(&v)),
        Arg::Strs(v) => {
            let mut line = format!("strings\t{p}\t{c}");
            for value in v {
                line.push('\t');
                line.push_str(&escape(value));
            }
            line
        }
        Arg::Delay => format!("delay\t{p}"),
        Arg::Enable(v) => format!("enable\t{p}\t{}", v as i32),
    }
}

/// The text of a buffer: one line per record, the report line of a message
/// ([`super::Msg::to_text`]) or the legacy line of a command ([`command_line`]). A record
/// which cannot be decoded gives a line `# <error>`.
pub fn to_text(buf: &[u8]) -> String {
    let records = match records(buf) {
        Ok(records) => records,
        Err(error) => return format!("# {error:?}"),
    };
    let mut lines = Vec::new();
    for record in records {
        let line = record.and_then(|record| {
            if record.id & 0x8000 == 0 {
                super::Msg::decode_record(&record).map(|msg| msg.to_text())
            } else {
                super::Cmd::decode_record(&record).map(|cmd| command_line(&cmd))
            }
        });
        lines.push(line.unwrap_or_else(|error| format!("# {error:?}")));
    }
    lines.join("\n")
}

/// The buffer of a text: report lines and legacy command lines (see [`to_text`]). Lines which
/// are empty, start with `#` or are not in the tables are skipped; `log` receives the errors of
/// the report values.
pub fn from_text(text: &str, log: super::text::rules::Log) -> Writer {
    let mut w = Writer::new();
    for line in text.split('\n') {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 2 && KINDS.contains(&fields[0]) {
            if let Some(processor) = Processor::from_id(fields[1]) {
                let command = fields.get(2).map(|c| unescape(c)).unwrap_or_default();
                let values: Vec<String> = fields.iter().skip(3).map(|v| unescape(v)).collect();
                let first = values.first().map_or("", String::as_str);
                use super::Arg;
                let arg = match fields[0] {
                    "noArg" => Some(Arg::None),
                    "int" => first.parse().ok().map(Arg::Int),
                    "double" => first.parse().ok().map(Arg::Double),
                    "string" => Some(Arg::Str(Cow::Borrowed(first))),
                    "strings" => Some(Arg::Strs(values.iter().map(String::as_str).collect())),
                    "delay" => Some(Arg::Delay),
                    _ => Some(Arg::Enable(fields.get(2) == Some(&"1"))),
                };
                let command = if matches!(fields[0], "delay" | "enable") { String::new() } else { command };
                if let Some(arg) = arg {
                    if let Some(cmd) = super::Cmd::from_legacy(processor, &command, &arg) {
                        cmd.encode(&mut w);
                    }
                }
            }
            continue;
        }
        let (address, value) = line.split_once(' ').unwrap_or((line, ""));
        if let Ok(msg) = super::Msg::from_text(address, value, log) {
            msg.encode(&mut w);
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Cmd, CmdId, Msg, MsgId};

    #[test]
    fn every_message_round_trips_through_its_record() {
        for id in MsgId::ALL {
            for k in 0..8 {
                let msg = id.sample(k);
                let mut w = Writer::new();
                msg.encode(&mut w);
                assert_eq!(w.as_bytes().len() % 8, 0);
                let all: Vec<_> = records(w.as_bytes()).unwrap().collect::<Result<_, _>>().unwrap();
                assert_eq!(all.len(), 1);
                assert_eq!(all[0].id, *id as u16);
                assert_eq!(all[0].ty, id.value_type(), "{id:?}");
                assert_eq!(Msg::decode_record(&all[0]), Ok(msg.clone()), "{id:?}");
                // text → record → text
                let line = msg.to_text();
                let (address, value) = line.split_once(' ').unwrap_or((&line, ""));
                let parsed = Msg::from_text(address, value, &mut |_| {}).unwrap();
                let mut w = Writer::new();
                parsed.encode(&mut w);
                let record = records(w.as_bytes()).unwrap().next().unwrap().unwrap();
                assert_eq!(Msg::decode_record(&record).unwrap().to_text(), line);
            }
        }
    }

    #[test]
    fn every_command_round_trips_through_its_record() {
        for id in CmdId::ALL {
            for k in 0..8 {
                let cmd = id.sample(k);
                let mut w = Writer::new();
                cmd.encode(&mut w);
                let all: Vec<_> = records(w.as_bytes()).unwrap().collect::<Result<_, _>>().unwrap();
                assert_eq!(all.len(), 1);
                assert_eq!(Cmd::decode_record(&all[0]), Ok(cmd.clone()), "{id:?}");
                assert_eq!(Cmd::decode_record(&all[0]).unwrap().legacy(), cmd.legacy());
            }
        }
    }

    /// The raindrops view's grid (1536 × 128, the Launchpads and the JAM) and the largest channel
    /// and row: the grid takes two slots, the channel and the row share one.
    #[test]
    fn large_grids_and_packed_channel_and_row() {
        use crate::protocol::{GridChannel, GridId, GridRow};
        let grid = GridId { steps: 1536, rows: 128 };
        let msg = Msg::GridStep { grid, channel: GridChannel(15), row: GridRow(127), step: 1535, value: None };
        let mut w = Writer::new();
        msg.encode(&mut w);
        let record = records(w.as_bytes()).unwrap().next().unwrap().unwrap();
        assert_eq!(record.idx, [1536, 128, 15 << 7 | 127, 1535]);
        assert_eq!(Msg::decode_record(&record), Ok(msg.clone()));
        assert_eq!(to_text(w.as_bytes()), "/clip/grid/1536x128/step/15/1535/127 0");
        assert_eq!(from_text(&msg.to_text(), &mut |_| {}).as_bytes(), w.as_bytes());

        let cmd = Cmd::GridStepToggle { grid, channel: GridChannel(15), note: GridRow(127), step: 1535, velocity: 100 };
        let mut w = Writer::new();
        cmd.encode(&mut w);
        let record = records(w.as_bytes()).unwrap().next().unwrap().unwrap();
        assert_eq!(record.idx, [1536, 128, 15 << 7 | 127, 1535]);
        assert_eq!(Cmd::decode_record(&record), Ok(cmd.clone()));
        assert_eq!(to_text(w.as_bytes()), "int\tclip\tgrid/1536x128/step/15/1535/127/toggle\t100");
        let slots = CmdId::GridStepToggle.index_slots();
        assert_eq!(slots.len(), 4);
        assert_eq!(slots[2].iter().map(|p| (p.name.as_str(), p.shift, p.bits)).collect::<Vec<_>>(), [("channel", 7, 4), ("note", 0, 7)]);
    }

    #[test]
    fn at_most_four_indices() {
        for id in MsgId::ALL {
            assert!(id.index_slots().len() <= 4, "{id:?}");
        }
        for id in CmdId::ALL {
            assert!(id.index_slots().len() <= 4, "{id:?}");
        }
    }

    #[test]
    fn text_conversions() {
        let text = "/track/3/name Synth\n/play 1\nnoArg\ttrack\t3/select\ndouble\ttrack\t3/volume\t0.5\nstrings\ttrack\taddTrack\ta\tb c\nenable\tsession\t1\ndelay\ttrack\nstring\tclip\tname\tx\\ty";
        let w = from_text(text, &mut |_| {});
        assert_eq!(to_text(w.as_bytes()), text);
        // Not in the tables
        let w = from_text("/unknown 1\nnoArg\ttrack\tunknown", &mut |_| {});
        assert!(w.is_empty());
    }

    #[test]
    fn several_records_unknown_ids_and_errors() {
        let mut w = Writer::new();
        Msg::ChannelName { ch: crate::protocol::Ch::Track(3), value: "Synth" }.encode(&mut w);
        w.record(0x7FFF, |_, out| out.payload.extend_from_slice(b"skipped"));
        Msg::Play { value: true }.encode(&mut w);
        let decoded: Vec<_> = records(w.as_bytes()).unwrap().map(|r| r.and_then(|r| Msg::decode_record(&r))).collect();
        assert_eq!(decoded[0], Ok(Msg::ChannelName { ch: crate::protocol::Ch::Track(3), value: "Synth" }));
        assert_eq!(decoded[1], Err(BinError::UnknownId(0x7FFF)));
        assert_eq!(decoded[2], Ok(Msg::Play { value: true }));
        assert_eq!(records(b"DBM2\x01\x00\x00\x00").err(), Some(BinError::Header));
        let mut bytes = w.as_bytes().to_vec();
        bytes.truncate(HEADER_SIZE + 20);
        assert!(matches!(records(&bytes).unwrap().next(), Some(Err(BinError::Truncated))));
        // The wrong value type
        let mut w = Writer::new();
        w.record(MsgId::Play as u16, |_, out| out.ty = ty::F64);
        let record = records(w.as_bytes()).unwrap().next().unwrap().unwrap();
        assert_eq!(Msg::decode_record(&record), Err(BinError::Value(MsgId::Play as u16)));
    }
}
