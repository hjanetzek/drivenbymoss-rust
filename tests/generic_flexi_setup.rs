// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of Generic Flexi: loading of the mapping file, the MIDI handling of the command slots,
//! the function layers, sending values back to the device, the slot editor and the MIDI learn.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::{BackendCall, RecordingBackend};
use drivenbymoss::config::generic_flexi as cfg;
use drivenbymoss::controller::generic::command::FlexiCommand;
use drivenbymoss::controller::generic::slot::{KnobMode, TYPE_CC, TYPE_NOTE};
use drivenbymoss::controller::generic::{editor, mapping};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::Modes;

const IN: i32 = 3;
const OUT: i32 = 5;

const MODEL: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/pan 0.5\n\
track/1/exists 1\ntrack/1/name Bass\ntrack/1/volume 0.25\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/0/volume 0.5\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 8\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.25\nplay 0\n";

/// A slot of a mapping file: type, number, MIDI channel, knob mode, function layer, command,
/// send value, high resolution.
pub struct Slot<'a> {
    pub slot_type: i32,
    pub number: i32,
    pub channel: i32,
    pub knob_mode: KnobMode,
    pub layer: i32,
    pub command: &'a str,
    pub send_value: bool,
    pub high_res: bool,
}

impl<'a> Slot<'a> {
    pub fn new(slot_type: i32, number: i32, command: &'a str) -> Self {
        Slot { slot_type, number, channel: 0, knob_mode: KnobMode::Absolute, layer: 0, command, send_value: false, high_res: false }
    }
}

/// Format a mapping file with the given slots.
pub fn mapping_file(slots: &[Slot]) -> String {
    let mut text = String::from("#Generic Flexi\n");
    for (i, slot) in slots.iter().enumerate() {
        text.push_str(&format!(
            "SLOT{i}_TYPE={}\nSLOT{i}_NUMBER={}\nSLOT{i}_MIDI_CHANNEL={}\nSLOT{i}_KNOB_MODE={}\nSLOT{i}_FUNCTION_LAYER={}\nSLOT{i}_COMMAND={}\nSLOT{i}_SEND_VALUE={}\nSLOT{i}_RESOLUTION={}\n",
            slot.slot_type,
            slot.number,
            slot.channel,
            slot.knob_mode.ordinal(),
            slot.layer,
            slot.command.replace(':', "\\:").replace(' ', "\\ "),
            slot.send_value,
            slot.high_res
        ));
    }
    text.push_str("ASSIGNABLE_FUNCTION_ACTIONS0=40001\nKEYBOARD_CHANNEL=1\nKEYBOARD_ROUTE_MODULATION=On\n");
    text
}

/// The test folder.
fn test_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-flexi-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A running Generic Flexi instance with a virtual clock.
pub struct Flexi {
    pub app: App,
    pub backend: Rc<RecordingBackend>,
    pub dir: PathBuf,
    time: Instant,
}

impl Flexi {
    /// Start an instance with the given mapping (an empty mapping if None) and wait until the
    /// mapping file is loaded.
    pub fn start(name: &str, slots: Option<&[Slot]>, extra_config: &str) -> Flexi {
        let dir = test_dir(name);
        std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.generic.GenericFlexiControllerInstance\n").unwrap();
        let mut config = format!("MIDI_INPUT0=My Controller\nMIDI_OUTPUT0=My Controller\n{extra_config}");
        if let Some(slots) = slots {
            let mapping = dir.join("mapping.properties");
            std::fs::write(&mapping, mapping_file(slots)).unwrap();
            config.push_str(&format!("LOAD_/_SAVE_FILENAME={}\n", mapping.display().to_string().replace('\\', "\\\\")));
        }
        std::fs::write(dir.join("DrivenByMoss4Reaper-Flexi.config"), config).unwrap();

        let backend = Rc::new(RecordingBackend {
            inputs: vec![(IN, "My Controller".into())],
            outputs: vec![(OUT, "My Controller".into())],
            ..Default::default()
        });
        let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
        let time = Instant::now();
        app.set_time(time);
        app.start_infrastructure();
        app.update_model(MODEL);
        let mut flexi = Flexi { app, backend, dir, time };
        // Startup (1 s), loading the file (2 s) and applying it (1 s)
        for _ in 0..50 {
            flexi.advance(100);
        }
        flexi
    }

    /// Advance the clock and tick.
    pub fn advance(&mut self, ms: u64) {
        self.time += Duration::from_millis(ms);
        self.app.set_time(self.time);
        self.app.tick();
    }

    pub fn ctx(&mut self) -> &mut Ctx {
        self.app.instance_ctx(0).unwrap()
    }

    pub fn midi(&mut self, data: &[u8]) {
        self.app.on_midi_event(IN, data);
    }

    pub fn take_calls(&self) -> Vec<BackendCall> {
        self.backend.take_calls()
    }
}

fn double_arg(calls: &[BackendCall], processor: &str, command: &str) -> Option<f64> {
    calls.iter().rev().find_map(|c| match c {
        BackendCall::IntArg(p, cmd, v) if p == processor && cmd == command => Some(*v as f64),
        BackendCall::DoubleArg(p, cmd, v) if p == processor && cmd == command => Some(*v),
        _ => None,
    })
}

fn has_call(calls: &[BackendCall], processor: &str, command: &str) -> bool {
    calls.iter().any(|c| match c {
        BackendCall::NoArg(p, cmd) | BackendCall::IntArg(p, cmd, _) | BackendCall::DoubleArg(p, cmd, _) | BackendCall::StringArg(p, cmd, _) => p == processor && cmd == command,
        _ => false,
    })
}

/// The MIDI messages sent to the device.
fn midi_out(calls: &[BackendCall]) -> Vec<Vec<u8>> {
    calls
        .iter()
        .filter_map(|c| match c {
            BackendCall::SendMidi(OUT, d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

fn state(flexi: &mut Flexi) -> &drivenbymoss::controller::generic::surface::FlexiState {
    drivenbymoss::controller::generic::surface::flexi(flexi.ctx())
}

#[test]
fn factory_and_mapping_file_are_loaded() {
    let factory = drivenbymoss::controller::find_factory("1b43f14f-caaf-468a-9418-417c678653a0").unwrap();
    assert_eq!(factory.java_class_name(), "de.mossgrabers.reaper.controller.generic.GenericFlexiControllerInstance");
    assert_eq!(factory.info().to_string(), "Generic Flexi");

    let mut slot = Slot::new(TYPE_NOTE, 60, "Transport: Play");
    slot.send_value = true;
    let slots = [Slot::new(TYPE_CC, 7, "Track 1: Set Volume"), slot];
    let mut flexi = Flexi::start("load", Some(&slots), "");
    let state = state(&mut flexi);
    assert_eq!(state.slots[0].command, FlexiCommand::Track1SetVolume);
    assert_eq!(state.slots[1].command, FlexiCommand::TransportPlay);
    assert_eq!(state.slots[1].number, 60);
    assert!(state.slots[1].send_value);
    assert_eq!(state.slots[2].command, FlexiCommand::Off);
    let ctx = flexi.ctx();
    assert_eq!(cfg::assignable_action(&ctx.config, 0), "40001");
    // The default mode (Track) of the "Selected Mode" setting
    assert!(ctx.modes.is_active(Modes::Track));
}

#[test]
fn notes_on_the_keyboard_channel_are_blocked() {
    let slots = [Slot::new(TYPE_NOTE, 60, "Transport: Play"), Slot { channel: 3, ..Slot::new(TYPE_NOTE, 61, "Transport: Stop") }];
    let flexi = Flexi::start("keymap", Some(&slots), "");
    let calls = flexi.take_calls();
    let table = calls
        .iter()
        .rev()
        .find_map(|c| match c {
            BackendCall::KeyTranslation(IN, 0, table) => Some(table.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(table[60], -1);
    // Another MIDI channel than the keyboard
    assert_eq!(table[61], 61);
    assert_eq!(table[59], 59);
}

#[test]
fn keyboard_filters() {
    let mut flexi = Flexi::start("filters", None, "KEYBOARD_/_PADS_(REQUIRES_RESTART)_MIDI_CHANNEL=All\nKEYBOARD_/_PADS_(REQUIRES_RESTART)_ROUTE_TIMBRE_(CC74)=On\n");
    let filters = drivenbymoss::controller::generic::setup::midi_filters(&flexi.ctx().config);
    assert_eq!(filters, vec!["8?????", "9?????", "A?????", "D?????", "B?01??", "B?40??", "B?4A??", "E?????"]);
}

#[test]
fn cc_sets_the_volume() {
    let slots = [Slot::new(TYPE_CC, 7, "Track 1: Set Volume"), Slot { knob_mode: KnobMode::RelativeSignedBit, ..Slot::new(TYPE_CC, 8, "Track 2: Set Volume") }];
    let mut flexi = Flexi::start("volume", Some(&slots), "");
    flexi.take_calls();
    flexi.midi(&[0xB0, 7, 127]);
    let calls = flexi.take_calls();
    assert_eq!(double_arg(&calls, "track", "0/volume"), Some(1.0));

    // Other MIDI channel
    flexi.midi(&[0xB1, 7, 127]);
    assert!(!has_call(&flexi.take_calls(), "track", "0/volume"));

    // Relative signed bit: 65 = +1
    flexi.midi(&[0xB0, 8, 65]);
    let volume = double_arg(&flexi.take_calls(), "track", "1/volume").unwrap();
    assert!(volume > 0.25 && volume < 0.26, "{volume}");
    // 1 = -1
    flexi.midi(&[0xB0, 8, 1]);
    let decreased = double_arg(&flexi.take_calls(), "track", "1/volume").unwrap();
    assert!(decreased < volume, "{decreased}");
}

#[test]
fn high_resolution_cc() {
    let slots = [Slot { high_res: true, ..Slot::new(TYPE_CC, 7, "Track 1: Set Volume") }];
    let mut flexi = Flexi::start("hires", Some(&slots), "");
    flexi.take_calls();
    // MSB, then LSB
    flexi.midi(&[0xB0, 7, 64]);
    flexi.midi(&[0xB0, 39, 0]);
    let calls = flexi.take_calls();
    let volume = double_arg(&calls, "track", "0/volume").unwrap();
    assert!((volume - 8192.0 / 16383.0).abs() < 0.0001, "{volume}");
}

#[test]
fn trigger_commands_and_program_change() {
    let slots = [
        Slot::new(TYPE_NOTE, 60, "Transport: Play"),
        Slot::new(drivenbymoss::controller::generic::slot::TYPE_PROGRAM_CHANGE, 5, "Transport: Toggle Metronome"),
        Slot::new(drivenbymoss::controller::generic::slot::TYPE_MMC, 2, "Transport: Toggle Repeat"),
        Slot::new(TYPE_CC, 20, "Action: Execute Action 1"),
    ];
    let mut flexi = Flexi::start("trigger", Some(&slots), "");
    flexi.take_calls();
    // Play is executed on release
    flexi.midi(&[0x90, 60, 127]);
    flexi.midi(&[0x80, 60, 0]);
    let calls = flexi.take_calls();
    assert!(has_call(&calls, "play", ""), "{calls:?}");

    flexi.midi(&[0xC0, 5, 0]);
    assert!(has_call(&flexi.take_calls(), "transport", "toggleMetronome"));

    // MMC play on device 0
    flexi.midi(&[0xF0, 0x7F, 0x00, 0x06, 0x02, 0xF7]);
    assert!(has_call(&flexi.take_calls(), "repeat", ""));

    flexi.midi(&[0xB0, 20, 127]);
    let calls = flexi.take_calls();
    assert!(calls.iter().any(|c| matches!(c, BackendCall::StringArg(p, _, v) if p == "action" && v == "40001") || matches!(c, BackendCall::IntArg(p, _, 40001) if p == "action")), "{calls:?}");
}

#[test]
fn function_layers() {
    let slots = [
        Slot { layer: -1, ..Slot::new(TYPE_NOTE, 10, "Function Layers: Activate Layer 2 temporarily") },
        Slot::new(TYPE_CC, 70, "Track 1: Set Volume"),
        Slot { layer: 1, ..Slot::new(TYPE_CC, 70, "Track 2: Set Volume") },
    ];
    let mut flexi = Flexi::start("layers", Some(&slots), "");
    flexi.take_calls();
    flexi.midi(&[0xB0, 70, 127]);
    let calls = flexi.take_calls();
    assert!(has_call(&calls, "track", "0/volume") && !has_call(&calls, "track", "1/volume"));

    // Hold the layer button
    flexi.midi(&[0x90, 10, 127]);
    assert_eq!(state(&mut flexi).function_layer, 1);
    flexi.midi(&[0xB0, 70, 127]);
    let calls = flexi.take_calls();
    assert!(!has_call(&calls, "track", "0/volume") && has_call(&calls, "track", "1/volume"));

    // Release returns to the previous layer
    flexi.midi(&[0x80, 10, 0]);
    assert_eq!(state(&mut flexi).function_layer, 0);
}

#[test]
fn values_are_sent_to_the_device() {
    let slots = [Slot { send_value: true, ..Slot::new(TYPE_CC, 7, "Track 1: Set Volume") }, Slot { send_value: true, ..Slot::new(TYPE_NOTE, 60, "Transport: Play") }];
    let mut flexi = Flexi::start("reflect", Some(&slots), "");
    let out = midi_out(&flexi.take_calls());
    // Volume 0.5 (about 8191 in 14 bit) is 64 in 7 bit, not playing is 0
    assert!(out.contains(&vec![0xB0, 7, 64]), "{out:?}");
    assert!(out.contains(&vec![0x90, 60, 0]), "{out:?}");

    // No resend if unchanged
    flexi.advance(50);
    assert!(midi_out(&flexi.take_calls()).is_empty());

    // Changed in the DAW
    flexi.app.update_model("track/0/volume 1\nplay 1\n");
    flexi.advance(50);
    let out = midi_out(&flexi.take_calls());
    assert!(out.contains(&vec![0xB0, 7, 127]), "{out:?}");
    assert!(out.contains(&vec![0x90, 60, 127]), "{out:?}");

    // While a command is handled (400 ms) no values are sent
    flexi.midi(&[0xB0, 7, 10]);
    flexi.app.update_model("track/0/volume 0\n");
    flexi.advance(100);
    assert!(midi_out(&flexi.take_calls()).is_empty());
    flexi.advance(400);
    assert!(midi_out(&flexi.take_calls()).contains(&vec![0xB0, 7, 0]));
}

#[test]
fn modes_and_selected_mode_setting() {
    let slots = [Slot::new(TYPE_NOTE, 1, "Modes: Select Device Mode"), Slot::new(TYPE_CC, 1, "Modes: Item 1: Set value"), Slot::new(TYPE_NOTE, 2, "Modes: Select Next Mode")];
    let mut flexi = Flexi::start("modes", Some(&slots), "");
    flexi.midi(&[0x90, 1, 127]);
    assert!(flexi.ctx().modes.is_active(Modes::DeviceParams));
    flexi.advance(50);
    assert_eq!(cfg::selected_mode_name(&flexi.ctx().config), "Device");
    flexi.take_calls();

    flexi.midi(&[0xB0, 1, 127]);
    let calls = flexi.take_calls();
    assert_eq!(double_arg(&calls, "device", "param/0/value"), Some(1.0));

    // Next mode after Device is Track
    flexi.midi(&[0x90, 2, 127]);
    assert!(flexi.ctx().modes.is_active(Modes::Track));

    // The setting selects the mode
    flexi.ctx().config.set_value(cfg::SELECTED_MODE, "Panning");
    flexi.advance(50);
    assert!(flexi.ctx().modes.is_active(Modes::Pan));
}

#[test]
fn shift_button() {
    let slots = [Slot::new(TYPE_NOTE, 1, "Global: Shift Button"), Slot { send_value: true, ..Slot::new(TYPE_NOTE, 1, "Global: Shift Button") }];
    let mut flexi = Flexi::start("shift", Some(&slots), "");
    flexi.midi(&[0x90, 1, 127]);
    assert!(flexi.ctx().surface.is_shift_pressed());
    assert!(flexi.ctx().surface.is_knob_sensitivity_slow());
    flexi.midi(&[0x80, 1, 0]);
    assert!(!flexi.ctx().surface.is_shift_pressed());
}

#[test]
fn export_and_import() {
    let slots = [Slot { knob_mode: KnobMode::RelativeOffsetBinary, layer: 3, ..Slot::new(TYPE_CC, 7, "Track 1: Set Volume") }, Slot::new(TYPE_NOTE, 60, "Transport: Play")];
    let mut flexi = Flexi::start("export", Some(&slots), "");
    let text = mapping::export_text(flexi.ctx());
    assert!(text.starts_with("#Generic Flexi\n#"));
    let props = drivenbymoss::config::Properties::parse(&text);
    assert_eq!(props.get("SLOT0_COMMAND"), Some("Track 1: Set Volume"));
    assert_eq!(props.get("SLOT0_KNOB_MODE"), Some("4"));
    assert_eq!(props.get("SLOT0_FUNCTION_LAYER"), Some("3"));
    assert_eq!(props.get("SLOT1_TYPE"), Some("1"));
    assert_eq!(props.get("SLOT2_TYPE"), Some("-1"));
    assert_eq!(props.get("SLOT299_COMMAND"), Some("Off"));
    assert_eq!(props.get("KEYBOARD_CHANNEL"), Some("1"));
    assert_eq!(props.get("ENABLE_MPE"), Some("Off"));
    assert_eq!(props.get("MPE_PITCHBEND_RANGE"), Some("48"));

    // Save to a new file and load it again (the ending is added)
    let file = flexi.dir.join("saved");
    mapping::save_file(flexi.ctx(), file.to_str().unwrap());
    let saved = format!("{}.properties", file.display());
    assert_eq!(cfg::filename(&flexi.ctx().config), saved);
    assert!(std::path::Path::new(&saved).exists());

    flexi.ctx().ext_mut::<drivenbymoss::controller::generic::surface::FlexiState>().slots[0].command = FlexiCommand::Off;
    let message = mapping::load_file(flexi.ctx(), &saved);
    assert!(message.starts_with("Imported from: "));
    flexi.advance(1100);
    assert_eq!(state(&mut flexi).slots[0].command, FlexiCommand::Track1SetVolume);
    assert_eq!(state(&mut flexi).slots[0].knob_mode, KnobMode::RelativeOffsetBinary);

    assert_eq!(mapping::load_file(flexi.ctx(), ""), "No file name set.");
    assert!(mapping::load_file(flexi.ctx(), "/does/not/exist.properties").starts_with("The entered file does not exist: "));
}

#[test]
fn slot_editor_and_midi_learn() {
    let mut flexi = Flexi::start("editor", None, "");
    {
        let ctx = flexi.ctx();
        ctx.config.set_value_from_host(cfg::SLOT_SELECTED, "3");
    }
    flexi.advance(50);
    assert_eq!(state(&mut flexi).selected_slot, 2);

    // Learn a CC on channel 2
    flexi.midi(&[0xB1, 74, 100]);
    assert_eq!(flexi.ctx().config.value(cfg::LEARN_TYPE).as_deref(), Some("CC"));
    assert_eq!(flexi.ctx().config.value(cfg::LEARN_MIDI_CHANNEL).as_deref(), Some("2"));
    editor::learn_set(flexi.ctx());
    flexi.advance(50);
    // Select a command in the category Transport
    let category = drivenbymoss::controller::generic::command::CommandCategory::Transport;
    flexi.ctx().config.set_value_from_host(cfg::function_setting(category), "Transport: Toggle Metronome");
    flexi.advance(50);
    {
        let slot = state(&mut flexi).slots[2].clone();
        assert_eq!(slot.slot_type, TYPE_CC);
        assert_eq!(slot.number, 74);
        assert_eq!(slot.midi_channel, 1);
        assert_eq!(slot.command, FlexiCommand::TransportToggleMetronome);
    }
    flexi.take_calls();
    flexi.midi(&[0xB1, 74, 127]);
    assert!(has_call(&flexi.take_calls(), "transport", "toggleMetronome"));

    // Selecting a command of another category resets the old category
    let groove = drivenbymoss::controller::generic::command::CommandCategory::Groove;
    flexi.ctx().config.set_value_from_host(cfg::function_setting(groove), "Groove: Set Shuffle Amount");
    flexi.advance(50);
    assert_eq!(state(&mut flexi).slots[2].command, FlexiCommand::GrooveShuffleAmount);
    assert_eq!(flexi.ctx().config.value(cfg::function_setting(category)).as_deref(), Some("Off"));
    flexi.advance(50);
    assert_eq!(state(&mut flexi).slots[2].command, FlexiCommand::GrooveShuffleAmount);

    // A trigger command forces the absolute knob mode
    flexi.ctx().config.set_value_from_host(cfg::SLOT_KNOB_MODE, KnobMode::RelativeSignedBit.label());
    flexi.advance(50);
    assert_eq!(state(&mut flexi).slots[2].knob_mode, KnobMode::RelativeSignedBit);
    flexi.ctx().config.set_value_from_host(cfg::function_setting(category), "Transport: Play");
    flexi.advance(50);
    assert_eq!(state(&mut flexi).slots[2].knob_mode, KnobMode::Absolute);

    // Selecting another slot shows its values
    flexi.ctx().config.set_value_from_host(cfg::SLOT_SELECTED, "1");
    flexi.advance(50);
    assert_eq!(flexi.ctx().config.value(cfg::SLOT_TYPE).as_deref(), Some("Off"));
    assert_eq!(flexi.ctx().config.value(cfg::function_setting(category)).as_deref(), Some("Off"));
    flexi.advance(50);
    assert_eq!(state(&mut flexi).slots[2].command, FlexiCommand::TransportPlay);
    assert_eq!(state(&mut flexi).slots[0].command, FlexiCommand::Off);

    // 14 bit is only possible for CCs below 32
    flexi.ctx().config.set_value_from_host(cfg::SLOT_SELECTED, "3");
    flexi.advance(50);
    flexi.ctx().config.set_value_from_host(cfg::SLOT_RESOLUTION, "14-bit");
    flexi.advance(50);
    assert!(!state(&mut flexi).slots[2].is_high_res);
    assert_eq!(flexi.ctx().config.value(cfg::SLOT_RESOLUTION).as_deref(), Some("7-bit"));
}

#[test]
fn settings_are_stored() {
    let mut flexi = Flexi::start("store", None, "OPTIONS_ACTION_2=40002\n");
    assert_eq!(cfg::assignable_action(&flexi.ctx().config, 1), "40002");
    flexi.ctx().config.set_value(cfg::SELECTED_MODE, "Send 2");
    flexi.advance(50);
    flexi.app.shutdown();
    let text = std::fs::read_to_string(flexi.dir.join("DrivenByMoss4Reaper-Flexi.config")).unwrap();
    assert!(text.contains("OPTIONS_SELECTED_MODE=Send 2"), "{text}");
    assert!(text.contains("OPTIONS_ACTION_2=40002"), "{text}");
    assert!(text.contains("MIDI_INPUT0=My Controller"), "{text}");
}

#[test]
fn program_banks() {
    let slots = [Slot::new(TYPE_CC, 7, "Track 1: Set Volume")];
    let mut flexi = Flexi::start("programs", Some(&slots), "");
    std::fs::write(flexi.dir.join("mapping.programs"), "$BANK=Piano$MSB=0$LSB=1$CHANNEL=2\nGrand\nUpright\n$BANK=Organ$MSB=3$LSB=4$CHANNEL=2\nB3\n").unwrap();
    // The programs file is read when the controller is initialised
    flexi.app.restart_controllers();
    for _ in 0..50 {
        flexi.advance(100);
    }
    let first = mapping::PROGRAM_BANK_FIRST;
    let second = first.offset(1);
    assert_eq!(flexi.ctx().config.descriptor(first).unwrap().category, "mapping Program Banks");
    flexi.take_calls();
    flexi.ctx().config.set_value(second, "B3");
    flexi.advance(50);
    flexi.ctx().config.set_value(first, "Upright");
    flexi.advance(50);
    let out = midi_out(&flexi.take_calls());
    // Bank select MSB/LSB and the program change on channel 3
    assert!(out.contains(&vec![0xB2, 0, 3]) && out.contains(&vec![0xB2, 32, 4]) && out.contains(&vec![0xC2, 0, 0]), "{out:?}");
    assert!(out.contains(&vec![0xC2, 1, 0]), "{out:?}");
    assert_eq!(flexi.ctx().config.value(second).as_deref(), Some("None"));
}

/// The MPE configuration (RPN 6 on zone 1 and 2) is only sent with the keyboard note input, as
/// the Bitwig version, which has no note input without a keyboard channel or MPE.
#[test]
fn mpe_configuration_only_with_the_keyboard() {
    let is_rpn = |m: &Vec<u8>| m.len() == 3 && (m[0] & 0xF0) == 0xB0 && m[1] == 0x65;
    let mut with_keyboard = Flexi::start("mpe-keyboard", None, "");
    with_keyboard.advance(2500);
    assert!(midi_out(&with_keyboard.take_calls()).iter().any(is_rpn), "the default keyboard channel 1 has the note input");

    let mut without = Flexi::start("mpe-off", None, "KEYBOARD_/_PADS_(REQUIRES_RESTART)_MIDI_CHANNEL=Off\n");
    without.advance(2500);
    assert!(!midi_out(&without.take_calls()).iter().any(is_rpn), "no MPE configuration without the note input");
}

/// The layer commands act on the layers of the cursor device, which the host reports as
/// `/device/hasLayers` and `/device/layer/<i>/…` (the Bitwig extension since 2026-11-02).
#[test]
fn layer_commands_use_the_layers_of_the_cursor_device() {
    let slots = [Slot::new(TYPE_CC, 70, "Layer 2: Select"), Slot::new(TYPE_CC, 71, "Layer 1: Toggle Mute")];
    let mut flexi = Flexi::start("layers", Some(&slots), "");
    flexi.app.update_model("/device/hasLayers 1\n/device/layer/count 2\n/device/layer/0/exists 1\n/device/layer/0/name A\n/device/layer/0/select 1\n/device/layer/1/exists 1\n/device/layer/1/name B\n");
    flexi.advance(100);
    flexi.take_calls();
    flexi.midi(&[0xB0, 70, 127]);
    flexi.midi(&[0xB0, 71, 127]);
    flexi.advance(100);
    let calls = flexi.take_calls();
    assert!(has_call(&calls, "device", "layer/1/select"), "{calls:?}");
    assert!(has_call(&calls, "device", "layer/0/mute"), "{calls:?}");
}

/// Without a keyboard channel (and MPE) the controller has no note input: an empty filter list,
/// as the Java version on Bitwig (no note input), so the notes do not reach the track.
#[test]
fn no_note_input_without_the_keyboard() {
    let filters = |flexi: &Flexi| -> Vec<Vec<String>> {
        flexi.backend.calls.borrow().iter().filter_map(|c| match c {
            BackendCall::NoteInputFilters(IN, 0, f) => Some(f.clone()),
            _ => None,
        }).collect()
    };
    let without = Flexi::start("no-note-input", None, "KEYBOARD_/_PADS_(REQUIRES_RESTART)_MIDI_CHANNEL=Off\n");
    assert_eq!(filters(&without), vec![Vec::<String>::new()]);
    let with = Flexi::start("note-input", None, "");
    assert!(filters(&with).iter().all(|f| f.iter().any(|f| f.starts_with("90"))), "{:?}", filters(&with));
}

/// "Device: Parameters" toggles the remote controls section of the cursor device on a host with
/// the device commands (Bitwig; Reaper has no such section).
#[test]
fn device_parameters_toggles_the_remote_controls_section() {
    let slots = [Slot::new(TYPE_CC, 70, "Device: Parameters")];
    let mut flexi = Flexi::start("device-parameters", Some(&slots), "");
    flexi.app.update_model("/host/features deviceCommands\n/device/parameters 1\n");
    flexi.advance(100);
    flexi.take_calls();
    flexi.midi(&[0xB0, 70, 127]);
    flexi.advance(100);
    let calls = flexi.take_calls();
    assert!(has_call(&calls, "device", "parameters/toggle"), "{calls:?}");
}
