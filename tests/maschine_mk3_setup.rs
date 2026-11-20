// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Maschine Mk3, Mikro Mk3 and Plus (the shared Maschine setup): registration and
//! discovery, the Shift of NI's driver, the buttons the Mk2 lacks (cursor keys of the 4-D encoder,
//! page buttons, foot switches), the display, the touch strip, the Mikro's emulated Shift and
//! missing knobs and group buttons, and a run through all views, modes and buttons.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordedCalls, RecordingBackend};
use drivenbymoss::config::maschine::MaschineConfiguration;
use drivenbymoss::controller::maschine::core::maschine::Maschine;
use drivenbymoss::controller::maschine::core::ribbon_mode::RibbonMode;
use drivenbymoss::controller::maschine::mk::definition::MaschineDefinition;
use drivenbymoss::controller::maschine::mk::surface as mk;
use drivenbymoss::controller::{all_factories, OperatingSystem};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::{ButtonId, Modes, Views};
use drivenbymoss::midi::{from_hex_str, ButtonEvent};
use drivenbymoss::protocol::Cmd;

const IN: i32 = 3;
const OUT: i32 = 5;

/// The header of the first line of the MCU display.
const DISPLAY_LINE_1: [u8; 7] = [0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x00];

/// The MIDI port names of a model (as NI's driver names them, see the definitions).
fn port_name(maschine: Maschine) -> &'static str {
    match maschine {
        Maschine::Mk3 => "Maschine MK3 Ctrl MIDI",
        Maschine::MikroMk3 => "Maschine Mikro MK3",
        Maschine::Plus => "Maschine Plus Virtual",
        _ => unreachable!(),
    }
}

/// Start an instance of a model with 2 tracks (the first one selected) and wait for the delayed
/// startup.
fn start(maschine: Maschine) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-mk3-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let class = MaschineDefinition::new(maschine).java_class_name();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={class}\n")).unwrap();

    let name = port_name(maschine);
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, name.into())], outputs: vec![(OUT, name.into())], ..Default::default() });
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

fn binding(app: &mut App, id: ButtonId) -> Option<i32> {
    ctx(app).surface.button(id).and_then(|b| b.binding.map(|b| b.2))
}

fn display_lines(calls: &[BackendCall]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(OUT, d) if d.len() > 8 && d[..7] == DISPLAY_LINE_1 => Some(String::from_utf8_lossy(&d[7..d.len() - 1]).to_string()),
            _ => None,
        })
        .collect()
}

/// The error messages the core logged.
fn errors(backend: &RecordingBackend) -> Vec<String> {
    backend.logs.borrow().iter().filter(|l| l.contains("ERROR") || l.contains("rror:")).cloned().collect()
}

/// A command which stops the transport (or anything else).
fn stops(c: &Cmd) -> bool {
    matches!(c, Cmd::StopToggle {} | Cmd::TransportStop {} | Cmd::TransportStopAndRewind {} | Cmd::TrackStop { .. } | Cmd::SceneStopAll {})
}

#[test]
fn the_models_are_registered_with_their_discovery_names() {
    let factories = all_factories();
    let find = |uuid: &str| factories.iter().find(|f| f.info().uuid == uuid).unwrap_or_else(|| panic!("{uuid} is registered"));
    let mk3 = find("9055C36B-0A41-48AD-8675-4D3F133E53AC");
    assert_eq!(mk3.info().hardware_model, "Maschine Mk3");
    let linux = mk3.midi_discovery_pairs(OperatingSystem::Linux);
    assert_eq!(linux[0], (vec!["Maschine MK3 Ctrl MIDI".to_string()], vec!["Maschine MK3 Ctrl MIDI".to_string()]));
    assert_eq!(mk3.midi_discovery_pairs(OperatingSystem::Mac)[0].0[0], "Maschine MK3 Virtual Input");
    let mikro = find("E87516A3-5401-44B1-9952-B4F821ED3DD5");
    assert_eq!(mikro.midi_discovery_pairs(OperatingSystem::Windows)[0].0[0], "Maschine Mikro MK3");
    let plus = find("BF0FD105-08A3-4A1D-9F19-A8F5A9C4E7DF");
    assert_eq!(plus.midi_discovery_pairs(OperatingSystem::Linux)[0].1[0], "Maschine Plus Virtual");
    // The Studio is not registered (its monitor section is not ported)
    assert!(!factories.iter().any(|f| f.info().uuid == "9E2D8B7C-D032-11EB-B8BC-0242AC130003"));
}

#[test]
fn mk3_starts_with_the_volume_mode_on_the_display() {
    let (mut app, backend) = start(Maschine::Mk3);
    let calls = backend.take_calls();
    assert_eq!(ctx(&mut app).views.active_id(), Some(Views::Play));
    assert!(ctx(&mut app).modes.is_active(Modes::Volume));
    let lines = display_lines(&calls);
    let first = lines.last().expect("the display was written");
    assert!(first.starts_with(">Drums Bass   "), "{first}");
    // No labels of the buttons above the display: they are only for dbm-nihost and the Mk2
    assert!(!calls.iter().any(|c| matches!(c, BackendCall::SendMidi(OUT, d) if d.starts_with(&[0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x10]))));
}

#[test]
fn mk3_shift_comes_from_nis_driver_and_stop_stops() {
    let (mut app, backend) = start(Maschine::Mk3);
    app.on_midi_event(IN, &from_hex_str(&Maschine::Mk3.message_shift_down()));
    assert!(ctx(&mut app).surface.is_shift_pressed());
    app.on_midi_event(IN, &from_hex_str(&Maschine::Mk3.message_shift_up()));
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(Maschine::Mk3.message_shift_down(), "F000210916004D5000014D01F7");
    // Stop is Stop, not Shift
    backend.take_calls();
    cc(&mut app, mk::STOP, 127);
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    cc(&mut app, mk::STOP, 0);
    assert!(backend.take_calls().has_cmd(stops));
}

#[test]
fn mk3_cursor_keys_page_buttons_and_foot_switches() {
    let (mut app, backend) = start(Maschine::Mk3);
    assert_eq!(binding(&mut app, ButtonId::ArrowUp), Some(mk::CURSOR_UP));
    assert_eq!(binding(&mut app, ButtonId::ArrowDown), Some(mk::CURSOR_DOWN));
    assert_eq!(binding(&mut app, ButtonId::ArrowLeft), Some(mk::CURSOR_LEFT));
    assert_eq!(binding(&mut app, ButtonId::ArrowRight), Some(mk::CURSOR_RIGHT));
    assert_eq!(binding(&mut app, ButtonId::PageLeft), Some(mk::PAGE_LEFT));
    assert_eq!(binding(&mut app, ButtonId::PageRight), Some(mk::PAGE_RIGHT));
    assert_eq!(binding(&mut app, ButtonId::Footswitch1), Some(mk::FOOTSWITCH1_TIP));
    assert_eq!(binding(&mut app, ButtonId::Footswitch2), Some(mk::FOOTSWITCH1_RING));
    assert_eq!(binding(&mut app, ButtonId::Footswitch3), None);
    // The track paging of the volume mode with the cursor keys
    backend.take_calls();
    press(&mut app, mk::CURSOR_RIGHT);
    press(&mut app, mk::CURSOR_LEFT);
    assert!(errors(&backend).is_empty(), "{:?}", errors(&backend));
    // The knobs and the encoder change the volume
    cc(&mut app, mk::MODE_KNOB_1, 1);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { .. })));
    cc(&mut app, mk::ENCODER, 127);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { .. })));
}

#[test]
fn mk3_touch_strip_without_the_touch_of_nis_driver() {
    let (mut app, backend) = start(Maschine::Mk3);
    press(&mut app, mk::MOD);
    assert_eq!(MaschineConfiguration::ribbon_mode(&ctx(&mut app).config), RibbonMode::Cc1);
    backend.take_calls();
    // NI's driver does not send the touch of the Mk3's strip (disabled in the DrivenByMoss
    // template): the value alone works
    cc(&mut app, mk::TOUCHSTRIP, 100);
    let calls = backend.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::Midi(IN, 0xB0, 1, 100))), "{calls:?}");
    app.tick();
    let led = backend.take_calls().iter().rev().find_map(|c| match c {
        BackendCall::SendMidi(OUT, d) if d.len() == 3 && d[0] == 0xB0 && d[1] == mk::TOUCHSTRIP as u8 => Some(d[2]),
        _ => None,
    });
    assert_eq!(led, Some(100));
}

#[test]
fn mikro_mk3_emulates_shift_with_stop_and_has_no_display() {
    let (mut app, backend) = start(Maschine::MikroMk3);
    let calls = backend.take_calls();
    assert!(display_lines(&calls).is_empty(), "the Mikro has no MCU display");
    // No knobs, group buttons or cursor keys
    assert!(ctx(&mut app).surface.button(ButtonId::Row31).is_none());
    assert!(ctx(&mut app).surface.button(ButtonId::ArrowUp).is_none());
    assert!(ctx(&mut app).surface.button(ButtonId::Row21).is_none());
    // Stop emulates Shift
    cc(&mut app, mk::STOP, 127);
    assert!(ctx(&mut app).surface.is_shift_pressed());
    backend.take_calls();
    pad(&mut app, 36, 127);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::Undo {})));
    cc(&mut app, mk::STOP, 0);
    assert!(!ctx(&mut app).surface.is_shift_pressed());
    // The encoder changes the volume of the selected track
    backend.take_calls();
    cc(&mut app, mk::ENCODER, 1);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::ChannelVolume { .. })));
    // 16 tracks in a bank (no group buttons)
    assert_eq!(ctx(&mut app).model.current_track_bank().page_size(), 16);
}

#[test]
fn plus_starts_like_the_mk3() {
    let (mut app, backend) = start(Maschine::Plus);
    let calls = backend.take_calls();
    assert!(!display_lines(&calls).is_empty());
    app.on_midi_event(IN, &from_hex_str(&Maschine::Plus.message_shift_down()));
    assert!(ctx(&mut app).surface.is_shift_pressed());
    assert_eq!(binding(&mut app, ButtonId::ArrowUp), Some(mk::CURSOR_UP));
}

/// Every view, mode and button of a model, without a panic or an error.
fn run_everything(maschine: Maschine) {
    let (mut app, backend) = start(maschine);
    let views = [Views::ScenePlay, Views::Session, Views::Drum, Views::Play, Views::Device, Views::RepeatNote, Views::Shift];
    let modes = [Modes::Browser, Modes::Volume, Modes::Pan, Modes::Send1, Modes::Tempo, Modes::Position, Modes::RepeatNote, Modes::Scales, Modes::Note, Modes::DeviceParams];
    for view in views {
        for mode in modes {
            if !ctx(&mut app).modes.contains(mode) {
                continue;
            }
            {
                let ctx = ctx(&mut app);
                ctx.set_active_view(view);
                ctx.set_active_mode(mode);
            }
            app.tick();
            // The Mikro has only the encoder (no knobs, no cursor keys: their CCs are unbound)
            let controls: &[i32] = if maschine == Maschine::MikroMk3 { &[mk::ENCODER] } else { &[mk::ENCODER, mk::MODE_KNOB_1, mk::MODE_KNOB_1 + 7, mk::CURSOR_UP, mk::CURSOR_LEFT] };
            for &control in controls {
                cc(&mut app, control, 1);
                cc(&mut app, control, 127);
            }
            for note in [36, 41, 47, 51] {
                pad(&mut app, note, 100);
                pad(&mut app, note, 0);
            }
            app.tick();
        }
    }
    for view in views {
        for id in ctx(&mut app).surface.button_ids() {
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
    for ribbon_mode in RibbonMode::ALL {
        MaschineConfiguration::set_ribbon_mode(&mut ctx(&mut app).config, ribbon_mode);
        cc(&mut app, mk::TOUCHSTRIP, 64);
        app.tick();
    }
    assert!(errors(&backend).is_empty(), "{:?}", errors(&backend));
    app.shutdown();
}

#[test]
fn mk3_all_views_modes_and_buttons() {
    run_everything(Maschine::Mk3);
}

#[test]
fn mikro_mk3_all_views_modes_and_buttons() {
    run_everything(Maschine::MikroMk3);
}

#[test]
fn plus_all_views_modes_and_buttons() {
    run_everything(Maschine::Plus);
}
