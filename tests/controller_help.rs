// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The help of the controls (doc/CONTROLLER_HELP_PLAN.md) on the Maschine JAM and the Mk2: the
//! changes since the last call, the texts in the states the modifiers and the views make, the
//! tables' coverage of every button in every view and mode.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::controller::maschine::jam::surface as jam;
use drivenbymoss::framework::help;
use drivenbymoss::framework::help_table::HelpKey;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::json::JsonValue;

const IN: i32 = 3;
const OUT: i32 = 5;
const SHIFT_DOWN: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x01, 0xF7];
const SHIFT_UP: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x00, 0xF7];

fn start() -> App {
    start_controller("maschine.jam.MaschineJamControllerInstance", "Maschine Jam - 1", "Maschine Jam - 1", None)
}

fn start_mk2() -> App {
    start_controller("maschine.mk3.MaschineMk2ControllerInstance", "Maschine MK2 In", "Maschine MK2 Out", None)
}

/// A Push (1, 2 or 3).
fn start_push(model: u8) -> App {
    let port = match model {
        1 => "Ableton Push User Port",
        2 => "Ableton Push 2 Live Port",
        _ => "Ableton Push 3 Live Port",
    };
    let class = format!("ableton.push.Push{model}ControllerInstance");
    let file = format!("DrivenByMoss4Reaper-Push {model}.config");
    start_controller(&class, port, port, Some((&file, &format!("MIDI_INPUT0={port}\nMIDI_OUTPUT0={port}\n"))))
}

/// `class`: below `de.mossgrabers.reaper.controller.ni.` / `.controller.` (the Push); `extra`: a
/// file of the controller's settings (name, content).
fn start_controller(class: &str, input: &str, output: &str, extra: Option<(&str, &str)>) -> App {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-help-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let package = if class.starts_with("ableton.") { "de.mossgrabers.reaper.controller" } else { "de.mossgrabers.reaper.controller.ni" };
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={package}.{class}\n")).unwrap();
    if let Some((name, content)) = extra {
        std::fs::write(dir.join(name), content).unwrap();
    }
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, input.into())], outputs: vec![(OUT, output.into())], ..Default::default() });
    let mut app = App::new(backend, dir.to_str().unwrap(), 7, 22);
    app.start_infrastructure();
    app.update_model("track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/1/exists 1\ntrack/1/name Bass\nscene/count 1\nscene/0/exists 1\nscene/0/name Verse\nplay 0\n");
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    app
}

fn cc(app: &mut App, control: i32, value: u8) {
    app.on_midi_event(IN, &[0xB0, control as u8, value]);
}

/// The changed entries by id.
fn take(app: &mut App) -> HashMap<String, JsonValue> {
    let text = app.take_control_help();
    text.lines()
        .map(|line| {
            let value = JsonValue::parse(line).unwrap_or_else(|e| panic!("{e}: {line}"));
            (value.get("id").unwrap().as_text(), value)
        })
        .collect()
}

fn short(entries: &HashMap<String, JsonValue>, id: &str) -> String {
    entries.get(id).unwrap_or_else(|| panic!("no entry {id}: {:?}", entries.keys())).get("short").unwrap().as_text()
}

/// All texts are short enough for 2 lines of a box.
fn check_lengths(app: &mut App, state: &str) {
    let ctx = app.instance_ctx(0).unwrap();
    for entry in help::collect(ctx) {
        if let Some(help) = entry.help {
            assert!(help.short.chars().count() <= 90 && !help.short.contains('\n'), "{state}: {}: {:?}", entry.id, help.short);
        }
    }
}

#[test]
fn the_first_call_has_all_then_only_changes() {
    let mut app = start();
    let entries = take(&mut app);
    assert_eq!(short(&entries, "Play"), "Start playback (double click: to zero)");
    let play = &entries["Play"];
    assert_eq!(play.get("bind").unwrap().as_text(), "cc");
    assert_eq!(play.get("control").unwrap().as_int(), jam::PLAY as i64);
    assert!(!play.get("long").unwrap().as_text().is_empty());
    // The JAM's Shift is NI's SysEx: no binding
    assert_eq!(entries["Shift"].get("control").unwrap().as_int(), -1);
    assert!(short(&entries, "grid").starts_with("Play C Major"), "{}", short(&entries, "grid"));
    assert_eq!(short(&entries, "strips"), "The volume of 8 tracks");
    // The group buttons and the scenes are one box each, the names in the long text
    assert!(short(&entries, "RowSelect1..RowSelect8").starts_with("Select the track"));
    assert!(entries["RowSelect1..RowSelect8"].get("long").unwrap().as_text().contains("A: Drums"));
    assert_eq!(entries["RowSelect1..RowSelect8"].get("members").unwrap().elements().len(), 8);
    assert_eq!(short(&entries, "Scene1..Scene8"), "Launch the scene");
    assert!(entries["Scene1..Scene8"].get("long").unwrap().as_text().contains("1: Verse"));
    assert!(!entries.contains_key("Scene1"), "no box of its own");
    // The scopes and the names of the view and the mode
    assert_eq!(entries["Play"].get("scope").unwrap().as_text(), "global");
    assert_eq!(entries["grid"].get("scope").unwrap().as_text(), "view");
    assert_eq!(entries["strips"].get("scope").unwrap().as_text(), "mode");
    assert_eq!(short(&entries, "@view"), "Play");
    assert_eq!(short(&entries, "@mode"), "Volume");
    assert!(short(&entries, "MasterKnob").contains("master volume"));
    assert!(take(&mut app).is_empty(), "nothing changed");

    // A reset hands out everything again
    app.reset_control_help();
    assert_eq!(take(&mut app).len(), entries.len());
}

#[test]
fn modifiers_change_the_texts() {
    let mut app = start();
    take(&mut app);
    app.on_midi_event(IN, &SHIFT_DOWN);
    app.tick();
    let entries = take(&mut app);
    assert_eq!(entries["Shift"].get("pressed").unwrap().as_text(), "true");
    assert_eq!(short(&entries, "Play"), "Restart playback from the beginning");
    assert_eq!(entries["Play"].get("scope").unwrap().as_text(), "modifier");
    assert!(!entries["Play"].get("long").unwrap().as_text().is_empty(), "the long text of the plain row");
    assert!(short(&entries, "Scene1..Scene8").contains("alternative launch"));
    assert!(short(&entries, "PageLeft").starts_with("Metronome"));
    check_lengths(&mut app, "Shift");
    app.on_midi_event(IN, &SHIFT_UP);
    app.tick();
    take(&mut app);

    cc(&mut app, jam::SELECT, 127);
    let entries = take(&mut app);
    assert!(short(&entries, "Play").starts_with("Punch in"));
    assert_eq!(short(&entries, "Track"), "Add an instrument track");
    assert!(short(&entries, "Session").starts_with("Page surface"));
    check_lengths(&mut app, "Select");
    cc(&mut app, jam::SELECT, 0);

    // Step held: the scene buttons pick the sequencers
    cc(&mut app, jam::STEP, 127);
    let entries = take(&mut app);
    assert_eq!(short(&entries, "Scene1..Scene8"), "1-6: the sequencers: Sequencer, Poly Seq., Raindrops, Drum, Drum 4, Drum 8");
    cc(&mut app, jam::STEP, 0);
}

#[test]
fn views_and_modes_describe_the_pads_and_strips() {
    let mut app = start();
    take(&mut app);
    // SONG: the session view
    cc(&mut app, jam::SONG, 127);
    cc(&mut app, jam::SONG, 0);
    app.tick();
    assert!(app.instance_ctx(0).unwrap().views.is_active(Views::Session));
    let entries = take(&mut app);
    assert!(short(&entries, "grid").starts_with("Launch"), "{}", short(&entries, "grid"));
    assert!(short(&entries, "ArrowRight").starts_with("Scroll the clips"));

    // AUX: the send mode on the strips
    cc(&mut app, jam::AUX, 127);
    cc(&mut app, jam::AUX, 0);
    app.tick();
    assert!(app.instance_ctx(0).unwrap().modes.is_active(Modes::Send1));
    let entries = take(&mut app);
    assert!(short(&entries, "strips").starts_with("Send 1"), "{}", short(&entries, "strips"));

    // Every view and mode of the JAM: the texts are short enough
    for view in VIEWS {
        let ctx = app.instance_ctx(0).unwrap();
        ctx.set_active_view(view);
        let has_grid = help::collect(ctx).iter().any(|e| e.id == "grid" && e.help.is_some());
        assert!(has_grid, "{view:?} describes its pads");
        check_lengths(&mut app, view.name());
    }
    for mode in [Modes::Track, Modes::Volume, Modes::Pan, Modes::Send2, Modes::DeviceParams, Modes::User] {
        let ctx = app.instance_ctx(0).unwrap();
        ctx.set_active_mode(mode);
        let has_strips = help::collect(ctx).iter().any(|e| e.id == "strips" && e.help.is_some());
        assert!(has_strips, "{mode:?} describes the strips");
        check_lengths(&mut app, mode.name());
    }
}

const VIEWS: [Views; 18] = [
    Views::Play, Views::Chords, Views::Piano, Views::Drum64, Views::Drum, Views::Drum4, Views::Drum8, Views::Sequencer, Views::PolySequencer,
    Views::Raindrops, Views::Session, Views::Shift, Views::RepeatNote, Views::Tempo, Views::Shuffle, Views::Browser, Views::Control, Views::SurfaceLink,
];
const MODES: [Modes; 15] = [
    Modes::Track, Modes::Volume, Modes::Pan, Modes::Send1, Modes::Send2, Modes::Send3, Modes::Send4, Modes::Send5, Modes::Send6, Modes::Send7,
    Modes::Send8, Modes::DeviceParams, Modes::User, Modes::HostStrips, Modes::SurfaceLink,
];

/// The table has a row (a text, `none` or `command`) for every button and continuous control
/// with a command in every view and mode of the JAM, and for the pads and the strips.
#[test]
fn the_table_covers_every_button() {
    let mut app = start();
    check_coverage(&mut app, &VIEWS, &MODES);
}

/// The table has a row for every button and continuous control with a command in every view and
/// mode of the JAM (or the Mk2), and for the pads and the mode's controls (strips, knobs).
fn check_coverage(app: &mut App, views: &[Views], modes: &[Modes]) {
    let ctx = app.instance_ctx(0).unwrap();
    let table = ctx.help_table.expect("the controller has a help table");
    let rows = table.button_rows();
    let buttons: Vec<ButtonId> = ctx.surface.button_ids().into_iter().filter(|id| ctx.surface.button(*id).unwrap().command().is_some()).collect();
    let continuous: Vec<HelpKey> = ContinuousId::ALL.iter().filter(|id| ctx.surface.continuous(**id).is_some_and(|c| c.command.is_some())).map(|id| HelpKey::Continuous(*id)).collect();
    let mut missing = Vec::new();
    for view in views {
        for mode in modes {
            for button in &buttons {
                let key = rows.iter().copied().find(|row| row.buttons().contains(button)).unwrap_or(HelpKey::Button(*button));
                if !table.has_entry(Some(*view), Some(*mode), key) {
                    missing.push(format!("{view:?}/{mode:?}: {button:?}"));
                }
            }
            for key in continuous.iter().copied().chain([HelpKey::Grid, HelpKey::Controls]) {
                if !table.has_entry(Some(*view), Some(*mode), key) {
                    missing.push(format!("{view:?}/{mode:?}: {key:?}"));
                }
            }
        }
    }
    missing.dedup();
    assert!(missing.is_empty(), "{} without a row, e.g. {:?}", missing.len(), &missing[..missing.len().min(10)]);
}

// ------------------------------------------------------------------------------------------
// The Maschine Mk2

const MK2_VIEWS: [Views; 7] = [Views::ScenePlay, Views::Session, Views::Drum, Views::Play, Views::Device, Views::RepeatNote, Views::Shift];
const MK2_MODES: [Modes; 21] = [
    Modes::Browser, Modes::Volume, Modes::Pan, Modes::Send1, Modes::Send2, Modes::Send3, Modes::Send4, Modes::Send5, Modes::Send6, Modes::Send7,
    Modes::Send8, Modes::Tempo, Modes::Position, Modes::LoopStart, Modes::LoopLength, Modes::RepeatNote, Modes::Scales, Modes::PlayOptions,
    Modes::Note, Modes::DeviceParams, Modes::User,
];

const MK2_STOP: i32 = 59;
const MK2_SELECT: i32 = 90;
const MK2_PAD_MODE: i32 = 81;
const MK2_STEP: i32 = 83;
const MK2_VOLUME: i32 = 44;
const MK2_SCENE: i32 = 85;
const MK2_ERASE: i32 = 54;
const MK2_HOST_SENDS_SHIFT: [u8; 8] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0x01, 0xF7];
const MK2_SHIFT_DOWN: [u8; 8] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x12, 0x01, 0xF7];
const MK2_SHIFT_UP: [u8; 8] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x12, 0x00, 0xF7];

#[test]
fn mk2_the_first_call_has_the_rows_knobs_and_pads() {
    let mut app = start_mk2();
    let entries = take(&mut app);
    assert_eq!(short(&entries, "Play"), "Start playback (double click: to zero)");
    assert_eq!(entries["Play"].get("control").unwrap().as_int(), 57);
    // The knobs and the buttons above the displays: one box each, the details in the long text
    assert_eq!(short(&entries, "knobs"), "The volume of 8 tracks");
    assert!(entries["knobs"].get("long").unwrap().as_text().contains("1: Drums, 2: Bass"));
    assert_eq!(entries["knobs"].get("scope").unwrap().as_text(), "mode");
    assert!(short(&entries, "Row21..Row28").starts_with("Knobs: Volume, Pan"));
    let members = entries["Row21..Row28"].get("members").unwrap().elements();
    assert_eq!(members.len(), 8);
    // The 7th button above the displays is a note (the MCU display needs one)
    assert_eq!(members[6].get("bind").unwrap().as_text(), "note");
    assert!(short(&entries, "Row31..Row38").starts_with("Select the track"));
    assert!(entries["Row31..Row38"].get("long").unwrap().as_text().contains("A: Drums (selected)"));
    assert!(!entries.contains_key("Knob1"), "the knobs have no boxes of their own");
    assert!(short(&entries, "MasterKnob").contains("volume of the selected track (Drums)"));
    assert_eq!(short(&entries, "ArrowLeft..ArrowRight"), "Select the previous / next track (the encoder's)");
    assert!(short(&entries, "grid").starts_with("Play"), "{}", short(&entries, "grid"));
    assert_eq!(short(&entries, "@mode"), "Volume");
    // The Mk3's buttons, which the Mk2 doesn't have: no boxes
    for id in ["Accent", "PageLeft", "F1", "Crossfader"] {
        assert!(!entries.contains_key(id), "{id}");
    }
}

#[test]
fn mk2_modifiers_change_the_texts() {
    let mut app = start_mk2();
    take(&mut app);
    // With NI's driver the transport's left arrow (Stop) is Shift
    let entries = take_after(&mut app, |app| cc(app, MK2_STOP, 127));
    assert_eq!(short(&entries, "Play"), "Loop: turn it on");
    assert_eq!(entries["Play"].get("scope").unwrap().as_text(), "modifier");
    assert!(short(&entries, "Stop").starts_with("Hold: Shift"));
    assert_eq!(short(&entries, "ArrowLeft..ArrowRight"), "The previous / next 8 tracks");
    check_lengths(&mut app, "Mk2 Stop as Shift");
    cc(&mut app, MK2_STOP, 0);
    app.tick();
    take(&mut app);

    // dbm-nihost sends the Shift button: the left arrow is Stop
    app.on_midi_event(IN, &MK2_HOST_SENDS_SHIFT);
    app.on_midi_event(IN, &MK2_SHIFT_DOWN);
    app.tick();
    let entries = take(&mut app);
    assert_eq!(short(&entries, "Play"), "Loop: turn it on");
    assert!(!entries.contains_key("Row31..Row38"), "the group buttons don't change with Shift");
    assert!(short(&entries, "Select").starts_with("Edit note mode"));
    assert!(short(&entries, "Row21..Row28").starts_with("Send − / +"));
    check_lengths(&mut app, "Mk2 Shift");
    app.on_midi_event(IN, &MK2_SHIFT_UP);
    app.tick();
    take(&mut app);
    let stop = help::collect(app.instance_ctx(0).unwrap()).into_iter().find(|e| e.id == "Stop").and_then(|e| e.help).unwrap();
    assert!(stop.short.starts_with("Stopped: back to the start"), "{}", stop.short);

    let entries = take_after(&mut app, |app| cc(app, MK2_SELECT, 127));
    assert!(short(&entries, "Play").starts_with("Punch in"));
    cc(&mut app, MK2_SELECT, 0);
    let entries = take_after(&mut app, |app| cc(app, MK2_ERASE, 127));
    assert_eq!(short(&entries, "Row31..Row38"), "Delete the track");
    cc(&mut app, MK2_ERASE, 0);
}

#[test]
fn mk2_views_and_modes_describe_the_pads_and_knobs() {
    let mut app = start_mk2();
    take(&mut app);
    // SCENE: the scene view
    cc(&mut app, MK2_SCENE, 127);
    let entries = take_after(&mut app, |_| {});
    assert!(short(&entries, "MasterKnob").contains("16 scenes"), "{}", short(&entries, "MasterKnob"));
    cc(&mut app, MK2_SCENE, 0);
    app.tick();
    assert!(app.instance_ctx(0).unwrap().views.is_active(Views::ScenePlay));
    let entries = take(&mut app);
    assert!(short(&entries, "grid").starts_with("Launch the 16 scenes"));
    // PAD MODE: the drum view, STEP: its sequencer
    cc(&mut app, MK2_PAD_MODE, 127);
    cc(&mut app, MK2_PAD_MODE, 0);
    cc(&mut app, MK2_STEP, 127);
    cc(&mut app, MK2_STEP, 0);
    app.tick();
    assert!(app.instance_ctx(0).unwrap().views.is_active(Views::Drum));
    let entries = take(&mut app);
    assert!(short(&entries, "grid").starts_with("The 16 steps of drum pad"), "{}", short(&entries, "grid"));
    // VOLUME: the panorama next
    let entries = take_after(&mut app, |app| {
        cc(app, MK2_VOLUME, 127);
        cc(app, MK2_VOLUME, 0);
    });
    assert_eq!(short(&entries, "knobs"), "The panorama of 8 tracks");
    assert!(short(&entries, "Volume").contains("send 1"), "{}", short(&entries, "Volume"));

    for view in MK2_VIEWS {
        let ctx = app.instance_ctx(0).unwrap();
        ctx.set_active_view(view);
        assert!(help::collect(ctx).iter().any(|e| e.id == "grid" && e.help.is_some()), "{view:?} describes its pads");
        check_lengths(&mut app, view.name());
    }
    for mode in MK2_MODES {
        let ctx = app.instance_ctx(0).unwrap();
        ctx.set_active_mode(mode);
        assert!(help::collect(ctx).iter().any(|e| e.id == "knobs" && e.help.is_some()), "{mode:?} describes the knobs");
        check_lengths(&mut app, mode.name());
    }
}

#[test]
fn mk2_the_table_covers_every_button() {
    let mut app = start_mk2();
    check_coverage(&mut app, &MK2_VIEWS, &MK2_MODES);
}

/// The changes after the function (and a tick).
fn take_after(app: &mut App, f: impl FnOnce(&mut App)) -> HashMap<String, JsonValue> {
    f(app);
    app.tick();
    take(app)
}

// ------------------------------------------------------------------------------------------
// The Push 1, 2 and 3 (one table)

const PUSH_VIEWS: [Views; 16] = [
    Views::Play, Views::Chords, Views::Piano, Views::PrgChange, Views::ClipLength, Views::Color, Views::Session, Views::Sequencer,
    Views::PolySequencer, Views::Drum, Views::DrumXox, Views::Drum4, Views::Drum8, Views::Raindrops, Views::ScenePlay, Views::Drum64,
];

/// The registered modes of a Push.
fn push_modes(app: &mut App) -> Vec<Modes> {
    let ctx = app.instance_ctx(0).unwrap();
    Modes::ALL.iter().copied().filter(|m| ctx.modes.contains(*m)).collect()
}

#[test]
fn push_the_table_covers_every_button_of_all_models() {
    for model in [1, 2, 3] {
        let mut app = start_push(model);
        let modes = push_modes(&mut app);
        assert!(modes.len() > 30, "Push {model}: {} modes", modes.len());
        check_coverage(&mut app, &PUSH_VIEWS, &modes);
    }
}

#[test]
fn push3_texts_in_every_view_and_mode() {
    let mut app = start_push(3);
    let entries = take(&mut app);
    assert!(short(&entries, "Play").starts_with("Start playback"));
    assert!(short(&entries, "Row11..Row18").contains("Select the track"), "{}", short(&entries, "Row11..Row18"));
    assert!(short(&entries, "Row21..Row28").starts_with("1 Volume, 2 Pan"));
    assert!(entries.contains_key("Scene1..Scene8"));
    assert!(entries.contains_key("knobs"));
    assert!(short(&entries, "PlayPosition").starts_with("Turn: the play position"));
    assert_eq!(short(&entries, "EncoderPush"), "Start / stop playback");
    for view in PUSH_VIEWS {
        let ctx = app.instance_ctx(0).unwrap();
        ctx.set_active_view(view);
        assert!(help::collect(ctx).iter().any(|e| e.id == "grid" && e.help.is_some()), "{view:?} describes its pads");
        check_lengths(&mut app, view.name());
    }
    for mode in push_modes(&mut app) {
        let ctx = app.instance_ctx(0).unwrap();
        ctx.set_active_mode(mode);
        check_lengths(&mut app, mode.name());
    }
}

#[test]
fn push_models_differ_where_the_code_does() {
    let mut app = start_push(1);
    let entries = take(&mut app);
    assert!(short(&entries, "Mute").starts_with("The 2nd row below the display"), "{}", short(&entries, "Mute"));
    assert!(!entries.contains_key("EncoderPush"), "no jog wheel on the Push 1");
    let mut app = start_push(2);
    let entries = take(&mut app);
    assert!(short(&entries, "Mute").starts_with("Mute / unmute the selected track"));
}
