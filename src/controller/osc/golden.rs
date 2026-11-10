// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Golden tests of the OSC controller: the complete behaviour of the controller for a fixed list
//! of commands (the log lines, the commands to the host and the MIDI events of every command)
//! and the OSC output of a full update and of a sequence of model changes (every message with
//! its type tags, and a hash of the encoded bundles as they are sent). The golden files in
//! `golden/` were recorded from the string-matching implementation before the typed address
//! table replaced it; the tests keep the behaviour byte-identical.
//!
//! Record them again (only after a deliberate change of the behaviour):
//! `DBM_OSC_GOLDEN_RECORD=1 cargo test --lib controller::osc::golden`
//!
//! The flush cost per tick (release build, 3 tracks and 1 device):
//! `cargo test --release --lib controller::osc::golden::flush_cost -- --ignored --nocapture`

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::setup::{create, OscSetup};
use crate::backend::{Backend, Sender};
use crate::controller::{ControllerSetup, MidiPorts};
use crate::framework::ctx::Ctx;
use crate::framework::osc::udp::MESSAGES_PER_BUNDLE;
use crate::framework::osc::{OscArg, OscMessage, OscPacket};
use crate::host::{Host, LogSink};
use crate::parser::Parser;
use crate::protocol::Cmd;

/// Records everything the controller does: log lines, host commands, MIDI.
#[derive(Default)]
struct Recorder {
    lines: RefCell<Vec<String>>,
}

impl Recorder {
    fn push(&self, line: String) {
        self.lines.borrow_mut().push(line);
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.lines.borrow_mut())
    }
}

struct RecLog(Rc<Recorder>);

impl LogSink for RecLog {
    fn log(&self, message: &str) {
        self.0.push(format!("log {message}"));
    }
}

struct RecBackend(Rc<Recorder>);

impl Backend for RecBackend {
    fn send_command(&self, cmd: &Cmd) {
        self.0.push(format!("cmd {cmd:?}"));
    }

    fn process_midi_arg(&self, device_id: i32, status: i32, data1: i32, data2: i32) {
        self.0.push(format!("midi {device_id} {status:#04x} {data1} {data2}"));
    }

    fn midi_inputs(&self) -> Vec<(i32, String)> {
        Vec::new()
    }

    fn midi_outputs(&self) -> Vec<(i32, String)> {
        Vec::new()
    }

    fn open_midi_input(&self, _device_id: i32) -> bool {
        true
    }

    fn open_midi_output(&self, _device_id: i32) -> bool {
        true
    }

    fn close_midi_input(&self, _device_id: i32) {}

    fn close_midi_output(&self, _device_id: i32) {}

    fn send_midi_data(&self, device_id: i32, data: &[u8]) {
        self.0.push(format!("sysex {device_id} {data:?}"));
    }

    fn set_note_input_filters(&self, _device_id: i32, _note_input_index: i32, _filters: &[String]) {}

    fn set_note_input_key_translation_table(&self, _device_id: i32, _note_input_index: i32, _table: &[i32]) {}

    fn set_note_input_velocity_translation_table(&self, _device_id: i32, _note_input_index: i32, _table: &[i32]) {}
}

struct G {
    ctx: Ctx,
    setup: OscSetup,
    rec: Rc<Recorder>,
    parser: Parser,
}

impl G {
    fn new() -> G {
        let rec = Rc::new(Recorder::default());
        let host = Host::new(Rc::new(RecLog(rec.clone())));
        let config = crate::config::osc::for_reaper();
        let parts = create(host, Sender::new(Rc::new(RecBackend(rec.clone()))), MidiPorts { input: 0, output: 0 }, config).unwrap();
        let mut ctx = parts.ctx;
        let mut setup = OscSetup::new(crate::config::osc::bank_page_size(&ctx.config));
        setup.init_model(&mut ctx);
        ctx.config.take_changed_settings();
        setup.handle_changes(&mut ctx);
        rec.take();
        G { ctx, setup, rec, parser: Parser::new() }
    }

    fn model(&mut self, lines: &str) {
        for line in lines.lines() {
            self.parser.parse_line(&mut self.ctx.model, line);
        }
        self.setup.handle_changes(&mut self.ctx);
        self.ctx.run_due_tasks(self.ctx.host.now() + Duration::from_secs(1));
    }

    /// Execute a message, return what happened (log lines, commands, MIDI, notifications).
    fn send(&mut self, address: &str, args: Vec<OscArg>) -> Vec<String> {
        self.setup.handle_message(&mut self.ctx, &OscMessage::new(address, args));
        self.ctx.run_due_tasks(self.ctx.host.now() + Duration::from_secs(1));
        self.setup.handle_changes(&mut self.ctx);
        let mut lines = self.rec.take();
        lines.extend(self.ctx.host.take_notifications().into_iter().map(|n| format!("notify {n}")));
        lines
    }
}

/// A message with the type tags of its values: `/track/1/name s:Drums`.
fn message_text(message: &OscMessage) -> String {
    let mut text = message.address.clone();
    for arg in &message.args {
        let _ = write!(text, " {}:{}", arg.type_tag(), arg.java_string());
    }
    text
}

/// FNV-1a, 64 bit.
fn fnv(data: &[u8], mut hash: u64) -> u64 {
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The messages as they are sent: in bundles of at most 101 messages (`OscClient::send_bundle`).
fn output_text(out: &mut String, title: &str, messages: &[OscMessage]) {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut datagrams = 0;
    for chunk in messages.chunks(MESSAGES_PER_BUNDLE) {
        hash = fnv(&OscPacket::bundle(chunk.to_vec()).encode(), hash);
        datagrams += 1;
    }
    let _ = writeln!(out, "## {title}: {} messages, {datagrams} bundles, fnv64 {hash:016x}", messages.len());
    for message in messages {
        out.push_str(&message_text(message));
        out.push('\n');
    }
}

pub(super) const TRACKS: &str = "track/count 3\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/canHoldNotes 1\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/2/exists 1\ntrack/2/name Keys\ntrack/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\ntrack/0/slot/count 8\ntrack/0/slot/2/exists 1\n\
track/0/slot/2/hasContent 1\n";

pub(super) const DEVICE: &str = "device/exists 1\ndevice/name Polysynth\ndevice/param/count 16\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\n\
device/param/1/name Res\ndevice/sibling/1/name Polysynth\ndevice/count 2\n";

/// Everything else the modules report: scenes, markers, project and track parameters, the
/// first instrument, the equalizer, layers, drum pads, the browser, the cursor clip, a selected
/// slot and effect tracks.
const MORE: &str = "scene/count 2\nscene/0/exists 1\nscene/0/name Verse\nscene/1/exists 1\nscene/1/name Chorus\nmarker/count 2\nmarker/0/exists 1\n\
marker/0/name Intro\nmarker/1/exists 1\nmarker/1/name Outro\nproject/name Song\nproject/param/count 8\nproject/param/0/name Macro\nproject/param/0/value 0.25\n\
project/page/count 2\nproject/page/0/name Main\nproject/page/1/name Other\nproject/page/selected 0\ntrack/fx/param/count 8\ntrack/fx/param/0/name Width\n\
track/fx/page/count 1\ntrack/fx/page/0/name Remote\nprimary/exists 1\nprimary/name Synth\nprimary/param/count 8\nprimary/param/0/name Osc\neq/exists 1\n\
eq/name EQ+\neq/band/0 3\ndevice/hasDrumPads 1\ndevice/hasLayers 1\ndevice/layer/count 2\ndevice/layer/0/exists 1\ndevice/layer/0/name Kick\n\
device/layer/0/select 1\ndevice/layer/1/exists 1\ndevice/layer/1/name Snare\ndevice/pad/0/exists 1\ndevice/pad/0/name Kick\ndevice/touchedParam 0\n\
browser/exists 1\nbrowser/column/count 2\nbrowser/column/0/exists 1\nbrowser/column/0/name Category\nbrowser/column/0/item/0/exists 1\n\
browser/column/0/item/0/name Bass\nbrowser/column/0/item/0/hits 3\nbrowser/column/1/exists 1\nbrowser/column/1/name Creator\nbrowser/result/0/exists 1\n\
browser/result/0/name Preset 1\nbrowser/result/1/exists 1\nbrowser/result/1/name Preset 2\nclip/exists 1\nclip/color 0.5 0.25 1\ntrack/0/slot/2/selected 1\n\
fxtrack/count 2\nfxtrack/0/exists 1\nfxtrack/0/name Reverb\nfxtrack/1/exists 1\nfxtrack/1/name Delay\n";

/// The values each command is sent with.
fn values() -> Vec<Vec<OscArg>> {
    vec![
        vec![],
        vec![OscArg::Int(1)],
        vec![OscArg::Int(0)],
        vec![OscArg::Float(0.5)],
        vec![OscArg::Str("x".into())],
        vec![OscArg::Int(60), OscArg::Int(100)],
        vec![OscArg::Bool(true)],
        vec![OscArg::Nil],
        vec![OscArg::Int(-5)],
    ]
}

/// Commands with special values.
fn special_values() -> Vec<(&'static str, Vec<OscArg>)> {
    let s = |v: &str| vec![OscArg::Str(v.into())];
    vec![
        ("/automationWriteMode", s("latch")),
        ("/automationWriteMode", s("touch")),
        ("/time/signature", s("3/4")),
        ("/time/signature", s("7/8/")),
        ("/time/signature", s("3 / 4")),
        ("/time/SIGNATURE", s("6/8")),
        ("/launcher/postRecordingAction", s("play_recorded")),
        ("/launcher/defaultQuantization", s("1/4")),
        ("/launcher/postRecordingTimeOffset", vec![OscArg::Double(5000.0)]),
        ("/layout", s("arrange")),
        ("/layout", s("Mix")),
        ("/track/1/color", s("rgb(255,0,0)")),
        ("/track/1/color", s("RGB(1,2.5,3)")),
        ("/track/1/clip/1/color", s("rgb(0,255,0)")),
        ("/track/1/recordQuantization", s("1/16")),
        ("/track/1/name", s("Beats")),
        ("/track/1/clip/1/insertFile", s("/tmp/a.wav")),
        ("/track/1/clip/1/create", vec![OscArg::Int(4)]),
        ("/scene/1/color", s("rgb(1,2,3)")),
        ("/scene/1/name", s("Bridge")),
        ("/clip/color", s("rgb(10,20,30)")),
        ("/clip/name", s("Clip A")),
        ("/clip/insertFile", s("/tmp/b.wav")),
        ("/eq/type/1", s("bell")),
        ("/eq/type/2", s("LOWCUT")),
        ("/eq/type/1", s("nonsense")),
        ("/eq/gain/1", vec![OscArg::Int(100)]),
        ("/vkb_midi/noterepeat/period", s("1/32")),
        ("/vkb_midi/noterepeat/length", s("1/8t")),
        ("/vkb_midi/noterepeat/period", s("1/3")),
        ("/vkb_midi/velocity", vec![OscArg::Int(64)]),
        ("/vkb_midi/1/note/62", vec![OscArg::Int(127)]),
        ("/vkb_midi/velocity", vec![OscArg::Int(0)]),
        ("/vkb_midi/1/note", vec![OscArg::Int(64), OscArg::Float(90.0)]),
        ("/vkb_midi/1/note", vec![OscArg::Int(64)]),
        ("/vkb_midi/1/note", vec![OscArg::Int(64), OscArg::Str("x".into())]),
        ("/vkb_midi/1/aftertouch/x", s("x")),
        ("/vkb_midi/1/aftertouch/x", vec![OscArg::Int(5)]),
        ("/vkb_midi/1/aftertouch/-1", vec![OscArg::Int(5)]),
        ("/vkb_midi/17/cc/7", vec![OscArg::Int(5)]),
        ("/vkb_midi/0/cc/7", vec![OscArg::Int(5)]),
        ("/vkb_midi/1/cc/x", vec![OscArg::Int(5)]),
        ("/vkb_midi/1/drum/-1", vec![OscArg::Int(5)]),
        ("/vkb_midi/1/note/-1", vec![OscArg::Int(5)]),
        ("/browser/insertClap", s("com.u-he.Diva")),
        ("/tempo/+", vec![OscArg::Double(2.5)]),
        ("/tempo/raw", vec![OscArg::Long(140)]),
        ("/device/layer/9/volume", s("abc")),
        ("/device/layer/1/activated", s("abc")),
        ("/device/param/garbage", vec![OscArg::Int(0)]),
        ("/device/param/garbage", vec![OscArg::Int(1)]),
        ("/device/sibling/0/select", vec![OscArg::Int(0)]),
        ("/device/sibling/0/select", vec![OscArg::Int(1)]),
        ("/device/sibling/2/selected", vec![OscArg::Int(1)]),
        ("/device/page/selected", vec![OscArg::Int(2)]),
        ("/device/page/select", vec![OscArg::Int(0)]),
        ("/project/page/selected", vec![OscArg::Int(2)]),
        ("/track/page/select", vec![OscArg::Int(1)]),
        ("/action/3", vec![]),
        ("/action/-4", vec![]),
        ("/action/+2", vec![]),
    ]
}

/// The commands of the old `every_command_once` test and the edge cases of the parser (address
/// forms, missing and unknown parts, state dependent errors).
const ADDRESSES: &[&str] = &[
    "/play", "/playbutton", "/stop", "/restart", "/record", "/overdub", "/overdub/launcher", "/overdub/x", "/repeat", "/punchIn", "/punchOut", "/click",
    "/click/volume", "/click/ticks", "/click/preroll", "/click/x", "/quantize", "/tempo/raw", "/tempo/tap", "/tempo/+", "/tempo/-", "/tempo/x", "/tempo",
    "/time", "/time/signature", "/time/Signature", "/time/x", "/position", "/position/+", "/position/-", "/position/++", "/position/--", "/position/start",
    "/position/x", "/crossfade", "/crossfade/reset", "/crossfade/x", "/autowrite", "/autowrite/launcher", "/autowrite/x", "/automationWriteMode",
    "/preroll", "/launcher", "/launcher/postRecordingAction", "/launcher/postRecordingTimeOffset", "/launcher/defaultQuantization", "/launcher/x", "/undo",
    "/redo", "/undo/x", "/layout", "/panel/noteEditor", "/panel/automationEditor", "/panel/devices", "/panel/mixer", "/panel/fullscreen", "/panel/x",
    "/panel", "/arranger/cueMarkerVisibility", "/arranger/playbackFollow", "/arranger/trackRowHeight", "/arranger/clipLauncherSectionVisibility",
    "/arranger/timeLineVisibility", "/arranger/ioSectionVisibility", "/arranger/effectTracksVisibility", "/arranger/x", "/arranger",
    "/mixer/clipLauncherSectionVisibility", "/mixer/crossFadeSectionVisibility", "/mixer/deviceSectionVisibility", "/mixer/sendsSectionVisibility",
    "/mixer/ioSectionVisibility", "/mixer/meterSectionVisibility", "/mixer/x", "/mixer", "/marker/1/launch", "/marker/2/launch", "/marker/0/launch",
    "/marker/1/x", "/marker/1", "/marker/x/launch", "/marker/bank/+", "/marker/bank/-", "/marker/bank/x", "/marker/bank", "/marker/x", "/marker",
    "/project/+", "/project/-", "/project/engine", "/project/save", "/project/param/1/value", "/project/param/1/indicate", "/project/param/1/reset",
    "/project/param/1/touched", "/project/param/1/x", "/project/param/1", "/project/param/0/value", "/project/param/9/value", "/project/param/+",
    "/project/param/-", "/project/param/bank/page/+", "/project/param/bank/page/-", "/project/param/bank/page/x", "/project/param/bank/page",
    "/project/param/bank/x", "/project/param/bank", "/project/param/x", "/project/param", "/project/page/1", "/project/page/2", "/project/page/0",
    "/project/page/select", "/project/page/selected", "/project/page/x", "/project/page", "/project/x", "/project", "/track/1/name", "/track/1/activated",
    "/track/1/crossfadeMode/A", "/track/1/crossfadeMode/B", "/track/1/crossfadeMode/AB", "/track/1/crossfadeMode", "/track/1/recordQuantization",
    "/track/1/select", "/track/1/selected", "/track/1/duplicate", "/track/1/remove", "/track/1/volume", "/track/1/volume/indicate",
    "/track/1/volume/reset", "/track/1/volume/touched", "/track/1/volume/x", "/track/1/pan", "/track/1/pan/indicate", "/track/1/pan/reset",
    "/track/1/pan/touched", "/track/1/pan/x", "/track/1/mute", "/track/1/solo", "/track/1/recarm", "/track/1/monitor", "/track/1/autoMonitor",
    "/track/1/send/1/activated", "/track/1/send/1/volume", "/track/1/send/1/volume/indicate", "/track/1/send/1/volume/reset",
    "/track/1/send/1/volume/touched", "/track/1/send/1/volume/x", "/track/1/send/1/x", "/track/1/send/1", "/track/1/send/0/volume",
    "/track/1/send/x/volume", "/track/1/send", "/track/1/clip/1/select", "/track/1/clip/1/selected", "/track/1/clip/1/launch",
    "/track/1/clip/1/launchAlt", "/track/1/clip/1/record", "/track/1/clip/1/create", "/track/1/clip/1/insertFile", "/track/1/clip/1/duplicate",
    "/track/1/clip/1/remove", "/track/1/clip/1/color", "/track/1/clip/1/x", "/track/1/clip/1", "/track/1/clip/0/launch", "/track/1/clip/stop",
    "/track/1/clip/returntoarrangement", "/track/1/clip/x", "/track/1/clip", "/track/1/enter", "/track/1/color", "/track/1/pinned", "/track/1/x",
    "/track/1", "/track/2/volume", "/track/9/volume", "/track/0/volume", "/track/-1/volume", "/track/+2/volume", "/track/2147483648/volume",
    "/track/1/volume/indicate/x", "/track//volume", "/track/indicate/volume", "/track/indicate/pan", "/track/indicate/send/1", "/track/indicate/send/0",
    "/track/indicate/send/x", "/track/indicate/send", "/track/indicate/x", "/track/indicate", "/track/bank/page/+", "/track/bank/page/-",
    "/track/bank/page/x", "/track/bank/page", "/track/bank/+", "/track/bank/-", "/track/bank/x", "/track/bank", "/track/+", "/track/-", "/track/add/audio",
    "/track/add/effect", "/track/add/instrument", "/track/add/x", "/track/add", "/track/stop", "/track/stopAlt", "/track/vu", "/track/toggleBank",
    "/track/parent", "/track/select/mute", "/track/selected/mute", "/track/selected/pinned", "/track/selected/volume/reset", "/track/selected/x",
    "/track/selected", "/track/select/send/1/volume", "/track/selected/clip/1/launch", "/track/selected/enter", "/track/param/1/value",
    "/track/param/1/reset", "/track/param/+", "/track/param/-", "/track/param/bank/page/+", "/track/param/x", "/track/page/1", "/track/page/select",
    "/track/page/x", "/track/x", "/track", "/master/volume", "/master/name", "/master/pinned", "/master/send/1/volume", "/master/clip/1/launch",
    "/master/enter", "/master/x", "/master", "/master/1/volume", "/scene/bank/+", "/scene/bank/-", "/scene/bank/x", "/scene/bank", "/scene/+", "/scene/-",
    "/scene/create", "/scene/add", "/scene/1/select", "/scene/1/selected", "/scene/1/launch", "/scene/1/launchAlt", "/scene/1/duplicate",
    "/scene/1/remove", "/scene/1/name", "/scene/1/color", "/scene/1/x", "/scene/1", "/scene/2/launch", "/scene/9/launch", "/scene/x", "/scene/selected",
    "/scene/0/launch", "/scene", "/device/sibling/1/select", "/device/sibling/1/selected", "/device/sibling/x/select", "/device/sibling/1/x",
    "/device/sibling/1", "/device/sibling", "/device/bank/page/+", "/device/bank/page/-", "/device/bank/page/x", "/device/bank/page", "/device/bank/x",
    "/device/bank", "/device/lastparam/value", "/device/lastparam/indicate", "/device/lastparam/reset", "/device/lastparam/touched", "/device/lastparam/x",
    "/device/lastparam", "/device/+", "/device/-", "/device/pinned", "/device/page/select", "/device/page/1", "/device/page/0", "/device/page/x",
    "/device/page", "/device/duplicate", "/device/remove", "/device/bypass", "/device/expand", "/device/parameters", "/device/window",
    "/device/indicate/param", "/device/indicate/x", "/device/indicate", "/device/param/1/value", "/device/param/1/indicate", "/device/param/1/reset",
    "/device/param/1/touched", "/device/param/1/x", "/device/param/1", "/device/param/0/value", "/device/param/+", "/device/param/-",
    "/device/param/bank/page/+", "/device/param/bank/page/-", "/device/param/bank/page", "/device/param/bank/x", "/device/param/bank", "/device/param/x",
    "/device/param", "/device/drumpad/1/mute", "/device/drumpad/selected/solo", "/device/drumpad/+", "/device/drumpad", "/device/drumpad/x",
    "/device/layer/1/activated", "/device/layer/1/select", "/device/layer/1/selected", "/device/layer/1/name", "/device/layer/1/volume",
    "/device/layer/1/volume/indicate", "/device/layer/1/volume/reset", "/device/layer/1/volume/touched", "/device/layer/1/volume/x",
    "/device/layer/1/pan", "/device/layer/1/pan/indicate", "/device/layer/1/pan/reset", "/device/layer/1/pan/touched", "/device/layer/1/mute",
    "/device/layer/1/solo", "/device/layer/1/send/1/volume", "/device/layer/1/send/1/volume/indicate", "/device/layer/1/send/1/volume/touched",
    "/device/layer/1/send/1/volume/reset", "/device/layer/1/send/1/activated", "/device/layer/1/send/1/x", "/device/layer/1/send/1",
    "/device/layer/1/send/x", "/device/layer/1/send/0/volume", "/device/layer/1/send", "/device/layer/1/enter", "/device/layer/1/x", "/device/layer/1",
    "/device/layer/0/mute", "/device/layer/0", "/device/layer/2/mute", "/device/layer/99/mute", "/device/layer/selected/mute",
    "/device/layer/select/volume", "/device/layer/parent", "/device/layer/+", "/device/layer/-", "/device/layer/page", "/device/layer/page/+",
    "/device/layer/page/-", "/device/layer/x", "/device/layer", "/device/x", "/device", "/primary/param/1/value", "/primary/param/+",
    "/primary/layer/1/mute", "/primary/layer/x", "/primary/layer/parent", "/primary/drumpad/1/mute", "/primary/bypass", "/primary/page/1",
    "/primary/indicate/param", "/primary/sibling/1/select", "/primary/lastparam/value", "/primary/x", "/primary", "/eq/type/1", "/eq/type/0", "/eq/type/x",
    "/eq/type", "/eq/gain/1", "/eq/gain/2", "/eq/freq/1", "/eq/freq/x", "/eq/q/1", "/eq/q/x", "/eq/q", "/eq/add", "/eq/bypass", "/eq/window",
    "/eq/param/1/value", "/eq/layer/1/mute", "/eq/drumpad/1/mute", "/eq/page/2", "/eq/x", "/eq", "/browser/preset", "/browser/tab/+", "/browser/tab/-",
    "/browser/tab/x", "/browser/tab", "/browser/device", "/browser/device/after", "/browser/device/before", "/browser/device/x", "/browser/insertClap",
    "/browser/commit", "/browser/cancel", "/browser/filter/1/+", "/browser/filter/6/-", "/browser/filter/1/reset", "/browser/filter/1/x",
    "/browser/filter/1", "/browser/filter/7/+", "/browser/filter/0/+", "/browser/filter/x/+", "/browser/filter", "/browser/result", "/browser/result/+",
    "/browser/result/-", "/browser/result/x", "/browser/x", "/browser", "/vkb_midi/velocity", "/vkb_midi/noterepeat/isActive",
    "/vkb_midi/noterepeat/period", "/vkb_midi/noterepeat/length", "/vkb_midi/noterepeat/x", "/vkb_midi/noterepeat", "/vkb_midi/1/note",
    "/vkb_midi/1/note/+", "/vkb_midi/1/note/-", "/vkb_midi/1/note/60", "/vkb_midi/1/note/300", "/vkb_midi/1/note/x", "/vkb_midi/1/drum/+",
    "/vkb_midi/1/drum/-", "/vkb_midi/1/drum/36", "/vkb_midi/1/drum/200", "/vkb_midi/1/drum/x", "/vkb_midi/1/drum", "/vkb_midi/1/cc", "/vkb_midi/1/cc/7",
    "/vkb_midi/1/aftertouch", "/vkb_midi/1/aftertouch/60", "/vkb_midi/1/aftertouch/300", "/vkb_midi/1/pitchbend", "/vkb_midi/2/note/61",
    "/vkb_midi/16/pitchbend", "/vkb_midi/1/x", "/vkb_midi/1", "/vkb_midi/x", "/vkb_midi", "/action/1", "/action/20", "/action/21", "/action/0",
    "/action/x", "/action", "/clip/stopall", "/clip/stopallAlt", "/clip/pinned", "/clip/quantize", "/clip/name", "/clip/color", "/clip/stop",
    "/clip/stopAlt", "/clip/+", "/clip/-", "/clip/launch", "/clip/launchAlt", "/clip/record", "/clip/create", "/clip/insertFile", "/clip/x", "/clip",
    "/refresh", "/refresh/x", "/", "", "//", "//play", "/play/", "/play//", "/x", "/x/y", "/Play", "/track/1/volume/",
];

/// The commands and what they did, in an empty model and with the complete model; and the
/// addresses which are accepted: executed without an unknown or missing command or an error for
/// at least one of the values, with the complete model.
fn input_golden() -> (String, String) {
    let mut out = String::new();
    let mut accepted: Vec<&str> = Vec::new();
    for (state, lines) in [("empty", ""), ("full", "full")] {
        let mut g = G::new();
        if !lines.is_empty() {
            g.model(TRACKS);
            g.model(DEVICE);
            g.model(MORE);
            g.rec.take();
            g.setup.collect_updates(&g.ctx, true);
        }
        let _ = writeln!(out, "# state {state}");
        let mut commands: Vec<(&str, Vec<OscArg>)> = Vec::new();
        for address in ADDRESSES {
            for value in values() {
                commands.push((address, value));
            }
        }
        commands.extend(special_values());
        for (address, args) in commands {
            let text = message_text(&OscMessage::new(address, args.clone()));
            let lines = g.send(address, args);
            let failed = lines.iter().any(|l| ["log Unknown OSC command", "log Missing command", "log Unknown Device command", "log ERROR"].iter().any(|p| l.starts_with(p)));
            if state == "full" && !failed && !accepted.contains(&address) {
                accepted.push(address);
            }
            // The changes the command caused, without the update frame
            let updates: Vec<String> = g.setup.collect_updates(&g.ctx, false).iter().filter(|m| m.address != "/update").map(message_text).collect();
            let _ = write!(out, "> {text}");
            if lines.is_empty() && updates.is_empty() {
                out.push_str(" -\n");
                continue;
            }
            out.push('\n');
            for line in lines {
                let _ = writeln!(out, "  {line}");
            }
            for update in updates {
                let _ = writeln!(out, "  out {update}");
            }
        }
    }
    let accepted: String = accepted.iter().map(|a| format!("{a}\n")).collect();
    (out, accepted)
}

/// The output of a full update and of a sequence of model changes.
fn output_golden() -> String {
    let mut out = String::new();
    let mut g = G::new();
    output_text(&mut out, "empty dump", &g.setup.collect_updates(&g.ctx, true));
    let steps: &[(&str, &str)] = &[
        ("tracks", TRACKS),
        ("device", DEVICE),
        ("everything else", MORE),
        ("names", "track/1/name Bass 2\ntrack/1/mute 1\ntrack/2/name Klänge\nscene/0/name Vers\nmarker/1/name Ende\nproject/name Lied\n"),
        ("volumes", "track/0/volume 0.5\ntrack/0/volume/str -6 dB\ntrack/1/pan 0.25\ntrack/0/send/0/volume 0.75\nmaster/volume 0.8\n"),
        ("vu", "track/0/vu 0.5\ntrack/1/vu 0.25\nmaster/vu 0.9\n"),
        ("selection", "track/0/select 0\ntrack/1/select 1\n"),
        ("device parameters", "device/param/0/value 0.75\ndevice/param/1/value 0.1\ndevice/param/0/value/str 75 %\ndevice/bypass 1\ndevice/window 1\n"),
        ("pages", "device/page/count 3\ndevice/page/0/name Main\ndevice/page/1/name Env\ndevice/page/2/name LFO\ndevice/page/selected 1\n"),
        ("layers", "device/layer/1/select 1\ndevice/layer/0/select 0\ndevice/layer/1/mute 1\ndevice/pad/1/exists 1\n"),
        ("transport", "play 1\ntempo 133.5\nrepeat 1\nclick 1\nrecord 1\n"),
        ("browser", "browser/column/0/item/0/select 1\nbrowser/result/0/select 1\nbrowser/column/1/item/0/exists 1\nbrowser/column/1/item/0/name u-he\n"),
        ("clips", "track/1/slot/count 8\ntrack/1/slot/0/exists 1\ntrack/1/slot/0/hasContent 1\ntrack/1/slot/0/playing 1\ntrack/1/slot/0/color 1 0 0\n"),
    ];
    for (title, lines) in steps {
        g.model(lines);
        output_text(&mut out, title, &g.setup.collect_updates(&g.ctx, false));
    }
    // The virtual keyboard marks the played note
    g.send("/vkb_midi/1/note/60", vec![OscArg::Int(100)]);
    output_text(&mut out, "played note", &g.setup.collect_updates(&g.ctx, false));
    g.send("/track/vu", vec![OscArg::Int(0)]);
    output_text(&mut out, "vu off", &g.setup.collect_updates(&g.ctx, false));
    g.send("/track/toggleBank", vec![]);
    g.model("fxtrack/0/select 1\n");
    output_text(&mut out, "effect tracks", &g.setup.collect_updates(&g.ctx, false));
    output_text(&mut out, "nothing changed", &g.setup.collect_updates(&g.ctx, false));
    output_text(&mut out, "full dump", &g.setup.collect_updates(&g.ctx, true));
    out
}

fn golden_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/controller/osc/golden").join(name)
}

fn check_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("DBM_OSC_GOLDEN_RECORD").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if expected != actual {
        let first = expected.lines().zip(actual.lines()).enumerate().find(|(_, (e, a))| e != a);
        match first {
            Some((line, (e, a))) => panic!("{name} differs at line {}:\nexpected: {e}\nactual:   {a}", line + 1),
            None => panic!("{name} differs in length: expected {} lines, got {}", expected.lines().count(), actual.lines().count()),
        }
    }
}

#[test]
fn input_is_unchanged() {
    let (input, accepted) = input_golden();
    check_golden("input.txt", &input);
    check_golden("accepted.txt", &accepted);
}

#[test]
fn output_is_unchanged() {
    check_golden("output.txt", &output_golden());
}

/// The flush cost per tick: 3 tracks, 1 device, nothing changes (the common case) and a full
/// update.
#[test]
#[ignore]
fn flush_cost() {
    let mut g = G::new();
    g.model(TRACKS);
    g.model(DEVICE);
    g.setup.collect_updates(&g.ctx, true);
    let ticks = 5000;
    let start = Instant::now();
    for _ in 0..ticks {
        let messages = g.setup.collect_updates(&g.ctx, false);
        assert!(messages.is_empty());
    }
    let idle = start.elapsed() / ticks;
    let dumps = 500;
    let start = Instant::now();
    let mut count = 0;
    for _ in 0..dumps {
        count = g.setup.collect_updates(&g.ctx, true).len();
    }
    let dump = start.elapsed() / dumps;
    println!("OSC flush per tick (3 tracks, 1 device): {idle:?} without changes, {dump:?} for a full update of {count} messages");
}
