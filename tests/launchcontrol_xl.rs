// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Novation Launch Control XL: templates, modes, knob/fader bindings and LEDs.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::controller::launchcontrol::surface::{self as xl, TEMPLATE_CHANGE_HEADER};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, ContinuousId, Modes};
use drivenbymoss::midi::BindType;

const IN: i32 = 3;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/pan 0.5\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.25\nplay 0\n";

/// Start a Launch Control XL instance and wait for the delayed startup. The template is not yet
/// selected.
fn start_without_template(name: &str) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-lcxl-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("DrivenByMoss4Reaper.config"),
        "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchcontrol.LaunchControlXLControllerInstance\n",
    )
    .unwrap();

    let backend = Rc::new(RecordingBackend {
        inputs: vec![(IN, "Launch Control XL".into())],
        outputs: vec![(OUT, "Launch Control XL".into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    app.start_infrastructure();
    app.update_model(TRACKS);
    // Startup is delayed by 1s
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    (app, backend)
}

/// The device answers the template selection.
fn select_template(app: &mut App, template: u8) {
    let mut data = TEMPLATE_CHANGE_HEADER.to_vec();
    data.push(template);
    data.push(0xF7);
    app.on_midi_event(IN, &data);
}

/// Start the instance with the given factory template.
fn start(name: &str, template: u8) -> (App, Rc<RecordingBackend>) {
    let (mut app, backend) = start_without_template(name);
    select_template(&mut app, template);
    app.tick();
    backend.take_calls();
    (app, backend)
}

fn track_mode_active(app: &mut App, mode: Modes) -> bool {
    let ctx = ctx(app);
    ctx.mode_manager(xl::track_modes(ctx)).is_active(mode)
}

fn fader_mode_active(app: &mut App, mode: Modes) -> bool {
    let ctx = ctx(app);
    ctx.mode_manager(xl::fader_modes(ctx)).is_active(mode)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).unwrap()
}

fn cc(app: &mut App, channel: u8, control: u8, value: u8) {
    app.on_midi_event(IN, &[0xB0 + channel, control, value]);
}

fn note(app: &mut App, channel: u8, note: u8, velocity: u8) {
    app.on_midi_event(IN, &[0x90 + channel, note, velocity]);
}

fn press_note(app: &mut App, channel: u8, n: u8) {
    note(app, channel, n, 127);
    note(app, channel, n, 0);
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
fn startup_selects_the_mixer_template() {
    let (mut app, backend) = start_without_template("startup");
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d == &[0xF0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x77, 0x08, 0xF7])));
    {
        let ctx = ctx(&mut app);
        assert_eq!(xl::xl(ctx).template_id, -1);
        // Not yet bound
        assert_eq!(ctx.surface.button(ButtonId::Row11).unwrap().binding, None);
    }

    select_template(&mut app, 8);
    app.tick();
    let calls = backend.take_calls();
    let ctx = ctx(&mut app);
    assert_eq!(xl::xl(ctx).template_id, 8);
    assert!(ctx.modes.is_active(Modes::Send));
    assert!(ctx.mode_manager(xl::track_modes(ctx)).is_active(Modes::Mute));
    assert!(ctx.mode_manager(xl::fader_modes(ctx)).is_active(Modes::Volume));
    assert_eq!(ctx.surface.button(ButtonId::Row11).unwrap().binding, Some((BindType::Note, 8, 0x29, -1)));
    assert_eq!(ctx.surface.button(ButtonId::Row25).unwrap().binding, Some((BindType::Note, 8, 0x59, -1)));
    assert_eq!(ctx.surface.button(ButtonId::MoveBankLeft).unwrap().binding, Some((BindType::Cc, 8, 0x68, -1)));
    assert_eq!(ctx.surface.continuous(ContinuousId::Fader1).unwrap().binding, Some((BindType::Cc, 8, 0x4D)));

    // LEDs: the 1st track is selected (yellow), the mute buttons show amber low
    assert_eq!(midi_out(&calls, 0x98, 0x29), Some(62));
    assert_eq!(midi_out(&calls, 0x98, 0x2A), Some(62 - 33));
    assert_eq!(midi_out(&calls, 0x98, 0x49), Some(30));
    // The 4th track does not exist (the 3rd is the master track)
    assert_eq!(midi_out(&calls, 0x98, 0x4C), Some(12));
    // Mute mode is active
    assert_eq!(midi_out(&calls, 0x98, 0x6A), Some(127));
    // The pan knob of track 1 (value 64): green 2 and red 2 (amber)
    assert_eq!(midi_out(&calls, 0x98, 13 + 2), Some(12 + 2 + (2 << 4)));
}

#[test]
fn knobs_and_faders_change_sends_pan_and_volume() {
    let (mut app, backend) = start("mixer", 8);

    // Send A knob of track 1
    cc(&mut app, 8, 0x0D, 127);
    let calls = backend.take_calls();
    assert_eq!(number_arg(&calls, "track", "0/send/0/volume"), Some(1.0));

    // Send B knob of track 1
    cc(&mut app, 8, 0x1D, 0);
    let calls = backend.take_calls();
    assert_eq!(number_arg(&calls, "track", "0/send/1/volume"), Some(0.0));

    // Pan knob of track 2
    cc(&mut app, 8, 0x32, 127);
    let calls = backend.take_calls();
    assert_eq!(number_arg(&calls, "track", "1/pan"), Some(1.0));

    // Fader of track 1
    cc(&mut app, 8, 0x4D, 127);
    let calls = backend.take_calls();
    assert_eq!(number_arg(&calls, "track", "0/volume"), Some(1.0));

    // A different MIDI channel is not bound
    cc(&mut app, 9, 0x4D, 0);
    assert_eq!(number_arg(&backend.take_calls(), "track", "0/volume"), None);
}

#[test]
fn device_button_switches_the_3rd_knob_row_to_device_parameters() {
    let (mut app, backend) = start("device", 8);
    press_note(&mut app, 8, 0x69);
    assert!(xl::xl(ctx(&mut app)).is_device_active);

    cc(&mut app, 8, 0x31, 127);
    let calls = backend.take_calls();
    assert_eq!(number_arg(&calls, "device", "param/0/value"), Some(1.0));
    assert_eq!(number_arg(&calls, "track", "0/pan"), None);

    // The device button LED is on
    app.tick();
    assert_eq!(midi_out(&backend.take_calls(), 0x98, 0x69), Some(127));

    // Toggle back
    press_note(&mut app, 8, 0x69);
    assert!(!xl::xl(ctx(&mut app)).is_device_active);
    cc(&mut app, 8, 0x31, 0);
    assert_eq!(number_arg(&backend.take_calls(), "track", "0/pan"), Some(0.0));
}

#[test]
fn track_buttons_select_and_mute() {
    let (mut app, backend) = start("buttons", 8);

    // Row 1 selects the track on release
    press_note(&mut app, 8, 0x2A);
    assert!(has_call(&backend.take_calls(), "track", "1/select"));

    // Row 2 mutes in mute mode
    press_note(&mut app, 8, 0x49);
    assert!(has_call(&backend.take_calls(), "track", "0/mute"));

    // Solo mode
    press_note(&mut app, 8, 0x6B);
    assert!(track_mode_active(&mut app, Modes::Solo));
    press_note(&mut app, 8, 0x49);
    assert!(has_call(&backend.take_calls(), "track", "0/solo"));
}

#[test]
fn rec_arm_and_row1_execute_transport_functions() {
    let (mut app, backend) = start("transport", 8);
    note(&mut app, 8, 0x6C, 127);
    // Rec arm mode is temporarily replaced by the transport mode
    assert!(track_mode_active(&mut app, Modes::Transport));
    // Row 2, button 1: play
    press_note(&mut app, 8, 0x49);
    // Row 1, button 6: redo
    press_note(&mut app, 8, 0x3A);
    note(&mut app, 8, 0x6C, 0);
    let calls = backend.take_calls();
    assert!(has_call(&calls, "play", ""), "{calls:?}");
    assert!(has_call(&calls, "redo", ""), "{calls:?}");
    // The transport mode has been used, therefore rec arm mode is not activated
    let ctx = ctx(&mut app);
    assert!(ctx.mode_manager(xl::track_modes(ctx)).is_active(Modes::Mute));
}

#[test]
fn row1_shows_master_and_fx_volumes_on_the_faders_while_pressed() {
    let (mut app, backend) = start("master", 8);
    note(&mut app, 8, 0x29, 127);
    assert!(fader_mode_active(&mut app, Modes::Master));
    // The last fader controls the master volume
    cc(&mut app, 8, 0x54, 127);
    assert_eq!(number_arg(&backend.take_calls(), "master", "volume"), Some(1.0));
    note(&mut app, 8, 0x29, 0);
    assert!(fader_mode_active(&mut app, Modes::Volume));
}

#[test]
fn equalizer_template_controls_the_eq() {
    let (mut app, backend) = start("eq", 9);
    app.update_model("eq/exists 1\neq/param/count 24\neq/param/1/name Gain\neq/param/2/name Q\n");
    backend.take_calls();
    {
        let ctx = ctx(&mut app);
        assert!(ctx.modes.is_active(Modes::EqDeviceParams));
        assert!(ctx.mode_manager(xl::fader_modes(ctx)).is_active(Modes::EqDeviceParams));
    }
    // Fader 1 controls the gain of band 1
    cc(&mut app, 9, 0x4D, 127);
    assert_eq!(number_arg(&backend.take_calls(), "eq", "param/1/value"), Some(1.0));
    // Send B knob 1 controls Q
    cc(&mut app, 9, 0x1D, 0);
    assert_eq!(number_arg(&backend.take_calls(), "eq", "param/2/value"), Some(0.0));
    // Send A knob controls the band type (fully right is notch)
    cc(&mut app, 9, 0x0D, 127);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::StringArg(p, cmd, v) if p == "eq" && cmd == "band/0" && v == "6")), "{calls:?}");
}

#[test]
fn track_mix_mode_selects_parameter_banks_with_device_and_cursor() {
    let (mut app, backend) = start("parambanks", 8);
    // Activate device parameters
    press_note(&mut app, 8, 0x69);
    // Device + Send Next selects the track parameters
    note(&mut app, 8, 0x69, 127);
    cc(&mut app, 8, 0x69, 127);
    cc(&mut app, 8, 0x69, 0);
    note(&mut app, 8, 0x69, 0);
    // The device mode was not toggled since the device parameter page mode has been used
    assert!(xl::xl(ctx(&mut app)).is_device_active);
    backend.take_calls();
    app.update_model("track/fx/param/count 8\ntrack/fx/param/0/name P1\n");
    cc(&mut app, 8, 0x31, 127);
    let calls = backend.take_calls();
    assert_eq!(number_arg(&calls, "device", "param/0/value"), None);
    assert!(calls.iter().any(|c| matches!(c, BackendCall::DoubleArg(p, _, _) if p == "track")), "{calls:?}");
}

#[test]
fn drum_sequencer_template() {
    let (mut app, backend) = start("drum", 11);
    {
        let ctx = ctx(&mut app);
        assert!(ctx.modes.is_active(Modes::DrumSequencer));
        assert!(ctx.mode_manager(xl::track_modes(ctx)).is_active(Modes::DrumSequencer));
        assert!(ctx.mode_manager(xl::fader_modes(ctx)).is_active(Modes::DrumSequencer));
    }
    // Select the 3rd pad
    press_note(&mut app, 8 + 3, 0x2B);
    assert_eq!(xl::xl(ctx(&mut app)).selected_pad, 2);
    let drum_offset = ctx(&mut app).model.scales().drum_offset();
    assert_eq!(xl::xl(ctx(&mut app)).drum_note, drum_offset + 2);
    backend.take_calls();
    // Toggle a step
    press_note(&mut app, 11, 0x4A);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| format!("{c:?}").contains("clip")), "{calls:?}");

    // Resolution mode while Mute is pressed
    note(&mut app, 11, 0x6A, 127);
    assert!(track_mode_active(&mut app, Modes::Configuration));
    press_note(&mut app, 11, 0x4A);
    note(&mut app, 11, 0x6A, 0);
    assert_eq!(xl::xl(ctx(&mut app)).drum_resolution_index, 1);
    // As in Java, the resolution mode does not mark itself as used, therefore releasing Mute
    // toggles the layer mute mode
    assert!(track_mode_active(&mut app, Modes::DeviceLayerMute));

    // Mute toggles back to the drum sequencer
    press_note(&mut app, 11, 0x6A);
    assert!(track_mode_active(&mut app, Modes::DrumSequencer));
}

#[test]
fn user_templates_do_nothing() {
    let (mut app, backend) = start("user", 0);
    {
        let ctx = ctx(&mut app);
        assert!(ctx.modes.is_active(Modes::Dummy));
        assert!(ctx.mode_manager(xl::fader_modes(ctx)).is_active(Modes::Dummy));
    }
    // The controls are still bound to the factory template channel of the startup (none)
    cc(&mut app, 0, 0x4D, 127);
    assert_eq!(number_arg(&backend.take_calls(), "track", "0/volume"), None);
}

#[test]
fn all_templates_modes_and_buttons_work() {
    let (mut app, _backend) = start("all", 8);
    for template in 0..16u8 {
        select_template(&mut app, template);
        let channel = template.min(15);
        let buttons = xl::xl(ctx(&mut app)).button_controls.clone();
        for (_, bind_type, control) in &buttons {
            let control = *control as u8;
            match bind_type {
                BindType::Cc => {
                    cc(&mut app, channel, control, 127);
                    cc(&mut app, channel, control, 0);
                }
                _ => press_note(&mut app, channel, control),
            }
        }
        // Long press
        if (8..13).contains(&template) {
            note(&mut app, channel, 0x29, 127);
            std::thread::sleep(Duration::from_millis(510));
            app.tick();
            note(&mut app, channel, 0x29, 0);
        }
        for control in [0x0D, 0x1D, 0x31, 0x4D, 0x14, 0x24, 0x38, 0x54] {
            cc(&mut app, channel, control, 100);
        }
        app.tick();
    }

    // Activate all track button modes and fader modes directly
    let ctx = ctx(&mut app);
    let track_modes = xl::track_modes(ctx);
    let fader_modes = xl::fader_modes(ctx);
    for main in [Modes::Send, Modes::EqDeviceParams, Modes::DeviceLayer, Modes::DrumSequencer, Modes::NoteSequencer, Modes::Dummy] {
        ctx.set_active_mode(main);
        for mode in [
            Modes::Mute,
            Modes::Solo,
            Modes::RecArm,
            Modes::DeviceParams,
            Modes::InstrumentDeviceParams,
            Modes::Transport,
            Modes::Clip,
            Modes::DeviceLayerMute,
            Modes::DeviceLayerSolo,
            Modes::DrumSequencer,
            Modes::NoteSequencer,
            Modes::LoopLength,
            Modes::Configuration,
        ] {
            ctx.set_active_mode_in(track_modes, mode);
            assert!(ctx.mode_manager(track_modes).is_active(mode));
            for i in 0..8 {
                ctx.trigger_button(ButtonId::get(ButtonId::Row21, i), drivenbymoss::midi::ButtonEvent::Down, 1.0);
                ctx.trigger_button(ButtonId::get(ButtonId::Row21, i), drivenbymoss::midi::ButtonEvent::Up, 0.0);
            }
            ctx.flush_surface();
        }
        for mode in [Modes::Volume, Modes::Master, Modes::EqDeviceParams, Modes::DeviceLayer, Modes::DrumSequencer, Modes::NoteSequencer, Modes::Dummy] {
            ctx.set_active_mode_in(fader_modes, mode);
            ctx.flush_surface();
        }
    }
}


fn clip_calls(calls: &[BackendCall]) -> Vec<String> {
    calls.iter().map(|c| format!("{c:?}")).filter(|c| c.contains("clip")).collect()
}

#[test]
fn note_sequencer_template_edits_notes() {
    let (mut app, backend) = start("note", 12);
    app.update_model("track/0/type INSTRUMENT\nclip/exists 1\nclip/start 0\nclip/end 4\nclip/loop 1\nclip/notes 0:0:0.25:0.5:0:60:100\n");
    app.tick();
    backend.take_calls();
    {
        let ctx = ctx(&mut app);
        assert!(ctx.modes.is_active(Modes::NoteSequencer));
        assert!(ctx.mode_manager(xl::track_modes(ctx)).is_active(Modes::NoteSequencer));
    }

    // Fader 2 changes the velocity of the note on step 2
    cc(&mut app, 12, 0x4E, 64);
    let calls = backend.take_calls();
    assert!(!clip_calls(&calls).is_empty(), "{calls:?}");

    // Send A knob 2 changes the pitch
    cc(&mut app, 12, 0x0E, 80);
    let calls = backend.take_calls();
    assert!(!clip_calls(&calls).is_empty(), "{calls:?}");

    // The 2nd button of row 1 shows that there is a note (amber) and the 1st not (black)
    app.tick();
    backend.take_calls();
    let ctx = ctx(&mut app);
    assert_eq!(ctx.mode_button_color_in(drivenbymoss::framework::ctx::MAIN_MODE_MANAGER, ButtonId::Row12), 63);
    assert_eq!(ctx.mode_button_color_in(drivenbymoss::framework::ctx::MAIN_MODE_MANAGER, ButtonId::Row11), 12);

    // Row 1 button 2 clears the step
    press_note(&mut app, 12, 0x2A);
    let calls = backend.take_calls();
    assert!(!clip_calls(&calls).is_empty(), "{calls:?}");
}

/// Note Sequencer: while Device is held, the Track Control row shows the parameter pages of the
/// first instrument (yellow = selected) and selects them; Track Focus does not change (as
/// XLBaseNoteEditMode.handleRow2 and XLSelectDeviceParamsPageMode of the Java version).
#[test]
fn note_sequencer_device_button_selects_instrument_pages() {
    let (mut app, backend) = start("noteseq-pages", 12);
    app.update_model("track/0/type INSTRUMENT\nclip/exists 1\nclip/start 0\nclip/end 4\nclip/loop 1\nclip/notes 0:0:0.25:0.5:0:60:100\n\
primary/exists 1\nprimary/name Polysynth\nprimary/param/count 24\nprimary/page/count 3\nprimary/page/0/name A\nprimary/page/1/name B\nprimary/page/2/name C\n");
    app.tick();
    backend.take_calls();
    let rows = |app: &mut App| {
        let ctx = ctx(app);
        let track_modes = xl::track_modes(ctx);
        let row1: Vec<i32> = (0..8).map(|i| ctx.mode_button_color_in(drivenbymoss::framework::ctx::MAIN_MODE_MANAGER, ButtonId::get(ButtonId::Row11, i))).collect();
        let row2: Vec<i32> = (0..8).map(|i| ctx.mode_button_color_in(track_modes, ButtonId::get(ButtonId::Row21, i))).collect();
        (row1, row2)
    };
    let (focus, control) = rows(&mut app);

    note(&mut app, 12, 0x69, 127);
    app.tick();
    let (held_focus, held_control) = rows(&mut app);
    assert_eq!(held_focus, focus);
    assert_eq!(held_control[..3], [62, 29, 29]);
    assert!(track_mode_active(&mut app, Modes::InstrumentDeviceParams));

    press_note(&mut app, 12, 0x4A);
    app.tick();
    assert_eq!(number_arg(&backend.take_calls(), "primary", "parameterPage"), Some(1.0));
    assert_eq!(rows(&mut app).1[..3], [29, 62, 29]);

    note(&mut app, 12, 0x69, 0);
    app.tick();
    assert_eq!(rows(&mut app), (focus, control));
    assert!(track_mode_active(&mut app, Modes::NoteSequencer));
}

/// Note Sequencer, Track Select: with Device held it selects the previous/next device, with the
/// Device toggled on (3rd knob row = device parameters) it pages the clip, as
/// XLBaseNoteEditMode.selectNext/PreviousItemPage of the Java version.
#[test]
fn note_sequencer_track_select_with_device() {
    let (mut app, backend) = start("probe-tracksel", 12);
    app.update_model("track/0/type INSTRUMENT\nclip/exists 1\nclip/start 0\nclip/end 8\nclip/loop 1\nclip/loopStart 0\nclip/loopLength 8\n\
primary/exists 1\nprimary/param/count 24\nprimary/page/count 3\ndevice/count 3\ndevice/position 1\n\
device/sibling/1/name A\ndevice/sibling/1/position 0\ndevice/sibling/2/name B\ndevice/sibling/2/position 1\ndevice/sibling/2/selected 1\ndevice/sibling/3/name C\ndevice/sibling/3/position 2\n");
    app.tick();
    backend.take_calls();
    let show = |calls: Vec<BackendCall>| calls.into_iter().filter(|c| !matches!(c, BackendCall::SendMidi(..))).collect::<Vec<_>>();
    // Device held
    note(&mut app, 12, 0x69, 127);
    cc(&mut app, 12, 0x6B, 127);
    cc(&mut app, 12, 0x6B, 0);
    assert_eq!(show(backend.take_calls()), vec![BackendCall::NoArg("device".into(), "+".into())]);
    cc(&mut app, 12, 0x6A, 127);
    cc(&mut app, 12, 0x6A, 0);
    assert_eq!(show(backend.take_calls()), vec![BackendCall::NoArg("device".into(), "-".into())]);
    note(&mut app, 12, 0x69, 0);
    app.tick();
    backend.take_calls();
    // Device toggled (press and release without use)
    press_note(&mut app, 12, 0x69);
    app.tick();
    backend.take_calls();
    cc(&mut app, 12, 0x6B, 127);
    cc(&mut app, 12, 0x6B, 0);
    assert_eq!(show(backend.take_calls()), vec![BackendCall::NoArg("clip".into(), "grid/8x127/page/+".into())]);
}

/// Note Sequencer with the step grid of Bitwig: the Send B knobs set the repeats (ratchets), the
/// Pan knobs the panning of the note of the step (NoteParameter of the Java version).
#[test]
fn note_sequencer_knobs_edit_repeat_and_panning_with_host_grid() {
    let (mut app, backend) = start("noteseq-host", 12);
    app.update_model(
        "track/0/type INSTRUMENT\nclip/exists 1\nclip/start 0\nclip/end 4\nclip/loop 1\nclip/loopStart 0\nclip/loopLength 4\n\
clip/grid/8x127/step/0/1/60 1 0 0 0.25 0.8 0 0 0 0 0 0 0.5 0 0 0 0 0 1 0 0 0 0 0 0\n",
    );
    app.tick();
    backend.take_calls();
    let update = |calls: &[BackendCall]| {
        calls.iter().find_map(|c| match c {
            BackendCall::StringArg(p, cmd, v) if p == "clip" && cmd == "grid/8x127/step/0/1/60/update" => Some(v.split(' ').map(str::to_string).collect::<Vec<_>>()),
            _ => None,
        })
    };

    // Send B knob 2: repeat count, the range is limited to -15..15
    cc(&mut app, 12, 0x1E, 127);
    let values = update(&backend.take_calls()).expect("repeat update");
    assert_eq!(values[20], "15");

    // Pan knob 2: panning -1..1
    cc(&mut app, 12, 0x32, 0);
    let values = update(&backend.take_calls()).expect("pan update");
    assert_eq!(values[9], "-1");
}

/// Layer Mix: Track Focus selects a layer of the first instrument, or with a Drum Machine a pad
/// of the page which starts at C1, as Bitwig's drum pad banks (XLLayerMixMode.executeRow0).
#[test]
fn layer_mix_selects_layers_and_drum_pads() {
    let (mut app, backend) = start("layer-select", 10);
    app.update_model("primary/exists 1\nprimary/name Instrument Layer\nprimary/hasDrumPads 0\nprimary/hasLayers 1\nprimary/layer/count 2\n\
primary/layer/0/exists 1\nprimary/layer/0/name A\nprimary/layer/1/exists 1\nprimary/layer/1/name B\n");
    app.tick();
    backend.take_calls();
    press_note(&mut app, 10, 0x2A);
    app.tick();
    assert!(has_call(&backend.take_calls(), "primary", "layer/1/select"));

    // A Drum Machine: Track Focus 3 is the 3rd pad from C1
    let mut lines = String::from("primary/name Drum Machine\nprimary/hasDrumPads 1\nprimary/hasLayers 0\n");
    for pad in 36..44 {
        lines.push_str(&format!("primary/pad/{pad}/exists 1\nprimary/pad/{pad}/name Pad {pad}\n"));
    }
    app.update_model(&lines);
    app.tick();
    backend.take_calls();
    press_note(&mut app, 10, 0x2B);
    app.tick();
    assert!(has_call(&backend.take_calls(), "primary", "pad/38/select"));
}

/// Track Mix, Track Select previous: with host paging (Bitwig) the first track of the page is
/// selected, as AbstractItemBankImpl.selectPreviousPage of the Java version for Bitwig (on
/// Reaper the last one).
#[test]
fn track_select_previous_page_selects_the_first_track_with_host_paging() {
    let (mut app, backend) = start("track-page", 8);
    let mut lines = String::from("track/offset 0\ntrack/count 10\n");
    for t in 0..10 {
        lines.push_str(&format!("track/{t}/exists 1\ntrack/{t}/name T{t}\n"));
    }
    app.update_model(&lines);
    app.tick();
    backend.take_calls();
    // Next page: track 8 is the first track of page 2
    cc(&mut app, 8, 0x6B, 127);
    cc(&mut app, 8, 0x6B, 0);
    app.update_model("track/offset 8\n");
    std::thread::sleep(Duration::from_millis(100));
    app.tick();
    assert!(has_call(&backend.take_calls(), "track", "8/select"));
    // Previous page: track 0, not track 7
    cc(&mut app, 8, 0x6A, 127);
    cc(&mut app, 8, 0x6A, 0);
    app.update_model("track/offset 0\n");
    std::thread::sleep(Duration::from_millis(100));
    app.tick();
    let calls = backend.take_calls();
    assert!(has_call(&calls, "track", "0/select"), "{calls:?}");
    assert!(!has_call(&calls, "track", "7/select"), "{calls:?}");
}

/// Track Mix: a change of the device parameters (the app activates the mode again to rebind the
/// knobs) keeps the track button mode, as the Java version for Bitwig.
#[test]
fn parameter_change_keeps_the_track_button_mode() {
    let (mut app, _backend) = start("rebind-mode", 8);
    press_note(&mut app, 8, 0x6C);
    app.tick();
    assert!(track_mode_active(&mut app, Modes::RecArm));
    app.update_model("device/param/count 16\n");
    app.tick();
    assert!(track_mode_active(&mut app, Modes::RecArm));
}
