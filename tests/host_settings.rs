// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The settings and notifications for hosts which show them in their own UI (Bitwig: controller
//! preferences, document state, popup notifications).

use std::rc::Rc;

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::config::SettingId;
use drivenbymoss::protocol::CmdId;

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

/// The global settings are written to the configuration file soon after a change (at most once
/// per SAVE_SETTINGS_DELAY_MS), not only when the controller stops; the document settings not.
#[test]
fn global_settings_are_saved_after_a_change() {
    let dir = std::env::temp_dir().join(format!("dbm-test-host-settings-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("DrivenByMoss-Launch-Control-XL.config");
    let backend = Rc::new(RecordingBackend { inputs: vec![(0, "in".into())], outputs: vec![(0, "out".into())], ..Default::default() });
    let mut app = App::new_single(backend, dir.to_str().unwrap(), "5AFA5045-FF92-4737-A091-5B371CD5E529", "Bitwig Studio 5.3").expect("Launch Control XL");
    let mut time = 0;
    app.set_time_ms(time);
    app.start_infrastructure();
    let mut run = |app: &mut App, ms: u64| {
        let end = time + ms;
        while time < end {
            time += 30;
            app.set_time_ms(time);
            app.tick();
        }
    };
    // No change: no file (also not after the startup of the controller)
    run(&mut app, 3000);
    assert!(!file.exists());

    let settings = app.settings();
    let length = fields(&settings, "New Clip Length");
    let id: i32 = length[0].parse().unwrap();
    let options: Vec<String> = length[7..].iter().filter(|o| **o != length[6]).take(2).cloned().collect();
    // Turned through two values: one write with the last one, after the delay
    app.set_setting(id, &options[0]);
    run(&mut app, 300);
    app.set_setting(id, &options[1]);
    run(&mut app, 300);
    assert!(!file.exists(), "not before the delay");
    run(&mut app, 600);
    let text = std::fs::read_to_string(&file).expect("written after the delay");
    assert!(text.contains(&format!("WORKFLOW_NEW_CLIP_LENGTH={}", options[1])), "{text}");

    // No further write without a change
    std::fs::remove_file(&file).unwrap();
    run(&mut app, 3000);
    assert!(!file.exists());

    // A document setting is stored by the host, not in the file
    let scale = fields(&app.settings(), "Scale");
    let other = scale[7..].iter().find(|o| **o != scale[6]).unwrap().clone();
    app.set_setting(scale[0].parse().unwrap(), &other);
    run(&mut app, 3000);
    assert!(!file.exists());

    // A change by the controller is written as well
    let config = &mut app.instance_ctx(0).unwrap().config;
    config.set_value(SettingId(id as u16), &options[0]);
    run(&mut app, 1100);
    let text = std::fs::read_to_string(&file).expect("written after the delay");
    assert!(text.contains(&format!("WORKFLOW_NEW_CLIP_LENGTH={}", options[0])), "{text}");

    // And on stop, as before
    std::fs::remove_file(&file).unwrap();
    app.shutdown();
    assert!(file.exists());
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

/// The settings of Generic Flexi as the Bitwig extension creates them (HostSettings): the kinds it
/// knows, enumeration defaults among the options, and no label twice in a category (Bitwig does
/// not create a second setting with the same label in the same category).
#[test]
fn settings_of_generic_flexi() {
    let dir = std::env::temp_dir().join(format!("dbm-test-host-settings-flexi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(0, "in".into())], outputs: vec![(0, "out".into())], ..Default::default() });
    let mut app = App::new_single(backend, dir.to_str().unwrap(), "1B43F14F-CAAF-468A-9418-417C678653A0", "Bitwig Studio 5.3").expect("Generic Flexi");
    app.start_infrastructure();
    let settings = app.settings();
    assert!(settings.lines().count() > 20, "Flexi has settings: {settings}");
    let mut seen = std::collections::HashSet::new();
    for line in settings.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        assert!(fields.len() >= 7, "fields of {line}");
        assert!(["enum", "integer", "boolean", "text", "action", "signal"].contains(&fields[2]), "kind of {line}");
        if fields[2] == "enum" {
            assert!(fields[7..].contains(&fields[5]), "the default of {} is one of the options", fields[4]);
        }
        assert!(seen.insert((fields[1], fields[3], fields[4])), "{} twice in the category {}", fields[4], fields[3]);
    }

    // As GenericFlexiConfiguration on Bitwig: the actions are action settings, Set/Save/Load
    // buttons
    let action = fields(&settings, "Action 1");
    assert_eq!(action[2..4], ["action", "Options"]);
    let save = fields(&settings, "  ");
    assert_eq!(save[2..4], ["signal", "Load / Save"]);
    assert_eq!(save[7], "Save");
    assert_eq!(fields(&settings, "   ")[7], "Load");
    let set = fields(&settings, " ");
    assert_eq!(set[2..4], ["signal", "Use a knob/fader/button then click Set..."]);
    assert_eq!(set[7], "Set");

    // The first states: the learn values are disabled, the settings of the selected slot (type
    // "Off") hidden
    let states: Vec<Vec<String>> = app.take_setting_states().lines().map(|l| l.split('\t').map(str::to_string).collect()).collect();
    let state_of = |label: &str, category: &str| {
        let line = settings.lines().find(|l| l.split('\t').nth(4) == Some(label) && l.split('\t').nth(3) == Some(category)).unwrap_or_else(|| panic!("{label}"));
        let id = line.split('\t').next().unwrap().to_string();
        states.iter().find(|s| s[0] == id).map(|s| (s[1].clone(), s[2].clone()))
    };
    assert_eq!(state_of("Type:", "Use a knob/fader/button then click Set..."), Some(("1".into(), "0".into())));
    assert_eq!(state_of("Number:", "Selected Slot - MIDI trigger"), Some(("0".into(), "1".into())));
    assert_eq!(state_of("Track:", "Selected Slot - Function"), Some(("0".into(), "1".into())));
    assert_eq!(state_of("Type:", "Selected Slot - MIDI trigger"), None, "the type stays visible");
    assert!(app.take_setting_states().is_empty(), "reported once");

    // A slot type other than Off shows them
    let type_id: i32 = settings.lines().find(|l| l.split('\t').nth(4) == Some("Type:") && l.split('\t').nth(3) == Some("Selected Slot - MIDI trigger")).unwrap().split('\t').next().unwrap().parse().unwrap();
    app.set_setting(type_id, "CC");
    app.tick();
    let shown = app.take_setting_states();
    assert!(shown.lines().count() >= 7 && shown.lines().all(|l| l.ends_with("\t1\t1")), "{shown}");

    // A button press is a change of the setting (here "Save" without a file name)
    let save_id: i32 = save[0].parse().unwrap();
    app.set_setting(save_id, "");
    app.tick();
    assert!(app.take_notifications().contains("No file name set."));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The states of the settings of a Push 2 whose host handles the given commands (None: a text
/// host; an empty bitset: all commands).
fn push2_states(name: &str, handled: Option<Vec<u64>>) -> (App, Rc<RecordingBackend>) {
    let dir = std::env::temp_dir().join(format!("dbm-test-host-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(0, "in".into())], outputs: vec![(0, "out".into())], ..Default::default() });
    *backend.handled.borrow_mut() = handled;
    let mut app = App::new_single(backend.clone(), dir.to_str().unwrap(), "15176AA0-C476-11E6-9598-0800200C9A66", "zynthian").expect("Push 2");
    app.start_infrastructure();
    (app, backend)
}

/// All command IDs except the given ones, as a handled-commands bitset.
fn all_but(except: &[CmdId]) -> Vec<u64> {
    let max = CmdId::ALL.iter().map(|&id| usize::from(id as u16 & 0x7FFF)).max().unwrap();
    let mut words = vec![0u64; max / 64 + 1];
    for &id in CmdId::ALL.iter().filter(|id| !except.contains(id)) {
        let bit = usize::from(id as u16 & 0x7FFF);
        words[bit / 64] |= 1 << (bit % 64);
    }
    words
}

const AUTOMATION: [CmdId; 3] = [CmdId::AutomationToggleArranger, CmdId::AutomationToggleLauncher, CmdId::AutomationMode];

/// A setting whose effects all need commands the host does not handle is reported hidden; with
/// one of them (or on a host which handles everything) it stays visible.
#[test]
fn settings_without_effect_on_the_host_are_hidden() {
    let label = "Stop automation recording on knob release";

    let (mut app, backend) = push2_states("no-automation", Some(all_but(&AUTOMATION)));
    let id = fields(&app.settings(), label)[0].clone();
    let states = app.take_setting_states();
    assert!(states.lines().any(|l| l == format!("{id}\t0\t1")), "{states}");
    // Still listed (the host decides how to show a hidden setting) and still stored
    assert!(app.settings().contains(label));
    // The host handles the automation commands later (dbm_set_command_callback again): shown
    *backend.handled.borrow_mut() = Some(all_but(&[]));
    assert_eq!(app.take_setting_states(), format!("{id}\t1\t1"));
    assert_eq!(app.take_setting_states(), "");

    // One of the commands is enough
    let (mut app, _) = push2_states("one-automation", Some(all_but(&AUTOMATION[1..])));
    assert!(!app.take_setting_states().lines().any(|l| l.starts_with(&format!("{id}\t"))));
    // All commands (empty bitset) and a text host: nothing hidden
    for (name, handled) in [("all-commands", Some(Vec::new())), ("text-host", None)] {
        let (mut app, _) = push2_states(name, handled);
        let states = app.take_setting_states();
        assert!(!states.lines().any(|l| l.starts_with(&format!("{id}\t"))), "{name}: {states}");
    }
}
