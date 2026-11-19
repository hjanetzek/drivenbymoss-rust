// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The encoder state of the Push 2/3 for the Push simulator (`controller::push::sim_values`):
//! taken from the rendered display, sent only on change and only to a host which takes it.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::controller::push::sim_values::{decode_message, EncoderState, Message, MASTER, TEMPO};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::Modes;

const IN: i32 = 3;
const OUT: i32 = 5;

const MODEL: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volume/str 0 dB\ntrack/0/pan 0.5\ntrack/1/exists 1\ntrack/1/name Bass\ntrack/1/volume 0.25\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\ndevice/param/0/value/str 50 %\nplay 0\n";

fn start(class: &str, port: &str, file: &str, simulator_data: bool) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-push-sim-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={class}\n")).unwrap();
    std::fs::write(dir.join(format!("DrivenByMoss4Reaper-{file}.config")), format!("MIDI_INPUT0={port}\nMIDI_OUTPUT0={port}\n")).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, port.into())], outputs: vec![(OUT, port.into())], simulator_data, ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    app.start_infrastructure();
    app.update_model(MODEL);
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    (app, backend)
}

fn push2(simulator_data: bool) -> (App, Rc<RecordingBackend>) {
    start("de.mossgrabers.reaper.controller.ableton.push.Push2ControllerInstance", "Ableton Push 2 Live Port", "Push 2", simulator_data)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).expect("instance runs")
}

/// The decoded encoder messages sent since the last call.
fn messages(backend: &RecordingBackend) -> Vec<Vec<EncoderState>> {
    backend
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            BackendCall::SimulatorData(instance, data) => {
                assert_eq!(instance, 0);
                match decode_message(&data) {
                    Some((Message::Encoders(states), size)) if size == data.len() => Some(states),
                    other => panic!("not an encoder message: {other:?}"),
                }
            }
            _ => None,
        })
        .collect()
}

fn activate(app: &mut App, mode: Modes) {
    ctx(app).set_active_mode(mode);
    app.tick();
}

#[test]
fn volume_mode_values_and_change_detection() {
    let (mut app, backend) = push2(true);
    activate(&mut app, Modes::Volume);
    let all = messages(&backend);
    let states = all.last().expect("a message after the mode change");
    assert_eq!(states.len(), 11, "{states:?}");
    assert_eq!(states.iter().map(|s| s.index).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6, 7, MASTER, 9, TEMPO]);
    let max = i32::from(states[0].max);
    assert!(max >= 1023, "{max}");
    assert_eq!(states[0].name, "Drums");
    assert!(states[0].has_value && !states[0].touched && !states[0].bipolar);
    assert!((i32::from(states[0].value) - max / 2).abs() <= 1, "{:?}", states[0]);
    assert_eq!(states[1].name, "Bass");
    assert!((i32::from(states[1].value) - max / 4).abs() <= 1, "{:?}", states[1]);
    // The value text only while touched (as the display shows it)
    assert_eq!(states[0].text, "");
    assert_eq!(states[TEMPO as usize].name, "Tempo");

    // Nothing changed: nothing sent
    app.tick();
    assert!(messages(&backend).is_empty());

    // Touch the first encoder: the display shows the volume text
    {
        let ctx = ctx(&mut app);
        let mode = ctx.active_mode().unwrap();
        mode.borrow_mut().on_knob_touch(ctx, 0, true);
    }
    app.tick();
    let states = messages(&backend).pop().expect("a message after the touch");
    assert!(states[0].touched);
    assert_eq!(states[0].text, "0 dB");

    // A changed value
    app.update_model("track/1/volume 0.75\n");
    app.tick();
    let states = messages(&backend).pop().expect("a message after the change");
    assert!((i32::from(states[1].value) - 3 * max / 4).abs() <= 1, "{:?}", states[1]);

    activate(&mut app, Modes::Pan);
    let states = messages(&backend).pop().expect("a message after the mode change");
    assert!(states[0].bipolar && states[0].has_value);
}

#[test]
fn device_mode_parameters() {
    let (mut app, backend) = push2(true);
    activate(&mut app, Modes::DeviceParams);
    let states = messages(&backend).pop().expect("a message");
    assert_eq!(states[0].name, "Cutoff");
    assert!(states[0].has_value);
    assert_eq!(states[0].text, "50 %");
}

#[test]
fn nothing_without_a_receiver() {
    let (mut app, backend) = push2(false);
    activate(&mut app, Modes::Volume);
    app.tick();
    let calls = backend.take_calls();
    assert!(!calls.iter().any(|c| matches!(c, BackendCall::SimulatorData(..))));
    // The display frames still go out
    assert_eq!(backend.last_usb_data.borrow().len(), drivenbymoss::controller::push::usb_display::FRAME_SIZE);
}

#[test]
fn push3_takes_the_same_path() {
    let (mut app, backend) = start("de.mossgrabers.reaper.controller.ableton.push.Push3ControllerInstance", "Ableton Push 3 Live Port", "Push 3", true);
    activate(&mut app, Modes::Volume);
    let states = messages(&backend).pop().expect("a message");
    assert_eq!(states[0].name, "Drums");
    // Push 3 has no play position encoder (its small encoder is tempo/swing)
    assert_eq!(states.iter().map(|s| s.index).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6, 7, MASTER, TEMPO]);
}

#[test]
fn push1_sends_nothing() {
    let (mut app, backend) = start("de.mossgrabers.reaper.controller.ableton.push.Push1ControllerInstance", "Ableton Push User Port", "Push 1", true);
    activate(&mut app, Modes::Volume);
    assert!(messages(&backend).is_empty());
}
