// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! "Touch everything" test of Generic Flexi: every command is executed with every knob mode and
//! with button, absolute, relative and 14 bit values; every mode, function layer and every
//! option of the slot editor settings is activated. Nothing may panic.

use std::rc::Rc;
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::RecordingBackend;
use drivenbymoss::config::generic_flexi as cfg;
use drivenbymoss::controller::generic::command::{CommandCategory, FlexiCommand};
use drivenbymoss::controller::generic::slot::{CommandSlot, KnobMode, TYPE_CC, TYPE_MMC, TYPE_NOTE, TYPE_PITCH_BEND, TYPE_PROGRAM_CHANGE};
use drivenbymoss::controller::generic::surface::{command_value, FlexiState};
use drivenbymoss::controller::generic::{editor, mapping};
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::ids::Modes;

const IN: i32 = 3;
const OUT: i32 = 5;

const MODEL: &str = "track/count 3\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/pan 0.5\n\
track/1/exists 1\ntrack/1/name Bass\ntrack/2/exists 1\ntrack/2/name Group\n\
track/0/send/count 2\ntrack/0/send/0/name FX1\ntrack/0/send/0/volume 0.5\ntrack/0/send/1/name FX2\n\
device/exists 1\ndevice/name ReaSynth\ndevice/param/count 16\ndevice/param/0/name Cutoff\ndevice/param/0/value 0.25\n\
device/hasLayers 1\ndevice/layer/count 2\ndevice/layer/0/exists 1\ndevice/layer/0/name A\ndevice/layer/1/exists 1\ndevice/layer/1/name B\n\
primary/exists 1\nprimary/name Synth\nprimary/param/count 8\n\
eq/exists 1\neq/param/count 24\neq/param/1/name Gain\n\
clip/exists 1\nclip/start 0\nclip/end 4\nplay 0\n";

struct Flexi {
    app: App,
    time: Instant,
}

impl Flexi {
    fn advance(&mut self, ms: u64) {
        self.time += Duration::from_millis(ms);
        self.app.set_time(self.time);
        self.app.tick();
    }

    fn ctx(&mut self) -> &mut Ctx {
        self.app.instance_ctx(0).unwrap()
    }

    fn midi(&mut self, data: &[u8]) {
        self.app.on_midi_event(IN, data);
    }
}

fn start() -> (Flexi, Rc<RecordingBackend>) {
    let dir = std::env::temp_dir().join(format!("dbm-test-flexi-touch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.generic.GenericFlexiControllerInstance\n").unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper-Flexi.config"), "MIDI_INPUT0=Flexi\nMIDI_OUTPUT0=Flexi\n").unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, "Flexi".into())], outputs: vec![(OUT, "Flexi".into())], ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    let time = Instant::now();
    app.set_time(time);
    app.start_infrastructure();
    app.update_model(MODEL);
    let mut flexi = Flexi { app, time };
    for _ in 0..50 {
        flexi.advance(100);
    }
    (flexi, backend)
}

fn set_slot(ctx: &mut Ctx, index: usize, slot: CommandSlot) {
    ctx.ext_mut::<FlexiState>().slots[index] = slot;
}

#[test]
fn touch_every_command() {
    let (mut flexi, backend) = start();
    let knob_modes = KnobMode::ALL;
    for command in FlexiCommand::ALL {
        for (k, knob_mode) in knob_modes.iter().enumerate() {
            let slot = CommandSlot {
                slot_type: TYPE_CC,
                number: 1,
                midi_channel: 0,
                is_high_res: false,
                knob_mode: *knob_mode,
                function_layer: -1,
                send_value: true,
                send_value_when_received: k % 2 == 0,
                command,
            };
            set_slot(flexi.ctx(), 0, slot.clone());
            set_slot(flexi.ctx(), 1, CommandSlot { slot_type: TYPE_NOTE, number: 2, ..slot.clone() });
            set_slot(flexi.ctx(), 2, CommandSlot { slot_type: TYPE_CC, number: 3, is_high_res: true, function_layer: 0, ..slot.clone() });
            set_slot(flexi.ctx(), 3, CommandSlot { slot_type: TYPE_PITCH_BEND, midi_channel: 16, ..slot.clone() });
            set_slot(flexi.ctx(), 4, CommandSlot { slot_type: TYPE_PROGRAM_CHANGE, number: 4, ..slot.clone() });
            set_slot(flexi.ctx(), 5, CommandSlot { slot_type: TYPE_MMC, number: 5, ..slot.clone() });

            for value in [127u8, 0, 65, 1, 63, 64] {
                flexi.midi(&[0xB0, 1, value]);
            }
            flexi.midi(&[0x90, 2, 127]);
            flexi.midi(&[0x80, 2, 0]);
            flexi.midi(&[0xB0, 3, 100]);
            flexi.midi(&[0xB0, 35, 10]);
            flexi.midi(&[0xE0, 0, 64]);
            flexi.midi(&[0xC0, 4, 0]);
            flexi.midi(&[0xF0, 0x7F, 0x00, 0x06, 0x05, 0xF7]);
            assert!(command_value(flexi.ctx(), command) >= -1);
        }
        // Reset the function layer which a layer command might have changed
        flexi.ctx().ext_mut::<FlexiState>().function_layer = 0;
        flexi.advance(450);
        backend.take_calls();
    }
}

#[test]
fn touch_every_mode_and_setting() {
    let (mut flexi, backend) = start();
    let modes = [
        Modes::Track,
        Modes::Volume,
        Modes::Pan,
        Modes::Send1,
        Modes::Send2,
        Modes::Send3,
        Modes::Send4,
        Modes::Send5,
        Modes::Send6,
        Modes::Send7,
        Modes::Send8,
        Modes::DeviceParams,
        Modes::Browser,
    ];
    for mode in modes {
        drivenbymoss::controller::generic::surface::activate_mode(flexi.ctx(), mode);
        for command in FlexiCommand::ALL.iter().filter(|c| c.category() == Some(CommandCategory::Modes)) {
            for knob_mode in KnobMode::ALL {
                drivenbymoss::controller::generic::handler::handle(flexi.ctx(), *command, knob_mode, drivenbymoss::controller::generic::slot::MidiValue::new(65, false));
                drivenbymoss::controller::generic::handler::handle(flexi.ctx(), *command, knob_mode, drivenbymoss::controller::generic::slot::MidiValue::new(8000, true));
            }
        }
        flexi.advance(600);
        backend.take_calls();
    }

    // All options of all settings of the slot editor, for some slots
    for slot in ["1", "2", "300"] {
        flexi.ctx().config.set_value_from_host(cfg::SLOT_SELECTED, slot);
        flexi.advance(50);
        let descriptors: Vec<_> = flexi.ctx().config.descriptors().to_vec();
        for descriptor in descriptors {
            if descriptor.id == cfg::SLOT_SELECTED || descriptor.id == cfg::FILENAME {
                continue;
            }
            for option in descriptor.options().iter().take(200) {
                flexi.ctx().config.set_value_from_host(descriptor.id, option);
                flexi.advance(1);
            }
        }
        editor::learn_set(flexi.ctx());
        flexi.midi(&[0xB0, 40, 1]);
        editor::learn_set(flexi.ctx());
        flexi.midi(&[0xE0, 1, 1]);
        editor::learn_set(flexi.ctx());
        flexi.advance(700);
        backend.take_calls();
    }

    // Export and re-import the edited mapping
    let text = mapping::export_text(flexi.ctx());
    mapping::apply_properties(flexi.ctx(), &drivenbymoss::config::Properties::parse(&text));
    flexi.advance(50);

    // Invalid mapping file values
    let props = drivenbymoss::config::Properties::parse("SLOT0_TYPE=x\nSLOT0_COMMAND=Transport\\: Play\n");
    mapping::apply_properties(flexi.ctx(), &props);
    flexi.advance(50);
    flexi.app.shutdown();
}
