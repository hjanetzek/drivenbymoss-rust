// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Akai MPC/Force with the ACVS protocol: startup, ping/pong, the screen items and
//! texts, the buttons and knobs, the device switch and all controls on the MPC and the Force.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::acvs as config;
use drivenbymoss::controller::akai::acvs::definition::AcvsDevice;
use drivenbymoss::controller::akai::acvs::surface as acvs;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;
const PORT: &str = "Akai Network - DAW Control";

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\nplay 0\n";

fn start(device: Option<&str>) -> (App, Rc<RecordingBackend>, std::path::PathBuf) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-acvs-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.akai.acvs.ACVSLiveControllerInstance\n").unwrap();
    if let Some(device) = device {
        std::fs::write(
            dir.join("DrivenByMoss4Reaper-MPC-Live-I-II,-One,-Force.config"),
            format!("MIDI_INPUT0={PORT}\nMIDI_OUTPUT0={PORT}\nHARDWARE_SETUP_DEVICE={device}\n"),
        )
        .unwrap();
    }
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, PORT.into())], outputs: vec![(OUT, PORT.into())], ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    assert_eq!(app.instance_count(), 1);
    app.start_infrastructure();
    app.update_model(TRACKS);
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    (app, backend, dir)
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

/// The texts sent to the device (item ID, text).
fn texts(calls: &[BackendCall]) -> Vec<(usize, String)> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(OUT, d) if d.len() > 9 && d[0] == 0xF0 && d[4] == 0x10 => {
                let item = ((d[5] as usize) << 8) + d[6] as usize;
                Some((item, String::from_utf8_lossy(&d[9..d.len() - 1]).to_string()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn startup_ping_and_pong() {
    let (mut app, backend, _dir) = start(None);
    {
        let ctx = ctx(&mut app);
        assert!(ctx.views.is_active(Views::Control));
        assert!(ctx.modes.is_active(Modes::User));
        let track_modes = acvs::track_modes(ctx);
        assert!(ctx.mode_manager(track_modes).is_active(Modes::StopClip));
        assert_eq!(acvs::state(ctx).device, AcvsDevice::MpcLiveOne);
    }
    // The track name is sent as text and the track select as note
    app.tick();
    let calls = backend.take_calls();
    assert!(texts(&calls).contains(&(0x00, "Drums".to_string())), "{:?}", texts(&calls));
    assert!(calls.contains(&BackendCall::SendMidi(OUT, vec![0x90, 0, 127])), "{calls:?}");

    // The ping is sent 2 seconds after the startup
    std::thread::sleep(Duration::from_millis(2050));
    app.tick();
    let calls = backend.take_calls();
    assert!(calls.contains(&BackendCall::SendMidi(OUT, vec![0xF0, 0x47, 0x00, 0x3B, 0x00, 0xF7])), "{calls:?}");

    // The pong forces a flush of all items
    app.on_midi_event(IN, &[0xF0, 0x47, 0x00, 0x3B, 0x01, 0x00, 0xF7]);
    app.tick();
    let calls = backend.take_calls();
    assert!(texts(&calls).contains(&(0x00, "Drums".to_string())), "{:?}", texts(&calls));

    // Tempo set on the device
    let mut tempo = vec![0xF0, 0x47, 0x00, 0x3B, 0x10, 0x03, 0x00, 0x00, 0x03];
    tempo.extend_from_slice(b"125");
    tempo.push(0xF7);
    app.on_midi_event(IN, &tempo);
    assert!(backend.take_calls().iter().any(|c| matches!(c, BackendCall::DoubleArg(p, _, v) if p == "tempo" && (*v - 125.0).abs() < 0.001)));
    app.shutdown();
}

#[test]
fn a_pong_of_another_device_restarts_the_instance() {
    let (mut app, _backend, _dir) = start(None);
    app.on_midi_event(IN, &[0xF0, 0x47, 0x00, 0x40, 0x01, 0x00, 0xF7]);
    assert_eq!(config::active_device(&ctx(&mut app).config), AcvsDevice::Force);
    app.tick();
    assert_eq!(acvs::state(ctx(&mut app)).device, AcvsDevice::Force);
    // The Force buttons are registered
    assert!(ctx(&mut app).surface.button(ButtonId::TrackAssign1).is_some());
    app.shutdown();
}

#[test]
fn buttons_and_knobs() {
    let (mut app, backend, _dir) = start(None);
    backend.take_calls();
    // Track select 2
    press(&mut app, 0, 1);
    assert!(has_call(&backend.take_calls(), "track", "1/select"));
    // Solo track 1 (channel 2)
    press(&mut app, 1, 0);
    assert!(has_call(&backend.take_calls(), "track", "0/solo"));
    // Clip launch
    press(&mut app, 0, 24);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| format!("{c:?}").contains("clip/0/launch") || format!("{c:?}").contains("launch")), "{calls:?}");
    // Volume fader of track 1
    cc(&mut app, 1, 0, 100);
    assert!(has_call(&backend.take_calls(), "track", "0/volume"));
    // Device knob
    cc(&mut app, 9, 0, 100);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| format!("{c:?}").contains("param/0/value")), "{calls:?}");
    // MPC: 16 Level toggles launching clips/scenes
    press(&mut app, 0x0C, 70);
    assert!(!config::is_launch_clips(&ctx(&mut app).config));
    // Metronome on the touch display
    press(&mut app, 0x0A, 0);
    assert!(has_call(&backend.take_calls(), "transport", "toggleMetronome"));
    app.shutdown();
}

#[test]
fn every_control_works_on_mpc_mpc_x_and_force() {
    for device in ["MPC Live / Live II / One", "MPC X", "Force"] {
        let (mut app, _backend, _dir) = start(Some(device));
        assert_eq!(acvs::state(ctx(&mut app)).device.name(), device);
        let track_modes = acvs::track_modes(ctx(&mut app));
        for mode in [Modes::Mute, Modes::Solo, Modes::RecArm, Modes::StopClip, Modes::CrossfadeModeA, Modes::CrossfadeModeB] {
            ctx(&mut app).set_active_mode_in(track_modes, mode);
            for launch_clips in [true, false] {
                config::set_launch_clips_or_scenes(&mut ctx(&mut app).config, launch_clips);
                let ids: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
                for id in ids {
                    let ctx = ctx(&mut app);
                    ctx.trigger_button(id, ButtonEvent::Down, 1.0);
                    ctx.trigger_button(id, ButtonEvent::Long, 1.0);
                    ctx.trigger_button(id, ButtonEvent::Up, 0.0);
                }
                // With Shift
                let ids: Vec<ButtonId> = ctx(&mut app).surface.button_ids();
                for id in ids {
                    let ctx = ctx(&mut app);
                    ctx.trigger_button(ButtonId::Shift, ButtonEvent::Down, 1.0);
                    ctx.trigger_button(id, ButtonEvent::Down, 0.5);
                    ctx.trigger_button(id, ButtonEvent::Up, 0.0);
                    ctx.trigger_button(ButtonId::Shift, ButtonEvent::Up, 0.0);
                }
                for channel in [1u8, 2, 9, 0x0A, 0x0D] {
                    for control in 0..17u8 {
                        cc(&mut app, channel, control, 65);
                        cc(&mut app, channel, control, 1);
                    }
                }
                // Knob touches
                for n in 0..16u8 {
                    note(&mut app, 0x0D, n, 127);
                    note(&mut app, 0x0D, n, 0);
                }
                app.tick();
            }
        }
        assert!(ctx(&mut app).surface.continuous(ContinuousId::Crossfader).is_some());
        app.shutdown();
    }
}
