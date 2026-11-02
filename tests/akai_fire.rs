// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Akai Fire: startup, the OLED display, the pads, the knob modes, the views, the
//! notifications, the shutdown and all controls in all views and modes.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::fire as config;
use drivenbymoss::controller::akai::fire::surface as fire;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::graphics::Component;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;
const PORT: &str = "FL STUDIO FIRE";

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volumeStr 0 dB\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\nplay 0\n";

fn start() -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-fire-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.akai.fire.FireControllerInstance\n").unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper-Fire.config"), format!("MIDI_INPUT0={PORT}\nMIDI_OUTPUT0={PORT}\n")).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, PORT.into())], outputs: vec![(OUT, PORT.into())], ..Default::default() });
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

fn note(app: &mut App, n: u8, velocity: u8) {
    app.on_midi_event(IN, &[0x90, n, velocity]);
}

fn press(app: &mut App, n: u8) {
    note(app, n, 127);
    note(app, n, 0);
}

fn cc(app: &mut App, control: u8, value: u8) {
    app.on_midi_event(IN, &[0xB0, control, value]);
}

fn sysex(calls: &[BackendCall], command: u8) -> Vec<Vec<u8>> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(OUT, d) if d.len() > 5 && d[0] == 0xF0 && d[3] == 0x43 && d[4] == command => Some(d.clone()),
            _ => None,
        })
        .collect()
}

fn has_call(calls: &[BackendCall], processor: &str, command: &str) -> bool {
    calls.iter().any(|c| match c {
        BackendCall::NoArg(p, cmd) | BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) | BackendCall::StringArg(p, cmd, _) => p == processor && cmd == command,
        _ => false,
    })
}

/// The components of the display after the next flush.
fn display_components(app: &mut App) -> Vec<Component> {
    let ctx = ctx(app);
    ctx.update_active_mode_display();
    let components = ctx.graphics_display.as_ref().unwrap().components().to_vec();
    fire::send_display(ctx);
    components
}

#[test]
fn startup_display_and_pads() {
    let (mut app, backend) = start();
    {
        let ctx = ctx(&mut app);
        assert!(ctx.views.is_active(Views::Play));
        assert!(ctx.modes.is_active(Modes::Track));
    }
    app.tick();
    let calls = backend.take_calls();
    // The whole display is sent as 8 stripes
    let stripes = sysex(&calls, 0x0E);
    assert_eq!(stripes.len(), 8, "{calls:?}");
    assert_eq!(stripes[0].len(), 12 + fire::STRIPE_SIZE);
    // The title text is drawn into the upper stripes
    assert!(stripes[..2].iter().any(|s| s[11..s.len() - 1].iter().any(|b| *b != 0)));
    // The pads are sent as RGB
    let pads = sysex(&calls, 0x65);
    assert!(!pads.is_empty());
    // The knob mode LED shows the track mode
    assert!(calls.contains(&BackendCall::SendMidi(OUT, vec![0xB0, 0x1B, 1])), "{calls:?}");

    // The display shows the selected track and its volume
    let components = display_components(&mut app);
    match &components[0] {
        Component::TitleValue { label1, label2, value, .. } => {
            assert_eq!(label1, "1: Drums");
            assert!(label2.starts_with("Vol: "), "{label2}");
            assert!(*value > 0);
        }
        c => panic!("Unexpected component {c:?}"),
    }

    // Nothing changed: no display update (until 3 seconds are over)
    backend.take_calls();
    app.tick();
    assert!(sysex(&backend.take_calls(), 0x0E).is_empty());
    app.shutdown();
}

#[test]
fn knobs_and_modes() {
    let (mut app, backend) = start();
    backend.take_calls();
    // Knob 1 controls the volume of the selected track
    cc(&mut app, 0x10, 1);
    assert!(has_call(&backend.take_calls(), "track", "0/volume"));
    // Touch knob 2: the panorama is shown
    note(&mut app, 0x11, 127);
    match &display_components(&mut app)[0] {
        Component::TitleValue { label2, is_pan, .. } => {
            assert!(label2.starts_with("Pan: "), "{label2}");
            assert!(is_pan);
        }
        c => panic!("Unexpected component {c:?}"),
    }
    note(&mut app, 0x11, 0);

    // BANK selects the next knob mode: device parameters
    press(&mut app, 0x1A);
    assert!(ctx(&mut app).modes.is_active(Modes::DeviceParams));
    app.tick();
    assert!(backend.take_calls().contains(&BackendCall::SendMidi(OUT, vec![0xB0, 0x1B, 2])));
    cc(&mut app, 0x10, 1);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| format!("{c:?}").contains("param/0/value")), "{calls:?}");
    match &display_components(&mut app)[0] {
        Component::TitleValue { label2, .. } => assert_eq!(label2, "ReaSynth"),
        c => panic!("Unexpected component {c:?}"),
    }

    // ALT + BANK toggles the mixer in the track mode
    ctx(&mut app).set_active_mode(Modes::Track);
    note(&mut app, 0x31, 127);
    press(&mut app, 0x1A);
    note(&mut app, 0x31, 0);
    assert!(ctx(&mut app).modes.is_active(Modes::Volume));
    assert!(matches!(&display_components(&mut app)[0], Component::TitleChannels { .. }));
    app.shutdown();
}

#[test]
fn views_notifications_and_shutdown() {
    let (mut app, backend) = start();
    // PERFORM: session view
    press(&mut app, 0x2F);
    assert!(ctx(&mut app).views.is_active(Views::Session));
    // PERFORM again: mix view
    press(&mut app, 0x2F);
    assert!(ctx(&mut app).views.is_active(Views::Mix));
    // Mix view: the upper row (notes 54..69) selects, the lower row (102..117) toggles rec arm
    backend.take_calls();
    press(&mut app, 102);
    assert!(has_call(&backend.take_calls(), "track", "0/recarm"));
    // STEP: sequencer, then poly sequencer
    press(&mut app, 0x2C);
    assert!(ctx(&mut app).views.is_active(Views::Sequencer));
    press(&mut app, 0x2C);
    assert!(ctx(&mut app).views.is_active(Views::PolySequencer));
    // DRUM: Drum 4, Drum 64
    press(&mut app, 0x2E);
    assert!(ctx(&mut app).views.is_active(Views::Drum4));
    press(&mut app, 0x2E);
    assert!(ctx(&mut app).views.is_active(Views::Drum64));
    // NOTE: Play, Piano
    press(&mut app, 0x2D);
    assert!(ctx(&mut app).views.is_active(Views::Play));
    press(&mut app, 0x2D);
    assert!(ctx(&mut app).views.is_active(Views::Piano));

    // ALT + PATTERN UP: redo with a notification on the display and the host
    ctx(&mut app).host.take_notifications();
    note(&mut app, 0x31, 127);
    press(&mut app, 0x1F);
    note(&mut app, 0x31, 0);
    assert!(has_call(&backend.take_calls(), "redo", ""));
    assert_eq!(ctx(&mut app).graphics_display.as_ref().unwrap().notification(), Some("Redo"));
    assert!(ctx(&mut app).host.take_notifications().contains(&"Redo".to_string()));

    // The pad brightness setting changes the pad colors
    config::pad_brightness(&ctx(&mut app).config);
    ctx(&mut app).config.set_value(config::PAD_BRIGHTNESS, "100");
    app.tick();
    assert!(fire::state(ctx(&mut app)).pad_brightness > 0.99);
    assert!(!sysex(&backend.take_calls(), 0x65).is_empty());

    app.shutdown();
    let calls = backend.take_calls();
    assert!(calls.contains(&BackendCall::SendMidi(OUT, vec![0xB0, 0x1B, 16])), "{calls:?}");
    assert!(!sysex(&calls, 0x0E).is_empty());
}

#[test]
fn every_control_works_in_every_view_and_mode() {
    let (mut app, _backend) = start();
    let views = [Views::Play, Views::Piano, Views::Drum4, Views::Drum64, Views::Sequencer, Views::PolySequencer, Views::Session, Views::Mix, Views::Shift];
    let modes = [Modes::Track, Modes::Volume, Modes::DeviceLayer, Modes::DeviceLayerVolume, Modes::DeviceParams, Modes::User, Modes::Note, Modes::Browser];
    for view in views {
        for mode in modes {
            {
                let ctx = ctx(&mut app);
                ctx.set_active_view(view);
                ctx.set_active_mode(mode);
            }
            for modifier in [None, Some(ButtonId::Shift), Some(ButtonId::Alt)] {
                let ids: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
                for id in ids {
                    let ctx = ctx(&mut app);
                    if let Some(modifier) = modifier {
                        ctx.trigger_button(modifier, ButtonEvent::Down, 1.0);
                    }
                    ctx.trigger_button(id, ButtonEvent::Down, 1.0);
                    ctx.trigger_button(id, ButtonEvent::Long, 1.0);
                    ctx.trigger_button(id, ButtonEvent::Up, 0.0);
                    if let Some(modifier) = modifier {
                        ctx.trigger_button(modifier, ButtonEvent::Up, 0.0);
                    }
                }
                if let Some(modifier) = modifier {
                    ctx(&mut app).trigger_button(modifier, ButtonEvent::Down, 1.0);
                }
                // Knobs, touches and the select knob
                for control in [0x10u8, 0x11, 0x12, 0x13, 0x76] {
                    cc(&mut app, control, 1);
                    cc(&mut app, control, 127);
                }
                for n in 0x10..0x14u8 {
                    note(&mut app, n, 127);
                    cc(&mut app, n, 2);
                    note(&mut app, n, 0);
                }
                // Select knob with SELECT, DRUM and PATTERN pressed
                for button in [0x19u8, 0x2E, 0x32] {
                    note(&mut app, button, 127);
                    cc(&mut app, 0x76, 1);
                    cc(&mut app, 0x76, 127);
                    note(&mut app, button, 0);
                }
                if let Some(modifier) = modifier {
                    ctx(&mut app).trigger_button(modifier, ButtonEvent::Up, 0.0);
                }
                // Pads
                for n in 54..118u8 {
                    note(&mut app, n, 100);
                    note(&mut app, n, 0);
                }
                app.tick();
            }
        }
    }
    // The settings
    config::toggle_control_last_param(&mut ctx(&mut app).config);
    cc(&mut app, 0x76, 1);
    ctx(&mut app).config.set_value(config::PAD_SATURATION, "50");
    app.tick();
    assert!(ctx(&mut app).surface.continuous(ContinuousId::ViewSelection).is_some());
    app.shutdown();
}
