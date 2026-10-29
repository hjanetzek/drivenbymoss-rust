// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! End-to-end test: start a Launchpad X instance with a recording backend.

use std::rc::Rc;

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dbm-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn launchpad_x_starts_and_talks_to_the_device() {
    let dir = temp_dir("lpx");
    std::fs::write(
        dir.join("DrivenByMoss4Reaper.config"),
        "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadXControllerInstance\n",
    )
    .unwrap();

    let backend = Rc::new(RecordingBackend {
        inputs: vec![(0, "Some Keyboard".into()), (3, "Launchpad X LPX MIDI Out".into())],
        outputs: vec![(5, "Launchpad X LPX MIDI In".into())],
        ..Default::default()
    });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    assert_eq!(app.instance_count(), 1);
    app.start_infrastructure();

    let calls = backend.take_calls();
    assert!(calls.contains(&BackendCall::OpenInput(3)));
    assert!(calls.contains(&BackendCall::OpenOutput(5)));
    assert!(calls.iter().any(|c| matches!(c, BackendCall::NoteInputFilters(3, 0, _))));
    // Programmer mode sysex for the Launchpad X
    let prg = vec![0xF0, 0x00, 0x20, 0x29, 0x02, 0x0C, 0x0E, 0x01, 0xF7];
    assert!(calls.contains(&BackendCall::SendMidi(5, prg)), "{calls:?}");

    app.update_model("track/count 1\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\nplay 0\n");
    app.tick();
    let calls = backend.take_calls();
    // Pads are notes 11..88 (column 1..8), the scene buttons (column 9) are also sent as notes on
    // the non-Pro models
    let is_pad = |d: &[u8]| d[0] == 0x90 && (1..=8).contains(&(d[1] % 10)) && (1..=8).contains(&(d[1] / 10));
    let pad_updates = calls.iter().filter(|c| matches!(c, BackendCall::SendMidi(5, d) if is_pad(d))).count();
    assert_eq!(pad_updates, 64, "all pads are sent once");

    // Nothing changed: nothing is sent again
    app.tick();
    assert!(backend.take_calls().iter().all(|c| !matches!(c, BackendCall::SendMidi(..))));

    // Document settings round trip
    let settings = app.formatted_document_settings();
    assert!(settings.starts_with("LAUNCHPAD_X=\""), "{settings}");
    app.set_formatted_document_settings(&settings);

    app.shutdown();
    assert!(backend.take_calls().contains(&BackendCall::CloseInput(3)));
    assert!(std::fs::read_to_string(dir.join("DrivenByMoss4Reaper-Launchpad-X.config")).unwrap().contains("Launchpad X LPX MIDI Out"));
}

#[test]
fn every_view_can_be_activated_and_drawn_on_all_models() {
    use drivenbymoss::framework::ids::Views;
    use drivenbymoss::midi::ButtonEvent;

    let models = [
        ("LaunchpadMkIIControllerInstance", "Launchpad MK2 MIDI 1"),
        ("LaunchpadProControllerInstance", "Launchpad Pro MIDI 2"),
        ("LaunchpadProMk3ControllerInstance", "Launchpad Pro MK3 MIDI 1"),
        ("LaunchpadXControllerInstance", "Launchpad X MIDI 2"),
        ("LaunchpadMiniMkIIIControllerInstance", "Launchpad Mini MK3 MIDI 2"),
    ];
    for (class, port) in models {
        let dir = temp_dir(class);
        std::fs::write(
            dir.join("DrivenByMoss4Reaper.config"),
            format!("CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchpad.{class}\n"),
        )
        .unwrap();
        let backend = Rc::new(RecordingBackend { inputs: vec![(1, port.into())], outputs: vec![(2, port.into())], ..Default::default() });
        let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
        app.start_infrastructure();
        app.update_model(
            "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/1/exists 1\ntrack/1/name Bass\n\
             track/0/clip/0/exists 1\nscene/count 2\nscene/0/exists 1\nplay 1\n",
        );
        app.tick();
        for view in Views::ALL {
            let ctx = app.instance_ctx(0).expect("instance runs");
            if !ctx.views.contains(*view) {
                continue;
            }
            ctx.set_active_view(*view);
            // Press and release some pads and a scene button in each view
            for pad in [drivenbymoss::framework::ids::ButtonId::Pad1, drivenbymoss::framework::ids::ButtonId::Pad64, drivenbymoss::framework::ids::ButtonId::Scene1] {
                ctx.trigger_button(pad, ButtonEvent::Down, 1.0);
                ctx.trigger_button(pad, ButtonEvent::Up, 0.0);
            }
            app.tick();
        }
        backend.take_calls();
        app.shutdown();
    }
}
