// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Launchpad controller setup: buttons, modes, view switching and button LEDs.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::{SettingId, TransportBehavior};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::feature::View;
use drivenbymoss::framework::ids::{ButtonId, Modes, Views};
use drivenbymoss::midi::BindType;

const IN: i32 = 3;
const OUT: i32 = 5;

/// A view which does nothing, for views which are not yet ported.
struct TestView(&'static str);

impl View for TestView {
    fn name(&self) -> &str {
        self.0
    }

    fn draw_grid(&mut self, _ctx: &mut Ctx) {}

    fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {}

    drivenbymoss::impl_as_any!();
}

/// Start a Launchpad instance with 2 tracks (the first one selected) and wait for the delayed
/// startup.
fn start(name: &str, class: &str, input: &str, output: &str) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-setup-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("DrivenByMoss4Reaper.config"),
        format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchpad.{class}\n"),
    )
    .unwrap();

    let backend = Rc::new(RecordingBackend {
        inputs: vec![(IN, input.into())],
        outputs: vec![(OUT, output.into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    app.start_infrastructure();
    app.update_model("track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/1/exists 1\ntrack/1/name Bass\nplay 0\n");
    {
        let ctx = app.instance_ctx(0).expect("instance is running");
        for (id, name) in [(Views::Session, "Session"), (Views::Control, "Note View Select"), (Views::Shift, "Shift")] {
            ctx.register_view(id, TestView(name));
        }
    }
    // Startup is delayed by 1s
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    backend.take_calls();
    (app, backend)
}

fn start_pro_mk3() -> (App, Rc<RecordingBackend>) {
    start("mk3", "LaunchpadProMk3ControllerInstance", "Launchpad Pro MK3 LPProMK3 MIDI", "Launchpad Pro MK3 LPProMK3 MIDI")
}

fn start_x() -> (App, Rc<RecordingBackend>) {
    start("x", "LaunchpadXControllerInstance", "Launchpad X LPX MIDI Out", "Launchpad X LPX MIDI In")
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

/// The last value sent to the CC of a button LED.
fn led(calls: &[BackendCall], control: u8) -> Option<u8> {
    calls.iter().rev().find_map(|c| match c {
        BackendCall::SendMidi(OUT, d) if d.len() == 3 && d[0] == 0xB0 && d[1] == control => Some(d[2]),
        _ => None,
    })
}

fn has_no_arg(calls: &[BackendCall], processor: &str) -> bool {
    calls.iter().any(|c| matches!(c, BackendCall::NoArg(p, _) if p == processor))
}

#[test]
fn startup_activates_the_startup_view_and_the_dummy_mode() {
    let (mut app, _backend) = start_x();
    let ctx = ctx(&mut app);
    assert_eq!(ctx.views.active_id(), Some(Views::Play));
    assert!(ctx.modes.is_active(Modes::Dummy));
    // Scene buttons are bound as CC on the X
    assert_eq!(ctx.surface.button(ButtonId::Scene1).unwrap().binding.map(|b| b.0), Some(BindType::Cc));
}

#[test]
fn mk2_scene_buttons_use_notes() {
    let (mut app, _backend) = start("mk2", "LaunchpadMkIIControllerInstance", "Launchpad MK2", "Launchpad MK2");
    let ctx = ctx(&mut app);
    assert_eq!(ctx.surface.button(ButtonId::Scene1).unwrap().binding, Some((BindType::Note, 0, 89, -1)));
    assert_eq!(ctx.surface.button(ButtonId::Shift).unwrap().binding, Some((BindType::Cc, 0, 111, -1)));
}

#[test]
fn play_button_starts_playback_and_shift_play_does_not() {
    let (mut app, backend) = start_pro_mk3();
    press(&mut app, 20);
    assert!(has_no_arg(&backend.take_calls(), "play"));

    // Shift + Play creates a new clip (PlayAndNewCommand)
    cc(&mut app, 90, 127);
    press(&mut app, 20);
    cc(&mut app, 90, 0);
    let calls = backend.take_calls();
    assert!(!has_no_arg(&calls, "play"), "{calls:?}");
}

#[test]
fn double_click_on_play_returns_to_zero() {
    let (mut app, backend) = start_pro_mk3();
    ctx(&mut app).config.set_value(SettingId::BEHAVIOUR_ON_PAUSE, TransportBehavior::Stop.name());
    press(&mut app, 20);
    assert!(has_no_arg(&backend.take_calls(), "play"));
    press(&mut app, 20);
    let calls = backend.take_calls();
    assert!(has_no_arg(&calls, "stop"), "{calls:?}");
    assert!(!has_no_arg(&calls, "play"), "{calls:?}");
}

#[test]
fn mute_mode_toggles_mute_with_the_track_buttons() {
    let (mut app, backend) = start_pro_mk3();

    press(&mut app, 2);
    assert!(ctx(&mut app).modes.is_active(Modes::Mute));
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Session), "mode buttons switch to the session view");

    ctx(&mut app).surface.clear_cache();
    app.tick();
    let calls = backend.take_calls();
    // Mute button yellow, track buttons yellow (low) for existing tracks, black otherwise
    assert_eq!(led(&calls, 2), Some(14));
    assert_eq!(led(&calls, 101), Some(15));
    // (the 3rd entry is the master track of the flat track list)
    assert_eq!(led(&calls, 108), Some(0));

    // The 1st track selection button mutes the 1st track
    press(&mut app, 101);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| format!("{c:?}").contains("mute")), "{calls:?}");

    // Pressing Mute again switches back to the dummy mode
    press(&mut app, 2);
    assert!(ctx(&mut app).modes.is_active(Modes::Dummy));
}

#[test]
fn shift_with_solo_toggles_the_metronome_on_the_pro() {
    let (mut app, backend) = start_pro_mk3();
    cc(&mut app, 90, 127);
    press(&mut app, 3);
    cc(&mut app, 90, 0);
    let calls = backend.take_calls();
    // Toggle metronome is a Reaper action
    assert!(calls.contains(&BackendCall::IntArg("action".into(), "".into(), 40364)), "{calls:?}");
    assert!(!ctx(&mut app).modes.is_active(Modes::Solo));
}

#[test]
fn session_button_selects_the_session_view_and_flips() {
    let (mut app, _backend) = start_x();
    press(&mut app, 95);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Session));
    assert!(!ctx(&mut app).config.is_flip_session());
    // Pressing it again in session view flips the session
    press(&mut app, 95);
    assert!(ctx(&mut app).config.is_flip_session());
}

#[test]
fn note_button_toggles_the_note_view_selection() {
    let (mut app, _backend) = start_x();
    // In a play view: show the view selection
    press(&mut app, 96);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Control));
    // Again: back to the play view
    press(&mut app, 96);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    // From the session view: recall the preferred view of the track
    press(&mut app, 95);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Session));
    press(&mut app, 96);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
}

#[test]
fn pro_mk3_note_button_cycles_the_play_views() {
    let (mut app, _backend) = start_pro_mk3();
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    press(&mut app, 94);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Piano));
    press(&mut app, 94);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Drum64));
    press(&mut app, 94);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    // The Sequencer button
    press(&mut app, 97);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Sequencer));
}

#[test]
fn button_leds() {
    let (mut app, backend) = start_x();
    // Force sending all LEDs
    ctx(&mut app).surface.clear_cache();
    app.tick();
    let calls = backend.take_calls();
    // Shift grey, Session grey, Note in Play view amber (view color)
    assert_eq!(led(&calls, 98), Some(1));
    assert_eq!(led(&calls, 95), Some(1));
    assert_eq!(led(&calls, 96), Some(10));

    // Shift pressed: white
    cc(&mut app, 98, 127);
    app.tick();
    assert_eq!(led(&backend.take_calls(), 98), Some(3));
    cc(&mut app, 98, 0);

    // Session view: Session button lime
    press(&mut app, 95);
    app.tick();
    assert_eq!(led(&backend.take_calls(), 95), Some(18));
}
