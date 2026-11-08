// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The legacy text form of the commands: `(processor, command, value)` as the C callbacks
//! `process_no_arg`, `process_int_arg`, … deliver them (HOST_BRIDGE_GUIDE §4).
//!
//! The command table ([`super::cmd`]) describes each command by a processor, a template of path
//! segments and a value; the traits here turn the typed fields into the segments and the value of
//! the legacy form and back:
//! - [`Field`]: a path segment (or several, or none), e.g. an index or a name;
//! - [`ProcField`]: a field which selects the processor (an owner such as `device`/`primary`);
//! - [`Value`]: the value of the command, with its legacy callback kind, e.g. a bool sent with
//!   `process_int_arg`; [`ConstValue`] for the commands which send a constant.

use std::borrow::Cow;
use std::fmt::Display;

use crate::backend::Processor;

/// The value of a command in its legacy form: the C callback which carries it.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg<'a> {
    /// `process_no_arg`.
    None,
    /// `process_int_arg`.
    Int(i32),
    /// `process_double_arg`.
    Double(f64),
    /// `process_string_arg`.
    Str(Cow<'a, str>),
    /// `process_string_args`.
    Strs(Vec<&'a str>),
    /// `delay_updates` (the command is empty).
    Delay,
    /// `enable_updates` (the command is empty).
    Enable(bool),
}

/// A command in its legacy form.
#[derive(Debug, Clone, PartialEq)]
pub struct Legacy<'a> {
    pub processor: Processor,
    /// The command: the path segments joined with `/` (may be empty).
    pub command: String,
    pub arg: Arg<'a>,
}

/// Builds the command text from the path segments.
#[derive(Debug, Default)]
pub struct PathWriter {
    text: String,
}

impl PathWriter {
    pub fn new() -> Self {
        PathWriter { text: String::new() }
    }

    /// Append a segment (a literal may contain `/`, e.g. `user/param/0/value`).
    pub fn seg(&mut self, segment: impl Display) {
        use std::fmt::Write;
        if !self.text.is_empty() {
            self.text.push('/');
        }
        let _ = write!(self.text, "{segment}");
    }

    pub fn finish(self) -> String {
        self.text
    }
}

/// Reads the path segments of a command text (the inverse of [`PathWriter`]).
#[derive(Debug, Clone)]
pub struct PathReader<'a> {
    rest: Option<&'a str>,
}

impl<'a> PathReader<'a> {
    pub fn new(command: &'a str) -> Self {
        PathReader { rest: if command.is_empty() { None } else { Some(command) } }
    }

    /// The next segment without consuming it.
    pub fn peek(&self) -> Option<&'a str> {
        self.rest.map(|rest| rest.split_once('/').map_or(rest, |(segment, _)| segment))
    }

    /// Consume and return the next segment.
    pub fn next_segment(&mut self) -> Option<&'a str> {
        let rest = self.rest?;
        match rest.split_once('/') {
            Some((segment, tail)) => {
                self.rest = Some(tail);
                Some(segment)
            }
            None => {
                self.rest = None;
                Some(rest)
            }
        }
    }

    /// Consume the segments of the literal (which may contain `/`); false if they differ.
    pub fn literal(&mut self, literal: &str) -> bool {
        literal.split('/').all(|expected| self.next_segment() == Some(expected))
    }

    /// Consume the segments of an optional literal if they follow.
    pub fn optional(&mut self, literal: &str) {
        let mut probe = self.clone();
        if probe.literal(literal) {
            *self = probe;
        }
    }

    /// Parse the next segment.
    pub fn parse<T: std::str::FromStr>(&mut self) -> Option<T> {
        self.next_segment()?.parse().ok()
    }

    pub fn is_empty(&self) -> bool {
        self.rest.is_none()
    }
}

/// A field of a command which is written into the command text.
pub trait Field: Sized {
    fn write(&self, w: &mut PathWriter);
    /// Read the field; `processor` is the processor of the command being decoded.
    fn read(processor: Processor, r: &mut PathReader<'_>) -> Option<Self>;
}

/// A field which selects the processor of a command.
pub trait ProcField: Sized {
    fn processor(&self) -> Processor;
    /// The field for a processor if the field is not part of the command text.
    fn from_processor(_processor: Processor) -> Option<Self> {
        None
    }
}

/// The value of a command. It may also append segments (the suffixes of [`super::ValueOp`]).
pub trait Value<'a>: Sized {
    fn encode(&self, w: &mut PathWriter) -> Arg<'_>;
    fn decode(r: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self>;
}

/// A constant value which the command always sends (e.g. `repeat` sends 1); no field.
pub trait ConstValue {
    fn encode() -> Arg<'static>;
    fn matches(arg: &Arg<'_>) -> bool;
}

// ------------------------------------------------------------------------------------------
// Fields

impl Field for i32 {
    fn write(&self, w: &mut PathWriter) {
        w.seg(self);
    }

    fn read(_: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        // Digits only (with a sign), as the indices are written
        let segment = r.next_segment()?;
        if segment.is_empty() || segment.starts_with('+') {
            return None;
        }
        segment.parse().ok()
    }
}

impl Field for usize {
    fn write(&self, w: &mut PathWriter) {
        w.seg(self);
    }

    fn read(_: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        let segment = r.next_segment()?;
        if segment.is_empty() || segment.starts_with('+') {
            return None;
        }
        segment.parse().ok()
    }
}

/// A processor as a segment, e.g. `bank track` or `browser replace/drum/pad/36`.
impl Field for Processor {
    fn write(&self, w: &mut PathWriter) {
        w.seg(self.id());
    }

    fn read(_: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        Processor::from_id(r.next_segment()?)
    }
}

impl ProcField for Processor {
    fn processor(&self) -> Processor {
        *self
    }

    fn from_processor(processor: Processor) -> Option<Self> {
        Some(processor)
    }
}

// ------------------------------------------------------------------------------------------
// Values

/// Booleans are sent with `process_int_arg` (0 or 1).
impl<'a> Value<'a> for bool {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Int(if *self { 1 } else { 0 })
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Int(0) => Some(false),
            Arg::Int(1) => Some(true),
            _ => None,
        }
    }
}

impl<'a> Value<'a> for i32 {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Int(*self)
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Int(v) => Some(*v),
            _ => None,
        }
    }
}

impl<'a> Value<'a> for f64 {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Double(*self)
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Double(v) => Some(*v),
            _ => None,
        }
    }
}

impl<'a> Value<'a> for &'a str {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Str(Cow::Borrowed(self))
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Str(v) => Some(v),
            _ => None,
        }
    }
}

impl<'a> Value<'a> for Vec<&'a str> {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Strs(self.clone())
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Strs(v) => Some(v.clone()),
            _ => None,
        }
    }
}

/// The constant 1 sent with `process_int_arg` (`repeat`, `toggleMultiSelect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct One;

impl ConstValue for One {
    fn encode() -> Arg<'static> {
        Arg::Int(1)
    }

    fn matches(arg: &Arg<'_>) -> bool {
        *arg == Arg::Int(1)
    }
}

/// The `delay_updates` callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Delay;

impl ConstValue for Delay {
    fn encode() -> Arg<'static> {
        Arg::Delay
    }

    fn matches(arg: &Arg<'_>) -> bool {
        *arg == Arg::Delay
    }
}

/// The value of the `enable_updates` callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enable(pub bool);

impl<'a> Value<'a> for Enable {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Enable(self.0)
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Enable(v) => Some(Enable(*v)),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------------------------------
// Samples for the round-trip test

/// Values of a type for the round-trip test of the command table.
#[cfg(test)]
pub trait Sample: Sized {
    fn samples() -> Vec<Self>;
}

#[cfg(test)]
impl Sample for i32 {
    fn samples() -> Vec<Self> {
        vec![3, 0, 17, 127]
    }
}

#[cfg(test)]
impl Sample for usize {
    fn samples() -> Vec<Self> {
        vec![2, 0, 7]
    }
}

#[cfg(test)]
impl Sample for bool {
    fn samples() -> Vec<Self> {
        vec![true, false]
    }
}

#[cfg(test)]
impl Sample for f64 {
    fn samples() -> Vec<Self> {
        vec![0.25, -1.5, 0.0, 120.000001]
    }
}

#[cfg(test)]
impl Sample for &'static str {
    fn samples() -> Vec<Self> {
        vec!["Synth", "", "a b/c"]
    }
}

#[cfg(test)]
impl Sample for Vec<&'static str> {
    fn samples() -> Vec<Self> {
        vec![vec!["INSTRUMENT", "Name", "RGB(1,2,3)", "dev"], vec![]]
    }
}

#[cfg(test)]
impl Sample for Enable {
    fn samples() -> Vec<Self> {
        vec![Enable(true), Enable(false)]
    }
}

#[cfg(test)]
impl Sample for Processor {
    fn samples() -> Vec<Self> {
        vec![Processor::Device, Processor::Primary, Processor::Drum, Processor::Eq, Processor::Project]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_reader_reads_segments_and_literals() {
        let mut r = PathReader::new("3/user/param/0/value");
        assert_eq!(r.peek(), Some("3"));
        assert_eq!(r.parse::<i32>(), Some(3));
        assert!(r.literal("user/param/0"));
        assert_eq!(r.next_segment(), Some("value"));
        assert!(r.is_empty());
        assert!(PathReader::new("").is_empty());
    }

    #[test]
    fn path_writer_joins_segments() {
        let mut w = PathWriter::new();
        w.seg("user/param");
        w.seg(3);
        assert_eq!(w.finish(), "user/param/3");
        assert_eq!(PathWriter::new().finish(), "");
    }
}
