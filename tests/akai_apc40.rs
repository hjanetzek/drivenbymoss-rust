// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Akai APC40 Mk1 and Mk2: startup, pads, faders and knobs, track buttons, modes,
//! the shift view and all views and modes.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 3\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/2/exists 1\ntrack/2/name Keys\ntrack/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 3\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\ntrack/0/send/2/name FX3\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\nplay 0\n";

#[derive(Clone, Copy, PartialEq)]
enum Model {
    Mk1,
    Mk2,
}

impl Model {
    fn class(self) -> &'static str {
        match self {
            Model::Mk1 => "APC40mkIControllerInstance",
            Model::Mk2 => "APC40mkIIControllerInstance",
        }
    }

    fn port(self) -> &'static str {
        match self {
            Model::Mk1 => "Akai APC40",
            Model::Mk2 => "APC40 mkII",
        }
    }
}

fn start(model: Model) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-apc40-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.akai.apc.{}\n", model.class())).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, model.port().into())], outputs: vec![(OUT, model.port().into())], ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    assert_eq!(app.instance_count(), 1);
    app.start_infrastructure();
    app.update_model(TRACKS);
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    (app, backend)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).expect("instance runs")
}

fn note(app: &mut App, channel: u8, n: u8, velocity: u8) {
    app.on_midi_event(IN, &[0x90 + channel, n, velocity]);
}

fn press(app: &mut App, channel: u8, n: u8) {
    note(app, channel, n, 127);
    note(app, channel, n, 0);
}

fn cc(app: &mut App, channel: u8, control: u8, value: u8) {
    app.on_midi_event(IN, &[0xB0 + channel, control, value]);
}

fn has_call(calls: &[BackendCall], processor: &str, command: &str) -> bool {
    calls.iter().any(|c| match c {
        BackendCall::NoArg(p, cmd) | BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) | BackendCall::StringArg(p, cmd, _) => {
            p == processor && cmd == command
        }
        _ => false,
    })
}

#[test]
fn startup_sets_mode_2_and_draws_the_pads() {
    for model in [Model::Mk1, Model::Mk2] {
        let (mut app, backend) = start(model);
        let calls = backend.take_calls();
        let id = if model == Model::Mk2 { 0x29 } else { 0x73 };
        assert!(calls.contains(&BackendCall::SendMidi(OUT, vec![0xF0, 0x47, 0x7F, id, 0x60, 0x00, 0x04, 0x41, 0x08, 0x02, 0x01, 0xF7])), "{calls:?}");
        assert!(ctx(&mut app).modes.is_active(Modes::Pan));
        assert!(ctx(&mut app).views.is_active(Views::Play));
        let pads = calls
            .iter()
            .filter(|c| match c {
                BackendCall::SendMidi(OUT, d) if d.len() == 3 && d[0] & 0xF0 == 0x90 => match model {
                    Model::Mk1 => (0x35..=0x39).contains(&d[1]) && d[0] & 0x0F < 8,
                    Model::Mk2 => d[0] == 0x90 && d[1] < 40,
                },
                _ => false,
            })
            .count();
        assert_eq!(pads, 40, "{calls:?}");
        app.shutdown();
    }
}

#[test]
fn faders_and_knobs() {
    let (mut app, backend) = start(Model::Mk2);
    backend.take_calls();
    // Track faders: CC 7 on the channel of the track
    cc(&mut app, 2, 0x07, 100);
    assert!(has_call(&backend.take_calls(), "track", "2/volume"));
    // Master
    cc(&mut app, 0, 0x0E, 100);
    assert!(has_call(&backend.take_calls(), "master", "volume"));
    // Device knobs
    cc(&mut app, 0, 0x10, 100);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| format!("{c:?}").contains("param/0/value")), "{calls:?}");
    // Track knobs in the pan mode
    cc(&mut app, 0, 0x30, 100);
    assert!(has_call(&backend.take_calls(), "track", "0/pan"));
    // The knob LEDs are sent 100ms after a flush
    app.tick();
    std::thread::sleep(Duration::from_millis(120));
    app.tick();
    let calls = backend.take_calls();
    // Track knob 1: LED mode pan, device knob 1: LED mode single
    assert!(calls.contains(&BackendCall::SendMidi(OUT, vec![0xB0, 0x38, 3])), "{calls:?}");
    assert!(calls.contains(&BackendCall::SendMidi(OUT, vec![0xB0, 0x18, 1])), "{calls:?}");
    app.shutdown();
}

#[test]
fn track_buttons() {
    let (mut app, backend) = start(Model::Mk1);
    backend.take_calls();
    // Solo, Activator (mute), Arm of track 1 (MIDI channel 2)
    press(&mut app, 1, 0x31);
    press(&mut app, 1, 0x32);
    press(&mut app, 1, 0x30);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "track", "1/solo"), "{calls:?}");
    assert!(has_call(&calls, "track", "1/mute"), "{calls:?}");
    assert!(has_call(&calls, "track", "1/recarm"), "{calls:?}");
    // Select
    press(&mut app, 2, 0x33);
    assert!(has_call(&backend.take_calls(), "track", "2/select"));
    // Stop clip
    press(&mut app, 0, 0x34);
    assert!(has_call(&backend.take_calls(), "track", "0/stop"));
    app.shutdown();
}

#[test]
fn send_and_user_modes() {
    let (mut app, _backend) = start(Model::Mk1);
    // Send A, with Shift Send D
    press(&mut app, 0, 0x58);
    assert!(ctx(&mut app).modes.is_active(Modes::Send1));
    note(&mut app, 0, 0x62, 127);
    press(&mut app, 0, 0x58);
    note(&mut app, 0, 0x62, 0);
    assert!(ctx(&mut app).modes.is_active(Modes::Send4));
    press(&mut app, 0, 0x57);
    assert!(ctx(&mut app).modes.is_active(Modes::Pan));
    app.shutdown();

    let (mut app, _backend) = start(Model::Mk2);
    // Sends button cycles through the sends
    press(&mut app, 0, 0x58);
    assert!(ctx(&mut app).modes.is_active(Modes::Send1));
    press(&mut app, 0, 0x58);
    assert!(ctx(&mut app).modes.is_active(Modes::Send2));
    // Sends + track select 3: send 3
    note(&mut app, 0, 0x58, 127);
    press(&mut app, 2, 0x33);
    note(&mut app, 0, 0x58, 0);
    assert!(ctx(&mut app).modes.is_active(Modes::Send3));
    // User mode
    press(&mut app, 0, 0x59);
    assert!(ctx(&mut app).modes.is_active(Modes::User));
    press(&mut app, 0, 0x59);
    assert!(ctx(&mut app).modes.is_active(Modes::User));
    app.shutdown();
}

#[test]
fn shift_selects_the_views_and_tap_tempo_shows_the_tempo() {
    let (mut app, backend) = start(Model::Mk2);
    // Holding Shift and pressing a button outside of the grid closes the shift view (as in
    // Java): the scene button goes to the play view (octave up)
    let octave = ctx(&mut app).model.scales().octave();
    note(&mut app, 0, 0x62, 127);
    assert!(ctx(&mut app).views.is_active(Views::Shift));
    press(&mut app, 0, 0x55);
    note(&mut app, 0, 0x62, 0);
    assert!(ctx(&mut app).views.is_active(Views::Play));
    assert_eq!(ctx(&mut app).model.scales().octave(), octave + 1);

    // A tap on Shift keeps the shift view open, then Scene 4 selects the sequencer
    press(&mut app, 0, 0x62);
    assert!(ctx(&mut app).views.is_active(Views::Shift));
    press(&mut app, 0, 0x55);
    app.tick();
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Sequencer));

    // Tap tempo shows the tempo view temporarily
    ctx(&mut app).set_active_view(Views::Session);
    backend.take_calls();
    press(&mut app, 0, 0x63);
    assert!(ctx(&mut app).views.is_active(Views::Tempo));
    assert!(has_call(&backend.take_calls(), "transport", "tapTempo"));
    app.tick();
    std::thread::sleep(Duration::from_millis(550));
    app.tick();
    assert!(ctx(&mut app).views.is_active(Views::Session));
    app.shutdown();
}

#[test]
fn every_view_mode_and_control_works_on_all_models() {
    for model in [Model::Mk1, Model::Mk2] {
        let (mut app, _backend) = start(model);
        app.update_model("track/0/type INSTRUMENT\nclip/exists 1\nclip/start 0\nclip/end 8\nclip/loop 1\nclip/notes 0:0:0.25:0.5:0:60:100\n");
        app.tick();
        let views = [Views::Play, Views::Session, Views::Sequencer, Views::Drum, Views::Raindrops, Views::Shift, Views::Tempo];
        let mut modes = vec![Modes::Pan, Modes::Note, Modes::User, Modes::Browser];
        for i in 0..8 {
            modes.push(Modes::get(Modes::Send1, i));
        }
        for view in views {
            assert!(ctx(&mut app).views.contains(view), "{view:?}");
            for mode in &modes {
                ctx(&mut app).set_active_view(view);
                ctx(&mut app).set_active_mode(*mode);
                assert!(ctx(&mut app).modes.is_active(*mode));
                for i in 0..8u8 {
                    cc(&mut app, 0, 0x30 + i, 100);
                    cc(&mut app, 0, 0x10 + i, 50);
                    cc(&mut app, i, 0x07, 70);
                }
                cc(&mut app, 0, 0x2F, 1);
                cc(&mut app, 0, 0x2F, 127);
                cc(&mut app, 0, 0x0D, 1);
                app.tick();
            }
            ctx(&mut app).set_active_view(view);
            let ids: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
            for id in ids {
                if id == ButtonId::Shift {
                    continue;
                }
                let ctx = ctx(&mut app);
                ctx.trigger_button(id, ButtonEvent::Down, 1.0);
                ctx.trigger_button(id, ButtonEvent::Long, 1.0);
                ctx.trigger_button(id, ButtonEvent::Up, 0.0);
            }
            app.tick();
            // With Shift
            let ids: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
            for id in ids {
                if id == ButtonId::Shift {
                    continue;
                }
                let ctx = ctx(&mut app);
                ctx.trigger_button(ButtonId::Shift, ButtonEvent::Down, 1.0);
                ctx.trigger_button(id, ButtonEvent::Down, 1.0);
                ctx.trigger_button(id, ButtonEvent::Up, 0.0);
                ctx.trigger_button(ButtonId::Shift, ButtonEvent::Up, 0.0);
            }
            app.tick();
            assert!(ctx(&mut app).surface.continuous(ContinuousId::Crossfader).is_some());
        }
        std::thread::sleep(Duration::from_millis(120));
        app.tick();
        app.shutdown();
    }
}
