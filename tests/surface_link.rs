// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the surface link with two controllers in one process (doc/SURFACE_LINK.md): the
//! Electra One as the provider and the Maschine JAM as the surface, each in its own App (as two
//! Bitwig extensions or two controllers of the Python host). Both start orders, the snapshot and
//! the changes on the JAM, the JAM's input as typed commands and as MIDI/SysEx to the Electra,
//! pages and rows, detach and re-attach, the Electra-side extension (electra-one
//! `docs/surface-link.md` §6b), and the generic provider for other presets (§6c).
//!
//! The registry is process-wide, so the tests of this file run one after the other.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordedCalls, RecordingBackend};
use drivenbymoss::controller::electra::surface as electra;
use drivenbymoss::controller::maschine::jam::surface_link as jam_surface;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{Modes, Views};
use drivenbymoss::framework::page_surface::view::palette_index;
use drivenbymoss::framework::surface_link::{LinkState, Rgb, SlotKind};
use drivenbymoss::protocol::{Chan, Cmd, ValueOp};

static SERIAL: Mutex<()> = Mutex::new(());

// The Electra One: Port 1 and CTRL
const E_IN: i32 = 3;
const E_CTRL_IN: i32 = 4;
const E_OUT: i32 = 5;
const E_CTRL_OUT: i32 = 6;
// The JAM
const J_IN: i32 = 13;
const J_OUT: i32 = 15;

const JAM_SELECT: u8 = 50;
const JAM_SONG: u8 = 30;
const JAM_KNOB_TURN: u8 = 57;
const JAM_KNOB_PUSH: u8 = 58;
const JAM_LEVEL: u8 = 37;
const JAM_NAV_DOWN: u8 = 42;
const JAM_NAV_LEFT: u8 = 43;
const JAM_NAV_RIGHT: u8 = 44;
const JAM_RIGHT: u8 = 93;
const JAM_FADER1: u8 = 8;
const JAM_FADER_TOUCH1: u8 = 20;

const MODEL: &str = "track/count 3\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volume/str 0 dB\ntrack/0/color 255 0 0\n\
track/1/exists 1\ntrack/1/name Bass\ntrack/1/mute 1\ntrack/1/color 0 0 255\ntrack/2/exists 1\ntrack/2/name Keys\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\nplay 0\n";

const INFO: &str = r#"{"versionText":"v5.0.6","versionSeq":500000600,"model":"mk2"}"#;
const INFO_SURFACE: &str = r#"{"versionText":"v5.0.6","versionSeq":500000600,"model":"mk2","surface":{"rows":true,"selection":true}}"#;
const INFO_GENERIC: &str = r#"{"versionText":"v5.0.6","versionSeq":500000600,"model":"mk2","surface":{"rows":true,"selection":true,"view":true,"input":true}}"#;
/// The page view of another preset (electra-one's fixture `surface-view.epr`).
const PAGE_VIEW: &str = include_str!("fixtures/electra_page_view.json");

fn sysex(command: &[u8], content: &[u8]) -> Vec<u8> {
    let mut data = vec![0xF0, 0x00, 0x21, 0x45];
    data.extend_from_slice(command);
    data.extend_from_slice(content);
    data.push(0xF7);
    data
}

fn config_dir(name: &str, class: &str) -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-link-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={class}\n")).unwrap();
    dir.to_str().unwrap().to_string()
}

/// The two controllers and the time.
struct Rig {
    electra: Option<(App, Rc<RecordingBackend>)>,
    jam: Option<(App, Rc<RecordingBackend>)>,
    time: u64,
}

impl Rig {
    fn new() -> Self {
        Rig { electra: None, jam: None, time: 0 }
    }

    /// Start the Electra and bring it online (with or without the surface capability).
    fn start_electra(&mut self, info: &str) {
        let backend = Rc::new(RecordingBackend {
            inputs: vec![(E_IN, "Electra Controller Electra Port".into()), (E_CTRL_IN, "Electra Controller Electra CTRL".into())],
            outputs: vec![(E_OUT, "Electra Controller Electra Port".into()), (E_CTRL_OUT, "Electra Controller Electra CTRL".into())],
            ..Default::default()
        });
        let mut app = App::new(backend.clone(), &config_dir("electra", "de.mossgrabers.reaper.controller.electra.one.ElectraOneControllerInstance"), 7, 22);
        app.set_time_ms(self.time);
        app.start_infrastructure();
        app.update_model(MODEL);
        self.electra = Some((app, backend));
        self.advance(1100);
        let app = self.electra_app();
        app.on_midi_event(E_CTRL_IN, &sysex(&[0x01, 0x7F], info.as_bytes()));
        let presets = r#"{"version":1,"presets":[{"slot":3,"bankNumber":1,"name":"DrivenByMoss"},{"slot":4,"bankNumber":1,"name":"SURFACE VIEW"}]}"#;
        app.on_midi_event(E_CTRL_IN, &sysex(&[0x01, 0x04], presets.as_bytes()));
        app.on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 3]));
        assert!(electra::is_online(self.electra_ctx()));
        self.advance(60);
    }

    fn start_jam(&mut self) {
        let backend = Rc::new(RecordingBackend { inputs: vec![(J_IN, "Maschine Jam - 1".into())], outputs: vec![(J_OUT, "Maschine Jam - 1".into())], ..Default::default() });
        let mut app = App::new(backend.clone(), &config_dir("jam", "de.mossgrabers.reaper.controller.ni.maschine.jam.MaschineJamControllerInstance"), 7, 22);
        app.set_time_ms(self.time);
        app.start_infrastructure();
        app.update_model(MODEL);
        self.jam = Some((app, backend));
        self.advance(1100);
    }

    fn electra_app(&mut self) -> &mut App {
        &mut self.electra.as_mut().unwrap().0
    }

    fn electra_backend(&self) -> Rc<RecordingBackend> {
        self.electra.as_ref().unwrap().1.clone()
    }

    fn electra_ctx(&mut self) -> &mut Ctx {
        self.electra_app().instance_ctx(0).unwrap()
    }

    fn jam_app(&mut self) -> &mut App {
        &mut self.jam.as_mut().unwrap().0
    }

    fn jam_backend(&self) -> Rc<RecordingBackend> {
        self.jam.as_ref().unwrap().1.clone()
    }

    fn jam_ctx(&mut self) -> &mut Ctx {
        self.jam_app().instance_ctx(0).unwrap()
    }

    /// Advance the time of both in ticks of 30ms.
    fn advance(&mut self, ms: u64) {
        let end = self.time + ms;
        while self.time < end {
            self.time = (self.time + 30).min(end);
            for (app, _) in [&mut self.electra, &mut self.jam].into_iter().flatten() {
                app.set_time_ms(self.time);
                app.tick();
            }
        }
    }

    fn jam_cc(&mut self, control: u8, value: u8) {
        self.jam_app().on_midi_event(J_IN, &[0xB0, control, value]);
    }

    fn jam_press(&mut self, control: u8) {
        self.jam_cc(control, 127);
        self.jam_cc(control, 0);
    }

    /// A pad of the JAM grid (x, y from the top left).
    fn jam_pad(&mut self, x: u8, y: u8) {
        let note = 36 + (7 - y) * 8 + x;
        self.jam_app().on_midi_event(J_IN, &[0x90, note, 127]);
        self.jam_app().on_midi_event(J_IN, &[0x80, note, 0]);
    }

    /// Select a row on the JAM's surface (the layout has no row selectors: a cell press or
    /// UP/DOWN select rows; this sends the same input as the row selectors did).
    fn jam_select_row(&mut self, row: usize) {
        jam_surface::client(self.jam_ctx()).borrow_mut().select_row(row);
    }

    /// Select + SONG.
    fn toggle_surface(&mut self) {
        self.jam_cc(JAM_SELECT, 127);
        self.jam_press(JAM_SONG);
        self.jam_cc(JAM_SELECT, 0);
        self.advance(30);
    }

    /// The color of a pad of the JAM grid.
    fn jam_pad_color(&mut self, x: i32, y: i32) -> i32 {
        let note = 36 + (7 - y) * 8 + x;
        self.jam_ctx().surface.grid_ref().light_info(note).color
    }

    fn jam_state(&mut self) -> LinkState {
        jam_surface::client(self.jam_ctx()).borrow().state()
    }

    /// The JAM's mirror of a slot.
    fn jam_slot(&mut self, id: usize) -> drivenbymoss::framework::surface_link::Slot {
        jam_surface::client(self.jam_ctx()).borrow().model().slots[id].clone()
    }
}

/// The messages sent to a port.
fn sent(calls: &[BackendCall], port: i32) -> Vec<Vec<u8>> {
    calls.iter().filter_map(|c| if let BackendCall::SendMidi(p, d) = c { (*p == port).then(|| d.clone()) } else { None }).collect()
}

/// The last value of a CC sent to a port.
fn last_cc(calls: &[BackendCall], port: i32, status: u8, control: u8) -> Option<u8> {
    sent(calls, port).into_iter().rev().find_map(|d| (d.len() == 3 && d[0] == status && d[1] == control).then_some(d[2]))
}

/// True if the app notified the text (since the last call).
fn notified(app: &mut App, text: &str) -> bool {
    app.take_notifications().contains(text)
}

/// The snapshot, the colors and values, input as commands and MIDI, rows, pages.
fn check_linked(rig: &mut Rig) {
    assert_eq!(rig.jam_state(), LinkState::Attached);
    rig.jam_backend().take_calls();
    rig.toggle_surface();
    assert!(rig.jam_ctx().views.is_active(Views::SurfaceLink));
    assert!(rig.jam_ctx().modes.is_active(Modes::SurfaceLink));
    assert!(notified(rig.jam_app(), "Surface: Electra One - Mixer"));
    rig.advance(60);

    // The snapshot: the mixer page
    let volume = rig.jam_slot(0);
    assert_eq!((volume.kind, volume.name.as_str(), volume.color, volume.enabled), (SlotKind::Fader, "VOLUME", Rgb::new(255, 0, 0), true));
    assert_eq!(volume.value_text, "0 dB");
    assert_eq!(rig.jam_slot(19).kind, SlotKind::Pad);
    assert_eq!(rig.jam_slot(19).name, "MUTE");
    // A track which does not exist
    assert!(!rig.jam_slot(4).enabled, "{:?}", rig.jam_slot(4));

    // The slots in the Electra's colors: track 1 red, track 2 blue, the master column
    let red = palette_index(rig.jam_ctx(), Rgb::new(255, 0, 0));
    let blue = palette_index(rig.jam_ctx(), Rgb::new(0, 0, 255));
    assert_ne!(red, 0);
    assert_ne!(red, blue);
    assert_eq!(rig.jam_pad_color(0, 1), red);
    assert_eq!(rig.jam_pad_color(1, 1), blue);
    assert_eq!(rig.jam_pad_color(4, 1), 0, "track 5 does not exist");
    // The page list (grid row 1): 6 of the 7 pages, the current one orange; its last two pads
    // the previous (none) and the next page
    use drivenbymoss::controller::maschine::core::colors::{COLOR_AMBER_LO, COLOR_ORANGE};
    assert_eq!(rig.jam_pad_color(0, 0), COLOR_ORANGE);
    assert_eq!(rig.jam_pad_color(1, 0), COLOR_AMBER_LO);
    assert_ne!(rig.jam_pad_color(5, 0), 0);
    assert_eq!(rig.jam_pad_color(6, 0), 0);
    assert_ne!(rig.jam_pad_color(7, 0), 0);
    // The 6-wide page leaves columns 7 and 8 and the section row dark
    assert_eq!(rig.jam_pad_color(6, 1), 0);
    assert_eq!(rig.jam_pad_color(7, 3), 0);
    assert_eq!(rig.jam_pad_color(0, 7), 0);

    // Strip 1 shows the volume of track 1 (0.5 of the Electra's 0..127) in its color; strip 7
    // (the selected slot) and 8 (no extras) are off
    let strips = jam_surface::fader_configs(rig.jam_ctx()).unwrap();
    assert!((62..=65).contains(&strips[0].value), "{strips:?}");
    assert_eq!(strips[0].color, red);
    assert_eq!(strips[1].color, blue);
    assert_eq!((strips[6].value, strips[7].value), (0, 0));
    rig.jam_backend().take_calls();

    // Strip 1 turns the volume of track 1: on the Electra as its knob (CC 10 on channel 16)
    rig.electra_backend().take_calls();
    rig.jam_cc(JAM_FADER_TOUCH1, 127);
    rig.jam_cc(JAM_FADER1, 60);
    rig.jam_cc(JAM_FADER1, 64);
    rig.jam_cc(JAM_FADER_TOUCH1, 0);
    rig.advance(30);
    let calls = rig.electra_backend().take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { p: 0, .. }, op: ValueOp::Set(_) | ValueOp::Inc(_) })), "{calls:?}");
    // The touch of the strip touches the parameter
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Track { p: 0, .. }, op: ValueOp::Touch(true) })), "{calls:?}");

    // The DAW reports the new volume: the Electra's knob and the JAM's strip follow
    rig.electra_app().update_model("track/0/volume 0.75\ntrack/0/volume/str +3 dB\n");
    rig.advance(60);
    let calls = rig.electra_backend().take_calls();
    assert_eq!(last_cc(&calls, E_OUT, 0xBF, 10), Some(95));
    assert_eq!(rig.jam_slot(0).value_text, "+3 dB");
    assert_eq!(last_cc(&rig.jam_backend().take_calls(), J_OUT, 0xB0, JAM_FADER1), Some(95));

    // A pad slot (row 4, column 2: mute of track 2): its pad CC on the Electra
    rig.jam_pad(1, 4);
    rig.advance(30);
    let calls = rig.electra_backend().take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelMute { ch: Chan::Track { p: 1, .. }, .. })), "{calls:?}");

    // Row 3 (control set 2)
    rig.jam_select_row(2);
    rig.advance(60);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().active_row, 2);

    // Select the master volume (row 1, column 6) with its pad: row 1 again, the main encoder
    // turns it
    rig.jam_pad(5, 1);
    rig.advance(60);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().selected_slot, Some(5));
    rig.electra_backend().take_calls();
    rig.jam_cc(JAM_KNOB_TURN, 1);
    rig.advance(30);
    assert!(rig.electra_backend().take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Master, .. })));
    // Its push resets it (to the default value without the host feature valueReset)
    rig.jam_press(JAM_KNOB_PUSH);
    rig.advance(30);
    let calls = rig.electra_backend().take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelVolume { ch: Chan::Master, op: ValueOp::Reset | ValueOp::Set(_) })), "{calls:?}");

    // The page list: the sends page; the Electra switches its mode and its page (09 0A 01)
    rig.jam_pad(1, 0);
    rig.advance(60);
    assert!(rig.electra_ctx().modes.is_active(Modes::Send));
    assert!(sent(&rig.electra_backend().take_calls(), E_CTRL_OUT).contains(&sysex(&[0x09, 0x0A], &[1])));
    let model = jam_surface::client(rig.jam_ctx()).borrow().model().clone();
    assert_eq!((model.page, model.page_name(), model.active_row, model.selected_slot), (1, "Sends", 0, None));
    assert_eq!(model.slots[0].name, "FX1");
    // PAGE RIGHT: the device page
    rig.jam_press(JAM_RIGHT);
    rig.advance(60);
    assert!(rig.electra_ctx().modes.is_active(Modes::DeviceParams));

    // A page switch on the Electra itself reaches the JAM
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x06], &[4]));
    rig.advance(60);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().page_name(), "Transport");

    // LEVEL leaves the surface for the volume mode; the view comes back
    rig.jam_press(JAM_LEVEL);
    rig.advance(30);
    assert!(rig.jam_ctx().modes.is_active(Modes::Volume));
    assert!(!rig.jam_ctx().views.is_active(Views::SurfaceLink));
    assert!(!jam_surface::is_active(rig.jam_ctx()));
}

#[test]
fn electra_first() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = Rig::new();
    rig.start_electra(INFO);
    rig.start_jam();
    check_linked(&mut rig);
}

#[test]
fn jam_first() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = Rig::new();
    rig.start_jam();
    assert_eq!(rig.jam_state(), LinkState::Missing);
    // Without a provider: the surface shows a red cross and tells it
    rig.jam_backend().take_calls();
    rig.toggle_surface();
    assert!(notified(rig.jam_app(), "Surface: no provider"));
    rig.advance(30);
    assert_ne!(rig.jam_pad_color(0, 1), 0);
    assert_eq!(rig.jam_pad_color(1, 1), 0);
    assert_ne!(rig.jam_pad_color(1, 2), 0);
    // Input goes nowhere
    rig.jam_pad(0, 1);
    rig.toggle_surface();
    assert!(rig.jam_ctx().views.is_active(Views::Play));

    rig.start_electra(INFO);
    check_linked(&mut rig);
}

#[test]
fn detach_and_reattach() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = Rig::new();
    rig.start_electra(INFO);
    rig.start_jam();
    rig.toggle_surface();
    assert!(jam_surface::is_active(rig.jam_ctx()));

    // The Electra goes away: the JAM falls back to its own view and mode
    rig.jam_backend().take_calls();
    if let Some((mut app, _)) = rig.electra.take() {
        app.shutdown();
    }
    rig.advance(30);
    assert_eq!(rig.jam_state(), LinkState::Detached);
    assert!(!jam_surface::is_active(rig.jam_ctx()));
    assert!(rig.jam_ctx().views.is_active(Views::Play));
    assert!(rig.jam_ctx().modes.is_active(Modes::Volume));
    assert!(notified(rig.jam_app(), "Surface: provider detached"));

    // A new Electra: attached again
    rig.start_electra(INFO);
    assert_eq!(rig.jam_state(), LinkState::Attached);
    check_linked(&mut rig);
}

#[test]
fn a_second_electra_does_not_provide() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = Rig::new();
    rig.start_electra(INFO);
    let first = rig.electra.take().unwrap();
    rig.start_electra(INFO);
    let second = rig.electra.take().unwrap();
    rig.electra = Some(first);
    rig.start_jam();
    assert_eq!(rig.jam_state(), LinkState::Attached);
    drop(second);
    rig.advance(30);
    assert_eq!(rig.jam_state(), LinkState::Attached);
}

// ---------------------------------------------------------------------------------------------
// The Electra-side extension (electra-one docs/surface-link.md §6b)

/// Enter the surface and clear the calls.
fn linked_with(info: &str) -> Rig {
    let mut rig = Rig::new();
    rig.start_electra(info);
    rig.start_jam();
    rig.toggle_surface();
    rig.advance(60);
    rig.electra_backend().take_calls();
    rig
}

fn ctrl_sent(rig: &Rig) -> Vec<Vec<u8>> {
    sent(&rig.electra_backend().take_calls(), E_CTRL_OUT)
}

#[test]
fn capability_absent_switches_control_sets() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = linked_with(INFO);
    // Row 2 is in the control set of row 1: nothing
    rig.jam_select_row(1);
    rig.advance(30);
    assert!(!ctrl_sent(&rig).iter().any(|m| m[4] == 0x09 || m[4..6] == [0x14, 0x51]));
    // Row 3: the control set 2 (09 0B 01)
    rig.jam_press(JAM_NAV_DOWN);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x0B], &[1])]);
    // A selection is not shown; its row is in the control set 1
    rig.jam_pad(1, 2);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x0B], &[0])]);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().selected_slot, Some(7));
}

#[test]
fn capability_present() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = Rig::new();
    rig.start_jam();
    // The subscription includes the control set events, online the row and selection are read
    rig.start_electra(INFO_SURFACE);
    let calls = rig.electra_backend().take_calls();
    let ctrl = sent(&calls, E_CTRL_OUT);
    assert!(ctrl.contains(&sysex(&[0x14, 0x79], &[0x3B])), "{ctrl:02X?}");
    assert!(ctrl.contains(&sysex(&[0x02, 0x50], &[])) && ctrl.contains(&sysex(&[0x02, 0x51], &[])));
    // The answers: row 2, nothing selected
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x01, 0x50], &[2]));
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x01, 0x51], &[0, 0]));
    rig.advance(60);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().active_row, 1);

    rig.toggle_surface();
    rig.advance(30);
    rig.electra_backend().take_calls();
    // Row 4: 09 50 04
    rig.jam_select_row(3);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[4])]);
    // Select row 1, column 3 (control 3): the row and the highlight
    rig.jam_pad(2, 1);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[1]), sysex(&[0x14, 0x51], &[3, 0])]);
    // Their echoes (origin 1) change nothing
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x60], &[1, 1]));
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x61], &[3, 0, 1]));
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x01], &[]));
    rig.advance(30);
    let model = jam_surface::client(rig.jam_ctx()).borrow().model().clone();
    assert_eq!((model.active_row, model.selected_slot), (0, Some(2)));

    // Changes on the device (origin 0) and by Lua (origin 2) go to the surfaces
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x60], &[6, 0]));
    rig.advance(30);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().active_row, 5);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x61], &[8, 0, 2]));
    rig.advance(30);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().selected_slot, Some(7));
    // A control of another page is not a selection of this one
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x61], &[40, 0, 0]));
    rig.advance(30);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().selected_slot, Some(7));
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x61], &[0, 0, 0]));
    rig.advance(30);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().selected_slot, None);

    // A page switch on the device resets the row and the selection
    rig.jam_pad(1, 2);
    rig.advance(30);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x06], &[1]));
    rig.advance(30);
    let model = jam_surface::client(rig.jam_ctx()).borrow().model().clone();
    assert_eq!((model.page, model.active_row, model.selected_slot), (1, 0, None));
    // Selections are control IDs of the current page (page 2: 37 + 7)
    rig.electra_backend().take_calls();
    rig.jam_pad(1, 2);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[2]), sysex(&[0x14, 0x51], &[44, 0])]);
}

#[test]
fn capability_nack() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = linked_with(INFO_SURFACE);
    // A NACK for the selection: it stays what it was
    rig.jam_pad(0, 1);
    rig.advance(30);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x61], &[1, 0, 1]));
    rig.jam_pad(1, 1);
    rig.advance(30);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x00], &[]));
    rig.advance(60);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().selected_slot, Some(0));
    // A NACK for a row: the device has no rows after all, back to the control sets
    rig.electra_backend().take_calls();
    rig.jam_select_row(2);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[3])]);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x00], &[]));
    rig.advance(30);
    assert_eq!(jam_surface::client(rig.jam_ctx()).borrow().model().active_row, 0);
    rig.jam_select_row(4);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x0B], &[2])]);
}

// ---------------------------------------------------------------------------------------------
// The generic provider for other presets (electra-one docs/surface-link.md §6c)

/// The page view message of the fixture, with some replacements.
fn page_view(replacements: &[(&str, &str)]) -> Vec<u8> {
    let mut json = PAGE_VIEW.trim().to_string();
    for (from, to) in replacements {
        assert!(json.contains(from), "{from}");
        json = json.replacen(from, to, 1);
    }
    sysex(&[0x01, 0x52], json.as_bytes())
}

fn jam_model(rig: &mut Rig) -> drivenbymoss::framework::surface_link::PageModel {
    jam_surface::client(rig.jam_ctx()).borrow().model().clone()
}

/// The JAM on the surface, the Electra on the other preset (slot 1-4) with its page view.
fn generic_rig() -> Rig {
    let mut rig = Rig::new();
    rig.start_electra(INFO_GENERIC);
    rig.start_jam();
    rig.toggle_surface();
    rig.electra_backend().take_calls();
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 4]));
    rig.advance(30);
    let ctrl = ctrl_sent(&rig);
    assert!(ctrl.contains(&sysex(&[0x14, 0x79], &[0x3B, 0x01])), "{ctrl:02X?}");
    assert_eq!(ctrl.iter().filter(|m| **m == sysex(&[0x02, 0x52], &[])).count(), 1, "{ctrl:02X?}");
    rig.electra_app().on_midi_event(E_CTRL_IN, &page_view(&[]));
    rig.advance(60);
    rig
}

#[test]
fn generic_mode_switch() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = generic_rig();
    // Offline for the DAW, the pages of the other preset on the JAM
    assert!(!electra::is_online(rig.electra_ctx()));
    assert!(rig.electra_ctx().modes.is_active(Modes::Dummy));
    assert!(jam_surface::is_active(rig.jam_ctx()));
    let model = jam_model(&mut rig);
    assert_eq!((model.pages.clone(), model.page), (vec!["VIEW".to_string(), "OTHER".to_string()], 0));
    assert_eq!(model.slots[0].name, "ENV · Attack");
    assert_eq!(model.slots[8].value_text, "Cut 64");
    assert_eq!(rig.jam_pad_color(0, 1), palette_index(rig.jam_ctx(), Rgb::new(0x03, 0xA5, 0x98)));

    // Another non-DrivenByMoss preset: the page view again
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 5]));
    rig.advance(30);
    assert!(ctrl_sent(&rig).contains(&sysex(&[0x02, 0x52], &[])));

    // The DrivenByMoss preset: the value changes off, the mixer page again
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 3]));
    rig.advance(60);
    assert!(electra::is_online(rig.electra_ctx()));
    assert!(ctrl_sent(&rig).contains(&sysex(&[0x14, 0x79], &[0x3B, 0x00])));
    let model = jam_model(&mut rig);
    assert_eq!((model.page_name(), model.slots[0].name.as_str()), ("Mixer", "VOLUME"));
    // A page view now is not for the surfaces
    rig.electra_app().on_midi_event(E_CTRL_IN, &page_view(&[]));
    rig.advance(30);
    assert_eq!(jam_model(&mut rig).page_name(), "Mixer");
}

#[test]
fn generic_needs_the_capability() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = linked_with(INFO_SURFACE);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x02], &[1, 4]));
    rig.advance(60);
    assert!(!ctrl_sent(&rig).iter().any(|m| m[4..6] == [0x02, 0x52] || m[4..6] == [0x14, 0x79]));
    assert_eq!(jam_model(&mut rig).page_name(), "Offline");
}

#[test]
fn generic_without_the_drivenbymoss_preset() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = Rig::new();
    rig.start_jam();
    let backend = Rc::new(RecordingBackend {
        inputs: vec![(E_IN, "Electra Controller Electra Port".into()), (E_CTRL_IN, "Electra Controller Electra CTRL".into())],
        outputs: vec![(E_OUT, "Electra Controller Electra Port".into()), (E_CTRL_OUT, "Electra Controller Electra CTRL".into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), &config_dir("electra", "de.mossgrabers.reaper.controller.electra.one.ElectraOneControllerInstance"), 7, 22);
    app.start_infrastructure();
    rig.electra = Some((app, backend));
    rig.advance(1100);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x01, 0x7F], INFO_GENERIC.as_bytes()));
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x01, 0x04], br#"{"version":1,"presets":[{"slot":4,"bankNumber":1,"name":"SURFACE VIEW"}]}"#));
    rig.advance(30);
    assert!(ctrl_sent(&rig).contains(&sysex(&[0x02, 0x52], &[])));
    rig.electra_app().on_midi_event(E_CTRL_IN, &page_view(&[]));
    rig.advance(60);
    assert_eq!(jam_model(&mut rig).slots[2].name, "WAVE");
}

#[test]
fn generic_input() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = generic_rig();
    rig.electra_backend().take_calls();
    let json = |c: &[u8], text: &str| sysex(c, text.as_bytes());

    // Strip 1: the envelope's knob 1, touched, turned by 4 (16 fine steps), let go
    rig.jam_cc(JAM_FADER_TOUCH1, 127);
    rig.jam_cc(JAM_FADER1, 60);
    rig.jam_cc(JAM_FADER1, 64);
    rig.jam_cc(JAM_FADER_TOUCH1, 0);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x14, 0x54], &[1, 1]), sysex(&[0x14, 0x53], &[1, 16, 0, 1]), sysex(&[0x14, 0x54], &[1, 0])]);
    // Down by 1: two's complement
    rig.jam_cc(JAM_FADER1, 64);
    rig.jam_cc(JAM_FADER1, 63);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x14, 0x53], &[1, 0x7C, 0x7F, 1])]);

    // Select the envelope; left / right switch its handle, the push resets the handle's value
    rig.jam_pad(0, 1);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x14, 0x51], &[1, 0])]);
    rig.jam_press(JAM_NAV_RIGHT);
    rig.jam_press(JAM_NAV_LEFT);
    rig.jam_press(JAM_KNOB_PUSH);
    rig.advance(30);
    assert_eq!(
        ctrl_sent(&rig),
        [
            json(&[0x14, 0x56], r#"{"controlId":1,"next":true}"#),
            json(&[0x14, 0x56], r#"{"controlId":1,"prev":true}"#),
            json(&[0x14, 0x55], r#"{"controlId":1,"valueId":"attack","default":true}"#)
        ]
    );
    // The device switched the handle: 7E 62, the page view again
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x62], &[1]));
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x02, 0x52], &[])]);
    rig.electra_app().on_midi_event(
        E_CTRL_IN,
        &page_view(&[(r#""activeValueId":"attack""#, r#""activeValueId":"decay""#), (r#"{"potId":1,"controlId":1,"valueId":"attack"}"#, r#"{"potId":1,"controlId":1,"valueId":"decay"}"#), (r#""selectedControlId":0"#, r#""selectedControlId":1"#)]),
    );
    rig.advance(60);
    let model = jam_model(&mut rig);
    assert_eq!((model.slots[0].name.as_str(), model.slots[0].value_text.as_str(), model.selected_slot), ("ENV · Decay", "20", Some(0)));
    // The main encoder turns the selected handle, with Shift fine
    rig.jam_cc(JAM_KNOB_TURN, 1);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x14, 0x53], &[1, 4, 0, 1])]);
    // A control with one value has no handles
    rig.jam_pad(2, 2);
    rig.jam_press(JAM_NAV_RIGHT);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[2]), sysex(&[0x14, 0x51], &[3, 0])]);

    // A list: selected and stepped to the next item; after the last item the first one
    rig.jam_pad(2, 1);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[1]), sysex(&[0x14, 0x51], &[2, 0]), json(&[0x14, 0x55], r#"{"controlId":2,"valueId":"value","step":1}"#)]);
    rig.electra_app().on_midi_event(E_CTRL_IN, &json(&[0x7E, 0x63], r#"{"controlId":2,"valueId":"value","value":3,"midiValue":96,"origin":"internal","text":"Sine"}"#));
    rig.advance(30);
    assert_eq!(jam_model(&mut rig).slots[2].value_text, "Sine");
    rig.jam_pad(2, 1);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [json(&[0x14, 0x55], r#"{"controlId":2,"valueId":"value","value":0}"#)]);

    // A Custom control: each knob its own value (strip 5 = knob 5, value b)
    rig.jam_cc(JAM_FADER1 + 4, 10);
    rig.jam_cc(JAM_FADER1 + 4, 11);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x14, 0x53], &[5, 4, 0, 1])]);
    // A covered cell of the envelope selects the envelope
    rig.jam_pad(1, 2);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x14, 0x51], &[1, 0])]);

    // A toggle pad of control set 3 (row 5, value 1): off, with the row first
    rig.jam_pad(1, 5);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [json(&[0x14, 0x55], r#"{"controlId":10,"valueId":"value","value":0}"#)]);

    // Select the fader of control set 2: its row first; the main encoder then turns its knob 1
    rig.jam_pad(0, 3);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[3]), sysex(&[0x14, 0x51], &[8, 0])]);
    rig.jam_cc(JAM_KNOB_TURN, 127);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x14, 0x53], &[1, 0x7C, 0x7F, 1])]);
    // The device goes back to row 1 (a button): the next turn of the selection switches first
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x60], &[1, 0]));
    rig.advance(30);
    assert_eq!(jam_model(&mut rig).active_row, 0);
    rig.jam_cc(JAM_KNOB_TURN, 1);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[3]), sysex(&[0x14, 0x53], &[1, 4, 0, 1])]);
    // A row and the hidden slot (knob 11 of set 1) is not turned
    rig.jam_select_row(1);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x50], &[2])]);
    rig.jam_cc(JAM_FADER1 + 4, 20);
    rig.jam_cc(JAM_FADER1 + 4, 30);
    rig.advance(30);
    assert!(ctrl_sent(&rig).is_empty());

    // The next page: 09 0A with the page index
    rig.jam_press(JAM_RIGHT);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x0A], &[1])]);
}

#[test]
fn generic_events_and_pages() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = generic_rig();
    rig.electra_backend().take_calls();

    // A value change: the slot and the strip follow
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x63], br#"{"controlId":1,"valueId":"attack","value":127,"midiValue":127,"origin":"midi","text":"127"}"#));
    rig.advance(60);
    assert_eq!((jam_model(&mut rig).slots[0].value, jam_model(&mut rig).slots[0].value_text.as_str().to_string()), (1.0, "127".to_string()));
    assert_eq!(jam_surface::fader_configs(rig.jam_ctx()).unwrap()[0].value, 127);
    // Of a value no slot shows (the envelope's release): nothing changes on the slots
    let before = jam_model(&mut rig);
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x63], br#"{"controlId":1,"valueId":"release","value":1,"midiValue":1,"origin":"midi","text":"1"}"#));
    rig.advance(60);
    assert_eq!(jam_model(&mut rig), before);

    // 7E 62 twice: one request, the second after the answer
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x62], &[1]));
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x62], &[1]));
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x02, 0x52], &[])]);
    rig.electra_app().on_midi_event(E_CTRL_IN, &page_view(&[(r#""name":"CUTOFF""#, r#""name":"FREQ""#)]));
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x02, 0x52], &[])]);
    assert_eq!(jam_model(&mut rig).slots[8].name, "FREQ");
    rig.electra_app().on_midi_event(E_CTRL_IN, &page_view(&[]));
    rig.advance(30);
    assert!(ctrl_sent(&rig).is_empty());
    // An answer which does not come is asked for again (three times)
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x62], &[1]));
    rig.advance(1200);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x02, 0x52], &[]), sysex(&[0x02, 0x52], &[])]);
    rig.advance(5000);
    assert_eq!(ctrl_sent(&rig).len(), 2);
    rig.electra_app().on_midi_event(E_CTRL_IN, &page_view(&[]));
    rig.advance(1200);
    assert!(ctrl_sent(&rig).is_empty());

    // A page switch on the device: the page view of page 2
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x06], &[1]));
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x02, 0x52], &[])]);
    rig.electra_app().on_midi_event(E_CTRL_IN, &page_view(&[(r#""page":{"id":1,"name":"VIEW"}"#, r#""page":{"id":2,"name":"OTHER"}"#)]));
    rig.advance(60);
    let model = jam_model(&mut rig);
    assert_eq!((model.page, model.page_name()), (1, "OTHER"));
    // The page list of the JAM: page 1 is VIEW
    rig.jam_pad(0, 0);
    rig.advance(30);
    assert_eq!(ctrl_sent(&rig), [sysex(&[0x09, 0x0A], &[0])]);
    // Rows and selections of the device (origin 0) reach the surface
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x60], &[5, 0]));
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x61], &[3, 0, 0]));
    rig.advance(60);
    let model = jam_model(&mut rig);
    assert_eq!((model.active_row, model.selected_slot), (4, Some(8)));
}

/// NI's palette has hues × levels: the slots take the hue of their color and the level of their
/// role (normal full, selected pale, disabled and covered dim; inactive selectors medium).
#[test]
fn jam_slot_roles() {
    use drivenbymoss::controller::maschine::core::colors::*;
    use drivenbymoss::framework::color::ColorRole;
    use drivenbymoss::framework::page_surface::view::palette_index_for_role;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rig = generic_rig();
    // Normal slots: full (ENV teal 03A598 is hue 9, WAVE orange F49500 hue 3, CUTOFF 529DEC hue 10)
    assert_eq!(rig.jam_pad_color(0, 1), 38, "ENV, teal full");
    assert_eq!(rig.jam_pad_color(2, 1), 14, "WAVE, orange full");
    assert_eq!(rig.jam_pad_color(2, 2), 42, "CUTOFF, blue full");
    assert_eq!(rig.jam_pad_color(3, 1), COLOR_WHITE, "GRID, white");
    // Covered cells (the envelope's other cells): the dim teal
    assert!(!rig.jam_slot(1).enabled);
    assert_eq!(rig.jam_pad_color(1, 1), 36, "covered: teal dim");
    assert_eq!(rig.jam_pad_color(0, 2), 36);
    // A hidden control: dim (SPARE F45C51 is hue 2)
    assert_eq!((rig.jam_slot(10).name.as_str(), rig.jam_slot(10).enabled), ("SPARE", false));
    assert_eq!(rig.jam_pad_color(4, 2), 8, "disabled: dim");
    // Empty cells stay off
    assert_eq!(rig.jam_pad_color(3, 2), 0);
    // The selected slot: pale, blinking white
    rig.electra_app().on_midi_event(E_CTRL_IN, &sysex(&[0x7E, 0x61], &[3, 0, 0]));
    rig.advance(60);
    assert_eq!(jam_model(&mut rig).selected_slot, Some(8));
    let note = 36 + (7 - 2) * 8 + 2;
    let info = rig.jam_ctx().surface.grid_ref().light_info(note);
    assert_eq!((info.color, info.blink_color), (43, COLOR_WHITE), "selected: blue pale");
    // The roles of an RGB color on the JAM
    let ctx = rig.jam_ctx();
    let teal = Rgb::new(0x03, 0xA5, 0x98);
    let levels = [ColorRole::Disabled, ColorRole::Inactive, ColorRole::Normal, ColorRole::Highlight].map(|role| palette_index_for_role(ctx, teal, role));
    assert_eq!(levels, [36, 37, 38, 39]);
    assert_eq!(palette_index_for_role(ctx, Rgb::new(0, 0, 0), ColorRole::Normal), 0, "black is off");
}
