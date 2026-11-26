// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The host's snapshots on the Maschine JAM (doc/HOST_BRIDGE_GUIDE.md §4.14): Select + SONG opens
//! the snapshot view (8 x 8 pads = slots 0-63 from the top left), a pad loads, Shift + pad
//! stores, Clear + pad removes; the colours show empty, stored, active and the targets. A host
//! without the commands (Bitwig) gets nothing.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::controller::maschine::core::colors::{dim_or_highlight_color, COLOR_BLACK, COLOR_DARK_GREY, COLOR_GREEN, COLOR_RED_LO, COLOR_WHITE};
use drivenbymoss::daw::color::ColorEx;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::Views;
use drivenbymoss::protocol::{Cmd, CmdId, ControllerViewKind};

const J_IN: i32 = 13;
const J_OUT: i32 = 15;

const JAM_SONG: u8 = 30;
const JAM_CLEAR: u8 = 33;
const JAM_LEVEL: u8 = 37;
const JAM_SELECT: u8 = 50;
const SHIFT_DOWN: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x01, 0xF7];
const SHIFT_UP: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x00, 0xF7];

fn config_dir(name: &str) -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-snapshots-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.ni.maschine.jam.MaschineJamControllerInstance\n").unwrap();
    dir.to_str().unwrap().to_string()
}

/// What the core sent of the snapshots and the controller view.
#[derive(Debug, Clone, PartialEq)]
enum Sent {
    View(ControllerViewKind),
    Load(usize),
    Store(usize),
    Remove(usize),
}

struct Rig {
    app: App,
    backend: Rc<RecordingBackend>,
    time: u64,
}

impl Rig {
    /// The JAM on a binary host which handles all commands (`snapshots`), or all but the
    /// snapshot and controller view commands (as the Bitwig extension).
    fn new(snapshots: bool) -> Self {
        let backend = Rc::new(RecordingBackend { inputs: vec![(J_IN, "Maschine Jam - 1".into())], outputs: vec![(J_OUT, "Maschine Jam - 1".into())], ..Default::default() });
        let handled = if snapshots {
            Vec::new()
        } else {
            let mut words = vec![0u64; 0x8000 / 64];
            let unhandled = [CmdId::ControllerView, CmdId::ClipEditWindow, CmdId::SnapshotLoad, CmdId::SnapshotStore, CmdId::SnapshotRemove];
            for id in CmdId::ALL.iter().filter(|id| !unhandled.contains(id)) {
                let bit = usize::from(*id as u16 & 0x7FFF);
                words[bit / 64] |= 1 << (bit % 64);
            }
            words
        };
        *backend.handled.borrow_mut() = Some(handled);
        let mut app = App::new(backend.clone(), &config_dir(if snapshots { "with" } else { "without" }), 7, 22);
        app.start_infrastructure();
        app.update_model("track/count 1\ntrack/0/exists 1\ntrack/0/name Synth\ntrack/0/type INSTRUMENT\ntrack/0/select 1\n");
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
        self.advance(60);
    }

    fn select_song(&mut self) {
        self.cc(JAM_SELECT, 127);
        self.press(JAM_SONG);
        self.cc(JAM_SELECT, 0);
        self.advance(60);
    }

    fn note(slot: usize) -> u8 {
        (36 + (7 - slot / 8) * 8 + slot % 8) as u8
    }

    fn pad_down(&mut self, slot: usize) {
        self.app.on_midi_event(J_IN, &[0x90, Self::note(slot), 127]);
        self.advance(30);
    }

    fn pad_up(&mut self, slot: usize) {
        self.app.on_midi_event(J_IN, &[0x80, Self::note(slot), 0]);
        self.advance(30);
    }

    fn pad(&mut self, slot: usize) {
        self.pad_down(slot);
        self.pad_up(slot);
    }

    fn shift(&mut self, down: bool) {
        self.app.on_midi_event(J_IN, if down { &SHIFT_DOWN } else { &SHIFT_UP });
        self.advance(30);
    }

    fn color(&mut self, slot: usize) -> i32 {
        self.ctx().surface.grid_ref().light_info(Self::note(slot).into()).color
    }

    fn sent(&self) -> Vec<Sent> {
        let calls = self.backend.take_calls();
        calls
            .iter()
            .filter_map(|call| match call.cmd()? {
                Cmd::ControllerView { kind } => Some(Sent::View(kind)),
                Cmd::SnapshotLoad { index } => Some(Sent::Load(index)),
                Cmd::SnapshotStore { index } => Some(Sent::Store(index)),
                Cmd::SnapshotRemove { index } => Some(Sent::Remove(index)),
                _ => None,
            })
            .collect()
    }

    fn view(&mut self) -> Option<Views> {
        self.ctx().views.active_id()
    }

    /// The colour of a stored snapshot with the RGB colour (0..255).
    fn stored_color(&mut self, rgb: [f64; 3]) -> i32 {
        let color = ColorEx::from_array([rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0]);
        dim_or_highlight_color(&self.ctx().colors, color, false)
    }
}

/// A dense list of 3 (appendOnly), "B" active, "A" orange.
const DENSE: &str = "snapshot/appendOnly 1\nsnapshot/count 3\nsnapshot/active 1\n\
    snapshot/0/exists 1\nsnapshot/0/name A\nsnapshot/0/color 255 80 0\n\
    snapshot/1/exists 1\nsnapshot/1/name B\n\
    snapshot/2/exists 1\nsnapshot/2/name C\n";

/// A sparse set as zynthian's (slot i = program change i, 128 slots, the window the first 64): 0,
/// 5 and 40 stored, 5 active.
const SPARSE: &str = "snapshot/appendOnly 0\nsnapshot/count 128\nsnapshot/active 5\n\
    snapshot/0/exists 1\nsnapshot/0/name Intro\nsnapshot/0/color 0 96 255\n\
    snapshot/5/exists 1\nsnapshot/5/name Verse\n\
    snapshot/40/exists 1\nsnapshot/40/name Chorus\n";

#[test]
fn select_song_enters_and_leaves_the_snapshot_view() {
    let mut rig = Rig::new(true);
    rig.press(JAM_SONG);
    assert_eq!(rig.view(), Some(Views::Session));
    rig.sent();
    rig.app.take_notifications();

    rig.select_song();
    assert_eq!(rig.view(), Some(Views::Snapshots));
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Snapshots)]);
    assert!(rig.app.take_notifications().contains("Snapshots"));

    // Again: back to the view before
    rig.select_song();
    assert_eq!(rig.view(), Some(Views::Session));
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Launcher)]);

    // Another view button leaves it too
    rig.select_song();
    assert_eq!(rig.view(), Some(Views::Snapshots));
    rig.press(JAM_SONG);
    assert_eq!(rig.view(), Some(Views::Session));
    // A track report does not leave it
    rig.select_song();
    rig.app.update_model("track/0/select 0\ntrack/0/select 1\n");
    rig.advance(60);
    assert_eq!(rig.view(), Some(Views::Snapshots));
    // LEVEL (a mode button) keeps the pads
    rig.press(JAM_LEVEL);
    assert_eq!(rig.view(), Some(Views::Snapshots));
}

#[test]
fn a_dense_list_loads_stores_at_count_and_removes() {
    let mut rig = Rig::new(true);
    rig.app.update_model(DENSE);
    rig.select_song();
    rig.sent();
    rig.app.take_notifications();

    // The colours: A its colour dimmed, B active, C dim white, the rest off
    let orange = rig.stored_color([255.0, 80.0, 0.0]);
    assert_ne!(orange, COLOR_BLACK);
    assert_eq!([rig.color(0), rig.color(1), rig.color(2), rig.color(3), rig.color(63)], [orange, COLOR_GREEN, COLOR_DARK_GREY, COLOR_BLACK, COLOR_BLACK]);

    // Pad: load, notify the name; white while held
    rig.pad_down(2);
    assert_eq!(rig.color(2), COLOR_WHITE);
    rig.pad_up(2);
    assert_eq!(rig.sent(), [Sent::Load(2)]);
    assert!(rig.app.take_notifications().contains('C'));
    assert_eq!(rig.color(2), COLOR_DARK_GREY);
    // An empty pad loads nothing
    rig.pad(10);
    assert_eq!(rig.sent(), []);

    // Shift held: only the pad at count is the store target
    rig.shift(true);
    assert_eq!([rig.color(3), rig.color(4), rig.color(1)], [COLOR_RED_LO, COLOR_BLACK, COLOR_GREEN]);
    // Shift + any empty pad: a new one at count; Shift + a stored one: overwrite
    rig.pad(20);
    rig.pad(0);
    rig.shift(false);
    assert_eq!(rig.sent(), [Sent::Store(3), Sent::Store(0)]);
    let notifications = rig.app.take_notifications();
    assert!(notifications.contains("Stored") && notifications.contains("Stored: A"), "{notifications}");
    // Shift was used: no Shift view
    assert_eq!(rig.view(), Some(Views::Snapshots));

    // Clear held: the stored pads are the targets; Clear + pad removes (an empty one: nothing)
    rig.cc(JAM_CLEAR, 127);
    rig.advance(30);
    assert_eq!([rig.color(0), rig.color(1), rig.color(2), rig.color(3)], [COLOR_RED_LO, COLOR_RED_LO, COLOR_RED_LO, COLOR_BLACK]);
    rig.pad(1);
    rig.pad(7);
    rig.cc(JAM_CLEAR, 0);
    rig.advance(30);
    assert_eq!(rig.sent(), [Sent::Remove(1)]);
    assert!(rig.app.take_notifications().contains("Removed: B"));
}

#[test]
fn a_sparse_set_with_gaps() {
    let mut rig = Rig::new(true);
    rig.app.update_model(SPARSE);
    rig.select_song();
    rig.sent();

    // The colours: 0 blue dimmed, 5 active, 40 dim white, the gaps off
    let blue = rig.stored_color([0.0, 96.0, 255.0]);
    assert_eq!([rig.color(0), rig.color(1), rig.color(5), rig.color(6), rig.color(40), rig.color(41)], [blue, COLOR_BLACK, COLOR_GREEN, COLOR_BLACK, COLOR_DARK_GREY, COLOR_BLACK]);

    // Load 40
    rig.pad(40);
    assert_eq!(rig.sent(), [Sent::Load(40)]);
    assert!(rig.app.take_notifications().contains("Chorus"));

    // Shift held: every empty pad is a store target; Shift + a gap stores into it
    rig.shift(true);
    assert_eq!([rig.color(1), rig.color(6), rig.color(63), rig.color(0), rig.color(5)], [COLOR_RED_LO, COLOR_RED_LO, COLOR_RED_LO, blue, COLOR_GREEN]);
    rig.pad(7);
    rig.shift(false);
    assert_eq!(rig.sent(), [Sent::Store(7)]);
    rig.app.update_model("snapshot/7/exists 1\nsnapshot/active 7\n");
    rig.advance(30);
    assert_eq!([rig.color(7), rig.color(5)], [COLOR_GREEN, COLOR_DARK_GREY]);

    // Clear + 5: removed
    rig.cc(JAM_CLEAR, 127);
    rig.pad(5);
    rig.cc(JAM_CLEAR, 0);
    rig.advance(30);
    assert_eq!(rig.sent(), [Sent::Remove(5)]);
    assert!(rig.app.take_notifications().contains("Removed: Verse"));
    rig.app.update_model("snapshot/5/exists 0\n");
    rig.advance(30);
    assert_eq!(rig.color(5), COLOR_BLACK);
}

#[test]
fn a_host_without_snapshots_gets_nothing() {
    let mut rig = Rig::new(false);
    // Even with reports (which such a host never sends) the pads stay off
    rig.app.update_model(DENSE);
    rig.app.take_notifications();
    rig.select_song();
    assert_eq!(rig.view(), Some(Views::Snapshots));
    assert!(rig.app.take_notifications().contains("No snapshots"));
    assert!((0..64).all(|slot| rig.color(slot) == COLOR_BLACK));
    rig.pad(0);
    rig.shift(true);
    rig.pad(3);
    rig.shift(false);
    rig.cc(JAM_CLEAR, 127);
    rig.pad(1);
    rig.cc(JAM_CLEAR, 0);
    rig.select_song();
    assert_eq!(rig.sent(), []);
    assert_ne!(rig.view(), Some(Views::Snapshots));
}
