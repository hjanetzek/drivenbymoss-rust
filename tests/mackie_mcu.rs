// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Mackie MCU protocol: main device alone and with extenders (several surfaces
//! in one instance), modes, faders, knobs, displays and a "touch everything" run of all models.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::reaper::ControllerConfigFile;
use drivenbymoss::controller::mackie::mcu::surface::McuSurfaceState;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, Modes};
use drivenbymoss::midi::ButtonEvent;

const MODELS: [&str; 4] =
    ["MCU - Control Universal", "MCU - Control Universal + 1 Extender", "MCU - Control Universal + 2 Extenders", "MCU - Control Universal + 3 Extenders"];

const TRACKS: &str = "track/count 16\n\
track/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/pan 0.5\ntrack/0/vu 0.7\n\
track/1/exists 1\ntrack/1/name Bass\n\
track/8/exists 1\ntrack/8/name Strings\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaEQ\ndevice/param/count 16\ndevice/param/0/name Gain\ndevice/param/0/value 0.25\n\
master/volume 0.5\nplay 0\ntempo 120\n";

/// The input and output port IDs of the device with the given index.
fn port_in(index: usize) -> i32 {
    10 + index as i32
}

fn port_out(index: usize) -> i32 {
    20 + index as i32
}

struct Mcu {
    app: App,
    backend: Rc<RecordingBackend>,
    time: Instant,
}

impl Mcu {
    /// Start an instance with `num_extenders` extenders. The MIDI ports are configured in the
    /// controller configuration file. Extra settings can be stored there as well.
    fn start(name: &str, num_extenders: usize, settings: &[(&str, &str)]) -> Mcu {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("dbm-test-mcu-{name}-{}-{count}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("DrivenByMoss4Reaper.config"),
            format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.mackie.mcu.MCU{}ControllerInstance\n", num_extenders + 1),
        )
        .unwrap();

        let mut file = ControllerConfigFile::default();
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for i in 0..=num_extenders {
            let port_name = format!("MCU Port {}", i + 1);
            file.set_midi_input(i, &port_name);
            file.set_midi_output(i, &port_name);
            inputs.push((port_in(i), port_name.clone()));
            outputs.push((port_out(i), port_name));
        }
        for (key, value) in settings {
            file.properties.put(key, value);
        }
        file.properties.save_file(&ControllerConfigFile::path(&dir, MODELS[num_extenders])).unwrap();

        let backend = Rc::new(RecordingBackend { inputs, outputs, ..Default::default() });
        let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
        let time = Instant::now();
        app.set_time(time);
        app.start_infrastructure();
        app.update_model(TRACKS);
        let mut mcu = Mcu { app, backend, time };
        // Startup is delayed by 1s
        mcu.advance(1100);
        mcu
    }

    fn advance(&mut self, ms: u64) {
        self.time += Duration::from_millis(ms);
        self.app.set_time(self.time);
        self.app.tick();
    }

    fn ctx(&mut self) -> &mut Ctx {
        self.app.instance_ctx(0).expect("running instance")
    }

    fn note(&mut self, device: usize, note: u8, velocity: u8) {
        self.app.on_midi_event(port_in(device), &[0x90, note, velocity]);
    }

    fn press(&mut self, device: usize, note: u8) {
        self.note(device, note, 127);
        self.note(device, note, 0);
    }

    fn calls(&self) -> Vec<BackendCall> {
        self.backend.take_calls()
    }
}

/// The commands sent to the host as "processor command" strings.
fn commands(calls: &[BackendCall]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::NoArg(p, cmd) => Some(format!("{p} {cmd}")),
            BackendCall::StringArg(p, cmd, v) => Some(format!("{p} {cmd} {v}")),
            BackendCall::IntArg(p, cmd, v) => Some(format!("{p} {cmd} {v}")),
            BackendCall::DoubleArg(p, cmd, v) => Some(format!("{p} {cmd} {v}")),
            _ => None,
        })
        .collect()
}

fn has_command(calls: &[BackendCall], prefix: &str) -> bool {
    commands(calls).iter().any(|c| c.starts_with(prefix))
}

/// The MIDI messages sent to a port.
fn midi_to(calls: &[BackendCall], port: i32) -> Vec<Vec<u8>> {
    calls.iter().filter_map(|c| if let BackendCall::SendMidi(p, d) = c { (*p == port).then(|| d.clone()) } else { None }).collect()
}

fn mode_of(ctx: &mut Ctx, surface: usize) -> Option<Modes> {
    ctx.with_surface(surface, |ctx| ctx.modes.active_id())
}

#[test]
fn main_device_starts_with_the_startup_mode() {
    let mut mcu = Mcu::start("startup", 0, &[]);
    let ctx = mcu.ctx();
    assert_eq!(ctx.surface_count(), 1);
    assert!(ctx.modes.is_active(Modes::Pan));
    assert!(ctx.ext::<McuSurfaceState>().is_main_device);

    // The assignment display shows the mode acronym (after one more flush)
    mcu.advance(30);
    let calls = mcu.calls();
    let out = midi_to(&calls, port_out(0));
    assert!(out.contains(&vec![0xB0, 0x4B, b'P' - 0x40]), "{out:?}");
    assert!(out.contains(&vec![0xB0, 0x4A, b'N' - 0x40]));
    // The first display shows the track names / values
    assert!(out.iter().any(|m| m.starts_with(&[0xF0, 0x00, 0x00, 0x66, 0x14, 0x12])));
}

#[test]
fn transport_and_mode_buttons() {
    let mut mcu = Mcu::start("transport", 0, &[]);
    mcu.calls();
    mcu.press(0, 0x5E);
    assert!(has_command(&mcu.calls(), "play "));

    // Track button: Volume, then Track mode
    mcu.press(0, 0x28);
    assert!(mcu.ctx().modes.is_active(Modes::Volume));
    mcu.press(0, 0x28);
    assert!(mcu.ctx().modes.is_active(Modes::Track));

    // Device button: device parameters
    mcu.press(0, 0x2B);
    assert!(mcu.ctx().modes.is_active(Modes::DeviceParams));
    mcu.advance(30);
    // The Scrub LED shows the device mode
    let out = midi_to(&mcu.calls(), port_out(0));
    assert!(out.contains(&vec![0x90, 0x65, 127]), "{out:?}");

    // Marker button toggles the marker mode
    mcu.press(0, 0x54);
    assert!(mcu.ctx().modes.is_active(Modes::Markers));
    mcu.press(0, 0x54);
    assert!(mcu.ctx().modes.is_active(Modes::DeviceParams));
}

#[test]
fn faders_and_knobs_control_the_tracks() {
    let mut mcu = Mcu::start("faders", 0, &[]);
    // Volume mode
    mcu.press(0, 0x28);
    assert!(mcu.ctx().modes.is_active(Modes::Volume));
    mcu.calls();
    mcu.app.on_midi_event(port_in(0), &[0xE1, 0x7F, 0x7F]);
    let calls = mcu.calls();
    assert!(has_command(&calls, "track 1/volume"), "{:?}", commands(&calls));

    // Knob 1 in pan mode (relative, signed bit 2)
    mcu.press(0, 0x2A);
    assert!(mcu.ctx().modes.is_active(Modes::Pan));
    mcu.calls();
    mcu.app.on_midi_event(port_in(0), &[0xB0, 0x10, 0x01]);
    let calls = mcu.calls();
    assert!(has_command(&calls, "track 0/pan"), "{:?}", commands(&calls));

    // The motor faders follow the volume
    mcu.advance(30);
    let out = midi_to(&mcu.calls(), port_out(0));
    assert!(out.iter().any(|m| m[0] == 0xE1), "{out:?}");
}

#[test]
fn fader_touch_activates_the_volume_mode_temporarily() {
    let mut mcu = Mcu::start("touch", 0, &[]);
    assert!(mcu.ctx().modes.is_active(Modes::Pan));
    mcu.note(0, 0x68, 127);
    assert!(mcu.ctx().modes.is_active(Modes::Volume));
    mcu.note(0, 0x68, 0);
    assert!(mcu.ctx().modes.is_active(Modes::Pan));
}

#[test]
fn extenders_are_separate_surfaces() {
    // Device 1 (left) is an extender, device 2 (right) is the main device
    let mut mcu = Mcu::start("extender", 1, &[]);
    let ctx = mcu.ctx();
    assert_eq!(ctx.surface_count(), 2);
    assert!(!ctx.with_surface(0, |ctx| ctx.ext::<McuSurfaceState>().is_main_device));
    assert!(ctx.with_surface(1, |ctx| ctx.ext::<McuSurfaceState>().is_main_device));
    assert_eq!(ctx.with_surface(1, |ctx| ctx.ext::<McuSurfaceState>().extender_offset), 8);
    // Only the main device has the transport buttons
    assert!(ctx.surface.button(ButtonId::Play).is_none());
    assert!(ctx.with_surface(1, |ctx| ctx.surface.button(ButtonId::Play).is_some()));
    assert_eq!(mode_of(ctx, 0), Some(Modes::Pan));
    assert_eq!(mode_of(ctx, 1), Some(Modes::Pan));

    // Play on the extender port does nothing, on the main port it plays
    mcu.calls();
    mcu.press(0, 0x5E);
    assert!(!has_command(&mcu.calls(), "play "));
    mcu.press(1, 0x5E);
    assert!(has_command(&mcu.calls(), "play "));

    // A mode selected on the main device is active on the extender as well
    mcu.press(1, 0x28);
    let ctx = mcu.ctx();
    assert_eq!(mode_of(ctx, 0), Some(Modes::Volume));
    assert_eq!(mode_of(ctx, 1), Some(Modes::Volume));

    // The faders of the extender control tracks 1-8, the ones of the main device 9-16
    mcu.calls();
    mcu.app.on_midi_event(port_in(0), &[0xE0, 0x00, 0x40]);
    assert!(has_command(&mcu.calls(), "track 0/volume"));
    mcu.app.on_midi_event(port_in(1), &[0xE0, 0x00, 0x40]);
    assert!(has_command(&mcu.calls(), "track 8/volume"));

    // Shift on the main device counts on the extender: Solo + Shift toggles the auto monitor
    mcu.calls();
    mcu.note(1, 0x46, 127);
    mcu.press(0, 0x08);
    let shifted = commands(&mcu.calls());
    mcu.note(1, 0x46, 0);
    mcu.press(0, 0x08);
    let normal = commands(&mcu.calls());
    assert!(normal.iter().any(|c| c.starts_with("track 0/solo")), "{normal:?}");
    assert!(!shifted.iter().any(|c| c.starts_with("track 0/solo")), "{shifted:?}");
    assert!(shifted.iter().any(|c| c.starts_with("track 0/")), "{shifted:?}");

    // The displays and LEDs of each device go to its own port
    mcu.advance(30);
    mcu.advance(30);
    let calls = mcu.calls();
    let main = midi_to(&calls, port_out(1));
    let extender = midi_to(&calls, port_out(0));
    // The mode acronym only on the main device
    assert!(main.contains(&vec![0xB0, 0x4B, b'V' - 0x40]), "{main:?}");
    assert!(!extender.iter().any(|m| m.len() == 3 && m[0] == 0xB0 && m[1] == 0x4B));
}

#[test]
fn select_buttons_of_the_extenders_select_their_tracks() {
    let mut mcu = Mcu::start("select", 2, &[]);
    assert_eq!(mcu.ctx().surface_count(), 3);
    mcu.calls();
    // Select 1 of the second device (center): track 9
    mcu.press(1, 0x18);
    let calls = mcu.calls();
    assert!(has_command(&calls, "track 8/select"), "{:?}", commands(&calls));
    // Long press of a button runs with its surface (scheduled task)
    mcu.note(2, 0x18, 127);
    mcu.advance(600);
    mcu.note(2, 0x18, 0);
    assert_eq!(mcu.ctx().surface_index(), 0);
}

#[test]
fn settings_are_loaded() {
    let mut mcu = Mcu::start(
        "settings",
        1,
        &[("WORKFLOW_STARTUP_MODE", "Volume"), ("EXTENDER_SETUP_(REQUIRES_RESTART)_MCU_DEVICE_1_-_LEFT", "Main"), ("EXTENDER_SETUP_(REQUIRES_RESTART)_MCU_DEVICE_2_-_RIGHT", "Mackie Extender")],
    );
    let ctx = mcu.ctx();
    assert!(ctx.modes.is_active(Modes::Volume));
    assert!(ctx.with_surface(0, |ctx| ctx.ext::<McuSurfaceState>().is_main_device));
    assert!(ctx.with_surface(0, |ctx| ctx.surface.button(ButtonId::Play).is_some()));
    assert!(!ctx.with_surface(1, |ctx| ctx.ext::<McuSurfaceState>().is_main_device));
    // A Mackie extender has its own display header
    mcu.advance(30);
    let extender = midi_to(&mcu.calls(), port_out(1));
    let _ = extender;
    mcu.ctx().with_surface(1, |ctx| {
        if let Some(display) = ctx.text_display.as_mut() {
            display.set_cell(0, 0, "X").all_done().force_flush();
        }
    });
    mcu.advance(30);
    let extender = midi_to(&mcu.calls(), port_out(1));
    assert!(extender.iter().any(|m| m.starts_with(&[0xF0, 0x00, 0x00, 0x66, 0x15, 0x12])), "{extender:?}");
}

#[test]
fn profile_sets_the_hardware_settings() {
    let mut mcu = Mcu::start("profile", 0, &[]);
    use drivenbymoss::config::mcu::{self as cfg, McuConfig, VuMeterStyle};
    mcu.ctx().config.set_value(cfg::PROFILE, "Behringer X-Touch");
    mcu.advance(30);
    let ctx = mcu.ctx();
    assert_eq!(ctx.config.vu_meter_style(), VuMeterStyle::Mackie);
    assert_eq!(ctx.config.value(cfg::PROFILE).as_deref(), Some("<Select a profile>"));
    // The VU meters are switched on
    mcu.advance(30);
    mcu.app.update_model("track/0/vu 0.9\n");
    mcu.advance(30);
    let out = midi_to(&mcu.calls(), port_out(0));
    assert!(out.iter().any(|m| m[0] == 0xD0), "{out:?}");
}

/// All modes of the MCU.
fn all_modes() -> Vec<Modes> {
    let mut modes = vec![Modes::Track, Modes::Volume, Modes::Pan, Modes::Master, Modes::DeviceLayer, Modes::DeviceLayerVolume, Modes::DeviceLayerPan];
    modes.extend((0..8).map(|i| Modes::get(Modes::Send1, i)));
    modes.extend((0..8).map(|i| Modes::get(Modes::DeviceLayerSend1, i)));
    modes.extend([Modes::DeviceParams, Modes::EqDeviceParams, Modes::InstrumentDeviceParams, Modes::ProjectParameters, Modes::TrackParameters, Modes::Browser, Modes::Markers]);
    modes
}

/// Move all knobs, faders and the jog wheel of a device via MIDI.
fn move_all_controls(mcu: &mut Mcu, device: usize) {
    for i in 0..8u8 {
        mcu.note(device, 0x68 + i, 127);
        mcu.app.on_midi_event(port_in(device), &[0xE0 + i, 0x10, 0x30]);
        mcu.note(device, 0x68 + i, 0);
        mcu.app.on_midi_event(port_in(device), &[0xB0, 0x10 + i, 0x41]);
        mcu.app.on_midi_event(port_in(device), &[0xB0, 0x10 + i, 0x01]);
        mcu.press(device, 0x20 + i);
    }
    mcu.note(device, 0x70, 127);
    mcu.app.on_midi_event(port_in(device), &[0xE8, 0x10, 0x30]);
    mcu.note(device, 0x70, 0);
    mcu.app.on_midi_event(port_in(device), &[0xB0, 0x3C, 0x01]);
    mcu.app.on_midi_event(port_in(device), &[0xB0, 0x3C, 0x41]);
}

/// Activate every mode on every surface and trigger every control of all models.
#[test]
fn touch_everything() {
    let settings = [
        [("HARDWARE_SETUP_VU_METER_TYPE", "Asparion"), ("HARDWARE_SETUP_DISPLAY_COLORS", "Behringer (X-Touch Firmware 1.25)"), ("HARDWARE_SETUP_MAIN_DISPLAY", "Asparion"), ("HARDWARE_SETUP_HAS_A_SECOND_DISPLAY", "Asparion")],
        [("HARDWARE_SETUP_VU_METER_TYPE", "iCON"), ("HARDWARE_SETUP_DISPLAY_COLORS", "iCON"), ("HARDWARE_SETUP_MAIN_DISPLAY", "Mackie - 7 characters"), ("HARDWARE_SETUP_HAS_A_SECOND_DISPLAY", "iCON QCon Pro-X")],
        [("HARDWARE_SETUP_VU_METER_TYPE", "Mackie"), ("HARDWARE_SETUP_DISPLAY_COLORS", "Behringer (X-Touch Firmware 1.22)"), ("HARDWARE_SETUP_USE_FADERS_LIKE_EDITING_KNOBS", "On"), ("HARDWARE_SETUP_HAS_A_SECOND_DISPLAY", "iCON V1-M")],
        [("HARDWARE_SETUP_VU_METER_TYPE", "None"), ("HARDWARE_SETUP_DISPLAY_COLORS", "Asparion"), ("HARDWARE_SETUP_HAS_ONLY_1_FADER", "On"), ("HARDWARE_SETUP_DISPLAY_TRACK_NAMES_IN_1ST_DISPLAY", "On")],
    ];
    for (extenders, settings) in settings.iter().enumerate() {
        let mut mcu = Mcu::start(&format!("touch-{extenders}"), extenders, settings);
        let surfaces = mcu.ctx().surface_count();
        assert_eq!(surfaces, extenders + 1);
        for mode in all_modes() {
            for surface in 0..surfaces {
                mcu.ctx().with_surface(surface, |ctx| ctx.set_active_mode(mode));
                mcu.advance(30);
                let buttons = mcu.ctx().with_surface(surface, |ctx| ctx.surface.button_ids());
                for id in buttons {
                    for (event, value) in [(ButtonEvent::Down, 1.0), (ButtonEvent::Long, 1.0), (ButtonEvent::Up, 0.0)] {
                        mcu.ctx().with_surface(surface, |ctx| ctx.trigger_button(id, event, value));
                    }
                    mcu.advance(5);
                }
                move_all_controls(&mut mcu, surface);
                mcu.advance(30);
            }
        }
        // All settings changed once more
        mcu.ctx().config.mark_all_changed();
        mcu.advance(30);
        mcu.app.shutdown();
    }
}
