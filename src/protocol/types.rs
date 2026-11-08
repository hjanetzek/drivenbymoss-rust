// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The field and value types of the commands: index domains (enums with a stable number and a
//! text form), the channel address, the value operations and the structured values.

use std::borrow::Cow;

#[cfg(test)]
use super::legacy::Sample;
use super::legacy::{Arg, Field, PathReader, PathWriter, ProcField, Value};
use crate::backend::Processor;

/// Defines an index domain: an enum whose values have a stable number (the discriminant, the
/// index in the binary form) and a text form (the segment in the legacy command text).
macro_rules! domain {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u16)]
        pub enum $name { $($(#[$vmeta])* $variant),* }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            /// The text form.
            pub fn name(self) -> &'static str {
                match self { $($name::$variant => $text),* }
            }

            pub fn from_name(text: &str) -> Option<$name> {
                match text { $($text => Some($name::$variant),)* _ => None }
            }
        }

        impl $crate::protocol::legacy::Field for $name {
            fn write(&self, w: &mut $crate::protocol::legacy::PathWriter) {
                w.seg(self.name());
            }

            fn read(_: $crate::backend::Processor, r: &mut $crate::protocol::legacy::PathReader<'_>) -> Option<Self> {
                $name::from_name(r.next_segment()?)
            }
        }

        impl $crate::protocol::text::Seg for $name {
            fn write(&self, w: &mut $crate::protocol::legacy::PathWriter) {
                w.seg(self.name());
            }

            fn read(r: &mut $crate::protocol::legacy::PathReader<'_>) -> Option<Self> {
                $name::from_name(r.next_segment()?)
            }

            fn patterns() -> Vec<Vec<$crate::protocol::text::Pat>> {
                $name::ALL.iter().map(|v| vec![$crate::protocol::text::Pat::Lit(v.name())]).collect()
            }
        }

        /// The number of the value.
        impl $crate::protocol::bin::BinIdx for $name {
            fn put(&self, idx: &mut $crate::protocol::bin::IdxOut) {
                idx.push(*self as u16);
            }

            fn get(idx: &mut $crate::protocol::bin::IdxIn) -> Option<Self> {
                $name::ALL.get(usize::from(idx.take()?)).copied()
            }
        }

        #[cfg(test)]
        impl $crate::protocol::legacy::Sample for $name {
            fn samples() -> Vec<Self> {
                $name::ALL.to_vec()
            }
        }
    };
}

domain! {
    /// A step to the previous or next item.
    Dir { Prev = "-", Next = "+" }
}

domain! {
    /// The tempo changes of `tempo <step>` (as the Reaper backend defines them).
    TempoStep { Plus = "+", PlusPlus = "++", Minus = "-", MinusMinus = "--" }
}

domain! {
    /// The commands of a clip launcher slot (`track <t>/clip/<s>/<action>`).
    SlotAction {
        Launch = "launch",
        LaunchAlt = "launchAlt",
        LaunchRelease = "launchRelease",
        LaunchReleaseAlt = "launchReleaseAlt",
        Record = "record",
        Remove = "remove",
        Duplicate = "duplicate",
        Select = "select",
    }
}

domain! {
    /// The commands of a scene (`scene <s>/<action>`).
    SceneAction {
        Launch = "launch",
        LaunchAlt = "launchAlt",
        LaunchRelease = "launchRelease",
        LaunchReleaseAlt = "launchReleaseAlt",
        Remove = "remove",
        Duplicate = "duplicate",
        Select = "select",
    }
}

domain! {
    /// The commands of a marker (`marker <m>/<action>`).
    MarkerAction { Launch = "launch", Remove = "remove", Select = "select" }
}

domain! {
    /// The commands of a device itself or of a sibling (`<dev> <action>`, `device
    /// sibling/<n>/<action>`).
    DeviceAction { Bypass = "bypass", Remove = "remove", Duplicate = "duplicate", Select = "select" }
}

domain! {
    /// The parameters of the groove of the host (Bitwig, `groove <name>`), in the order of
    /// `HOST_GROOVE_PARAMETERS`.
    GrooveParam {
        Enabled = "enabled",
        ShuffleAmount = "shuffleAmount",
        ShuffleRate = "shuffleRate",
        AccentAmount = "accentAmount",
        AccentRate = "accentRate",
        AccentPhase = "accentPhase",
    }
}

domain! {
    /// The cue parameters of the project (Bitwig, `project <name>`).
    CueParam { Volume = "cueVolume", Mix = "cueMix" }
}

domain! {
    /// The parts of the clip launcher navigator of the host (`navigator <target>/-|+`).
    NavigatorTarget { Tracks = "tracks", Clips = "clips", Scenes = "scenes" }
}

domain! {
    /// The attributes of a step of the host's step grid
    /// (`clip grid/<id>/step/<c>/<x>/<y>/<attribute> <value>`).
    StepAttr {
        Chance = "chance",
        ChanceEnabled = "chanceEnabled",
        Duration = "duration",
        Gain = "gain",
        Muted = "muted",
        Occurrence = "occurrence",
        OccurrenceEnabled = "occurrenceEnabled",
        Pan = "pan",
        Pressure = "pressure",
        RecurrenceEnabled = "recurrenceEnabled",
        RecurrenceLength = "recurrenceLength",
        RecurrenceMask = "recurrenceMask",
        ReleaseVelocity = "releaseVelocity",
        RepeatCount = "repeatCount",
        RepeatCurve = "repeatCurve",
        RepeatEnabled = "repeatEnabled",
        RepeatVelocityCurve = "repeatVelocityCurve",
        RepeatVelocityEnd = "repeatVelocityEnd",
        Timbre = "timbre",
        Transpose = "transpose",
        Velocity = "velocity",
        VelocitySpread = "velocitySpread",
    }
}

// ------------------------------------------------------------------------------------------
// The channel address

/// The address of a channel in the commands: the processor and the prefix of its commands
/// (`Channel::create_command`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chan {
    /// The master track: processor `master`, no prefix.
    Master,
    /// A track of a channel bank (`track` or `fxtrack`) at its absolute position: `<p>/…`.
    Track { bank: Processor, p: i32 },
    /// A drum pad of a device (`drum`, `primary`, `device`): `pad/<i>/…`.
    Pad { owner: Processor, i: i32 },
    /// A layer of a device: `layer/<i>/…`.
    Layer { owner: Processor, i: i32 },
}

impl ProcField for Chan {
    fn processor(&self) -> Processor {
        match self {
            Chan::Master => Processor::Master,
            Chan::Track { bank, .. } => *bank,
            Chan::Pad { owner, .. } | Chan::Layer { owner, .. } => *owner,
        }
    }
}

impl Field for Chan {
    fn write(&self, w: &mut PathWriter) {
        match self {
            Chan::Master => {}
            Chan::Track { p, .. } => w.seg(p),
            Chan::Pad { i, .. } => {
                w.seg("pad");
                w.seg(i);
            }
            Chan::Layer { i, .. } => {
                w.seg("layer");
                w.seg(i);
            }
        }
    }

    /// Only the processors of channels: `master`, the banks `track`/`fxtrack` and the devices
    /// with pads or layers.
    fn read(processor: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        match processor {
            Processor::Master => return Some(Chan::Master),
            Processor::Track | Processor::FxTrack => return Some(Chan::Track { bank: processor, p: i32::read(processor, r)? }),
            Processor::Device | Processor::Primary | Processor::Drum => {}
            _ => return None,
        }
        match r.next_segment()? {
            "pad" => Some(Chan::Pad { owner: processor, i: i32::read(processor, r)? }),
            "layer" => Some(Chan::Layer { owner: processor, i: i32::read(processor, r)? }),
            _ => None,
        }
    }
}

#[cfg(test)]
impl Sample for Chan {
    fn samples() -> Vec<Self> {
        vec![
            Chan::Track { bank: Processor::Track, p: 3 },
            Chan::Master,
            Chan::Track { bank: Processor::FxTrack, p: 1 },
            Chan::Pad { owner: Processor::Drum, i: 36 },
            Chan::Layer { owner: Processor::Primary, i: 2 },
        ]
    }
}

// ------------------------------------------------------------------------------------------
// Value commands

/// The operation of a value command (HOST_BRIDGE_GUIDE §4.1): the suffix of the legacy command
/// and the value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueOp {
    /// `<value command> <value>`.
    Set(f64),
    /// `<value command>/inc <delta>` (feature `valueInc`).
    Inc(f64),
    /// `<value command>/reset` (feature `valueReset`).
    Reset,
    /// `<value command>/immediately <value>` (feature `immediately`).
    Immediately(f64),
    /// `<value command>/touch <0|1>`.
    Touch(bool),
    /// `<value command>/indicate <0|1>` (feature `indication`).
    Indicate(bool),
}

impl<'a> Value<'a> for ValueOp {
    fn encode(&self, w: &mut PathWriter) -> Arg<'_> {
        match *self {
            ValueOp::Set(v) => Arg::Double(v),
            ValueOp::Inc(v) => {
                w.seg("inc");
                Arg::Double(v)
            }
            ValueOp::Reset => {
                w.seg("reset");
                Arg::None
            }
            ValueOp::Immediately(v) => {
                w.seg("immediately");
                Arg::Double(v)
            }
            ValueOp::Touch(v) => {
                w.seg("touch");
                Arg::Int(v as i32)
            }
            ValueOp::Indicate(v) => {
                w.seg("indicate");
                Arg::Int(v as i32)
            }
        }
    }

    fn decode(r: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        let flag = |arg: &Arg| match arg {
            Arg::Int(0) => Some(false),
            Arg::Int(1) => Some(true),
            _ => None,
        };
        match (r.next_segment(), arg) {
            (None, Arg::Double(v)) => Some(ValueOp::Set(*v)),
            (Some("inc"), Arg::Double(v)) => Some(ValueOp::Inc(*v)),
            (Some("reset"), Arg::None) => Some(ValueOp::Reset),
            (Some("immediately"), Arg::Double(v)) => Some(ValueOp::Immediately(*v)),
            (Some("touch"), arg) => flag(arg).map(ValueOp::Touch),
            (Some("indicate"), arg) => flag(arg).map(ValueOp::Indicate),
            _ => None,
        }
    }
}

#[cfg(test)]
impl Sample for ValueOp {
    fn samples() -> Vec<Self> {
        vec![
            ValueOp::Set(0.5),
            ValueOp::Inc(-0.01),
            ValueOp::Reset,
            ValueOp::Immediately(0.75),
            ValueOp::Touch(true),
            ValueOp::Indicate(false),
        ]
    }
}

// ------------------------------------------------------------------------------------------
// Structured values and fields

/// A color sent as `RGB(r,g,b)` (0..255 each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub [i32; 3]);

impl<'a> Value<'a> for Rgb {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        let [r, g, b] = self.0;
        Arg::Str(Cow::Owned(format!("RGB({r},{g},{b})")))
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        let Arg::Str(text) = arg else {
            return None;
        };
        let mut parts = text.strip_prefix("RGB(")?.strip_suffix(')')?.split(',').map(|p| p.parse::<i32>().ok());
        let color = Rgb([parts.next()??, parts.next()??, parts.next()??]);
        parts.next().is_none().then_some(color)
    }
}

/// The time signature, sent as one int `numerator + denominator * 100`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    pub numerator: i32,
    pub denominator: i32,
}

impl<'a> Value<'a> for Signature {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Int(self.numerator + self.denominator * 100)
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Int(v) => Some(Signature { numerator: v % 100, denominator: v / 100 }),
            _ => None,
        }
    }
}

/// An int sent as a string (`eq band/<b> <type>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntText(pub i32);

impl<'a> Value<'a> for IntText {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Str(Cow::Owned(self.0.to_string()))
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Str(v) => v.parse().ok().map(IntText),
            _ => None,
        }
    }
}

/// A slot (track and slot position), sent as `<track>/<slot>` (`…/clip/<s>/paste`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotRef {
    pub track: i32,
    pub slot: i32,
}

impl<'a> Value<'a> for SlotRef {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Str(Cow::Owned(format!("{}/{}", self.track, self.slot)))
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        let Arg::Str(text) = arg else {
            return None;
        };
        let (track, slot) = text.split_once('/')?;
        Some(SlotRef { track: track.parse().ok()?, slot: slot.parse().ok()? })
    }
}

/// The velocity and duration of a new note of the host's step grid, sent as `<velocity>
/// <duration>` (`…/step/<c>/<x>/<y>/set`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridNote {
    pub velocity: i32,
    pub duration: f64,
}

impl<'a> Value<'a> for GridNote {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Str(Cow::Owned(format!("{} {}", self.velocity, self.duration)))
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        let Arg::Str(text) = arg else {
            return None;
        };
        let (velocity, duration) = text.split_once(' ')?;
        Some(GridNote { velocity: velocity.parse().ok()?, duration: duration.parse().ok()? })
    }
}

/// An attribute of a step and its value: `…/<attribute> <value>` in the legacy text (the
/// attribute is the last segment); in the binary form a struct `u32 attribute, u32 0, f64 value`
/// (the step address already takes the four indices).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepValue {
    pub attr: StepAttr,
    pub value: f64,
}

impl<'a> Value<'a> for StepValue {
    fn encode(&self, w: &mut PathWriter) -> Arg<'_> {
        w.seg(self.attr.name());
        Arg::Double(self.value)
    }

    fn decode(r: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        let attr = StepAttr::from_name(r.next_segment()?)?;
        match arg {
            Arg::Double(value) => Some(StepValue { attr, value: *value }),
            _ => None,
        }
    }
}

#[cfg(test)]
impl Sample for StepValue {
    fn samples() -> Vec<Self> {
        StepAttr::ALL.iter().enumerate().map(|(i, attr)| StepValue { attr: *attr, value: i as f64 * 0.25 }).collect()
    }
}

/// The ID of a note clip of the host's step grid: its steps and rows, `<steps>x<rows>` in the
/// legacy text (two numbers in one segment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridId {
    pub steps: u16,
    pub rows: u16,
}

impl Field for GridId {
    fn write(&self, w: &mut PathWriter) {
        w.seg(format_args!("{}x{}", self.steps, self.rows));
    }

    fn read(_: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        let (steps, rows) = r.next_segment()?.split_once('x')?;
        Some(GridId { steps: steps.parse().ok()?, rows: rows.parse().ok()? })
    }
}

/// The MIDI channel of a step of the host's step grid (0–15). In the binary form it shares one
/// index slot with the [`GridRow`] which follows it in the braces of the entry
/// (`channel << 7 | row`), so that a step (grid steps, grid rows, channel/row, step) fits into the
/// four index slots of a record (TYPED_PROTOCOL_PLAN §5.3 Q7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridChannel(pub i32);

/// The row (the note, 0–127) of a step of the host's step grid; packed into the slot of the
/// [`GridChannel`] before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridRow(pub i32);

impl Field for GridChannel {
    fn write(&self, w: &mut PathWriter) {
        self.0.write(w);
    }

    fn read(processor: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        i32::read(processor, r).map(GridChannel)
    }
}

impl Field for GridRow {
    fn write(&self, w: &mut PathWriter) {
        self.0.write(w);
    }

    fn read(processor: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        i32::read(processor, r).map(GridRow)
    }
}

/// A 0-based index written 1-based in the legacy text (the devices of the Reaper commands
/// `device <n>/…`, `device selected <n>` and the siblings `device sibling/<n>/…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pos1(pub i32);

impl Field for Pos1 {
    fn write(&self, w: &mut PathWriter) {
        w.seg(self.0 + 1);
    }

    fn read(processor: Processor, r: &mut PathReader<'_>) -> Option<Self> {
        Some(Pos1(i32::read(processor, r)? - 1))
    }
}

impl<'a> Value<'a> for Pos1 {
    fn encode(&self, _: &mut PathWriter) -> Arg<'_> {
        Arg::Int(self.0 + 1)
    }

    fn decode(_: &mut PathReader<'a>, arg: &'a Arg<'a>) -> Option<Self> {
        match arg {
            Arg::Int(v) => Some(Pos1(v - 1)),
            _ => None,
        }
    }
}

#[cfg(test)]
impl Sample for Rgb {
    fn samples() -> Vec<Self> {
        vec![Rgb([255, 0, 127]), Rgb([0, 0, 0])]
    }
}

#[cfg(test)]
impl Sample for Signature {
    fn samples() -> Vec<Self> {
        vec![Signature { numerator: 7, denominator: 8 }, Signature { numerator: 4, denominator: 4 }]
    }
}

#[cfg(test)]
impl Sample for IntText {
    fn samples() -> Vec<Self> {
        vec![IntText(3), IntText(0)]
    }
}

#[cfg(test)]
impl Sample for SlotRef {
    fn samples() -> Vec<Self> {
        vec![SlotRef { track: 2, slot: 5 }]
    }
}

#[cfg(test)]
impl Sample for GridNote {
    fn samples() -> Vec<Self> {
        vec![GridNote { velocity: 100, duration: 0.25 }, GridNote { velocity: 1, duration: 4.0 }]
    }
}

#[cfg(test)]
impl Sample for GridId {
    fn samples() -> Vec<Self> {
        vec![GridId { steps: 8, rows: 128 }, GridId { steps: 64, rows: 1 }, GridId { steps: 1536, rows: 128 }]
    }
}

#[cfg(test)]
impl Sample for GridChannel {
    fn samples() -> Vec<Self> {
        vec![GridChannel(0), GridChannel(15), GridChannel(9)]
    }
}

#[cfg(test)]
impl Sample for GridRow {
    fn samples() -> Vec<Self> {
        vec![GridRow(60), GridRow(127), GridRow(0)]
    }
}

#[cfg(test)]
impl Sample for Pos1 {
    fn samples() -> Vec<Self> {
        vec![Pos1(0), Pos1(4), Pos1(-1)]
    }
}
