// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Replay of recorded Bitwig sessions (doc/CONFORMANCE_TESTING_PLAN.md §5).
//!
//! The Bitwig extension records everything which goes into and out of the core
//! (`ProtocolRecorder`, files `*.rec`). A replay feeds the inputs (model lines, ticks, MIDI) into
//! a fresh core at the recorded times and checks:
//!
//! - **regression:** after each input, the core sends the same commands, MIDI and note input
//!   tables as in the recording (as a set, the order within one call may differ);
//! - **conformance:** if there is a `<controller>-java.leds` next to `<controller>-rust.rec` (the
//!   final LED state of the Java version in the same scenarios, written by the conformance
//!   runner), the LED state at the end marker of the run equals it, except for the known
//!   differences in `<controller>-java.known`.
//!
//! Replays all recordings in `tests/conformance/*/replay/`, or the one given in `DBM_REPLAY`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};

/// The first line of a LED file of the runner: how system exclusive messages are keyed.
const SYSEX_KEY_BYTES: &str = "#sysex_key_bytes";

/// An event of a recording.
struct Event {
    /// Milliseconds since the start (with decimals since 2026-10-31, whole before).
    time: f64,
    kind: String,
    fields: Vec<String>,
}

fn unescape(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some(other) => result.push(other),
            None => {}
        }
    }
    result
}

fn parse_recording(path: &Path) -> Vec<Event> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut parts = line.split('\t');
            let time = parts.next().and_then(|t| t.parse().ok()).unwrap_or(0.0);
            let kind = parts.next().unwrap_or("").to_string();
            Event { time, kind, fields: parts.map(unescape).collect() }
        })
        .collect()
}

/// The kinds of output events (from the core to the host).
fn is_output(kind: &str) -> bool {
    matches!(kind, "noArg" | "string" | "strings" | "int" | "double" | "delay" | "enable" | "out" | "raw" | "filters" | "keys" | "velocities") || midi_port(kind, "out").is_some()
}

/// The port of a MIDI event of a further port ("in2", "out3", ...; controllers with several
/// surfaces like the MCU with extenders), `None` for other kinds.
fn midi_port(kind: &str, direction: &str) -> Option<i32> {
    kind.strip_prefix(direction).filter(|n| !n.is_empty()).and_then(|n| n.parse().ok()).filter(|n| *n > 0)
}

/// Doubles are compared with 6 fraction digits (Java's and Rust's formatting differ).
fn double(text: &str) -> String {
    text.parse::<f64>().map_or_else(|_| text.to_string(), |v| format!("{v:.6}"))
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// A recorded output event in the normalized form.
fn normalize_event(event: &Event) -> String {
    let mut fields = event.fields.clone();
    if event.kind == "double" {
        if let Some(last) = fields.last_mut() {
            *last = double(last);
        }
    }
    // Without a trailing space: an event without fields (e.g. an empty filter list)
    format!("{} {}", event.kind, fields.join(" ")).trim_end().to_string()
}

/// A call of the core in the normalized form of [`normalize_event`]; None for calls which the
/// extension does not record.
fn normalize_call(call: &BackendCall) -> Option<String> {
    // Without a trailing space, as normalize_event (commands or lists without a value)
    format_call(call).map(|text| text.trim_end().to_string())
}

fn format_call(call: &BackendCall) -> Option<String> {
    Some(match call {
        BackendCall::NoArg(p, c) => format!("noArg {p} {c}"),
        BackendCall::StringArg(p, c, v) => format!("string {p} {c} {v}"),
        BackendCall::StringArgs(p, c, v) => format!("strings {p} {c} {}", v.join(" ")),
        BackendCall::IntArg(p, c, v) => format!("int {p} {c} {v}"),
        BackendCall::DoubleArg(p, c, v) => format!("double {p} {c} {v:.6}"),
        BackendCall::DelayUpdates(p) => format!("delay {p}"),
        BackendCall::EnableUpdates(p, e) => format!("enable {p} {}", if *e { 1 } else { 0 }),
        BackendCall::SendMidi(0, data) if !data.is_empty() => format!("out {}", hex(data)),
        BackendCall::SendMidi(port, data) if !data.is_empty() => format!("out{port} {}", hex(data)),
        // A MIDI event as if played on the device (e.g. the notes of a chord)
        BackendCall::Midi(0, status, data1, data2) => format!("raw {}", hex(&[*status as u8, *data1 as u8, *data2 as u8])),
        BackendCall::NoteInputFilters(0, i, f) => format!("filters {i} {}", f.join(" ")),
        BackendCall::KeyTranslation(0, i, t) => format!("keys {i} {}", t.iter().map(i32::to_string).collect::<Vec<_>>().join(" ")),
        BackendCall::VelocityTranslation(0, i, t) => format!("velocities {i} {}", t.iter().map(i32::to_string).collect::<Vec<_>>().join(" ")),
        _ => return None,
    })
}

/// The LED state of the controller (as the conformance runner keeps it): the last value per
/// status and data 1; for system exclusive messages per the first `sysex_key_bytes` bytes (the
/// rest is the value), or per all but the last data byte if 0.
fn update_leds(leds: &mut BTreeMap<String, String>, data: &[u8], sysex_key_bytes: usize) {
    if data.first() == Some(&0xF0) && sysex_key_bytes > 0 && sysex_key_bytes < data.len() {
        leds.insert(hex(&data[..sysex_key_bytes]), hex(&data[sysex_key_bytes..data.len() - 1]));
    } else if data.first() == Some(&0xF0) && data.len() >= 2 {
        leds.insert(hex(&data[..data.len() - 2]), format!("{:02X}", data[data.len() - 2]));
    } else if data.len() == 3 {
        leds.insert(hex(&data[..2]), format!("{:02X}", data[2]));
    }
}

/// A pattern of the runner (`fnmatch`: `*`, `?`, `[...]`).
fn glob(pattern: &[char], text: &[char]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some('*') => (0..=text.len()).any(|i| glob(&pattern[1..], &text[i..])),
        Some('?') => !text.is_empty() && glob(&pattern[1..], &text[1..]),
        Some('[') => {
            let Some(end) = pattern.iter().position(|&c| c == ']') else {
                return false;
            };
            !text.is_empty() && pattern[1..end].contains(&text[0]) && glob(&pattern[end + 1..], &text[1..])
        }
        Some(&c) => text.first() == Some(&c) && glob(&pattern[1..], &text[1..]),
    }
}

fn read_pairs(path: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(a, b)| (a.to_string(), b.to_string())))
        .collect()
}

/// Replay one recording; returns the problems. `binary`: the binary form of the protocol (the
/// model lines converted to records and fed through `update_model_bin`, every command encoded
/// as a record and decoded again before it is compared).
fn replay(path: &Path, binary: bool) -> Vec<String> {
    let events = parse_recording(path);
    let init = events.iter().find(|e| e.kind == "init").expect("init event");
    let (core_id, app_version) = (init.fields[0].clone(), init.fields.get(1).cloned().unwrap_or_default());

    // The configuration files of the recorded controller
    // (per mode: the text and the binary replay run in parallel)
    let mode = if binary { "bin" } else { "text" };
    let dir = std::env::temp_dir().join(format!("dbm-replay-{}-{mode}-{}", std::process::id(), path.file_stem().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The files which settings name (the mapping of Generic Flexi), as they were when the controller
    // started: copies in the folder of the replay, which replace the recorded paths
    let mut files: Vec<(String, String)> = Vec::new();
    for (index, event) in events.iter().filter(|e| e.kind == "file").enumerate() {
        let original = event.fields[0].clone();
        let name = Path::new(&original).file_name().map_or_else(|| "file".to_string(), |n| n.to_string_lossy().to_string());
        let copy = dir.join(format!("{index}-{name}"));
        std::fs::write(&copy, event.fields.get(1).map_or("", String::as_str)).unwrap();
        files.push((original, copy.to_string_lossy().to_string()));
    }
    let replace_paths = |text: &str| files.iter().fold(text.to_string(), |text, (original, copy)| text.replace(original.as_str(), copy))
        .to_string();
    for event in events.iter().filter(|e| e.kind == "config") {
        std::fs::write(dir.join(&event.fields[0]), replace_paths(event.fields.get(1).map_or("", String::as_str))).unwrap();
    }

    let backend = Rc::new(RecordingBackend { inputs: vec![(0, "replay".into())], outputs: vec![(0, "replay".into())], ..Default::default() });
    backend.binary.set(binary);
    let mut app = App::new_single(backend.clone(), dir.to_str().unwrap(), &core_id, &app_version).expect("controller of the recording");
    // How the runner keyed the system exclusive messages (first line of the Java LED file)
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let java = path.with_file_name(format!("{}-java.leds", stem.trim_end_matches("-rust")));
    let sysex_key_bytes = read_pairs(&java).iter().find(|(k, _)| k == SYSEX_KEY_BYTES).and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
    let base = Instant::now();
    let mut problems = Vec::new();
    let mut expected: Vec<String> = Vec::new();
    let mut actual: Vec<String> = Vec::new();
    let mut leds = BTreeMap::new();
    let mut leds_at_end = None;
    let mut last_input = String::from("start of the recording");

    let compare = |expected: &mut Vec<String>, actual: &mut Vec<String>, input: &str, problems: &mut Vec<String>| {
        // As sets: the order and repetitions of the same call do not matter (e.g. a note mapping
        // which is sent again unchanged)
        expected.sort();
        expected.dedup();
        actual.sort();
        actual.dedup();
        if expected != actual && problems.len() < 20 {
            let missing: Vec<_> = expected.iter().filter(|e| !actual.contains(e)).take(5).collect();
            let extra: Vec<_> = actual.iter().filter(|a| !expected.contains(a)).take(5).collect();
            problems.push(format!("after {input}: missing {missing:?}, unexpected {extra:?}"));
        }
        expected.clear();
        actual.clear();
    };

    for event in &events {
        if is_output(&event.kind) {
            expected.push(normalize_event(event));
            continue;
        }
        let input_port = midi_port(&event.kind, "in");
        if input_port.is_none() && !matches!(event.kind.as_str(), "documentSettings" | "setting" | "start" | "model" | "tick" | "in" | "mark") {
            continue;
        }
        // The outputs of the previous input are complete
        actual.extend(backend.take_calls().iter().filter_map(|call| {
            if let BackendCall::SendMidi(0, data) = call {
                update_leds(&mut leds, data, sysex_key_bytes);
            }
            normalize_call(call)
        }));
        compare(&mut expected, &mut actual, &last_input, &mut problems);

        app.set_time(base + Duration::from_secs_f64(event.time / 1000.0));
        last_input = format!("{} at {} ms", event.kind, event.time);
        match event.kind.as_str() {
            "documentSettings" => app.set_formatted_document_settings(&event.fields[0]),
            "setting" => app.set_setting(event.fields[0].parse().unwrap_or(-1), &replace_paths(event.fields.get(1).map_or("", String::as_str))),
            "start" => app.start_infrastructure(),
            "model" if binary => app.update_model_bin(drivenbymoss::protocol::bin::from_text(&event.fields[0], &mut |_| {}).as_bytes()),
            "model" => app.update_model(&event.fields[0]),
            "tick" => app.tick(),
            "in" => {
                let data: Vec<u8> = event.fields[0].split(' ').filter_map(|b| u8::from_str_radix(b, 16).ok()).collect();
                app.on_midi_event(0, &data);
            }
            "mark" if event.fields.first().is_some_and(|f| f == "end") => leds_at_end = Some(leds.clone()),
            _ if input_port.is_some() => {
                let data: Vec<u8> = event.fields[0].split(' ').filter_map(|b| u8::from_str_radix(b, 16).ok()).collect();
                app.on_midi_event(input_port.unwrap_or(0), &data);
            }
            _ => {}
        }
    }
    actual.extend(backend.take_calls().iter().filter_map(normalize_call));
    compare(&mut expected, &mut actual, &last_input, &mut problems);

    // Conformance with the Java version
    if let (Some(leds), true) = (leds_at_end, java.exists()) {
        let known: Vec<Vec<char>> = read_pairs(&java.with_extension("known")).into_iter().map(|(p, _)| p.chars().collect()).collect();
        for (key, value) in read_pairs(&java).into_iter().filter(|(k, _)| k != SYSEX_KEY_BYTES) {
            let id: Vec<char> = format!("leds {key}").chars().collect();
            if leds.get(&key) != Some(&value) && !known.iter().any(|p| glob(p, &id)) {
                problems.push(format!("LED {key}: Java {value}, Rust {:?}", leds.get(&key)));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    problems
}

fn recordings() -> Vec<PathBuf> {
    if let Ok(file) = std::env::var("DBM_REPLAY") {
        return vec![PathBuf::from(file)];
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
    let mut files = Vec::new();
    for controller in std::fs::read_dir(root).into_iter().flatten().flatten() {
        for file in std::fs::read_dir(controller.path().join("replay")).into_iter().flatten().flatten() {
            if file.path().extension().is_some_and(|e| e == "rec") {
                files.push(file.path());
            }
        }
    }
    files.sort();
    files
}

#[test]
fn replay_recorded_sessions() {
    replay_all(false);
}

/// The binary mode (TYPED_PROTOCOL_PLAN §3, step 1): the binary form changes nothing.
#[test]
fn replay_recorded_sessions_binary() {
    replay_all(true);
}

fn replay_all(binary: bool) {
    let mut failures = Vec::new();
    for path in recordings() {
        let problems = replay(&path, binary);
        if !problems.is_empty() {
            failures.push(format!("{}:\n  {}", path.display(), problems.join("\n  ")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn glob_patterns() {
    let m = |p: &str, t: &str| glob(&p.chars().collect::<Vec<_>>(), &t.chars().collect::<Vec<_>>());
    assert!(m("leds 9C [45][9ABC]", "leds 9C 4A"));
    assert!(!m("leds 9C [45][9ABC]", "leds 9C 4D"));
    assert!(m("*/vu", "osc:/track/1/vu"));
}
