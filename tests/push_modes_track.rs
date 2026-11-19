// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Push track (mix) and device modes on the Push 1 (text display) and the Push 2/3
//! (graphics display). The modes are driven directly (`on_button`, `on_knob_value`) so that the
//! tests do not depend on the button registration.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::{RecordedCalls, RecordingBackend};
use drivenbymoss::config::push as config;
use drivenbymoss::controller::push::mode::device::params;
use drivenbymoss::controller::push::mode::track::add_track::{AddMode, AddTrackMode};
use drivenbymoss::controller::push::mode::track::clip::ClipMode;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::graphics::Component;
use drivenbymoss::framework::ids::{ButtonId, Modes};
use drivenbymoss::midi::ButtonEvent;
use drivenbymoss::protocol::Cmd;

const IN: i32 = 3;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volumeStr 0 dB\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\nscene/count 2\nscene/0/exists 1\nscene/1/exists 1\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.5\nplay 0\n";

/// The modes of this module.
fn track_and_device_modes() -> Vec<Modes> {
    let mut modes = vec![Modes::Track, Modes::TrackDetails, Modes::Volume, Modes::Pan, Modes::Crossfader, Modes::Master, Modes::MasterTemp, Modes::Clip, Modes::AddTrack];
    modes.extend((0..8).map(|i| Modes::get(Modes::Send1, i)));
    modes.extend([Modes::DeviceParams, Modes::DeviceChains, Modes::DeviceLayer, Modes::DeviceLayerVolume, Modes::DeviceLayerPan, Modes::DeviceLayerDetails, Modes::Browser, Modes::User]);
    modes.extend((0..8).map(|i| Modes::get(Modes::DeviceLayerSend1, i)));
    modes
}

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
    let dir = std::env::temp_dir().join(format!("dbm-test-push-track-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={}\n", model.class())).unwrap();
    let port = model.port();
    std::fs::write(dir.join(format!("DrivenByMoss4Reaper-{}.config", model.file())), format!("MIDI_INPUT0={port}\nMIDI_OUTPUT0={port}\n")).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, port.into())], outputs: vec![(OUT, port.into())], ..Default::default() });
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

/// Activate a mode and run the deferred tasks.
fn activate(app: &mut App, mode: Modes) {
    ctx(app).set_active_mode(mode);
    app.tick();
}

/// Run the deferred tasks (mode switches requested by a mode).
fn run_tasks(app: &mut App) {
    let ctx = ctx(app);
    let now = ctx.host.now() + Duration::from_millis(10);
    ctx.run_due_tasks(now);
}

fn button(app: &mut App, row: usize, index: usize, event: ButtonEvent) {
    let ctx = ctx(app);
    let mode = ctx.active_mode().expect("a mode is active");
    mode.borrow_mut().on_button(ctx, row, index, event);
}

fn click(app: &mut App, row: usize, index: usize) {
    button(app, row, index, ButtonEvent::Down);
    button(app, row, index, ButtonEvent::Up);
}

fn button_color(app: &mut App, button: ButtonId) -> i32 {
    ctx(app).mode_button_color(button)
}

/// The components which the active mode adds to the graphics display.
fn components(app: &mut App) -> Vec<Component> {
    let ctx = ctx(app);
    ctx.update_active_mode_display();
    let display = ctx.graphics_display.as_mut().expect("graphics display");
    let components = display.components().to_vec();
    // Clear the components as the flush does
    display.render(Instant::now());
    components
}

/// The 4 rows of the Push 1 display.
fn text_rows(app: &mut App) -> Vec<String> {
    let ctx = ctx(app);
    ctx.update_active_mode_display();
    let display = ctx.text_display.as_mut().expect("text display");
    display.flush();
    (0..4).map(|row| display.current_line(row).unwrap_or_default().to_string()).collect()
}

#[test]
fn all_modes_are_registered() {
    for model in [Model::Push1, Model::Push2, Model::Push3] {
        let (mut app, _) = start(model);
        let ctx = ctx(&mut app);
        for mode in track_and_device_modes() {
            assert!(ctx.modes.contains(mode), "{model:?}: {mode:?}");
        }
    }
}

#[test]
fn track_mode_push1_display() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::Track);
    let rows = text_rows(&mut app);
    assert!(rows[0].starts_with("Volume"), "{rows:?}");
    assert!(rows[0].contains("Pan"), "{rows:?}");
    assert!(rows[0].contains("FX1"), "{rows:?}");
    // The selected track has the select arrow
    assert!(rows[3].contains("Drums"), "{rows:?}");
    assert!(rows[3].contains("Bass"), "{rows:?}");

    activate(&mut app, Modes::Volume);
    let rows = text_rows(&mut app);
    assert!(rows[0].starts_with("Volume   Volume"), "{rows:?}");
}

#[test]
fn track_mode_push2_display() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Track);
    let track = components(&mut app);
    assert_eq!(track.len(), 8, "{track:?}");
    // The selected track is a channel, followed by its sends
    assert!(matches!(track[0], Component::Channel(_)), "{:?}", track[0]);
    assert!(matches!(track[1], Component::Sends(_)), "{:?}", track[1]);
    assert!(matches!(track[2], Component::ChannelSelect(_)), "{:?}", track[2]);

    for mode in [Modes::Volume, Modes::Pan, Modes::Crossfader] {
        activate(&mut app, mode);
        let channels = components(&mut app);
        assert_eq!(channels.len(), 8);
        assert!(channels.iter().all(|c| matches!(c, Component::Channel(_))), "{mode:?}");
    }
    activate(&mut app, Modes::Send1);
    let sends = components(&mut app);
    assert!(sends.iter().all(|c| matches!(c, Component::Sends(_))));
}

#[test]
fn second_row_switches_mix_modes() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Track);
    button(&mut app, 1, 0, ButtonEvent::Down);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Volume));
    assert_eq!(config::mixer_mode(&ctx(&mut app).config), Modes::Volume);
    assert_eq!(button_color(&mut app, ButtonId::Row21), drivenbymoss::controller::push::colors::PUSH2_COLOR2_WHITE);

    // Pressing it again returns to the track mode
    button(&mut app, 1, 0, ButtonEvent::Down);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Track));

    // The first send
    button(&mut app, 1, 4, ButtonEvent::Down);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Send1));
}

#[test]
fn first_row_selects_track_and_mutes() {
    let (mut app, backend) = start(Model::Push2);
    activate(&mut app, Modes::Track);
    backend.take_calls();
    click(&mut app, 0, 1);
    let calls = backend.take_calls();
    assert!(calls.commands().count() > 0, "{calls:?}");

    // In the mute state the second row mutes the tracks
    drivenbymoss::controller::push::surface::set_lock_state(ctx(&mut app), config::LockState::Mute);
    backend.take_calls();
    button(&mut app, 1, 1, ButtonEvent::Down);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ChannelMute { value: true, .. })), "{calls:?}");
}

#[test]
fn master_mode_saves_project() {
    let (mut app, backend) = start(Model::Push2);
    activate(&mut app, Modes::Master);
    let components = components(&mut app);
    assert_eq!(components.len(), 8);
    assert!(matches!(components[0], Component::Channel(_)));
    backend.take_calls();
    button(&mut app, 1, 7, ButtonEvent::Down);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ProjectSave {})), "{calls:?}");
}

#[test]
fn device_params_mode() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::DeviceParams);
    let rows = text_rows(&mut app);
    assert!(rows[0].starts_with("Cutoff"), "{rows:?}");
    assert!(rows[2].contains("Selected Device:"), "{rows:?}");
    assert!(rows[2].contains("ReaSynth"), "{rows:?}");

    // Banks shows the parameter pages instead of the devices
    assert!(params::is_show_devices(ctx(&mut app)));
    button(&mut app, 1, 4, ButtonEvent::Down);
    assert!(!params::is_show_devices(ctx(&mut app)));

    // Up shows the devices again
    button(&mut app, 1, 7, ButtonEvent::Down);
    assert!(params::is_show_devices(ctx(&mut app)));
}

#[test]
fn device_params_push2_display() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::DeviceParams);
    let components = components(&mut app);
    assert_eq!(components.len(), 8, "{components:?}");
    assert!(components.iter().all(|c| matches!(c, Component::Parameter(_))));
}

#[test]
fn device_layer_without_layers() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::DeviceLayer);
    let components = components(&mut app);
    assert_eq!(components.len(), 8);
    match &components[3] {
        Component::Options(o) => assert_eq!(o.header_top, "This device does not have layers."),
        c => panic!("{c:?}"),
    }
}

#[test]
fn add_track_mode() {
    let (mut app, backend) = start(Model::Push2);
    activate(&mut app, Modes::AddTrack);
    assert_eq!(components(&mut app).len(), 8);
    backend.take_calls();
    button(&mut app, 0, 0, ButtonEvent::Up);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::TrackAdd { .. })), "{calls:?}");

    // The Add Effect command sets the device mode
    let ctx = ctx(&mut app);
    let mode = ctx.modes.get(Modes::AddTrack).unwrap();
    mode.borrow_mut().as_any_mut().downcast_mut::<AddTrackMode>().unwrap().set_add_mode(AddMode::Device);
    assert_eq!(mode.borrow().as_any().downcast_ref::<AddTrackMode>().unwrap().add_mode(), AddMode::Device);
}

#[test]
fn clip_mode_without_clip() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Clip);
    let components = components(&mut app);
    assert_eq!(components.len(), 1);
    let ctx = ctx(&mut app);
    let mode = ctx.modes.get(Modes::Clip).unwrap();
    let mut mode = mode.borrow_mut();
    let clip = mode.as_any_mut().downcast_mut::<ClipMode>().unwrap();
    clip.toggle_piano_roll();
    assert!(clip.is_piano_roll());
}

#[test]
fn track_details_push1() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::TrackDetails);
    let rows = text_rows(&mut app);
    assert!(rows[0].contains("Track: Drums"), "{rows:?}");
    assert!(rows[2].starts_with("Active"), "{rows:?}");
    let channel = ctx(&mut app).config.midi_edit_channel();
    button(&mut app, 1, 7, ButtonEvent::Down);
    assert_eq!(ctx(&mut app).config.midi_edit_channel(), (channel + 1).min(15));
}

/// Activate every mode, press every button of the two rows and turn and touch every knob.
#[test]
fn touch_everything() {
    for model in [Model::Push1, Model::Push2, Model::Push3] {
        let (mut app, _) = start(model);
        for mode in track_and_device_modes() {
            for row in 0..2 {
                for index in 0..8 {
                    activate(&mut app, mode);
                    for event in [ButtonEvent::Down, ButtonEvent::Long, ButtonEvent::Up] {
                        button(&mut app, row, index, event);
                    }
                    for id in [ButtonId::get(ButtonId::Row11, index), ButtonId::get(ButtonId::Row21, index)] {
                        button_color(&mut app, id);
                    }
                    app.tick();
                }
            }
            activate(&mut app, mode);
            for index in 0..8 {
                let ctx = ctx(&mut app);
                let active = ctx.active_mode().unwrap();
                let mut active = active.borrow_mut();
                active.on_knob_touch(ctx, index, true);
                active.on_knob_value(ctx, index, 1);
                active.on_knob_value(ctx, index, 127);
                active.knob_value(ctx, index);
                active.on_knob_touch(ctx, index, false);
                active.select_previous_item(ctx);
                active.select_next_item(ctx);
                active.select_previous_item_page(ctx);
                active.select_next_item_page(ctx);
                active.has_previous_item(ctx);
                active.has_next_item_page(ctx);
                active.update_display(ctx);
            }
            app.tick();
        }
    }
}

#[test]
fn push3_encoder_modes() {
    use drivenbymoss::controller::push::mode::misc::as_push3_encoder;
    let (mut app, _) = start(Model::Push3);
    let ctx = ctx(&mut app);
    for (mode, has_encoder) in [(Modes::Clip, true), (Modes::Browser, true), (Modes::Track, false)] {
        let mode_ref = ctx.modes.get(mode).unwrap();
        let mut m = mode_ref.borrow_mut();
        let encoder = as_push3_encoder(&mut *m);
        assert_eq!(encoder.is_some(), has_encoder, "{mode:?}");
        if let Some(encoder) = encoder {
            encoder.encoder_turn(ctx, 1);
            encoder.encoder_left(ctx, ButtonEvent::Down);
            encoder.encoder_right(ctx, ButtonEvent::Down);
            encoder.encoder_press(ctx, ButtonEvent::Up);
            encoder.arrow_center(ctx, ButtonEvent::Down);
        }
    }
}
