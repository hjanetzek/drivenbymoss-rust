// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Checks of the Generic Flexi conformance scenarios (`tests/conformance/generic_flexi/*.toml`)
//! without Bitwig: the commands of the mappings exist, the mapping loads into the port with all
//! its slots, and the MIDI of all steps runs through the port without errors. What the steps do
//! in Bitwig is checked by the pair runs (doc/CONFORMANCE_TESTING_PLAN.md).
//!
//! The scenario files are TOML; the crate has no TOML parser, so this reads the subset the files
//! use: the `slots` of the `[mapping]` (one inline table per line) and the `midi` values of the
//! steps. The runner (`backends/bitwig/tools/conformance_runner.py`, `flexi_mapping_properties`)
//! writes the same mapping file for Bitwig.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::controller::generic::command::FlexiCommand;
use drivenbymoss::controller::generic::surface::flexi;

const IN: i32 = 3;
const OUT: i32 = 5;

/// A model with the tracks of the fixture (tests/conformance/FIXTURE.md), enough for the commands
/// to find their targets.
const MODEL: &str = "track/count 10\n\
track/0/exists 1\ntrack/0/name Synth\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/pan 0.5\n\
track/1/exists 1\ntrack/1/name Drums\ntrack/2/exists 1\ntrack/2/name Layers\ntrack/3/exists 1\ntrack/3/name EQ\n\
track/4/exists 1\ntrack/4/name Audio 5\ntrack/5/exists 1\ntrack/5/name Audio 6\ntrack/6/exists 1\ntrack/6/name Audio 7\n\
track/7/exists 1\ntrack/7/name Audio 8\ntrack/8/exists 1\ntrack/8/name Audio 9\ntrack/9/exists 1\ntrack/9/name Audio 10\n\
track/0/send/count 2\ntrack/0/send/0/name FX 1\ntrack/0/send/1/name FX 2\n\
device/exists 1\ndevice/name Polysynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.25\nplay 0\n";

/// The keys of the runner's FLEXI_STARTUP_SLOT: the runner adds it as the last slot.
const STARTUP_SLOT: &str = r#"{ type = "cc", number = 119, channel = 16, command = "Master: Set Volume", send_value = true }"#;

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

/// Parse the key/value pairs of a TOML inline table on one line, e.g.
/// `{ type = "cc", number = 70, command = "Track 1: Set Volume", high_res = true }`.
fn parse_inline_table(line: &str) -> HashMap<String, Value> {
    let inner = line.trim().trim_end_matches(',').trim();
    let inner = inner.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or_else(|| panic!("not an inline table: {line}"));
    let mut table = HashMap::new();
    let mut chars = inner.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            chars.next();
        }
        let key: String = std::iter::from_fn(|| chars.next_if(|c| c.is_alphanumeric() || *c == '_')).collect();
        if key.is_empty() {
            break;
        }
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == '=') {
            chars.next();
        }
        let value = if chars.peek() == Some(&'"') {
            chars.next();
            let mut text = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => text.push(chars.next().expect("escape")),
                    _ => text.push(c),
                }
            }
            Value::Str(text)
        } else {
            let word: String = std::iter::from_fn(|| chars.next_if(|c| !c.is_whitespace() && *c != ',')).collect();
            match word.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::Int(word.parse().unwrap_or_else(|_| panic!("value {word} in {line}"))),
            }
        };
        table.insert(key, value);
    }
    table
}

/// A scenario file: its slots (with the startup slot of the runner), its keyboard channel and
/// the MIDI messages of all steps in the order of the file.
struct ScenarioFile {
    path: PathBuf,
    slots: Vec<HashMap<String, Value>>,
    keyboard_channel: Option<String>,
    midi: Vec<Vec<u8>>,
}

fn parse_hex(text: &str) -> Vec<u8> {
    text.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap_or_else(|_| panic!("hex byte {b}"))).collect()
}

fn read_scenario_file(path: &Path) -> ScenarioFile {
    let text = std::fs::read_to_string(path).unwrap();
    let mut slots = Vec::new();
    let mut keyboard_channel = None;
    let mut midi = Vec::new();
    let mut in_mapping = false;
    let mut in_slots = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && !trimmed.starts_with("[[") && trimmed.ends_with(']') {
            in_mapping = trimmed == "[mapping]";
            continue;
        }
        if trimmed.starts_with("[[") {
            in_mapping = false;
        }
        if in_mapping && trimmed.starts_with("slots = [") {
            in_slots = true;
            continue;
        }
        if in_slots {
            if trimmed == "]" {
                in_slots = false;
            } else if trimmed.starts_with('{') {
                slots.push(parse_inline_table(trimmed));
            }
            continue;
        }
        if in_mapping && trimmed.starts_with("keyboard = ") {
            if let Some(Value::Str(channel)) = parse_inline_table(trimmed.trim_start_matches("keyboard = ")).get("channel") {
                keyboard_channel = Some(channel.clone());
            }
            continue;
        }
        // The midi values of the setup, before and steps arrays, in the order of the file
        let mut rest = line;
        while let Some(start) = rest.find("midi = \"") {
            let after = &rest[start + 8..];
            let end = after.find('"').expect("closing quote");
            midi.extend(after[..end].split('|').map(str::trim).filter(|m| !m.is_empty()).map(parse_hex));
            rest = &after[end..];
        }
    }
    slots.push(parse_inline_table(STARTUP_SLOT));
    ScenarioFile { path: path.to_path_buf(), slots, keyboard_channel, midi }
}

fn int(slot: &HashMap<String, Value>, key: &str, default: i64) -> i64 {
    match slot.get(key) {
        Some(Value::Int(v)) => *v,
        Some(Value::Str(s)) if s == "all" => -1,
        None => default,
        other => panic!("{key}: {other:?}"),
    }
}

fn boolean(slot: &HashMap<String, Value>, key: &str) -> bool {
    matches!(slot.get(key), Some(Value::Bool(true)))
}

fn string<'a>(slot: &'a HashMap<String, Value>, key: &str) -> &'a str {
    match slot.get(key) {
        Some(Value::Str(s)) => s,
        other => panic!("{key}: {other:?}"),
    }
}

/// The mapping file, as the runner writes it (slot indices: the slots in order, the startup slot
/// at 299; the empty slots are left out, they are off after a restart).
fn mapping_file(file: &ScenarioFile) -> String {
    const TYPES: [&str; 5] = ["cc", "note", "pc", "pitchbend", "mmc"];
    const KNOB_MODES: [&str; 6] = ["absolute", "relative_twos_complement", "relative_signed_bit", "relative_signed_bit_2", "relative_offset_binary", "absolute_toggle"];
    let mut text = String::from("#Generic Flexi\n");
    let count = file.slots.len();
    for (i, slot) in file.slots.iter().enumerate() {
        let index = if i == count - 1 { 299 } else { i };
        let slot_type = TYPES.iter().position(|t| *t == string(slot, "type")).expect("slot type") as i64;
        let channel = int(slot, "channel", 1);
        let layer = int(slot, "layer", 1);
        let knob_mode = match slot.get("knob_mode") {
            Some(Value::Str(mode)) => KNOB_MODES.iter().position(|m| m == mode).unwrap_or_else(|| panic!("knob mode {mode}")),
            _ => 0,
        };
        text.push_str(&format!(
            "SLOT{index}_TYPE={slot_type}\nSLOT{index}_NUMBER={}\nSLOT{index}_MIDI_CHANNEL={}\nSLOT{index}_RESOLUTION={}\nSLOT{index}_KNOB_MODE={knob_mode}\n\
SLOT{index}_FUNCTION_LAYER={}\nSLOT{index}_COMMAND={}\nSLOT{index}_SEND_VALUE={}\nSLOT{index}_SEND_VALUE_WHEN_RECEIVED={}\n",
            int(slot, "number", 0),
            if channel < 0 { 16 } else { channel - 1 },
            boolean(slot, "high_res"),
            if layer < 0 { -1 } else { layer - 1 },
            string(slot, "command").replace(':', "\\:"),
            boolean(slot, "send_value"),
            boolean(slot, "send_value_when_received"),
        ));
    }
    text.push_str(&format!("KEYBOARD_CHANNEL={}\n", file.keyboard_channel.as_deref().unwrap_or("Off")));
    text
}

fn scenario_files() -> Vec<ScenarioFile> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/generic_flexi");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "toml")).collect();
    paths.sort();
    assert!(!paths.is_empty(), "no scenario files in {}", dir.display());
    paths.iter().map(|p| read_scenario_file(p)).collect()
}

#[test]
fn the_commands_of_the_mappings_exist() {
    for file in scenario_files() {
        for slot in &file.slots {
            let name = string(slot, "command");
            assert_ne!(FlexiCommand::lookup_by_name(name), FlexiCommand::Off, "{}: unknown command '{name}'", file.path.display());
        }
    }
}

#[test]
fn the_steps_run_through_the_port_without_errors() {
    for file in scenario_files() {
        let name = file.path.file_stem().unwrap().to_string_lossy().to_string();
        let dir = std::env::temp_dir().join(format!("dbm-test-flexi-scenarios-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mapping = dir.join("mapping.properties");
        std::fs::write(&mapping, mapping_file(&file)).unwrap();
        std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.generic.GenericFlexiControllerInstance\n").unwrap();
        std::fs::write(
            dir.join("DrivenByMoss4Reaper-Flexi.config"),
            format!("MIDI_INPUT0=My Controller\nMIDI_OUTPUT0=My Controller\nLOAD_/_SAVE_FILENAME={}\n", mapping.display().to_string().replace('\\', "\\\\")),
        )
        .unwrap();

        let backend = Rc::new(RecordingBackend { inputs: vec![(IN, "My Controller".into())], outputs: vec![(OUT, "My Controller".into())], ..Default::default() });
        let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
        let start = Instant::now();
        let mut elapsed = 0;
        let mut advance = |app: &mut App, ms: u64| {
            elapsed += ms;
            app.set_time(start + Duration::from_millis(elapsed));
            app.tick();
        };
        app.set_time(start);
        app.start_infrastructure();
        app.update_model(MODEL);
        // Startup (1 s), loading the file (2 s) and applying it (1 s)
        for _ in 0..50 {
            advance(&mut app, 100);
        }

        let loaded = flexi(app.instance_ctx(0).unwrap()).slots.iter().filter(|s| s.command != FlexiCommand::Off).count();
        assert_eq!(loaded, file.slots.len(), "{name}: not all slots of the mapping are loaded");
        // The startup slot sends the master volume: the startup output the runner waits for
        assert!(
            backend.take_calls().iter().any(|c| matches!(c, drivenbymoss::backend::BackendCall::SendMidi(OUT, d) if d.first() == Some(&0xBF) && d.get(1) == Some(&119))),
            "{name}: the startup slot did not send its value"
        );

        assert!(!file.midi.is_empty(), "{name}: no MIDI steps");
        for message in &file.midi {
            app.on_midi_event(IN, message);
            advance(&mut app, 100);
        }
        for _ in 0..20 {
            advance(&mut app, 100);
        }

        let errors: Vec<String> = backend.logs.borrow().iter().filter(|l| l.starts_with("ERROR")).cloned().collect();
        assert!(errors.is_empty(), "{name}: errors while running the steps: {errors:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn inline_tables_are_parsed() {
    let table = parse_inline_table(r#"{ type = "cc", number = 3, layer = "all", command = "Track 8: Set Volume", high_res = true, note = "a, b" },"#);
    assert_eq!(table.get("type"), Some(&Value::Str("cc".into())));
    assert_eq!(table.get("number"), Some(&Value::Int(3)));
    assert_eq!(int(&table, "layer", 1), -1);
    assert_eq!(table.get("high_res"), Some(&Value::Bool(true)));
    assert_eq!(table.get("note"), Some(&Value::Str("a, b".into())));
}
