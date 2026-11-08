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
// The table

messages! {
    // --------------------------------------------------------------------------------------
    // 0x01 Transport, time, beats, click

    Play = 0x0101 { value: bool } ["play"] => value = dflag;
    /// Reaper: the play state is `value == 0`.
    Stop = 0x0102 { value: f64 } ["stop"] => value = f64;
    Repeat = 0x0103 { value: bool } ["repeat"] => value = dflag;
    Record = 0x0104 { value: bool } ["record"] => value = dflag;
    Tempo = 0x0105 { value: f64 } ["tempo"] => value = f64;
    /// The play position in seconds.
    Time = 0x0110 { value: f64 } ["time"] => value = f64;
    /// The play position formatted by the core (Reaper).
    TimeStr = 0x0111 { value: &'a str } ["time" "str"] => value = text;
    /// The play position formatted by the host, used as is (Bitwig).
    TimeText = 0x0112 { value: &'a str } ["time" "text"] => value = text;
    TimeHZoom = 0x0113 { value: f64 } ["time" "hzoom"] => value = f64;
    TimeLoopStart = 0x0114 { value: f64 } ["time" "loop" "start"] => value = f64;
    TimeLoopStartStr = 0x0115 { value: &'a str } ["time" "loop" "start" "str"] => value = text;
    TimeLoopStartBeat = 0x0116 { value: &'a str } ["time" "loop" "start" "beat"] => value = text;
    TimeLoopLength = 0x0117 { value: f64 } ["time" "loop" "length"] => value = f64;
    TimeLoopLengthStr = 0x0118 { value: &'a str } ["time" "loop" "length" "str"] => value = text;
    TimeLoopLengthBeat = 0x0119 { value: &'a str } ["time" "loop" "length" "beat"] => value = text;
    /// The play position in quarter notes.
    BeatsPosition = 0x0120 { value: f64 } ["beats" "position"] => value = f64;
    BeatsText = 0x0121 { value: &'a str } ["beats" "text"] => value = text;
    BeatsLoopStart = 0x0122 { value: f64 } ["beats" "loop" "start"] => value = f64;
    BeatsLoopLength = 0x0123 { value: f64 } ["beats" "loop" "length"] => value = f64;
    BeatsLoopStartText = 0x0124 { value: &'a str } ["beats" "loop" "start" "text"] => value = text;
    BeatsLoopLengthText = 0x0125 { value: &'a str } ["beats" "loop" "length" "text"] => value = text;
    /// The play position as a beat text (Reaper).
    Beat = 0x0126 { value: &'a str } ["beat"] => value = text;
    Numerator = 0x0130 { value: i32 } ["numerator"] => value = f64_as_int;
    Denominator = 0x0131 { value: i32 } ["denominator"] => value = f64_as_int;
    PunchIn = 0x0140 { value: bool } ["punchIn"] => value = flag;
    PunchOut = 0x0141 { value: bool } ["punchOut"] => value = flag;
    Fill = 0x0142 { value: bool } ["fill"] => value = flag;
    FollowPlayback = 0x0143 { value: bool } ["followPlayback"] => value = dflag;
    OverdubArranger = 0x0144 { value: bool } ["overdub" "arranger"] => value = flag;
    OverdubLauncher = 0x0145 { value: bool } ["overdub" "launcher"] => value = flag;
    /// The automation write mode as an int (0–5: trim read, read, touch, write, latch, latch
    /// preview); other values are ignored.
    AutomationMode = 0x0146 { value: i32 } ["automode"] => value = f64_as_int;
    AutomationArranger = 0x0147 { value: bool } ["automode" "arranger"] => value = flag;
    AutomationLauncher = 0x0148 { value: bool } ["automode" "launcher"] => value = flag;
    Metronome = 0x0150 { value: bool } ["click"] => value = dflag;
    /// Reaper: bit 2 is the pre-roll metronome.
    MetronomePreroll = 0x0151 { value: i32 } ["click" "preroll"] => value = int;
    MetronomePrerollMeasures = 0x0152 { value: i32 } ["click" "prerollMeasures"] => value = f64_as_int;
    MetronomeVolume = 0x0153 { value: f64 } ["click" "volume"] => value = f64;
    MetronomeVolumeStr = 0x0154 { value: &'a str } ["click" "volumeStr"] => value = text;
    MetronomeTicks = 0x0155 { value: bool } ["click" "ticks"] => value = flag;

    // --------------------------------------------------------------------------------------
    // 0x02 Application, project, host

    ProjectName = 0x0201 { value: &'a str } ["project" "name"] => value = text;
    ProjectEngine = 0x0202 { value: bool } ["project" "engine"] => value = iflag;
    ProjectCanUndo = 0x0203 { value: bool } ["project" "canUndo"] => value = iflag;
    ProjectCanRedo = 0x0204 { value: bool } ["project" "canRedo"] => value = iflag;
    ProjectIsDirty = 0x0205 { value: bool } ["project" "isDirty"] => value = iflag;
    ProjectHasSolo = 0x0206 { value: bool } ["project" "hasSolo"] => value = flag;
    ProjectHasMute = 0x0207 { value: bool } ["project" "hasMute"] => value = flag;
    /// The panel layout (`ARRANGE`, `MIX`, `EDIT`).
    Layout = 0x0210 { value: &'a str } ["layout"] => value = text;
    RecordQuantization = 0x0211 { value: RecordQuantization } ["recordQuantization"] => value = record_quantization;
    RecordQuantizationNoteLength = 0x0212 { value: bool } ["recordQuantization" "noteLength"] => value = flag;
    Arranger = 0x0213 { flag: ArrangerFlag, value: bool } ["arranger" flag] => value = flag;
    Mixer = 0x0214 { flag: MixerFlag, value: bool } ["mixer" flag] => value = flag;
    /// The command forms the host understands (space separated `HostFeature` names).
    HostFeatures = 0x0220 { value: &'a str } ["host" "features"] => value = text;
    /// An action was selected in Reaper (for the configuration dialog).
    ActionSelect = 0x0230 { value: &'a str } ["action" "select"] => value = text;

    // --------------------------------------------------------------------------------------
    // 0x03 Channels, sends, track banks

    TrackCount = 0x0301 { value: i32 } ["track" "count"] => value = int;
    /// The host pages the tracks (the value is ignored).
    TrackOffset = 0x0302 {} ["track" "offset"];
    /// The position of the pinned cursor track, -1 if not pinned (Bitwig).
    TrackPinned = 0x0303 { value: i32 } ["track" "pinned"] => value = int;
    TrackHasPrevious = 0x0304 { value: bool } ["track" "hasPrevious"] => value = flag;
    TrackHasNext = 0x0305 { value: bool } ["track" "hasNext"] => value = flag;
    EffectTrackCount = 0x0306 { value: i32 } ["fxtrack" "count"] => value = int;
    /// The host pages the effect tracks (the value is ignored).
    EffectTrackOffset = 0x0307 {} ["fxtrack" "offset"];

    ChannelExists = 0x0310 { ch: Ch, value: bool } [ch "exists"] => value = dflag;
    ChannelDepth = 0x0311 { ch: Ch, value: i32 } [ch "depth"] => value = int;
    ChannelActive = 0x0312 { ch: Ch, value: bool } [ch "active"] => value = dflag;
    ChannelCanHoldNotes = 0x0313 { ch: Ch, value: bool } [ch "canHoldNotes"] => value = flag;
    ChannelCanHoldAudio = 0x0314 { ch: Ch, value: bool } [ch "canHoldAudio"] => value = flag;
    ChannelType = 0x0315 { ch: Ch, value: ChannelType } [ch "type"] => value = channel_type;
    ChannelIsGroupExpanded = 0x0316 { ch: Ch, value: bool } [ch "isGroupExpanded"] => value = iflag;
    /// Only a change counts (a repeated selection does not recall views).
    ChannelSelected = 0x0317 { ch: Ch, value: bool } [ch "select"] => value = dflag;
    ChannelNumber = 0x0318 { ch: Ch, value: i32 } [ch "number"] => value = int;
    ChannelName = 0x0319 { ch: Ch, value: &'a str } [ch "name"] => value = text;
    ChannelVolume = 0x031A { ch: Ch, value: f64 } [ch "volume"] => value = f64;
    ChannelVolumeStr = 0x031B { ch: Ch, limit: Option<usize>, value: &'a str } [ch "volume" "str" limit] => value = text;
    ChannelPan = 0x031C { ch: Ch, value: f64 } [ch "pan"] => value = f64;
    ChannelPanStr = 0x031D { ch: Ch, limit: Option<usize>, value: &'a str } [ch "pan" "str" limit] => value = text;
    ChannelVu = 0x031E { ch: Ch, value: f64 } [ch "vu"] => value = f64;
    ChannelVuLeft = 0x031F { ch: Ch, value: f64 } [ch "vuleft"] => value = f64;
    ChannelVuRight = 0x0320 { ch: Ch, value: f64 } [ch "vuright"] => value = f64;
    ChannelVuHoldLeft = 0x0321 { ch: Ch, value: f64 } [ch "vuholdleft"] => value = f64;
    ChannelVuHoldRight = 0x0322 { ch: Ch, value: f64 } [ch "vuholdright"] => value = f64;
    ChannelMute = 0x0323 { ch: Ch, value: bool } [ch "mute"] => value = dflag;
    ChannelSolo = 0x0324 { ch: Ch, value: bool } [ch "solo"] => value = dflag;
    ChannelRecArm = 0x0325 { ch: Ch, value: bool } [ch "recarm"] => value = dflag;
    ChannelMonitor = 0x0326 { ch: Ch, value: bool } [ch "monitor"] => value = dflag;
    ChannelAutoMonitor = 0x0327 { ch: Ch, value: bool } [ch "autoMonitor"] => value = dflag;
    ChannelOverdub = 0x0328 { ch: Ch, value: bool } [ch "overdub"] => value = dflag;
    ChannelInQuantLengthEnabled = 0x0329 { ch: Ch, value: bool } [ch "inQuantLengthEnabled"] => value = dflag;
    ChannelInQuantResolution = 0x032A { ch: Ch, value: f64 } [ch "inQuantResolution"] => value = f64;
    ChannelColor = 0x032B { ch: Ch, value: Option<[f64; 3]> } [ch "color"] => value = color_logged;
    ChannelIsStopped = 0x032C { ch: Ch, value: bool } [ch "isStopped"] => value = dflag;
    /// The playing notes of a track (not of the master).
    ChannelPlayingNotes = 0x032D { ch: Ch, value: Vec<Note> } [ch "playingnotes"] => value = notes;

    SendCount = 0x0340 { ch: Ch, value: i32 } [ch "send" "count"] => value = int;
    SendActive = 0x0341 { ch: Ch, send: usize, value: bool } [ch "send" send "active"] => value = iflag_or_false;
    SendName = 0x0342 { ch: Ch, send: usize, value: &'a str } [ch "send" send "name"] => value = text;
    SendVolume = 0x0343 { ch: Ch, send: usize, value: f64 } [ch "send" send "volume"] => value = f64;
    SendVolumeStr = 0x0344 { ch: Ch, send: usize, limit: Option<usize>, value: &'a str } [ch "send" send "volume" "str" limit] => value = text;
    SendColor = 0x0345 { ch: Ch, send: usize, value: Option<[f64; 3]> } [ch "send" send "color"] => value = color;

    // --------------------------------------------------------------------------------------
    // 0x04 Slots and scenes

    SlotCount = 0x0401 { ch: Ch, value: i32 } [ch "slot" "count"] => value = int;
    SlotExists = 0x0402 { ch: Ch, slot: usize, value: bool } [ch "slot" slot "exists"] => value = flag;
    SlotHasContent = 0x0403 { ch: Ch, slot: usize, value: bool } [ch "slot" slot "hasContent"] => value = flag;
    SlotName = 0x0404 { ch: Ch, slot: usize, value: &'a str } [ch "slot" slot "name"] => value = text;
    SlotColor = 0x0405 { ch: Ch, slot: usize, value: Option<[f64; 3]> } [ch "slot" slot "color"] => value = color;
    SlotSelected = 0x0406 { ch: Ch, slot: usize, value: bool } [ch "slot" slot "selected"] => value = flag;
    SlotPlaying = 0x0407 { ch: Ch, slot: usize, value: bool } [ch "slot" slot "playing"] => value = flag;
    SlotRecording = 0x0408 { ch: Ch, slot: usize, value: bool } [ch "slot" slot "recording"] => value = flag;
    SlotPlayQueued = 0x0409 { ch: Ch, slot: usize, value: bool } [ch "slot" slot "playQueued"] => value = flag;
    SlotRecQueued = 0x040A { ch: Ch, slot: usize, value: bool } [ch "slot" slot "recQueued"] => value = flag;
    SlotStopQueued = 0x040B { ch: Ch, slot: usize, value: bool } [ch "slot" slot "stopQueued"] => value = flag;

    /// The host pages the scenes (the value is ignored).
    SceneOffset = 0x0420 {} ["scene" "offset"];
    SceneCount = 0x0421 { value: i32 } ["scene" "count"] => value = int;
    SceneExists = 0x0422 { scene: usize, value: bool } ["scene" scene "exists"] => value = dflag;
    SceneNumber = 0x0423 { scene: usize, value: i32 } ["scene" scene "number"] => value = int;
    SceneName = 0x0424 { scene: usize, value: &'a str } ["scene" scene "name"] => value = text;
    SceneSelected = 0x0425 { scene: usize, value: bool } ["scene" scene "select"] => value = flag;
    SceneColor = 0x0426 { scene: usize, value: Option<[f64; 3]> } ["scene" scene "color"] => value = color_logged;
    ScenePosition = 0x0427 { scene: usize, value: f64 } ["scene" scene "position"] => value = f64;
    SceneEndPosition = 0x0428 { scene: usize, value: f64 } ["scene" scene "endPosition"] => value = f64;

    // --------------------------------------------------------------------------------------
    // 0x05 Devices, siblings, drum machine, pads, layers, EQ

    /// The number of devices of the cursor track (only for the cursor device).
    DeviceCount = 0x0501 { dev: Dev, value: i32 } [dev "count"] => value = int;
    DeviceExists = 0x0502 { dev: Dev, value: bool } [dev "exists"] => value = iflag;
    /// The position of the device in its chain, -1 for none.
    DevicePosition = 0x0503 { dev: Dev, value: i32 } [dev "position"] => value = int;
    DeviceBypass = 0x0504 { dev: Dev, value: bool } [dev "bypass"] => value = int_nonzero;
    DeviceName = 0x0505 { dev: Dev, value: &'a str } [dev "name"] => value = text;
    DeviceWindow = 0x0506 { dev: Dev, value: bool } [dev "window"] => value = dflag;
    DevicePinned = 0x0507 { dev: Dev, value: bool } [dev "pinned"] => value = dflag;
    DeviceExpand = 0x0508 { dev: Dev, value: bool } [dev "expand"] => value = dflag;
    /// The parameter page section is visible.
    DeviceParameters = 0x0509 { dev: Dev, value: bool } [dev "parameters"] => value = dflag;
    DeviceTouchedParam = 0x050A { dev: Dev, value: i32 } [dev "touchedParam"] => value = int;
    DeviceHasDrumPads = 0x050B { dev: Dev, value: bool } [dev "hasDrumPads"] => value = flag;
    DeviceHasLayers = 0x050C { dev: Dev, value: bool } [dev "hasLayers"] => value = flag;
    DeviceHasMutedPads = 0x050D { dev: Dev, value: bool } [dev "hasMutedPads"] => value = flag;
    DeviceHasSoloedPads = 0x050E { dev: Dev, value: bool } [dev "hasSoloedPads"] => value = flag;
    /// Accepted and ignored (the pads are always 128).
    DevicePadCount = 0x050F { dev: Dev } [dev "pad" "count"];
    DeviceLayerCount = 0x0510 { dev: Dev, value: i32 } [dev "layer" "count"] => value = int;
    /// The siblings of the cursor device (1-based in the text).
    DeviceSiblingName = 0x0520 { dev: Dev, sibling: Pos1, value: &'a str } [dev "sibling" sibling "name"] => value = text;
    DeviceSiblingBypass = 0x0521 { dev: Dev, sibling: Pos1, value: bool } [dev "sibling" sibling "bypass"] => value = int_nonzero;
    DeviceSiblingPosition = 0x0522 { dev: Dev, sibling: Pos1, value: i32 } [dev "sibling" sibling "position"] => value = int;
    DeviceSiblingSelected = 0x0523 { dev: Dev, sibling: Pos1, value: bool } [dev "sibling" sibling "selected"] => value = iflag;
    /// The type of an EQ band (Reaper type index, -1 off); Bitwig appends `/value`.
    EqBandType = 0x0530 { band: usize, value: i32 } ["eq" "band" band ("value")] => value = eq_band_type;

    /// The drum machines of the cursor track (the main and the additional drum devices).
    DrumExists = 0x0540 { value: bool } ["drum" "exists"] => value = flag;
    DrumName = 0x0541 { value: &'a str } ["drum" "name"] => value = text;
    DrumBypass = 0x0542 { value: bool } ["drum" "bypass"] => value = flag;
    DrumWindow = 0x0543 { value: bool } ["drum" "window"] => value = flag;
    DrumExpand = 0x0544 { value: bool } ["drum" "expand"] => value = flag;
    DrumParameters = 0x0545 { value: bool } ["drum" "parameters"] => value = flag_or_true;
    DrumHasDrumPads = 0x0546 { value: bool } ["drum" "hasDrumPads"] => value = flag;
    DrumHasLayers = 0x0547 { value: bool } ["drum" "hasLayers"] => value = flag;
    DrumHasMutedPads = 0x0548 { value: bool } ["drum" "hasMutedPads"] => value = flag;
    DrumHasSoloedPads = 0x0549 { value: bool } ["drum" "hasSoloedPads"] => value = flag;
    /// Accepted and ignored.
    DrumPadCount = 0x054A {} ["drum" "pad" "count"];

    PadExists = 0x0560 { pad: PadRef, value: bool } [pad "exists"] => value = flag;
    PadName = 0x0561 { pad: PadRef, value: &'a str } [pad "name"] => value = text;
    PadColor = 0x0562 { pad: PadRef, value: Option<[f64; 3]> } [pad "color"] => value = color;
    PadSelected = 0x0563 { pad: PadRef, value: bool } [pad "select"] => value = flag;
    PadActive = 0x0564 { pad: PadRef, value: bool } [pad "active"] => value = flag;
    PadMute = 0x0565 { pad: PadRef, value: bool } [pad "mute"] => value = flag;
    PadSolo = 0x0566 { pad: PadRef, value: bool } [pad "solo"] => value = flag;
    PadVolume = 0x0567 { pad: PadRef, value: f64 } [pad "volume"] => value = f64;
    PadVolumeStr = 0x0568 { pad: PadRef, value: &'a str } [pad "volume" "str"] => value = text;
    PadPan = 0x0569 { pad: PadRef, value: f64 } [pad "pan"] => value = f64;
    PadPanStr = 0x056A { pad: PadRef, value: &'a str } [pad "pan" "str"] => value = text;

    // --------------------------------------------------------------------------------------
    // 0x06 Parameters, pages, host parameters

    ParamCount = 0x0601 { owner: ParamOwner, value: i32 } [owner "param" "count"] => value = int;
    ParamName = 0x0602 { owner: ParamOwner, param: usize, value: &'a str } [owner "param" param "name"] => value = text;
    ParamValue = 0x0603 { owner: ParamOwner, param: usize, value: f64 } [owner "param" param "value"] => value = f64;
    ParamValueStr = 0x0604 { owner: ParamOwner, param: usize, limit: Option<usize>, value: &'a str } [owner "param" param "value" "str" limit] => value = text;
    ParamSteps = 0x0605 { owner: ParamOwner, param: usize, value: i32 } [owner "param" param "steps"] => value = int;
    ParamModulated = 0x0606 { owner: ParamOwner, param: usize, value: f64 } [owner "param" param "modulated"] => value = f64;
    PageCount = 0x0610 { owner: PageOwner, value: i32 } [owner "page" "count"] => value = int;
    PageSelected = 0x0611 { owner: PageOwner, value: i32 } [owner "page" "selected"] => value = int;
    PageName = 0x0612 { owner: PageOwner, page: usize, value: &'a str } [owner "page" page "name"] => value = text;
    /// The value of a host parameter; the parameter exists once its value is reported.
    HostParamValue = 0x0620 { param: HostParam, value: f64 } [param] => value = f64;
    HostParamStr = 0x0621 { param: HostParam, limit: Option<usize>, value: &'a str } [param "str" limit] => value = text;
    HostParamName = 0x0622 { param: HostParam, value: &'a str } [param "name"] => value = text;

    // --------------------------------------------------------------------------------------
    // 0x07 Clip, step grid

    ClipExists = 0x0701 { value: bool } ["clip" "exists"] => value = dflag;
    ClipStart = 0x0702 { value: f64 } ["clip" "start"] => value = f64;
    ClipEnd = 0x0703 { value: f64 } ["clip" "end"] => value = f64;
    ClipPlayPosition = 0x0704 { value: f64 } ["clip" "playposition"] => value = f64;
    ClipColor = 0x0705 { value: Option<[f64; 3]> } ["clip" "color"] => value = color_logged;
    ClipLoop = 0x0706 { value: bool } ["clip" "loop"] => value = dflag;
    /// The notes of the cursor clip (Reaper).
    ClipNotes = 0x0707 { value: Vec<Note> } ["clip" "notes"] => value = notes;
    /// The step grids of the host: the clip values (all grids).
    ClipLoopStart = 0x0710 { value: f64 } ["clip" "loopStart"] => value = f64_or_zero;
    ClipLoopLength = 0x0711 { value: f64 } ["clip" "loopLength"] => value = f64_or_zero;
    ClipShuffle = 0x0712 { value: bool } ["clip" "shuffle"] => value = flag;
    ClipAccent = 0x0713 { value: f64 } ["clip" "accent"] => value = f64_or_zero;
    ClipPinned = 0x0714 { value: bool } ["clip" "pinned"] => value = flag;
    GridPlayingStep = 0x0720 { grid: GridId, value: i32 } ["clip" "grid" grid "playingStep"] => value = int_or(-1);
    /// A step of a grid (channel, step, row); None: empty.
    GridStep = 0x0721 { grid: GridId, channel: usize, step: usize, row: usize, value: Option<StepInfo> } ["clip" "grid" grid "step" channel step row] => value = step;
    /// All clips of the project (Reaper).
    ClipAll = 0x0730 { value: &'a str } ["clip" "all"] => value = text;

    // --------------------------------------------------------------------------------------
    // 0x08 Browser

    BrowserPresetsFile = 0x0801 { value: &'a str } ["browser" "presetsfile"] => value = text;
    BrowserSelectedIndex = 0x0802 { value: i32 } ["browser" "selected" "index"] => value = int;
    /// Accepted and ignored (Reaper).
    BrowserSelectedName = 0x0803 {} ["browser" "selected" "name"];
    BrowserExists = 0x0804 { value: bool } ["browser" "exists"] => value = flag;
    BrowserPreview = 0x0805 { value: bool } ["browser" "preview"] => value = flag;
    ContentTypeCount = 0x0810 { value: i32 } ["browser" "contentType" "count"] => value = int_or(0);
    ContentTypeSelected = 0x0811 { value: i32 } ["browser" "contentType" "selected"] => value = int_or(0);
    ContentTypeSelectedName = 0x0812 { value: &'a str } ["browser" "contentType" "selected" "name"] => value = text;
    ContentTypeName = 0x0813 { index: usize, value: &'a str } ["browser" "contentType" index "name"] => value = text;
    ColumnCount = 0x0820 { value: i32 } ["browser" "column" "count"] => value = int_or(0);
    ColumnExists = 0x0821 { column: usize, value: bool } ["browser" "column" column "exists"] => value = flag;
    ColumnName = 0x0822 { column: usize, value: &'a str } ["browser" "column" column "name"] => value = text;
    ColumnWildcard = 0x0823 { column: usize, value: &'a str } ["browser" "column" column "wildcard"] => value = text;
    ColumnCursorIndex = 0x0824 { column: usize, value: i32 } ["browser" "column" column "cursorIndex"] => value = int_or(-1);
    ColumnCursorExists = 0x0825 { column: usize, value: bool } ["browser" "column" column "cursor" "exists"] => value = flag;
    ColumnCursorName = 0x0826 { column: usize, value: &'a str } ["browser" "column" column "cursor" "name"] => value = text;
    ResultCursorName = 0x0830 { value: &'a str } ["browser" "result" "cursor" "name"] => value = text;
    ItemExists = 0x0840 { item: BrowserItem, value: bool } [item "exists"] => value = flag;
    ItemName = 0x0841 { item: BrowserItem, value: &'a str } [item "name"] => value = text;
    ItemSelected = 0x0842 { item: BrowserItem, value: bool } [item "select"] => value = flag;
    ItemHits = 0x0843 { item: BrowserItem, value: i32 } [item "hits"] => value = int_or(0);

    // --------------------------------------------------------------------------------------
    // 0x09 Markers

    MarkerCount = 0x0901 { value: i32 } ["marker" "count"] => value = int;
    MarkerExists = 0x0902 { marker: usize, value: bool } ["marker" marker "exists"] => value = dflag;
    MarkerNumber = 0x0903 { marker: usize, value: i32 } ["marker" marker "number"] => value = int;
    MarkerName = 0x0904 { marker: usize, value: &'a str } ["marker" marker "name"] => value = text;
    MarkerColor = 0x0905 { marker: usize, value: Option<[f64; 3]> } ["marker" marker "color"] => value = color_logged;
    /// Accepted and ignored.
    MarkerPosition = 0x0906 { marker: usize } ["marker" marker "position"];
    /// Accepted and ignored.
    MarkerEndPosition = 0x0907 { marker: usize } ["marker" marker "endPosition"];

    // --------------------------------------------------------------------------------------
    // 0x0B Note repeat, quantize, groove

    NoteRepeatActive = 0x0B01 { value: bool } ["noterepeat" "active"] => value = dflag;
    /// The period (the core uses 1 / value).
    NoteRepeatPeriod = 0x0B02 { value: f64 } ["noterepeat" "period"] => value = f64;
    NoteRepeatNoteLength = 0x0B03 { value: f64 } ["noterepeat" "notelength"] => value = f64;
    NoteRepeatMode = 0x0B04 { value: i32 } ["noterepeat" "mode"] => value = int;
    NoteRepeatVelocity = 0x0B05 { value: bool } ["noterepeat" "velocity"] => value = iflag;
    QuantizeStrength = 0x0B10 { value: i32 } ["quantize" "strength"] => value = int;
    /// Reaper's groove (Bitwig's are [`HostParam::Groove`]).
    GrooveActive = 0x0B20 { value: f64 } ["groove" "active"] => value = f64;
    GrooveAmount = 0x0B21 { value: f64 } ["groove" "amount"] => value = f64;
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
