// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Electra One (Mk1/Mk2) and the Electra One Mini: the handshake on the CTRL port
//! (device information, event subscription, preset list, preset switch, online/offline), the
//! CC values and their numbering, the JSON control updates, the value texts, the `sgt` Lua calls
//! and the repaint framing, the pages and modes, the knobs and pads, the knob touches and their
//! combinations, the SysEx chunks, and all modes and controls.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordedCalls, RecordingBackend};
use drivenbymoss::config::electra as config;
use drivenbymoss::controller::electra::surface as electra;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ContinuousId, Modes};
use drivenbymoss::midi::ButtonEvent;
use drivenbymoss::protocol::{Chan, Cmd, ValueOp};

/// Port 1 (the controls).
const IN: i32 = 3;
/// The CTRL port (SysEx).
const CTRL_IN: i32 = 4;
const OUT: i32 = 5;
const CTRL_OUT: i32 = 6;

const MODEL: &str = "track/count 3\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volume/str 0 dB\ntrack/0/pan/str C\ntrack/0/color 255 0 0\n\
track/1/exists 1\ntrack/1/name Bäss\ntrack/1/volume/str -6 dB\ntrack/1/mute 1\n\
track/2/exists 1\ntrack/2/name Keys\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\ntrack/0/slot/0/name Loop\ntrack/0/slot/1/exists 1\n\
scene/count 2\nscene/0/exists 1\nscene/0/name Verse\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\ndevice/param/0/value/str 50 %\nplay 0\n";

const INFO_MK2: &str = r#"{"versionText":"v5.0.6","versionSeq":500000600,"serial":"EO-00000","hwRevision":"2.00","model":"mk2","modelNum":2}"#;
const INFO_MK1: &str = r#"{"versionText":"v3.4.3","versionSeq":300400300,"serial":"EO-1","hwRevision":"1.00"}"#;
const INFO_MINI: &str = r#"{"versionText":"v5.0.6","versionSeq":500000600,"serial":"EO-2","hwRevision":"3.00","model":"mini","modelNum":3}"#;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    Normal,
    Mini,
}

impl Variant {
    fn class(self) -> &'static str {
        match self {
            Variant::Normal => "ElectraOneControllerInstance",
            Variant::Mini => "ElectraOneMiniControllerInstance",
        }
    }

    fn preset(self) -> &'static str {
        match self {
            Variant::Normal => "DrivenByMoss",
            Variant::Mini => "DrivenByMoss Mini",
        }
    }

    fn info(self) -> &'static str {
        match self {
            Variant::Normal => INFO_MK2,
            Variant::Mini => INFO_MINI,
        }
    }
}

/// A SysEx message of the Electra API.
fn sysex(command: &[u8], content: &[u8]) -> Vec<u8> {
    let mut data = vec![0xF0, 0x00, 0x21, 0x45];
    data.extend_from_slice(command);
    data.extend_from_slice(content);
    data.push(0xF7);
    data
}

/// The preset list with the DrivenByMoss preset in bank 1, slot 3 and a preset of a synth.
fn preset_list(name: &str) -> Vec<u8> {
    let json = format!(r#"{{"version":1,"presets":[{{"slot":0,"bankNumber":0,"name":"ReaSynth","projectId":"x"}},{{"slot":3,"bankNumber":1,"name":"{name}","projectId":"y"}},{{"slot":4,"name":"No bank"}}]}}"#);
    sysex(&[0x01, 0x04], json.as_bytes())
}

/// Start an instance (detecting the ports) and run the delayed startup (at 1100ms).
fn start(variant: Variant) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-electra-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.electra.one.{}\n", variant.class())).unwrap();
    let backend = Rc::new(RecordingBackend {
        inputs: vec![(IN, "Electra Controller Electra Port".into()), (CTRL_IN, "Electra Controller Electra CTRL".into())],
        outputs: vec![(OUT, "Electra Controller Electra Port".into()), (CTRL_OUT, "Electra Controller Electra CTRL".into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    assert_eq!(app.instance_count(), 1);
    app.set_time_ms(0);
    app.start_infrastructure();
    app.update_model(MODEL);
    app.set_time_ms(1100);
    app.tick();
    (app, backend)
}

/// Start and go through the handshake until the controller is online.
fn start_online(variant: Variant) -> (App, Rc<RecordingBackend>, u64) {
    let (mut app, backend) = start(variant);
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], variant.info().as_bytes()));
    app.on_midi_event(CTRL_IN, &preset_list(variant.preset()));
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 3]));
    assert!(electra::is_online(ctx(&mut app)));
    (app, backend, 1100)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).expect("instance runs")
}

fn advance(app: &mut App, time_ms: &mut u64, ms: u64) {
    *time_ms += ms;
    app.set_time_ms(*time_ms);
    app.tick();
}

/// The messages sent to a port.
fn sent(calls: &[BackendCall], port: i32) -> Vec<Vec<u8>> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(p, d) if *p == port => Some(d.clone()),
            _ => None,
        })
        .collect()
}

/// The SysEx messages sent to CTRL as (command bytes after the header, content as text).
fn ctrl_messages(calls: &[BackendCall]) -> Vec<(Vec<u8>, String)> {
    sent(calls, CTRL_OUT)
        .into_iter()
        .map(|d| {
            assert_eq!(d[..4], [0xF0, 0x00, 0x21, 0x45], "{d:02X?}");
            assert_eq!(*d.last().unwrap(), 0xF7);
            assert!(d[1..d.len() - 1].iter().all(|b| *b < 0x80), "a data byte above 127: {d:02X?}");
            let body = &d[4..d.len() - 1];
            let (command, text) = match body[..2] {
                [0x14, 0x07] => (body[..4].to_vec(), &body[4..]),
                [0x14, 0x0E] => (body[..5].to_vec(), &body[5..]),
                [0x08, 0x0D] => (body[..2].to_vec(), &body[2..]),
                _ => (body.to_vec(), &[][..]),
            };
            (command, String::from_utf8(text.to_vec()).unwrap())
        })
        .collect()
}

/// The JSON control updates by control ID.
fn control_updates(calls: &[BackendCall]) -> Vec<(i32, String)> {
    ctrl_messages(calls).into_iter().filter(|(c, _)| c[..2] == [0x14, 0x07]).map(|(c, t)| (c[2] as i32 + ((c[3] as i32) << 7), t)).collect()
}

/// The value texts by control ID.
fn value_texts(calls: &[BackendCall]) -> Vec<(i32, String)> {
    ctrl_messages(calls).into_iter().filter(|(c, _)| c[..2] == [0x14, 0x0E]).map(|(c, t)| (c[2] as i32 + ((c[3] as i32) << 7), t)).collect()
}

/// The Lua calls.
fn lua(calls: &[BackendCall]) -> Vec<String> {
    ctrl_messages(calls).into_iter().filter(|(c, _)| c[..] == [0x08, 0x0D]).map(|(_, t)| t).collect()
}

fn active_mode(app: &mut App) -> Option<Modes> {
    ctx(app).modes.active_id()
}

#[test]
fn handshake() {
    let (mut app, backend) = start(Variant::Normal);
    let calls = backend.take_calls();
    // 15 note inputs on Port 1 (MIDI channels 1-15), none on CTRL
    let filters: Vec<&Vec<String>> = calls.iter().filter_map(|c| if let BackendCall::NoteInputFilters(IN, _, f) = c { Some(f) } else { None }).collect();
    assert_eq!(filters.len(), 15, "{calls:?}");
    assert_eq!(filters[0], &["80", "90", "A0", "B0", "C0", "D0", "E0"]);
    assert!(filters[14].contains(&"9E".to_string()));
    assert!(!calls.iter().any(|c| matches!(c, BackendCall::NoteInputFilters(CTRL_IN, ..))));
    assert!(calls.contains(&BackendCall::OpenInput(CTRL_IN)) && calls.contains(&BackendCall::OpenOutput(CTRL_OUT)));
    // The device information request (startup) and the logger setting (an observer)
    assert_eq!(sent(&calls, CTRL_OUT), [sysex(&[0x02, 0x7F], &[]), sysex(&[0x7F, 0x7D], &[0, 0])]);
    assert_eq!(active_mode(&mut app), None);

    // The information: subscribe to pages, pots and buttons, ask for the presets
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], INFO_MK2.as_bytes()));
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x14, 0x79], &[0x39]), sysex(&[0x02, 0x04], &[])]);
    assert_eq!(electra::state(ctx(&mut app)).electra_one_type, electra::ElectraOneType::Mk2);

    // The preset list: select the DrivenByMoss preset, remember the others
    app.on_midi_event(CTRL_IN, &preset_list("DrivenByMoss"));
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x08], &[1, 3])]);
    let state = electra::state(ctx(&mut app));
    assert_eq!((state.bank_index, state.preset_index), (1, 3));
    assert_eq!(state.preset("ReaSynth"), Some((0, 0)));
    assert_eq!(state.preset("No bank"), None);
    assert!(!electra::is_online(ctx(&mut app)));

    // The device switched to it: online, the mixer page
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 3]));
    assert!(electra::is_online(ctx(&mut app)));
    assert_eq!(active_mode(&mut app), Some(Modes::Volume));
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x0A], &[0])]);

    // Another preset: offline, the Dummy mode; back: the page of the last mode
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[4]));
    assert_eq!(active_mode(&mut app), Some(Modes::Transport));
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x02], &[0, 0]));
    assert!(!electra::is_online(ctx(&mut app)));
    assert_eq!(active_mode(&mut app), Some(Modes::Dummy));
    // A page switch is ignored while offline
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[1]));
    assert_eq!(active_mode(&mut app), Some(Modes::Dummy));
    // Going offline again keeps the mode (Java remembers DUMMY and selects the page -1)
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x02], &[0, 1]));
    backend.take_calls();
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 3]));
    assert_eq!(active_mode(&mut app), Some(Modes::Volume));
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x0A], &[4])]);
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[4]));
    assert_eq!(active_mode(&mut app), Some(Modes::Transport));
}

#[test]
fn handshake_failures() {
    // An old firmware: no subscription
    let (mut app, backend) = start(Variant::Normal);
    backend.take_calls();
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], br#"{"versionText":"v3.4.2","versionSeq":300400200}"#));
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    assert!(backend.logs.borrow().iter().any(|l| l.contains("Firmware must be at least 3.4.3 but is v3.4.2")), "{:?}", backend.logs.borrow());
    // The Mini is no Mk1/Mk2
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], INFO_MINI.as_bytes()));
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    // Broken JSON
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], b"{\"versionSeq\":"));
    assert!(backend.logs.borrow().iter().any(|l| l.contains("Could not parse JSON information")));
    // A Mk1 has no model
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], INFO_MK1.as_bytes()));
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT).len(), 2);
    assert_eq!(electra::state(ctx(&mut app)).electra_one_type, electra::ElectraOneType::Mk1);
    // No DrivenByMoss preset: nothing selected
    app.on_midi_event(CTRL_IN, &preset_list("Something else"));
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    assert!(backend.logs.borrow().iter().any(|l| l.contains("The DrivenByMoss template is not installed on the Electra.One.")));
    // Not the Electra's manufacturer or unknown commands: ignored
    app.on_midi_event(CTRL_IN, &[0xF0, 0x00, 0x20, 0x29, 0x01, 0x04, 0xF7]);
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x01], &[]));
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
}

#[test]
fn sysex_chunks_are_joined() {
    let (mut app, backend) = start(Variant::Normal);
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], INFO_MK2.as_bytes()));
    backend.take_calls();
    let list = preset_list("DrivenByMoss");
    let (first, second) = list.split_at(40);
    let (second, third) = second.split_at(30);
    app.on_midi_event(CTRL_IN, first);
    app.on_midi_event(CTRL_IN, second);
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    app.on_midi_event(CTRL_IN, third);
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x08], &[1, 3])]);
    // Short messages on CTRL are ignored, also between the chunks
    app.on_midi_event(CTRL_IN, first);
    app.on_midi_event(CTRL_IN, &[0xB0, 1, 2]);
    app.on_midi_event(CTRL_IN, &[0xFE]);
    app.on_midi_event(CTRL_IN, &list[40..]);
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x08], &[1, 3])]);
    // A new message before the end of the last one: the chunks are dropped
    app.on_midi_event(CTRL_IN, first);
    app.on_midi_event(CTRL_IN, &list);
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x08], &[1, 3])]);
    assert!(backend.logs.borrow().iter().any(|l| l.contains("Unsound sysex message without ending F7 received.")));
}

#[test]
fn mixer_page_updates() {
    let (mut app, backend, mut t) = start_online(Variant::Normal);
    backend.take_calls();
    advance(&mut app, &mut t, 30);
    let calls = backend.take_calls();
    let ctrl = ctrl_messages(&calls);
    // The flush is framed by a repaint pause
    assert_eq!(ctrl.first().unwrap().0, [0x7F, 0x7A, 0x00, 0x00]);
    assert_eq!(ctrl.last().unwrap().0, [0x7F, 0x7A, 0x01, 0x00]);
    assert_eq!(ctrl.iter().filter(|(c, _)| c[..2] == [0x7F, 0x7A]).count(), 2);

    // The values: CCs on MIDI channel 16 to Port 1, CC 10 + 10 * row + column
    let out = sent(&calls, OUT);
    assert!(out.contains(&vec![0xBF, 10, 64]), "{out:?}");
    assert!(out.contains(&vec![0xBF, 11, 0]));
    assert!(out.iter().all(|d| d[0] == 0xBF && (10..=65).contains(&d[1])), "only the controls' values: {out:?}");

    // The names, colors and visibilities (control ID = page * 36 + 6 * row + column + 1)
    let updates = control_updates(&calls);
    assert!(updates.contains(&(1, r#"{"name":"VOLUME","color":"FF0000","visible":true}"#.to_string())), "{updates:?}");
    assert!(updates.contains(&(7, r#"{"name":"PAN","color":"FF0000","visible":true}"#.to_string())));
    assert!(updates.contains(&(20, r#"{"name":"MUTE","color":"FF5000","visible":true}"#.to_string())), "track 2 is muted: {updates:?}");
    assert!(updates.contains(&(31, r#"{"name":"SELECT","color":"00FFFF","visible":true}"#.to_string())));
    // (the full flat track list has the master as the 4th track)
    assert!(updates.contains(&(5, r#"{"name":" ","color":"000000","visible":false}"#.to_string())), "track 5 does not exist: {updates:?}");
    // The master column: the color of the master, record and play
    assert!(updates.iter().any(|(id, json)| *id == 30 && json.contains("\"color\":\"800000\"")), "{updates:?}");
    assert!(updates.iter().any(|(id, json)| *id == 36 && json.contains("\"color\":\"008000\"")));

    // The value texts
    let texts = value_texts(&calls);
    assert!(texts.contains(&(1, "0 dB".to_string())), "{texts:?}");
    assert!(texts.contains(&(7, "C".to_string())));
    assert!(texts.contains(&(2, "-6 dB".to_string())));
    assert!(texts.contains(&(5, " ".to_string())));

    // The group labels with the Lua function of the preset (non-ASCII replaced)
    let calls_lua = lua(&calls);
    assert!(calls_lua.contains(&r#"sgt(501,"1: Drums")"#.to_string()), "{calls_lua:?}");
    assert!(calls_lua.contains(&r#"sgt(502,"2: Baess")"#.to_string()), "{calls_lua:?}");
    assert!(calls_lua.contains(&r#"sgt(505,"")"#.to_string()));

    // Nothing changed: nothing is sent
    advance(&mut app, &mut t, 30);
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());

    // A change: only that control
    app.update_model("track/0/mute 1\n");
    advance(&mut app, &mut t, 30);
    let calls = backend.take_calls();
    assert_eq!(control_updates(&calls), [(19, r#"{"color":"FF5000"}"#.to_string())]);
    assert!(value_texts(&calls).is_empty() && lua(&calls).is_empty());

    // After about 10 flushes, the beat and position texts of the play position control
    for _ in 0..10 {
        advance(&mut app, &mut t, 30);
    }
    let calls = backend.take_calls();
    assert!(value_texts(&calls).iter().any(|(id, _)| *id == 12), "{:?}", value_texts(&calls));
}

#[test]
fn a_rebind_does_not_send_the_page_again() {
    let (mut app, backend, mut t) = start_online(Variant::Normal);
    advance(&mut app, &mut t, 30);
    backend.take_calls();
    // A bank size report re-binds the knobs (re-activates the active mode)
    app.update_model("track/count 3\n");
    advance(&mut app, &mut t, 30);
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    // A mode switch sends the page completely
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[1]));
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[0]));
    advance(&mut app, &mut t, 30);
    assert!(control_updates(&backend.take_calls()).len() > 20);
}

#[test]
fn knobs_and_touches() {
    let (mut app, backend, _) = start_online(Variant::Normal);
    backend.take_calls();
    // Knob row 1, column 1: the volume of track 1, relative (two's complement) on channel 16
    app.on_midi_event(IN, &[0xBF, 10, 0x01]);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { p: 0, .. }, op: ValueOp::Set(_) | ValueOp::Inc(_) })), "{calls:?}");
    // Column 6: the master volume
    app.on_midi_event(IN, &[0xBF, 15, 0x7F]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Master, .. })));
    // Row 2, column 2: the panorama of track 2
    app.on_midi_event(IN, &[0xBF, 21, 0x01]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelPan { ch: Chan::Track { p: 1, .. }, .. })));
    // Row 2, column 6: the play position
    app.on_midi_event(IN, &[0xBF, 25, 0x01]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::BeatsPosition { .. })));
    // Row 3: no parameters on the mixer page
    app.on_midi_event(IN, &[0xBF, 30, 0x01]);
    assert!(backend.take_calls().commands().next().is_none());

    // A knob touch (pot 0, control 1): the volume of track 1 is touched
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[0, 1, 0, 1]));
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { p: 0, .. }, op: ValueOp::Touch(true) })));
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[0, 1, 0, 0]));
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { p: 0, .. }, op: ValueOp::Touch(false) })));
    // A control of the 2nd page (control 37 + 6 = the panorama of track 1 on the sends page is
    // the index 6 on any page)
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[3, 43, 0, 1]));
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelPan { ch: Chan::Track { p: 0, .. }, op: ValueOp::Touch(true) })));
    // A pot outside of 0-11
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[12, 1, 0, 1]));
    assert!(backend.logs.borrow().iter().any(|l| l.contains("Touch event with knob ID outside of range: 12")));
    assert_eq!(electra::state(ctx(&mut app)).knob_states[3], 1);
}

#[test]
fn knob_touch_combinations() {
    let (mut app, backend, mut t) = start_online(Variant::Normal);
    backend.take_calls();
    let touch = |app: &mut App, pot: u8, touched: u8| app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[pot, 1, 0, touched]));
    // Linear 2 - 4: the sends page
    touch(&mut app, 1, 1);
    touch(&mut app, 2, 1);
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    touch(&mut app, 3, 1);
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x0A], &[1])]);
    // The states are cleared after a match
    assert_eq!(electra::state(ctx(&mut app)).knob_states, [0; 12]);

    // Triangle 2, 7, 9: toggle play (a footswitch function)
    touch(&mut app, 1, 1);
    touch(&mut app, 6, 1);
    touch(&mut app, 8, 1);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::TransportPlay {} | Cmd::PlayToggle {})));
    advance(&mut app, &mut t, 30);

    // Corner Left 1, 2, 7 is Shift: as in Java the Shift patterns are not matched
    for pot in [0, 1, 6] {
        touch(&mut app, pot, 1);
    }
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(electra::state(ctx(&mut app)).knob_states.iter().sum::<i32>(), 3);

    // Linear 1 - 3 with an action
    let c = ctx(&mut app);
    c.config.set_value(config::assignable_setting(0), "Action");
    c.config.set_value(config::assignable_action_setting(0), "my_action");
    advance(&mut app, &mut t, 30);
    for pot in [0, 1, 6] {
        touch(&mut app, pot, 0);
    }
    backend.take_calls();
    for pot in [0, 1, 2] {
        touch(&mut app, pot, 1);
    }
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::Action { id: "my_action" })));

    // Linear 10 - 12 is "Select Synth Preset": the preset with the name of the cursor device
    for pot in [9, 10, 11] {
        touch(&mut app, pot, 1);
    }
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x08], &[0, 0])]);
    // Without such a preset: the device page
    app.update_model("device/name Other\n");
    for pot in [9, 10, 11] {
        touch(&mut app, pot, 1);
    }
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x0A], &[2])]);
}

#[test]
fn pads() {
    let (mut app, backend, mut t) = start_online(Variant::Normal);
    // Mixer: row 4 is mute, CC 70 + 10 * row + column
    backend.take_calls();
    app.on_midi_event(IN, &[0xBF, 101, 127]);
    app.on_midi_event(IN, &[0xBF, 101, 0]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelMute { ch: Chan::Track { p: 1, .. }, value: false })));
    // Row 6, column 6: play
    app.on_midi_event(IN, &[0xBF, 125, 127]);
    app.on_midi_event(IN, &[0xBF, 125, 0]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::TransportPlay {} | Cmd::PlayToggle {})));
    // Row 3, column 6: the next track page
    app.on_midi_event(IN, &[0xBF, 95, 127]);
    app.on_midi_event(IN, &[0xBF, 95, 0]);
    advance(&mut app, &mut t, 30);

    // Transport page: row 6, column 1 is undo
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[4]));
    backend.take_calls();
    app.on_midi_event(IN, &[0xBF, 120, 127]);
    app.on_midi_event(IN, &[0xBF, 120, 0]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::Undo {})));
    // The tempo knob (row 3, column 6)
    app.on_midi_event(IN, &[0xBF, 35, 0x01]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::TempoStep { .. })));

    // The pads have no light; offline nothing is sent to them
    advance(&mut app, &mut t, 30);
    let out = sent(&backend.take_calls(), OUT);
    assert!(out.iter().all(|d| d[1] < 70), "{out:?}");
}

#[test]
fn pages_and_modes() {
    let (mut app, backend, mut t) = start_online(Variant::Normal);
    let modes = [Modes::Volume, Modes::Send, Modes::DeviceParams, Modes::EqDeviceParams, Modes::Transport, Modes::Session, Modes::Project];
    for (page, mode) in modes.iter().enumerate() {
        app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[page as u8]));
        assert_eq!(active_mode(&mut app), Some(*mode));
        backend.take_calls();
        advance(&mut app, &mut t, 30);
        let calls = backend.take_calls();
        // Every page is sent completely with its control IDs (page * 36 + 1 ..)
        let ids: Vec<i32> = control_updates(&calls).iter().map(|(id, _)| *id).collect();
        assert!(!ids.is_empty(), "page {page}");
        let first = page as i32 * 36 + 1;
        assert!(ids.iter().all(|id| (first..first + 36).contains(id)), "page {page}: {ids:?}");
    }
    // The page 7 does not exist
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[7]));
    assert_eq!(active_mode(&mut app), Some(Modes::Project));

    // The device page shows the parameter name and value
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[2]));
    backend.take_calls();
    advance(&mut app, &mut t, 30);
    let calls = backend.take_calls();
    assert!(control_updates(&calls).contains(&(74, r#"{"name":"Cutoff","color":"FF5000","visible":true}"#.to_string())), "{:?}", control_updates(&calls));
    assert!(value_texts(&calls).contains(&(74, "50 %".to_string())));
    // The session page: the scene names in the groups
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[5]));
    backend.take_calls();
    advance(&mut app, &mut t, 30);
    let calls = backend.take_calls();
    assert!(lua(&calls).contains(&r#"sgt(550,"1: Verse")"#.to_string()), "{:?}", lua(&calls));
    assert!(control_updates(&calls).iter().any(|(id, json)| *id == 187 && json.contains("\"name\":\"Loop\"")), "{:?}", control_updates(&calls));
}

#[test]
fn logging() {
    let (mut app, backend, mut t) = start_online(Variant::Normal);
    // Logging off: the log lines of the device are dropped
    app.on_midi_event(CTRL_IN, &sysex(&[0x7F, 0x00], b"lua: hello"));
    assert!(!backend.logs.borrow().iter().any(|l| l.contains("lua: hello")));
    ctx(&mut app).config.set_value(config::LOG_TO_CONSOLE, "On");
    backend.take_calls();
    advance(&mut app, &mut t, 30);
    assert!(sent(&backend.take_calls(), CTRL_OUT).contains(&sysex(&[0x7F, 0x7D], &[1, 0])));
    app.on_midi_event(CTRL_IN, &sysex(&[0x7F, 0x00], b"lua: hello"));
    assert!(backend.logs.borrow().iter().any(|l| l.contains("lua: hello")));
}

#[test]
fn notes_go_to_the_note_inputs() {
    let (mut app, backend, _) = start_online(Variant::Normal);
    backend.take_calls();
    // A note on channel 1 is not a control
    app.on_midi_event(IN, &[0x90, 60, 100]);
    assert!(backend.take_calls().commands().next().is_none());
}

#[test]
fn mini() {
    let (mut app, backend) = start(Variant::Mini);
    assert!(sent(&backend.take_calls(), CTRL_OUT).contains(&sysex(&[0x02, 0x7F], &[])));
    // A Mk2 is no Mini, an old firmware neither
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], INFO_MK2.as_bytes()));
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], br#"{"versionText":"v5.0.3","versionSeq":500000300,"model":"mini"}"#));
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    app.on_midi_event(CTRL_IN, &sysex(&[0x01, 0x7F], INFO_MINI.as_bytes()));
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x14, 0x79], &[0x39]), sysex(&[0x02, 0x04], &[])]);
    // The Mini preset, not the other one
    app.on_midi_event(CTRL_IN, &preset_list("DrivenByMoss"));
    assert!(sent(&backend.take_calls(), CTRL_OUT).is_empty());
    app.on_midi_event(CTRL_IN, &preset_list("DrivenByMoss Mini"));
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x08], &[1, 3])]);
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 3]));
    assert_eq!(active_mode(&mut app), Some(Modes::Volume));
    let mut t = 1100;
    backend.take_calls();
    advance(&mut app, &mut t, 30);
    let calls = backend.take_calls();
    // 8 knobs in row 1 (CC 10-17), the control IDs 1-8, the group labels 100..
    let updates = control_updates(&calls);
    assert!(updates.contains(&(1, r#"{"name":"VOLUME","color":"FF0000","visible":true}"#.to_string())), "{updates:?}");
    assert!(updates.contains(&(9, r#"{"name":"VOLUME","color":"0000FF","visible":true}"#.to_string())), "{updates:?}");
    assert!(updates.contains(&(10, r#"{"name":"MUTE","color":"FF5000","visible":true}"#.to_string())));
    assert!(sent(&calls, OUT).contains(&vec![0xBF, 10, 64]));
    assert!(lua(&calls).contains(&r#"sgt(100,"1: Drums")"#.to_string()), "{:?}", lua(&calls));

    // The knobs and their pushes (mute in the push mode MUTE)
    app.on_midi_event(IN, &[0xBF, 11, 0x01]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { p: 1, .. }, .. })));
    app.on_midi_event(IN, &[0xBF, 20, 127]);
    app.on_midi_event(IN, &[0xBF, 20, 0]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelMute { ch: Chan::Track { p: 0, .. }, value: true })));
    // Button 1: panorama on the knobs
    app.on_midi_event(IN, &[0xBF, 30, 127]);
    app.on_midi_event(IN, &[0xBF, 30, 0]);
    app.on_midi_event(IN, &[0xBF, 10, 0x01]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelPan { ch: Chan::Track { p: 0, .. }, .. })));
    // Button 4 is Shift: Shift + button 2 selects the rec arm push mode
    app.on_midi_event(IN, &[0xBF, 33, 127]);
    assert!(ctx(&mut app).surface.is_shift_pressed());
    app.on_midi_event(IN, &[0xBF, 31, 127]);
    app.on_midi_event(IN, &[0xBF, 31, 0]);
    app.on_midi_event(IN, &[0xBF, 33, 0]);
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    app.on_midi_event(IN, &[0xBF, 22, 127]);
    app.on_midi_event(IN, &[0xBF, 22, 0]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::TrackRecArm { ch: Chan::Track { p: 2, .. }, .. })));

    // The pages: device and transport
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[1]));
    assert_eq!(active_mode(&mut app), Some(Modes::DeviceParams));
    backend.take_calls();
    advance(&mut app, &mut t, 30);
    let calls = backend.take_calls();
    assert!(control_updates(&calls).contains(&(17, r#"{"name":"Page 1: Cutoff","color":"FF5000","visible":true}"#.to_string())), "{:?}", control_updates(&calls));
    assert!(lua(&calls).iter().any(|l| l.starts_with("sgt(108,")), "{:?}", lua(&calls));
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x06], &[2]));
    assert_eq!(active_mode(&mut app), Some(Modes::Transport));
    backend.take_calls();
    advance(&mut app, &mut t, 30);
    assert!(control_updates(&backend.take_calls()).contains(&(43, r#"{"name":"PLAY","color":"00FF00","visible":true}"#.to_string())));
    // Button 3: play
    app.on_midi_event(IN, &[0xBF, 32, 127]);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::TransportPlay {} | Cmd::PlayToggle {})));
    app.on_midi_event(IN, &[0xBF, 32, 0]);

    // Knob touch combinations: Linear 6 - 8 is the transport page, Linear 2 - 4 the device page
    for pot in [1, 2, 3] {
        app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[pot, 1, 0, 1]));
    }
    assert_eq!(sent(&backend.take_calls(), CTRL_OUT), [sysex(&[0x09, 0x0A], &[1])]);
    // The pots 9-12 of the Mini do not exist (Java throws)
    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[9, 1, 0, 1]));
    assert_eq!(electra::state(ctx(&mut app)).knob_states, [0; 8]);
    // As in Java, the footswitch functions of the Mini are shifted by 6: Triangle 2, 5, 7 says
    // "Toggle Play" but calls the footswitch function -6, which does not exist
    backend.take_calls();
    for pot in [1, 4, 6] {
        app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[pot, 1, 0, 1]));
    }
    assert!(!backend.take_calls().has_cmd(|c| matches!(c, Cmd::TransportPlay {} | Cmd::PlayToggle {})));
    assert!(backend.logs.borrow().iter().any(|l| l.contains("Unknown footswitch command called: -6")));
}

#[test]
fn touch_everything() {
    for variant in [Variant::Normal, Variant::Mini] {
        let (mut app, _backend, mut t) = start_online(variant);
        let modes = [Modes::Volume, Modes::Send, Modes::DeviceParams, Modes::EqDeviceParams, Modes::Transport, Modes::Session, Modes::Project, Modes::Dummy];
        for mode in modes {
            if !ctx(&mut app).modes.contains(mode) {
                continue;
            }
            ctx(&mut app).set_active_mode(mode);
            advance(&mut app, &mut t, 50);
            let buttons = ctx(&mut app).surface.button_ids();
            for id in buttons {
                for event in [ButtonEvent::Down, ButtonEvent::Long, ButtonEvent::Up] {
                    ctx(&mut app).trigger_button(id, event, if event == ButtonEvent::Up { 0.0 } else { 1.0 });
                }
                advance(&mut app, &mut t, 50);
            }
            let ids: Vec<ContinuousId> = drivenbymoss::controller::electra::definition::ElectraVariant::ALL
                .iter()
                .flat_map(|v| v.knob_ids())
                .collect();
            for id in ids {
                if let Some((_, channel, number)) = ctx(&mut app).surface.continuous(id).and_then(|c| c.binding) {
                    app.on_midi_event(IN, &[0xB0 + channel as u8, number as u8, 1]);
                    app.on_midi_event(IN, &[0xB0 + channel as u8, number as u8, 127]);
                }
            }
            for pot in 0..12u8 {
                for control in [1u8, 37, 73, 109, 145, 181, 217] {
                    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[pot, control, 0, 1]));
                    app.on_midi_event(CTRL_IN, &sysex(&[0x7E, 0x0A], &[pot, control, 0, 0]));
                }
            }
            advance(&mut app, &mut t, 50);
        }
    }
}
