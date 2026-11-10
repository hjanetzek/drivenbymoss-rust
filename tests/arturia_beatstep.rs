// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Arturia Beatstep: startup, the pad LEDs (sysex), the knobs in all views, the
//! shift view and the sequencers.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::controller::arturia::beatstep::definition::light_pad_message;
use drivenbymoss::controller::arturia::beatstep::view::{DeviceView, DrumView, SequencerView};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;
const PORT: &str = "Arturia BeatStep";

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/type INSTRUMENT\n\
track/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\ntrack/1/slot/count 2\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/bypass 0\ndevice/name ReaSynth\ndevice/param/count 16\ndevice/param/0/name Cutoff\nplay 0\n";

/// Start an instance and wait for the delayed startup.
fn start() -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-beatstep-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.arturia.beatstep.BeatstepControllerInstance\n").unwrap();
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

/// A pad (grid note 36-51) on MIDI channel 3.
fn pad(app: &mut App, note: u8, velocity: u8) {
    app.on_midi_event(IN, &[if velocity == 0 { 0x82 } else { 0x92 }, note, velocity]);
}

fn press_pad(app: &mut App, note: u8) {
    pad(app, note, 127);
    pad(app, note, 0);
}

fn shift(app: &mut App, is_pressed: bool) {
    app.on_midi_event(IN, &[0x90, 7, if is_pressed { 127 } else { 0 }]);
}

/// Turn a knob (0-15) by `change` steps (offset binary on MIDI channel 3).
fn knob(app: &mut App, index: u8, change: i32) {
    let cc = if index < 8 { 20 + index } else { 30 + index - 8 };
    app.on_midi_event(IN, &[0xB2, cc, (64 + change) as u8]);
}

fn has_call(calls: &[BackendCall], processor: &str, command: &str) -> bool {
    calls.iter().any(|c| match c {
        BackendCall::NoArg(p, cmd) | BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) | BackendCall::StringArg(p, cmd, _) => {
            p == processor && cmd == command
        }
        _ => false,
    })
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

fn pad_color(app: &mut App, note: i32) -> i32 {
    ctx(app).surface.grid_ref().light_info(note).color
}

#[test]
fn startup_enables_shift_and_lights_the_track_view() {
    let (mut app, backend) = start();
    let calls = backend.take_calls();
    assert!(calls.contains(&BackendCall::OpenInput(IN)));
    assert!(calls.contains(&BackendCall::OpenOutput(OUT)));
    // The pads and their aftertouch on channel 3 go to the DAW, the hardware sequencer on
    // channel 1 is a second note input which blocks the Shift note
    assert!(calls.contains(&BackendCall::NoteInputFilters(IN, 0, vec!["82".into(), "92".into(), "A2".into()])), "{calls:?}");
    assert!(calls.contains(&BackendCall::NoteInputFilters(IN, 1, vec!["80".into(), "90".into()])), "{calls:?}");
    assert!(calls.iter().any(|c| matches!(c, BackendCall::KeyTranslation(IN, 1, t) if t[7] == -1 && t[8] == 8 && t[6] == 6)), "{calls:?}");

    let messages = sysex(&calls);
    assert_eq!(messages[0], vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x01, 0x5E, 0x09, 0xF7]);
    assert!(ctx(&mut app).views.is_active(Views::Track));
    // All 16 pads are sent once: track 1 is selected (pad 9 blue), it is activated (36 blue),
    // the add track pads are pink
    assert_eq!(messages.len(), 17, "{messages:?}");
    assert!(messages.contains(&light_pad_message(44, 16).to_vec()));
    assert!(messages.contains(&light_pad_message(45, 0).to_vec()));
    assert!(messages.contains(&light_pad_message(36, 16).to_vec()));
    assert!(messages.contains(&light_pad_message(39, 17).to_vec()));
    app.shutdown();
}

#[test]
fn released_pads_get_their_color_again() {
    let (mut app, backend) = start();
    backend.take_calls();
    // Pad 2 (note 45) selects the second track
    press_pad(&mut app, 45);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "track", "1/select"), "{calls:?}");
    // The release sends the stored color (off, the selection is not reported yet)
    assert_eq!(sysex(&calls), vec![light_pad_message(45, 0).to_vec()]);
    app.update_model("track/0/select 0\ntrack/1/select 1\n");
    app.tick();
    assert_eq!(pad_color(&mut app, 45), 16);
    assert_eq!(pad_color(&mut app, 44), 0);
    app.shutdown();
}

#[test]
fn track_knobs_change_the_selected_track() {
    let (mut app, backend) = start();
    backend.take_calls();
    knob(&mut app, 0, 1);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) if p == "track" && cmd == "0/volume")), "{calls:?}");
    knob(&mut app, 2, 1);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "track", "0/mute"), "{calls:?}");
    knob(&mut app, 7, -3);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "master", "volume"), "{calls:?}");
    // No change: nothing is sent
    knob(&mut app, 0, 0);
    assert!(backend.take_calls().is_empty());
    // Sends 1 and 5
    knob(&mut app, 8, 2);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) if p == "track" && cmd == "0/send/0/volume")), "{calls:?}");
    // The main knob changes the play position
    app.on_midi_event(IN, &[0xB2, 40, 65]);
    let calls = backend.take_calls();
    assert!(!calls.is_empty(), "{calls:?}");
    app.shutdown();
}

#[test]
fn device_view_controls_the_parameters() {
    let (mut app, backend) = start();
    ctx(&mut app).set_active_view(Views::Device);
    app.tick();
    backend.take_calls();
    // Knob 9: the first parameter
    knob(&mut app, 8, 1);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) if p == "device" && cmd.starts_with("param/0/"))), "{calls:?}");
    // The device is on: pad 1 is red
    assert_eq!(pad_color(&mut app, 36), 1);
    assert_eq!(pad_color(&mut app, 44), 16);
    // Pad 8: next parameter page
    press_pad(&mut app, 43);
    let calls = backend.take_calls();
    assert!(!calls.is_empty());
    // Pad 1: toggle the device
    press_pad(&mut app, 36);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::IntArg(p, cmd, _) | BackendCall::NoArg(p, cmd) if p == "device" && cmd.contains("bypass"))), "{calls:?}");
    // No layers: enter layer does nothing
    press_pad(&mut app, 39);
    let view = ctx(&mut app).views.get(Views::Device).unwrap();
    assert!(!view.borrow().as_any().downcast_ref::<DeviceView>().unwrap().is_layer());
    app.shutdown();
}

#[test]
fn play_view_changes_scale_and_octave() {
    let (mut app, _backend) = start();
    ctx(&mut app).set_active_view(Views::Play);
    app.tick();
    // The note C is the octave color (red), the in-scale notes blue
    assert_eq!(pad_color(&mut app, 36), 1);
    assert_eq!(pad_color(&mut app, 37), 16);
    let scale = ctx(&mut app).config.scale().to_string();
    knob(&mut app, 14, 1);
    app.tick();
    assert_ne!(ctx(&mut app).config.scale(), scale);
    knob(&mut app, 13, 1);
    app.tick();
    assert_eq!(ctx(&mut app).config.scale_base(), "G");
    knob(&mut app, 12, -1);
    app.tick();
    assert!(!ctx(&mut app).config.is_scale_in_key());
    assert!(ctx(&mut app).model.scales().is_chromatic());
    let octave = ctx(&mut app).model.scales().octave();
    knob(&mut app, 15, 1);
    assert_eq!(ctx(&mut app).model.scales().octave(), octave + 1);
    // The pads play notes: a pressed pad is pink
    pad(&mut app, 40, 100);
    app.tick();
    assert_eq!(pad_color(&mut app, 40), 17);
    pad(&mut app, 40, 0);
    app.shutdown();
}

#[test]
fn shift_view_selects_views_and_controls_the_transport() {
    let (mut app, backend) = start();
    backend.take_calls();
    shift(&mut app, true);
    assert!(ctx(&mut app).views.is_active(Views::Shift));
    app.tick();
    // Play is blue (stopped), record red
    assert_eq!(pad_color(&mut app, 36), 16);
    assert_eq!(pad_color(&mut app, 37), 1);
    press_pad(&mut app, 39);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "transport", "toggleMetronome") || has_call(&calls, "click", ""), "{calls:?}");
    press_pad(&mut app, 36);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "play", ""), "{calls:?}");
    // Pad 11: the play view is selected on release of Shift
    press_pad(&mut app, 46);
    shift(&mut app, false);
    assert!(ctx(&mut app).views.is_active(Views::Play));
    // The release of Shift sends all 16 pads again
    let calls = backend.take_calls();
    assert_eq!(sysex(&calls).len(), 16 + 1, "{calls:?}");
    app.shutdown();
}

#[test]
fn session_view_launches_scenes() {
    let (mut app, backend) = start();
    ctx(&mut app).set_active_view(Views::Session);
    app.tick();
    assert_eq!(pad_color(&mut app, 36), 0);
    assert_eq!(pad_color(&mut app, 44), 16);
    backend.take_calls();
    pad(&mut app, 44, 127);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::IntArg(p, cmd, _) | BackendCall::NoArg(p, cmd) if p == "scene" && cmd.starts_with("0/"))), "{calls:?}");
    pad(&mut app, 44, 0);
    app.shutdown();
}

#[test]
fn sequencer_views_toggle_between_play_and_sequence() {
    let (mut app, backend) = start();
    app.update_model("clip/exists 1\nclip/start 0\nclip/end 8\nclip/loop 1\nclip/notes 0:0:0.25:0.5:0:60:100\n");
    ctx(&mut app).set_active_view(Views::Sequencer);
    app.tick();
    let view_state = |app: &mut App| {
        let view = ctx(app).views.get(Views::Sequencer).unwrap();
        let view = view.borrow();
        let view = view.as_any().downcast_ref::<SequencerView>().unwrap();
        (view.is_play_mode(), view.selected_pad())
    };
    assert_eq!(view_state(&mut app), (true, 0));
    // The selected pad is pink
    assert_eq!(pad_color(&mut app, 36), 17);
    press_pad(&mut app, 38);
    assert_eq!(view_state(&mut app), (true, 2));
    // Knob 16 to the right: sequence mode, to the left: play mode
    knob(&mut app, 15, -1);
    assert_eq!(view_state(&mut app), (true, 2));
    knob(&mut app, 15, 1);
    assert_eq!(view_state(&mut app), (false, 2));
    app.tick();
    backend.take_calls();
    // Pad 1 (upper row) is step 1
    press_pad(&mut app, 44);
    let calls = backend.take_calls();
    assert!(!calls.is_empty(), "{calls:?}");
    // Knob 14: resolution (1/16 -> 1/16T)
    let resolution = |app: &mut App| {
        let view = ctx(app).views.get(Views::Sequencer).unwrap();
        let index = view.borrow().sequencer_resolution_index(ctx(app));
        index
    };
    assert_eq!(resolution(&mut app), Some(4));
    knob(&mut app, 13, 1);
    assert_eq!(resolution(&mut app), Some(5));
    knob(&mut app, 13, -1);
    assert_eq!(resolution(&mut app), Some(4));

    // The drum view
    ctx(&mut app).set_active_view(Views::Drum);
    app.tick();
    let drum_state = |app: &mut App| {
        let view = ctx(app).views.get(Views::Drum).unwrap();
        let view = view.borrow();
        let view = view.as_any().downcast_ref::<DrumView>().unwrap();
        (view.is_play_mode(), view.selected_pad())
    };
    assert_eq!(drum_state(&mut app), (true, 0));
    assert_eq!(pad_color(&mut app, 36), 1);
    press_pad(&mut app, 41);
    assert_eq!(drum_state(&mut app), (true, 5));
    let offset = ctx(&mut app).model.scales().drum_offset();
    knob(&mut app, 14, 1);
    assert_eq!(ctx(&mut app).model.scales().drum_offset(), offset + 16);
    knob(&mut app, 15, 1);
    assert_eq!(drum_state(&mut app), (false, 5));
    app.shutdown();
}

#[test]
fn every_view_and_control_works() {
    let (mut app, _backend) = start();
    app.update_model("clip/exists 1\nclip/start 0\nclip/end 8\nclip/loop 1\nclip/notes 0:0:0.25:0.5:0:60:100\n");
    app.tick();
    let views = [Views::Track, Views::Device, Views::Play, Views::Drum, Views::Sequencer, Views::Session, Views::Browser, Views::Shift];
    for view in views {
        assert!(ctx(&mut app).views.contains(view));
        ctx(&mut app).set_active_view(view);
        app.tick();
        for mode in [false, true] {
            for index in 0..16u8 {
                knob(&mut app, index, if mode { -12 } else { 3 });
            }
            app.on_midi_event(IN, &[0xB2, 40, 63]);
            app.tick();
        }
        let ids: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
        for id in ids {
            // Shift switches to the shift view, which is tested separately
            if id == ButtonId::Shift {
                continue;
            }
            let ctx = ctx(&mut app);
            ctx.trigger_button(id, ButtonEvent::Down, 1.0);
            ctx.trigger_button(id, ButtonEvent::Long, 1.0);
            ctx.trigger_button(id, ButtonEvent::Up, 0.0);
        }
        // Poly aftertouch of a pad
        app.on_midi_event(IN, &[0xA2, 36, 50]);
        app.tick();
        for control in [ContinuousId::Knob1, ContinuousId::Knob8, ContinuousId::DeviceKnob1, ContinuousId::DeviceKnob8, ContinuousId::MasterKnob] {
            assert!(ctx(&mut app).surface.continuous(control).is_some());
        }
    }
    shift(&mut app, true);
    app.tick();
    shift(&mut app, false);
    app.tick();
    app.shutdown();
}
