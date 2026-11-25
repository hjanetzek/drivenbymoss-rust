// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The host's parameter pages on the Maschine JAM (doc/HOST_PAGE_PLAN.md): the host reports a
//! zynthian-like 8x6 page (`/hostpage/…`), the JAM shows it in its layout (the page list in row 1,
//! the cells in rows 2-7, the sections in row 8, the strips on the active row) and sends its
//! input as typed `hostpage …` commands. The selection is the host's: the JAM shows what is
//! reported, and a selecting input is only a command.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordedCalls, RecordingBackend};
use drivenbymoss::controller::maschine::jam::surface_link as jam_surface;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::Views;
use drivenbymoss::framework::page_surface::view::palette_index;
use drivenbymoss::framework::page_surface::{LinkState, Rgb, SlotKind};
use drivenbymoss::protocol::{Cmd, Resolution, TurnUnit};

const J_IN: i32 = 13;
const J_OUT: i32 = 15;

const JAM_SELECT: u8 = 50;
const JAM_SONG: u8 = 30;
const JAM_KNOB_TURN: u8 = 57;
const JAM_KNOB_PUSH: u8 = 58;
const JAM_NAV_UP: u8 = 41;
const JAM_NAV_DOWN: u8 = 42;
const JAM_NAV_LEFT: u8 = 43;
const JAM_NAV_RIGHT: u8 = 44;
const JAM_RIGHT: u8 = 93;
const JAM_FADER1: u8 = 8;
const JAM_FADER_TOUCH1: u8 = 20;

fn config_dir(name: &str) -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-host-page-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.ni.maschine.jam.MaschineJamControllerInstance\n").unwrap();
    dir.to_str().unwrap().to_string()
}

/// A page as zynthian's grid8x6 would report it: 2 control sets of 4 cells a row, 3 sections.
fn page_report(epoch: i32, set: i32, cell: i32) -> String {
    let mut lines = vec![
        format!("hostpage/epoch {epoch}"),
        "hostpage/exists 1".into(),
        "hostpage/title Host · Synth".into(),
        "hostpage/columns 8".into(),
        "hostpage/rows 6".into(),
        "hostpage/page/count 2".into(),
        "hostpage/page/0/name Chain Mixer".into(),
        "hostpage/page/1/name Helm Voices".into(),
        "hostpage/page/selected 0".into(),
        "hostpage/section/count 2".into(),
        "hostpage/section/0/name Chain".into(),
        "hostpage/section/0/color 200 200 200".into(),
        "hostpage/section/1/name Helm".into(),
        "hostpage/section/1/color 255 80 0".into(),
        "hostpage/set/count 3".into(),
    ];
    for (s, (name, section)) in [("Mixer", 0), ("Osc 1", 1), ("Filter", 1)].iter().enumerate() {
        lines.push(format!("hostpage/set/{s}/name {name}"));
        lines.push(format!("hostpage/set/{s}/section {section}"));
        lines.push(format!("hostpage/set/{s}/color {}", if *section == 0 { "200 200 200" } else { "255 80 0" }));
    }
    // Row 0: Mixer (4 cells), Osc 1 (4 cells); row 1: Filter (3 cells, a gap)
    let cells: [(usize, i32, &str, f64, &str, i32, i32); 11] = [
        (0, 1, "Level", 0.9, "-6.0 dB", 0, 0),
        (1, 1, "Balance", 0.5, "C", 0, 0),
        (2, 3, "Mute", 0.0, "Off", 2, 0),
        (3, 4, "Panic", 0.0, "Fire", 0, 0),
        (4, 2, "Wave", 0.5, "Saw", 5, 1),
        (5, 1, "Transpose", 0.5, "0 st", 49, 1),
        (6, 1, "Tune", 0.5, "+0 ct", 0, 1),
        (7, 1, "Volume", 0.7, "70 %", 0, 1),
        (8, 1, "Cutoff", 0.25, "2.00 kHz", 0, 2),
        (9, 1, "Resonance", 0.3, "30 %", 0, 2),
        (10, 2, "Style", 0.0, "12 dB", 4, 2),
    ];
    for (cell, kind, name, value, text, steps, set) in cells {
        let color = if set == 0 { "200 200 200" } else { "255 80 0" };
        lines.extend([
            format!("hostpage/cell/{cell}/kind {kind}"),
            format!("hostpage/cell/{cell}/name {name}"),
            format!("hostpage/cell/{cell}/color {color}"),
            format!("hostpage/cell/{cell}/value {value}"),
            format!("hostpage/cell/{cell}/text {text}"),
            format!("hostpage/cell/{cell}/steps {steps}"),
            format!("hostpage/cell/{cell}/enabled 1"),
            format!("hostpage/cell/{cell}/set {set}"),
        ]);
    }
    lines.push(format!("hostpage/set/selected {set}"));
    lines.push(format!("hostpage/section/selected {}", if set == 0 { 0 } else { 1 }));
    lines.push(format!("hostpage/row {}", set / 2));
    lines.push(format!("hostpage/cell/selected {cell}"));
    lines.join("\n") + "\n"
}

struct Rig {
    app: App,
    backend: Rc<RecordingBackend>,
    time: u64,
}

impl Rig {
    /// The JAM on a binary host which handles all commands (with them the host pages).
    fn new(host_pages: bool) -> Self {
        let backend = Rc::new(RecordingBackend { inputs: vec![(J_IN, "Maschine Jam - 1".into())], outputs: vec![(J_OUT, "Maschine Jam - 1".into())], ..Default::default() });
        if host_pages {
            *backend.handled.borrow_mut() = Some(Vec::new());
        }
        let mut app = App::new(backend.clone(), &config_dir(if host_pages { "with" } else { "without" }), 7, 22);
        app.start_infrastructure();
        app.update_model("track/count 1\ntrack/0/exists 1\ntrack/0/name Synth\ntrack/0/select 1\n");
        let mut rig = Rig { app, backend, time: 0 };
        rig.advance(1100);
        rig
    }

    fn advance(&mut self, ms: u64) {
        let end = self.time + ms;
        while self.time < end {
            self.time = (self.time + 30).min(end);
            self.app.set_time_ms(self.time);
            self.app.tick();
        }
    }

    fn ctx(&mut self) -> &mut Ctx {
        self.app.instance_ctx(0).unwrap()
    }

    fn cc(&mut self, control: u8, value: u8) {
        self.app.on_midi_event(J_IN, &[0xB0, control, value]);
    }

    fn press(&mut self, control: u8) {
        self.cc(control, 127);
        self.cc(control, 0);
    }

    fn pad(&mut self, x: u8, y: u8) {
        let note = 36 + (7 - y) * 8 + x;
        self.app.on_midi_event(J_IN, &[0x90, note, 127]);
        self.app.on_midi_event(J_IN, &[0x80, note, 0]);
    }

    fn pad_color(&mut self, x: i32, y: i32) -> i32 {
        let note = 36 + (7 - y) * 8 + x;
        self.ctx().surface.grid_ref().light_info(note).color
    }

    fn select_song(&mut self) {
        self.cc(JAM_SELECT, 127);
        self.press(JAM_SONG);
        self.cc(JAM_SELECT, 0);
        self.advance(30);
    }

    fn commands(&self) -> Vec<BackendCall> {
        self.backend.take_calls()
    }

    /// The host page commands since the last call.
    fn host_commands(&self) -> Vec<String> {
        let calls = self.backend.take_calls();
        calls.commands().filter(|c| matches!(c.id() as u16 >> 8, 0x8C)).map(|c| format!("{c:?}")).collect()
    }
}

fn cmd_text(cmd: Cmd) -> String {
    format!("{cmd:?}")
}

#[test]
fn without_the_feature_there_is_no_host_source() {
    let mut rig = Rig::new(false);
    let client = jam_surface::client(rig.ctx());
    assert_eq!(client.borrow().source_state(1), LinkState::Missing);
    // Select + SONG with no source at all: the surface with the cross (as before)
    rig.select_song();
    assert!(jam_surface::is_active(rig.ctx()));
    assert_eq!(client.borrow().current_source(), 0);
    assert!(rig.host_commands().is_empty());
}

#[test]
fn shows_the_host_page_and_sends_the_input() {
    let mut rig = Rig::new(true);
    let client = jam_surface::client(rig.ctx());
    assert_eq!(client.borrow().source_state(1), LinkState::Attached);
    rig.commands();

    // Select + SONG: the Electra is not there, so the host's pages; the host is told
    rig.select_song();
    assert!(rig.ctx().views.is_active(Views::SurfaceLink));
    assert_eq!(client.borrow().current_source(), 1);
    assert_eq!(rig.host_commands(), [cmd_text(Cmd::HostPageShow { columns: 8, rows: 6, encoders: 8, rows_per_set: 1, value: true })]);

    // The host's dump
    rig.app.update_model(&page_report(1, 1, 4));
    rig.advance(60);
    assert_eq!(client.borrow().provider_name(), "Host · Synth");
    assert!(rig.app.take_notifications().contains("Surface: Host"));
    let model = client.borrow().model().clone();
    assert_eq!((model.columns, model.rows, model.slots.len()), (8, 6, 48));
    assert_eq!((model.slots[4].kind, model.slots[4].name.as_str(), model.slots[4].value_text.as_str(), model.slots[4].steps), (SlotKind::List, "Wave", "Saw", 5));
    assert_eq!((model.slots[2].kind, model.slots[3].kind), (SlotKind::Pad, SlotKind::Pad));
    assert_eq!((model.active_set, model.active_row, model.selected_slot, model.active_section), (Some(1), 0, Some(4), Some(1)));
    assert_eq!(model.slots[11].kind, SlotKind::Empty);

    // The grid: the page list in row 1 (2 pages, the first orange; a next page, no previous),
    // the cells in rows 2-7 over 8 columns, the sections in row 8
    use drivenbymoss::controller::maschine::core::colors::{COLOR_AMBER_LO, COLOR_ORANGE};
    assert_eq!(rig.pad_color(0, 0), COLOR_ORANGE);
    assert_eq!(rig.pad_color(1, 0), COLOR_AMBER_LO);
    assert_eq!(rig.pad_color(2, 0), 0);
    assert_eq!(rig.pad_color(6, 0), 0);
    assert_ne!(rig.pad_color(7, 0), 0);
    let orange = palette_index(rig.ctx(), Rgb::new(255, 80, 0));
    // The active set (Osc 1) at full level, the other cells one level lower (NI's palette)
    assert_eq!(rig.pad_color(5, 1), orange);
    assert_ne!(rig.pad_color(0, 2), 0);
    assert_ne!(rig.pad_color(0, 2), orange, "outside the active set: dimmer");
    assert_eq!(rig.pad_color(3, 2), 0, "the gap of the Filter page");
    assert_eq!(rig.pad_color(0, 3), 0, "row 3 has no page");
    assert_eq!(rig.pad_color(1, 7), orange, "the active section");
    assert_ne!(rig.pad_color(0, 7), 0);
    assert_eq!(rig.pad_color(2, 7), 0);

    // Strips 1-8: the 8 cells of the active row
    let strips = jam_surface::fader_configs(rig.ctx()).unwrap();
    assert_eq!(strips[0].value, 114, "Level 0.9");
    assert_eq!(strips[7].value, 89, "Volume 0.7: strip 8 is column 8");

    // Strip 8 turns Volume by the travel; its touch touches it
    rig.cc(JAM_FADER_TOUCH1 + 7, 127);
    rig.cc(JAM_FADER1 + 7, 60);
    rig.cc(JAM_FADER1 + 7, 64);
    rig.cc(JAM_FADER_TOUCH1 + 7, 0);
    rig.advance(30);
    assert_eq!(
        rig.host_commands(),
        [
            cmd_text(Cmd::HostPageCellTouch { cell: 7, epoch: 1, value: true }),
            cmd_text(Cmd::HostPageCellTurn { cell: 7, epoch: 1, unit: TurnUnit::Travel, resolution: Resolution::Normal, delta: 4 }),
            cmd_text(Cmd::HostPageCellTouch { cell: 7, epoch: 1, value: false }),
        ]
    );

    // The main encoder turns the selected cell (Wave) in detents, its push resets it
    rig.cc(JAM_KNOB_TURN, 1);
    rig.press(JAM_KNOB_PUSH);
    rig.advance(30);
    assert_eq!(
        rig.host_commands(),
        [
            cmd_text(Cmd::HostPageCellTurn { cell: 4, epoch: 1, unit: TurnUnit::Detent, resolution: Resolution::Normal, delta: 1 }),
            cmd_text(Cmd::HostPageCellReset { cell: 4, epoch: 1 }),
        ]
    );

    // Selecting input is only a command: the model keeps the host's selection
    rig.pad(1, 2);
    rig.press(JAM_NAV_DOWN);
    rig.press(JAM_NAV_LEFT);
    rig.press(JAM_NAV_RIGHT);
    rig.pad(0, 7);
    rig.pad(1, 0);
    rig.press(JAM_RIGHT);
    rig.advance(30);
    assert_eq!(
        rig.host_commands(),
        [
            cmd_text(Cmd::HostPageCellPress { cell: 9, epoch: 1, value: true }),
            cmd_text(Cmd::HostPageCellPress { cell: 9, epoch: 1, value: false }),
            cmd_text(Cmd::HostPageRow { row: 1 }),
            cmd_text(Cmd::HostPageSet { set: 0 }),
            cmd_text(Cmd::HostPageSet { set: 2 }),
            cmd_text(Cmd::HostPageSection { section: 0 }),
            cmd_text(Cmd::HostPagePage { page: 1 }),
            cmd_text(Cmd::HostPagePage { page: 1 }),
        ]
    );
    let model = client.borrow().model().clone();
    assert_eq!((model.active_set, model.active_row, model.selected_slot), (Some(1), 0, Some(4)));

    // The host applies a selection (its screen, or the JAM's press) and reports it: the JAM follows
    rig.app.update_model("hostpage/set/selected 2\nhostpage/section/selected 1\nhostpage/row 1\nhostpage/cell/selected 9\n");
    rig.advance(30);
    let model = client.borrow().model().clone();
    assert_eq!((model.active_set, model.active_row, model.selected_slot), (Some(2), 1, Some(9)));
    let orange = palette_index(rig.ctx(), Rgb::new(255, 80, 0));
    assert_ne!(rig.pad_color(5, 1), orange, "Osc 1 is no longer the active set");
    assert_eq!(rig.pad_color(0, 2), orange, "Filter is");
    let strips = jam_surface::fader_configs(rig.ctx()).unwrap();
    assert_eq!(strips[0].value, 32, "the strips on row 2: Cutoff 0.25");
    // UP: the row before
    rig.press(JAM_NAV_UP);
    rig.advance(30);
    assert_eq!(rig.host_commands(), [cmd_text(Cmd::HostPageRow { row: 0 })]);

    // A value change of the host: the strip follows
    rig.app.update_model("hostpage/cell/8/value 0.5\nhostpage/cell/8/text 4.00 kHz\n");
    rig.advance(30);
    assert_eq!(jam_surface::fader_configs(rig.ctx()).unwrap()[0].value, 64);
    assert_eq!(client.borrow().model().slots[8].value_text, "4.00 kHz");

    // A new epoch (another chain): the cell commands carry it
    rig.app.update_model(&page_report(2, 0, 0));
    rig.advance(30);
    rig.cc(JAM_KNOB_TURN, 127);
    rig.advance(30);
    assert_eq!(rig.host_commands(), [cmd_text(Cmd::HostPageCellTurn { cell: 0, epoch: 2, unit: TurnUnit::Detent, resolution: Resolution::Normal, delta: -1 })]);

    // Select + SONG again: no further source, off; the host is told
    rig.select_song();
    assert!(!jam_surface::is_active(rig.ctx()));
    assert_eq!(rig.host_commands(), [cmd_text(Cmd::HostPageShow { columns: 8, rows: 6, encoders: 8, rows_per_set: 1, value: false })]);
}

#[test]
fn a_track_change_keeps_the_host_page() {
    // Another track (in zynthian: another chain, from the group buttons or zynthian's screen)
    // changes the host's page; it must not recall the track's play view and leave the surface
    let mut rig = Rig::new(true);
    rig.app.update_model("track/count 4\ntrack/1/exists 1\ntrack/1/name Drums\ntrack/2/exists 1\ntrack/2/name Bass\ntrack/3/exists 1\ntrack/3/name Keys\n");
    rig.advance(60);
    rig.select_song();
    rig.app.update_model(&page_report(1, 1, 4));
    rig.advance(60);
    assert!(jam_surface::is_active(rig.ctx()));
    rig.commands();

    rig.app.update_model("track/0/select 0\ntrack/2/select 1\n");
    rig.advance(60);
    assert!(jam_surface::is_active(rig.ctx()));
    assert!(rig.ctx().views.is_active(Views::SurfaceLink));
    assert!(!rig.host_commands().contains(&cmd_text(Cmd::HostPageShow { columns: 8, rows: 6, encoders: 8, rows_per_set: 1, value: false })));
}

#[test]
fn the_notification_shows_the_new_title_after_switching_on() {
    let mut rig = Rig::new(true);
    rig.select_song();
    rig.app.update_model(&page_report(1, 1, 4));
    rig.advance(60);
    assert!(rig.app.take_notifications().contains("Surface: Host · Synth"));
    // Off, another chain in the host, on again: the client still has the old title at first
    rig.select_song();
    assert!(!jam_surface::is_active(rig.ctx()));
    rig.select_song();
    rig.app.take_notifications();
    rig.app.update_model(&(page_report(2, 1, 4) + "hostpage/title Host · Bass\n"));
    rig.advance(60);
    let notifications = rig.app.take_notifications();
    assert!(notifications.contains("Surface: Host · Bass"), "{notifications}");
    // Only once: a later page change is no reason for another notification
    rig.app.update_model("hostpage/page/selected 1\n");
    rig.advance(90);
    assert!(!rig.app.take_notifications().contains("Surface:"));
}
