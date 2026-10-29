// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Launchpad play, session, mixer and other non-sequencer views.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::controller::launchpad::colors::*;
use drivenbymoss::controller::launchpad::surface::translate_to_controller;
use drivenbymoss::controller::launchpad::view::chords_view::ChordsView;
use drivenbymoss::controller::launchpad::view::piano_view::PianoView;
use drivenbymoss::controller::launchpad::view::device_view::DeviceView;
use drivenbymoss::controller::launchpad::view::mix_view::MixView;
use drivenbymoss::controller::launchpad::view::note_view_select_view::NoteViewSelectView;
use drivenbymoss::controller::launchpad::view::pan_view::PanView;
use drivenbymoss::controller::launchpad::view::play_view::PlayView;
use drivenbymoss::controller::launchpad::view::project_view::ProjectView;
use drivenbymoss::controller::launchpad::view::sends_view::SendsView;
use drivenbymoss::controller::launchpad::view::session_view::{is_session_view, SessionView};
use drivenbymoss::controller::launchpad::view::shift_view::ShiftView;
use drivenbymoss::controller::launchpad::view::tempo_view::{LaunchpadShuffleView, LaunchpadTempoView};
use drivenbymoss::controller::launchpad::view::user_view::UserView;
use drivenbymoss::controller::launchpad::view::volume_view::VolumeView;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, Views};

const IN: i32 = 3;
const OUT: i32 = 5;

/// Start a Launchpad X instance with 2 tracks (the first one selected), register the views of
/// this module and wait for the delayed startup.
fn start() -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-views-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("DrivenByMoss4Reaper.config"),
        "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadXControllerInstance\n",
    )
    .unwrap();

    let backend = Rc::new(RecordingBackend {
        inputs: vec![(IN, "Launchpad X LPX MIDI Out".into())],
        outputs: vec![(OUT, "Launchpad X LPX MIDI In".into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    app.start_infrastructure();
    app.update_model(
        "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/1/exists 1\ntrack/1/name Bass\nplay 0\n",
    );
    {
        let ctx = app.instance_ctx(0).expect("instance is running");
        let session = SessionView::new("Session", ctx);
        ctx.register_view(Views::Session, session);
        let view = ShiftView::new(ctx);
        ctx.register_view(Views::Shift, view);
        let view = VolumeView::new(ctx);
        ctx.register_view(Views::TrackVolume, view);
        let view = PanView::new(ctx);
        ctx.register_view(Views::TrackPan, view);
        let view = SendsView::new(ctx);
        ctx.register_view(Views::TrackSends, view);
        let view = DeviceView::new(ctx);
        ctx.register_view(Views::Device, view);
        let view = UserView::new(ctx);
        ctx.register_view(Views::User, view);
        let view = MixView::new(ctx);
        ctx.register_view(Views::Mix, view);
        let view = NoteViewSelectView::new(ctx);
        ctx.register_view(Views::Control, view);
        let view = ProjectView::new(ctx);
        ctx.register_view(Views::Project, view);
        let view = LaunchpadTempoView::new(ctx, LAUNCHPAD_COLOR_BLUE_HI, LAUNCHPAD_COLOR_WHITE, LAUNCHPAD_COLOR_BLACK);
        ctx.register_view(Views::Tempo, view);
        let view = LaunchpadShuffleView::new(ctx, LAUNCHPAD_COLOR_PINK_HI, LAUNCHPAD_COLOR_WHITE, LAUNCHPAD_COLOR_BLACK);
        ctx.register_view(Views::Shuffle, view);
        let view = PlayView::new(ctx);
        ctx.register_view(Views::Play, view);
        let view = ChordsView::new(ctx);
        ctx.register_view(Views::Chords, view);
        let view = PianoView::new(ctx);
        ctx.register_view(Views::Piano, view);
    }
    // Startup is delayed by 1s
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    backend.take_calls();
    (app, backend)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).unwrap()
}

/// Press (velocity > 0) or release a pad, addressed by its grid note (36-99).
fn pad(app: &mut App, grid_note: i32, velocity: u8) {
    let (_, note) = translate_to_controller(grid_note);
    app.on_midi_event(IN, &[0x90, note as u8, velocity]);
}

fn activate(app: &mut App, view: Views) {
    ctx(app).set_active_view(view);
    app.tick();
}

fn grid_color(app: &mut App, grid_note: i32) -> i32 {
    ctx(app).surface.grid_ref().light_info(grid_note).color
}

fn has_no_arg(calls: &[BackendCall], processor: &str, command: &str) -> bool {
    calls.iter().any(|c| matches!(c, BackendCall::NoArg(p, cmd) if p == processor && cmd == command))
}

/// Find a call with a numeric value for the given processor and command.
fn number_arg(calls: &[BackendCall], processor: &str, command: &str) -> Option<f64> {
    calls.iter().rev().find_map(|c| match c {
        BackendCall::IntArg(p, cmd, v) if p == processor && cmd == command => Some(*v as f64),
        BackendCall::DoubleArg(p, cmd, v) if p == processor && cmd == command => Some(*v),
        _ => None,
    })
}

#[test]
fn session_view_draws_and_launches_clips() {
    let (mut app, backend) = start();
    ctx(&mut app).model.set_clips("0;1;Clip;0;;0;");
    activate(&mut app, Views::Session);
    assert!(is_session_view(&*ctx(&mut app).active_view().unwrap().borrow()));

    // Upper left pad: track 0, slot 0 has content (amber, no clip color for empty IDs)
    assert_ne!(grid_color(&mut app, 92), LAUNCHPAD_COLOR_BLACK);
    // No clip on track 1
    assert_eq!(grid_color(&mut app, 93), LAUNCHPAD_COLOR_BLACK);

    backend.take_calls();
    pad(&mut app, 92, 127);
    pad(&mut app, 92, 0);
    let calls = backend.take_calls();
    assert!(has_no_arg(&calls, "track", "0/clip/0/launch"), "{calls:?}");

    // Releasing the pad requests the temporary mode of the session command
    assert!(drivenbymoss::controller::launchpad::surface::ext(ctx(&mut app)).session_temporary);
}

#[test]
fn session_view_birds_eye() {
    let (mut app, _backend) = start();
    activate(&mut app, Views::Session);
    {
        let ctx = ctx(&mut app);
        let view = ctx.views.get(Views::Session).unwrap();
        view.borrow_mut().set_birds_eye_active(true);
        assert!(view.borrow().is_birds_eye_active());
    }
    app.tick();
    // The first page is selected
    assert_eq!(grid_color(&mut app, 92), LAUNCHPAD_COLOR_GREEN);
}

#[test]
fn volume_view_moves_the_fader() {
    let (mut app, backend) = start();
    activate(&mut app, Views::TrackVolume);
    assert!(is_session_view(&*ctx(&mut app).active_view().unwrap().borrow()));

    // The volume of track 1 (0.5) is drawn in the first column: 4 pads lit from the bottom
    let colors: Vec<i32> = (0..8).map(|row| grid_color(&mut app, 36 + row * 8)).collect();
    assert!(colors[0] > 0 && colors[3] == colors[0], "{colors:?}");
    assert_eq!(colors[7], 0, "{colors:?}");

    // Press the top pad of the first column with full velocity
    backend.take_calls();
    pad(&mut app, 92, 127);
    pad(&mut app, 92, 0);
    let calls = backend.take_calls();
    let volume = number_arg(&calls, "track", "0/volume").expect("volume is sent");
    assert!(volume > 0.5, "{volume}");

    // The fader continues moving in steps by scheduled tasks
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(10));
        app.tick();
    }
    assert_eq!(ctx(&mut app).model.current_track_bank().item(0).volume(), 127);
}

#[test]
fn sends_view_selects_the_send() {
    let (mut app, _backend) = start();
    activate(&mut app, Views::TrackSends);
    let ctx = ctx(&mut app);
    let view = ctx.views.get(Views::TrackSends).unwrap();
    view.borrow_mut().on_button(ctx, ButtonId::Scene3, drivenbymoss::midi::ButtonEvent::Down, 127);
    let selected = view.borrow().as_any().downcast_ref::<SendsView>().unwrap().selected_send();
    assert_eq!(selected, 2);
    assert_eq!(view.borrow().button_color(ctx, ButtonId::Scene3), LAUNCHPAD_COLOR_ORCHID);
}

#[test]
fn mix_view_toggles_mute() {
    let (mut app, backend) = start();
    activate(&mut app, Views::Mix);
    // Row 3 from the bottom is mute
    assert_eq!(grid_color(&mut app, 52), LAUNCHPAD_COLOR_YELLOW_HI);
    backend.take_calls();
    pad(&mut app, 52 + 1, 127);
    pad(&mut app, 52 + 1, 0);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::IntArg(p, cmd, 1) if p == "track" && cmd == "1/mute")), "{calls:?}");
}

#[test]
fn play_view_blocks_notes_with_play_controls() {
    let (mut app, _backend) = start();
    activate(&mut app, Views::Play);
    let note_of_first_pad = |app: &mut App| ctx(app).surface.key_translation_table()[translate_to_controller(36).1 as usize];
    assert!(note_of_first_pad(&mut app) >= 0);

    // Scene 4 toggles the play controls which block the first row
    {
        let ctx = ctx(&mut app);
        let view = ctx.views.get(Views::Play).unwrap();
        view.borrow_mut().on_button(ctx, ButtonId::Scene4, drivenbymoss::midi::ButtonEvent::Down, 127);
        assert_eq!(view.borrow().button_color(ctx, ButtonId::Scene4), LAUNCHPAD_COLOR_GREEN_HI);
    }
    assert_eq!(note_of_first_pad(&mut app), -1);
    app.tick();
    // Sustain pad of the play controls
    assert_eq!(grid_color(&mut app, 36), LAUNCHPAD_COLOR_YELLOW_LO);

    // Scene 5 switches to the chords view
    {
        let ctx = ctx(&mut app);
        let view = ctx.views.get(Views::Play).unwrap();
        view.borrow_mut().on_button(ctx, ButtonId::Scene5, drivenbymoss::midi::ButtonEvent::Down, 127);
    }
    app.tick();
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Chords));
}

#[test]
fn played_pads_are_marked() {
    let (mut app, _backend) = start();
    activate(&mut app, Views::Play);
    pad(&mut app, 37, 100);
    app.tick();
    assert_eq!(grid_color(&mut app, 37), ctx(&mut app).colors.color_index("PLAY_VIEW_COLOR_PLAY"));
    pad(&mut app, 37, 0);
    app.tick();
    assert_ne!(grid_color(&mut app, 37), ctx(&mut app).colors.color_index("PLAY_VIEW_COLOR_PLAY"));
}

#[test]
fn tempo_view_displays_the_tempo_and_returns() {
    let (mut app, _backend) = start();
    app.update_model("tempo 123\n");
    activate(&mut app, Views::Session);
    ctx(&mut app).set_temporary_view(Views::Tempo);
    app.tick();
    // Digit "1": first column of the first two columns is lit
    assert_eq!(grid_color(&mut app, 92), LAUNCHPAD_COLOR_BLUE_HI);
    assert_eq!(grid_color(&mut app, 93), LAUNCHPAD_COLOR_BLACK);
    // Releasing a pad restores the previous view
    pad(&mut app, 40, 127);
    pad(&mut app, 40, 0);
    app.tick();
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Session));
}

#[test]
fn note_view_select_view_activates_a_view() {
    let (mut app, _backend) = start();
    activate(&mut app, Views::Session);
    ctx(&mut app).set_temporary_view(Views::Control);
    app.tick();
    // The play view was active before the session view
    assert_eq!(grid_color(&mut app, 92), COLOR_VIEW_SELECTED);
    assert_eq!(grid_color(&mut app, 93), COLOR_VIEW_PLAY);
    pad(&mut app, 93, 127);
    pad(&mut app, 93, 0);
    app.tick();
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Piano));
}

#[test]
fn shift_view_sets_the_clip_length() {
    let (mut app, _backend) = start();
    activate(&mut app, Views::Shift);
    pad(&mut app, 40, 127);
    pad(&mut app, 40, 0);
    app.tick();
    let ctx = ctx(&mut app);
    assert_eq!(ctx.config.new_clip_length(), 4);
    assert!(ctx.views.get(Views::Shift).unwrap().borrow().was_shift_view_used());
    assert_eq!(ctx.surface.grid_ref().light_info(40).color, LAUNCHPAD_COLOR_WHITE);
}

#[test]
fn project_view_toggles_the_mixer() {
    let (mut app, backend) = start();
    activate(&mut app, Views::Project);
    assert_eq!(grid_color(&mut app, 42), LAUNCHPAD_COLOR_LIME_HI);
    backend.take_calls();
    pad(&mut app, 97, 127);
    let calls = backend.take_calls();
    // Next project is a Reaper action
    assert!(calls.iter().any(|c| matches!(c, BackendCall::IntArg(p, _, _) if p == "action")), "{calls:?}");
}
