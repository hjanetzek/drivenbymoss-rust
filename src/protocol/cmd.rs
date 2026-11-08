// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The command half of the protocol table: every command of the core to the host
//! (TYPED_PROTOCOL_PLAN §1.3, §5.4). The syntax of an entry is described in
//! [`super::macros`].
//!
//! IDs: one block of 0x100 per area, the high bit set (core → host). IDs are never renumbered or
//! reused.
//!
//! | Block | Area |
//! | --- | --- |
//! | 0x81 | transport, time, tempo, automation, undo, actions |
//! | 0x82 | application, arranger, mixer, project |
//! | 0x83 | channels (tracks, effect tracks, master, drum pads, layers), banks |
//! | 0x84 | slots and scenes |
//! | 0x85 | devices, siblings, drum pad banks, EQ |
//! | 0x86 | parameters and the other value commands |
//! | 0x87 | the cursor clip, the host's step grid, the Reaper note commands |
//! | 0x88 | browser |
//! | 0x89 | markers |
//! | 0x8A | navigator, refresh, update hints |
//! | 0x8B | note repeat, groove |

#![allow(clippy::enum_variant_names)]

use super::legacy::{Arg, ConstValue, Delay, Enable, Field, Legacy, One, PathReader, PathWriter, ProcField, Value};
use super::types::*;
use crate::backend::Processor;
use crate::daw::context::HostFeature;

/// The processor of a command in the metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcSpec {
    Fixed(Processor),
    /// Selected by the field with this name.
    Field(&'static str),
}

/// The metadata of a command (for diagnostics, documentation and the generators of step 1).
#[derive(Debug)]
pub struct CmdInfo {
    pub id: CmdId,
    pub name: &'static str,
    pub processor: ProcSpec,
    /// The command text with the fields in braces, e.g. `{ch}/send/{send}/volume`.
    pub template: &'static str,
    /// The fields (name, Rust type), including the value.
    pub fields: &'static [(&'static str, &'static str)],
    /// The field holding the value, `= Type` for a constant, None for no value.
    pub value: Option<&'static str>,
    /// The host feature the command needs.
    pub feature: Option<HostFeature>,
}

commands! {
    // --------------------------------------------------------------------------------------
    // 0x81 Transport, time, tempo, automation

    /// TransportImpl.play (Bitwig).
    TransportPlay = 0x8101 {} @ Transport ["play"], feature = TransportCommands;
    /// TransportImpl.restart (Bitwig).
    TransportRestart = 0x8102 {} @ Transport ["restart"], feature = TransportCommands;
    /// TransportImpl.stop (Bitwig).
    TransportStop = 0x8103 {} @ Transport ["stop"], feature = TransportCommands;
    /// Stop and move the play position to the start (Bitwig).
    TransportStopAndRewind = 0x8104 {} @ Transport ["stopAndRewind"], feature = TransportCommands;
    TransportToggleMetronomeTicks = 0x8105 {} @ Transport ["toggleMetronomeTicks"], feature = TransportCommands;
    TransportTogglePunchIn = 0x8106 {} @ Transport ["togglePunchIn"], feature = TransportCommands;
    TransportTogglePunchOut = 0x8107 {} @ Transport ["togglePunchOut"], feature = TransportCommands;
    TransportToggleFill = 0x8108 {} @ Transport ["toggleFill"], feature = TransportCommands;
    /// Toggle play (the basic form of [`Cmd::TransportPlay`]).
    PlayToggle = 0x8110 {} @ Play [];
    /// Stop (the basic form of [`Cmd::TransportStop`]).
    StopToggle = 0x8111 {} @ Stop [];
    /// Toggle record.
    RecordToggle = 0x8112 {} @ Record [];
    /// Toggle the loop (sends a constant 1).
    RepeatToggle = 0x8113 {} @ Repeat [] => (= One);
    TransportMetronome = 0x8120 { value: bool } @ Transport ["metronome"] => value;
    TransportToggleMetronome = 0x8121 {} @ Transport ["toggleMetronome"];
    TransportMetronomeTicks = 0x8122 { value: bool } @ Transport ["metronomeTicks"] => value;
    TransportTogglePrerollMetronome = 0x8123 {} @ Transport ["togglePrerollMetronome"];
    /// The metronome volume (Bitwig, normalized).
    TransportMetronomeVolume = 0x8124 { op: ValueOp } @ Transport ["metronomeVolume"] => op, feature = TransportCommands;
    /// The metronome volume (Reaper, 0..127).
    MetroVolSet = 0x8125 { value: i32 } @ MetroVol [] => value;
    /// Step the metronome volume (Reaper).
    MetroVolStep = 0x8126 { dir: Dir } @ MetroVol [dir];
    /// The pre-roll in measures (Reaper's ini file).
    IniFilePrerollMeasures = 0x8127 { value: i32 } @ IniFile ["reaper" "prerollmeas"] => value;
    TransportPunchIn = 0x8130 { value: bool } @ Transport ["punchIn"] => value;
    TransportPunchOut = 0x8131 { value: bool } @ Transport ["punchOut"] => value;
    TransportFill = 0x8132 { value: bool } @ Transport ["fill"] => value;
    TransportEnd = 0x8133 {} @ Transport ["end"];
    TransportTapTempo = 0x8134 {} @ Transport ["tapTempo"];
    TransportOverdubLauncher = 0x8135 { value: bool } @ Transport ["overdub" "launcher"] => value;
    TransportOverdubArranger = 0x8136 { value: bool } @ Transport ["overdub" "arranger"] => value;
    /// The crossfader of the host's transport (Bitwig).
    TransportCrossfade = 0x8137 { op: ValueOp } @ Transport ["crossfade"] => op;
    TempoStep = 0x8140 { step: TempoStep } @ Tempo [step];
    TempoSet = 0x8141 { value: f64 } @ Tempo [] => value;
    /// The play position in seconds (Reaper).
    TimeSet = 0x8142 { value: f64 } @ Time [] => value;
    /// The play position as an int (`time 0`: rewind).
    TimeSetInt = 0x8143 { value: i32 } @ Time [] => value;
    TimeSetNoSnap = 0x8144 { value: f64 } @ Time ["nosnap"] => value;
    /// The loop start in seconds (Reaper).
    TimeLoopStart = 0x8145 { value: f64 } @ Time ["loop" "start"] => value;
    /// The loop length in seconds (Reaper).
    TimeLoopLength = 0x8146 { value: f64 } @ Time ["loop" "length"] => value;
    TimeSignature = 0x8147 { value: Signature } @ Time ["signature"] => value;
    /// The play position in quarters (Bitwig).
    BeatsPosition = 0x8148 { value: f64 } @ Beats ["position"] => value;
    BeatsPositionNoSnap = 0x8149 { value: f64 } @ Beats ["position" "nosnap"] => value;
    /// The loop start in quarters (Bitwig).
    BeatsLoopStart = 0x814A { value: f64 } @ Beats ["loop" "start"] => value;
    /// The loop length in quarters (Bitwig).
    BeatsLoopLength = 0x814B { value: f64 } @ Beats ["loop" "length"] => value;
    /// The automation mode (0 trim read, 1 read, 2 touch, 3 write, 4 latch, 5 latch preview).
    AutomationMode = 0x8150 { value: i32 } @ Automation [] => value;
    AutomationToggleArranger = 0x8151 {} @ Automation ["toggleArranger"];
    AutomationToggleLauncher = 0x8152 {} @ Automation ["toggleLauncher"];
    AutomationResetOverrides = 0x8153 {} @ Automation ["resetOverrides"];
    Quantize = 0x8160 { value: f64 } @ Quantize [] => value;
    Undo = 0x8161 {} @ Undo [];
    Redo = 0x8162 {} @ Redo [];
    /// Execute an action of the host by its ID.
    Action = 0x8163 { id: &'a str } @ Action [] => id;

    // --------------------------------------------------------------------------------------
    // 0x82 Application, arranger, mixer, project

    ProjectEngine = 0x8201 { value: bool } @ Project ["engine"] => value;
    /// The index of the panel layout.
    ApplicationLayout = 0x8202 { value: i32 } @ Application ["layout"] => value;
    ApplicationLayoutStep = 0x8203 { dir: Dir } @ Application ["layout" dir], feature = LayoutStep;
    ApplicationToggleNoteEditor = 0x8204 {} @ Application ["toggleNoteEditor"];
    ApplicationToggleAutomationEditor = 0x8205 {} @ Application ["toggleAutomationEditor"];
    ApplicationToggleDevices = 0x8206 {} @ Application ["toggleDevices"];
    ApplicationToggleInspector = 0x8207 {} @ Application ["toggleInspector"];
    ApplicationToggleMixer = 0x8208 {} @ Application ["toggleMixer"];
    ApplicationToggleFullScreen = 0x8209 {} @ Application ["toggleFullScreen"];
    ApplicationToggleBrowser = 0x820A {} @ Application ["toggleBrowser"];
    ApplicationDuplicate = 0x820B {} @ Application ["duplicate"];
    ApplicationDeleteSelection = 0x820C {} @ Application ["deleteSelection"];
    ApplicationArrowLeft = 0x820D {} @ Application ["arrowLeft"];
    ApplicationArrowUp = 0x820E {} @ Application ["arrowUp"];
    ApplicationArrowRight = 0x820F {} @ Application ["arrowRight"];
    ApplicationArrowDown = 0x8210 {} @ Application ["arrowDown"];
    ApplicationZoomOut = 0x8211 {} @ Application ["zoomOut"];
    ApplicationZoomIn = 0x8212 {} @ Application ["zoomIn"];
    ApplicationDecTrackHeight = 0x8213 {} @ Application ["decTrackHeight"];
    ApplicationIncTrackHeight = 0x8214 {} @ Application ["incTrackHeight"];
    ApplicationEnter = 0x8215 {} @ Application ["enter"];
    ApplicationEscape = 0x8216 {} @ Application ["escape"];
    ApplicationSliceToSampler = 0x8217 {} @ Application ["sliceToSampler"];
    ApplicationSliceToDrumMachine = 0x8218 {} @ Application ["sliceToDrumMachine"];
    /// The global record quantization grid (Bitwig: `OFF`, `1/32`, …).
    ApplicationRecordQuantization = 0x8219 { value: &'a str } @ Application ["recordQuantization"] => value;
    ApplicationToggleRecordQuantizationNoteLength = 0x821A {} @ Application ["recordQuantization" "noteLength" "toggle"];
    ArrangerToggleCueMarkers = 0x8230 {} @ Arranger ["toggleCueMarkers"];
    ArrangerTogglePlaybackFollow = 0x8231 {} @ Arranger ["togglePlaybackFollow"];
    ArrangerToggleTrackRowHeight = 0x8232 {} @ Arranger ["toggleTrackRowHeight"];
    ArrangerToggleClipLauncher = 0x8233 {} @ Arranger ["toggleClipLauncher"];
    ArrangerToggleTimeLine = 0x8234 {} @ Arranger ["toggleTimeLine"];
    ArrangerToggleIoSection = 0x8235 {} @ Arranger ["toggleIoSection"];
    ArrangerToggleEffectTracks = 0x8236 {} @ Arranger ["toggleEffectTracks"];
    MixerToggleClipLauncherSection = 0x8240 {} @ Mixer ["toggleClipLauncherSection"];
    MixerToggleCrossFadeSection = 0x8241 {} @ Mixer ["toggleCrossFadeSection"];
    MixerToggleDeviceSection = 0x8242 {} @ Mixer ["toggleDeviceSection"];
    MixerToggleIoSection = 0x8243 {} @ Mixer ["toggleIoSection"];
    MixerToggleMeterSection = 0x8244 {} @ Mixer ["toggleMeterSection"];
    MixerToggleSendsSection = 0x8245 {} @ Mixer ["toggleSendsSection"];
    ProjectPrevious = 0x8250 {} @ Project ["previous"];
    ProjectNext = 0x8251 {} @ Project ["next"];
    ProjectCreateSceneFromPlayingLauncherClips = 0x8252 {} @ Project ["createSceneFromPlayingLauncherClips"];
    /// Create a scene; the value is the length of new clips in beats.
    ProjectCreateScene = 0x8253 { value: i32 } @ Project ["createScene"] => value;
    ProjectSave = 0x8254 {} @ Project ["save"];
    ProjectLoad = 0x8255 {} @ Project ["load"];
    ProjectUnsoloAll = 0x8256 {} @ Project ["unsoloAll"];
    ProjectUnmuteAll = 0x8257 {} @ Project ["unmuteAll"];
    /// The cue volume or mix (Bitwig).
    ProjectCue = 0x8258 { param: CueParam, op: ValueOp } @ Project [param] => op;

    // --------------------------------------------------------------------------------------
    // 0x83 Channels and banks

    ChannelColor = 0x8301 { ch: Chan, value: Rgb } @ [ch] [ch "color"] => value;
    ChannelMute = 0x8302 { ch: Chan, value: bool } @ [ch] [ch "mute"] => value;
    ChannelSolo = 0x8303 { ch: Chan, value: bool } @ [ch] [ch "solo"] => value;
    /// Toggle solo with the host's exclusive solo preference (Bitwig).
    ChannelSoloToggle = 0x8304 { ch: Chan } @ [ch] [ch "solo" "toggle"], feature = SoloToggle;
    ChannelActive = 0x8305 { ch: Chan, value: bool } @ [ch] [ch "active"] => value;
    ChannelRemove = 0x8306 { ch: Chan } @ [ch] [ch "remove"];
    ChannelDuplicate = 0x8307 { ch: Chan } @ [ch] [ch "duplicate"];
    /// Select (true) or deselect a track (sent as an int).
    ChannelSelect = 0x8308 { ch: Chan, value: bool } @ [ch] [ch "select"] => value;
    /// Select a drum pad or layer in the editor (no value).
    ChannelSelectInEditor = 0x8309 { ch: Chan } @ [ch] [ch "select"];
    ChannelVolume = 0x830A { ch: Chan, op: ValueOp } @ [ch] [ch "volume"] => op;
    ChannelPan = 0x830B { ch: Chan, op: ValueOp } @ [ch] [ch "pan"] => op;
    ChannelSendVolume = 0x830C { ch: Chan, send: i32, op: ValueOp } @ [ch] [ch "send" send "volume"] => op;
    /// Toggle a send (the value is the send volume, which the host ignores).
    ChannelSendActive = 0x830D { ch: Chan, send: i32, value: f64 } @ [ch] [ch "send" send "active"] => value;
    /// Toggle the track in the multi-selection (sends a constant 1).
    TrackToggleMultiSelect = 0x8320 { ch: Chan } @ [ch] [ch "toggleMultiSelect"] => (= One);
    TrackName = 0x8321 { ch: Chan, name: &'a str } @ [ch] [ch "name"] => name;
    TrackGroupExpanded = 0x8322 { ch: Chan, value: bool } @ [ch] [ch "isGroupExpanded"] => value;
    TrackRecArm = 0x8323 { ch: Chan, value: bool } @ [ch] [ch "recarm"] => value;
    TrackMonitor = 0x8324 { ch: Chan, value: i32 } @ [ch] [ch "monitor"] => value;
    TrackAutoMonitor = 0x8325 { ch: Chan, value: bool } @ [ch] [ch "autoMonitor"] => value;
    TrackStop = 0x8326 { ch: Chan } @ [ch] [ch "stop"];
    TrackStopAlt = 0x8327 { ch: Chan } @ [ch] [ch "stopAlt"];
    TrackReturnToArrangement = 0x8328 { ch: Chan } @ [ch] [ch "returnToArrangement"];
    /// The input quantization of the track in beats (Reaper).
    TrackInQuantResolution = 0x8329 { ch: Chan, value: f64 } @ [ch] [ch "inQuantResolution"] => value;
    /// Create a clip in the slot; the value is its length in beats.
    TrackCreateClip = 0x832A { ch: Chan, slot: i32, length: i32 } @ [ch] [ch "createClip" slot] => length;
    TrackRecordClip = 0x832B { ch: Chan } @ [ch] [ch "recordClip"];
    TrackMovePrev = 0x832C { ch: Chan } @ [ch] [ch "movePrev"];
    TrackMoveNext = 0x832D { ch: Chan } @ [ch] [ch "moveNext"];
    /// Pin the cursor track to the position (Reaper).
    TrackPin = 0x832E { ch: Chan, value: i32 } @ [ch] [ch "pin"] => value;
    /// The overdub of the track (Reaper).
    TrackOverdub = 0x832F { ch: Chan, value: bool } @ [ch] [ch "overdub"] => value;
    /// Scroll the track bank to the track (Reaper).
    TrackScrollTo = 0x8330 { ch: Chan } @ [ch] [ch "scrollto"];
    TrackPinnedToggle = 0x8340 {} @ Track ["pinned" "toggle"];
    TrackPinned = 0x8341 { value: bool } @ Track ["pinned"] => value;
    TrackSelectPrevious = 0x8342 {} @ Track ["selectPrevious"];
    TrackSelectNext = 0x8343 {} @ Track ["selectNext"];
    /// Add a track: type, name, color (`RGB(r,g,b)` or empty), device names.
    TrackAdd = 0x8344 { params: Vec<&'a str> } @ Track ["addTrack"] => params;
    /// The clip launcher feedback of a bank (`track`, `fxtrack`) or the indication of the pads
    /// of a drum pad bank (`drum`, `primary`).
    BankIndicate = 0x8350 { bank: Processor, value: bool } @ (bank) ["indicate"] => value;
    /// The offset of a bank which the host pages (`track`, `fxtrack`, `scene`).
    BankScroll = 0x8351 { bank: Processor, offset: i32 } @ Bank [bank] => offset;

    // --------------------------------------------------------------------------------------
    // 0x84 Slots and scenes

    SlotAction = 0x8401 { track: i32, slot: i32, action: SlotAction } @ Track [track "clip" slot action];
    SlotColor = 0x8402 { track: i32, slot: i32, value: Rgb } @ Track [track "clip" slot "color"] => value;
    /// Paste the source slot into the slot.
    SlotPaste = 0x8403 { track: i32, slot: i32, source: SlotRef } @ Track [track "clip" slot "paste"] => source;
    SlotInsertFile = 0x8404 { track: i32, slot: i32, path: &'a str } @ Track [track "clip" slot "insertFile"] => path;
    SceneAction = 0x8410 { scene: i32, action: SceneAction } @ Scene [scene action];
    SceneName = 0x8411 { scene: i32, name: &'a str } @ Scene [scene "name"] => name;
    SceneColor = 0x8412 { scene: i32, value: Rgb } @ Scene [scene "color"] => value;
    /// Stop all clips.
    SceneStopAll = 0x8413 {} @ Scene ["stop"];
    SceneStopAllAlt = 0x8414 {} @ Scene ["stopAlt"];

    // --------------------------------------------------------------------------------------
    // 0x85 Devices

    /// A command to a sibling of the cursor device (Bitwig).
    DeviceSibling = 0x8501 { sibling: Pos1, action: DeviceAction } @ Device ["sibling" sibling action], feature = DeviceCommands;
    /// A command to a device itself (Bitwig: `device`, `primary`, `drum`, `eq`).
    DeviceAction = 0x8502 { dev: Processor, action: DeviceAction } @ (dev) [action], feature = DeviceCommands;
    DeviceToggleWindow = 0x8503 { dev: Processor } @ (dev) ["window" "toggle"], feature = DeviceCommands;
    DeviceToggleExpanded = 0x8504 { dev: Processor } @ (dev) ["expand" "toggle"], feature = DeviceCommands;
    DeviceToggleParameterPageSection = 0x8505 { dev: Processor } @ (dev) ["parameters" "toggle"], feature = DeviceCommands;
    /// Set the bypass of the device at the index (Reaper).
    DeviceIndexBypass = 0x8510 { dev: Processor, device: Pos1, value: bool } @ (dev) [device "bypass"] => value;
    DeviceIndexRemove = 0x8511 { dev: Processor, device: Pos1 } @ (dev) [device "remove"];
    DeviceIndexDuplicate = 0x8512 { dev: Processor, device: Pos1 } @ (dev) [device "duplicate"];
    DeviceIndexMovePrev = 0x8513 { device: Pos1 } @ Device [device "movePrev"];
    DeviceIndexMoveNext = 0x8514 { device: Pos1 } @ Device [device "moveNext"];
    /// Select the device at the index (Reaper).
    DeviceSelected = 0x8515 { dev: Processor, device: Pos1 } @ (dev) ["selected"] => device;
    DeviceWindow = 0x8516 { value: bool } @ Device ["window"] => value;
    DeviceExpand = 0x8517 { value: bool } @ Device ["expand"] => value;
    DevicePageStep = 0x8518 { dir: Dir } @ Device ["page" dir];
    /// Select the previous or next device.
    DeviceStep = 0x8519 { dir: Dir } @ Device [dir];
    DevicePinned = 0x851A { dev: Processor, value: bool } @ (dev) ["pinned"] => value;
    /// Add the device with the creation name to the start of the chain.
    DeviceAdd = 0x851B { name: &'a str } @ Device ["add" "0"] => name;
    /// Load the preset with the index (Reaper's own browser).
    DevicePreset = 0x851C { value: i32 } @ Device ["preset"] => value;
    DeviceParameterPage = 0x851D { dev: Processor, page: i32 } @ (dev) ["parameterPage"] => page;
    DrumClearMute = 0x8520 { dev: Processor } @ (dev) ["clearMute"];
    DrumClearSolo = 0x8521 { dev: Processor } @ (dev) ["clearSolo"];
    EqAdd = 0x8530 {} @ Eq ["add"];
    /// The type of an EQ band (Reaper's index, sent as a string).
    EqBandType = 0x8531 { band: usize, value: IntText } @ Eq ["band" band] => value;

    // --------------------------------------------------------------------------------------
    // 0x86 Parameters and other value commands

    /// A parameter of a device, the project or the master (`<dev> param/<i>/value`).
    ParamValue = 0x8601 { dev: Processor, param: i32, op: ValueOp } @ (dev) ["param" param "value"] => op;
    /// A track FX parameter of the cursor track (`track <t>/param/<i>/value`).
    TrackParamValue = 0x8602 { track: i32, param: i32, op: ValueOp } @ Track [track "param" param "value"] => op;
    ParamTouch = 0x8603 { dev: Processor, param: i32, value: bool } @ (dev) ["param" param "touch"] => value;
    ParamIndicate = 0x8604 { dev: Processor, param: i32, value: bool } @ (dev) ["param" param "indicate"] => value;
    /// The crossfader on the master track (Reaper).
    MasterCrossfader = 0x8605 { op: ValueOp } @ Master ["user" "param" "0" "value"] => op;

    // --------------------------------------------------------------------------------------
    // 0x87 Cursor clip, step grid, Reaper notes

    ClipPinned = 0x8701 { value: bool } @ Clip ["pinned"] => value;
    ClipName = 0x8702 { name: &'a str } @ Clip ["name"] => name;
    ClipColor = 0x8703 { value: Rgb } @ Clip ["color"] => value;
    /// The clip start in beats (`/inc`: a change in beats).
    ClipStart = 0x8704 { op: ValueOp } @ Clip ["start"] => op;
    ClipEnd = 0x8705 { op: ValueOp } @ Clip ["end"] => op;
    ClipLoopStart = 0x8706 { op: ValueOp } @ Clip ["loopStart"] => op;
    ClipLoopLength = 0x8707 { op: ValueOp } @ Clip ["loopLength"] => op;
    ClipLoop = 0x8708 { value: bool } @ Clip ["loop"] => value;
    ClipShuffle = 0x8709 { value: bool } @ Clip ["shuffle"] => value;
    /// The accent of the clip (normalized).
    ClipAccent = 0x870A { op: ValueOp } @ Clip ["accent"] => op;
    ClipDuplicate = 0x870B {} @ Clip ["duplicate"];
    ClipDuplicateContent = 0x870C {} @ Clip ["duplicateContent"];
    ClipTranspose = 0x870D { value: i32 } @ Clip ["transpose"] => value;
    /// Clear all notes (Reaper).
    ClipClear = 0x870E {} @ Clip ["clear"];
    GridStepSize = 0x8720 { grid: GridId, value: f64 } @ Clip ["grid" grid "stepSize"] => value;
    /// Toggle the note at a step with the MIDI velocity.
    GridStepToggle = 0x8721 { grid: GridId, channel: i32, step: i32, note: i32, velocity: i32 } @ Clip ["grid" grid "step" channel step note "toggle"] => velocity;
    GridStepSet = 0x8722 { grid: GridId, channel: i32, step: i32, note: i32, value: GridNote } @ Clip ["grid" grid "step" channel step note "set"] => value;
    /// Create the note and copy the attributes of the step (the step text of
    /// `StepInfo::to_wire`).
    GridStepSetFrom = 0x8723 { grid: GridId, channel: i32, step: i32, note: i32, info: &'a str } @ Clip ["grid" grid "step" channel step note "setFrom"] => info;
    /// Update all attributes of the note (the step text of `StepInfo::to_wire`).
    GridStepUpdate = 0x8724 { grid: GridId, channel: i32, step: i32, note: i32, info: &'a str } @ Clip ["grid" grid "step" channel step note "update"] => info;
    GridStepClear = 0x8725 { grid: GridId, channel: i32, step: i32, note: i32 } @ Clip ["grid" grid "step" channel step note "clear"];
    GridStepMoveY = 0x8726 { grid: GridId, channel: i32, step: i32, note: i32, row: i32 } @ Clip ["grid" grid "step" channel step note "moveY"] => row;
    /// Change one attribute of the note.
    GridStepAttribute = 0x8727 { grid: GridId, channel: i32, step: i32, note: i32, attr: StepAttr, value: f64 } @ Clip ["grid" grid "step" channel step note attr] => value;
    GridClearAll = 0x8728 { grid: GridId } @ Clip ["grid" grid "clearAll"];
    GridClearRow = 0x8729 { grid: GridId, channel: i32, row: i32 } @ Clip ["grid" grid "clearRow" channel row];
    GridClearColumn = 0x872A { grid: GridId, channel: i32, column: i32 } @ Clip ["grid" grid "clearColumn" channel column];
    /// Scroll the grid to the step.
    GridScroll = 0x872B { grid: GridId, step: i32 } @ Clip ["grid" grid "scroll"] => step;
    GridPage = 0x872C { grid: GridId, dir: Dir } @ Clip ["grid" grid "page" dir];
    /// Reaper: toggle a note, `<position> <length> <velocity> <channel> 0`.
    ClipNoteToggle = 0x8740 { note: i32, value: &'a str } @ Clip ["note" note "toggle"] => value;
    /// Reaper: set a note, `<position> <length> <velocity> <channel> 0`.
    ClipNoteSet = 0x8741 { note: i32, value: &'a str } @ Clip ["note" note "set"] => value;
    /// Reaper: update a note, `<position> <length> <velocity> <channel> <muted>`.
    ClipNoteUpdate = 0x8742 { note: i32, value: &'a str } @ Clip ["note" note "update"] => value;
    /// Reaper: clear the note at the position (in beats).
    ClipNoteClear = 0x8743 { note: i32, channel: i32, position: f64 } @ Clip ["note" note "clear" channel] => position;
    /// Reaper: move the note at the position (in beats) to another row.
    ClipNoteMoveY = 0x8744 { note: i32, channel: i32, row: i32, position: f64 } @ Clip ["note" note "moveY" channel row] => position;
    /// Reaper: clear all notes of the row.
    ClipNoteClearRow = 0x8745 { note: i32, channel: i32 } @ Clip ["note" note "clear" channel];
    /// Reaper: clear all notes at the position (in beats).
    ClipNoteClearPosition = 0x8746 { channel: i32, position: f64 } @ Clip ["note" "0" "clearPosition" channel] => position;

    // --------------------------------------------------------------------------------------
    // 0x88 Browser

    BrowserContentType = 0x8801 { dir: Dir } @ Browser ["contentType" dir];
    BrowserPreview = 0x8802 { value: bool } @ Browser ["preview"] => value;
    BrowserColumnStep = 0x8803 { column: usize, dir: Dir } @ Browser ["column" column dir];
    BrowserColumnPage = 0x8804 { column: usize, dir: Dir } @ Browser ["column" column "page" dir];
    BrowserColumnReset = 0x8805 { column: usize } @ Browser ["column" column "reset"];
    BrowserReplaceDevice = 0x8806 {} @ Browser ["replace" "device"];
    BrowserReplaceSlot = 0x8807 { track: i32, slot: i32 } @ Browser ["replace" "slot" track slot];
    /// Browse for the device of a drum pad of the owner (`drum`, `primary`).
    BrowserReplacePad = 0x8808 { owner: Processor, pad: i32 } @ Browser ["replace" owner "pad" pad];
    BrowserAddToTrack = 0x8809 { track: i32 } @ Browser ["add" "track" track];
    BrowserAddToMaster = 0x880A {} @ Browser ["add" "master"];
    BrowserInsertBefore = 0x880B {} @ Browser ["insertBefore"];
    BrowserInsertAfter = 0x880C {} @ Browser ["insertAfter"];
    /// Stop browsing; true commits the selection.
    BrowserStop = 0x880D { commit: bool } @ Browser ["stop"] => commit;
    BrowserResultStep = 0x880E { dir: Dir } @ Browser ["result" dir];
    BrowserResultPage = 0x880F { dir: Dir } @ Browser ["result" "page" dir];

    // --------------------------------------------------------------------------------------
    // 0x89 Markers

    MarkerAction = 0x8901 { marker: i32, action: MarkerAction } @ Marker [marker action];
    MarkerAdd = 0x8902 {} @ Marker ["add"];
    MarkerPrevious = 0x8903 {} @ Marker ["previous"];
    MarkerNext = 0x8904 {} @ Marker ["next"];

    // --------------------------------------------------------------------------------------
    // 0x8A Navigator, refresh, update hints

    NavigatorStep = 0x8A01 { target: NavigatorTarget, dir: Dir } @ Navigator [target dir], feature = ClipLauncherNavigator;
    NavigatorSelectTrack = 0x8A02 { position: i32 } @ Navigator ["selectTrack"] => position, feature = ClipLauncherNavigator;
    /// The host sends all data again.
    Refresh = 0x8A03 {} @ Refresh [];
    /// The host delays the updates of the area (e.g. while automation is written).
    DelayUpdates = 0x8A04 { area: Processor } @ (area) [] => (= Delay);
    /// The host enables or disables the updates of the area.
    EnableUpdates = 0x8A05 { area: Processor, enable: Enable } @ (area) [] => enable;

    // --------------------------------------------------------------------------------------
    // 0x8B Note repeat, groove

    NoteRepeatActive = 0x8B01 { value: bool } @ NoteRepeat ["active"] => value;
    /// The rate (1 / period in beats).
    NoteRepeatRate = 0x8B02 { value: f64 } @ NoteRepeat ["rate"] => value;
    NoteRepeatNoteLength = 0x8B03 { value: f64 } @ NoteRepeat ["notelength"] => value;
    /// Use the velocity (true) or the pressure.
    NoteRepeatVelocity = 0x8B04 { value: bool } @ NoteRepeat ["velocity"] => value;
    NoteRepeatMode = 0x8B05 { value: i32 } @ NoteRepeat ["mode"] => value;
    /// The groove on/off (Reaper).
    GrooveActive = 0x8B10 { op: ValueOp } @ Groove ["active"] => op;
    /// The groove amount (Reaper).
    GrooveAmount = 0x8B11 { op: ValueOp } @ Groove ["amount"] => op;
    /// A parameter of the groove of the host (Bitwig).
    GrooveParam = 0x8B12 { param: GrooveParam, op: ValueOp } @ Groove [param] => op;
}

impl Cmd<'_> {
    /// Helper: enable or disable the updates of an area.
    pub fn enable_updates(area: Processor, enable: bool) -> Cmd<'static> {
        Cmd::EnableUpdates { area, enable: Enable(enable) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The number of samples per ID: enough to use every sample of every field type once.
    const SAMPLES: usize = 8;

    #[test]
    fn every_command_has_a_legacy_form_which_parses_back() {
        for &id in CmdId::ALL {
            for k in 0..SAMPLES {
                let cmd = id.sample(k);
                assert_eq!(cmd.id(), id);
                let legacy = cmd.legacy();
                let back = Cmd::from_legacy(legacy.processor, &legacy.command, &legacy.arg);
                assert_eq!(back.as_ref(), Some(&cmd), "{id:?} sample {k}: {legacy:?}");
            }
        }
    }

    #[test]
    fn the_table_has_unique_names_and_ids_with_the_direction_bit() {
        let mut names: Vec<_> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len());
        assert_eq!(COMMANDS.len(), CmdId::ALL.len());
        for info in COMMANDS {
            assert!(info.id as u16 & 0x8000 != 0, "{}", info.name);
            assert_eq!(info.id.info().name, info.name);
        }
    }

    #[test]
    fn domains_match_the_names_of_the_model() {
        let names: Vec<_> = GrooveParam::ALL.iter().map(|p| p.name()).collect();
        assert_eq!(names, crate::daw::groove::HOST_GROOVE_PARAMETERS);
    }

    #[test]
    fn legacy_texts() {
        let ch = Chan::Track { bank: Processor::FxTrack, p: 2 };
        let c = Cmd::ChannelSendVolume { ch, send: 1, op: ValueOp::Indicate(true) };
        let l = c.legacy();
        assert_eq!((l.processor, l.command.as_str(), l.arg), (Processor::FxTrack, "2/send/1/volume/indicate", Arg::Int(1)));
        let c = Cmd::ChannelVolume { ch: Chan::Master, op: ValueOp::Reset };
        let l = c.legacy();
        assert_eq!((l.processor, l.command.as_str(), l.arg), (Processor::Master, "volume/reset", Arg::None));
        let c = Cmd::ChannelColor { ch: Chan::Pad { owner: Processor::Drum, i: 36 }, value: Rgb([1, 2, 3]) };
        let l = c.legacy();
        assert_eq!((l.processor, l.command.as_str()), (Processor::Drum, "pad/36/color"));
        assert_eq!(l.arg, Arg::Str("RGB(1,2,3)".into()));
        let c = Cmd::GridStepAttribute { grid: GridId { steps: 8, rows: 128 }, channel: 0, step: 3, note: 60, attr: StepAttr::Velocity, value: 0.5 };
        let l = c.legacy();
        assert_eq!(l.command, "grid/8x128/step/0/3/60/velocity");
        let c = Cmd::DeviceSibling { sibling: Pos1(0), action: DeviceAction::Bypass };
        let l = c.legacy();
        assert_eq!(l.command, "sibling/1/bypass");
        let c = Cmd::Undo {};
        let l = c.legacy();
        assert_eq!((l.processor, l.command.as_str(), l.arg), (Processor::Undo, "", Arg::None));
        assert_eq!(CmdId::ChannelSendVolume.info().template, "{ch}/send/{send}/volume");
    }
}
