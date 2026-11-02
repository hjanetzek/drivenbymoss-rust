// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Akai APC mini Mk1 and Mk2: startup, pads, faders, track and scene buttons, the
//! shift view and all views and modes.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::apcmini as config;
use drivenbymoss::controller::akai::apcmini::surface as apc;
use drivenbymoss::controller::akai::apcmini::view::SequencerView;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\ntrack/1/slot/count 2\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\nplay 0\n";

#[derive(Clone, Copy)]
enum Model {
    Mk1,
    Mk2,
}

impl Model {
    fn class(self) -> &'static str {
        match self {
            Model::Mk1 => "APCminiMk1ControllerInstance",
            Model::Mk2 => "APCminiMk2ControllerInstance",
        }
    }

    fn port(self) -> &'static str {
        match self {
            Model::Mk1 => "APC MINI",
            Model::Mk2 => "APC mini mk2",
        }
    }

    fn shift(self) -> u8 {
        match self {
            Model::Mk1 => 98,
            Model::Mk2 => 0x7A,
        }
    }

    fn track(self, index: u8) -> u8 {
        match self {
            Model::Mk1 => 64 + index,
            Model::Mk2 => 0x64 + index,
        }
    }

    fn scene(self, index: u8) -> u8 {
        match self {
            Model::Mk1 => 82 + index,
            Model::Mk2 => 0x70 + index,
        }
    }
}

/// Start an instance and wait for the delayed startup.
fn start(model: Model, extra_config: &str) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-apcmini-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.akai.apcmini.{}\n", model.class()))
        .unwrap();
    if !extra_config.is_empty() {
        let name = match model {
            Model::Mk1 => "DrivenByMoss4Reaper-APCmini-Mk1.config",
            Model::Mk2 => "DrivenByMoss4Reaper-APCmini-Mk2.config",
        };
        std::fs::write(dir.join(name), extra_config).unwrap();
    }
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

fn number_arg(calls: &[BackendCall], processor: &str, command: &str) -> Option<f64> {
    calls.iter().rev().find_map(|c| match c {
        BackendCall::IntArg(p, cmd, v) if p == processor && cmd == command => Some(*v as f64),
        BackendCall::DoubleArg(p, cmd, v) if p == processor && cmd == command => Some(*v),
        _ => None,
    })
}

fn has_call(calls: &[BackendCall], processor: &str, command: &str) -> bool {
    calls.iter().any(|c| match c {
        BackendCall::NoArg(p, cmd) | BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) | BackendCall::StringArg(p, cmd, _) => {
            p == processor && cmd == command
        }
        _ => false,
    })
}

/// The last MIDI message sent with the given status and data1.
fn midi_out(calls: &[BackendCall], status: u8, data1: u8) -> Option<u8> {
    calls.iter().rev().find_map(|c| match c {
        BackendCall::SendMidi(OUT, d) if d.len() == 3 && d[0] == status && d[1] == data1 => Some(d[2]),
        _ => None,
    })
}

#[test]
fn mk1_starts_with_the_session_pads_on_channel_1() {
    let (mut app, backend) = start(Model::Mk1, "");
    let calls = backend.take_calls();
    assert!(calls.contains(&BackendCall::OpenInput(IN)));
    assert!(calls.contains(&BackendCall::OpenOutput(OUT)));
    // The default startup view is the play view
    assert!(ctx(&mut app).views.is_active(Views::Play));
    assert!(ctx(&mut app).modes.is_active(Modes::Volume));
    // All 64 pads are sent once on MIDI channel 1
    let pads = calls.iter().filter(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d[0] == 0x90 && d[1] < 64)).count();
    assert_eq!(pads, 64, "{calls:?}");
    // The shift button shows BUTTON_STATE_ON (green) while not pressed
    assert_eq!(midi_out(&calls, 0x90, 98), Some(1));

    // Session view: the clip with content is yellow (top left pad = note 56)
    ctx(&mut app).set_active_view(Views::Session);
    app.tick();
    let calls = backend.take_calls();
    assert_eq!(ctx(&mut app).surface.grid_ref().light_info(36 + 56).color, 5);
    assert_eq!(ctx(&mut app).surface.grid_ref().light_info(36 + 57).color, 0);
    // The existing scenes are green, the first one is not selected
    let light = |app: &mut App, id: ButtonId| ctx(app).surface.button_light(id).and_then(|l| l.state());
    assert_eq!(light(&mut app, ButtonId::Scene1), Some(1));
    assert_eq!(light(&mut app, ButtonId::Scene3), Some(0));
    // Track buttons are off (soft key 'Clip Stop', nothing pressed)
    assert_eq!(light(&mut app, ButtonId::RowSelect1), Some(0));
    assert!(calls.iter().all(|c| !matches!(c, BackendCall::SendMidi(OUT, d) if d[1] == 64)));
    app.shutdown();
}

#[test]
fn mk2_sends_the_pads_with_the_brightness_as_channel() {
    let (mut app, backend) = start(Model::Mk2, "");
    let calls = backend.take_calls();
    // Default brightness is 75% (index 4) -> MIDI channel 5
    let pads = calls.iter().filter(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d[0] == 0x94 && d[1] < 64)).count();
    assert_eq!(pads, 64, "{calls:?}");

    // Change the brightness: all pads are sent again on the new channel
    ctx(&mut app).config.set_value(config::PAD_BRIGHTNESS, "100%");
    app.tick();
    let calls = backend.take_calls();
    let pads = calls.iter().filter(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d[0] == 0x96 && d[1] < 64)).count();
    assert_eq!(pads, 64, "{calls:?}");
    app.shutdown();
}

#[test]
fn faders_control_volume_and_master() {
    let (mut app, backend) = start(Model::Mk1, "");
    backend.take_calls();
    cc(&mut app, 48, 127);
    let calls = backend.take_calls();
    assert!(number_arg(&calls, "track", "0/volume").is_some(), "{calls:?}");
    cc(&mut app, 56, 64);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "master", "volume"), "{calls:?}");

    // Fader control setting switches the fader mode
    config::set_fader_ctrl(&mut ctx(&mut app).config, "Pan");
    app.tick();
    assert!(ctx(&mut app).modes.is_active(Modes::Pan));
    cc(&mut app, 49, 100);
    let calls = backend.take_calls();
    assert!(number_arg(&calls, "track", "1/pan").is_some(), "{calls:?}");
    app.shutdown();
}

#[test]
fn shift_and_track_buttons_select_the_fader_modes() {
    for model in [Model::Mk1, Model::Mk2] {
        let (mut app, backend) = start(model, "");
        // Mk1: track button 5-8 are volume, pan, send, device; on the Mk2 these are 1-4
        let first = match model {
            Model::Mk1 => 4,
            Model::Mk2 => 0,
        };
        note(&mut app, model.shift(), 127);
        assert!(ctx(&mut app).views.is_active(Views::Shift));
        press(&mut app, model.track(first + 1));
        assert!(ctx(&mut app).modes.is_active(Modes::Pan));
        assert_eq!(config::fader_ctrl(&ctx(&mut app).config), "Pan");
        press(&mut app, model.track(first + 2));
        assert!(ctx(&mut app).modes.is_active(Modes::Send1));
        // The second press selects the next send (2 sends exist)
        press(&mut app, model.track(first + 2));
        assert!(ctx(&mut app).modes.is_active(Modes::Send2));
        press(&mut app, model.track(first + 3));
        assert!(ctx(&mut app).modes.is_active(Modes::DeviceParams));
        press(&mut app, model.track(first));
        assert!(ctx(&mut app).modes.is_active(Modes::Volume));
        app.tick();
        // Track button lights in the shift view: volume mode is lit (red)
        let calls = backend.take_calls();
        assert_eq!(midi_out(&calls, 0x90, model.track(first)), Some(3), "{calls:?}");
        // The track buttons do not mark the shift view as used, it stays open (as in Java)
        note(&mut app, model.shift(), 0);
        app.tick();
        assert!(ctx(&mut app).views.is_active(Views::Shift));
        // The next press closes it
        press(&mut app, model.shift());
        assert!(!ctx(&mut app).views.is_active(Views::Shift));
        app.shutdown();
    }
}

#[test]
fn soft_keys_change_the_track_button_function() {
    let (mut app, backend) = start(Model::Mk2, "");
    ctx(&mut app).set_active_view(Views::Session);
    app.tick();
    backend.take_calls();

    // Default: clip stop
    press(&mut app, Model::Mk2.track(0));
    let calls = backend.take_calls();
    assert!(has_call(&calls, "track", "0/stop"), "{calls:?}");

    // Shift (a tap opens the shift view in the session view) + Scene 2: solo
    press(&mut app, Model::Mk2.shift());
    assert!(ctx(&mut app).views.is_active(Views::Shift));
    press(&mut app, Model::Mk2.scene(1));
    press(&mut app, Model::Mk2.shift());
    assert!(ctx(&mut app).views.is_active(Views::Session));
    assert_eq!(apc::track_state(ctx(&mut app)), apc::TRACK_STATE_SOLO);
    assert_eq!(config::soft_keys(&ctx(&mut app).config), "Solo");
    press(&mut app, Model::Mk2.track(1));
    let calls = backend.take_calls();
    assert!(has_call(&calls, "track", "1/solo"), "{calls:?}");

    // Mk2: Scene 3 is mute (switched with rec arm)
    press(&mut app, Model::Mk2.shift());
    press(&mut app, Model::Mk2.scene(2));
    press(&mut app, Model::Mk2.shift());
    assert_eq!(apc::track_state(ctx(&mut app)), apc::TRACK_STATE_MUTE);

    // The setting is applied on the next change check
    config::set_soft_keys(&mut ctx(&mut app).config, "Select");
    app.tick();
    assert_eq!(apc::track_state(ctx(&mut app)), apc::TRACK_STATE_SELECT);
    press(&mut app, Model::Mk2.track(1));
    let calls = backend.take_calls();
    assert!(has_call(&calls, "track", "1/select"), "{calls:?}");
    app.shutdown();
}

#[test]
fn shift_view_selects_views_and_transport() {
    let (mut app, backend) = start(Model::Mk1, "");
    backend.take_calls();
    note(&mut app, Model::Mk1.shift(), 127);
    // Pad 56: session view (switched after the pad event)
    press(&mut app, 56);
    app.tick();
    note(&mut app, Model::Mk1.shift(), 0);
    app.tick();
    // As in Java, the release of Shift opens the shift view again (the session view is a
    // session alternative)
    assert_eq!(ctx(&mut app).views.active_id_ignore_temporary(), Some(Views::Session));
    assert!(ctx(&mut app).views.is_active(Views::Shift));
    press(&mut app, Model::Mk1.shift());
    assert!(ctx(&mut app).views.is_active(Views::Session));

    // Pad 63: play
    ctx(&mut app).set_active_view(Views::Play);
    note(&mut app, Model::Mk1.shift(), 127);
    press(&mut app, 63);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "play", ""), "{calls:?}");
    // Pad 39: metronome
    press(&mut app, 39);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "transport", "toggleMetronome"), "{calls:?}");
    // Pad 1 (G): scale base
    press(&mut app, 4);
    assert_eq!(ctx(&mut app).config.scale_base(), "G");
    note(&mut app, Model::Mk1.shift(), 0);
    app.shutdown();
}

#[test]
fn play_view_changes_the_scale_with_the_scene_buttons() {
    let (mut app, _backend) = start(Model::Mk1, "");
    assert!(ctx(&mut app).views.is_active(Views::Play));
    let scale = ctx(&mut app).config.scale().to_string();
    press(&mut app, Model::Mk1.scene(4));
    assert_ne!(ctx(&mut app).config.scale(), scale);
    press(&mut app, Model::Mk1.scene(5));
    assert!(!ctx(&mut app).config.is_scale_in_key());
    let octave = ctx(&mut app).model.scales().octave();
    press(&mut app, Model::Mk1.scene(6));
    assert_eq!(ctx(&mut app).model.scales().octave(), octave + 1);
    app.shutdown();
}

#[test]
fn every_view_mode_and_control_works_on_all_models() {
    for model in [Model::Mk1, Model::Mk2] {
        let (mut app, _backend) = start(model, "");
        // A note clip, so that the sequencers are active
        app.update_model("track/0/type INSTRUMENT\nclip/exists 1\nclip/start 0\nclip/end 8\nclip/loop 1\nclip/notes 0:0:0.25:0.5:0:60:100\n");
        app.tick();
        let views = [Views::Play, Views::Session, Views::Sequencer, Views::Drum, Views::Raindrops, Views::Browser, Views::Shift];
        let modes = [
            Modes::Volume,
            Modes::Pan,
            Modes::Send1,
            Modes::Send2,
            Modes::Send3,
            Modes::Send4,
            Modes::Send5,
            Modes::Send6,
            Modes::Send7,
            Modes::Send8,
            Modes::DeviceParams,
        ];
        for view in views {
            assert!(ctx(&mut app).views.contains(view));
            ctx(&mut app).set_active_view(view);
            app.tick();
            for mode in modes {
                ctx(&mut app).set_active_mode(mode);
                assert!(ctx(&mut app).modes.is_active(mode));
                for i in 0..8u8 {
                    cc(&mut app, 48 + i, 100);
                }
                cc(&mut app, 56, 50);
                app.tick();
            }
            for track_state in 0..5 {
                apc::set_track_state(ctx(&mut app), track_state);
                let ids: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
                for id in ids {
                    // Shift toggles the shift view, which is tested separately
                    if id == ButtonId::Shift {
                        continue;
                    }
                    let ctx = ctx(&mut app);
                    ctx.trigger_button(id, ButtonEvent::Down, 1.0);
                    ctx.trigger_button(id, ButtonEvent::Long, 1.0);
                    ctx.trigger_button(id, ButtonEvent::Up, 0.0);
                }
                app.tick();
            }
            for control in [ContinuousId::FaderMaster, ContinuousId::Fader1, ContinuousId::Fader8] {
                assert!(ctx(&mut app).surface.continuous(control).is_some());
            }
        }
        // Shift itself
        ctx(&mut app).set_active_view(Views::Session);
        press(&mut app, model.shift());
        app.tick();
        press(&mut app, model.shift());
        app.tick();
        app.shutdown();
    }
}

#[test]
fn sequencer_track_buttons_change_octave_and_resolution() {
    let (mut app, backend) = start(Model::Mk2, "");
    app.update_model("track/0/type INSTRUMENT\nclip/exists 1\nclip/start 0\nclip/end 8\nclip/loop 1\nclip/notes 0:0:0.25:0.5:0:60:100\n");
    ctx(&mut app).set_active_view(Views::Sequencer);
    app.tick();
    backend.take_calls();
    // Mk2: the octave buttons are track buttons 5 and 6 (swapped with the fader mode buttons)
    let offset_y = |app: &mut App| {
        let view = ctx(app).views.get(Views::Sequencer).unwrap();
        let view = view.borrow();
        view.as_any().downcast_ref::<SequencerView>().unwrap().offset_y()
    };
    let offset = offset_y(&mut app);
    press(&mut app, Model::Mk2.track(4));
    assert!(offset_y(&mut app) > offset);
    press(&mut app, Model::Mk2.track(5));
    assert_eq!(offset_y(&mut app), offset);
    // The resolution is shown on the scene buttons (1/16 = index 4 -> scene 4)
    app.tick();
    let light = |app: &mut App, id: ButtonId| ctx(app).surface.button_light(id).and_then(|l| l.state());
    assert_eq!(light(&mut app, ButtonId::Scene4), Some(1));
    assert_eq!(light(&mut app, ButtonId::Scene1), Some(0));
    // Scene 1: resolution 1/1 (index 7)
    press(&mut app, Model::Mk2.scene(0));
    app.tick();
    assert_eq!(light(&mut app, ButtonId::Scene1), Some(1));
    assert_eq!(light(&mut app, ButtonId::Scene4), Some(0));
    app.shutdown();
}
