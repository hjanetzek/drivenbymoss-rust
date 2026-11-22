// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Novation Launchkey Mk3 and Mini Mk3: the DAW mode handshake, the display, the
//! pads in the session and drum views, the pad mode select view, the knob and fader modes, the
//! buttons, the keyboard port and all views and modes.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, Processor, RecordedCalls, RecordingBackend};
use drivenbymoss::controller::launchkey::surface as lk;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;
use drivenbymoss::protocol::{Arg, Chan, Cmd, SceneAction, SlotAction};

/// The DAW port.
const IN: i32 = 3;
/// The keyboard port.
const KEYS: i32 = 4;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volume/str 0 dB\ntrack/0/pan/str C\n\
track/1/exists 1\ntrack/1/name Bass\ntrack/1/volume/str -6 dB\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\ntrack/0/slot/1/exists 1\ntrack/1/slot/count 2\ntrack/1/slot/0/exists 1\n\
scene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\nplay 0\n";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Model {
    Mk3,
    Mini,
}

impl Model {
    fn class(self) -> &'static str {
        match self {
            Model::Mk3 => "LaunchkeyMk3ControllerInstance",
            Model::Mini => "LaunchkeyMiniMk3ControllerInstance",
        }
    }

    /// The names of the DAW input, the keyboard input and the DAW output.
    fn ports(self) -> [&'static str; 3] {
        match self {
            Model::Mk3 => ["Launchkey MK3 49 LKMK3 DAW Out", "Launchkey MK3 49 LKMK3 MIDI Out", "Launchkey MK3 49 LKMK3 DAW In"],
            Model::Mini => ["Launchkey Mini MK3 MIDI 2", "Launchkey Mini MK3 MIDI 1", "Launchkey Mini MK3 MIDI 2"],
        }
    }
}

/// Start an instance (detecting the ports) and run the delayed startup (at 1100ms).
fn start(model: Model) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-launchkey-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchkey.{}\n", model.class())).unwrap();
    let [daw_in, keys_in, daw_out] = model.ports();
    let backend = Rc::new(RecordingBackend {
        inputs: vec![(IN, daw_in.into()), (KEYS, keys_in.into())],
        outputs: vec![(OUT, daw_out.into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    assert_eq!(app.instance_count(), 1);
    app.set_time_ms(0);
    app.start_infrastructure();
    app.update_model(TRACKS);
    app.set_time_ms(1100);
    app.tick();
    (app, backend)
}

/// Start the Mk3 and connect it (the device answers the DAW mode note), until it is ready.
fn start_connected_mk3() -> (App, Rc<RecordingBackend>) {
    let (mut app, backend) = start(Model::Mk3);
    app.set_time_ms(1350);
    app.tick();
    // The device confirms the DAW mode
    app.on_midi_event(IN, &[0x9F, 0x0C, 0x7F]);
    for t in [1600, 1850, 2100] {
        app.set_time_ms(t);
        app.tick();
    }
    assert!(lk::state(ctx(&mut app)).is_ready);
    (app, backend)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).expect("instance runs")
}

/// Advance the time of the application and tick.
fn advance(app: &mut App, time_ms: &mut u64, ms: u64) {
    *time_ms += ms;
    app.set_time_ms(*time_ms);
    app.tick();
}

fn note(app: &mut App, channel: u8, n: u8, velocity: u8) {
    app.on_midi_event(IN, &[0x90 + channel, n, velocity]);
}

fn cc(app: &mut App, channel: u8, control: u8, value: u8) {
    app.on_midi_event(IN, &[0xB0 + channel, control, value]);
}

/// All short MIDI messages sent to the device.
fn midi_out(calls: &[BackendCall]) -> Vec<Vec<u8>> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(OUT, d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

/// The last value sent with the given status and data1.
fn last_out(calls: &[BackendCall], status: u8, data1: u8) -> Option<u8> {
    midi_out(calls).iter().rev().find_map(|d| (d.len() == 3 && d[0] == status && d[1] == data1).then_some(d[2]))
}

/// The texts of the display rows sent to the device (row index, text).
fn display_rows(calls: &[BackendCall]) -> Vec<(usize, String)> {
    midi_out(calls)
        .iter()
        .filter(|d| d.len() > 8 && d[0] == 0xF0 && d[1..6] == [0x00, 0x20, 0x29, 0x02, 0x0F])
        .map(|d| {
            let row = match d[6] {
                0x04 => d[7] as usize,
                kind => {
                    let id = d[7] as usize;
                    let base = if id >= 80 { 18 + (id - 80) * 2 } else { 2 + (id - 56) * 2 };
                    base + usize::from(kind == 0x08)
                }
            };
            (row, String::from_utf8_lossy(&d[8..d.len() - 1]).to_string())
        })
        .collect()
}

fn row_text(calls: &[BackendCall], row: usize) -> Option<String> {
    display_rows(calls).into_iter().rev().find(|(r, _)| *r == row).map(|(_, t)| t)
}

/// Press and release a CC button on MIDI channel 16 of the DAW port.
fn press_cc(app: &mut App, control: u8) {
    cc(app, 15, control, 127);
    cc(app, 15, control, 0);
}

#[test]
fn mk3_daw_mode_handshake() {
    let (mut app, backend) = start(Model::Mk3);
    let calls = backend.take_calls();
    // The keyboard port is opened, both ports have note inputs
    assert!(calls.contains(&BackendCall::OpenInput(KEYS)), "{calls:?}");
    assert!(calls.iter().any(|c| matches!(c, BackendCall::NoteInputFilters(KEYS, 0, f) if f.contains(&"E0".to_string()) && f.contains(&"B040".to_string()))));
    assert!(calls.iter().any(|c| matches!(c, BackendCall::NoteInputFilters(IN, 0, f) if f.contains(&"9E".to_string()) && !f.contains(&"9F".to_string()))));
    // Device inquiry, knob mode volume and pad mode session
    let out = midi_out(&calls);
    assert!(out.contains(&vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]));
    assert!(out.contains(&vec![0xBF, 0x09, 1]) && out.contains(&vec![0xBF, 0x03, 2]), "{out:?}");
    // Nothing is flushed before the device is connected
    assert!(!out.iter().any(|d| d[0] == 0x90), "{out:?}");

    // DAW mode on after 200ms
    let mut t = 1100;
    advance(&mut app, &mut t, 250);
    let out = midi_out(&backend.take_calls());
    assert!(out.contains(&vec![0x9F, 0x0C, 0x7F]), "{out:?}");
    // Still waiting
    advance(&mut app, &mut t, 250);
    assert!(midi_out(&backend.take_calls()).is_empty());

    // The device confirms, the modes are synchronized, ready after 200ms more
    app.on_midi_event(IN, &[0x9F, 0x0C, 0x7F]);
    assert!(lk::is_daw_connected(ctx(&mut app)));
    advance(&mut app, &mut t, 250);
    let out = midi_out(&backend.take_calls());
    assert_eq!(out, vec![vec![0xBF, 0x09, 3], vec![0xBF, 0x0A, 1], vec![0xBF, 0x03, 2]]);
    advance(&mut app, &mut t, 250);
    advance(&mut app, &mut t, 50);
    let calls = backend.take_calls();
    // The pads (session mapping), Play (off) and the DAW online light. The first slot has the
    // grey (51, 51, 51): not Bitwig (App::new), so the nearest entry of the palette (0x40
    // grey, 117), not the DAW color table's dark grey 1
    assert_eq!(last_out(&calls, 0x90, 96), Some(117), "{calls:?}");
    assert_eq!(last_out(&calls, 0xBF, 0x73), Some(1));
    assert_eq!(last_out(&calls, 0x9F, 0x0C), Some(127));

    // Exit leaves the DAW mode
    app.shutdown();
    assert_eq!(last_out(&backend.take_calls(), 0x9F, 0x0C), Some(0));
}

#[test]
fn mk3_display() {
    let (mut app, backend) = start_connected_mk3();
    let calls = backend.take_calls();
    assert!(row_text(&calls, 0).unwrap().starts_with("Pan "), "{:?}", display_rows(&calls));
    assert_eq!(row_text(&calls, 1).unwrap().trim_end(), "1: Drums");
    assert_eq!(row_text(&calls, 2).unwrap().trim_end(), "1: Drums");
    assert_eq!(row_text(&calls, 3).unwrap().trim_end(), "Pan: C");
    assert_eq!(row_text(&calls, 18).unwrap().trim_end(), "1: Drums");
    assert_eq!(row_text(&calls, 19).unwrap().trim_end(), "Vol: 0 dB");
    assert_eq!(row_text(&calls, 21).unwrap().trim_end(), "Vol: -6 dB");
    assert_eq!(row_text(&calls, 34).unwrap().trim_end(), "Master");

    // Device mode on the knobs (the device sends the selected knob mode)
    let mut t = 2100;
    cc(&mut app, 15, 0x09, 2);
    assert!(ctx(&mut app).modes.is_active(Modes::DeviceParams));
    advance(&mut app, &mut t, 1000);
    let calls = backend.take_calls();
    assert!(row_text(&calls, 0).unwrap().starts_with("ReaSynth"), "{:?}", display_rows(&calls));
    assert_eq!(row_text(&calls, 2).unwrap().trim_end(), "Cutoff");

    // Send 1 on the faders
    cc(&mut app, 15, 0x0A, 4);
    let manager = lk::fader_mode_manager(ctx(&mut app));
    assert!(ctx(&mut app).mode_manager(manager).is_active(Modes::Send1));
    advance(&mut app, &mut t, 1000);
    let calls = backend.take_calls();
    assert!(row_text(&calls, 18).unwrap().contains("FX1"), "{:?}", display_rows(&calls));

    // A custom knob mode
    cc(&mut app, 15, 0x09, 6);
    assert!(ctx(&mut app).modes.is_active(Modes::DeviceLayerSend1));
    advance(&mut app, &mut t, 1000);
    let calls = backend.take_calls();
    assert_eq!(row_text(&calls, 0).unwrap().trim_end(), "Custom mode 1");

    // A notification is shown on the display (all rows)
    ctx(&mut app).notify("Hello");
    let calls = backend.take_calls();
    assert_eq!(row_text(&calls, 0).unwrap().trim_end(), "Hello");
    assert_eq!(display_rows(&calls).len(), 36);
}

#[test]
fn mk3_device_inquiry() {
    let (mut app, backend) = start_connected_mk3();
    backend.take_calls();
    // Launchkey 88: the display uses the device ID 0x12
    app.on_midi_event(IN, &[0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x20, 0x29, 64, 0x01, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0xF7]);
    assert!(lk::state(ctx(&mut app)).is_88_key_version.get());
    assert!(lk::has_faders(ctx(&mut app)));
    let mut t = 2100;
    advance(&mut app, &mut t, 100);
    let out = midi_out(&backend.take_calls());
    assert!(out.iter().any(|d| d.len() > 6 && d[0] == 0xF0 && d[5] == 0x12), "{out:?}");
    // Launchkey 25: no faders
    app.on_midi_event(IN, &[0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x20, 0x29, 52, 0x01, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0xF7]);
    assert!(!lk::has_faders(ctx(&mut app)));
}

#[test]
fn mk3_session_pads() {
    let (mut app, backend) = start_connected_mk3();
    backend.take_calls();
    // Upper left pad: the first clip of the first track
    note(&mut app, 0, 96, 127);
    note(&mut app, 0, 96, 0);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::SlotAction { track: 0, slot: 0, action: SlotAction::Launch | SlotAction::LaunchAlt | SlotAction::LaunchRelease | SlotAction::LaunchReleaseAlt })), "{calls:?}");

    // Stop + pad stops the track
    cc(&mut app, 15, 0x74, 127);
    note(&mut app, 0, 97, 127);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::TrackStop { ch: Chan::Track { bank: Processor::Track, p: 1 } })), "{calls:?}");
    note(&mut app, 0, 97, 0);
    cc(&mut app, 15, 0x74, 0);

    // Scene 2 (held) shows the pad mode select view
    let mut t = 2100;
    cc(&mut app, 0, 0x69, 127);
    assert!(ctx(&mut app).views.is_active(Views::Control));
    advance(&mut app, &mut t, 50);
    // Lower row: the second pad selects the rec arm mode
    note(&mut app, 0, 113, 127);
    note(&mut app, 0, 113, 0);
    assert_eq!(lk::state(ctx(&mut app)).session_pad_mode, Some(Modes::RecArm));
    // Upper row: the clip length
    note(&mut app, 0, 100, 127);
    assert_eq!(ctx(&mut app).config.new_clip_length(), 4);
    backend.take_calls();
    // Releasing the scene button restores the session view and does not launch the scene
    cc(&mut app, 0, 0x69, 0);
    assert!(ctx(&mut app).views.is_active(Views::Session));
    assert!(!backend.take_calls().has_cmd(|c| matches!(c, Cmd::SceneAction { scene: 1, action: SceneAction::Launch | SceneAction::LaunchAlt | SceneAction::LaunchRelease | SceneAction::LaunchReleaseAlt })));

    // The lower row now arms the tracks
    advance(&mut app, &mut t, 50);
    let calls = backend.take_calls();
    // Track 1 is not armed: red low; the second scene button shows the rec arm mode
    assert_eq!(last_out(&calls, 0x90, 112), Some(7), "{calls:?}");
    assert_eq!(last_out(&calls, 0xB0, 0x69), Some(5));
    note(&mut app, 0, 112, 127);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::TrackRecArm { ch: Chan::Track { bank: Processor::Track, p: 0 }, .. })), "{calls:?}");
}

#[test]
fn mk3_scene_buttons() {
    let (mut app, backend) = start_connected_mk3();
    backend.take_calls();
    cc(&mut app, 0, 0x68, 127);
    assert!(!backend.take_calls().has_cmd(|c| matches!(c, Cmd::SceneAction { scene: 0, action: SceneAction::Launch | SceneAction::LaunchAlt | SceneAction::LaunchRelease | SceneAction::LaunchReleaseAlt })));
    cc(&mut app, 0, 0x68, 0);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::SceneAction { scene: 0, .. } | Cmd::SceneName { scene: 0, .. } | Cmd::SceneColor { scene: 0, .. })), "{calls:?}");
    // The redundant messages of the arrow buttons on channel 16 are ignored
    cc(&mut app, 15, 0x68, 127);
    cc(&mut app, 15, 0x68, 0);
    assert!(!backend.take_calls().has_cmd(|c| matches!(c, Cmd::SceneAction { scene: 0, .. } | Cmd::SceneName { scene: 0, .. } | Cmd::SceneColor { scene: 0, .. })));
}

#[test]
fn mk3_buttons() {
    let (mut app, backend) = start_connected_mk3();
    backend.take_calls();
    // Play
    press_cc(&mut app, 0x73);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::PlayToggle {})));
    // Metronome
    press_cc(&mut app, 0x4C);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::TransportToggleMetronome {} | Cmd::TransportToggleMetronomeTicks {})), "{calls:?}");
    // Device lock: pin the device, with Shift open its window
    press_cc(&mut app, 0x34);
    assert!(ctx(&mut app).model.cursor_device().is_pinned());
    // Previous / next track
    press_cc(&mut app, 0x66);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelSelect { ch: Chan::Track { bank: Processor::Track, p: 1 }, .. })), "{calls:?}");

    // The buttons below the faders select tracks; with Undo they delete them
    cc(&mut app, 15, 0x26, 127);
    cc(&mut app, 15, 0x26, 0);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelSelect { ch: Chan::Track { bank: Processor::Track, p: 1 }, .. })));
    cc(&mut app, 15, 0x4D, 127);
    cc(&mut app, 15, 0x25, 127);
    cc(&mut app, 15, 0x25, 0);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelRemove { ch: Chan::Track { bank: Processor::Track, p: 0 } })), "{calls:?}");
    cc(&mut app, 15, 0x4D, 0);
    assert!(!backend.take_calls().has_cmd(|c| matches!(c, Cmd::Undo {})), "Undo is consumed");

    // Toggle to rec arm
    press_cc(&mut app, 0x2D);
    assert!(!lk::state(ctx(&mut app)).is_select);
    cc(&mut app, 15, 0x25, 127);
    cc(&mut app, 15, 0x25, 0);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::TrackRecArm { ch: Chan::Track { bank: Processor::Track, p: 0 }, .. })));
    let mut t = 2100;
    advance(&mut app, &mut t, 50);
    let calls = backend.take_calls();
    // Toggle light and the select buttons (not armed: grey low)
    assert_eq!(last_out(&calls, 0xBF, 0x2D), Some(127), "{calls:?}");
}

/// Shift + Play returns to the arrangement on a host with the transport commands (Bitwig), and
/// does nothing on the others (Reaper).
#[test]
fn mk3_shift_play_returns_to_arrangement() {
    let (mut app, backend) = start_connected_mk3();
    backend.take_calls();
    let shift_play = |app: &mut App| {
        cc(app, 0, 0x6C, 127);
        press_cc(app, 0x73);
        cc(app, 0, 0x6C, 0);
    };
    shift_play(&mut app);
    let calls = backend.take_calls();
    assert!(!calls.has_cmd(|c| matches!(c, Cmd::TransportReturnToArrangement {} | Cmd::PlayToggle {} | Cmd::TransportPlay {})), "{calls:?}");

    app.update_model("/host/features transportCommands\n");
    shift_play(&mut app);
    let calls = backend.take_calls();
    assert_eq!(calls.count_cmd(|c| matches!(c, Cmd::TransportReturnToArrangement {})), 1, "{calls:?}");
    assert!(!calls.has_cmd(|c| matches!(c, Cmd::TransportPlay {} | Cmd::PlayToggle {})), "{calls:?}");
    // The text form (Reaper's protocol)
    let legacy = Cmd::TransportReturnToArrangement {}.legacy();
    assert_eq!((legacy.processor, legacy.command.as_str(), legacy.arg), (Processor::Transport, "returnToArrangement", Arg::None));
}

#[test]
fn mk3_track_select_lights() {
    let (_app, backend) = start_connected_mk3();
    let calls = backend.take_calls();
    // The selected first track is sent on channel 3 (pulsing), the second on channel 1
    assert!(last_out(&calls, 0xB2, 0x25).is_some(), "{calls:?}");
    assert!(last_out(&calls, 0xB0, 0x26).is_some());
    assert_eq!(last_out(&calls, 0xB0, 0x28), Some(0));
}

#[test]
fn mk3_view_selection() {
    let (mut app, backend) = start_connected_mk3();
    backend.take_calls();
    let mut t = 2100;
    // The drum pad mode
    cc(&mut app, 15, 0x03, 1);
    assert!(ctx(&mut app).views.is_active(Views::Drum));
    assert_eq!(lk::state(ctx(&mut app)).pad_view, Some(Views::Drum));
    advance(&mut app, &mut t, 50);
    let calls = backend.take_calls();
    // The drum pads are lit on channel 10
    assert!(midi_out(&calls).iter().any(|d| d[0] == 0x99), "{calls:?}");
    // The drum pads play notes: the key translation maps them to the drum offset
    let table = ctx(&mut app).surface.key_translation_table().to_vec();
    assert_eq!(table[36], 36);
    assert_eq!(table[44], 40);

    // Scene 1 toggles the step sequencer
    cc(&mut app, 0, 0x68, 127);
    cc(&mut app, 0, 0x68, 0);
    assert_eq!(ctx(&mut app).surface.key_translation_table()[36], -1);

    // Device select mode
    cc(&mut app, 15, 0x03, 9);
    assert!(ctx(&mut app).views.is_active(Views::Device));
    // The first pad of the upper row selects the first parameter page
    backend.take_calls();
    note(&mut app, 0, 64, 127);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::DevicePageStep { .. } | Cmd::ParamValue { dev: Processor::Device, .. } | Cmd::ParamTouch { dev: Processor::Device, .. } | Cmd::ParamIndicate { dev: Processor::Device, .. } | Cmd::DeviceParameterPage { dev: Processor::Device, .. } | Cmd::DeviceToggleParameterPageSection { dev: Processor::Device })), "{calls:?}");

    // A custom pad mode
    cc(&mut app, 15, 0x03, 5);
    assert!(ctx(&mut app).views.is_active(Views::Dummy3));

    // The browser: opened with the navigation pad mode
    cc(&mut app, 15, 0x03, 2);
    backend.take_calls();
    cc(&mut app, 15, 0x03, 10);
    assert!(ctx(&mut app).views.is_active(Views::Browser));
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| *c == Cmd::enable_updates(Processor::Browser, true)), "{calls:?}");
    // Back to session: the pad mode is sent to the device
    cc(&mut app, 15, 0x03, 2);
    let calls = backend.take_calls();
    assert!(ctx(&mut app).views.is_active(Views::Session));
    assert_eq!(last_out(&calls, 0xBF, 0x03), Some(2), "{calls:?}");
}

#[test]
fn mk3_master_fader() {
    let (mut app, backend) = start_connected_mk3();
    backend.take_calls();
    cc(&mut app, 15, 0x3D, 100);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Master, .. })));
    // The faders are bound to the track volumes
    cc(&mut app, 15, 0x35, 100);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { bank: Processor::Track, p: 0 }, .. })));
    // The knobs to the panorama
    cc(&mut app, 15, 0x15, 100);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelPan { ch: Chan::Track { bank: Processor::Track, p: 0 }, .. })));
}

#[test]
fn mini_startup_and_modes() {
    let (mut app, backend) = start(Model::Mini);
    let calls = backend.take_calls();
    let out = midi_out(&calls);
    // DAW mode, knob mode volume, pad mode session; the surface is flushed at once
    assert!(out.contains(&vec![0x9F, 0x0C, 0x7F]), "{out:?}");
    assert!(out.contains(&vec![0xBF, 0x09, 1]) && out.contains(&vec![0xBF, 0x03, 2]));
    assert_eq!(last_out(&calls, 0x90, 96), Some(117), "{calls:?}");
    assert!(ctx(&mut app).text_display.is_none());

    // The knobs control the volume
    cc(&mut app, 15, 0x15, 100);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { bank: Processor::Track, p: 0 }, .. })));

    // Program change selects the next page of tracks
    app.on_midi_event(IN, &[0xC0, 5, 0]);
    app.on_midi_event(IN, &[0xC0, 6, 0]);
    assert!(lk::state(ctx(&mut app)).last_prg_change == 6);

    // The custom knob mode: the knobs send on the keyboard port on channel 1
    app.update_model("project/param/count 8\nproject/param/0/name Tempo\nproject/param/0/value 0.2\n");
    cc(&mut app, 15, 0x09, 6);
    assert!(ctx(&mut app).modes.is_active(Modes::User));
    backend.take_calls();
    app.on_midi_event(KEYS, &[0xB0, 0x15, 64]);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ParamValue { dev: Processor::Project, param: 0, .. })), "{calls:?}");
    // The same CC on the DAW port is a knob of the active mode, on another channel of the
    // keyboard port it is ignored
    app.on_midi_event(KEYS, &[0xB1, 0x15, 64]);
    assert!(!backend.take_calls().has_cmd(|c| matches!(c.legacy().arg, Arg::Int(_) | Arg::Double(_))));
}

#[test]
fn mini_session_and_pad_mode_select() {
    let (mut app, backend) = start(Model::Mini);
    backend.take_calls();
    let mut t = 1100;
    // Shift + scene 2 scrolls the scenes (a page without a pad mode)
    cc(&mut app, 0, 0x6C, 127);
    cc(&mut app, 0, 0x69, 127);
    cc(&mut app, 0, 0x69, 0);
    cc(&mut app, 0, 0x6C, 0);
    assert!(ctx(&mut app).views.is_active(Views::Session));

    // Scene 2 shows the pad mode select view, the upper row has the functions
    cc(&mut app, 0, 0x69, 127);
    assert!(ctx(&mut app).views.is_active(Views::Control));
    advance(&mut app, &mut t, 50);
    backend.take_calls();
    // Metronome
    note(&mut app, 0, 96, 127);
    // Undo
    note(&mut app, 0, 98, 127);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::Undo {})), "{calls:?}");
    cc(&mut app, 0, 0x69, 0);
    assert!(ctx(&mut app).views.is_active(Views::Session));

    // The drum view and its configuration
    cc(&mut app, 15, 0x03, 1);
    assert!(ctx(&mut app).views.is_active(Views::Drum));
    assert_eq!(lk::state(ctx(&mut app)).pad_view, Some(Views::Drum));
    // The custom pad mode
    cc(&mut app, 15, 0x03, 5);
    assert!(ctx(&mut app).views.is_active(Views::Play));
}

/// Activate all views and modes and trigger all buttons and continuous controls of both models:
/// no panics.
#[test]
fn touch_everything() {
    for model in [Model::Mk3, Model::Mini] {
        let (mut app, backend) = if model == Model::Mk3 { start_connected_mk3() } else { start(model) };
        let mut t = 2100;
        let views = [
            Views::Session,
            Views::Control,
            Views::Drum,
            Views::Play,
            Views::Shift,
            Views::Device,
            Views::Browser,
            Views::Dummy1,
            Views::Dummy2,
            Views::Dummy3,
            Views::Dummy4,
            Views::Dummy5,
            Views::Dummy6,
        ];
        let modes = [Modes::Volume, Modes::Pan, Modes::Send1, Modes::Send2, Modes::DeviceParams, Modes::User, Modes::DeviceLayerSend1, Modes::DeviceLayerSend4];
        for view in views {
            if !ctx(&mut app).views.contains(view) {
                continue;
            }
            ctx(&mut app).set_active_view(view);
            for mode in modes {
                if !ctx(&mut app).modes.contains(mode) {
                    continue;
                }
                ctx(&mut app).set_active_mode(mode);
                advance(&mut app, &mut t, 50);
            }
            let buttons = ctx(&mut app).surface.button_ids();
            for id in buttons {
                for event in [ButtonEvent::Down, ButtonEvent::Long, ButtonEvent::Up] {
                    ctx(&mut app).trigger_button(id, event, if event == ButtonEvent::Up { 0.0 } else { 1.0 });
                }
                advance(&mut app, &mut t, 50);
            }
            for i in 0..8 {
                for id in [ContinuousId::get(ContinuousId::Knob1, i), ContinuousId::get(ContinuousId::Fader1, i), ContinuousId::get(ContinuousId::DeviceKnob1, i)] {
                    if let Some(control) = ctx(&mut app).surface.continuous(id) {
                        if let Some((_, channel, number)) = control.binding {
                            app.on_midi_event(IN, &[0xB0 + channel as u8, number as u8, 64]);
                        }
                    }
                }
                app.on_midi_event(KEYS, &[0xB0, 0x15 + i as u8, 64]);
                app.on_midi_event(KEYS, &[0x90, 60, 100]);
            }
            for n in 36..52 {
                note(&mut app, 0, n, 127);
                note(&mut app, 0, n, 0);
            }
            advance(&mut app, &mut t, 50);
        }
        backend.take_calls();
        app.shutdown();
        let logs = backend.logs.borrow();
        let errors: Vec<&String> = logs.iter().filter(|l| l.contains("already in use") || l.contains("Recursive")).collect();
        assert!(errors.is_empty(), "{errors:?}");
    }
}

/// On a host with its own ports (Bitwig) the keyboard port is the second input (port 1).
#[test]
fn single_host_keyboard_port() {
    let dir = std::env::temp_dir().join(format!("dbm-test-launchkey-single-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (uuid, knob_channel) in [("10CB5692-541C-4A5D-9EB4-07D80F34A02C", None), ("5359D5B1-28CD-4457-B49D-F8D3D7BC52B9", Some(0u8))] {
        let backend = Rc::new(RecordingBackend::default());
        let mut app = App::new_single(backend.clone(), dir.to_str().unwrap(), uuid, "Bitwig Studio 6.1").expect("Launchkey");
        app.set_time_ms(0);
        app.start_infrastructure();
        let calls = backend.take_calls();
        assert!(calls.contains(&BackendCall::OpenInput(0)) && calls.contains(&BackendCall::OpenInput(1)), "{calls:?}");
        assert!(calls.iter().any(|c| matches!(c, BackendCall::NoteInputFilters(1, 0, f) if f.contains(&"E0".to_string()))), "{calls:?}");
        if let Some(channel) = knob_channel {
            app.update_model("project/param/count 8\nproject/param/0/name Tempo\n");
            app.set_time_ms(1100);
            app.tick();
            app.instance_ctx(0).unwrap().set_active_mode(Modes::User);
            backend.take_calls();
            app.on_midi_event(1, &[0xB0 + channel, 0x15, 100]);
            assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ParamValue { dev: Processor::Project, param: 0, .. })));
        }
        app.shutdown();
    }
}
