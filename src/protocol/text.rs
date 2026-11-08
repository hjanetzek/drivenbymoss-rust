// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The text form of the reports of the host (`<address> <value>`, HOST_BRIDGE_GUIDE §3) and the
//! pieces which the table of [`super::msg`] is built from (TYPED_PROTOCOL_PLAN §2.6):
//! - [`Seg`]: a typed field of an address (one or several path segments), with its [`Pat`]terns
//!   for the trie;
//! - [`rules`]: the lenient value parsers of the former `parser.rs` (each with its rule for an
//!   invalid value: drop the message or use a default);
//! - [`TextValue`]: the text of a typed value (the inverse of the rules, for the round trip and
//!   the conversions of step 1);
//! - [`Trie`]: the path matching, built from the patterns of the table.

use std::collections::HashMap;
use std::marker::PhantomData;

use super::legacy::{PathReader, PathWriter};
use super::types::{GridId, Pos1};
use crate::daw::clip::{Note, StepInfo};
use crate::daw::constants::{ChannelType, RecordQuantization};

/// A segment pattern of an address template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pat {
    /// A literal segment.
    Lit(&'static str),
    /// An index: digits only.
    Num,
    /// A grid ID: `<digits>x<digits>`.
    Grid,
}

/// A typed field of an address: one or several path segments (or none).
pub trait Seg: Sized {
    fn write(&self, w: &mut PathWriter);
    fn read(r: &mut PathReader<'_>) -> Option<Self>;
    /// The alternative segment sequences of the field (for the trie).
    fn patterns() -> Vec<Vec<Pat>>;
}

/// An index: digits only (the former `parse_index`).
impl Seg for usize {
    fn write(&self, w: &mut PathWriter) {
        w.seg(self);
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        let segment = r.next_segment()?;
        if !is_digits(segment) {
            return None;
        }
        segment.parse().ok()
    }

    fn patterns() -> Vec<Vec<Pat>> {
        vec![vec![Pat::Num]]
    }
}

/// An optional trailing index: the limit of a shortened text (`…/str/<limit>`).
impl Seg for Option<usize> {
    fn write(&self, w: &mut PathWriter) {
        if let Some(limit) = self {
            w.seg(limit);
        }
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        if r.is_empty() {
            return Some(None);
        }
        usize::read(r).map(Some)
    }

    fn patterns() -> Vec<Vec<Pat>> {
        vec![vec![], vec![Pat::Num]]
    }
}

/// A 1-based number in the address (`/device/sibling/<n>/…`), 0-based in the field.
impl Seg for Pos1 {
    fn write(&self, w: &mut PathWriter) {
        w.seg(self.0 + 1);
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        let n: usize = Seg::read(r)?;
        Some(Pos1(i32::try_from(n).ok()? - 1))
    }

    fn patterns() -> Vec<Vec<Pat>> {
        vec![vec![Pat::Num]]
    }
}

impl Seg for GridId {
    fn write(&self, w: &mut PathWriter) {
        w.seg(format_args!("{}x{}", self.steps, self.rows));
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        let (steps, rows) = r.next_segment()?.split_once('x')?;
        Some(GridId { steps: steps.parse().ok()?, rows: rows.parse().ok()? })
    }

    fn patterns() -> Vec<Vec<Pat>> {
        vec![vec![Pat::Grid]]
    }
}

pub(crate) fn is_digits(segment: &str) -> bool {
    !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit())
}

fn is_grid(segment: &str) -> bool {
    segment.split_once('x').is_some_and(|(steps, rows)| is_digits(steps) && is_digits(rows))
}

// ------------------------------------------------------------------------------------------
// Building the patterns of an entry (used by the `messages!` macro)

/// Append a literal (which may contain `/`) to all alternatives.
pub(crate) fn push_lit(acc: &mut [Vec<Pat>], literal: &'static str) {
    for alternative in acc.iter_mut() {
        alternative.extend(literal.split('/').map(Pat::Lit));
    }
}

/// An optional literal: every alternative with and without it.
pub(crate) fn push_opt_lit(acc: &mut Vec<Vec<Pat>>, literal: &'static str) {
    let with: Vec<Vec<Pat>> = acc.iter().map(|a| a.iter().copied().chain(literal.split('/').map(Pat::Lit)).collect()).collect();
    acc.extend(with);
}

/// Append the alternatives of a field to all alternatives.
pub(crate) fn push_field<T: Seg>(acc: &mut Vec<Vec<Pat>>, _: &PhantomData<T>) {
    let field = T::patterns();
    *acc = acc.iter().flat_map(|a| field.iter().map(move |f| a.iter().chain(f).copied().collect())).collect();
}

// ------------------------------------------------------------------------------------------
// The trie

/// A node of the path trie.
#[derive(Debug)]
struct Node<I> {
    lits: HashMap<&'static str, Node<I>>,
    num: Option<Box<Node<I>>>,
    grid: Option<Box<Node<I>>>,
    /// The entries whose template ends here, in the order of the table.
    ids: Vec<I>,
}

impl<I> Default for Node<I> {
    fn default() -> Self {
        Node { lits: HashMap::new(), num: None, grid: None, ids: Vec::new() }
    }
}

/// The path matching of the reports: a trie over the path segments, built from the patterns of
/// the table. A path leads to the entries which can decode it (one in a consistent table).
#[derive(Debug)]
pub struct Trie<I> {
    root: Node<I>,
}

impl<I: Copy> Trie<I> {
    pub fn new(entries: impl IntoIterator<Item = (I, Vec<Vec<Pat>>)>) -> Self {
        let mut root = Node::default();
        for (id, patterns) in entries {
            for pattern in patterns {
                let mut node = &mut root;
                for pat in pattern {
                    node = match pat {
                        Pat::Lit(literal) => node.lits.entry(literal).or_default(),
                        Pat::Num => node.num.get_or_insert_with(Default::default),
                        Pat::Grid => node.grid.get_or_insert_with(Default::default),
                    };
                }
                node.ids.push(id);
            }
        }
        Trie { root }
    }

    /// The entries for a path (segments separated by `/`, without the leading `/`).
    pub fn lookup(&self, path: &str) -> &[I] {
        let mut node = &self.root;
        for segment in path.split('/') {
            let next = match node.lits.get(segment) {
                Some(next) => Some(next),
                None if is_digits(segment) => node.num.as_deref(),
                None if is_grid(segment) => node.grid.as_deref(),
                None => None,
            };
            match next {
                Some(next) => node = next,
                None => return &[],
            }
        }
        &node.ids
    }

    /// All paths with more than one entry (for the consistency test of the table).
    #[cfg(test)]
    pub fn ambiguous(&self) -> Vec<Vec<I>> {
        fn walk<I: Copy>(node: &Node<I>, out: &mut Vec<Vec<I>>) {
            if node.ids.len() > 1 {
                out.push(node.ids.clone());
            }
            node.lits.values().chain(node.num.as_deref()).chain(node.grid.as_deref()).for_each(|n| walk(n, out));
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }
}

// ------------------------------------------------------------------------------------------
// The values

/// Why a report line did not give a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miss {
    /// The address is not in the table.
    Path,
    /// The address is known, but the value is invalid and the rule of the entry drops the
    /// message (as the former parser ignored it).
    Value,
}

/// The value parsers of the reports. Each keeps the rule of the former `parser.rs` for its
/// fields; `None` drops the message ([`Miss::Value`]).
pub mod rules {
    use super::*;

    /// The error log of the parser (the host's error output).
    pub type Log<'l> = &'l mut dyn FnMut(&str);

    /// A double (the former `parse_double`); invalid: dropped.
    pub fn f64(text: &str, _: Log) -> Option<f64> {
        text.trim().parse::<f64>().ok()
    }

    /// An int, or a double truncated to an int (the former `parse_int`); invalid: dropped.
    pub fn int(text: &str, _: Log) -> Option<i32> {
        let v = text.trim();
        v.parse::<i32>().ok().or_else(|| v.parse::<f64>().ok().map(|d| d as i32))
    }

    /// An int with a default for an invalid value.
    pub fn int_or(text: &str, log: Log, default: i32) -> Option<i32> {
        Some(int(text, log).unwrap_or(default))
    }

    /// A double truncated to an int (`v as i32`); invalid: dropped.
    pub fn f64_as_int(text: &str, log: Log) -> Option<i32> {
        f64(text, log).map(|v| v as i32)
    }

    /// A double, 0 for an invalid value.
    pub fn f64_or_zero(text: &str, log: Log) -> Option<f64> {
        Some(f64(text, log).unwrap_or(0.0))
    }

    /// A flag: a double above 0 (the former `parse_flag`); invalid: false.
    pub fn flag(text: &str, log: Log) -> Option<bool> {
        Some(f64(text, log).is_some_and(|v| v > 0.0))
    }

    /// A flag: a double above 0; invalid: true.
    pub fn flag_or_true(text: &str, log: Log) -> Option<bool> {
        Some(f64(text, log).is_none_or(|v| v > 0.0))
    }

    /// A flag: a double above 0; invalid: dropped.
    pub fn dflag(text: &str, log: Log) -> Option<bool> {
        f64(text, log).map(|v| v > 0.0)
    }

    /// A flag: an int (see [`int`]) above 0; invalid: dropped.
    pub fn iflag(text: &str, log: Log) -> Option<bool> {
        int(text, log).map(|v| v > 0)
    }

    /// A flag: an int above 0; invalid: false.
    pub fn iflag_or_false(text: &str, log: Log) -> Option<bool> {
        Some(int(text, log).is_some_and(|v| v > 0))
    }

    /// A flag: an int other than 0 (bypass); invalid: dropped.
    pub fn int_nonzero(text: &str, log: Log) -> Option<bool> {
        int(text, log).map(|v| v != 0)
    }

    /// The type of an EQ band (an int); invalid: logged and dropped.
    pub fn eq_band_type(text: &str, log: Log) -> Option<i32> {
        let v = int(text, log);
        if v.is_none() {
            log(&format!("Unhandled Equalizer band type: {text}"));
        }
        v
    }

    /// The text as it is.
    pub fn text<'a>(text: &'a str, _: Log) -> Option<&'a str> {
        Some(text)
    }

    /// A color `r g b` (0..255 each), None for "no color" (r < 0) and for an invalid value;
    /// logs a wrong number of values (the former `parse_color_static`).
    pub fn color(text: &str, log: Log) -> Option<Option<[f64; 3]>> {
        let values: Vec<&str> = text.split(' ').collect();
        if values.len() != 3 {
            log(&format!("Color: Wrong number of arguments: {}", values.len()));
            return Some(None);
        }
        let parse = |s: &str| s.trim().parse::<f64>().ok();
        let color = (|| {
            let r = parse(values[0])?;
            if r < 0.0 {
                return None;
            }
            Some([r / 255.0, parse(values[1])? / 255.0, parse(values[2])? / 255.0])
        })();
        Some(color)
    }

    /// A color as [`color`], but logs as the former `Model::parse_color` (also the values and
    /// the unparsable ones).
    pub fn color_logged(text: &str, log: Log) -> Option<Option<[f64; 3]>> {
        let values: Vec<&str> = text.split(' ').collect();
        if values.len() != 3 {
            log(&format!("Color: Wrong number of arguments: {}", values.len()));
            log(&values.iter().map(|v| format!("{v}:")).collect::<String>());
            return Some(None);
        }
        let parse = |s: &str| s.trim().parse::<f64>().ok();
        let (Some(r), Some(g), Some(b)) = (parse(values[0]), parse(values[1]), parse(values[2])) else {
            log(&format!("Color: Could not parse: {text}"));
            return Some(None);
        };
        if r < 0.0 {
            return Some(None);
        }
        Some(Some([r / 255.0, g / 255.0, b / 255.0]))
    }

    /// Notes `selected:muted:start:end:channel:pitch:velocity;…` (malformed notes are skipped).
    pub fn notes(text: &str, _: Log) -> Option<Vec<Note>> {
        Some(Note::parse_notes(text))
    }

    /// A step of the host's step grid (24 numbers, [`StepInfo::from_wire`]); None: empty.
    pub fn step(text: &str, _: Log) -> Option<Option<StepInfo>> {
        Some(StepInfo::from_wire(text))
    }

    /// A channel type by its name; unknown: logged and dropped.
    pub fn channel_type(text: &str, log: Log) -> Option<ChannelType> {
        let t = ChannelType::from_name(text.trim());
        if t.is_none() {
            log(&format!("Unknown channel type: {text}"));
        }
        t
    }

    /// A record quantization by its value (`OFF`, `1/32`, …); unknown: the default.
    pub fn record_quantization(text: &str, _: Log) -> Option<RecordQuantization> {
        Some(RecordQuantization::lookup(text.trim()))
    }
}

/// The text of a typed value in a report line (parsed back by the rule of its entry).
pub trait TextValue {
    fn write_text(&self, out: &mut String);
}

impl TextValue for f64 {
    /// The shortest form which parses back to the same double (TYPED_PROTOCOL_PLAN §5.3, 6).
    fn write_text(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, "{self}");
    }
}

impl TextValue for i32 {
    fn write_text(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, "{self}");
    }
}

impl TextValue for bool {
    fn write_text(&self, out: &mut String) {
        out.push(if *self { '1' } else { '0' });
    }
}

impl TextValue for &str {
    fn write_text(&self, out: &mut String) {
        out.push_str(self);
    }
}

impl TextValue for Option<[f64; 3]> {
    fn write_text(&self, out: &mut String) {
        use std::fmt::Write;
        match self {
            Some([r, g, b]) => {
                let c = |v: f64| (v * 255.0).round();
                let _ = write!(out, "{} {} {}", c(*r), c(*g), c(*b));
            }
            None => out.push_str("-1 -1 -1"),
        }
    }
}

impl TextValue for Vec<Note> {
    fn write_text(&self, out: &mut String) {
        use std::fmt::Write;
        for (i, n) in self.iter().enumerate() {
            if i > 0 {
                out.push(';');
            }
            let _ = write!(
                out,
                "{}:{}:{}:{}:{}:{}:{}",
                n.is_selected() as i32,
                n.is_muted() as i32,
                n.start(),
                n.end(),
                n.channel(),
                n.pitch(),
                n.velocity()
            );
        }
    }
}

impl TextValue for Option<StepInfo> {
    fn write_text(&self, out: &mut String) {
        match self {
            Some(step) => out.push_str(&step.to_wire()),
            None => out.push('0'),
        }
    }
}

impl TextValue for ChannelType {
    fn write_text(&self, out: &mut String) {
        out.push_str(self.name());
    }
}

impl TextValue for RecordQuantization {
    fn write_text(&self, out: &mut String) {
        out.push_str(self.value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trie_matches_literals_numbers_and_grids() {
        let trie = Trie::new([
            (1, vec![vec![Pat::Lit("track"), Pat::Num, Pat::Lit("exists")]]),
            (2, vec![vec![Pat::Lit("track"), Pat::Lit("count")]]),
            (3, vec![vec![Pat::Lit("clip"), Pat::Lit("grid"), Pat::Grid, Pat::Lit("playingStep")]]),
        ]);
        assert_eq!(trie.lookup("track/12/exists"), &[1]);
        assert_eq!(trie.lookup("track/count"), &[2]);
        assert_eq!(trie.lookup("clip/grid/16x8/playingStep"), &[3]);
        assert!(trie.lookup("track/x/exists").is_empty());
        assert!(trie.lookup("track/1").is_empty());
        assert!(trie.lookup("track/1/exists/more").is_empty());
        assert!(trie.ambiguous().is_empty());
    }

    #[test]
    fn patterns_of_optional_parts() {
        let mut acc = vec![vec![]];
        push_lit(&mut acc, "eq/band");
        push_field(&mut acc, &PhantomData::<usize>);
        push_opt_lit(&mut acc, "value");
        assert_eq!(
            acc,
            vec![vec![Pat::Lit("eq"), Pat::Lit("band"), Pat::Num], vec![Pat::Lit("eq"), Pat::Lit("band"), Pat::Num, Pat::Lit("value")]]
        );
        let mut acc = vec![vec![Pat::Lit("str")]];
        push_field(&mut acc, &PhantomData::<Option<usize>>);
        assert_eq!(acc, vec![vec![Pat::Lit("str")], vec![Pat::Lit("str"), Pat::Num]]);
    }

    #[test]
    fn lenient_values() {
        let mut log = |_: &str| {};
        assert_eq!(rules::int(" 3.7 ", &mut log), Some(3));
        assert_eq!(rules::flag("x", &mut log), Some(false));
        assert_eq!(rules::flag_or_true("x", &mut log), Some(true));
        assert_eq!(rules::dflag("x", &mut log), None);
        assert_eq!(rules::iflag("0.5", &mut log), Some(false));
        assert_eq!(rules::dflag("0.5", &mut log), Some(true));
        assert_eq!(rules::int_or("x", &mut log, -1), Some(-1));
        let mut lines = Vec::new();
        assert_eq!(rules::color("1 2", &mut |l: &str| lines.push(l.to_string())), Some(None));
        assert_eq!(rules::color("-1 0 0", &mut |l: &str| lines.push(l.to_string())), Some(None));
        assert_eq!(rules::color("255 0 51", &mut |l: &str| lines.push(l.to_string())), Some(Some([1.0, 0.0, 0.2])));
        assert_eq!(lines, ["Color: Wrong number of arguments: 2"]);
    }
}
