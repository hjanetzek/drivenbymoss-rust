// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The settings and notifications for hosts which show them in their own UI (Bitwig: controller
//! preferences, document state, popup notifications).

use std::rc::Rc;

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::config::SettingId;

/// A single Launch Control XL instance as the Bitwig extension creates it.
fn start(name: &str) -> App {
    let dir = std::env::temp_dir().join(format!("dbm-test-host-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(0, "in".into())], outputs: vec![(0, "out".into())], ..Default::default() });
    let mut app = App::new_single(backend, dir.to_str().unwrap(), "5AFA5045-FF92-4737-A091-5B371CD5E529", "Bitwig Studio 5.3").expect("Launch Control XL");
    app.start_infrastructure();
    app
}

fn fields(settings: &str, label: &str) -> Vec<String> {
    let line = settings.lines().find(|l| l.split('\t').nth(4) == Some(label)).unwrap_or_else(|| panic!("{label} in {settings}"));
    line.split('\t').map(str::to_string).collect()
}

#[test]
fn settings_of_the_controller() {
    let mut app = start("settings");
    let settings = app.settings();
    // As LaunchControlXLConfiguration.init: scale settings in the document, the rest global
    let labels: Vec<&str> = settings.lines().map(|l| l.split('\t').nth(4).unwrap()).collect();
    assert_eq!(labels, ["Scale", "Base", "In Key", "Behaviour on Pause", "Double click PLAY returns to zero", "MIDI Edit/Insert note channel", "Exclude deactivated items (tracks, sends, devices, layers)", "New Clip Length"]);

    let scale = fields(&settings, "Scale");
    assert_eq!(scale[1..4], ["document", "enum", "Scales"]);
    let pause = fields(&settings, "Behaviour on Pause");
    assert_eq!(pause[1..4], ["global", "enum", "Transport"]);
    assert!(pause[7..].contains(&pause[5]), "the default is one of the options");
    let double_click = fields(&settings, "Double click PLAY returns to zero");
    assert_eq!(double_click[2], "enum");
    let channel = fields(&settings, "MIDI Edit/Insert note channel");
    assert_eq!(channel[2], "enum");
}

#[test]
fn changes_from_the_host_and_the_controller() {
    let mut app = start("changes");
    let settings = app.settings();
    let length = fields(&settings, "New Clip Length");
    let id: i32 = length[0].parse().unwrap();
    let other = length[7..].iter().find(|o| **o != length[6]).unwrap().clone();
    // The values loaded on start
    app.take_setting_changes();

    // Set by the host: applied, not reported back
    app.set_setting(id, &other);
    assert_eq!(fields(&app.settings(), "New Clip Length")[6], other);
    assert_eq!(app.take_setting_changes(), "");
    // Invalid values are ignored
    app.set_setting(id, "no such option");
    assert_eq!(fields(&app.settings(), "New Clip Length")[6], other);

    // Changed by the controller: reported to the host
    let config = &mut app.instance_ctx(0).unwrap().config;
    config.set_value(SettingId(id as u16), &length[6]);
    assert_eq!(app.take_setting_changes(), format!("{id}\t{}", length[6]));
    assert_eq!(app.take_setting_changes(), "");
}

#[test]
fn notifications() {
    let mut app = start("notifications");
    app.take_notifications();
    // Factory template 1 (Mixer)
    app.on_midi_event(0, &[0xF0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x77, 0x08, 0xF7]);
    assert_eq!(app.take_notifications(), "Selected Mode: Mixer");
    assert_eq!(app.take_notifications(), "");
}
