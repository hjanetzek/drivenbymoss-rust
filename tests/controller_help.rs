// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The help of the controls (doc/CONTROLLER_HELP_PLAN.md) on the Maschine JAM: the changes since
//! the last call, the texts in the states the modifiers and the views make.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::controller::maschine::jam::surface as jam;
use drivenbymoss::framework::help;
use drivenbymoss::framework::help_table::HelpKey;
use drivenbymoss::framework::ids::{ButtonId, Modes, Views};
use drivenbymoss::json::JsonValue;

const IN: i32 = 3;
const OUT: i32 = 5;
const SHIFT_DOWN: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x01, 0xF7];
const SHIFT_UP: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x00, 0xF7];

fn start() -> App {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-help-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.ni.maschine.jam.MaschineJamControllerInstance\n").unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, "Maschine Jam - 1".into())], outputs: vec![(OUT, "Maschine Jam - 1".into())], ..Default::default() });
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

/// The table has a row (a text, `none` or `command`) for every button with a command in every
/// view and mode of the JAM, and for the pads and the strips.
#[test]
fn the_table_covers_every_button() {
    let mut app = start();
    let ctx = app.instance_ctx(0).unwrap();
    let table = ctx.help_table.expect("the JAM has a help table");
    let rows = table.button_rows();
    let buttons: Vec<ButtonId> = ctx.surface.button_ids().into_iter().filter(|id| ctx.surface.button(*id).unwrap().command().is_some()).collect();
    let mut missing = Vec::new();
    for view in VIEWS {
        for mode in MODES {
            for button in &buttons {
                let key = rows.iter().copied().find(|row| row.buttons().contains(button)).unwrap_or(HelpKey::Button(*button));
                if !table.has_entry(Some(view), Some(mode), key) {
                    missing.push(format!("{view:?}/{mode:?}: {button:?}"));
                }
            }
            for key in [HelpKey::Grid, HelpKey::Controls] {
                if !table.has_entry(Some(view), Some(mode), key) {
                    missing.push(format!("{view:?}/{mode:?}: {key:?}"));
                }
            }
        }
    }
    missing.dedup();
    assert!(missing.is_empty(), "{} without a row, e.g. {:?}", missing.len(), &missing[..missing.len().min(10)]);
}
