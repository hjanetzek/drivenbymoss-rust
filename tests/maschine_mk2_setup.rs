// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Maschine Mk2 controller setup: buttons, knobs, encoder, touch strip, views,
//! modes, LEDs and the MCU display.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::maschine::MaschineConfiguration;
use drivenbymoss::daw::clip::NotePosition;
use drivenbymoss::controller::maschine::core::ribbon_mode::RibbonMode;
use drivenbymoss::controller::maschine::mk::setup::{HOST_QUERY, HOST_SENDS_SHIFT, HOST_SHIFT_HEADER};
use drivenbymoss::controller::maschine::mk::surface as mk;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use drivenbymoss::midi::ButtonEvent;

const IN: i32 = 3;
const OUT: i32 = 5;

/// The header of the first line of the MCU display.
const DISPLAY_LINE_1: [u8; 7] = [0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x00];
/// The header of the second line of the MCU display.
const DISPLAY_LINE_2: [u8; 7] = [0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x38];

/// Start a Maschine Mk2 instance with 2 tracks (the first one selected) and wait for the delayed
/// startup.
fn start() -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-mk2-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.ni.maschine.mk3.MaschineMk2ControllerInstance\n").unwrap();

    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, "Maschine MK2 In".into())], outputs: vec![(OUT, "Maschine MK2 Out".into())], ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    app.start_infrastructure();
    app.update_model(
        "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volume/str -6.0dB\n\
         track/1/exists 1\ntrack/1/name Bass\nplay 0\n",
    );
    assert!(app.instance_ctx(0).is_some(), "instance is running");
    // Startup is delayed by 1s
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    (app, backend)
}

fn cc(app: &mut App, control: i32, value: u8) {
    app.on_midi_event(IN, &[0xB0, control as u8, value]);
}

fn press(app: &mut App, control: i32) {
    cc(app, control, 127);
    cc(app, control, 0);
}

fn pad(app: &mut App, note: u8, velocity: u8) {
    app.on_midi_event(IN, &[0x90, note, velocity]);
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).unwrap()
}

/// The last value sent to a CC.
fn led(calls: &[BackendCall], control: i32) -> Option<u8> {
    calls.iter().rev().find_map(|c| match c {
        BackendCall::SendMidi(OUT, d) if d.len() == 3 && d[0] == 0xB0 && d[1] == control as u8 => Some(d[2]),
        _ => None,
    })
}

/// The texts of the lines sent to the display (row index, text).
fn display_lines(calls: &[BackendCall]) -> Vec<(usize, String)> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(OUT, d) if d.len() > 8 && d[..7] == DISPLAY_LINE_1 => Some((0, String::from_utf8_lossy(&d[7..d.len() - 1]).to_string())),
            BackendCall::SendMidi(OUT, d) if d.len() > 8 && d[..7] == DISPLAY_LINE_2 => Some((1, String::from_utf8_lossy(&d[7..d.len() - 1]).to_string())),
            _ => None,
        })
        .collect()
}

fn contains(calls: &[BackendCall], text: &str) -> bool {
    calls.iter().any(|c| format!("{c:?}").contains(text))
}

#[test]
fn startup_shows_the_volume_mode_on_the_display() {
    let (mut app, backend) = start();
    let calls = backend.take_calls();
    let ctx = ctx(&mut app);
    assert_eq!(ctx.views.active_id(), Some(Views::Play));
    assert!(ctx.modes.is_active(Modes::Volume));
    assert_eq!(ctx.surface.button(ButtonId::Play).unwrap().binding.map(|b| b.2), Some(mk::PLAY));

    let lines = display_lines(&calls);
    let first = lines.iter().rev().find(|(row, _)| *row == 0).expect("first line was sent");
    assert_eq!(first.1.len(), 56);
    assert!(first.1.starts_with(">Drums Bass   "), "{}", first.1);
    let second = lines.iter().rev().find(|(row, _)| *row == 1).expect("second line was sent");
    assert!(second.1.starts_with("-6.0dB "), "{}", second.1);
}

#[test]
fn notifications_are_shown_on_the_display() {
    let (mut app, backend) = start();
    backend.take_calls();
    // Fixed velocity on/off
    press(&mut app, mk::FIXED_VEL);
    let lines = display_lines(&backend.take_calls());
    assert!(lines.iter().any(|(row, text)| *row == 0 && text.starts_with("Fixed Velocity: On")), "{lines:?}");
    assert!(ctx(&mut app).config.is_accent_active());
    // Not shown by the host
    assert!(ctx(&mut app).host.take_notifications().is_empty());

    // The notification is removed after 700ms
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(30));
        app.tick();
    }
    let lines = display_lines(&backend.take_calls());
    assert!(lines.iter().any(|(row, text)| *row == 0 && text.starts_with(">Drums")), "{lines:?}");
}

/// The notification ends after 700ms, also if the ticks come in a coarser interval than the
/// checks (each check only runs with a tick, counting the checks made it last 900ms and more).
#[test]
fn notifications_end_after_their_time() {
    let (mut app, backend) = start();
    let base = std::time::Instant::now();
    app.set_time(base);
    backend.take_calls();
    press(&mut app, mk::FIXED_VEL);
    backend.take_calls();
    // Ticks every 42ms (as in Bitwig) for 750ms
    let mut t = 0;
    while t < 750 {
        t += 42;
        app.set_time(base + Duration::from_millis(t));
        app.tick();
    }
    let lines = display_lines(&backend.take_calls());
    assert!(lines.iter().any(|(row, text)| *row == 0 && text.starts_with(">Drums")), "{lines:?}");
}

/// The labels of the buttons above the display (for dbm-nihost, not in the Java version).
#[test]
fn button_labels_follow_the_mode() {
    let labels = |calls: &[BackendCall]| {
        calls.iter().rev().find_map(|c| match c {
            BackendCall::SendMidi(OUT, d) if d.starts_with(&[0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x10, 0x00]) => Some(String::from_utf8_lossy(&d[7..d.len() - 1]).to_string()),
            _ => None,
        })
    };
    let (mut app, backend) = start();
    app.tick();
    let calls = backend.take_calls();
    let text = labels(&calls).expect("labels after the startup");
    assert_eq!(text.len(), 56);
    assert!(text.starts_with(">Volume Pan    "), "{text}");
    assert!(text.contains(" PinTrk"), "{text}");
    // Parameters mode: Pin and Active act on the device
    press(&mut app, mk::MODE_BUTTON_8);
    app.tick();
    let text = labels(&backend.take_calls()).expect("labels after the mode change");
    assert!(text.contains(" PinDev DevOn  User  >Params"), "{text}");
    // Only sent when they change
    app.tick();
    assert!(labels(&backend.take_calls()).is_none());
}

#[test]
fn play_and_stop_leds() {
    let (mut app, backend) = start();
    backend.take_calls();
    press(&mut app, mk::PLAY);
    assert!(contains(&backend.take_calls(), "play"));
    app.update_model("play 1\n");
    app.tick();
    let calls = backend.take_calls();
    assert_eq!(led(&calls, mk::PLAY), Some(127));
    assert_eq!(led(&calls, mk::STOP), Some(0));
}

#[test]
fn stop_emulates_shift() {
    let (mut app, backend) = start();
    // Stop down activates the shift view
    cc(&mut app, mk::STOP, 127);
    assert!(ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Shift));
    backend.take_calls();
    // The lower left pad is Undo
    pad(&mut app, 36, 127);
    let calls = backend.take_calls();
    assert!(contains(&calls, "undo"), "{calls:?}");
    // Stop up restores the view and does not stop since a function was used
    cc(&mut app, mk::STOP, 0);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    let calls = backend.take_calls();
    assert!(!contains(&calls, "stop"), "{calls:?}");

    // Stop without a function stops
    press(&mut app, mk::STOP);
    assert!(contains(&backend.take_calls(), "stop"));
}

/// The startup asks dbm-nihost whether it sends the Shift button (not in the Java version).
#[test]
fn startup_sends_the_host_query() {
    let (_app, backend) = start();
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d[..] == HOST_QUERY)), "{calls:?}");
}

/// Without the announcement of dbm-nihost (NI's driver), STEP LEFT stays Stop and Shift, and the
/// Shift sysex of dbm-nihost is ignored.
#[test]
fn shift_stays_emulated_without_the_announcement() {
    let (mut app, backend) = start();
    app.on_midi_event(IN, &shift_sysex(true));
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    app.on_midi_event(IN, &shift_sysex(false));

    cc(&mut app, mk::STOP, 127);
    assert!(ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Shift));
    backend.take_calls();
    cc(&mut app, mk::STOP, 0);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    assert!(contains(&backend.take_calls(), "stop"));
}

/// After the announcement of dbm-nihost, its Shift sysex is Shift and STEP LEFT only stops, as on
/// a Mk3 (not in the Java version).
#[test]
fn shift_from_dbm_nihost() {
    let (mut app, backend) = start();
    app.on_midi_event(IN, &HOST_SENDS_SHIFT);

    // Shift down activates the shift view, the lower left pad is Undo
    app.on_midi_event(IN, &shift_sysex(true));
    assert!(ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Shift));
    backend.take_calls();
    pad(&mut app, 36, 127);
    assert!(contains(&backend.take_calls(), "undo"));
    app.on_midi_event(IN, &shift_sysex(false));
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    backend.take_calls();

    // STEP LEFT is Stop: no Shift view, stops on release
    cc(&mut app, mk::STOP, 127);
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    assert!(!contains(&backend.take_calls(), "stop"));
    cc(&mut app, mk::STOP, 0);
    assert!(contains(&backend.take_calls(), "stop"));

    // The reply to the query (the same message) changes nothing
    app.on_midi_event(IN, &HOST_SENDS_SHIFT);
    cc(&mut app, mk::STOP, 127);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    cc(&mut app, mk::STOP, 0);
}

/// The Shift button of dbm-nihost.
fn shift_sysex(down: bool) -> Vec<u8> {
    let mut data = HOST_SHIFT_HEADER.to_vec();
    data.extend([if down { 0x01 } else { 0x00 }, 0xF7]);
    data
}

#[test]
fn knobs_and_encoder_change_the_volume() {
    let (mut app, backend) = start();
    backend.take_calls();
    // The first mode knob is bound to the volume of the first track
    cc(&mut app, mk::MODE_KNOB_1, 1);
    let calls = backend.take_calls();
    assert!(contains(&calls, "volume"), "{calls:?}");
    // The encoder changes the volume of the selected track
    cc(&mut app, mk::ENCODER, 127);
    let calls = backend.take_calls();
    assert!(contains(&calls, "volume"), "{calls:?}");
}

#[test]
fn display_buttons_select_modes() {
    let (mut app, backend) = start();
    press(&mut app, mk::MODE_BUTTON_2);
    assert!(ctx(&mut app).modes.is_active(Modes::Pan));
    press(&mut app, mk::MODE_BUTTON_8);
    assert!(ctx(&mut app).modes.is_active(Modes::DeviceParams));
    // The 7th mode button is a note
    backend.take_calls();
    app.on_midi_event(IN, &[0x90, mk::MODE_BUTTON_7 as u8, 127]);
    app.on_midi_event(IN, &[0x80, mk::MODE_BUTTON_7 as u8, 0]);
    assert!(ctx(&mut app).modes.is_active(Modes::User));
    app.tick();
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d == &vec![0x90, mk::MODE_BUTTON_7 as u8, 127])), "{calls:?}");
    // The volume button selects the next track mode
    press(&mut app, mk::VOLUME);
    assert!(ctx(&mut app).modes.is_active(Modes::Volume));
}

#[test]
fn ribbon_modes_and_touch_strip() {
    let (mut app, backend) = start();
    // The pitch button cycles the pitch modes
    press(&mut app, mk::PITCH);
    assert_eq!(MaschineConfiguration::ribbon_mode(&ctx(&mut app).config), RibbonMode::PitchDownUp);
    press(&mut app, mk::MOD);
    assert_eq!(MaschineConfiguration::ribbon_mode(&ctx(&mut app).config), RibbonMode::Cc1);
    backend.take_calls();
    cc(&mut app, mk::TOUCHSTRIP, 100);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(IN, 0xB0, 1, 100))), "{calls:?}");
    app.tick();
    assert_eq!(led(&backend.take_calls(), mk::TOUCHSTRIP), Some(100));

    // Coming from another ribbon mode, the last selected pitch mode is activated
    press(&mut app, mk::PITCH);
    assert_eq!(MaschineConfiguration::ribbon_mode(&ctx(&mut app).config), RibbonMode::PitchDownUp);
    press(&mut app, mk::PITCH);
    assert_eq!(MaschineConfiguration::ribbon_mode(&ctx(&mut app).config), RibbonMode::PitchUp);
    press(&mut app, mk::PITCH);
    assert_eq!(MaschineConfiguration::ribbon_mode(&ctx(&mut app).config), RibbonMode::PitchDown);
    backend.take_calls();
    cc(&mut app, mk::TOUCHSTRIP_TOUCH, 127);
    cc(&mut app, mk::TOUCHSTRIP, 20);
    cc(&mut app, mk::TOUCHSTRIP_TOUCH, 0);
    let calls = backend.take_calls();
    // Pitch down: (127 - 20) / 2 = 53, release resets to the center
    assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(IN, 0xE0, 0, 53))), "{calls:?}");
    assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(IN, 0xE0, 0, 64))), "{calls:?}");
}

#[test]
fn group_buttons_select_tracks() {
    let (mut app, backend) = start();
    backend.take_calls();
    cc(&mut app, mk::BANK_1 + 1, 127);
    cc(&mut app, mk::BANK_1 + 1, 0);
    let calls = backend.take_calls();
    assert!(contains(&calls, "select"), "{calls:?}");
    // The selected track is lit white
    app.update_model("track/0/color 217 46 36\n");
    app.tick();
    assert_eq!(led(&backend.take_calls(), mk::BANK_1), Some(6));
}

#[test]
fn pad_mode_switches_between_drum_play_and_chord() {
    let (mut app, _backend) = start();
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    press(&mut app, mk::PAD_MODE);
    // Play -> Chord
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    press(&mut app, mk::PAD_MODE);
    // Chord -> Drum
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Drum));
    press(&mut app, mk::PAD_MODE);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    // Pattern selects the clip view
    press(&mut app, mk::PATTERN);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Session));
    press(&mut app, mk::SCENE);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::ScenePlay));
    press(&mut app, mk::VARIATION);
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Device));
}

#[test]
fn events_and_tempo_modes() {
    let (mut app, _backend) = start();
    press(&mut app, mk::EVENTS);
    assert!(ctx(&mut app).modes.is_active(Modes::Note));
    press(&mut app, mk::EVENTS);
    assert!(!ctx(&mut app).modes.is_active(Modes::Note));
    press(&mut app, mk::TEMPO);
    assert!(ctx(&mut app).modes.is_active(Modes::Tempo));
    press(&mut app, mk::SWING);
    assert!(ctx(&mut app).modes.is_active(Modes::Position));
    press(&mut app, mk::SWING);
    assert!(ctx(&mut app).modes.is_active(Modes::LoopStart));
}

#[test]
fn step_and_grid_buttons_toggle_the_sequencer() {
    let (mut app, backend) = start();
    press(&mut app, mk::STEP);
    app.tick();
    press(&mut app, mk::FOLLOW);
    app.tick();
    let calls = backend.take_calls();
    assert_eq!(led(&calls, mk::STEP), Some(127));
    assert_eq!(led(&calls, mk::FOLLOW), Some(127));
}

#[test]
fn all_views_modes_and_buttons() {
    let (mut app, _backend) = start();
    let views = [Views::ScenePlay, Views::Session, Views::Drum, Views::Play, Views::Device, Views::RepeatNote, Views::Shift];
    let modes = [
        Modes::Browser,
        Modes::Volume,
        Modes::Pan,
        Modes::Send1,
        Modes::Send8,
        Modes::Tempo,
        Modes::Position,
        Modes::LoopStart,
        Modes::LoopLength,
        Modes::RepeatNote,
        Modes::Scales,
        Modes::PlayOptions,
        Modes::Note,
        Modes::DeviceParams,
        Modes::User,
    ];
    for view in views {
        assert!(ctx(&mut app).views.contains(view), "{view:?}");
        for mode in modes {
            assert!(ctx(&mut app).modes.contains(mode), "{mode:?}");
            {
                let ctx = ctx(&mut app);
                ctx.set_active_view(view);
                ctx.set_active_mode(mode);
            }
            app.tick();
            // The knobs and the encoder
            for control in [mk::ENCODER, mk::MODE_KNOB_1, mk::MODE_KNOB_1 + 7] {
                cc(&mut app, control, 1);
                cc(&mut app, control, 127);
            }
            cc(&mut app, mk::ENCODER_TOUCH, 127);
            cc(&mut app, mk::ENCODER_TOUCH, 0);
            cc(&mut app, mk::MODE_KNOB_TOUCH_1, 127);
            cc(&mut app, mk::MODE_KNOB_TOUCH_1, 0);
            for note in [36, 41, 47, 51] {
                pad(&mut app, note, 100);
                pad(&mut app, note, 0);
            }
            app.tick();
        }
    }

    // Trigger every button once in every view
    for view in views {
        let ids = ctx(&mut app).surface.button_ids();
        for id in ids {
            {
                let ctx = ctx(&mut app);
                if !ctx.views.is_active(view) {
                    ctx.set_active_view(view);
                }
                ctx.trigger_button(id, ButtonEvent::Down, 1.0);
                ctx.trigger_button(id, ButtonEvent::Long, 1.0);
                ctx.trigger_button(id, ButtonEvent::Up, 0.0);
            }
            app.tick();
        }
    }

    // The touch strip in all ribbon modes
    for ribbon_mode in RibbonMode::ALL {
        MaschineConfiguration::set_ribbon_mode(&mut ctx(&mut app).config, ribbon_mode);
        cc(&mut app, mk::TOUCHSTRIP_TOUCH, 127);
        cc(&mut app, mk::TOUCHSTRIP, 64);
        cc(&mut app, mk::TOUCHSTRIP_TOUCH, 0);
        app.tick();
    }
    assert!(ctx(&mut app).surface.continuous(ContinuousId::Crossfader).is_some());
    app.shutdown();
}

#[test]
fn pad_leds_and_blinking() {
    let (mut app, backend) = start();
    // The play view lights the pads (the octave note in the track color (dark grey), notes white)
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d == &vec![0x90, 36, 76])), "{calls:?}");
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d == &vec![0x90, 37, 78])), "{calls:?}");

    // The select view of the parameters blinks the selected parameter
    app.update_model("device/exists 1\ndevice/name ReaEQ\ndevice/param/count 8\ndevice/param/0/name Gain\n");
    ctx(&mut app).set_active_view(Views::Device);
    app.tick();
    let calls = backend.take_calls();
    // The selected parameter in red (blinking white)
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d == &vec![0x90, 40, 6])), "{calls:?}");
    std::thread::sleep(Duration::from_millis(650));
    app.tick();
    let calls = backend.take_calls();
    // The pad of the selected parameter (note 40) toggles between red and white
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d[0] == 0x90 && d[1] == 40 && (d[2] == 6 || d[2] == 78))), "{calls:?}");
}

#[test]
fn edit_note_mode_shows_the_note() {
    let (mut app, backend) = start();
    // No note selected
    press(&mut app, mk::EVENTS);
    app.tick();
    let lines = display_lines(&backend.take_calls());
    assert!(lines.iter().any(|(row, text)| *row == 0 && text.starts_with("Please select a note...")), "{lines:?}");

    // The parameters mode without a device
    press(&mut app, mk::MODE_BUTTON_8);
    app.tick();
    let lines = display_lines(&backend.take_calls());
    assert!(lines.iter().any(|(row, text)| *row == 0 && text.starts_with("Please select a device...")), "{lines:?}");
}

/// The clip of the drum and play views (16 steps, 128 rows).
const CLIP_KEY: (usize, usize) = (16, 128);

/// Activate the edit note mode and edit the note at step 3 of row 36. Waits until the
/// notification of the mode has ended.
fn edit_note(app: &mut App) -> NotePosition {
    let base = std::time::Instant::now();
    app.set_time(base);
    press(app, mk::EVENTS);
    app.set_time(base + Duration::from_millis(800));
    app.tick();
    let position = NotePosition::new(0, 2, 36);
    let mode = ctx(app).modes.get(Modes::Note).unwrap();
    mode.borrow_mut().note_editor_mut().unwrap().set_note(CLIP_KEY, &position);
    position
}

/// Press the button while STEP LEFT (Shift on the Mk2, sends the STOP CC) is held.
fn shifted_press(app: &mut App, control: i32) {
    cc(app, mk::STOP, 127);
    press(app, control);
    cc(app, mk::STOP, 0);
}

/// Has the edit note mode a next / previous page?
fn note_mode_pages(app: &mut App) -> (bool, bool) {
    let ctx = ctx(app);
    let mode = ctx.modes.get(Modes::Note).unwrap();
    let mode = mode.borrow();
    (mode.has_previous_item_page(ctx), mode.has_next_item_page(ctx))
}

/// The texts of the first display line after the next tick.
fn first_display_line(app: &mut App, backend: &RecordingBackend) -> String {
    app.tick();
    let lines = display_lines(&backend.take_calls());
    lines.iter().rev().find(|(row, _)| *row == 0).map(|(_, text)| text.clone()).unwrap_or_default()
}

#[test]
fn edit_note_mode_has_3_pages_with_the_host_grid() {
    let (mut app, backend) = start();
    ctx(&mut app).model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
    // A note of the step grid of the host (Bitwig) with a gain of 0.5
    app.update_model("clip/exists 1\nclip/grid/16x128/step/0/2/36 1 0 0 0.25 0.8 0 0 0 0 0 0 0.5 1 0.5 0 0 0 1 0 0 0 0 0 0\n");
    let position = edit_note(&mut app);
    backend.take_calls();

    // EditNoteMode.updateDisplay: all attributes of the 'Note' page (the cells have 6
    // characters)
    let line = first_display_line(&mut app, &backend);
    for label in ["Lengt", "Veloci", "Spread", "R-Vel", "Chance", "Occurn"] {
        assert!(line.contains(label), "{label}: {line}");
    }
    assert_eq!(note_mode_pages(&mut app), (false, true));

    // The Mk2 has no encoder up/down: STEP LEFT (Shift, the STOP CC in MIDI mode) + the right
    // arrow below the encoder selects the next page (MaschineCursorCommand.scrollRight ->
    // scrollUp -> selectNextItemPage)
    shifted_press(&mut app, mk::CURSOR_RIGHT);
    let line = first_display_line(&mut app, &backend);
    for label in ["Gain", "Pan", "Pitch", "Timbre", "Pressu"] {
        assert!(line.contains(label), "{label}: {line}");
    }
    assert_eq!(note_mode_pages(&mut app), (true, true));

    shifted_press(&mut app, mk::CURSOR_RIGHT);
    let line = first_display_line(&mut app, &backend);
    for label in ["Veloci", "Count", "Curve", "V-Curv", "V-End"] {
        assert!(line.contains(label), "{label}: {line}");
    }
    assert_eq!(note_mode_pages(&mut app), (true, false));
    // No further page
    shifted_press(&mut app, mk::CURSOR_RIGHT);
    assert_eq!(note_mode_pages(&mut app), (true, false));

    // STEP LEFT + the left arrow selects the previous page
    shifted_press(&mut app, mk::CURSOR_LEFT);
    assert_eq!(note_mode_pages(&mut app), (true, true));
    // STEP LEFT is consumed by the page selection, its release does not stop (the Java version
    // stops, UPSTREAM_CANDIDATES.md L)
    assert!(!contains(&backend.take_calls(), "stop"));
    // The encoder up/down CCs (registered for the Mk2 as in the Java version) select the pages
    // as well
    press(&mut app, mk::CURSOR_UP);
    assert_eq!(note_mode_pages(&mut app), (true, false));
    press(&mut app, mk::CURSOR_DOWN);
    assert_eq!(note_mode_pages(&mut app), (true, true));

    // The 4th knob changes the gain on the 'Expressions' page
    backend.take_calls();
    cc(&mut app, mk::MODE_KNOB_1 + 3, 1);
    let calls = backend.take_calls();
    let gain = calls.iter().find_map(|c| match c {
        BackendCall::DoubleArg(p, c, v) if p == "clip" && c == "grid/16x128/step/0/2/36/gain" => Some(*v),
        _ => None,
    });
    assert!(gain.is_some_and(|v| v > 0.5 && v < 0.6), "{calls:?}");
    let gain = ctx(&mut app).model.note_clip(CLIP_KEY.0, CLIP_KEY.1).step(&position).gain();
    assert!(gain > 0.5, "{gain}");
}

#[test]
fn edit_note_mode_has_only_the_note_page_on_reaper() {
    let (mut app, backend) = start();
    ctx(&mut app).model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
    app.update_model("clip/exists 1\n");
    edit_note(&mut app);
    backend.take_calls();

    // Only length and velocity are supported by Reaper
    let line = first_display_line(&mut app, &backend);
    assert!(line.contains("Lengt") && line.contains("Veloci"), "{line}");
    for label in ["Spread", "R-Vel", "Chance", "Occurn"] {
        assert!(!line.contains(label), "{label}: {line}");
    }
    assert_eq!(note_mode_pages(&mut app), (false, false));

    // No page switching (the display does not change)
    shifted_press(&mut app, mk::CURSOR_RIGHT);
    let line = first_display_line(&mut app, &backend);
    assert!(!line.contains("Gain"), "{line}");
    assert_eq!(note_mode_pages(&mut app), (false, false));
}

#[test]
fn shutdown_clears_the_display() {
    let (mut app, backend) = start();
    backend.take_calls();
    app.shutdown();
    let calls = backend.take_calls();
    let lines = display_lines(&calls);
    assert!(lines.iter().any(|(row, text)| *row == 0 && text.trim().is_empty()), "{lines:?}");
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d == &vec![0x90, 36, 0])), "{calls:?}");
}
