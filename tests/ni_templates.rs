// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Checks that the MIDI numbers of the Maschine surfaces match the NI Controller Editor templates,
//! as generated for the Linux NI host emulation (`support/linux/ni_host/src/template_map.h`).

use std::collections::HashMap;

use drivenbymoss::controller::maschine::jam::surface as jam;
use drivenbymoss::controller::maschine::mk::surface as mk;

/// (direction, kind, channel, number) of each control of a device in the generated table.
fn template(device: &str) -> HashMap<String, (String, String, i32, i32)> {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/support/linux/ni_host/src/template_map.h")).unwrap();
    let start = text.find(&format!("TemplateControl {device}[]")).unwrap();
    let end = text[start..].find("};").unwrap() + start;
    let mut controls = HashMap::new();
    for line in text[start..end].lines() {
        let line = line.trim();
        let Some(body) = line.strip_prefix("{ \"") else {
            continue;
        };
        let (id, rest) = body.split_once('"').unwrap();
        let fields: Vec<&str> = rest.trim_start_matches(',').split(',').map(str::trim).collect();
        let direction = fields[0].trim_start_matches("Direction::").to_string();
        let kind = fields[1].trim_start_matches("MessageKind::").to_string();
        controls.insert(format!("{id}/{direction}"), (direction, kind, fields[2].parse().unwrap(), fields[3].parse().unwrap()));
    }
    controls
}

fn cc(controls: &HashMap<String, (String, String, i32, i32)>, id: &str) -> i32 {
    let (_, kind, channel, number) = controls.get(&format!("{id}/Input")).unwrap_or_else(|| panic!("{id} is missing"));
    assert_eq!((kind.as_str(), *channel), ("Cc", 0), "{id}");
    *number
}

#[test]
fn jam_surface_matches_the_template() {
    let t = template("jam");
    let pairs = [
        ("BtnArrange", jam::SONG),
        ("BtnStep", jam::STEP),
        ("BtnPadMode", jam::PAD_MODE),
        ("BtnClear", jam::CLEAR),
        ("BtnDuplicate", jam::DUPLICATE),
        ("BtnArpRepeat", jam::NOTE_REPEAT),
        ("BtnMacro", jam::MACRO),
        ("BtnLevel", jam::LEVEL),
        ("BtnAux", jam::AUX),
        ("BtnControl", jam::CONTROL),
        ("BtnAuto", jam::AUTO),
        ("BtnDpad1", jam::NAV_UP),
        ("BtnDpad3", jam::NAV_DOWN),
        ("BtnDpad4", jam::NAV_LEFT),
        ("BtnDpad2", jam::NAV_RIGHT),
        ("BtnSelect", jam::SELECT),
        ("BtnSwing", jam::SWING),
        ("BtnTune", jam::TUNE),
        ("BtnLock", jam::LOCK),
        ("BtnVariation", jam::NOTES),
        ("BtnPerform", jam::PERFORM),
        ("BtnBrowse", jam::BROWSE),
        ("EncBrowse", jam::KNOB_TURN),
        ("PshBrowse", jam::KNOB_PUSH),
        ("CapBrowse", jam::KNOB_TOUCH),
        ("BtnIn1", jam::IN),
        ("BtnCue", jam::HEADPHONE),
        ("BtnMst", jam::MASTER),
        ("BtnGrp", jam::GROUP),
        ("BtnPlay", jam::PLAY),
        ("BtnRecord", jam::RECORD),
        ("BtnArrowLeft", jam::LEFT),
        ("BtnArrowRight", jam::RIGHT),
        ("BtnTempo", jam::TEMPO),
        ("BtnGrid", jam::GRID),
        ("BtnSolo", jam::SOLO),
        ("BtnMute", jam::MUTE),
    ];
    for (id, number) in pairs {
        assert_eq!(cc(&t, id), number, "{id}");
    }
    for i in 0..8 {
        let letter = (b'A' + i as u8) as char;
        assert_eq!(cc(&t, &format!("BtnGroup{letter}")), jam::GROUP_A + i, "group {letter}");
        assert_eq!(cc(&t, &format!("BtnScene{}", i + 1)), jam::SCENE1 + i, "scene {}", i + 1);
        assert_eq!(cc(&t, &format!("Tst{letter}")), jam::FADER1 + i, "strip {letter}");
        assert_eq!(cc(&t, &format!("CapTst{letter}")), jam::FADER_TOUCH1 + i, "strip touch {letter}");
    }
    // The matrix: notes from 36 (bottom left) to 99 (top right), as the pad grid of the surface
    for x in 0..8 {
        for y in 0..8 {
            let id = format!("Btn{}{}/Input", (b'A' + x as u8) as char, y + 1);
            let (_, kind, _, number) = &t[&id];
            assert_eq!((kind.as_str(), *number), ("Note", 36 + x + 8 * (7 - y)), "{id}");
        }
    }
}

#[test]
fn mk2_surface_matches_the_template() {
    let t = template("mk2");
    let pairs = [
        ("Dial", mk::ENCODER),
        ("Push", mk::ENCODER_PUSH),
        ("MasterR", mk::CURSOR_RIGHT),
        ("MasterL", mk::CURSOR_LEFT),
        ("AutoWrite", mk::AUTO),
        ("Enter", mk::LOCK),
        ("NoteRep", mk::NOTE_REPEAT),
        ("All", mk::PROJECT),
        ("Browse", mk::BROWSER),
        ("Control", mk::CHANNEL),
        ("Volume", mk::VOLUME),
        ("Swing", mk::SWING),
        ("Sampling", mk::SAMPLING),
        ("Tempo", mk::TEMPO),
        ("Restart", mk::RESTART),
        ("Erase", mk::ERASE),
        ("StepR", mk::TAP_METRO),
        ("Grid", mk::FOLLOW),
        ("Play", mk::PLAY),
        ("Rec", mk::REC),
        ("StepL", mk::STOP),
        ("PadMode", mk::PAD_MODE),
        ("Step", mk::STEP),
        ("Scene", mk::SCENE),
        ("Pattern", mk::PATTERN),
        ("Navigate", mk::VARIATION),
        ("Duplicate", mk::DUPLICATE),
        ("Select", mk::SELECT),
        ("Solo", mk::SOLO),
        ("Mute", mk::MUTE),
    ];
    for (id, number) in pairs {
        assert_eq!(cc(&t, id), number, "{id}");
    }
    for i in 0..8 {
        let letter = (b'A' + i as u8) as char;
        assert_eq!(cc(&t, &format!("Group{letter}")), mk::BANK_1 + i, "group {letter}");
        assert_eq!(cc(&t, &format!("Knob{}", i + 1)), mk::MODE_KNOB_1 + i, "knob {}", i + 1);
        if i != 6 {
            assert_eq!(cc(&t, &format!("Button{}", i + 1)), mk::MODE_BUTTON_1 + i, "button {}", i + 1);
        }
    }
    // The 7th display button is an MCU button: a note
    let (_, kind, _, number) = &t["Button7/Input"];
    assert_eq!((kind.as_str(), *number), ("Note", mk::MODE_BUTTON_7));
    // Pads 1..16 from the bottom left: notes 36..51
    for n in 1..=16 {
        let (_, kind, _, number) = &t[&format!("Pad{n}/Input")];
        assert_eq!((kind.as_str(), *number), ("Note", 35 + n), "pad {n}");
    }
}
