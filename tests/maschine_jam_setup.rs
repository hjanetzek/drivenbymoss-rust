// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Maschine JAM controller setup: buttons, faders, encoder, views, modes and LEDs.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::maschine_jam::MaschineJamConfiguration;
use drivenbymoss::controller::maschine::core::encoder_mode_manager::EncoderMode;
use drivenbymoss::controller::maschine::jam::surface as jam;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;
const SHIFT_DOWN: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x01, 0xF7];
const SHIFT_UP: [u8; 13] = [0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x00, 0xF7];

/// Start a Maschine JAM instance with 2 tracks (the first one selected) and wait for the delayed
/// startup.
fn start() -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-jam-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.ni.maschine.jam.MaschineJamControllerInstance\n").unwrap();

    let backend = Rc::new(RecordingBackend {
        inputs: vec![(IN, "Maschine Jam - 1".into())],
        outputs: vec![(OUT, "Maschine Jam - 1".into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    app.start_infrastructure();
    app.update_model(
        "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/1/exists 1\ntrack/1/name Bass\nplay 0\n",
    );
    assert!(app.instance_ctx(0).is_some(), "instance is running");
    // Startup is delayed by 1s
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    backend.take_calls();
    (app, backend)
}

fn cc(app: &mut App, control: u8, value: u8) {
    app.on_midi_event(IN, &[0xB0, control, value]);
}

fn press(app: &mut App, control: u8) {
    cc(app, control, 127);
    cc(app, control, 0);
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).unwrap()
}

/// The last value sent to a CC.
fn led(calls: &[BackendCall], control: u8) -> Option<u8> {
    calls.iter().rev().find_map(|c| match c {
        BackendCall::SendMidi(OUT, d) if d.len() == 3 && d[0] == 0xB0 && d[1] == control => Some(d[2]),
        _ => None,
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

fn contains(calls: &[BackendCall], text: &str) -> bool {
    calls.iter().any(|c| format!("{c:?}").contains(text))
}

#[test]
fn startup_activates_the_startup_view_and_mode() {
    let (mut app, _backend) = start();
    let ctx = ctx(&mut app);
    assert_eq!(ctx.views.active_id(), Some(Views::Play));
    assert!(ctx.modes.is_active(Modes::Volume));
    assert_eq!(ctx.surface.button(ButtonId::Play).unwrap().binding.map(|b| b.2), Some(jam::PLAY));
}

#[test]
fn sysex_shift_and_play_restarts() {
    let (mut app, backend) = start();
    app.on_midi_event(IN, &SHIFT_DOWN);
    assert!(ctx(&mut app).surface.is_shift_pressed());
    press(&mut app, jam::PLAY as u8);
    app.on_midi_event(IN, &SHIFT_UP);
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    let calls = backend.take_calls();
    // Shift + Play restarts the playback
    assert!(calls.iter().any(|c| matches!(c, BackendCall::NoArg(p, _) if p == "play")), "{calls:?}");
    // The shift view is not shown since Shift was used
    assert_ne!(ctx(&mut app).views.active_id(), Some(Views::Shift));

    // Shift alone toggles the shift view
    app.on_midi_event(IN, &SHIFT_DOWN);
    app.on_midi_event(IN, &SHIFT_UP);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Shift));
}

#[test]
fn play_led_and_touch_strips() {
    let (mut app, backend) = start();
    app.update_model("play 1\n");
    app.tick();
    let calls = backend.take_calls();
    assert_eq!(led(&calls, jam::PLAY as u8), Some(127));
    // While playing the volume mode uses dual touch strips with the VU
    let sysex = sysex(&calls);
    assert!(sysex.iter().any(|s| s.len() == 28 && s[10] == 0x05 && s[11] == 3), "{sysex:?}");
    // The VU meters of the touch strips
    assert!(led(&calls, jam::STRIP_LEFT as u8).is_some());
    assert!(led(&calls, jam::STRIP_RIGHT as u8).is_some());
}

#[test]
fn fader_changes_the_volume() {
    let (mut app, backend) = start();
    cc(&mut app, jam::FADER1 as u8, 100);
    let calls = backend.take_calls();
    assert!(contains(&calls, "0/volume"), "{calls:?}");
}

#[test]
fn level_and_aux_buttons_select_modes() {
    let (mut app, _backend) = start();
    // Level toggles between volume and panorama
    press(&mut app, jam::LEVEL as u8);
    assert!(ctx(&mut app).modes.is_active(Modes::Pan));
    press(&mut app, jam::LEVEL as u8);
    assert!(ctx(&mut app).modes.is_active(Modes::Volume));
    press(&mut app, jam::MACRO as u8);
    assert!(ctx(&mut app).modes.is_active(Modes::Track));
    press(&mut app, jam::CONTROL as u8);
    assert!(ctx(&mut app).modes.is_active(Modes::DeviceParams));
}

#[test]
fn encoder_modes() {
    let (mut app, backend) = start();
    let manager = jam::encoder_manager(ctx(&mut app));
    assert_eq!(manager.borrow().active_encoder_mode(), Some(EncoderMode::MasterVolume));
    cc(&mut app, jam::KNOB_TURN as u8, 1);
    let calls = backend.take_calls();
    assert!(contains(&calls, "master") && contains(&calls, "volume"), "{calls:?}");

    press(&mut app, jam::IN as u8);
    assert_eq!(manager.borrow().active_encoder_mode(), Some(EncoderMode::MetronomeVolume));

    // Tempo shows the tempo view while pressed and the encoder changes the tempo
    cc(&mut app, jam::TEMPO as u8, 127);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Tempo));
    backend.take_calls();
    cc(&mut app, jam::KNOB_TURN as u8, 1);
    assert!(contains(&backend.take_calls(), "tempo"));
    cc(&mut app, jam::TEMPO as u8, 0);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
}

#[test]
fn perform_changes_the_scale_of_the_play_view() {
    let (mut app, _backend) = start();
    let scale = ctx(&mut app).model.scales().scale();
    cc(&mut app, jam::PERFORM as u8, 127);
    cc(&mut app, jam::KNOB_TURN as u8, 1);
    cc(&mut app, jam::PERFORM as u8, 0);
    assert_ne!(ctx(&mut app).model.scales().scale(), scale);
    let ctx = ctx(&mut app);
    assert_eq!(ctx.config.scale(), ctx.model.scales().scale().name());
}

#[test]
fn step_and_scene_buttons_select_views() {
    let (mut app, _backend) = start();
    press(&mut app, jam::STEP as u8);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Sequencer));
    // Step + Scene 4 selects the drum view
    cc(&mut app, jam::STEP as u8, 127);
    press(&mut app, (jam::SCENE1 + 3) as u8);
    cc(&mut app, jam::STEP as u8, 0);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Drum));
    press(&mut app, jam::SONG as u8);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Session));
    press(&mut app, jam::SONG as u8);
    assert!(ctx(&mut app).config.is_flip_session());
}

#[test]
fn shift_select_toggles_slow_fader_change() {
    let (mut app, _backend) = start();
    app.on_midi_event(IN, &SHIFT_DOWN);
    press(&mut app, jam::SELECT as u8);
    app.on_midi_event(IN, &SHIFT_UP);
    assert!(MaschineJamConfiguration::is_slow_fader_change(&ctx(&mut app).config));
}

#[test]
fn group_buttons_select_tracks() {
    let (mut app, backend) = start();
    press(&mut app, (jam::GROUP_A + 1) as u8);
    let calls = backend.take_calls();
    assert!(contains(&calls, "track") && contains(&calls, "select"), "{calls:?}");
}

#[test]
fn all_views_modes_and_buttons() {
    let (mut app, _backend) = start();
    let views = [
        Views::Shift,
        Views::RepeatNote,
        Views::Session,
        Views::Play,
        Views::Chords,
        Views::Piano,
        Views::Drum64,
        Views::Drum,
        Views::Drum4,
        Views::Drum8,
        Views::Raindrops,
        Views::Sequencer,
        Views::PolySequencer,
        Views::Tempo,
        Views::Shuffle,
        Views::Browser,
        Views::Control,
    ];
    let mut modes = vec![Modes::Track, Modes::Volume, Modes::Pan, Modes::DeviceParams, Modes::User];
    modes.extend((0..8).map(|i| Modes::get(Modes::Send1, i)));

    for view in views {
        ctx(&mut app).set_active_view(view);
        assert_eq!(ctx(&mut app).views.active_id(), Some(view));
        for mode in &modes {
            ctx(&mut app).set_active_mode(*mode);
            assert!(ctx(&mut app).modes.is_active(*mode));
            cc(&mut app, jam::FADER1 as u8, 64);
            cc(&mut app, jam::KNOB_TURN as u8, 127);
        }
        // Pads
        for note in [36u8, 50, 70, 99] {
            app.on_midi_event(IN, &[0x90, note, 100]);
            app.on_midi_event(IN, &[0x80, note, 0]);
        }
        app.tick();
    }

    let button_ids = ctx(&mut app).surface.button_ids();
    for id in button_ids {
        if id >= ButtonId::Pad1 && id <= ButtonId::Pad64 {
            continue;
        }
        let ctx = ctx(&mut app);
        ctx.trigger_button(id, ButtonEvent::Down, 1.0);
        ctx.trigger_button(id, ButtonEvent::Long, 1.0);
        ctx.trigger_button(id, ButtonEvent::Up, 0.0);
        app.tick();
    }
    assert!(ctx(&mut app).surface.continuous(ContinuousId::MasterKnob).is_some());
    app.shutdown();
}

#[test]
fn slow_fader_change_increments_the_volume() {
    let (mut app, backend) = start();
    MaschineJamConfiguration::toggle_slow_fader_change(&mut ctx(&mut app).config);
    let volume = ctx(&mut app).model.current_track_bank().item(0).volume();
    // Each change increments by 1 (as in Java the first value is not only stored since the
    // initial value is -1 and not 0)
    cc(&mut app, jam::FADER1 as u8, 10);
    cc(&mut app, jam::FADER1 as u8, 11);
    let calls = backend.take_calls();
    assert!(contains(&calls, "0/volume"), "{calls:?}");
    let new_volume = ctx(&mut app).model.current_track_bank().item(0).volume();
    assert_eq!(new_volume, volume + 2);
}

/// The notes which the core sent to the note input of the DAW (note on, with velocity 0 a note
/// off).
fn notes(calls: &[BackendCall]) -> Vec<(i32, i32)> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::Midi(_, 0x90, note, velocity) => Some((*note, *velocity)),
            _ => None,
        })
        .collect()
}

#[test]
fn chords_sharing_notes_keep_them_until_both_are_released() {
    let (mut app, backend) = start();
    cc(&mut app, jam::PAD_MODE as u8, 127);
    press(&mut app, jam::SCENE1 as u8 + 1);
    cc(&mut app, jam::PAD_MODE as u8, 0);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Chords));
    backend.take_calls();

    // The triads of the 1st and 3rd pad of the lowest row: C (E G added) and E (G B added)
    app.on_midi_event(IN, &[0x90, 36, 100]);
    app.on_midi_event(IN, &[0x90, 38, 90]);
    assert_eq!(notes(&backend.take_calls()), vec![(40, 100), (43, 100), (47, 90)]);
    // Releasing C keeps E (the root of the E triad) and G
    app.on_midi_event(IN, &[0x90, 36, 0]);
    assert_eq!(notes(&backend.take_calls()), vec![]);
    app.on_midi_event(IN, &[0x90, 38, 0]);
    assert_eq!(notes(&backend.take_calls()), vec![(43, 0), (47, 0)]);

    // The other order: the note input ends E with the E triad, it is played again for C
    app.on_midi_event(IN, &[0x90, 38, 90]);
    app.on_midi_event(IN, &[0x90, 36, 100]);
    backend.take_calls();
    app.on_midi_event(IN, &[0x90, 38, 0]);
    assert_eq!(notes(&backend.take_calls()), vec![(47, 0), (40, 100)]);
    app.on_midi_event(IN, &[0x90, 36, 0]);
    assert_eq!(notes(&backend.take_calls()), vec![(40, 0), (43, 0)]);
}

/// The Shift view stays open only after a tap of Shift (not in the Java version). As in Java the
/// JAM does not show it while Shift is held (Shift + pad are the alternative functions), unless
/// the setting is on.
#[test]
fn shift_view_after_a_tap_and_the_hold_setting() {
    let (mut app, _backend) = start();
    let view = ctx(&mut app).views.active_id();

    // A tap opens the Shift view on release, the next press closes it
    app.on_midi_event(IN, &SHIFT_DOWN);
    assert_eq!(ctx(&mut app).views.active_id(), view);
    app.on_midi_event(IN, &SHIFT_UP);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Shift));
    app.on_midi_event(IN, &SHIFT_DOWN);
    app.on_midi_event(IN, &SHIFT_UP);
    assert_eq!(ctx(&mut app).views.active_id(), view);

    // Held longer than the tap time
    app.on_midi_event(IN, &SHIFT_DOWN);
    std::thread::sleep(Duration::from_millis(300));
    app.on_midi_event(IN, &SHIFT_UP);
    assert_eq!(ctx(&mut app).views.active_id(), view);

    // Combined with a button (Shift + arrow right: scroll by 1)
    app.on_midi_event(IN, &SHIFT_DOWN);
    press(&mut app, jam::NAV_RIGHT as u8);
    app.on_midi_event(IN, &SHIFT_UP);
    assert_eq!(ctx(&mut app).views.active_id(), view);

    // Setting on: the Shift view is shown while held, a tap keeps it
    ctx(&mut app).config.set_value(MaschineJamConfiguration::SHIFT_VIEW_WHILE_HELD, "On");
    app.on_midi_event(IN, &SHIFT_DOWN);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Shift));
    app.on_midi_event(IN, &SHIFT_UP);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Shift));
    app.on_midi_event(IN, &SHIFT_DOWN);
    app.on_midi_event(IN, &SHIFT_UP);
    assert_eq!(ctx(&mut app).views.active_id(), view);
}

/// A Shift combination which consumes Shift (Shift + LEFT: the previous page) does not leave the
/// knobs slow (the Java version does, UPSTREAM_CANDIDATES.md C).
#[test]
fn consumed_shift_resets_the_slow_knobs() {
    let (mut app, _backend) = start();
    app.on_midi_event(IN, &SHIFT_DOWN);
    assert!(ctx(&mut app).surface.is_knob_sensitivity_slow());
    press(&mut app, jam::LEFT as u8);
    assert!(ctx(&mut app).surface.is_trigger_consumed(ButtonId::Shift));
    app.on_midi_event(IN, &SHIFT_UP);
    assert!(!ctx(&mut app).surface.is_knob_sensitivity_slow());
}
