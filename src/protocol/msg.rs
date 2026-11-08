// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The address half of the protocol table: every report of the host to the core
//! (TYPED_PROTOCOL_PLAN §1.2, §2.6, step 0b). The syntax of an entry is described at the
//! `messages!` macro in [`super::macros`].
//!
//! The text form of an entry is its address `/<segments>` and the value; [`Msg::from_text`]
//! finds the entry with a trie over the path segments and parses the value with the rule of
//! the entry ([`super::text::rules`]), which keeps the lenient rules of the former `parser.rs`.
//! The core applies a [`Msg`] in `Parser::apply` (`src/parser.rs`), an exhaustive `match`.
//!
//! IDs: one block of 0x100 per area, the high bit clear (host → core). IDs are never renumbered
//! or reused.
//!
//! | Block | Area |
//! | --- | --- |
//! | 0x01 | transport, time, beats, click |
//! | 0x02 | application, project, host, arranger, mixer, record quantization, actions |
//! | 0x03 | channels (tracks, effect tracks, master), sends, the track banks |
//! | 0x04 | slots and scenes |
//! | 0x05 | devices, siblings, drum machine, drum pads, layers, EQ |
//! | 0x06 | parameters, parameter pages, host parameters (crossfade, cue, groove) |
//! | 0x07 | the cursor clip, the host's step grid, the Reaper clips |
//! | 0x08 | browser |
//! | 0x09 | markers |
//! | 0x0B | note repeat, quantize, groove |

#![allow(clippy::enum_variant_names)]

use std::marker::PhantomData;
use std::sync::OnceLock;

use super::bin::{BinError, BinIdx, BinValue, IdxIn, IdxOut, Record, Writer};
use super::legacy::{PathReader, PathWriter};
use super::text::{push_field, push_lit, push_opt_lit, rules, Miss, Pat, Seg, TextValue, Trie};
use super::types::{CueParam, GridId, GrooveParam, Pos1};
use crate::daw::clip::{Note, StepInfo};
use crate::daw::constants::{ChannelType, RecordQuantization};

/// The metadata of a message (for diagnostics, documentation and the generators of step 1).
#[derive(Debug)]
pub struct MsgInfo {
    pub id: MsgId,
    pub name: &'static str,
    /// The address with the fields in braces and optional literals in brackets, e.g.
    /// `/{ch}/volume/str/{limit}`.
    pub template: &'static str,
    /// The fields (name, Rust type), including the value.
    pub fields: &'static [(&'static str, &'static str)],
    /// The field holding the value, None if the value is ignored.
    pub value: Option<&'static str>,
    /// The rule which parses the value ([`super::text::rules`]).
    pub rule: Option<&'static str>,
}

// ------------------------------------------------------------------------------------------
// The address fields

domain! {
    /// The devices with the common device reports (`/device`, `/primary`, `/eq`).
    Dev { Device = "device", Primary = "primary", Eq = "eq" }
}

domain! {
    /// The visibility flags of the arranger (Bitwig, `/arranger/<flag>`).
    ArrangerFlag {
        CueMarkers = "cueMarkers",
        DoubleRowTrackHeight = "doubleRowTrackHeight",
        ClipLauncher = "clipLauncher",
        Timeline = "timeline",
        IoSection = "ioSection",
        EffectTracks = "effectTracks",
    }
}

domain! {
    /// The visibility flags of the mixer (Bitwig, `/mixer/<flag>`).
    MixerFlag {
        ClipLauncherSection = "clipLauncherSection",
        CrossFadeSection = "crossFadeSection",
        DeviceSection = "deviceSection",
        IoSection = "ioSection",
        MeterSection = "meterSection",
        SendSection = "sendSection",
    }
}

/// A channel in the reports: `track/<p>`, `fxtrack/<p>` (absolute positions) or `master`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ch {
    Track(usize),
    Effect(usize),
    Master,
}

impl Seg for Ch {
    fn write(&self, w: &mut PathWriter) {
        match self {
            Ch::Track(p) => {
                w.seg("track");
                w.seg(p);
            }
            Ch::Effect(p) => {
                w.seg("fxtrack");
                w.seg(p);
            }
            Ch::Master => w.seg("master"),
        }
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        match r.next_segment()? {
            "track" => Some(Ch::Track(Seg::read(r)?)),
            "fxtrack" => Some(Ch::Effect(Seg::read(r)?)),
            "master" => Some(Ch::Master),
            _ => None,
        }
    }

    fn patterns() -> Vec<Vec<Pat>> {
        vec![vec![Pat::Lit("track"), Pat::Num], vec![Pat::Lit("fxtrack"), Pat::Num], vec![Pat::Lit("master")]]
    }
}

/// The owner of a parameter bank: a device, the project's remote controls (Bitwig), the
/// remote controls of the cursor track (`track/fx`) or the master track (`master/fx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamOwner {
    Device(Dev),
    Project,
    TrackFx,
    MasterFx,
}

impl Seg for ParamOwner {
    fn write(&self, w: &mut PathWriter) {
        match self {
            ParamOwner::Device(dev) => w.seg(dev.name()),
            ParamOwner::Project => w.seg("project"),
            ParamOwner::TrackFx => w.seg("track/fx"),
            ParamOwner::MasterFx => w.seg("master/fx"),
        }
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        match r.next_segment()? {
            "project" => Some(ParamOwner::Project),
            "track" => r.literal("fx").then_some(ParamOwner::TrackFx),
            "master" => r.literal("fx").then_some(ParamOwner::MasterFx),
            other => Dev::from_name(other).map(ParamOwner::Device),
        }
    }

    fn patterns() -> Vec<Vec<Pat>> {
        let mut patterns: Vec<Vec<Pat>> = Dev::ALL.iter().map(|dev| vec![Pat::Lit(dev.name())]).collect();
        patterns.push(vec![Pat::Lit("project")]);
        patterns.push(vec![Pat::Lit("track"), Pat::Lit("fx")]);
        patterns.push(vec![Pat::Lit("master"), Pat::Lit("fx")]);
        patterns
    }
}

/// The owner of named parameter pages: a device, the project or the cursor track (`track/fx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageOwner {
    Device(Dev),
    Project,
    TrackFx,
}

impl Seg for PageOwner {
    fn write(&self, w: &mut PathWriter) {
        match self {
            PageOwner::Device(dev) => w.seg(dev.name()),
            PageOwner::Project => w.seg("project"),
            PageOwner::TrackFx => w.seg("track/fx"),
        }
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        match r.next_segment()? {
            "project" => Some(PageOwner::Project),
            "track" => r.literal("fx").then_some(PageOwner::TrackFx),
            other => Dev::from_name(other).map(PageOwner::Device),
        }
    }

    fn patterns() -> Vec<Vec<Pat>> {
        let mut patterns: Vec<Vec<Pat>> = Dev::ALL.iter().map(|dev| vec![Pat::Lit(dev.name())]).collect();
        patterns.push(vec![Pat::Lit("project")]);
        patterns.push(vec![Pat::Lit("track"), Pat::Lit("fx")]);
        patterns
    }
}

/// A drum pad or layer: `drum/pad/<i>` (all drum machines), `<dev>/pad/<i>`, `<dev>/layer/<i>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadRef {
    DrumPad(usize),
    Pad(Dev, usize),
    Layer(Dev, usize),
}

impl Seg for PadRef {
    fn write(&self, w: &mut PathWriter) {
        let (owner, kind, i) = match self {
            PadRef::DrumPad(i) => ("drum", "pad", i),
            PadRef::Pad(dev, i) => (dev.name(), "pad", i),
            PadRef::Layer(dev, i) => (dev.name(), "layer", i),
        };
        w.seg(owner);
        w.seg(kind);
        w.seg(i);
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        let owner = r.next_segment()?;
        let kind = r.next_segment()?;
        let i: usize = Seg::read(r)?;
        match (owner, kind) {
            ("drum", "pad") => Some(PadRef::DrumPad(i)),
            (dev, "pad") => Some(PadRef::Pad(Dev::from_name(dev)?, i)),
            (dev, "layer") => Some(PadRef::Layer(Dev::from_name(dev)?, i)),
            _ => None,
        }
    }

    fn patterns() -> Vec<Vec<Pat>> {
        let mut patterns = vec![vec![Pat::Lit("drum"), Pat::Lit("pad"), Pat::Num]];
        for dev in Dev::ALL {
            patterns.push(vec![Pat::Lit(dev.name()), Pat::Lit("pad"), Pat::Num]);
            patterns.push(vec![Pat::Lit(dev.name()), Pat::Lit("layer"), Pat::Num]);
        }
        patterns
    }
}

/// A parameter of the host reported as `<prefix>`, `<prefix>/str[/<limit>]` and
/// `<prefix>/name`: the crossfader (`crossfade`), the cue parameters (`project/cueVolume|cueMix`)
/// and the groove parameters (`groove/<name>`, Bitwig).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostParam {
    Crossfade,
    Cue(CueParam),
    Groove(GrooveParam),
}

impl Seg for HostParam {
    fn write(&self, w: &mut PathWriter) {
        match self {
            HostParam::Crossfade => w.seg("crossfade"),
            HostParam::Cue(cue) => {
                w.seg("project");
                w.seg(cue.name());
            }
            HostParam::Groove(groove) => {
                w.seg("groove");
                w.seg(groove.name());
            }
        }
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        match r.next_segment()? {
            "crossfade" => Some(HostParam::Crossfade),
            "project" => CueParam::from_name(r.next_segment()?).map(HostParam::Cue),
            "groove" => GrooveParam::from_name(r.next_segment()?).map(HostParam::Groove),
            _ => None,
        }
    }

    fn patterns() -> Vec<Vec<Pat>> {
        let mut patterns = vec![vec![Pat::Lit("crossfade")]];
        patterns.extend(CueParam::ALL.iter().map(|cue| vec![Pat::Lit("project"), Pat::Lit(cue.name())]));
        patterns.extend(GrooveParam::ALL.iter().map(|groove| vec![Pat::Lit("groove"), Pat::Lit(groove.name())]));
        patterns
    }
}

/// An item of the browser: of a filter column (`browser/column/<c>/item/<i>`) or a result
/// (`browser/result/<i>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowserItem {
    Column(usize, usize),
    Result(usize),
}

impl Seg for BrowserItem {
    fn write(&self, w: &mut PathWriter) {
        w.seg("browser");
        match self {
            BrowserItem::Column(column, item) => {
                w.seg("column");
                w.seg(column);
                w.seg("item");
                w.seg(item);
            }
            BrowserItem::Result(item) => {
                w.seg("result");
                w.seg(item);
            }
        }
    }

    fn read(r: &mut PathReader<'_>) -> Option<Self> {
        if !r.literal("browser") {
            return None;
        }
        match r.next_segment()? {
            "column" => {
                let column = Seg::read(r)?;
                r.literal("item").then_some(())?;
                Some(BrowserItem::Column(column, Seg::read(r)?))
            }
            "result" => Some(BrowserItem::Result(Seg::read(r)?)),
            _ => None,
        }
    }

    fn patterns() -> Vec<Vec<Pat>> {
        vec![
            vec![Pat::Lit("browser"), Pat::Lit("column"), Pat::Num, Pat::Lit("item"), Pat::Num],
            vec![Pat::Lit("browser"), Pat::Lit("result"), Pat::Num],
        ]
    }
}


// ------------------------------------------------------------------------------------------
// The address fields in the binary form

/// kind (0 track, 1 effect track, 2 master), position.
impl BinIdx for Ch {
    const SLOTS: &'static [&'static str] = &["kind", "pos"];

    fn put(&self, idx: &mut IdxOut) {
        let (kind, p) = match *self {
            Ch::Track(p) => (0, p),
            Ch::Effect(p) => (1, p),
            Ch::Master => (2, 0),
        };
        idx.push(kind);
        p.put(idx);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        let (kind, p) = (idx.take()?, usize::get(idx)?);
        Some(match kind {
            0 => Ch::Track(p),
            1 => Ch::Effect(p),
            2 => Ch::Master,
            _ => return None,
        })
    }
}

/// 0–2 the devices (`Dev`), 3 project, 4 track FX, 5 master FX.
impl BinIdx for ParamOwner {
    fn put(&self, idx: &mut IdxOut) {
        idx.push(match *self {
            ParamOwner::Device(dev) => dev as u16,
            ParamOwner::Project => 3,
            ParamOwner::TrackFx => 4,
            ParamOwner::MasterFx => 5,
        });
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        Some(match idx.take()? {
            3 => ParamOwner::Project,
            4 => ParamOwner::TrackFx,
            5 => ParamOwner::MasterFx,
            n => ParamOwner::Device(*Dev::ALL.get(usize::from(n))?),
        })
    }
}

/// 0–2 the devices (`Dev`), 3 project, 4 track FX.
impl BinIdx for PageOwner {
    fn put(&self, idx: &mut IdxOut) {
        idx.push(match *self {
            PageOwner::Device(dev) => dev as u16,
            PageOwner::Project => 3,
            PageOwner::TrackFx => 4,
        });
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        Some(match idx.take()? {
            3 => PageOwner::Project,
            4 => PageOwner::TrackFx,
            n => PageOwner::Device(*Dev::ALL.get(usize::from(n))?),
        })
    }
}

/// kind (0 drum machine pad, 1 device pad, 2 device layer), device (`Dev`), index.
impl BinIdx for PadRef {
    const SLOTS: &'static [&'static str] = &["kind", "dev", "index"];

    fn put(&self, idx: &mut IdxOut) {
        let (kind, dev, i) = match *self {
            PadRef::DrumPad(i) => (0, Dev::Device, i),
            PadRef::Pad(dev, i) => (1, dev, i),
            PadRef::Layer(dev, i) => (2, dev, i),
        };
        idx.push(kind);
        dev.put(idx);
        i.put(idx);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        let (kind, dev, i) = (idx.take()?, Dev::get(idx)?, usize::get(idx)?);
        Some(match kind {
            0 => PadRef::DrumPad(i),
            1 => PadRef::Pad(dev, i),
            2 => PadRef::Layer(dev, i),
            _ => return None,
        })
    }
}

/// kind (0 crossfader, 1 cue, 2 groove), parameter (`CueParam`, `GrooveParam`).
impl BinIdx for HostParam {
    const SLOTS: &'static [&'static str] = &["kind", "param"];

    fn put(&self, idx: &mut IdxOut) {
        let (kind, param) = match *self {
            HostParam::Crossfade => (0, 0),
            HostParam::Cue(cue) => (1, cue as u16),
            HostParam::Groove(groove) => (2, groove as u16),
        };
        idx.push(kind);
        idx.push(param);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        let (kind, param) = (idx.take()?, usize::from(idx.take()?));
        Some(match kind {
            0 => HostParam::Crossfade,
            1 => HostParam::Cue(*CueParam::ALL.get(param)?),
            2 => HostParam::Groove(*GrooveParam::ALL.get(param)?),
            _ => return None,
        })
    }
}

/// kind (0 column item, 1 result), column, item.
impl BinIdx for BrowserItem {
    const SLOTS: &'static [&'static str] = &["kind", "column", "item"];

    fn put(&self, idx: &mut IdxOut) {
        let (kind, column, item) = match *self {
            BrowserItem::Column(column, item) => (0, column, item),
            BrowserItem::Result(item) => (1, 0, item),
        };
        idx.push(kind);
        column.put(idx);
        item.put(idx);
    }

    fn get(idx: &mut IdxIn) -> Option<Self> {
        let (kind, column, item) = (idx.take()?, usize::get(idx)?, usize::get(idx)?);
        Some(match kind {
            0 => BrowserItem::Column(column, item),
            1 => BrowserItem::Result(item),
            _ => return None,
        })
    }
}

// ------------------------------------------------------------------------------------------
// The table

messages! {
    // --------------------------------------------------------------------------------------
    // 0x01 Transport, time, beats, click

    Play = 0x0101 {} ["play"] => value: bool = dflag;
    /// Reaper: the play state is `value == 0`.
    Stop = 0x0102 {} ["stop"] => value: f64 = f64;
    Repeat = 0x0103 {} ["repeat"] => value: bool = dflag;
    Record = 0x0104 {} ["record"] => value: bool = dflag;
    Tempo = 0x0105 {} ["tempo"] => value: f64 = f64;
    /// The play position in seconds.
    Time = 0x0110 {} ["time"] => value: f64 = f64;
    /// The play position formatted by the core (Reaper).
    TimeStr = 0x0111 {} ["time" "str"] => value: &'a str = text;
    /// The play position formatted by the host, used as is (Bitwig).
    TimeText = 0x0112 {} ["time" "text"] => value: &'a str = text;
    TimeHZoom = 0x0113 {} ["time" "hzoom"] => value: f64 = f64;
    TimeLoopStart = 0x0114 {} ["time" "loop" "start"] => value: f64 = f64;
    TimeLoopStartStr = 0x0115 {} ["time" "loop" "start" "str"] => value: &'a str = text;
    TimeLoopStartBeat = 0x0116 {} ["time" "loop" "start" "beat"] => value: &'a str = text;
    TimeLoopLength = 0x0117 {} ["time" "loop" "length"] => value: f64 = f64;
    TimeLoopLengthStr = 0x0118 {} ["time" "loop" "length" "str"] => value: &'a str = text;
    TimeLoopLengthBeat = 0x0119 {} ["time" "loop" "length" "beat"] => value: &'a str = text;
    /// The play position in quarter notes.
    BeatsPosition = 0x0120 {} ["beats" "position"] => value: f64 = f64;
    BeatsText = 0x0121 {} ["beats" "text"] => value: &'a str = text;
    BeatsLoopStart = 0x0122 {} ["beats" "loop" "start"] => value: f64 = f64;
    BeatsLoopLength = 0x0123 {} ["beats" "loop" "length"] => value: f64 = f64;
    BeatsLoopStartText = 0x0124 {} ["beats" "loop" "start" "text"] => value: &'a str = text;
    BeatsLoopLengthText = 0x0125 {} ["beats" "loop" "length" "text"] => value: &'a str = text;
    /// The play position as a beat text (Reaper).
    Beat = 0x0126 {} ["beat"] => value: &'a str = text;
    Numerator = 0x0130 {} ["numerator"] => value: i32 = f64_as_int;
    Denominator = 0x0131 {} ["denominator"] => value: i32 = f64_as_int;
    PunchIn = 0x0140 {} ["punchIn"] => value: bool = flag;
    PunchOut = 0x0141 {} ["punchOut"] => value: bool = flag;
    Fill = 0x0142 {} ["fill"] => value: bool = flag;
    FollowPlayback = 0x0143 {} ["followPlayback"] => value: bool = dflag;
    OverdubArranger = 0x0144 {} ["overdub" "arranger"] => value: bool = flag;
    OverdubLauncher = 0x0145 {} ["overdub" "launcher"] => value: bool = flag;
    /// The automation write mode as an int (0–5: trim read, read, touch, write, latch, latch
    /// preview); other values are ignored.
    AutomationMode = 0x0146 {} ["automode"] => value: i32 = f64_as_int;
    AutomationArranger = 0x0147 {} ["automode" "arranger"] => value: bool = flag;
    AutomationLauncher = 0x0148 {} ["automode" "launcher"] => value: bool = flag;
    Metronome = 0x0150 {} ["click"] => value: bool = dflag;
    /// Reaper: bit 2 is the pre-roll metronome.
    MetronomePreroll = 0x0151 {} ["click" "preroll"] => value: i32 = int;
    MetronomePrerollMeasures = 0x0152 {} ["click" "prerollMeasures"] => value: i32 = f64_as_int;
    MetronomeVolume = 0x0153 {} ["click" "volume"] => value: f64 = f64;
    MetronomeVolumeStr = 0x0154 {} ["click" "volumeStr"] => value: &'a str = text;
    MetronomeTicks = 0x0155 {} ["click" "ticks"] => value: bool = flag;

    // --------------------------------------------------------------------------------------
    // 0x02 Application, project, host

    ProjectName = 0x0201 {} ["project" "name"] => value: &'a str = text;
    ProjectEngine = 0x0202 {} ["project" "engine"] => value: bool = iflag;
    ProjectCanUndo = 0x0203 {} ["project" "canUndo"] => value: bool = iflag;
    ProjectCanRedo = 0x0204 {} ["project" "canRedo"] => value: bool = iflag;
    ProjectIsDirty = 0x0205 {} ["project" "isDirty"] => value: bool = iflag;
    ProjectHasSolo = 0x0206 {} ["project" "hasSolo"] => value: bool = flag;
    ProjectHasMute = 0x0207 {} ["project" "hasMute"] => value: bool = flag;
    /// The panel layout (`ARRANGE`, `MIX`, `EDIT`).
    Layout = 0x0210 {} ["layout"] => value: &'a str = text;
    RecordQuantization = 0x0211 {} ["recordQuantization"] => value: RecordQuantization = record_quantization;
    RecordQuantizationNoteLength = 0x0212 {} ["recordQuantization" "noteLength"] => value: bool = flag;
    Arranger = 0x0213 { flag: ArrangerFlag } ["arranger" flag] => value: bool = flag;
    Mixer = 0x0214 { flag: MixerFlag } ["mixer" flag] => value: bool = flag;
    /// The command forms the host understands (space separated `HostFeature` names).
    HostFeatures = 0x0220 {} ["host" "features"] => value: &'a str = text;
    /// An action was selected in Reaper (for the configuration dialog).
    ActionSelect = 0x0230 {} ["action" "select"] => value: &'a str = text;

    // --------------------------------------------------------------------------------------
    // 0x03 Channels, sends, track banks

    TrackCount = 0x0301 {} ["track" "count"] => value: i32 = int;
    /// The host pages the tracks (the value is ignored).
    TrackOffset = 0x0302 {} ["track" "offset"];
    /// The position of the pinned cursor track, -1 if not pinned (Bitwig).
    TrackPinned = 0x0303 {} ["track" "pinned"] => value: i32 = int;
    TrackHasPrevious = 0x0304 {} ["track" "hasPrevious"] => value: bool = flag;
    TrackHasNext = 0x0305 {} ["track" "hasNext"] => value: bool = flag;
    EffectTrackCount = 0x0306 {} ["fxtrack" "count"] => value: i32 = int;
    /// The host pages the effect tracks (the value is ignored).
    EffectTrackOffset = 0x0307 {} ["fxtrack" "offset"];

    ChannelExists = 0x0310 { ch: Ch } [ch "exists"] => value: bool = dflag;
    ChannelDepth = 0x0311 { ch: Ch } [ch "depth"] => value: i32 = int;
    ChannelActive = 0x0312 { ch: Ch } [ch "active"] => value: bool = dflag;
    ChannelCanHoldNotes = 0x0313 { ch: Ch } [ch "canHoldNotes"] => value: bool = flag;
    ChannelCanHoldAudio = 0x0314 { ch: Ch } [ch "canHoldAudio"] => value: bool = flag;
    ChannelType = 0x0315 { ch: Ch } [ch "type"] => value: ChannelType = channel_type;
    ChannelIsGroupExpanded = 0x0316 { ch: Ch } [ch "isGroupExpanded"] => value: bool = iflag;
    /// Only a change counts (a repeated selection does not recall views).
    ChannelSelected = 0x0317 { ch: Ch } [ch "select"] => value: bool = dflag;
    ChannelNumber = 0x0318 { ch: Ch } [ch "number"] => value: i32 = int;
    ChannelName = 0x0319 { ch: Ch } [ch "name"] => value: &'a str = text;
    ChannelVolume = 0x031A { ch: Ch } [ch "volume"] => value: f64 = f64;
    ChannelVolumeStr = 0x031B { ch: Ch, limit: Option<usize> } [ch "volume" "str" limit] => value: &'a str = text;
    ChannelPan = 0x031C { ch: Ch } [ch "pan"] => value: f64 = f64;
    ChannelPanStr = 0x031D { ch: Ch, limit: Option<usize> } [ch "pan" "str" limit] => value: &'a str = text;
    ChannelVu = 0x031E { ch: Ch } [ch "vu"] => value: f64 = f64;
    ChannelVuLeft = 0x031F { ch: Ch } [ch "vuleft"] => value: f64 = f64;
    ChannelVuRight = 0x0320 { ch: Ch } [ch "vuright"] => value: f64 = f64;
    ChannelVuHoldLeft = 0x0321 { ch: Ch } [ch "vuholdleft"] => value: f64 = f64;
    ChannelVuHoldRight = 0x0322 { ch: Ch } [ch "vuholdright"] => value: f64 = f64;
    ChannelMute = 0x0323 { ch: Ch } [ch "mute"] => value: bool = dflag;
    ChannelSolo = 0x0324 { ch: Ch } [ch "solo"] => value: bool = dflag;
    ChannelRecArm = 0x0325 { ch: Ch } [ch "recarm"] => value: bool = dflag;
    ChannelMonitor = 0x0326 { ch: Ch } [ch "monitor"] => value: bool = dflag;
    ChannelAutoMonitor = 0x0327 { ch: Ch } [ch "autoMonitor"] => value: bool = dflag;
    ChannelOverdub = 0x0328 { ch: Ch } [ch "overdub"] => value: bool = dflag;
    ChannelInQuantLengthEnabled = 0x0329 { ch: Ch } [ch "inQuantLengthEnabled"] => value: bool = dflag;
    ChannelInQuantResolution = 0x032A { ch: Ch } [ch "inQuantResolution"] => value: f64 = f64;
    ChannelColor = 0x032B { ch: Ch } [ch "color"] => value: Option<[f64; 3]> = color_logged;
    ChannelIsStopped = 0x032C { ch: Ch } [ch "isStopped"] => value: bool = dflag;
    /// The playing notes of a track (not of the master).
    ChannelPlayingNotes = 0x032D { ch: Ch } [ch "playingnotes"] => value: Vec<Note> = notes;

    SendCount = 0x0340 { ch: Ch } [ch "send" "count"] => value: i32 = int;
    SendActive = 0x0341 { ch: Ch, send: usize } [ch "send" send "active"] => value: bool = iflag_or_false;
    SendName = 0x0342 { ch: Ch, send: usize } [ch "send" send "name"] => value: &'a str = text;
    SendVolume = 0x0343 { ch: Ch, send: usize } [ch "send" send "volume"] => value: f64 = f64;
    SendVolumeStr = 0x0344 { ch: Ch, send: usize, limit: Option<usize> } [ch "send" send "volume" "str" limit] => value: &'a str = text;
    SendColor = 0x0345 { ch: Ch, send: usize } [ch "send" send "color"] => value: Option<[f64; 3]> = color;

    // --------------------------------------------------------------------------------------
    // 0x04 Slots and scenes

    SlotCount = 0x0401 { ch: Ch } [ch "slot" "count"] => value: i32 = int;
    SlotExists = 0x0402 { ch: Ch, slot: usize } [ch "slot" slot "exists"] => value: bool = flag;
    SlotHasContent = 0x0403 { ch: Ch, slot: usize } [ch "slot" slot "hasContent"] => value: bool = flag;
    SlotName = 0x0404 { ch: Ch, slot: usize } [ch "slot" slot "name"] => value: &'a str = text;
    SlotColor = 0x0405 { ch: Ch, slot: usize } [ch "slot" slot "color"] => value: Option<[f64; 3]> = color;
    SlotSelected = 0x0406 { ch: Ch, slot: usize } [ch "slot" slot "selected"] => value: bool = flag;
    SlotPlaying = 0x0407 { ch: Ch, slot: usize } [ch "slot" slot "playing"] => value: bool = flag;
    SlotRecording = 0x0408 { ch: Ch, slot: usize } [ch "slot" slot "recording"] => value: bool = flag;
    SlotPlayQueued = 0x0409 { ch: Ch, slot: usize } [ch "slot" slot "playQueued"] => value: bool = flag;
    SlotRecQueued = 0x040A { ch: Ch, slot: usize } [ch "slot" slot "recQueued"] => value: bool = flag;
    SlotStopQueued = 0x040B { ch: Ch, slot: usize } [ch "slot" slot "stopQueued"] => value: bool = flag;

    /// The host pages the scenes (the value is ignored).
    SceneOffset = 0x0420 {} ["scene" "offset"];
    SceneCount = 0x0421 {} ["scene" "count"] => value: i32 = int;
    SceneExists = 0x0422 { scene: usize } ["scene" scene "exists"] => value: bool = dflag;
    SceneNumber = 0x0423 { scene: usize } ["scene" scene "number"] => value: i32 = int;
    SceneName = 0x0424 { scene: usize } ["scene" scene "name"] => value: &'a str = text;
    SceneSelected = 0x0425 { scene: usize } ["scene" scene "select"] => value: bool = flag;
    SceneColor = 0x0426 { scene: usize } ["scene" scene "color"] => value: Option<[f64; 3]> = color_logged;
    ScenePosition = 0x0427 { scene: usize } ["scene" scene "position"] => value: f64 = f64;
    SceneEndPosition = 0x0428 { scene: usize } ["scene" scene "endPosition"] => value: f64 = f64;

    // --------------------------------------------------------------------------------------
    // 0x05 Devices, siblings, drum machine, pads, layers, EQ

    /// The number of devices of the cursor track (only for the cursor device).
    DeviceCount = 0x0501 { dev: Dev } [dev "count"] => value: i32 = int;
    DeviceExists = 0x0502 { dev: Dev } [dev "exists"] => value: bool = iflag;
    /// The position of the device in its chain, -1 for none.
    DevicePosition = 0x0503 { dev: Dev } [dev "position"] => value: i32 = int;
    DeviceBypass = 0x0504 { dev: Dev } [dev "bypass"] => value: bool = int_nonzero;
    DeviceName = 0x0505 { dev: Dev } [dev "name"] => value: &'a str = text;
    DeviceWindow = 0x0506 { dev: Dev } [dev "window"] => value: bool = dflag;
    DevicePinned = 0x0507 { dev: Dev } [dev "pinned"] => value: bool = dflag;
    DeviceExpand = 0x0508 { dev: Dev } [dev "expand"] => value: bool = dflag;
    /// The parameter page section is visible.
    DeviceParameters = 0x0509 { dev: Dev } [dev "parameters"] => value: bool = dflag;
    DeviceTouchedParam = 0x050A { dev: Dev } [dev "touchedParam"] => value: i32 = int;
    DeviceHasDrumPads = 0x050B { dev: Dev } [dev "hasDrumPads"] => value: bool = flag;
    DeviceHasLayers = 0x050C { dev: Dev } [dev "hasLayers"] => value: bool = flag;
    DeviceHasMutedPads = 0x050D { dev: Dev } [dev "hasMutedPads"] => value: bool = flag;
    DeviceHasSoloedPads = 0x050E { dev: Dev } [dev "hasSoloedPads"] => value: bool = flag;
    /// Accepted and ignored (the pads are always 128).
    DevicePadCount = 0x050F { dev: Dev } [dev "pad" "count"];
    DeviceLayerCount = 0x0510 { dev: Dev } [dev "layer" "count"] => value: i32 = int;
    /// The siblings of the cursor device (1-based in the text).
    DeviceSiblingName = 0x0520 { dev: Dev, sibling: Pos1 } [dev "sibling" sibling "name"] => value: &'a str = text;
    DeviceSiblingBypass = 0x0521 { dev: Dev, sibling: Pos1 } [dev "sibling" sibling "bypass"] => value: bool = int_nonzero;
    DeviceSiblingPosition = 0x0522 { dev: Dev, sibling: Pos1 } [dev "sibling" sibling "position"] => value: i32 = int;
    DeviceSiblingSelected = 0x0523 { dev: Dev, sibling: Pos1 } [dev "sibling" sibling "selected"] => value: bool = iflag;
    /// The type of an EQ band (Reaper type index, -1 off); Bitwig appends `/value`.
    EqBandType = 0x0530 { band: usize } ["eq" "band" band ("value")] => value: i32 = eq_band_type;

    /// The drum machines of the cursor track (the main and the additional drum devices).
    DrumExists = 0x0540 {} ["drum" "exists"] => value: bool = flag;
    DrumName = 0x0541 {} ["drum" "name"] => value: &'a str = text;
    DrumBypass = 0x0542 {} ["drum" "bypass"] => value: bool = flag;
    DrumWindow = 0x0543 {} ["drum" "window"] => value: bool = flag;
    DrumExpand = 0x0544 {} ["drum" "expand"] => value: bool = flag;
    DrumParameters = 0x0545 {} ["drum" "parameters"] => value: bool = flag_or_true;
    DrumHasDrumPads = 0x0546 {} ["drum" "hasDrumPads"] => value: bool = flag;
    DrumHasLayers = 0x0547 {} ["drum" "hasLayers"] => value: bool = flag;
    DrumHasMutedPads = 0x0548 {} ["drum" "hasMutedPads"] => value: bool = flag;
    DrumHasSoloedPads = 0x0549 {} ["drum" "hasSoloedPads"] => value: bool = flag;
    /// Accepted and ignored.
    DrumPadCount = 0x054A {} ["drum" "pad" "count"];

    PadExists = 0x0560 { pad: PadRef } [pad "exists"] => value: bool = flag;
    PadName = 0x0561 { pad: PadRef } [pad "name"] => value: &'a str = text;
    PadColor = 0x0562 { pad: PadRef } [pad "color"] => value: Option<[f64; 3]> = color;
    PadSelected = 0x0563 { pad: PadRef } [pad "select"] => value: bool = flag;
    PadActive = 0x0564 { pad: PadRef } [pad "active"] => value: bool = flag;
    PadMute = 0x0565 { pad: PadRef } [pad "mute"] => value: bool = flag;
    PadSolo = 0x0566 { pad: PadRef } [pad "solo"] => value: bool = flag;
    PadVolume = 0x0567 { pad: PadRef } [pad "volume"] => value: f64 = f64;
    PadVolumeStr = 0x0568 { pad: PadRef } [pad "volume" "str"] => value: &'a str = text;
    PadPan = 0x0569 { pad: PadRef } [pad "pan"] => value: f64 = f64;
    PadPanStr = 0x056A { pad: PadRef } [pad "pan" "str"] => value: &'a str = text;

    // --------------------------------------------------------------------------------------
    // 0x06 Parameters, pages, host parameters

    ParamCount = 0x0601 { owner: ParamOwner } [owner "param" "count"] => value: i32 = int;
    ParamName = 0x0602 { owner: ParamOwner, param: usize } [owner "param" param "name"] => value: &'a str = text;
    ParamValue = 0x0603 { owner: ParamOwner, param: usize } [owner "param" param "value"] => value: f64 = f64;
    ParamValueStr = 0x0604 { owner: ParamOwner, param: usize, limit: Option<usize> } [owner "param" param "value" "str" limit] => value: &'a str = text;
    ParamSteps = 0x0605 { owner: ParamOwner, param: usize } [owner "param" param "steps"] => value: i32 = int;
    ParamModulated = 0x0606 { owner: ParamOwner, param: usize } [owner "param" param "modulated"] => value: f64 = f64;
    PageCount = 0x0610 { owner: PageOwner } [owner "page" "count"] => value: i32 = int;
    PageSelected = 0x0611 { owner: PageOwner } [owner "page" "selected"] => value: i32 = int;
    PageName = 0x0612 { owner: PageOwner, page: usize } [owner "page" page "name"] => value: &'a str = text;
    /// The value of a host parameter; the parameter exists once its value is reported.
    HostParamValue = 0x0620 { param: HostParam } [param] => value: f64 = f64;
    HostParamStr = 0x0621 { param: HostParam, limit: Option<usize> } [param "str" limit] => value: &'a str = text;
    HostParamName = 0x0622 { param: HostParam } [param "name"] => value: &'a str = text;

    // --------------------------------------------------------------------------------------
    // 0x07 Clip, step grid

    ClipExists = 0x0701 {} ["clip" "exists"] => value: bool = dflag;
    ClipStart = 0x0702 {} ["clip" "start"] => value: f64 = f64;
    ClipEnd = 0x0703 {} ["clip" "end"] => value: f64 = f64;
    ClipPlayPosition = 0x0704 {} ["clip" "playposition"] => value: f64 = f64;
    ClipColor = 0x0705 {} ["clip" "color"] => value: Option<[f64; 3]> = color_logged;
    ClipLoop = 0x0706 {} ["clip" "loop"] => value: bool = dflag;
    /// The notes of the cursor clip (Reaper).
    ClipNotes = 0x0707 {} ["clip" "notes"] => value: Vec<Note> = notes;
    /// The step grids of the host: the clip values (all grids).
    ClipLoopStart = 0x0710 {} ["clip" "loopStart"] => value: f64 = f64_or_zero;
    ClipLoopLength = 0x0711 {} ["clip" "loopLength"] => value: f64 = f64_or_zero;
    ClipShuffle = 0x0712 {} ["clip" "shuffle"] => value: bool = flag;
    ClipAccent = 0x0713 {} ["clip" "accent"] => value: f64 = f64_or_zero;
    ClipPinned = 0x0714 {} ["clip" "pinned"] => value: bool = flag;
    GridPlayingStep = 0x0720 { grid: GridId } ["clip" "grid" grid "playingStep"] => value: i32 = int_or(-1);
    /// A step of a grid (channel, step, row); None: empty.
    GridStep = 0x0721 { grid: GridId, channel: usize, step: usize, row: usize } ["clip" "grid" grid "step" channel step row] => value: Option<StepInfo> = step;
    /// All clips of the project (Reaper).
    ClipAll = 0x0730 {} ["clip" "all"] => value: &'a str = text;

    // --------------------------------------------------------------------------------------
    // 0x08 Browser

    BrowserPresetsFile = 0x0801 {} ["browser" "presetsfile"] => value: &'a str = text;
    BrowserSelectedIndex = 0x0802 {} ["browser" "selected" "index"] => value: i32 = int;
    /// Accepted and ignored (Reaper).
    BrowserSelectedName = 0x0803 {} ["browser" "selected" "name"];
    BrowserExists = 0x0804 {} ["browser" "exists"] => value: bool = flag;
    BrowserPreview = 0x0805 {} ["browser" "preview"] => value: bool = flag;
    ContentTypeCount = 0x0810 {} ["browser" "contentType" "count"] => value: i32 = int_or(0);
    ContentTypeSelected = 0x0811 {} ["browser" "contentType" "selected"] => value: i32 = int_or(0);
    ContentTypeSelectedName = 0x0812 {} ["browser" "contentType" "selected" "name"] => value: &'a str = text;
    ContentTypeName = 0x0813 { index: usize } ["browser" "contentType" index "name"] => value: &'a str = text;
    ColumnCount = 0x0820 {} ["browser" "column" "count"] => value: i32 = int_or(0);
    ColumnExists = 0x0821 { column: usize } ["browser" "column" column "exists"] => value: bool = flag;
    ColumnName = 0x0822 { column: usize } ["browser" "column" column "name"] => value: &'a str = text;
    ColumnWildcard = 0x0823 { column: usize } ["browser" "column" column "wildcard"] => value: &'a str = text;
    ColumnCursorIndex = 0x0824 { column: usize } ["browser" "column" column "cursorIndex"] => value: i32 = int_or(-1);
    ColumnCursorExists = 0x0825 { column: usize } ["browser" "column" column "cursor" "exists"] => value: bool = flag;
    ColumnCursorName = 0x0826 { column: usize } ["browser" "column" column "cursor" "name"] => value: &'a str = text;
    ResultCursorName = 0x0830 {} ["browser" "result" "cursor" "name"] => value: &'a str = text;
    ItemExists = 0x0840 { item: BrowserItem } [item "exists"] => value: bool = flag;
    ItemName = 0x0841 { item: BrowserItem } [item "name"] => value: &'a str = text;
    ItemSelected = 0x0842 { item: BrowserItem } [item "select"] => value: bool = flag;
    ItemHits = 0x0843 { item: BrowserItem } [item "hits"] => value: i32 = int_or(0);

    // --------------------------------------------------------------------------------------
    // 0x09 Markers

    MarkerCount = 0x0901 {} ["marker" "count"] => value: i32 = int;
    MarkerExists = 0x0902 { marker: usize } ["marker" marker "exists"] => value: bool = dflag;
    MarkerNumber = 0x0903 { marker: usize } ["marker" marker "number"] => value: i32 = int;
    MarkerName = 0x0904 { marker: usize } ["marker" marker "name"] => value: &'a str = text;
    MarkerColor = 0x0905 { marker: usize } ["marker" marker "color"] => value: Option<[f64; 3]> = color_logged;
    /// Accepted and ignored.
    MarkerPosition = 0x0906 { marker: usize } ["marker" marker "position"];
    /// Accepted and ignored.
    MarkerEndPosition = 0x0907 { marker: usize } ["marker" marker "endPosition"];

    // --------------------------------------------------------------------------------------
    // 0x0B Note repeat, quantize, groove

    NoteRepeatActive = 0x0B01 {} ["noterepeat" "active"] => value: bool = dflag;
    /// The period (the core uses 1 / value).
    NoteRepeatPeriod = 0x0B02 {} ["noterepeat" "period"] => value: f64 = f64;
    NoteRepeatNoteLength = 0x0B03 {} ["noterepeat" "notelength"] => value: f64 = f64;
    NoteRepeatMode = 0x0B04 {} ["noterepeat" "mode"] => value: i32 = int;
    NoteRepeatVelocity = 0x0B05 {} ["noterepeat" "velocity"] => value: bool = iflag;
    QuantizeStrength = 0x0B10 {} ["quantize" "strength"] => value: i32 = int;
    /// Reaper's groove (Bitwig's are [`HostParam::Groove`]).
    GrooveActive = 0x0B20 {} ["groove" "active"] => value: f64 = f64;
    GrooveAmount = 0x0B21 {} ["groove" "amount"] => value: f64 = f64;
}

/// The trie of the table.
fn trie() -> &'static Trie<MsgId> {
    static TRIE: OnceLock<Trie<MsgId>> = OnceLock::new();
    TRIE.get_or_init(|| Trie::new(MsgId::ALL.iter().map(|id| (*id, id.patterns()))))
}

impl<'a> Msg<'a> {
    /// The message of a report line: the address (with or without the leading `/`) and the
    /// value. `log` receives the errors of the value parsers (e.g. a malformed color).
    pub fn from_text(address: &'a str, value: &'a str, log: rules::Log) -> Result<Msg<'a>, Miss> {
        let path = address.strip_prefix('/').unwrap_or(address);
        for id in trie().lookup(path) {
            match id.decode(path, value, log) {
                Err(Miss::Path) => {}
                result => return result,
            }
        }
        Err(Miss::Path)
    }
}

// ------------------------------------------------------------------------------------------
// Samples for the round-trip test

#[cfg(test)]
mod samples {
    use super::*;
    use crate::protocol::legacy::Sample;

    impl Sample for Ch {
        fn samples() -> Vec<Self> {
            vec![Ch::Track(3), Ch::Master, Ch::Effect(1), Ch::Track(0)]
        }
    }

    impl Sample for ParamOwner {
        fn samples() -> Vec<Self> {
            vec![ParamOwner::Device(Dev::Device), ParamOwner::Project, ParamOwner::TrackFx, ParamOwner::MasterFx, ParamOwner::Device(Dev::Eq), ParamOwner::Device(Dev::Primary)]
        }
    }

    impl Sample for PageOwner {
        fn samples() -> Vec<Self> {
            vec![PageOwner::Device(Dev::Device), PageOwner::Project, PageOwner::TrackFx, PageOwner::Device(Dev::Primary)]
        }
    }

    impl Sample for PadRef {
        fn samples() -> Vec<Self> {
            vec![PadRef::DrumPad(36), PadRef::Pad(Dev::Primary, 3), PadRef::Layer(Dev::Device, 1), PadRef::Pad(Dev::Eq, 0)]
        }
    }

    impl Sample for HostParam {
        fn samples() -> Vec<Self> {
            vec![HostParam::Crossfade, HostParam::Cue(CueParam::Mix), HostParam::Groove(GrooveParam::AccentPhase), HostParam::Cue(CueParam::Volume)]
        }
    }

    impl Sample for BrowserItem {
        fn samples() -> Vec<Self> {
            vec![BrowserItem::Column(2, 5), BrowserItem::Result(0)]
        }
    }

    impl Sample for Option<usize> {
        fn samples() -> Vec<Self> {
            vec![None, Some(8), Some(6)]
        }
    }

    impl Sample for Option<[f64; 3]> {
        fn samples() -> Vec<Self> {
            vec![Some([1.0, 0.0, 51.0 / 255.0]), None]
        }
    }

    impl Sample for Vec<Note> {
        fn samples() -> Vec<Self> {
            vec![vec![Note::new(true, false, 0.5, 1.25, 0, 60, 100), Note::new(false, true, 2.0, 3.0, 9, 36, 1)], vec![]]
        }
    }

    impl Sample for Option<StepInfo> {
        fn samples() -> Vec<Self> {
            let mut step = StepInfo::new();
            step.set_state(crate::daw::clip::StepState::Start);
            step.set_duration(0.25);
            step.set_velocity(0.75);
            vec![Some(step), None]
        }
    }

    impl Sample for ChannelType {
        fn samples() -> Vec<Self> {
            vec![ChannelType::Instrument, ChannelType::Master, ChannelType::GroupOpen]
        }
    }

    impl Sample for RecordQuantization {
        fn samples() -> Vec<Self> {
            RecordQuantization::ALL.to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn parse(line: &str) -> Result<Msg<'_>, Miss> {
        let (address, value) = line.split_once(' ').unwrap_or((line, ""));
        Msg::from_text(address, value, &mut |_| {})
    }

    #[test]
    fn ids_and_names_are_unique() {
        let ids: HashSet<u16> = MsgId::ALL.iter().map(|id| *id as u16).collect();
        assert_eq!(ids.len(), MsgId::ALL.len());
        assert!(MsgId::ALL.iter().all(|id| (*id as u16) & 0x8000 == 0), "host → core: high bit clear");
        let names: HashSet<&str> = MESSAGES.iter().map(|info| info.name).collect();
        assert_eq!(names.len(), MESSAGES.len());
        assert_eq!(MESSAGES.len(), MsgId::ALL.len());
    }

    #[test]
    fn the_table_is_unambiguous() {
        let ambiguous = trie().ambiguous();
        assert!(ambiguous.is_empty(), "paths with several entries: {ambiguous:?}");
    }

    /// Every message, with several samples of each field, is written as a line which parses back
    /// to the same message.
    #[test]
    fn every_message_round_trips_through_its_text() {
        for id in MsgId::ALL {
            for k in 0..8 {
                let msg = id.sample(k);
                let line = msg.to_text();
                assert_eq!(parse(&line), Ok(msg.clone()), "{id:?}: {line}");
            }
        }
    }

    #[test]
    fn fixed_texts() {
        assert_eq!(parse("/track/3/volume/str/8 -6 dB"), Ok(Msg::ChannelVolumeStr { ch: Ch::Track(3), limit: Some(8), value: "-6 dB" }));
        assert_eq!(parse("track/3/volume 0.5"), Ok(Msg::ChannelVolume { ch: Ch::Track(3), value: 0.5 }));
        assert_eq!(parse("/master/fx/param/0/value 1"), Ok(Msg::ParamValue { owner: ParamOwner::MasterFx, param: 0, value: 1.0 }));
        assert_eq!(parse("/eq/band/2 4"), Ok(Msg::EqBandType { band: 2, value: 4 }));
        assert_eq!(parse("/eq/band/2/value 4"), Ok(Msg::EqBandType { band: 2, value: 4 }));
        assert_eq!(parse("/device/sibling/1/name ReaEQ"), Ok(Msg::DeviceSiblingName { dev: Dev::Device, sibling: Pos1(0), value: "ReaEQ" }));
        assert_eq!(parse("/groove/shuffleAmount/str 50 %"), Ok(Msg::HostParamStr { param: HostParam::Groove(GrooveParam::ShuffleAmount), limit: None, value: "50 %" }));
        assert_eq!(parse("/track/offset 3"), Ok(Msg::TrackOffset {}));
        assert_eq!(parse("/track/3/exists abc"), Err(Miss::Value));
        assert_eq!(parse("/track/3/exists/more 1"), Err(Miss::Path));
        assert_eq!(parse("/track/x/exists 1"), Err(Miss::Path));
        assert_eq!(parse("/unknown 1"), Err(Miss::Path));
        assert_eq!(Msg::ChannelVolume { ch: Ch::Effect(2), value: 0.1 }.to_text(), "/fxtrack/2/volume 0.1");
        assert_eq!(MsgId::ChannelVolumeStr.info().template, "/{ch}/volume/str/{limit}");
        assert_eq!(MsgId::EqBandType.info().template, "/eq/band/{band}/[value]");
    }
}
