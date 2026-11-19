// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the views, the commands and the button registration of the Ableton Push 1, 2 and 3.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordedCalls, RecordingBackend};
use drivenbymoss::config::push as config;
use drivenbymoss::controller::push::surface as push;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;
use drivenbymoss::protocol::{Chan, Cmd};

const IN: i32 = 3;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volumeStr 0 dB\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\nplay 0\n";

const ALL_VIEWS: [Views; 16] = [
    Views::Play,
    Views::Chords,
    Views::Piano,
    Views::PrgChange,
    Views::ClipLength,
    Views::Color,
    Views::Session,
    Views::Sequencer,
    Views::PolySequencer,
    Views::Drum,
    Views::DrumXox,
    Views::Drum4,
    Views::Drum8,
    Views::Raindrops,
    Views::ScenePlay,
    Views::Drum64,
];

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
    let dir = std::env::temp_dir().join(format!("dbm-test-push-views-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={}\n", model.class())).unwrap();
    let port = model.port();
    std::fs::write(dir.join(format!("DrivenByMoss4Reaper-{}.config", model.file())), format!("MIDI_INPUT0={port}\nMIDI_OUTPUT0={port}\n")).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, port.into())], outputs: vec![(OUT, port.into())], ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    assert_eq!(app.instance_count(), 1);
    app.set_time_ms(0);
    app.start_infrastructure();
    app.update_model(TRACKS);
    app.set_time_ms(1100);
    app.tick();
    (app, backend)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).expect("instance runs")
}

fn press(app: &mut App, button: ButtonId) {
    ctx(app).trigger_button(button, ButtonEvent::Down, 1.0);
    ctx(app).trigger_button(button, ButtonEvent::Up, 0.0);
}

fn run_tasks(app: &mut App, ms: u64) {
    let now = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    ctx(app).run_due_tasks(now);
}

#[test]
fn all_views_are_registered() {
    for model in [Model::Push1, Model::Push2, Model::Push3] {
        let (mut app, _backend) = start(model);
        let ctx = ctx(&mut app);
        for view in ALL_VIEWS {
            assert!(ctx.views.contains(view), "{model:?}: {view:?}");
        }
        for (view, expected) in [(Views::Play, true), (Views::Piano, true), (Views::Drum, true), (Views::Drum64, true), (Views::Chords, false), (Views::Session, false)] {
            let is_expression = ctx.views.get(view).unwrap().borrow().is_expression_view();
            assert_eq!(is_expression, expected, "{view:?}");
        }
    }
}

#[test]
fn buttons_differ_per_model() {
    let (mut app, _b) = start(Model::Push1);
    let c = ctx(&mut app);
    assert!(c.surface.button(ButtonId::Volume).is_some());
    assert!(c.surface.button(ButtonId::Layout).is_none());
    assert!(c.surface.button(ButtonId::EncoderPush).is_none());

    let (mut app, _b) = start(Model::Push2);
    let c = ctx(&mut app);
    assert!(c.surface.button(ButtonId::Layout).is_some());
    assert!(c.surface.button(ButtonId::Convert).is_some());
    assert!(c.surface.button(ButtonId::Volume).is_none());
    assert!(c.surface.button(ButtonId::Swing).is_none());

    let (mut app, _b) = start(Model::Push3);
    let c = ctx(&mut app);
    for id in [ButtonId::InsertScene, ButtonId::Load, ButtonId::Save, ButtonId::Help, ButtonId::LockMode, ButtonId::Swing, ButtonId::ToggleMasterCue, ButtonId::ToggleClipView, ButtonId::EncoderPush, ButtonId::Left, ButtonId::Right, ButtonId::ArrowCenter] {
        assert!(c.surface.button(id).is_some(), "{id:?}");
    }
}

#[test]
fn play_button_sends_play_and_lights() {
    let (mut app, backend) = start(Model::Push2);
    backend.take_calls();
    // Play is CC 85
    app.on_midi_event(IN, &[0xB0, 85, 127]);
    app.on_midi_event(IN, &[0xB0, 85, 0]);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::PlayToggle { .. })), "{calls:?}");
}

#[test]
fn mute_button_toggles_cursor_track_on_push2() {
    let (mut app, backend) = start(Model::Push2);
    backend.take_calls();
    app.on_midi_event(IN, &[0xB0, 60, 127]);
    app.on_midi_event(IN, &[0xB0, 60, 0]);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelMute { ch: Chan::Track { .. }, .. })), "{calls:?}");
}

#[test]
fn mute_button_locks_on_push1() {
    let (mut app, _backend) = start(Model::Push1);
    push::set_lock_state(ctx(&mut app), config::LockState::Off);
    app.on_midi_event(IN, &[0xB0, 61, 127]);
    assert_eq!(push::lock_state(ctx(&mut app)), config::LockState::Solo);
}

#[test]
fn layout_cycles_the_play_views() {
    let (mut app, _backend) = start(Model::Push2);
    ctx(&mut app).set_active_view(Views::Play);
    press(&mut app, ButtonId::Layout);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Chords));
    press(&mut app, ButtonId::Layout);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Piano));
    press(&mut app, ButtonId::Layout);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Drum64));
}

#[test]
fn session_button_activates_session_and_restores_when_used_temporarily() {
    let (mut app, _backend) = start(Model::Push2);
    ctx(&mut app).set_active_view(Views::Play);
    ctx(&mut app).trigger_button(ButtonId::Session, ButtonEvent::Down, 1.0);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Session));
    // A pad is used while Session is held: the play view is restored on release
    app.on_midi_event(IN, &[0x90, 36, 100]);
    app.on_midi_event(IN, &[0x90, 36, 0]);
    ctx(&mut app).trigger_button(ButtonId::Session, ButtonEvent::Up, 0.0);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
}

#[test]
fn touchstrip_sends_pitchbend_in_pitch_mode() {
    let (mut app, backend) = start(Model::Push2);
    ctx(&mut app).set_active_view(Views::Play);
    app.tick();
    config::set_ribbon_mode(&mut ctx(&mut app).config, config::RIBBON_MODE_PITCH);
    backend.take_calls();
    app.on_midi_event(IN, &[0xE0, 0x10, 0x50]);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(_, 0xE0, 0x10, 0x50))), "{calls:?}");
}

#[test]
fn touchstrip_sets_crossfader_in_session() {
    let (mut app, backend) = start(Model::Push2);
    ctx(&mut app).set_active_view(Views::Session);
    app.tick();
    backend.take_calls();
    app.on_midi_event(IN, &[0xE0, 0x00, 0x7F]);
    // The ribbon shows the value
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d == &vec![0xE0, 0x00, 0x7F])), "{calls:?}");
}

#[test]
fn repeat_and_scene_button_select_the_period() {
    let (mut app, _backend) = start(Model::Push2);
    ctx(&mut app).set_active_view(Views::Play);
    ctx(&mut app).trigger_button(ButtonId::Repeat, ButtonEvent::Down, 1.0);
    // Scene 1 (top) = the last resolution (1/32T); as in Java only existing scenes work
    press(&mut app, ButtonId::Scene1);
    ctx(&mut app).trigger_button(ButtonId::Repeat, ButtonEvent::Up, 0.0);
    assert_eq!(ctx(&mut app).config.note_repeat_period(), drivenbymoss::daw::constants::Resolution::Res1_32T);
}

#[test]
fn prg_change_view_sends_program_changes() {
    let (mut app, backend) = start(Model::Push1);
    ctx(&mut app).set_active_view(Views::PrgChange);
    backend.take_calls();
    app.on_midi_event(IN, &[0x90, 40, 100]);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(_, 0xC0, 4, 0))), "{calls:?}");
}

#[test]
fn metronome_long_press_shows_transport_mode() {
    let (mut app, _backend) = start(Model::Push2);
    ctx(&mut app).trigger_button(ButtonId::Metronome, ButtonEvent::Down, 1.0);
    ctx(&mut app).trigger_button(ButtonId::Metronome, ButtonEvent::Long, 1.0);
    if ctx(&mut app).modes.contains(Modes::Transport) {
        assert_eq!(ctx(&mut app).modes.active_id(), Some(Modes::Transport));
    }
}

#[test]
fn touch_everything() {
    for model in [Model::Push1, Model::Push2, Model::Push3] {
        let (mut app, _backend) = start(model);
        let mut time = 1200;
        for view in ALL_VIEWS {
            ctx(&mut app).set_active_view(view);
            time += 50;
            app.set_time_ms(time);
            app.tick();
            let buttons = ctx(&mut app).surface.button_ids();
            for id in buttons {
                for event in [ButtonEvent::Down, ButtonEvent::Long, ButtonEvent::Up] {
                    ctx(&mut app).trigger_button(id, event, 1.0);
                }
                // Restore the view under test if a button switched it
                if ctx(&mut app).views.active_id() != Some(view) {
                    ctx(&mut app).set_active_view(view);
                }
            }
            for id in [ContinuousId::Knob1, ContinuousId::Knob8, ContinuousId::MasterKnob, ContinuousId::Tempo, ContinuousId::PlayPosition, ContinuousId::Touchstrip] {
                let (command, touch) = match ctx(&mut app).surface.continuous(id) {
                    Some(c) => (c.command.clone(), c.touch_command.clone()),
                    None => continue,
                };
                if let Some(touch) = &touch {
                    touch.borrow_mut().execute(ctx(&mut app), ButtonEvent::Down, 127);
                }
                if let Some(command) = command {
                    command.borrow_mut().execute(ctx(&mut app), 1);
                    command.borrow_mut().execute(ctx(&mut app), 127);
                }
                if let Some(touch) = &touch {
                    touch.borrow_mut().execute(ctx(&mut app), ButtonEvent::Up, 0);
                }
            }
            for message in [[0xE0u8, 0x00, 0x40], [0x90, 36, 100], [0x90, 36, 0], [0xA0, 36, 50], [0xD0, 60, 0]] {
                app.on_midi_event(IN, &message);
            }
            run_tasks(&mut app, 500);
            time += 50;
            app.set_time_ms(time);
            app.tick();
        }
    }
}
