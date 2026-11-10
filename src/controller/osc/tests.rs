// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the OSC modules without the network: a message is executed by the setup (former
//! `OSCParser.handle`), the commands for the host are recorded; model changes are reported by
//! the modules (`collect_updates`).

use std::rc::Rc;

use super::setup::{create, OscSetup};
use crate::backend::{BackendCall, RecordingBackend, Sender};
use crate::controller::{ControllerSetup, MidiPorts};
use crate::framework::ctx::Ctx;
use crate::framework::osc::{OscArg, OscMessage};
use crate::host::{Host, MemoryLog};
use crate::parser::Parser;

pub(super) struct T {
    pub ctx: Ctx,
    pub setup: OscSetup,
    pub backend: Rc<RecordingBackend>,
    pub log: Rc<MemoryLog>,
    parser: Parser,
}

impl T {
    pub fn new() -> T {
        Self::with_config(crate::config::osc::for_reaper())
    }

    pub fn with_config(config: crate::config::Configuration) -> T {
        let backend = Rc::new(RecordingBackend::default());
        let log = Rc::new(MemoryLog::default());
        let host = Host::new(log.clone());
        let parts = create(host, Sender::new(backend.clone()), MidiPorts { input: 0, output: 0 }, config).unwrap();
        let mut ctx = parts.ctx;
        let mut setup = OscSetup::new(crate::config::osc::bank_page_size(&ctx.config));
        setup.init_model(&mut ctx);
        // Without the network: the changes of the loaded settings would start the server
        ctx.config.take_changed_settings();
        setup.handle_changes(&mut ctx);
        backend.take_calls();
        T { ctx, setup, backend, log, parser: Parser::new() }
    }

    /// Feed lines of the text protocol into the model.
    pub fn model(&mut self, lines: &str) {
        for line in lines.lines() {
            self.parser.parse_line(&mut self.ctx.model, line);
        }
        self.setup.handle_changes(&mut self.ctx);
    }

    /// Execute an OSC message, return the recorded calls.
    pub fn send(&mut self, address: &str, args: Vec<OscArg>) -> Vec<BackendCall> {
        self.setup.handle_message(&mut self.ctx, &OscMessage::new(address, args));
        self.ctx.run_due_tasks(self.ctx.host.now() + std::time::Duration::from_secs(1));
        self.backend.take_calls()
    }

    pub fn calls(&mut self, address: &str, args: Vec<OscArg>) -> String {
        format!("{:?}", self.send(address, args))
    }

    /// The changed values since the last call as `address [ values ]`, without `/update`.
    pub fn updates(&mut self) -> Vec<String> {
        self.setup.collect_updates(&self.ctx, false).iter().filter(|m| m.address != "/update").map(OscMessage::log_text).collect()
    }

    pub fn dump(&mut self) -> Vec<OscMessage> {
        self.setup.collect_updates(&self.ctx, true)
    }

    pub fn logs(&self) -> Vec<String> {
        self.log.lines.borrow().iter().cloned().collect()
    }
}

pub fn i(v: i32) -> OscArg {
    OscArg::Int(v)
}

pub fn s(v: &str) -> OscArg {
    OscArg::Str(v.to_string())
}

const TRACKS: &str = "track/count 3\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/canHoldNotes 1\ntrack/1/exists 1\ntrack/1/name Bass\ntrack/2/exists 1\ntrack/2/name Keys\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\ntrack/0/slot/count 8\ntrack/0/slot/2/exists 1\ntrack/0/slot/2/hasContent 1\n";

const DEVICE: &str = "device/exists 1\ndevice/name Polysynth\ndevice/param/count 16\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\ndevice/param/1/name Res\n\
device/sibling/0/name Polysynth\ndevice/count 2\n";

fn contains(calls: &str, part: &str) {
    assert!(calls.to_lowercase().contains(&part.to_lowercase()), "{part} in {calls}");
}

#[test]
fn transport_commands() {
    let mut t = T::new();
    assert_eq!(t.calls("/play", vec![]), r#"[NoArg("play", "")]"#, "starts when stopped");
    assert_eq!(t.calls("/play", vec![i(0)]), "[]", "already stopped");
    t.model("play 1\n");
    assert_eq!(t.calls("/play", vec![i(1)]), "[]", "already playing");
    assert_eq!(t.calls("/play", vec![i(0)]), r#"[NoArg("play", "")]"#);
    contains(&t.calls("/stop", vec![]), "stop");
    assert_eq!(t.calls("/repeat", vec![i(1)]), r#"[IntArg("repeat", "", 1)]"#);
    contains(&t.calls("/click/volume", vec![i(64)]), r#"IntArg("metro_vol", "", 64)"#);
    contains(&t.calls("/tempo/raw", vec![OscArg::Double(99.5)]), r#"DoubleArg("tempo", "", 99.5)"#);
    contains(&t.calls("/tempo/-", vec![i(10)]), r#"DoubleArg("tempo", "", 110.0)"#);
    contains(&t.calls("/automationWriteMode", vec![s("latch")]), "automation");
    assert_eq!(t.calls("/automationWriteMode", vec![s("nonsense")]), "[]");
    assert!(t.logs().iter().any(|l| l.contains("No enum constant AutomationMode.NONSENSE")), "{:?}", t.logs());
    contains(&t.calls("/time/signature", vec![s("3/4")]), "signature");
    assert_eq!(t.calls("/time/signature", vec![s("3 / 4")]), "[]");
    assert!(t.logs().iter().any(|l| l == "Illegal parameter: /time/signature Illegal parameter: 3 / 4"), "{:?}", t.logs());
    // playbutton executes the play command with DOWN, which it ignores (as in Java)
    assert_eq!(t.calls("/playbutton", vec![i(1)]), "[]");
    assert_eq!(t.calls("/restart", vec![i(0)]), "[]", "not a trigger");
    contains(&t.calls("/click", vec![OscArg::Bool(true)]), r#"IntArg("transport", "metronome", 0)"#);
}

#[test]
fn transport_updates() {
    let mut t = T::new();
    let dump = t.dump();
    assert_eq!(dump.first().unwrap().log_text(), "/update [ 1 ]");
    assert_eq!(dump.last().unwrap().log_text(), "/update [ 0 ]");
    t.model("play 1\ntempo 133.5\n");
    let updates = t.updates();
    assert!(updates.contains(&"/play [ 1 ]".to_string()), "{updates:?}");
    assert!(updates.contains(&"/tempo/raw [ 133.5 ]".to_string()), "{updates:?}");
    assert!(t.updates().is_empty(), "only changes");
    let raw = t.setup.collect_updates(&t.ctx, true);
    assert!(raw.iter().any(|m| m.address == "/tempo/raw" && m.args == [OscArg::Float(133.5)]), "a double is sent as a float");
}

#[test]
fn track_commands() {
    let mut t = T::new();
    t.model(TRACKS);
    contains(&t.calls("/track/2/select", vec![i(1)]), r#"IntArg("track", "1/select", 1)"#);
    contains(&t.calls("/track/1/volume", vec![i(127)]), r#"DoubleArg("track", "0/volume", 1.0)"#);
    contains(&t.calls("/track/1/volume/reset", vec![]), "volume");
    contains(&t.calls("/track/3/pan", vec![i(0)]), r#"DoubleArg("track", "2/pan", 0.0)"#);
    contains(&t.calls("/track/1/mute", vec![]), r#"IntArg("track", "0/mute", 1)"#);
    contains(&t.calls("/track/1/solo", vec![i(1)]), "0/solo");
    contains(&t.calls("/track/1/recarm", vec![i(1)]), "0/recarm");
    contains(&t.calls("/master/volume", vec![i(10)]), "master");
    contains(&t.calls("/track/selected/mute", vec![]), "0/mute", );
    contains(&t.calls("/track/1/send/2/volume", vec![i(127)]), "0/send/1/volume");
    contains(&t.calls("/track/1/clip/3/launch", vec![i(1)]), "launch");
    contains(&t.calls("/track/1/color", vec![s("rgb(255,0,0)")]), r#"StringArg("track", "0/color", "RGB(255,0,0)")"#);
    assert_eq!(t.calls("/track/1/color", vec![s("red")]), "[]");
    contains(&t.calls("/track/1/name", vec![s("Beats")]), "Beats");
    contains(&t.calls("/track/+", vec![]), r#"IntArg("track", "1/select", 1)"#);
    contains(&t.calls("/track/add/audio", vec![]), "add");
    assert_eq!(t.calls("/track/9/volume", vec![i(1)]), "[]", "no track");
    // Errors as in Java
    t.send("/track/1/send/x/volume", vec![i(1)]);
    t.send("/track/0/volume", vec![i(1)]);
    t.send("/track/1/volume", vec![]);
    t.send("/track/add/nonsense", vec![]);
    let logs = t.logs();
    assert!(logs.iter().any(|l| l.contains("/track/1/send/x/volume") && l.starts_with("ERROR")), "{logs:?}");
    assert!(logs.iter().any(|l| l.contains("/track/0/volume") && l.starts_with("ERROR")), "{logs:?}");
    assert!(logs.iter().any(|l| l == "Illegal parameter: /track/1/volume Number parameter missing"), "{logs:?}");
    assert!(logs.iter().any(|l| l == "Unknown OSC command: /track/add/nonsense nonsense"), "{logs:?}");

    // VU meters are switched by the client
    assert!(t.ctx.config.is_enable_vu_meters());
    t.send("/track/vu", vec![i(0)]);
    assert!(!t.ctx.config.is_enable_vu_meters());
}

#[test]
fn track_updates() {
    let mut t = T::new();
    t.model(TRACKS);
    let updates = t.updates();
    for expected in [
        "/track/1/exists [ 1 ]",
        "/track/1/name [ Drums ]",
        "/track/1/selected [ 1 ]",
        "/track/2/name [ Bass ]",
        "/track/1/send/1/name [ FX1 ]",
        "/track/1/send/2/exists [ 1 ]",
        "/track/1/clip/3/hasContent [ 1 ]",
        "/track/selected/name [ Drums ]",
        "/track/selected/pinned [ 0 ]",
        "/master/name [ Master ]",
        "/track/1/crossfadeMode/AB [ 1 ]",
    ] {
        assert!(updates.contains(&expected.to_string()), "{expected} in {updates:?}");
    }
    assert!(!updates.iter().any(|u| u.starts_with("/master/send/") || u.starts_with("/master/clip/")), "the master has no sends and clips");
    assert!(!updates.iter().any(|u| u.starts_with("/track/1/pinned")), "only the cursor track has pinned");
    t.model("track/1/name Bass 2\ntrack/1/mute 1\n");
    assert_eq!(t.updates(), ["/track/2/name [ Bass 2 ]", "/track/2/mute [ 1 ]"]);
    t.model("track/0/select 0\ntrack/1/select 1\n");
    let updates = t.updates();
    assert!(updates.contains(&"/track/selected/name [ Bass 2 ]".to_string()), "{updates:?}");

    // Non-ASCII names are replaced
    t.model("track/2/name Klänge\n");
    assert_eq!(t.updates(), ["/track/3/name [ Klaenge ]"]);
}

#[test]
fn device_commands_and_updates() {
    let mut t = T::new();
    t.model(TRACKS);
    t.model(DEVICE);
    let updates = t.updates();
    for expected in ["/device/exists [ 1 ]", "/device/name [ Polysynth ]", "/device/param/1/name [ Cutoff ]", "/device/param/1/value [ 64 ]", "/device/bypass [ 1 ]"] {
        assert!(updates.contains(&expected.to_string()), "{expected} in {updates:?}");
    }
    contains(&t.calls("/device/param/1/value", vec![i(127)]), "param/0/value");
    contains(&t.calls("/device/param/2/reset", vec![]), "param/1");
    contains(&t.calls("/device/bypass", vec![]), "bypass");
    contains(&t.calls("/device/window", vec![]), "window");
    contains(&t.calls("/device/+", vec![]), "device");
    contains(&t.calls("/device/page/2", vec![]), "device");
    assert_eq!(t.calls("/device/param/+", vec![i(0)]), "[]", "not a trigger");
    t.send("/device/nonsense", vec![]);
    assert!(t.logs().iter().any(|l| l == "Unknown Device command: nonsense"), "{:?}", t.logs());
    t.send("/eq/type/1", vec![s("nonsense")]);
    assert!(t.logs().iter().any(|l| l.contains("No enum constant EqualizerBandType.NONSENSE")), "{:?}", t.logs());
    // The type of an equalizer band
    contains(&t.calls("/eq/type/1", vec![s("bell")]), "eq");
    contains(&t.calls("/eq/add", vec![]), "eq");
    t.send("/device/layer/selected/mute", vec![]);
    assert!(t.logs().iter().any(|l| l.contains("/device/layer/selected/mute") && l.starts_with("ERROR")), "no selected layer: {:?}", t.logs());
}

#[test]
fn scenes_markers_and_more() {
    let mut t = T::new();
    t.model(TRACKS);
    t.model("scene/count 2\nscene/0/exists 1\nscene/0/name Verse\nscene/1/exists 1\nmarker/count 1\nmarker/0/exists 1\nmarker/0/name Intro\n");
    let updates = t.updates();
    for expected in ["/scene/1/name [ Verse ]", "/scene/2/exists [ 1 ]", "/marker/1/name [ Intro ]", "/marker/1/exists [ 1 ]"] {
        assert!(updates.contains(&expected.to_string()), "{expected} in {updates:?}");
    }
    contains(&t.calls("/scene/2/launch", vec![i(1)]), "scene");
    contains(&t.calls("/scene/1/name", vec![s("Chorus")]), "Chorus");
    contains(&t.calls("/scene/add", vec![]), "createScene");
    contains(&t.calls("/marker/1/launch", vec![]), "marker");
    contains(&t.calls("/layout", vec![s("mix")]), r#"IntArg("application", "layout", 1)"#);
    contains(&t.calls("/panel/mixer", vec![]), "mixer");
    contains(&t.calls("/arranger/playbackFollow", vec![]), "arranger");
    contains(&t.calls("/project/engine", vec![i(0)]), "engine");
    contains(&t.calls("/undo", vec![]), "undo");
    contains(&t.calls("/redo", vec![]), "redo");
    assert_eq!(t.calls("/clip/stopall", vec![]), r#"[NoArg("scene", "stop")]"#);
    contains(&t.calls("/clip/stop", vec![]), "stop");
    // No action is assigned
    assert_eq!(t.calls("/action/1", vec![]), "[]");
    t.ctx.config.set_value(crate::config::osc::action_setting(1), "Save");
    contains(&t.calls("/action/2", vec![]), "Save");
    contains(&t.calls("/action/25", vec![]), "[]");
}

#[test]
fn virtual_keyboard() {
    let mut t = T::new();
    t.model(TRACKS);
    assert_eq!(t.send("/vkb_midi/1/note/60", vec![i(100)]), [BackendCall::Midi(0, 0x90, 60, 100)]);
    assert_eq!(t.send("/vkb_midi/2/note", vec![i(61), i(90)]), [BackendCall::Midi(0, 0x91, 61, 90)]);
    assert_eq!(t.send("/vkb_midi/16/cc/7", vec![i(99)]), [BackendCall::Midi(0, 0xBF, 7, 99)]);
    assert_eq!(t.send("/vkb_midi/1/pitchbend", vec![i(64)]), [BackendCall::Midi(0, 0xE0, 0, 64)]);
    assert_eq!(t.send("/vkb_midi/1/aftertouch", vec![i(64)]), [BackendCall::Midi(0, 0xD0, 0, 64)]);
    assert_eq!(t.send("/vkb_midi/1/aftertouch/60", vec![i(64)]), [BackendCall::Midi(0, 0xA0, 60, 64)]);
    // The played note is marked, the others have the scale colors
    let updates = t.updates();
    assert!(updates.contains(&"/vkb_midi/note/60/color [ rgb(0,255,0) ]".to_string()), "{updates:?}");
    // Fixed velocity
    t.send("/vkb_midi/velocity", vec![i(64)]);
    assert_eq!(t.send("/vkb_midi/1/note/62", vec![i(127)]), [BackendCall::Midi(0, 0x90, 62, 64)]);
    assert_eq!(t.send("/vkb_midi/1/note/62", vec![i(0)]), [BackendCall::Midi(0, 0x90, 62, 0)], "note off stays");
    // Octave up: the key translation moves the notes
    t.send("/vkb_midi/1/note/+", vec![]);
    assert_eq!(t.send("/vkb_midi/1/note/60", vec![i(0)]), [BackendCall::Midi(0, 0x90, 72, 0)]);
    t.send("/vkb_midi/noterepeat/period", vec![s("1/32")]);
    t.send("/vkb_midi/noterepeat/period", vec![s("1/3")]);
    assert!(t.logs().iter().any(|l| l.starts_with("Illegal parameter: /vkb_midi/noterepeat/period Value must be one of")), "{:?}", t.logs());
    let updates = t.updates();
    assert!(updates.contains(&"/vkb_midi/noterepeat/isActive [ 0 ]".to_string()) || t.dump().iter().any(|m| m.address == "/vkb_midi/noterepeat/isActive"));
}

#[test]
fn value_resolution_and_settings() {
    let mut config = crate::config::osc::for_reaper();
    config.set_value(crate::config::osc::VALUE_RESOLUTION, crate::config::osc::VALUE_RESOLUTION_OPTIONS[2]);
    let mut t = T::with_config(config);
    t.model(TRACKS);
    t.model("track/0/volume 0.5\n");
    let updates = t.updates();
    assert!(updates.contains(&"/track/1/volume [ 8192 ]".to_string()), "high resolution: {updates:?}");
    contains(&t.calls("/track/1/volume", vec![i(16383)]), r#"DoubleArg("track", "0/volume", 1.0)"#);
    // Another resolution restarts the controller
    assert!(!t.ctx.restart_requested);
    t.ctx.config.set_value(crate::config::osc::VALUE_RESOLUTION, crate::config::osc::VALUE_RESOLUTION_OPTIONS[0]);
    t.setup.handle_settings(&mut t.ctx);
    assert!(t.ctx.restart_requested);
}

#[test]
fn every_command_once() {
    // No panic for any command with several values (the "touch everything" test)
    let mut t = T::new();
    t.model(TRACKS);
    t.model(DEVICE);
    let values: Vec<Vec<OscArg>> = vec![vec![], vec![i(1)], vec![i(0)], vec![OscArg::Float(0.5)], vec![s("x")], vec![i(60), i(100)], vec![OscArg::Bool(true)], vec![OscArg::Nil], vec![i(-5)]];
    let addresses = [
        "/play", "/playbutton", "/stop", "/restart", "/record", "/overdub", "/overdub/launcher", "/repeat", "/punchIn", "/punchOut", "/click", "/click/volume",
        "/click/ticks", "/click/preroll", "/click/x", "/quantize", "/tempo/raw", "/tempo/tap", "/tempo/+", "/tempo/-", "/tempo/x", "/tempo", "/time",
        "/time/signature", "/time/x", "/position", "/position/+", "/position/-", "/position/++", "/position/--", "/position/start", "/position/x",
        "/crossfade", "/crossfade/reset", "/autowrite", "/autowrite/launcher", "/automationWriteMode", "/preroll", "/launcher",
        "/launcher/postRecordingAction", "/launcher/postRecordingTimeOffset", "/launcher/defaultQuantization", "/launcher/x", "/undo", "/redo", "/layout",
        "/panel/noteEditor", "/panel/automationEditor", "/panel/devices", "/panel/mixer", "/panel/fullscreen", "/panel/x", "/panel",
        "/arranger/cueMarkerVisibility", "/arranger/playbackFollow", "/arranger/trackRowHeight", "/arranger/clipLauncherSectionVisibility",
        "/arranger/timeLineVisibility", "/arranger/ioSectionVisibility", "/arranger/effectTracksVisibility", "/arranger/x",
        "/mixer/clipLauncherSectionVisibility", "/mixer/crossFadeSectionVisibility", "/mixer/deviceSectionVisibility", "/mixer/sendsSectionVisibility",
        "/mixer/ioSectionVisibility", "/mixer/meterSectionVisibility", "/mixer/x", "/marker/1/launch", "/marker/0/launch", "/marker/1/x", "/marker/bank/+",
        "/marker/bank/-", "/marker/bank/x", "/marker/x", "/marker", "/project/+", "/project/-", "/project/engine", "/project/save", "/project/param/1/value",
        "/project/param/1/indicate", "/project/param/1/reset", "/project/param/1/touched", "/project/param/1/x", "/project/param/+", "/project/param/-",
        "/project/param/bank/page/+", "/project/param/bank/page/-", "/project/param/bank/x", "/project/page/1", "/project/page/select", "/project/page/x",
        "/project/x", "/track/1/name", "/track/1/activated", "/track/1/crossfadeMode/A", "/track/1/crossfadeMode", "/track/1/recordQuantization",
        "/track/1/select", "/track/1/duplicate", "/track/1/remove", "/track/1/volume", "/track/1/volume/indicate", "/track/1/volume/reset",
        "/track/1/volume/touched", "/track/1/pan", "/track/1/pan/indicate", "/track/1/pan/reset", "/track/1/pan/touched", "/track/1/mute", "/track/1/solo",
        "/track/1/recarm", "/track/1/monitor", "/track/1/autoMonitor", "/track/1/send/1/activated", "/track/1/send/1/volume",
        "/track/1/send/1/volume/indicate", "/track/1/send/1/volume/reset", "/track/1/send/1/volume/touched", "/track/1/send/1/x", "/track/1/send",
        "/track/1/clip/1/select", "/track/1/clip/1/launch", "/track/1/clip/1/launchAlt", "/track/1/clip/1/record", "/track/1/clip/1/create",
        "/track/1/clip/1/insertFile", "/track/1/clip/1/duplicate", "/track/1/clip/1/remove", "/track/1/clip/1/color", "/track/1/clip/1/x",
        "/track/1/clip/stop", "/track/1/clip/returntoarrangement", "/track/1/clip/x", "/track/1/enter", "/track/1/color", "/track/1/pinned", "/track/1/x",
        "/track/indicate/volume", "/track/indicate/pan", "/track/indicate/send/1", "/track/indicate/send", "/track/indicate/x", "/track/bank/page/+",
        "/track/bank/page/-", "/track/bank/+", "/track/bank/-", "/track/bank/x", "/track/+", "/track/-", "/track/add/audio", "/track/add/effect",
        "/track/add/instrument", "/track/stop", "/track/stopAlt", "/track/vu", "/track/toggleBank", "/track/parent", "/track/select/mute",
        "/track/selected/pinned", "/track/param/1/value", "/track/param/+", "/track/page/1", "/track/x", "/master/volume", "/master/pinned",
        "/master/clip/1/launch", "/scene/bank/+", "/scene/bank/-", "/scene/bank/x", "/scene/+", "/scene/-", "/scene/create", "/scene/add",
        "/scene/1/select", "/scene/1/launch", "/scene/1/launchAlt", "/scene/1/duplicate", "/scene/1/remove", "/scene/1/name", "/scene/1/color",
        "/scene/1/x", "/scene/x", "/scene/0/launch", "/device/sibling/1/select", "/device/sibling/x/select", "/device/sibling/1/x", "/device/bank/page/+",
        "/device/bank/page/-", "/device/bank/x", "/device/lastparam/value", "/device/+", "/device/-", "/device/pinned", "/device/page/select",
        "/device/page/1", "/device/page/x", "/device/duplicate", "/device/remove", "/device/bypass", "/device/expand", "/device/parameters",
        "/device/window", "/device/indicate/param", "/device/indicate/x", "/device/param/1/value", "/device/param/1/x", "/device/param/+",
        "/device/param/-", "/device/param/bank/page/+", "/device/param/bank/page/-", "/device/param/bank/x", "/device/param/x", "/device/drumpad/1/mute",
        "/device/layer/1/activated", "/device/layer/1/select", "/device/layer/1/name", "/device/layer/1/volume", "/device/layer/1/volume/indicate",
        "/device/layer/1/volume/reset", "/device/layer/1/volume/touched", "/device/layer/1/pan", "/device/layer/1/pan/reset", "/device/layer/1/mute",
        "/device/layer/1/solo", "/device/layer/1/send/1/volume", "/device/layer/1/send/1/activated", "/device/layer/1/send/1/x",
        "/device/layer/1/send/1", "/device/layer/1/send", "/device/layer/1/enter", "/device/layer/1/x", "/device/layer/99/mute",
        "/device/layer/selected/mute", "/device/layer/parent", "/device/layer/+", "/device/layer/-", "/device/layer/page", "/device/layer/page/+",
        "/device/layer/x", "/primary/param/1/value", "/primary/layer/1/mute", "/primary/bypass", "/eq/type/1", "/eq/type/x", "/eq/gain/1", "/eq/freq/1",
        "/eq/q/1", "/eq/q/x", "/eq/add", "/eq/bypass", "/eq/param/1/value", "/browser/preset", "/browser/tab/+", "/browser/device", "/browser/device/before",
        "/browser/insertClap", "/browser/commit", "/browser/cancel", "/browser/filter/1/+", "/browser/filter/7/+", "/browser/filter/x/+",
        "/browser/result", "/browser/result/-", "/browser/x", "/vkb_midi/velocity", "/vkb_midi/noterepeat/isActive", "/vkb_midi/noterepeat/period",
        "/vkb_midi/noterepeat/length", "/vkb_midi/noterepeat/x", "/vkb_midi/1/note", "/vkb_midi/1/note/+", "/vkb_midi/1/note/-", "/vkb_midi/1/note/60",
        "/vkb_midi/1/note/300", "/vkb_midi/1/note/x", "/vkb_midi/1/drum/+", "/vkb_midi/1/drum/-", "/vkb_midi/1/drum/36", "/vkb_midi/1/drum/200",
        "/vkb_midi/1/cc", "/vkb_midi/1/cc/7", "/vkb_midi/1/aftertouch", "/vkb_midi/1/aftertouch/60", "/vkb_midi/1/aftertouch/300",
        "/vkb_midi/1/pitchbend", "/vkb_midi/1/x", "/vkb_midi/x", "/vkb_midi", "/action/1", "/action/x", "/action", "/clip/stopall", "/clip/stopallAlt",
        "/clip/pinned", "/clip/quantize", "/clip/name", "/clip/color", "/clip/stop", "/clip/stopAlt", "/clip/+", "/clip/-", "/clip/launch",
        "/clip/launchAlt", "/clip/record", "/clip/create", "/clip/insertFile", "/clip/x", "/refresh", "/", "/x",
    ];
    for address in addresses {
        for value in &values {
            t.send(address, value.clone());
        }
        t.updates();
    }
    // The modules cover all commands of the Java version
    let commands: Vec<&str> = addresses.iter().filter_map(|a| a.split('/').nth(1)).collect();
    for command in ["play", "track", "master", "device", "primary", "eq", "browser", "vkb_midi", "action", "clip", "scene", "marker", "project", "layout", "panel", "arranger", "mixer", "undo", "redo"] {
        assert!(commands.contains(&command), "{command}");
    }
}

