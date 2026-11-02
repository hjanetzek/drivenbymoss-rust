// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Mackie HUI protocol: the zone/port buttons, the hi-res faders, the knobs, the
//! displays, extenders (several surfaces in one instance) and a "touch everything" run.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::reaper::ControllerConfigFile;
use drivenbymoss::controller::mackie::hui::definition::*;
use drivenbymoss::controller::mackie::hui::surface::HuiSurfaceState;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, Modes};
use drivenbymoss::midi::ButtonEvent;

const MODELS: [&str; 3] = ["HUI - Human User Interface", "HUI - Human User Interface + 1 Extender", "HUI - Human User Interface + 2 Extenders"];

const TRACKS: &str = "track/count 16\n\
track/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/pan 0.5\n\
track/1/exists 1\ntrack/1/name Bass\ntrack/8/exists 1\ntrack/8/name Strings\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ndevice/exists 1\ndevice/name ReaEQ\ndevice/param/count 8\ndevice/param/0/name Gain\nplay 0\n";

fn port_in(index: usize) -> i32 {
    30 + index as i32
}

fn port_out(index: usize) -> i32 {
    40 + index as i32
}

struct Hui {
    app: App,
    backend: Rc<RecordingBackend>,
    time: Instant,
}

impl Hui {
    fn start(name: &str, num_extenders: usize, settings: &[(&str, &str)]) -> Hui {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("dbm-test-hui-{name}-{}-{count}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("DrivenByMoss4Reaper.config"),
            format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.mackie.hui.HUI{}ControllerInstance\n", num_extenders + 1),
        )
        .unwrap();
        let mut file = ControllerConfigFile::default();
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for i in 0..=num_extenders {
            let port_name = format!("HUI Port {}", i + 1);
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
        let mut hui = Hui { app, backend, time };
        hui.advance(1100);
        hui
    }

    fn advance(&mut self, ms: u64) {
        self.time += Duration::from_millis(ms);
        self.app.set_time(self.time);
        self.app.tick();
    }

    fn ctx(&mut self) -> &mut Ctx {
        self.app.instance_ctx(0).expect("running instance")
    }

    fn cc(&mut self, device: usize, control: u8, value: u8) {
        self.app.on_midi_event(port_in(device), &[0xB0, control, value]);
    }

    /// Press or release a button by its HUI control (zone and port).
    fn button(&mut self, device: usize, control: i32, is_down: bool) {
        self.cc(device, 0x0F, (control / 8) as u8);
        self.cc(device, 0x2F, (control % 8) as u8 + if is_down { 0x40 } else { 0 });
    }

    fn press(&mut self, device: usize, control: i32) {
        self.button(device, control, true);
        self.button(device, control, false);
    }

    fn calls(&self) -> Vec<BackendCall> {
        self.backend.take_calls()
    }
}

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

fn midi_to(calls: &[BackendCall], port: i32) -> Vec<Vec<u8>> {
    calls.iter().filter_map(|c| if let BackendCall::SendMidi(p, d) = c { (*p == port).then(|| d.clone()) } else { None }).collect()
}

#[test]
fn startup_lights_and_display() {
    let mut hui = Hui::start("startup", 0, &[]);
    assert!(hui.ctx().modes.is_active(Modes::Pan));
    hui.calls();
    hui.ctx().surface.clear_cache();
    hui.ctx().text_display.as_mut().unwrap().force_flush();
    hui.advance(30);
    let out = midi_to(&hui.calls(), port_out(0));
    // The stop LED is on: zone 14, port 3
    let stop = out.windows(2).any(|w| w[0] == vec![0xB0, 0x0C, 14] && w[1] == vec![0xB0, 0x2C, 0x43]);
    assert!(stop, "{out:?}");
    // The display shows the track names and the mode
    assert!(out.contains(&vec![0xF0, 0x00, 0x00, 0x66, 0x05, 0x00, 0x10, 0, b'D', b'r', b'm', b's', 0xF7]), "{out:?}");
    assert!(out.contains(&vec![0xF0, 0x00, 0x00, 0x66, 0x05, 0x00, 0x10, 8, b'P', b'a', b'n', b's', 0xF7]));
    // A ping is sent every second
    hui.advance(1000);
    let out = midi_to(&hui.calls(), port_out(0));
    assert!(out.contains(&vec![0x90, 0, 0]), "{out:?}");
}

#[test]
fn buttons_faders_and_knobs() {
    let mut hui = Hui::start("buttons", 0, &[]);
    hui.calls();
    hui.press(0, HUI_TRANSPORT_PLAY);
    assert!(has_command(&hui.calls(), "play "));

    // A fader with the high and low byte
    hui.cc(0, 0x01, 0x40);
    hui.cc(0, 0x21, 0x00);
    let calls = hui.calls();
    assert!(has_command(&calls, "track 1/volume"), "{:?}", commands(&calls));

    // A knob in the pan mode
    hui.cc(0, 0x40, 0x41);
    let calls = hui.calls();
    assert!(has_command(&calls, "track 0/pan"), "{:?}", commands(&calls));

    // Mode selection
    hui.press(0, HUI_ASSIGN2_BYPASS);
    assert!(hui.ctx().modes.is_active(Modes::DeviceParams));
    hui.press(0, HUI_ASSIGN1_SEND_A);
    assert!(hui.ctx().modes.is_active(Modes::Send1));
    let name = hui.ctx().active_mode().unwrap().borrow().name().to_string();
    assert_eq!(name, "Send A: FX1");

    // Touching a fader activates the volume mode
    hui.button(0, HUI_FADER1, true);
    assert!(hui.ctx().modes.is_active(Modes::Volume));
    hui.button(0, HUI_FADER1, false);
    assert!(hui.ctx().modes.is_active(Modes::Send1));

    // An unknown button is reported
    hui.button(0, 255, true);
}

#[test]
fn extenders_are_separate_surfaces() {
    let mut hui = Hui::start("extenders", 1, &[]);
    let ctx = hui.ctx();
    assert_eq!(ctx.surface_count(), 2);
    assert_eq!(ctx.with_surface(1, |ctx| ctx.ext::<HuiSurfaceState>().extender_offset), 8);
    assert!(ctx.with_surface(1, |ctx| ctx.modes.is_active(Modes::Pan)));

    // The faders of the extender (second device) control tracks 9-16
    hui.calls();
    hui.cc(1, 0x00, 0x40);
    hui.cc(1, 0x20, 0x00);
    let calls = hui.calls();
    assert!(has_command(&calls, "track 8/volume"), "{:?}", commands(&calls));

    // A mode selected on one device is active on all
    hui.press(1, HUI_ASSIGN1_INPUT);
    assert!(hui.ctx().with_surface(0, |ctx| ctx.modes.is_active(Modes::Track)));
    assert!(hui.ctx().with_surface(1, |ctx| ctx.modes.is_active(Modes::Track)));

    // Shift of the first device counts on the second: Mute toggles the monitor
    hui.calls();
    hui.button(0, HUI_KEY_SHIFT_AD, true);
    hui.press(1, HUI_MUTE1);
    let shifted = commands(&hui.calls());
    hui.button(0, HUI_KEY_SHIFT_AD, false);
    hui.press(1, HUI_MUTE1);
    let normal = commands(&hui.calls());
    assert!(normal.iter().any(|c| c.starts_with("track 8/mute")), "{normal:?}");
    assert!(!shifted.iter().any(|c| c.starts_with("track 8/mute")), "{shifted:?}");

    // The VU meters and the motor faders of each device go to its own port
    hui.app.update_model("track/8/vuleft 0.5\ntrack/8/vuright 0.4\ntrack/8/volume 0.25\n");
    hui.advance(30);
    let calls = hui.calls();
    let second = midi_to(&calls, port_out(1));
    assert!(second.iter().any(|m| m[0] == 0xA0), "{second:?}");
    assert!(second.iter().any(|m| m[0] == 0xB0 && m[1] == 0x20), "{second:?}");
}

#[test]
fn profile() {
    let mut hui = Hui::start("profile", 0, &[("HARDWARE_SETUP_PROFILE", "Novation MkIII")]);
    use drivenbymoss::config::hui::HuiConfig;
    let ctx = hui.ctx();
    assert!(!ctx.config.has_motor_faders());
    assert!(!ctx.config.is_enable_vu_meters());
    assert_eq!(ctx.config.value(drivenbymoss::config::hui::PROFILE).as_deref(), Some("<Select a profile>"));
}

/// Activate every mode on every surface and trigger every control of all models.
#[test]
fn touch_everything() {
    let mut modes = vec![Modes::Track, Modes::Volume, Modes::Pan, Modes::DeviceParams, Modes::EqDeviceParams];
    modes.extend((0..5).map(|i| Modes::get(Modes::Send1, i)));
    for extenders in 0..3 {
        let mut hui = Hui::start(&format!("touch-{extenders}"), extenders, &[("WORKFLOW_ZOOM", if extenders == 1 { "On" } else { "Off" })]);
        let surfaces = hui.ctx().surface_count();
        for mode in &modes {
            for surface in 0..surfaces {
                hui.ctx().with_surface(surface, |ctx| ctx.set_active_mode(*mode));
                hui.advance(30);
                let buttons = hui.ctx().with_surface(surface, |ctx| ctx.surface.button_ids());
                for id in buttons {
                    for (event, value) in [(ButtonEvent::Down, 1.0), (ButtonEvent::Long, 1.0), (ButtonEvent::Up, 0.0)] {
                        hui.ctx().with_surface(surface, |ctx| ctx.trigger_button(id, event, value));
                    }
                    hui.advance(5);
                }
                for i in 0..9u8 {
                    hui.cc(surface, i, 0x30);
                    hui.cc(surface, 0x20 + i, 0x10);
                }
                for i in 0..8u8 {
                    hui.cc(surface, 0x40 + i, 0x41);
                    hui.cc(surface, 0x40 + i, 0x01);
                }
                hui.cc(surface, 0x0D, 0x41);
                hui.cc(surface, 0x0D, 0x01);
                hui.advance(30);
            }
        }
        assert!(hui.ctx().surface.button(ButtonId::Play).is_some());
        hui.ctx().config.mark_all_changed();
        hui.advance(30);
        hui.app.shutdown();
    }
}
