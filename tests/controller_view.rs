// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The view which the user picks on the Maschine JAM (`ControllerView`) and the edit window of
//! its step sequencer (`ClipEditWindow`), for a host which follows the controller
//! (doc/HOST_BRIDGE_GUIDE.md §4.13): sent only for a press of a view button, the window after
//! a sequencer's view and on its changes, and only to a host which handles the commands.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::Views;
use drivenbymoss::protocol::{Cmd, CmdId, ControllerViewKind, EditWindow};

const J_IN: i32 = 13;
const J_OUT: i32 = 15;

const JAM_SONG: u8 = 30;
const JAM_STEP: u8 = 31;
const JAM_PAD_MODE: u8 = 32;
const JAM_LEVEL: u8 = 37;
const JAM_CONTROL: u8 = 39;
const JAM_NAV_UP: u8 = 41;
const JAM_NAV_LEFT: u8 = 43;
const JAM_NAV_RIGHT: u8 = 44;
const JAM_SCENE1: u8 = 80;

fn config_dir(name: &str) -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-controller-view-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.ni.maschine.jam.MaschineJamControllerInstance\n").unwrap();
    dir.to_str().unwrap().to_string()
}

/// A reported command of the controller: the view or the window.
#[derive(Debug, Clone, PartialEq)]
enum Sent {
    View(ControllerViewKind),
    Window(EditWindow),
}

struct Rig {
    app: App,
    backend: Rc<RecordingBackend>,
    time: u64,
}

impl Rig {
    /// The JAM on a binary host which handles all commands (`follows`) or all but the two of
    /// the controller.
    fn new(follows: bool) -> Self {
        let backend = Rc::new(RecordingBackend { inputs: vec![(J_IN, "Maschine Jam - 1".into())], outputs: vec![(J_OUT, "Maschine Jam - 1".into())], ..Default::default() });
        let handled = if follows {
            Vec::new()
        } else {
            let mut words = vec![0u64; 0x8000 / 64];
            for id in CmdId::ALL.iter().filter(|id| !matches!(id, CmdId::ControllerView | CmdId::ClipEditWindow)) {
                let bit = usize::from(*id as u16 & 0x7FFF);
                words[bit / 64] |= 1 << (bit % 64);
            }
            words
        };
        *backend.handled.borrow_mut() = Some(handled);
        let mut app = App::new(backend.clone(), &config_dir(if follows { "follows" } else { "not" }), 7, 22);
        app.start_infrastructure();
        app.update_model(
            "track/count 2\ntrack/0/exists 1\ntrack/0/name Synth\ntrack/0/type INSTRUMENT\ntrack/0/select 1\n\
             track/1/exists 1\ntrack/1/name Bass\ntrack/1/type INSTRUMENT\n\
             track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/1/exists 1\ntrack/0/slot/0/selected 1\n\
             clip/exists 1\nclip/start 0\nclip/end 16\nclip/loop 1\n",
        );
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

    /// Hold the button and press the scene button `scene` (0-based).
    fn press_with_scene(&mut self, control: u8, scene: u8) {
        self.cc(control, 127);
        self.cc(JAM_SCENE1 + scene, 127);
        self.cc(JAM_SCENE1 + scene, 0);
        self.cc(control, 0);
        self.advance(60);
    }

    /// The views and windows sent since the last call.
    fn sent(&self) -> Vec<Sent> {
        let calls = self.backend.take_calls();
        calls
            .iter()
            .filter_map(|call| match call.cmd()? {
                Cmd::ControllerView { kind } => Some(Sent::View(kind)),
                Cmd::ClipEditWindow { window } => Some(Sent::Window(window)),
                _ => None,
            })
            .collect()
    }

    fn active_view(&mut self) -> Option<Views> {
        self.ctx().views.active_id()
    }
}

fn windows(sent: &[Sent]) -> Vec<EditWindow> {
    sent.iter().filter_map(|s| if let Sent::Window(w) = s { Some(*w) } else { None }).collect()
}

#[test]
fn song_sends_the_launcher_also_twice_in_a_row() {
    let mut rig = Rig::new(true);
    assert_eq!(rig.sent(), [], "Nothing on startup");
    rig.press(JAM_SONG);
    assert_eq!(rig.active_view(), Some(Views::Session));
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Launcher)]);
    // Again in the session view (it flips the session): the view is picked again
    rig.press(JAM_SONG);
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Launcher)]);
}

#[test]
fn step_sends_the_sequencer_then_its_window_and_paging_a_new_window() {
    let mut rig = Rig::new(true);
    rig.sent();
    rig.press(JAM_STEP);
    assert_eq!(rig.active_view(), Some(Views::Sequencer));
    let sent = rig.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[0], Sent::View(ControllerViewKind::Sequencer));
    let Sent::Window(window) = sent[1] else { panic!("{sent:?}") };
    // 8 steps of 1/16 on page 0; 7 rows of the scale from C1 (36)
    assert_eq!((window.start, window.length), (0.0, 2.0));
    assert_eq!(window.low, 36);
    assert!(window.high > window.low && window.high < 60, "{window:?}");

    // Nothing more without a change
    rig.advance(300);
    assert_eq!(rig.sent(), []);

    // The next page: one new window
    rig.press(JAM_NAV_RIGHT);
    rig.advance(300);
    assert_eq!(windows(&rig.sent()), [EditWindow { start: 2.0, ..window }]);

    // The octave up: the rows move
    rig.press(JAM_NAV_UP);
    let moved = windows(&rig.sent());
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert_eq!((moved[0].start, moved[0].length), (2.0, 2.0));
    assert!(moved[0].low > window.low, "{moved:?}");

    // STEP again (the next sequencer, poly): the view and its window again
    rig.press(JAM_STEP);
    assert_eq!(rig.active_view(), Some(Views::PolySequencer));
    let sent = rig.sent();
    assert_eq!(sent.first(), Some(&Sent::View(ControllerViewKind::Sequencer)), "{sent:?}");
    assert_eq!(windows(&sent).len(), 1, "{sent:?}");
}

#[test]
fn a_press_sends_the_window_again_also_unchanged() {
    let mut rig = Rig::new(true);
    rig.press_with_scene(JAM_STEP, 0);
    assert_eq!(rig.active_view(), Some(Views::Sequencer));
    let first = rig.sent();
    assert_eq!(first.len(), 2, "{first:?}");
    // STEP + the same scene: the same view and window, sent again
    rig.press_with_scene(JAM_STEP, 0);
    assert_eq!(rig.sent(), first);
}

#[test]
fn the_drum_window_has_the_keys_of_the_visible_pads() {
    let mut rig = Rig::new(true);
    rig.sent();
    // STEP + scene 4: the drum sequencer (16 pads)
    rig.press_with_scene(JAM_STEP, 3);
    assert_eq!(rig.active_view(), Some(Views::Drum));
    let offset = rig.ctx().model.scales().drum_offset();
    // 32 steps of 1/16
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::DrumSequencer), Sent::Window(EditWindow { start: 0.0, length: 8.0, low: offset, high: offset + 15 })]);

    // The pads one octave (16 pads) up: the next 16 keys
    rig.press(JAM_NAV_UP);
    let offset = rig.ctx().model.scales().drum_offset();
    assert_eq!(windows(&rig.sent()), [EditWindow { start: 0.0, length: 8.0, low: offset, high: offset + 15 }]);

    // Drum 4: 4 lanes of 16 steps
    rig.press_with_scene(JAM_STEP, 4);
    assert_eq!(rig.active_view(), Some(Views::Drum4));
    let offset = rig.ctx().model.scales().drum_offset();
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::DrumSequencer), Sent::Window(EditWindow { start: 0.0, length: 4.0, low: offset, high: offset + 3 })]);
}

#[test]
fn the_pad_modes_and_the_modes_send_their_kinds() {
    let mut rig = Rig::new(true);
    rig.sent();
    rig.press(JAM_PAD_MODE);
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Play)]);
    // Drum 64 plays (PAD MODE + scene 4)
    rig.press_with_scene(JAM_PAD_MODE, 3);
    assert_eq!(rig.active_view(), Some(Views::Drum64));
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Play)]);
    rig.press(JAM_LEVEL);
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Mixer)]);
    rig.press(JAM_CONTROL);
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Device)]);
}

#[test]
fn nothing_on_a_track_change_or_a_setting_change() {
    let mut rig = Rig::new(true);
    // Before a pick: nothing at all
    rig.app.update_model("track/0/select 0\ntrack/1/select 1\n");
    rig.advance(300);
    rig.app.update_model("track/1/select 0\ntrack/0/select 1\n");
    rig.advance(300);
    rig.ctx().config.set_flip_session(true);
    rig.advance(300);
    assert_eq!(rig.sent(), []);

    // After STEP: no view on a track change
    rig.press(JAM_STEP);
    rig.sent();
    rig.app.update_model("track/0/select 0\ntrack/1/select 1\n");
    rig.advance(300);
    assert!(!rig.sent().iter().any(|s| matches!(s, Sent::View(_))));
}

#[test]
fn another_clip_sends_the_window_again() {
    let mut rig = Rig::new(true);
    rig.press(JAM_STEP);
    let window = windows(&rig.sent());
    assert_eq!(window.len(), 1);
    // The other slot of the track: the same window, but of another clip
    rig.app.update_model("track/0/slot/0/selected 0\ntrack/0/slot/1/selected 1\n");
    rig.advance(300);
    assert_eq!(windows(&rig.sent()), window);
    // A view which is no sequencer: no more windows
    rig.press(JAM_SONG);
    assert_eq!(rig.sent(), [Sent::View(ControllerViewKind::Launcher)]);
    rig.app.update_model("track/0/slot/1/selected 0\ntrack/0/slot/0/selected 1\n");
    rig.advance(300);
    assert_eq!(rig.sent(), []);
}

#[test]
fn nothing_to_a_host_which_does_not_handle_them() {
    let mut rig = Rig::new(false);
    rig.press(JAM_SONG);
    rig.press(JAM_STEP);
    rig.press(JAM_NAV_RIGHT);
    rig.press_with_scene(JAM_STEP, 3);
    rig.press(JAM_LEVEL);
    assert_eq!(rig.active_view(), Some(Views::Drum));
    assert_eq!(rig.sent(), []);
}

/// The views, windows and the commands of the host's step grid (scroll, page, step size) since
/// the last call.
fn sent_with_grid(rig: &Rig) -> Vec<String> {
    let calls = rig.backend.take_calls();
    calls
        .iter()
        .filter_map(|call| match call.cmd()? {
            cmd @ (Cmd::ControllerView { .. } | Cmd::ClipEditWindow { .. } | Cmd::GridScroll { .. } | Cmd::GridPage { .. } | Cmd::GridStepSize { .. }) => Some(format!("{cmd:?}")),
            _ => None,
        })
        .collect()
}

#[test]
fn with_the_host_grid_the_window_follows_the_grid_commands() {
    let mut rig = Rig::new(true);
    rig.press(JAM_STEP);
    assert_eq!(windows(&rig.sent()).len(), 1);
    // The host reports its step grid (zynthian's NoteGrid): from now on the host pages
    rig.app.update_model("clip/grid/8x128/playingStep -1\n");
    rig.advance(60);
    assert!(rig.ctx().model.note_clip(8, 128).is_host_grid());
    assert_eq!(sent_with_grid(&rig), Vec::<String>::new(), "The same window");

    // STEP again (to the poly sequencer and around to the note sequencer): view and window
    for _ in 0..3 {
        rig.press(JAM_STEP);
    }
    rig.backend.take_calls();
    rig.press_with_scene(JAM_STEP, 0);
    assert_eq!(rig.active_view(), Some(Views::Sequencer));
    let sent = rig.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[0], Sent::View(ControllerViewKind::Sequencer));
    let Sent::Window(window) = sent[1] else { panic!("{sent:?}") };
    assert_eq!((window.start, window.length), (0.0, 2.0));

    let page = |start: f64| format!("{:?}", Cmd::ClipEditWindow { window: EditWindow { start, ..window } });
    let grid_page = |next: bool| if next { "GridPage { grid: GridId { steps: 8, rows: 128 }, dir: Next }" } else { "GridPage { grid: GridId { steps: 8, rows: 128 }, dir: Prev }" };
    // Paging: the host's GridPage, then the window of the new page
    rig.press(JAM_NAV_RIGHT);
    assert_eq!(sent_with_grid(&rig), [grid_page(true).to_string(), page(2.0)]);
    rig.press(JAM_NAV_RIGHT);
    assert_eq!(sent_with_grid(&rig), [grid_page(true).to_string(), page(4.0)]);
    rig.press(JAM_NAV_LEFT);
    assert_eq!(sent_with_grid(&rig), [grid_page(false).to_string(), page(2.0)]);
    rig.press(JAM_NAV_LEFT);
    assert_eq!(sent_with_grid(&rig), [grid_page(false).to_string(), page(0.0)]);
    // On the first page the core sends no page back (neither does the host scroll): nothing
    rig.press(JAM_NAV_LEFT);
    assert_eq!(sent_with_grid(&rig), Vec::<String>::new());

    // The step size: the host's GridStepSize, the window of the same page in the new length
    rig.press(JAM_NAV_RIGHT);
    rig.backend.take_calls();
    {
        let ctx = rig.ctx();
        let view = ctx.active_view().unwrap();
        assert!(view.borrow_mut().set_sequencer_resolution_index(ctx, 2));
    }
    rig.advance(60);
    let sent = sent_with_grid(&rig);
    assert_eq!(sent.len(), 2, "{sent:?}");
    let step = rig.ctx().model.note_clip(8, 128).step_length();
    assert_eq!(sent[0], format!("{:?}", Cmd::GridStepSize { grid: drivenbymoss::protocol::GridId { steps: 8, rows: 128 }, value: step }));
    assert_eq!(sent[1], format!("{:?}", Cmd::ClipEditWindow { window: EditWindow { start: 8.0 * step, length: 8.0 * step, ..window } }));
    assert_ne!(step, 0.25);
}
