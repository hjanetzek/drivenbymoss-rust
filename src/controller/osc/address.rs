// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The table of the OSC addresses of the controller, both directions.
//!
//! **Incoming** ([`OscIn`]): every command the modules understand is one entry of the
//! `osc_in!` table: a typed variant with the indices of its address (`/track/{n}/volume` →
//! `OscIn::Track(TrackIn::Volume { track: TrackRef::Bank(n - 1), op })`), the address template
//! and the kind of its value ([`ValueKind`]). [`parse`] reads an address segment by segment: each
//! segment is looked up in a `keywords!` table (or read as a number) and selects the next part
//! of the grammar, so the modules only `match` the typed commands (exhaustively, per module).
//!
//! The parser keeps the errors of the Java parser exactly (`OSCParser` and the `execute` methods
//! of the modules, which read the address while executing it): a missing part is
//! [`OscError::MissingCommand`], an unknown part [`OscError::UnknownCommand`] with that part (or
//! the module name, where Java reports that), a Java runtime exception (a non-numeric send
//! index, an index 0) [`OscError::Runtime`]. Where Java only reads the rest of an address in a
//! certain state (e.g. `/track/selected/...` only if there is a cursor track, the drum pads only
//! if the device has some), the parser records a [`Guard`]: the setup checks the guards of a
//! command in their order before it executes the command or reports its error, so a command
//! whose guard does not hold does nothing, as in Java. A value is converted when the command is
//! executed (as in Java, after the address is read).
//!
//! **Outgoing** ([`TrackKeys`] etc.): the values a module sends, as typed keys of the
//! [`OscWriter`](crate::framework::osc::OscWriter). A key group is created once per address prefix (`/track/1/`, `/master/`, ...)
//! and interns its addresses in the writer, which then compares the values by key; no address
//! is formatted when the modules are flushed. Lists of a dynamic size (sends, clips, parameters,
//! browser items) are [`KeyList`](crate::framework::osc::writer::KeyList)s, which create the keys of an index when it is used first.

use super::module::OscError;
use crate::config::osc::NUM_ASSIGNABLE_ACTIONS;
use crate::daw::events::ParameterBankId;
use crate::framework::osc::writer::osc_keys;

// ---------------------------------------------------------------------------------------------
// The table macros

/// A set of literal segments: an enum with the text of each variant and a lookup.
macro_rules! keywords {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident = $text:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($variant),*
        }

        impl $name {
            /// All variants, in the order of the table.
            #[allow(dead_code)]
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            /// The segment of the variant.
            #[allow(dead_code)]
            pub fn text(self) -> &'static str {
                match self {
                    $($name::$variant => $text),*
                }
            }

            /// The variant of a segment.
            pub fn parse(segment: &str) -> Option<Self> {
                match segment {
                    $($text => Some($name::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

/// The table of the incoming commands: one group (enum) per module, one entry per command with
/// its typed fields, its address template and the kind of its value.
///
/// Template syntax (used by the documentation and the tests): segments separated by `/`, a
/// segment is a literal, alternatives `a|b`, or a placeholder in braces (`{n}` a 1-based
/// number, `{track}` `track/{n}`, `track/selected` or `master`, see the test samples).
macro_rules! osc_in {
    ($(
        $(#[$gmeta:meta])*
        $group:ident($genum:ident) {
            $(
                $(#[$vmeta:meta])*
                $variant:ident { $($field:ident : $fty:ty),* $(,)? } = $template:literal : $kind:ident
            ),* $(,)?
        }
    )*) => {
        /// An incoming command: the typed form of an address, by module.
        #[derive(Debug, Clone, PartialEq)]
        pub enum OscIn {
            $($(#[$gmeta])* $group($genum),)*
        }

        $(
            $(#[$gmeta])*
            #[derive(Debug, Clone, PartialEq)]
            pub enum $genum {
                $($(#[$vmeta])* $variant { $($field: $fty),* },)*
            }

            impl $genum {
                /// The table entries of the group.
                pub const INFO: &'static [OscInInfo] = &[
                    $(OscInInfo { module: stringify!($group), name: stringify!($variant), template: $template, value: ValueKind::$kind },)*
                ];

                /// The name of the entry.
                pub fn name(&self) -> &'static str {
                    match self {
                        $($genum::$variant { .. } => stringify!($variant),)*
                    }
                }
            }
        )*

        impl OscIn {
            /// The name of the entry (`Track/Volume`).
            pub fn name(&self) -> (&'static str, &'static str) {
                match self {
                    $(OscIn::$group(command) => (stringify!($group), command.name()),)*
                }
            }

            /// All entries of the table.
            pub fn table() -> impl Iterator<Item = &'static OscInInfo> {
                [$($genum::INFO,)*].into_iter().flatten()
            }
        }
    };
}

/// An entry of the table of the incoming commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OscInInfo {
    pub module: &'static str,
    pub name: &'static str,
    pub template: &'static str,
    pub value: ValueKind,
}

/// The kind of the value of a command (for the documentation; the modules convert the value
/// when they execute the command, with the helpers of [`super::module`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// The value is ignored.
    None,
    /// A trigger: no value or a number > 0 (`isTrigger`).
    Trigger,
    /// No value toggles, otherwise a trigger sets it.
    Toggle,
    /// An integer (`toInteger`).
    Int,
    /// A number (`toNumber`).
    Number,
    /// A text (`toString`).
    Text,
    /// A color `rgb(r,g,b)`.
    Color,
    /// Two numbers: note and velocity.
    NoteVelocity,
}

// ---------------------------------------------------------------------------------------------
// The segments

keywords! {
    /// The top level addresses and their modules (former `OSCParser.modules`).
    pub enum Top {
        Play = "play", PlayButton = "playbutton", Stop = "stop", Restart = "restart", Record = "record", Overdub = "overdub", Repeat = "repeat",
        PunchIn = "punchIn", PunchOut = "punchOut", Click = "click", Quantize = "quantize", Tempo = "tempo", Time = "time", Position = "position",
        Crossfade = "crossfade", Autowrite = "autowrite", AutomationWriteMode = "automationWriteMode", Preroll = "preroll", Launcher = "launcher",
        Undo = "undo", Redo = "redo",
        Layout = "layout", Panel = "panel", Arranger = "arranger", Mixer = "mixer",
        Marker = "marker",
        Project = "project",
        Track = "track", Master = "master",
        Scene = "scene",
        Device = "device", Primary = "primary", Eq = "eq",
        Browser = "browser",
        VkbMidi = "vkb_midi",
        Action = "action",
        Clip = "clip",
        Refresh = "refresh",
    }
}

keywords! {
    /// `+` and `-`.
    pub enum Dir { Next = "+", Previous = "-" }
}

impl Dir {
    /// Java `"+".equals (direction) ? next : previous`.
    fn loose(segment: &str) -> Dir {
        if segment == "+" {
            Dir::Next
        } else {
            Dir::Previous
        }
    }
}

keywords! {
    pub enum ClickKw { Volume = "volume", Ticks = "ticks", Preroll = "preroll" }
}

keywords! {
    pub enum TempoKw { Raw = "raw", Tap = "tap", Inc = "+", Dec = "-" }
}

keywords! {
    /// `/position/<move>`.
    pub enum PositionMove { Forward = "+", Back = "-", FastForward = "++", FastBack = "--", Start = "start" }
}

keywords! {
    pub enum LauncherKw { PostRecordingAction = "postRecordingAction", PostRecordingTimeOffset = "postRecordingTimeOffset", DefaultQuantization = "defaultQuantization" }
}

keywords! {
    /// `/panel/<panel>`.
    pub enum Panel { NoteEditor = "noteEditor", AutomationEditor = "automationEditor", Devices = "devices", Mixer = "mixer", Fullscreen = "fullscreen" }
}

keywords! {
    /// `/arranger/<toggle>`.
    pub enum ArrangerToggle {
        CueMarkerVisibility = "cueMarkerVisibility", PlaybackFollow = "playbackFollow", TrackRowHeight = "trackRowHeight",
        ClipLauncherSectionVisibility = "clipLauncherSectionVisibility", TimeLineVisibility = "timeLineVisibility",
        IoSectionVisibility = "ioSectionVisibility", EffectTracksVisibility = "effectTracksVisibility",
    }
}

keywords! {
    /// `/mixer/<toggle>`.
    pub enum MixerToggle {
        ClipLauncherSectionVisibility = "clipLauncherSectionVisibility", CrossFadeSectionVisibility = "crossFadeSectionVisibility",
        DeviceSectionVisibility = "deviceSectionVisibility", SendsSectionVisibility = "sendsSectionVisibility",
        IoSectionVisibility = "ioSectionVisibility", MeterSectionVisibility = "meterSectionVisibility",
    }
}

keywords! {
    pub enum ProjectKw { Next = "+", Previous = "-", Engine = "engine", Save = "save", Param = "param", Page = "page" }
}

keywords! {
    /// The operation on a parameter (former `parseFXParamValue`).
    pub enum ParamOp { Value = "value", Indicate = "indicate", Reset = "reset", Touched = "touched" }
}

keywords! {
    pub enum TrackCommandKw {
        Indicate = "indicate", Bank = "bank", Next = "+", Previous = "-", Add = "add", Stop = "stop", StopAlt = "stopAlt", Vu = "vu",
        ToggleBank = "toggleBank", Parent = "parent", Select = "select", Selected = "selected", Param = "param", Page = "page",
    }
}

keywords! {
    pub enum TrackValueKw {
        Name = "name", Activated = "activated", CrossfadeMode = "crossfadeMode", RecordQuantization = "recordQuantization", Select = "select",
        Selected = "selected", Duplicate = "duplicate", Remove = "remove", Volume = "volume", Pan = "pan", Mute = "mute", Solo = "solo",
        RecArm = "recarm", Monitor = "monitor", AutoMonitor = "autoMonitor", Send = "send", Clip = "clip", Enter = "enter", Color = "color",
        Pinned = "pinned",
    }
}

keywords! {
    /// The flags of a track which are toggled without a value.
    pub enum TrackFlag { Mute = "mute", Solo = "solo", RecArm = "recarm", Monitor = "monitor", AutoMonitor = "autoMonitor" }
}

keywords! {
    /// `/track/add/<kind>`.
    pub enum AddTrack { Audio = "audio", Effect = "effect", Instrument = "instrument" }
}

keywords! {
    pub enum IndicateKw { Volume = "volume", Pan = "pan", Send = "send" }
}

keywords! {
    /// The commands of a send.
    pub enum SendKw { Activated = "activated", Volume = "volume" }
}

keywords! {
    pub enum SlotNavKw { Stop = "stop", ReturnToArrangement = "returntoarrangement" }
}

keywords! {
    pub enum FaderKw { Indicate = "indicate", Reset = "reset", Touched = "touched" }
}

keywords! {
    pub enum SlotKw {
        Select = "select", Selected = "selected", Launch = "launch", LaunchAlt = "launchAlt", Record = "record", Create = "create",
        InsertFile = "insertFile", Duplicate = "duplicate", Remove = "remove", Color = "color",
    }
}

keywords! {
    pub enum SceneKw { Bank = "bank", Next = "+", Previous = "-", Create = "create", Add = "add" }
}

keywords! {
    pub enum SceneCmd { Select = "select", Launch = "launch", LaunchAlt = "launchAlt", Duplicate = "duplicate", Remove = "remove", Name = "name", Color = "color" }
}

keywords! {
    pub enum CursorDeviceKw { Sibling = "sibling", Bank = "bank", LastParam = "lastparam", Next = "+", Previous = "-", Pinned = "pinned" }
}

keywords! {
    pub enum DeviceKw {
        Page = "page", Duplicate = "duplicate", Remove = "remove", Bypass = "bypass", Expand = "expand", Parameters = "parameters",
        Window = "window", Indicate = "indicate", Param = "param", Drumpad = "drumpad", Layer = "layer",
    }
}

keywords! {
    /// The device commands without a value.
    pub enum DeviceToggle { Duplicate = "duplicate", Remove = "remove", Bypass = "bypass", Expand = "expand", Parameters = "parameters", Window = "window" }
}

keywords! {
    pub enum LayerNavKw { Parent = "parent", Next = "+", Previous = "-", Page = "page" }
}

keywords! {
    pub enum LayerKw {
        Activated = "activated", Select = "select", Selected = "selected", Name = "name", Volume = "volume", Pan = "pan", Mute = "mute",
        Solo = "solo", Send = "send", Enter = "enter",
    }
}

keywords! {
    pub enum EqKw { Type = "type", Gain = "gain", Freq = "freq", Q = "q", Add = "add" }
}

keywords! {
    /// The parameters of an equalizer band.
    pub enum EqParam { Gain = "gain", Freq = "freq", Q = "q" }
}

keywords! {
    pub enum BrowserKw {
        Preset = "preset", Tab = "tab", Device = "device", InsertClap = "insertClap", Commit = "commit", Cancel = "cancel", Filter = "filter",
        Result = "result",
    }
}

keywords! {
    /// `/browser/filter/{n}/<op>`.
    pub enum FilterOp { Next = "+", Previous = "-", Reset = "reset" }
}

keywords! {
    pub enum MidiKw { Velocity = "velocity", NoteRepeat = "noterepeat" }
}

keywords! {
    /// `/vkb_midi/noterepeat/<param>`.
    pub enum NoteRepeatParam { IsActive = "isActive", Period = "period", Length = "length" }
}

keywords! {
    pub enum MidiChannelKw { Note = "note", Drum = "drum", Cc = "cc", Aftertouch = "aftertouch", Pitchbend = "pitchbend" }
}

keywords! {
    pub enum ClipKw {
        StopAll = "stopall", StopAllAlt = "stopallAlt", Pinned = "pinned", Quantize = "quantize", Name = "name", Color = "color", Stop = "stop",
        StopAlt = "stopAlt", Next = "+", Previous = "-", Launch = "launch", LaunchAlt = "launchAlt", Record = "record", Create = "create",
        InsertFile = "insertFile",
    }
}

// ---------------------------------------------------------------------------------------------
// The typed fields

/// The track which a command addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackRef {
    /// The track at the index of the page of the current track bank.
    Bank(usize),
    /// The master track.
    Master,
    /// The cursor (selected or pinned) track.
    Cursor,
}

/// The device which a command addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRef {
    Cursor,
    Primary,
    Eq,
}

/// A parameter bank of the project, the cursor track or a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamBank {
    Project,
    CursorTrack,
    Device(DeviceRef),
}

impl ParamBank {
    pub fn id(self) -> ParameterBankId {
        match self {
            ParamBank::Project => ParameterBankId::Project,
            ParamBank::CursorTrack => ParameterBankId::CursorTrack,
            ParamBank::Device(DeviceRef::Cursor) => ParameterBankId::CursorDevice,
            ParamBank::Device(DeviceRef::Primary) => ParameterBankId::FirstInstrument,
            ParamBank::Device(DeviceRef::Eq) => ParameterBankId::Eq,
        }
    }
}

/// The operation on a volume or panorama: set it (no further part) or one of the sub commands;
/// another part is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaderOp {
    Set,
    Indicate,
    Reset,
    Touched,
    Ignore,
}

/// The page of `page/{n}` (`n - 1`) or the value of `page/select`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageArg {
    FromValue,
    Index(i32),
}

/// The indication of all tracks of the bank (`/track/indicate/...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicate {
    Volume,
    Pan,
    Send(usize),
}

/// A command of a clip slot of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotCmd {
    Select,
    Launch { alt: bool },
    Record,
    Create,
    InsertFile,
    Duplicate,
    Remove,
    Color,
}

/// A layer or drum pad: the selected one or `n - 1` of the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerIndex {
    Selected,
    Number(i32),
}

/// A command of a layer or drum pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerCmd {
    Activated,
    Select,
    Name,
    Volume(FaderOp),
    Pan(FaderOp),
    Mute,
    Solo,
    SendVolume(usize, FaderOp),
    SendActivated(usize),
    /// `send/{n}` without a command does nothing.
    SendNothing(usize),
    Enter,
}

// ---------------------------------------------------------------------------------------------
// The table of the incoming commands

osc_in! {
    /// The cache of all values (`/refresh`).
    Setup(SetupIn) {
        Refresh {} = "/refresh" : None,
    }

    /// Former `TransportModule`.
    Transport(TransportIn) {
        Play {} = "/play" : Trigger,
        PlayButton {} = "/playbutton" : Trigger,
        Stop {} = "/stop" : None,
        Restart {} = "/restart" : Trigger,
        Record {} = "/record" : None,
        /// `/overdub/launcher` the clip launcher, otherwise the arranger.
        Overdub { launcher: bool } = "/overdub/{launcher}" : None,
        Repeat {} = "/repeat" : Toggle,
        PunchIn {} = "/punchIn" : Toggle,
        PunchOut {} = "/punchOut" : Toggle,
        Click {} = "/click" : Toggle,
        ClickVolume {} = "/click/volume" : Int,
        ClickTicks {} = "/click/ticks" : Toggle,
        ClickPreroll {} = "/click/preroll" : Trigger,
        Quantize {} = "/quantize" : None,
        TempoRaw {} = "/tempo/raw" : Number,
        TempoTap {} = "/tempo/tap" : Trigger,
        /// The value is the change (default 1).
        TempoChange { dir: Dir } = "/tempo/+|-" : Number,
        /// Bitwig: the play start position in beats.
        Time {} = "/time" : Number,
        /// `numerator/denominator`, the segment in any case.
        TimeSignature {} = "/time/signature|Signature|SIGNATURE" : Text,
        Position {} = "/position" : Number,
        PositionMove { step: PositionMove } = "/position/+|-|++|--|start" : None,
        Crossfade { op: FaderOp } = "/crossfade/{crossfade}" : Int,
        /// `/autowrite/launcher` the clip launcher, otherwise the arranger.
        Autowrite { launcher: bool } = "/autowrite/{launcher}" : None,
        AutomationWriteMode {} = "/automationWriteMode" : Text,
        Preroll {} = "/preroll" : Int,
        PostRecordingAction {} = "/launcher/postRecordingAction" : Text,
        PostRecordingTimeOffset {} = "/launcher/postRecordingTimeOffset" : Number,
        DefaultQuantization {} = "/launcher/defaultQuantization" : Text,
    }

    /// Former `GlobalModule`.
    Global(GlobalIn) {
        Undo {} = "/undo" : None,
        Redo {} = "/redo" : None,
    }

    /// Former `LayoutModule`.
    Layout(LayoutIn) {
        Layout {} = "/layout" : Text,
        Panel { panel: Panel } = "/panel/noteEditor|automationEditor|devices|mixer|fullscreen" : None,
        Arranger { toggle: ArrangerToggle } =
            "/arranger/cueMarkerVisibility|playbackFollow|trackRowHeight|clipLauncherSectionVisibility|timeLineVisibility|ioSectionVisibility|effectTracksVisibility" : None,
        Mixer { toggle: MixerToggle } =
            "/mixer/clipLauncherSectionVisibility|crossFadeSectionVisibility|deviceSectionVisibility|sendsSectionVisibility|ioSectionVisibility|meterSectionVisibility" : None,
    }

    /// Former `MarkerModule`.
    Marker(MarkerIn) {
        Launch { marker: usize } = "/marker/{n}/launch" : None,
        Bank { dir: Dir } = "/marker/bank/+|-" : None,
    }

    /// Former `ProjectModule`.
    Project(ProjectIn) {
        Switch { dir: Dir } = "/project/+|-" : None,
        Engine {} = "/project/engine" : Toggle,
        Save {} = "/project/save" : None,
    }

    /// The parameter banks of the project, the cursor track and the devices (former
    /// `parseParamValue`, `parsePageValue` of the project, track and device modules).
    Param(ParamIn) {
        Value { bank: ParamBank, param: usize, op: ParamOp } = "/{bank}/param/{n}/value|indicate|reset|touched" : Int,
        /// Project and cursor track: select the next or previous parameter.
        SelectItem { bank: ParamBank, dir: Dir } = "/project|track/param/+|-" : Trigger,
        /// Devices: scroll the parameter pages.
        ScrollPages { bank: ParamBank, dir: Dir } = "/{device}/param/+|-" : Trigger,
        PageBank { bank: ParamBank, dir: Dir } = "/{bank}/param/bank/page/{loose}" : None,
        SelectPage { bank: ParamBank, page: PageArg } = "/{bank}/page/{page}" : Int,
    }

    /// Former `TrackModule`.
    Track(TrackIn) {
        Name { track: TrackRef } = "/{track}/name" : Text,
        Activated { track: TrackRef } = "/{track}/activated" : Trigger,
        /// Not supported by the core (the value would be A, B or AB).
        CrossfadeMode { track: TrackRef } = "/{track}/crossfadeMode/A|B|AB" : None,
        RecordQuantization { track: TrackRef } = "/{track}/recordQuantization" : Text,
        Select { track: TrackRef } = "/{track}/select|selected" : Trigger,
        Duplicate { track: TrackRef } = "/{track}/duplicate" : None,
        Remove { track: TrackRef } = "/{track}/remove" : None,
        Volume { track: TrackRef, op: FaderOp } = "/{track}/volume/{fader}" : Int,
        Pan { track: TrackRef, op: FaderOp } = "/{track}/pan/{fader}" : Int,
        Flag { track: TrackRef, flag: TrackFlag } = "/{track}/mute|solo|recarm|monitor|autoMonitor" : Toggle,
        SendActivated { track: TrackRef, send: usize } = "/{track}/send/{n}/activated" : Trigger,
        SendVolume { track: TrackRef, send: usize, op: FaderOp } = "/{track}/send/{n}/volume/{fader}" : Int,
        Slot { track: TrackRef, slot: usize, cmd: SlotCmd } =
            "/{track}/clip/{n}/select|selected|launch|launchAlt|record|create|insertFile|duplicate|remove|color" : Int,
        StopClip { track: TrackRef } = "/{track}/clip/stop" : None,
        ReturnToArrangement { track: TrackRef } = "/{track}/clip/returntoarrangement" : None,
        Enter { track: TrackRef } = "/{track}/enter" : Trigger,
        Color { track: TrackRef } = "/{track}/color" : Color,
        /// Only the cursor track.
        Pinned { track: TrackRef } = "/{track}/pinned" : Toggle,
        Indicate { what: Indicate } = "/track/indicate/volume|pan|send/{n}" : Trigger,
        BankPage { dir: Dir } = "/track/bank/page/{loose}" : None,
        BankScroll { dir: Dir } = "/track/bank/+|-" : None,
        SelectAdjacent { dir: Dir } = "/track/+|-" : None,
        Add { kind: AddTrack } = "/track/add/audio|effect|instrument" : None,
        Stop { alt: bool } = "/track/stop|stopAlt" : None,
        Vu {} = "/track/vu" : Trigger,
        ToggleBank {} = "/track/toggleBank" : None,
        Parent {} = "/track/parent" : None,
    }

    /// Former `SceneModule`.
    Scene(SceneIn) {
        BankPage { dir: Dir } = "/scene/bank/+|-" : Trigger,
        Scroll { dir: Dir } = "/scene/+|-" : Trigger,
        Create {} = "/scene/create" : Trigger,
        Add {} = "/scene/add" : Trigger,
        Scene { scene: usize, cmd: SceneCmd } = "/scene/{n}/select|launch|launchAlt|duplicate|remove|name|color" : Trigger,
    }

    /// Former `DeviceModule`.
    Device(DeviceIn) {
        SiblingSelect { sibling: i32 } = "/device/sibling/{n}/select|selected" : Trigger,
        BankPage { dir: Dir } = "/device/bank/page/{loose}" : None,
        LastParam { op: ParamOp } = "/device/lastparam/value|indicate|reset|touched" : Int,
        SelectAdjacent { dir: Dir } = "/device/+|-" : Trigger,
        Pinned {} = "/device/pinned" : Toggle,
        Toggle { device: DeviceRef, toggle: DeviceToggle } = "/{device}/duplicate|remove|bypass|expand|parameters|window" : None,
        IndicateParams { device: DeviceRef } = "/{device}/indicate/param" : Trigger,
        /// Logged ("Unknown Device command").
        Unknown { command: String } = "/{device}/{unknown}" : None,
        Layer { device: DeviceRef, layer: LayerIndex, cmd: LayerCmd } = "/{device}/{layers}/{layer}/{layer_cmd}" : Int,
        LayerParent { device: DeviceRef } = "/{device}/{layers}/parent" : None,
        LayerSelectAdjacent { device: DeviceRef, dir: Dir } = "/{device}/{layers}/+|-" : None,
        /// Without a direction it is logged.
        LayerPage { device: DeviceRef, dir: Option<Dir> } = "/{device}/{layers}/page/{loose}" : None,
        EqType { band: usize } = "/eq/type/{n}" : Text,
        EqParam { band: usize, param: EqParam } = "/eq/gain|freq|q/{n}" : Int,
        EqAdd {} = "/eq/add" : Trigger,
    }

    /// Former `BrowserModule`.
    Browser(BrowserIn) {
        Preset {} = "/browser/preset" : None,
        /// An unknown direction does nothing.
        Tab { dir: Option<Dir> } = "/browser/tab/+|-|x" : None,
        Device { before: bool } = "/browser/device/{insert}" : None,
        /// Not supported by the core (logged).
        InsertClap {} = "/browser/insertClap" : Text,
        Stop { commit: bool } = "/browser/commit|cancel" : None,
        /// The column 0-based; an unknown operation does nothing.
        Filter { column: usize, op: Option<FilterOp> } = "/browser/filter/{n}/+|-|reset|x" : None,
        Result { next: bool } = "/browser/result/{result}" : None,
    }

    /// Former `MidiModule`.
    Midi(MidiIn) {
        Velocity {} = "/vkb_midi/velocity" : Int,
        NoteRepeat { param: NoteRepeatParam } = "/vkb_midi/noterepeat/isActive|period|length" : Text,
        /// The value is note and velocity.
        NoteArray { channel: i32 } = "/vkb_midi/{n}/note" : NoteVelocity,
        Note { channel: i32, note: i32 } = "/vkb_midi/{n}/note/{n}" : Int,
        NoteOctave { dir: Dir } = "/vkb_midi/{n}/note/+|-" : Trigger,
        Drum { channel: i32, note: i32 } = "/vkb_midi/{n}/drum/{n}" : Int,
        DrumOctave { dir: Dir } = "/vkb_midi/{n}/drum/+|-" : Trigger,
        /// Without a CC number it is logged.
        Cc { channel: i32, cc: Option<i32> } = "/vkb_midi/{n}/cc/{n}" : Int,
        Aftertouch { channel: i32, note: Option<usize> } = "/vkb_midi/{n}/aftertouch/{opt_n}" : Int,
        Pitchbend { channel: i32 } = "/vkb_midi/{n}/pitchbend" : Int,
    }

    /// Former `ActionModule`.
    Action(ActionIn) {
        /// The 0-based index of the action, clamped.
        Action { action: usize } = "/action/{n}" : None,
    }

    /// Former `ClipModule`.
    Clip(ClipIn) {
        StopAll { alt: bool } = "/clip/stopall|stopallAlt" : None,
        Pinned {} = "/clip/pinned" : Toggle,
        Quantize {} = "/clip/quantize" : None,
        Name {} = "/clip/name" : Text,
        Color {} = "/clip/color" : Color,
        Stop { alt: bool } = "/clip/stop|stopAlt" : None,
        SelectAdjacent { dir: Dir } = "/clip/+|-" : None,
        Launch { alt: bool } = "/clip/launch|launchAlt" : Int,
        Record {} = "/clip/record" : None,
        Create {} = "/clip/create" : Int,
        InsertFile {} = "/clip/insertFile" : Text,
    }
}

// ---------------------------------------------------------------------------------------------
// The parser

/// A condition which Java checks before it reads the rest of an address (see the module
/// documentation). The setup checks it with the model and the value.
#[derive(Debug, Clone, PartialEq)]
pub enum Guard {
    /// There is a cursor track.
    CursorTrackExists,
    /// The device exists and has drum pads.
    HasDrumPads(DeviceRef),
    /// The device exists.
    DeviceExists(DeviceRef),
    /// The layer index is inside the page of the layer bank (else it is logged), not negative
    /// (else a runtime error), and the device exists.
    LayerInRange(DeviceRef, LayerIndex),
    /// The value is a trigger.
    Trigger,
    /// The parameter bank exists.
    ParamBankExists(ParamBank),
    /// The focused parameter exists.
    FocusedParamExists,
    /// The browser is active.
    BrowserActive,
    /// The 1-based column is in 1..=6 and the browser is active.
    BrowserFilterColumn(i32),
    /// The cursor track has a selected slot.
    CursorSlotSelected,
    /// The value is an integer (else its error).
    IntValue,
}

/// A parsed address: the guards to check and the command or the error of the address.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub guards: Vec<Guard>,
    pub result: Result<OscIn, OscError>,
}

/// The parts of an address (former `LinkedList<String> path`).
struct Segs<'a> {
    parts: Vec<&'a str>,
    pos: usize,
}

impl<'a> Segs<'a> {
    /// Split the address without the leading empty part (former `parseAddress`, Java
    /// `String.split ("/")`, which drops the trailing empty parts).
    fn new(address: &'a str) -> Self {
        let mut parts: Vec<&str> = address.split('/').collect();
        while parts.last().is_some_and(|p| p.is_empty()) {
            parts.pop();
        }
        Segs { parts, pos: 1 }
    }

    fn next(&mut self) -> Option<&'a str> {
        let part = self.parts.get(self.pos).copied();
        if part.is_some() {
            self.pos += 1;
        }
        part
    }

    fn peek(&self) -> Option<&'a str> {
        self.parts.get(self.pos).copied()
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.parts.len()
    }

    /// Former `getSubCommand`.
    fn sub(&mut self) -> Result<&'a str, OscError> {
        self.next().ok_or(OscError::MissingCommand)
    }

    /// Former `path.get (0)`, which throws if the path is empty.
    fn first(&self) -> Result<&'a str, OscError> {
        self.peek().ok_or_else(|| OscError::Runtime("Index 0 out of bounds for length 0".into()))
    }

    /// Former `path.removeFirst ()`, which throws if the path is empty.
    fn remove_first(&mut self) -> Result<&'a str, OscError> {
        self.next().ok_or_else(|| OscError::Runtime("NoSuchElementException".into()))
    }
}

/// Former `Integer.parseInt`.
pub fn parse_int(text: &str) -> Option<i32> {
    text.parse::<i32>().ok()
}

/// Former `Integer.parseInt` where the `NumberFormatException` is not caught.
pub fn parse_int_or_throw(text: &str) -> Result<i32, OscError> {
    parse_int(text).ok_or_else(|| OscError::Runtime(format!("For input string: \"{text}\"")))
}

/// A 1-based index of the address as a 0-based index of a page, an error if it is negative
/// (Java throws an `IndexOutOfBoundsException` for it).
pub fn index_of(one_based: i32) -> Result<usize, OscError> {
    usize::try_from(i64::from(one_based) - 1).map_err(|_| OscError::Runtime(format!("Index {} out of bounds", i64::from(one_based) - 1)))
}

fn unknown(segment: &str) -> OscError {
    OscError::UnknownCommand(segment.to_string())
}

/// The guards of the address read so far.
struct Parser<'a> {
    segs: Segs<'a>,
    guards: Vec<Guard>,
}

type R<T> = Result<T, OscError>;

/// Parse an address (former `OSCParser.handle` and the `execute` methods of the modules). None
/// for an address without parts (`/`), which does nothing.
pub fn parse(address: &str) -> Option<Parsed> {
    let mut p = Parser { segs: Segs::new(address), guards: Vec::new() };
    let command = p.segs.next()?;
    let result = p.top(command);
    Some(Parsed { guards: p.guards, result })
}

impl<'a> Parser<'a> {
    fn guard(&mut self, guard: Guard) {
        self.guards.push(guard);
    }

    fn top(&mut self, command: &'a str) -> R<OscIn> {
        let Some(top) = Top::parse(command) else {
            return Err(unknown(command));
        };
        use TransportIn as T;
        Ok(match top {
            Top::Refresh => OscIn::Setup(SetupIn::Refresh {}),
            Top::Play => OscIn::Transport(T::Play {}),
            Top::PlayButton => OscIn::Transport(T::PlayButton {}),
            Top::Stop => OscIn::Transport(T::Stop {}),
            Top::Restart => OscIn::Transport(T::Restart {}),
            Top::Record => OscIn::Transport(T::Record {}),
            Top::Overdub => OscIn::Transport(T::Overdub { launcher: self.segs.peek() == Some("launcher") }),
            Top::Repeat => OscIn::Transport(T::Repeat {}),
            Top::PunchIn => OscIn::Transport(T::PunchIn {}),
            Top::PunchOut => OscIn::Transport(T::PunchOut {}),
            Top::Click => OscIn::Transport(self.click()?),
            Top::Quantize => OscIn::Transport(T::Quantize {}),
            Top::Tempo => OscIn::Transport(self.tempo()?),
            Top::Time => OscIn::Transport(self.time()?),
            Top::Position => OscIn::Transport(self.position()?),
            Top::Crossfade => OscIn::Transport(T::Crossfade { op: self.crossfade()? }),
            Top::Autowrite => OscIn::Transport(T::Autowrite { launcher: self.segs.peek() == Some("launcher") }),
            Top::AutomationWriteMode => OscIn::Transport(T::AutomationWriteMode {}),
            Top::Preroll => OscIn::Transport(T::Preroll {}),
            Top::Launcher => OscIn::Transport(self.launcher()?),
            Top::Undo => OscIn::Global(GlobalIn::Undo {}),
            Top::Redo => OscIn::Global(GlobalIn::Redo {}),
            Top::Layout => OscIn::Layout(LayoutIn::Layout {}),
            Top::Panel => {
                let sub = self.segs.sub()?;
                OscIn::Layout(LayoutIn::Panel { panel: Panel::parse(sub).ok_or_else(|| unknown(sub))? })
            }
            Top::Arranger => {
                let sub = self.segs.sub()?;
                OscIn::Layout(LayoutIn::Arranger { toggle: ArrangerToggle::parse(sub).ok_or_else(|| unknown(sub))? })
            }
            Top::Mixer => {
                let sub = self.segs.sub()?;
                OscIn::Layout(LayoutIn::Mixer { toggle: MixerToggle::parse(sub).ok_or_else(|| unknown(sub))? })
            }
            Top::Marker => OscIn::Marker(self.marker()?),
            Top::Project => self.project()?,
            Top::Track => {
                let sub = self.segs.sub()?;
                match parse_int(sub) {
                    Some(number) => OscIn::Track(self.track_value(TrackRef::Bank(index_of(number)?))?),
                    None => self.track_commands(sub)?,
                }
            }
            Top::Master => OscIn::Track(self.track_value(TrackRef::Master)?),
            Top::Scene => OscIn::Scene(self.scene()?),
            Top::Device => self.cursor_device()?,
            Top::Primary => self.device_value(DeviceRef::Primary)?,
            Top::Eq => self.eq()?,
            Top::Browser => OscIn::Browser(self.browser()?),
            Top::VkbMidi => OscIn::Midi(self.midi()?),
            Top::Action => {
                let sub = self.segs.sub()?;
                let number = parse_int(sub).ok_or_else(|| unknown(sub))?;
                let action = (i64::from(number) - 1).clamp(0, NUM_ASSIGNABLE_ACTIONS as i64 - 1) as usize;
                OscIn::Action(ActionIn::Action { action })
            }
            Top::Clip => OscIn::Clip(self.clip()?),
        })
    }

    // -- Transport

    fn click(&mut self) -> R<TransportIn> {
        if self.segs.is_empty() {
            return Ok(TransportIn::Click {});
        }
        let sub = self.segs.sub()?;
        Ok(match ClickKw::parse(sub).ok_or_else(|| unknown(sub))? {
            ClickKw::Volume => TransportIn::ClickVolume {},
            ClickKw::Ticks => TransportIn::ClickTicks {},
            ClickKw::Preroll => TransportIn::ClickPreroll {},
        })
    }

    fn tempo(&mut self) -> R<TransportIn> {
        let sub = self.segs.sub()?;
        Ok(match TempoKw::parse(sub).ok_or_else(|| unknown(sub))? {
            TempoKw::Raw => TransportIn::TempoRaw {},
            TempoKw::Tap => TransportIn::TempoTap {},
            TempoKw::Inc => TransportIn::TempoChange { dir: Dir::Next },
            TempoKw::Dec => TransportIn::TempoChange { dir: Dir::Previous },
        })
    }

    fn time(&mut self) -> R<TransportIn> {
        if self.segs.is_empty() {
            return Ok(TransportIn::Time {});
        }
        let sub = self.segs.sub()?;
        if !sub.eq_ignore_ascii_case("signature") {
            return Err(unknown(sub));
        }
        Ok(TransportIn::TimeSignature {})
    }

    fn position(&mut self) -> R<TransportIn> {
        if self.segs.is_empty() {
            return Ok(TransportIn::Position {});
        }
        let sub = self.segs.first()?;
        Ok(TransportIn::PositionMove { step: PositionMove::parse(sub).ok_or_else(|| unknown(sub))? })
    }

    fn crossfade(&mut self) -> R<FaderOp> {
        if self.segs.is_empty() {
            return Ok(FaderOp::Set);
        }
        Ok(if self.segs.first()? == "reset" { FaderOp::Reset } else { FaderOp::Ignore })
    }

    fn launcher(&mut self) -> R<TransportIn> {
        let sub = self.segs.first()?;
        Ok(match LauncherKw::parse(sub).ok_or_else(|| unknown(sub))? {
            LauncherKw::PostRecordingAction => TransportIn::PostRecordingAction {},
            LauncherKw::PostRecordingTimeOffset => TransportIn::PostRecordingTimeOffset {},
            LauncherKw::DefaultQuantization => TransportIn::DefaultQuantization {},
        })
    }

    // -- Markers, project, parameters

    fn marker(&mut self) -> R<MarkerIn> {
        let sub = self.segs.sub()?;
        if let Some(number) = parse_int(sub) {
            let marker = index_of(number)?;
            let sub2 = self.segs.sub()?;
            if sub2 != "launch" {
                return Err(unknown(sub2));
            }
            return Ok(MarkerIn::Launch { marker });
        }
        if sub != "bank" {
            return Err(unknown(sub));
        }
        let sub2 = self.segs.sub()?;
        Ok(MarkerIn::Bank { dir: Dir::parse(sub2).ok_or_else(|| unknown(sub2))? })
    }

    fn project(&mut self) -> R<OscIn> {
        let sub = self.segs.sub()?;
        Ok(match ProjectKw::parse(sub).ok_or_else(|| unknown(sub))? {
            ProjectKw::Next => OscIn::Project(ProjectIn::Switch { dir: Dir::Next }),
            ProjectKw::Previous => OscIn::Project(ProjectIn::Switch { dir: Dir::Previous }),
            ProjectKw::Engine => OscIn::Project(ProjectIn::Engine {}),
            ProjectKw::Save => OscIn::Project(ProjectIn::Save {}),
            ProjectKw::Param => OscIn::Param(self.param(ParamBank::Project)?),
            ProjectKw::Page => OscIn::Param(self.page(ParamBank::Project)?),
        })
    }

    /// The operation of a parameter (former `parseFXParamValue`).
    fn param_op(&mut self) -> R<ParamOp> {
        let sub = self.segs.sub()?;
        ParamOp::parse(sub).ok_or_else(|| unknown(sub))
    }

    /// `param/bank/page/<direction>`.
    fn param_page_bank(&mut self, bank: ParamBank) -> R<ParamIn> {
        let sub = self.segs.sub()?;
        if sub != "page" {
            return Err(unknown(sub));
        }
        let direction = self.segs.sub()?;
        Ok(ParamIn::PageBank { bank, dir: Dir::loose(direction) })
    }

    /// `param/{n}/<op>` (all banks).
    fn param_value(&mut self, bank: ParamBank, number: i32) -> R<ParamIn> {
        let param = index_of(number)?;
        self.guard(Guard::ParamBankExists(bank));
        Ok(ParamIn::Value { bank, param, op: self.param_op()? })
    }

    /// The parameters of the project and the cursor track (former `parseParamValue`).
    fn param(&mut self, bank: ParamBank) -> R<ParamIn> {
        let sub = self.segs.sub()?;
        if let Some(number) = parse_int(sub) {
            return self.param_value(bank, number);
        }
        match Dir::parse(sub) {
            Some(dir) => Ok(ParamIn::SelectItem { bank, dir }),
            None if sub == "bank" => self.param_page_bank(bank),
            None => Err(unknown(sub)),
        }
    }

    /// The pages (former `parsePageValue`).
    fn page(&mut self, bank: ParamBank) -> R<ParamIn> {
        let sub = self.segs.sub()?;
        let page = if sub == "select" || sub == "selected" {
            PageArg::FromValue
        } else {
            PageArg::Index(parse_int(sub).ok_or_else(|| unknown(sub))?.wrapping_sub(1))
        };
        Ok(ParamIn::SelectPage { bank, page })
    }

    // -- Tracks

    /// The commands of `/track/<command>` which do not address a track by its number (former
    /// `parseTrackCommands`).
    fn track_commands(&mut self, command: &'a str) -> R<OscIn> {
        use TrackIn as T;
        let kw = TrackCommandKw::parse(command).ok_or_else(|| unknown(command))?;
        Ok(OscIn::Track(match kw {
            TrackCommandKw::Indicate => {
                let sub = self.segs.sub()?;
                let what = match IndicateKw::parse(sub).ok_or_else(|| unknown(sub))? {
                    IndicateKw::Volume => Indicate::Volume,
                    IndicateKw::Pan => Indicate::Pan,
                    IndicateKw::Send => Indicate::Send(index_of(parse_int_or_throw(self.segs.first()?)?)?),
                };
                T::Indicate { what }
            }
            TrackCommandKw::Bank => {
                let sub = self.segs.sub()?;
                if sub == "page" {
                    T::BankPage { dir: Dir::loose(self.segs.sub()?) }
                } else {
                    T::BankScroll { dir: Dir::parse(sub).ok_or_else(|| unknown(sub))? }
                }
            }
            TrackCommandKw::Next => T::SelectAdjacent { dir: Dir::Next },
            TrackCommandKw::Previous => T::SelectAdjacent { dir: Dir::Previous },
            TrackCommandKw::Add => {
                let sub = self.segs.sub()?;
                T::Add { kind: AddTrack::parse(sub).ok_or_else(|| unknown(sub))? }
            }
            TrackCommandKw::Stop => T::Stop { alt: false },
            TrackCommandKw::StopAlt => T::Stop { alt: true },
            TrackCommandKw::Vu => T::Vu {},
            TrackCommandKw::ToggleBank => T::ToggleBank {},
            TrackCommandKw::Parent => T::Parent {},
            TrackCommandKw::Select | TrackCommandKw::Selected => {
                self.guard(Guard::CursorTrackExists);
                self.track_value(TrackRef::Cursor)?
            }
            TrackCommandKw::Param => return Ok(OscIn::Param(self.param(ParamBank::CursorTrack)?)),
            TrackCommandKw::Page => return Ok(OscIn::Param(self.page(ParamBank::CursorTrack)?)),
        }))
    }

    /// `volume` or `pan` with an optional sub command, which is not removed.
    fn fader(&self, with_reset: bool) -> R<FaderOp> {
        if self.segs.is_empty() {
            return Ok(FaderOp::Set);
        }
        Ok(match FaderKw::parse(self.segs.first()?) {
            Some(FaderKw::Indicate) => FaderOp::Indicate,
            Some(FaderKw::Reset) if with_reset => FaderOp::Reset,
            Some(FaderKw::Touched) => FaderOp::Touched,
            _ => FaderOp::Ignore,
        })
    }

    /// The commands of a track (former `parseTrackValue`).
    fn track_value(&mut self, track: TrackRef) -> R<TrackIn> {
        use TrackIn as T;
        let command = self.segs.sub()?;
        let kw = TrackValueKw::parse(command).ok_or_else(|| unknown(command))?;
        Ok(match kw {
            TrackValueKw::Name => T::Name { track },
            TrackValueKw::Activated => T::Activated { track },
            TrackValueKw::CrossfadeMode => {
                self.segs.sub()?;
                T::CrossfadeMode { track }
            }
            TrackValueKw::RecordQuantization => T::RecordQuantization { track },
            TrackValueKw::Select | TrackValueKw::Selected => T::Select { track },
            TrackValueKw::Duplicate => T::Duplicate { track },
            TrackValueKw::Remove => T::Remove { track },
            TrackValueKw::Volume => T::Volume { track, op: self.fader(true)? },
            TrackValueKw::Pan => T::Pan { track, op: self.fader(true)? },
            TrackValueKw::Mute => T::Flag { track, flag: TrackFlag::Mute },
            TrackValueKw::Solo => T::Flag { track, flag: TrackFlag::Solo },
            TrackValueKw::RecArm => T::Flag { track, flag: TrackFlag::RecArm },
            TrackValueKw::Monitor => T::Flag { track, flag: TrackFlag::Monitor },
            TrackValueKw::AutoMonitor => T::Flag { track, flag: TrackFlag::AutoMonitor },
            TrackValueKw::Send => {
                // Java: Integer.parseInt (path.removeFirst ()), both throw
                let send = index_of(parse_int_or_throw(self.segs.remove_first()?)?)?;
                // Former parseSendValue
                let sub = self.segs.sub()?;
                match SendKw::parse(sub).ok_or_else(|| unknown(sub))? {
                    SendKw::Activated => T::SendActivated { track, send },
                    SendKw::Volume => T::SendVolume { track, send, op: self.fader(true)? },
                }
            }
            TrackValueKw::Clip => self.slot(track)?,
            TrackValueKw::Enter => T::Enter { track },
            TrackValueKw::Color => T::Color { track },
            TrackValueKw::Pinned => T::Pinned { track },
        })
    }

    /// The commands of a clip slot of a track (former `parseClipValue`).
    fn slot(&mut self, track: TrackRef) -> R<TrackIn> {
        let command = self.segs.sub()?;
        let Some(number) = parse_int(command) else {
            return Ok(match SlotNavKw::parse(command).ok_or_else(|| unknown(command))? {
                SlotNavKw::Stop => TrackIn::StopClip { track },
                SlotNavKw::ReturnToArrangement => TrackIn::ReturnToArrangement { track },
            });
        };
        let slot = index_of(number)?;
        let sub = self.segs.sub()?;
        let cmd = match SlotKw::parse(sub).ok_or_else(|| unknown(sub))? {
            SlotKw::Select | SlotKw::Selected => SlotCmd::Select,
            SlotKw::Launch => SlotCmd::Launch { alt: false },
            SlotKw::LaunchAlt => SlotCmd::Launch { alt: true },
            SlotKw::Record => SlotCmd::Record,
            SlotKw::Create => SlotCmd::Create,
            SlotKw::InsertFile => SlotCmd::InsertFile,
            SlotKw::Duplicate => SlotCmd::Duplicate,
            SlotKw::Remove => SlotCmd::Remove,
            SlotKw::Color => SlotCmd::Color,
        };
        Ok(TrackIn::Slot { track, slot, cmd })
    }

    // -- Scenes

    fn scene(&mut self) -> R<SceneIn> {
        let command = self.segs.sub()?;
        if let Some(kw) = SceneKw::parse(command) {
            return Ok(match kw {
                SceneKw::Bank => {
                    let sub = self.segs.sub()?;
                    SceneIn::BankPage { dir: Dir::parse(sub).ok_or_else(|| unknown(sub))? }
                }
                SceneKw::Next => SceneIn::Scroll { dir: Dir::Next },
                SceneKw::Previous => SceneIn::Scroll { dir: Dir::Previous },
                SceneKw::Create => SceneIn::Create {},
                SceneKw::Add => SceneIn::Add {},
            });
        }
        // Java: Integer.parseInt, its NumberFormatException is not caught
        let scene = index_of(parse_int_or_throw(command)?)?;
        let sub = self.segs.sub()?;
        Ok(SceneIn::Scene { scene, cmd: SceneCmd::parse(sub).ok_or_else(|| unknown(sub))? })
    }

    // -- Devices

    /// The commands of the cursor device (former `parseCursorDeviceValue`).
    fn cursor_device(&mut self) -> R<OscIn> {
        let Some(kw) = self.segs.peek().and_then(CursorDeviceKw::parse) else {
            return self.device_value(DeviceRef::Cursor);
        };
        self.segs.next();
        Ok(OscIn::Device(match kw {
            CursorDeviceKw::Sibling => {
                let sibling = parse_int_or_throw(self.segs.sub()?)?;
                let sub = self.segs.sub()?;
                if sub != "select" && sub != "selected" {
                    return Err(unknown(sub));
                }
                DeviceIn::SiblingSelect { sibling }
            }
            CursorDeviceKw::Bank => {
                let sub = self.segs.sub()?;
                if sub != "page" {
                    return Err(unknown(sub));
                }
                DeviceIn::BankPage { dir: Dir::loose(self.segs.sub()?) }
            }
            CursorDeviceKw::LastParam => {
                self.guard(Guard::FocusedParamExists);
                DeviceIn::LastParam { op: self.param_op()? }
            }
            CursorDeviceKw::Next => DeviceIn::SelectAdjacent { dir: Dir::Next },
            CursorDeviceKw::Previous => DeviceIn::SelectAdjacent { dir: Dir::Previous },
            CursorDeviceKw::Pinned => DeviceIn::Pinned {},
        }))
    }

    /// The commands of a device (former `parseDeviceValue`).
    fn device_value(&mut self, device: DeviceRef) -> R<OscIn> {
        let command = self.segs.sub()?;
        let Some(kw) = DeviceKw::parse(command) else {
            return Ok(OscIn::Device(DeviceIn::Unknown { command: command.to_string() }));
        };
        let bank = ParamBank::Device(device);
        Ok(match kw {
            DeviceKw::Page => OscIn::Param(self.page(bank)?),
            DeviceKw::Duplicate => OscIn::Device(DeviceIn::Toggle { device, toggle: DeviceToggle::Duplicate }),
            DeviceKw::Remove => OscIn::Device(DeviceIn::Toggle { device, toggle: DeviceToggle::Remove }),
            DeviceKw::Bypass => OscIn::Device(DeviceIn::Toggle { device, toggle: DeviceToggle::Bypass }),
            DeviceKw::Expand => OscIn::Device(DeviceIn::Toggle { device, toggle: DeviceToggle::Expand }),
            DeviceKw::Parameters => OscIn::Device(DeviceIn::Toggle { device, toggle: DeviceToggle::Parameters }),
            DeviceKw::Window => OscIn::Device(DeviceIn::Toggle { device, toggle: DeviceToggle::Window }),
            DeviceKw::Indicate => {
                let sub = self.segs.sub()?;
                if sub != "param" {
                    return Err(unknown(sub));
                }
                OscIn::Device(DeviceIn::IndicateParams { device })
            }
            DeviceKw::Param => {
                let sub = self.segs.sub()?;
                if let Some(number) = parse_int(sub) {
                    return Ok(OscIn::Param(self.param_value(bank, number)?));
                }
                self.guard(Guard::Trigger);
                OscIn::Param(match Dir::parse(sub) {
                    Some(dir) => ParamIn::ScrollPages { bank, dir },
                    None if sub == "bank" => self.param_page_bank(bank)?,
                    None => return Err(unknown(sub)),
                })
            }
            DeviceKw::Drumpad => {
                self.guard(Guard::HasDrumPads(device));
                OscIn::Device(self.layer_or_drumpad(device)?)
            }
            DeviceKw::Layer => OscIn::Device(self.layer_or_drumpad(device)?),
        })
    }

    /// The commands of the equalizer (former `parseEqValue`); the others are device commands.
    fn eq(&mut self) -> R<OscIn> {
        let Some(kw) = self.segs.peek().and_then(EqKw::parse) else {
            return self.device_value(DeviceRef::Eq);
        };
        self.segs.next();
        let mut band = || -> R<usize> {
            let sub = self.segs.sub()?;
            index_of(parse_int(sub).ok_or_else(|| unknown(sub))?)
        };
        Ok(OscIn::Device(match kw {
            EqKw::Type => DeviceIn::EqType { band: band()? },
            EqKw::Gain => DeviceIn::EqParam { band: band()?, param: EqParam::Gain },
            EqKw::Freq => DeviceIn::EqParam { band: band()?, param: EqParam::Freq },
            EqKw::Q => DeviceIn::EqParam { band: band()?, param: EqParam::Q },
            EqKw::Add => DeviceIn::EqAdd {},
        }))
    }

    /// The layers or drum pads of a device (former `parseLayerOrDrumpad`; as in Java both
    /// address the layer bank).
    fn layer_or_drumpad(&mut self, device: DeviceRef) -> R<DeviceIn> {
        let command = self.segs.sub()?;
        let layer = if command == "selected" || command == "select" {
            Some(LayerIndex::Selected)
        } else {
            parse_int(command).map(|number| LayerIndex::Number(number.wrapping_sub(1)))
        };
        if let Some(layer) = layer {
            return self.layer(device, layer);
        }
        self.guard(Guard::DeviceExists(device));
        Ok(match LayerNavKw::parse(command).ok_or_else(|| unknown(command))? {
            LayerNavKw::Parent => DeviceIn::LayerParent { device },
            LayerNavKw::Next => DeviceIn::LayerSelectAdjacent { device, dir: Dir::Next },
            LayerNavKw::Previous => DeviceIn::LayerSelectAdjacent { device, dir: Dir::Previous },
            LayerNavKw::Page => DeviceIn::LayerPage { device, dir: self.segs.peek().map(Dir::loose) },
        })
    }

    /// The commands of a layer (former `parseDeviceLayerValue`).
    fn layer(&mut self, device: DeviceRef, layer: LayerIndex) -> R<DeviceIn> {
        let command = self.segs.sub()?;
        self.guard(Guard::LayerInRange(device, layer));
        let cmd = match LayerKw::parse(command).ok_or_else(|| unknown(command))? {
            LayerKw::Activated => LayerCmd::Activated,
            LayerKw::Select | LayerKw::Selected => LayerCmd::Select,
            LayerKw::Name => LayerCmd::Name,
            LayerKw::Volume => LayerCmd::Volume(self.fader(true)?),
            LayerKw::Pan => LayerCmd::Pan(self.fader(true)?),
            LayerKw::Mute => LayerCmd::Mute,
            LayerKw::Solo => LayerCmd::Solo,
            LayerKw::Send => {
                let send = index_of(parse_int_or_throw(self.segs.remove_first()?)?)?;
                let Some(sub) = self.segs.next() else {
                    return Ok(DeviceIn::Layer { device, layer, cmd: LayerCmd::SendNothing(send) });
                };
                match SendKw::parse(sub).ok_or_else(|| unknown(sub))? {
                    SendKw::Volume => LayerCmd::SendVolume(send, self.fader(false)?),
                    SendKw::Activated => LayerCmd::SendActivated(send),
                }
            }
            LayerKw::Enter => LayerCmd::Enter,
        };
        Ok(DeviceIn::Layer { device, layer, cmd })
    }

    // -- Browser, virtual keyboard, clip

    fn browser(&mut self) -> R<BrowserIn> {
        let sub = self.segs.sub()?;
        let Some(kw) = BrowserKw::parse(sub) else {
            return Err(unknown("browser"));
        };
        Ok(match kw {
            BrowserKw::Preset => BrowserIn::Preset {},
            BrowserKw::Tab => {
                self.guard(Guard::BrowserActive);
                BrowserIn::Tab { dir: Dir::parse(self.segs.sub()?) }
            }
            BrowserKw::Device => BrowserIn::Device { before: self.segs.next().is_some_and(|location| location != "after") },
            BrowserKw::InsertClap => BrowserIn::InsertClap {},
            BrowserKw::Commit => BrowserIn::Stop { commit: true },
            BrowserKw::Cancel => BrowserIn::Stop { commit: false },
            BrowserKw::Filter => {
                let column = parse_int_or_throw(self.segs.sub()?)?;
                self.guard(Guard::BrowserFilterColumn(column));
                let op = FilterOp::parse(self.segs.sub()?);
                BrowserIn::Filter { column: (column - 1) as usize, op }
            }
            BrowserKw::Result => {
                self.guard(Guard::BrowserActive);
                BrowserIn::Result { next: self.segs.next().is_none_or(|direction| direction == "+") }
            }
        })
    }

    fn midi(&mut self) -> R<MidiIn> {
        let command = self.segs.sub()?;
        if let Some(kw) = MidiKw::parse(command) {
            return Ok(match kw {
                MidiKw::Velocity => MidiIn::Velocity {},
                MidiKw::NoteRepeat => {
                    let sub = self.segs.sub()?;
                    MidiIn::NoteRepeat { param: NoteRepeatParam::parse(sub).ok_or_else(|| unknown(sub))? }
                }
            });
        }
        let number = parse_int(command).ok_or_else(|| unknown(command))?;
        let channel = (i64::from(number) - 1).clamp(0, 15) as i32;
        let sub = self.segs.sub()?;
        // An unknown command reports the channel part, as in Java
        Ok(match MidiChannelKw::parse(sub).ok_or_else(|| unknown(command))? {
            MidiChannelKw::Note => match self.segs.next() {
                None => MidiIn::NoteArray { channel },
                Some(n) => match Dir::parse(n) {
                    Some(dir) => MidiIn::NoteOctave { dir },
                    None => MidiIn::Note { channel, note: parse_int_or_throw(n)? },
                },
            },
            MidiChannelKw::Drum => {
                let n = self.segs.sub()?;
                match Dir::parse(n) {
                    Some(dir) => MidiIn::DrumOctave { dir },
                    None => MidiIn::Drum { channel, note: parse_int_or_throw(n)? },
                }
            }
            MidiChannelKw::Cc => MidiIn::Cc { channel, cc: self.segs.next().map(parse_int_or_throw).transpose()? },
            MidiChannelKw::Aftertouch => {
                // Java converts the value first
                self.guard(Guard::IntValue);
                let note = match self.segs.next() {
                    None => None,
                    Some(note) => Some(index_of(parse_int_or_throw(note)?.wrapping_add(1))?),
                };
                MidiIn::Aftertouch { channel, note }
            }
            MidiChannelKw::Pitchbend => MidiIn::Pitchbend { channel },
        })
    }

    fn clip(&mut self) -> R<ClipIn> {
        let sub = self.segs.sub()?;
        let kw = ClipKw::parse(sub);
        // Cursor clip related commands
        match kw {
            Some(ClipKw::StopAll) => return Ok(ClipIn::StopAll { alt: false }),
            Some(ClipKw::StopAllAlt) => return Ok(ClipIn::StopAll { alt: true }),
            Some(ClipKw::Pinned) => return Ok(ClipIn::Pinned {}),
            Some(ClipKw::Quantize) => return Ok(ClipIn::Quantize {}),
            // These continue with the slot commands when they are executed
            Some(ClipKw::Name) => return Ok(ClipIn::Name {}),
            Some(ClipKw::Color) => return Ok(ClipIn::Color {}),
            _ => {}
        }
        // Slot bank related commands
        self.guard(Guard::CursorTrackExists);
        match kw {
            Some(ClipKw::Stop) => return Ok(ClipIn::Stop { alt: false }),
            Some(ClipKw::StopAlt) => return Ok(ClipIn::Stop { alt: true }),
            Some(ClipKw::Next) => return Ok(ClipIn::SelectAdjacent { dir: Dir::Next }),
            Some(ClipKw::Previous) => return Ok(ClipIn::SelectAdjacent { dir: Dir::Previous }),
            _ => {}
        }
        self.guard(Guard::CursorSlotSelected);
        Ok(match kw {
            Some(ClipKw::Launch) => ClipIn::Launch { alt: false },
            Some(ClipKw::LaunchAlt) => ClipIn::Launch { alt: true },
            Some(ClipKw::Record) => ClipIn::Record {},
            Some(ClipKw::Create) => ClipIn::Create {},
            Some(ClipKw::InsertFile) => ClipIn::InsertFile {},
            // As in Java, the module name
            _ => return Err(unknown("clip")),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// The table of the outgoing values

osc_keys! {
    /// The values of a parameter (former `flushParameterData` of a parameter).
    pub struct ParamKeys {
        exists: bool = "exists",
        name: str = "name",
        value_str: str = "valueStr",
        value: i32 = "value",
        modulated_value: i32 = "modulatedValue",
    }
}

osc_keys! {
    /// The values of a send (former `flushParameterData` of a send).
    pub struct SendKeys {
        activated: bool = "activated",
        exists: bool = "exists",
        name: str = "name",
        volume_str: str = "volumeStr",
        volume: i32 = "volume",
        modulated_value: i32 = "modulatedValue",
    }
}

osc_keys! {
    /// A parameter page: `page/{n}/` itself is the name.
    pub struct PageKeys {
        exists: bool = "exists",
        page: str = "",
        name: str = "name",
        selected: bool = "selected",
    }
}

osc_keys! {
    /// A parameter bank and its pages (`<prefix>param/{n}/...`, `<prefix>page/...`).
    pub struct ParamBankKeys {
        params: [ParamKeys] = "param/",
        pages: [PageKeys] = "page/",
        selected_page_name: str = "page/selected/name",
    }
}

osc_keys! {
    /// A clip slot of a track.
    pub struct SlotKeys {
        name: str = "name",
        is_selected: bool = "isSelected",
        has_content: bool = "hasContent",
        is_playing: bool = "isPlaying",
        is_recording: bool = "isRecording",
        is_playing_queued: bool = "isPlayingQueued",
        is_recording_queued: bool = "isRecordingQueued",
        is_stop_queued: bool = "isStopQueued",
        color: ColorEx = "color",
    }
}

osc_keys! {
    /// A track (former `flushTrack`).
    pub struct TrackKeys {
        exists: bool = "exists",
        kind: str = "type",
        activated: bool = "activated",
        selected: bool = "selected",
        is_group: bool = "isGroup",
        name: str = "name",
        volume_str: str = "volumeStr",
        volume: i32 = "volume",
        pan_str: str = "panStr",
        pan: i32 = "pan",
        mute: bool = "mute",
        solo: bool = "solo",
        recarm: bool = "recarm",
        monitor: bool = "monitor",
        auto_monitor: bool = "autoMonitor",
        can_hold_notes: bool = "canHoldNotes",
        can_hold_audio_data: bool = "canHoldAudioData",
        position: i32 = "position",
        pinned: bool = "pinned",
        sends: [SendKeys] = "send/",
        clips: [SlotKeys] = "clip/",
        color: ColorEx = "color",
        crossfade_a: bool = "crossfadeMode/A",
        crossfade_b: bool = "crossfadeMode/B",
        crossfade_ab: bool = "crossfadeMode/AB",
        record_quantization: str = "recordQuantization",
        vu: i32 = "vu",
    }
}

osc_keys! {
    /// A layer or drum pad of a device (former `flushDeviceLayer`).
    pub struct LayerKeys {
        exists: bool = "exists",
        activated: bool = "activated",
        selected: bool = "selected",
        name: str = "name",
        volume_str: str = "volumeStr",
        volume: i32 = "volume",
        pan_str: str = "panStr",
        pan: i32 = "pan",
        mute: bool = "mute",
        solo: bool = "solo",
        sends: [SendKeys] = "send/",
        vu: i32 = "vu",
        color: ColorEx = "color",
    }
}

osc_keys! {
    /// A sibling of the cursor device.
    pub struct SiblingKeys {
        exists: bool = "exists",
        name: str = "name",
        bypass: bool = "bypass",
        selected: bool = "selected",
    }
}

osc_keys! {
    /// A band of the equalizer.
    pub struct EqBandKeys {
        kind: str = "value",
    }
}

osc_keys! {
    /// A device (former `flushDevice`).
    pub struct DeviceKeys {
        exists: bool = "exists",
        name: str = "name",
        bypass: bool = "bypass",
        expand: bool = "expand",
        parameters: bool = "parameters",
        window: bool = "window",
        band_types: [EqBandKeys] = "type/",
        gains: [ParamKeys] = "gain/",
        freqs: [ParamKeys] = "freq/",
        qs: [ParamKeys] = "q/",
        siblings: [SiblingKeys] = "sibling/",
        bank: ParamBankKeys = "",
    }
}

osc_keys! {
    /// A scene.
    pub struct SceneKeys {
        exists: bool = "exists",
        name: str = "name",
        selected: bool = "selected",
        color: ColorEx = "color",
    }
}

osc_keys! {
    /// A cue marker.
    pub struct MarkerKeys {
        exists: bool = "exists",
        name: str = "name",
        color: ColorEx = "color",
    }
}

osc_keys! {
    /// An item of a browser column or a result.
    pub struct BrowserItemKeys {
        exists: bool = "exists",
        name: str = "name",
        hits: i32 = "hits",
        is_selected: bool = "isSelected",
    }
}

osc_keys! {
    /// A filter column of the browser.
    pub struct BrowserColumnKeys {
        exists: bool = "exists",
        name: str = "name",
        wildcard: str = "wildcard",
        items: [BrowserItemKeys] = "item/",
    }
}

osc_keys! {
    /// A note of the virtual keyboard.
    pub struct NoteKeys {
        color: ColorEx = "color",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The samples of the placeholders of the templates.
    fn samples(placeholder: &str) -> &'static [&'static str] {
        match placeholder {
            "n" => &["1", "3"],
            "opt_n" => &["", "60"],
            "launcher" => &["", "launcher", "x"],
            "crossfade" => &["", "reset", "x"],
            "fader" => &["", "indicate", "reset", "touched", "x"],
            "track" => &["track/1", "track/2", "track/selected", "track/select", "master"],
            "bank" => &["project", "track", "device", "primary", "eq"],
            "device" => &["device", "primary", "eq"],
            "page" => &["1", "select", "selected"],
            "loose" => &["+", "-", "x"],
            "unknown" => &["x", "nonsense"],
            "layers" => &["layer", "drumpad"],
            "layer" => &["1", "2", "selected", "select"],
            "layer_cmd" => &[
                "activated", "select", "selected", "name", "volume", "volume/indicate", "volume/x", "pan", "pan/reset", "mute", "solo", "send/1/volume",
                "send/1/volume/touched", "send/1/activated", "send/1", "enter",
            ],
            "insert" => &["", "after", "before"],
            "result" => &["", "+", "-"],
            other => panic!("no samples for the placeholder {other}"),
        }
    }

    /// All addresses of a template.
    fn expand(template: &str) -> Vec<String> {
        let mut addresses = vec![String::new()];
        for segment in template.trim_start_matches('/').split('/') {
            let alternatives: Vec<&str> = match segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                Some(placeholder) => samples(placeholder).to_vec(),
                None => segment.split('|').collect(),
            };
            addresses = addresses
                .iter()
                .flat_map(|address| alternatives.iter().map(move |alt| if alt.is_empty() { address.clone() } else { format!("{address}/{alt}") }))
                .collect();
        }
        addresses
    }

    #[test]
    fn every_template_parses_to_its_entry() {
        let mut count = 0;
        for info in OscIn::table() {
            for address in expand(info.template) {
                let parsed = parse(&address).unwrap_or_else(|| panic!("{address}: nothing"));
                let command = parsed.result.unwrap_or_else(|e| panic!("{address} ({}): {e:?}", info.name));
                assert_eq!(command.name(), (info.module, info.name), "{address}");
                count += 1;
            }
        }
        assert!(count > 400, "{count}");
    }

    #[test]
    fn the_table_is_complete() {
        let names: Vec<(&str, &str)> = OscIn::table().map(|i| (i.module, i.name)).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "duplicate entries");
        // Every top level address has a module
        for top in Top::ALL {
            let parsed = parse(&format!("/{}", top.text())).unwrap();
            assert!(!matches!(parsed.result, Err(OscError::UnknownCommand(ref c)) if c == top.text()), "{top:?}");
        }
    }

    /// Every address which the former string matching implementation accepted (recorded in
    /// `golden/accepted.txt` before it was replaced) is a typed command.
    #[test]
    fn the_former_addresses_map() {
        let accepted = include_str!("golden/accepted.txt");
        let mut guarded = Vec::new();
        for address in accepted.lines() {
            match parse(address) {
                None => assert!(address.trim_matches('/').is_empty(), "{address}"),
                Some(parsed) => {
                    // An error after a guard only occurs if the guard holds (the former
                    // implementation accepted these with other values or another state)
                    if let Err(error) = parsed.result {
                        assert!(!parsed.guards.is_empty(), "{address}: {error:?}");
                        guarded.push(address);
                    }
                }
            }
        }
        assert_eq!(guarded, ["/device/param/bank/page", "/device/param/bank/x", "/device/param/bank", "/device/param/x", "/vkb_midi/1/aftertouch/x", "/device/param/garbage"]);
        assert!(accepted.lines().count() > 350);
    }

    #[test]
    fn errors_and_guards() {
        let p = |address: &str| parse(address).unwrap();
        assert_eq!(p("/x").result, Err(OscError::UnknownCommand("x".into())));
        assert_eq!(p("/track").result, Err(OscError::MissingCommand));
        assert_eq!(p("/track/1/send/x/volume").result, Err(OscError::Runtime("For input string: \"x\"".into())));
        assert_eq!(p("/track/0/volume").result, Err(OscError::Runtime("Index -1 out of bounds".into())));
        assert_eq!(p("/browser/x").result, Err(OscError::UnknownCommand("browser".into())));
        assert_eq!(p("/vkb_midi/1/x").result, Err(OscError::UnknownCommand("1".into())));
        let selected = p("/track/selected");
        assert_eq!((selected.guards, selected.result), (vec![Guard::CursorTrackExists], Err(OscError::MissingCommand)));
        let drum = p("/eq/drumpad/selected/x");
        assert_eq!(drum.guards, vec![Guard::HasDrumPads(DeviceRef::Eq), Guard::LayerInRange(DeviceRef::Eq, LayerIndex::Selected)]);
        assert_eq!(drum.result, Err(OscError::UnknownCommand("x".into())));
        assert_eq!(
            p("/track/2/volume/touched").result,
            Ok(OscIn::Track(TrackIn::Volume { track: TrackRef::Bank(1), op: FaderOp::Touched }))
        );
        assert_eq!(p("/device/x").result, Ok(OscIn::Device(DeviceIn::Unknown { command: "x".into() })));
        assert!(parse("/").is_none());
        assert!(parse("//").is_none());
        assert_eq!(p("//a").result, Err(OscError::UnknownCommand(String::new())));
    }
}
