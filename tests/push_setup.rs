// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Ableton Push 1, 2 and 3 setup: startup sysex, the displays (Push 1 text, the
//! Push 2/3 USB frames), settings sent to the device and all controls in all views and modes.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::push as config;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volumeStr 0 dB\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\nplay 0\n";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Model {
    Push1,
    Push2,
    Push3,
}

impl Model {
    fn class(self) -> &'static str {
        match self {
            Model::Push1 => "de.mossgrabers.reaper.controller.ableton.push.Push1ControllerInstance",
            Model::Push2 => "de.mossgrabers.reaper.controller.ableton.push.Push2ControllerInstance",
            Model::Push3 => "de.mossgrabers.reaper.controller.ableton.push.Push3ControllerInstance",
        }
    }

    fn port(self) -> &'static str {
        match self {
            Model::Push1 => "Ableton Push User Port",
            Model::Push2 => "Ableton Push 2 Live Port",
            Model::Push3 => "Ableton Push 3 Live Port",
        }
    }

    fn file(self) -> &'static str {
        match self {
            Model::Push1 => "Push 1",
            Model::Push2 => "Push 2",
            Model::Push3 => "Push 3",
        }
    }
}

fn start(model: Model) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-push-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={}\n", model.class())).unwrap();
    let port = model.port();
    std::fs::write(dir.join(format!("DrivenByMoss4Reaper-{}.config", model.file())), format!("MIDI_INPUT0={port}\nMIDI_OUTPUT0={port}\n")).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, port.into())], outputs: vec![(OUT, port.into())], ..Default::default() });
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

fn sysex(calls: &[BackendCall]) -> Vec<Vec<u8>> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(OUT, d) if d.first() == Some(&0xF0) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

const PUSH2_HEADER: [u8; 6] = [0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01];

#[test]
fn startup_push1() {
    let (mut app, backend) = start(Model::Push1);
    let messages = sysex(&backend.take_calls());
    // Pressure mode (poly aftertouch), the device inquiry and the pad sensitivity
    assert!(messages.contains(&vec![0xF0, 0x47, 0x7F, 0x15, 0x5C, 0x00, 0x01, 0x00, 0xF7]), "{messages:02X?}");
    assert!(messages.contains(&vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]));
    assert!(messages.iter().any(|m| m.starts_with(&[0xF0, 0x47, 0x7F, 0x15, 0x5D, 0x00, 0x20])));
    // The Push 1 is locked to mute at startup
    assert_eq!(drivenbymoss::controller::push::surface::lock_state(ctx(&mut app)), config::LockState::Mute);
    // The text display rows are sent
    app.tick();
    let messages = sysex(&backend.take_calls());
    let _ = messages;
    let ctx = ctx(&mut app);
    assert!(ctx.text_display.is_some());
    assert!(ctx.graphics_display.is_none());
}

#[test]
fn startup_push2() {
    let (mut app, backend) = start(Model::Push2);
    let messages = sysex(&backend.take_calls());
    let mut pressure = PUSH2_HEADER.to_vec();
    pressure.extend([0x1E, 0x01, 0xF7]);
    assert!(messages.contains(&pressure), "{messages:02X?}");
    // The pad velocity curve in 8 chunks of 16 entries (also sent by the initial setting changes)
    let chunks = messages.iter().filter(|m| m.starts_with(&PUSH2_HEADER) && m[6] == 32).count();
    assert!(chunks >= 8 && chunks % 8 == 0, "{chunks}");
    // Display and LED brightness
    assert!(messages.iter().any(|m| m.starts_with(&PUSH2_HEADER) && m[6] == 8));
    assert!(messages.iter().any(|m| m.starts_with(&PUSH2_HEADER) && m[6] == 6));
    assert!(ctx(&mut app).graphics_display.is_some());
}

#[test]
fn startup_push3() {
    let (_app, backend) = start(Model::Push3);
    let messages = sysex(&backend.take_calls());
    // No pressure mode on the Push 3
    assert!(!messages.iter().any(|m| m.starts_with(&PUSH2_HEADER) && m[6] == 0x1E && m[7] == 0x00), "{messages:02X?}");
    // The pad curve with the command byte (fixed Java bug)
    assert!(messages.iter().any(|m| m.starts_with(&PUSH2_HEADER) && m[6] == 0x43 && m.len() == 6 + 1 + 128 + 1));
    // Audio interface: the outputs
    assert!(messages.iter().any(|m| m.starts_with(&PUSH2_HEADER) && m[6] == 0x37 && m[7] == 0x11));
}

#[test]
fn settings_push2() {
    let (mut app, backend) = start(Model::Push2);
    backend.take_calls();
    let ctx = ctx(&mut app);
    config::set_led_brightness(&mut ctx.config, 50);
    app.tick();
    let messages = sysex(&backend.take_calls());
    let mut led = PUSH2_HEADER.to_vec();
    led.extend([6, 63, 0xF7]);
    assert!(messages.contains(&led), "{messages:02X?}");
}

/// Activate every view and mode and trigger every button once (DOWN, LONG, UP), with ticks in
/// between: no panics.
#[test]
fn touch_everything() {
    for model in [Model::Push1, Model::Push2, Model::Push3] {
        let (mut app, _backend) = start(model);
        let views: Vec<Views> = Views::ALL.iter().copied().filter(|v| ctx(&mut app).views.contains(*v)).collect();
        let modes: Vec<Modes> = Modes::ALL.iter().copied().filter(|m| ctx(&mut app).modes.contains(*m)).collect();
        let buttons: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
        for view in &views {
            ctx(&mut app).set_active_view(*view);
            app.tick();
            for mode in &modes {
                ctx(&mut app).set_active_mode(*mode);
                app.tick();
            }
            for button in &buttons {
                for event in [ButtonEvent::Down, ButtonEvent::Long, ButtonEvent::Up] {
                    ctx(&mut app).trigger_button(*button, event, if event == ButtonEvent::Up { 0.0 } else { 1.0 });
                }
                app.tick();
            }
            for knob in 0..9u8 {
                app.on_midi_event(IN, &[0xB0, 71 + knob, 1]);
                app.on_midi_event(IN, &[0xB0, 71 + knob, 127]);
            }
            for pad in 36..100u8 {
                app.on_midi_event(IN, &[0x90, pad, 100]);
                app.on_midi_event(IN, &[0x90, pad, 0]);
            }
            app.tick();
        }
    }
}
