// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The mapping files of the command slots (former `GenericFlexiConfiguration.exportTo`,
//! `importFrom` and `applyProperties`, `GenericFlexiControlSurface.loadFile` and `saveFile`)
//! and the program banks of the programs file (part of the former
//! `GenericFlexiControllerSetup.init`).
//!
//! The mapping files are Java properties files with the same keys as in Java, so existing
//! mappings keep working and mappings can be exchanged with the Java version.
//!
//! Differences to Java:
//! - There are no file dialogs: [`save_file`] and [`load_file`] get the file name.
//! - Empty values (e.g. an unused assignable action) are not written to the file (they are
//!   read back as the default, which is the same).
//! - The program bank settings are created when the controller is initialised, after the
//!   document settings of the project were applied; their stored values are applied with the
//!   next document settings of the host.

use std::path::{Path, PathBuf};

use super::command::FlexiCommand;
use super::editor::select_slot;
use super::slot::{KnobMode, ProgramBank, TYPE_OFF};
use super::surface::{clear_note_map, flexi, flexi_mut, update_key_translation};
use crate::config::generic_flexi::{self as cfg, GENERIC_FLEXI_NAME, NUM_ACTIONS};
use crate::config::{Properties, Scope, SettingId};
use crate::framework::ctx::Ctx;

const TAG_TYPE: &str = "TYPE";
const TAG_NUMBER: &str = "NUMBER";
const TAG_MIDI_CHANNEL: &str = "MIDI_CHANNEL";
const TAG_RESOLUTION: &str = "RESOLUTION";
const TAG_KNOB_MODE: &str = "KNOB_MODE";
const TAG_FUNCTION_LAYER: &str = "FUNCTION_LAYER";
const TAG_SEND_VALUE: &str = "SEND_VALUE";
const TAG_SEND_VALUE_WHEN_RECEIVED: &str = "SEND_VALUE_WHEN_RECEIVED";
const TAG_COMMAND: &str = "COMMAND";
const TAG_ASSIGNABLE_FUNCTION_ACTIONS: &str = "ASSIGNABLE_FUNCTION_ACTIONS";
const TAG_KEYBOARD_INPUT_NAME: &str = "KEYBOARD_INPUT_NAME";
const TAG_KEYBOARD_CHANNEL: &str = "KEYBOARD_CHANNEL";
const TAG_KEYBOARD_ROUTE_TIMBRE: &str = "KEYBOARD_ROUTE_TIMBRE";
const TAG_KEYBOARD_ROUTE_MODULATION: &str = "KEYBOARD_ROUTE_MODULATION";
const TAG_KEYBOARD_ROUTE_EXPRESSION: &str = "KEYBOARD_ROUTE_EXPRESSION";
const TAG_KEYBOARD_ROUTE_SUSTAIN: &str = "KEYBOARD_ROUTE_SUSTAIN";
const TAG_KEYBOARD_ROUTE_PROGRAMCHANGE: &str = "KEYBOARD_ROUTE_PROGRAMCHANGE";
const TAG_KEYBOARD_ROUTE_PITCHBEND: &str = "KEYBOARD_ROUTE_PITCHBEND";
const TAG_ENABLE_MPE: &str = "ENABLE_MPE";
const TAG_MPE_PITCHBEND_RANGE: &str = "MPE_PITCHBEND_RANGE";

/// The delay before the properties of an imported file are applied (former 1000 ms task of
/// `importFrom`).
pub const APPLY_DELAY_MS: u64 = 1000;

/// The keyboard settings and their tags in the mapping file, with the default if the tag is
/// missing (the defaults of `applyProperties`, not of the settings).
const KEYBOARD_TAGS: [(SettingId, &str, &str); 8] = [
    (cfg::KEYBOARD_INPUT_NAME, TAG_KEYBOARD_INPUT_NAME, GENERIC_FLEXI_NAME),
    (cfg::KEYBOARD_CHANNEL, TAG_KEYBOARD_CHANNEL, "1"),
    (cfg::KEYBOARD_ROUTE_TIMBRE, TAG_KEYBOARD_ROUTE_TIMBRE, "Off"),
    (cfg::KEYBOARD_ROUTE_MODULATION, TAG_KEYBOARD_ROUTE_MODULATION, "Off"),
    (cfg::KEYBOARD_ROUTE_EXPRESSION, TAG_KEYBOARD_ROUTE_EXPRESSION, "Off"),
    (cfg::KEYBOARD_ROUTE_SUSTAIN, TAG_KEYBOARD_ROUTE_SUSTAIN, "Off"),
    (cfg::KEYBOARD_ROUTE_PROGRAM_CHANGE, TAG_KEYBOARD_ROUTE_PROGRAMCHANGE, "Off"),
    (cfg::KEYBOARD_ROUTE_PITCHBEND, TAG_KEYBOARD_ROUTE_PITCHBEND, "Off"),
];

/// Format the mapping file (former `exportTo`).
pub fn export_text(ctx: &Ctx) -> String {
    let mut props = Properties::new();
    for (i, slot) in flexi(ctx).slots.iter().enumerate() {
        let slot_name = format!("SLOT{i}_");
        props.put(&format!("{slot_name}{TAG_TYPE}"), &slot.slot_type.to_string());
        props.put(&format!("{slot_name}{TAG_NUMBER}"), &slot.number.to_string());
        props.put(&format!("{slot_name}{TAG_MIDI_CHANNEL}"), &slot.midi_channel.to_string());
        props.put_bool(&format!("{slot_name}{TAG_RESOLUTION}"), slot.is_high_res);
        props.put(&format!("{slot_name}{TAG_KNOB_MODE}"), &slot.knob_mode.ordinal().to_string());
        props.put(&format!("{slot_name}{TAG_FUNCTION_LAYER}"), &slot.function_layer.to_string());
        props.put(&format!("{slot_name}{TAG_COMMAND}"), slot.command.name());
        props.put_bool(&format!("{slot_name}{TAG_SEND_VALUE}"), slot.send_value);
        props.put_bool(&format!("{slot_name}{TAG_SEND_VALUE_WHEN_RECEIVED}"), slot.send_value_when_received);
    }

    for i in 0..NUM_ACTIONS {
        props.put(&format!("{TAG_ASSIGNABLE_FUNCTION_ACTIONS}{i}"), &cfg::assignable_action(&ctx.config, i));
    }

    for (id, tag, _) in KEYBOARD_TAGS {
        props.put(tag, &ctx.config.value(id).unwrap_or_default());
    }
    props.put(TAG_ENABLE_MPE, &ctx.config.value(crate::config::SettingId::ENABLED_MPE_ZONES).unwrap_or_default());
    props.put(TAG_MPE_PITCHBEND_RANGE, &ctx.config.mpe_pitch_bend_range().to_string());

    // Properties.store (writer, GENERIC_FLEXI_NAME): the comment, the date and the entries
    let text = props.to_text();
    format!("#{GENERIC_FLEXI_NAME}\n{}", text.strip_prefix("#\n").unwrap_or(&text))
}

/// Export the configuration to the given file (former `exportTo`).
pub fn export_to(ctx: &Ctx, path: &Path) -> std::io::Result<()> {
    std::fs::write(path, export_text(ctx))
}

/// Store all settings in a file (former `saveFile`, without the file dialog). The file name gets
/// the ending ".properties" if it does not have it and is stored in the "Filename" setting.
pub fn save_file(ctx: &mut Ctx, filename: &str) {
    let mut filename = filename.to_string();
    // Ensure to end with .properties
    if !filename.ends_with(".properties") {
        filename.push_str(".properties");
    }
    ctx.config.set_value(cfg::FILENAME, &filename);
    match export_to(ctx, Path::new(&filename)) {
        Ok(()) => ctx.host.show_notification(&format!("Exported to: {filename}")),
        Err(error) => ctx.host.show_notification(&format!("Error writing file: {error}")),
    }
}

/// Load the mapping from a file and show the result (former `loadAndSelectFile`, without the
/// file dialog). The file name is stored in the "Filename" setting.
pub fn load_and_select_file(ctx: &mut Ctx, filename: &str) {
    ctx.config.set_value(cfg::FILENAME, filename);
    let message = load_file(ctx, filename);
    ctx.host.show_notification(&message);
}

/// Load all settings from a file (former `loadFile`). Returns an error or success text.
pub fn load_file(ctx: &mut Ctx, filename: &str) -> String {
    if filename.trim().is_empty() {
        return "No file name set.".to_string();
    }
    let path = Path::new(filename);
    if !path.exists() {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        return format!("The entered file does not exist: {}", absolute.display());
    }
    import_from(ctx, path);
    update_key_translation(ctx);
    format!("Imported from: {filename}")
}

/// Import the configuration from the given file (former `importFrom`). The values are applied
/// after one second.
pub fn import_from(ctx: &mut Ctx, path: &Path) {
    ctx.config.set_value(cfg::SLOT_SELECTED, "1");
    let props = match std::fs::read(path) {
        Ok(bytes) => Properties::parse(&String::from_utf8_lossy(&bytes)),
        Err(error) => {
            ctx.host.error(&format!("Could not import from file. {error}"));
            ctx.host.show_notification("Could not import from file. Check Script Console for detailed error.");
            return;
        }
    };
    ctx.schedule(APPLY_DELAY_MS, move |ctx| apply_properties(ctx, &props));
}

/// The (Java) Integer.parseInt of a property.
fn parse_int(value: Option<&str>, default_value: i32) -> Result<i32, String> {
    match value {
        None => Ok(default_value),
        Some(value) => value.parse::<i32>().map_err(|_| format!("For input string: \"{value}\"")),
    }
}

/// Apply the properties of a mapping file (former `applyProperties`).
pub fn apply_properties(ctx: &mut Ctx, props: &Properties) {
    if let Err(error) = apply_slots(ctx, props) {
        ctx.host.error(&format!("Could not import from file. {error}"));
        ctx.host.show_notification("Could not import from file. Check Script Console for detailed error.");
        return;
    }

    for i in 0..NUM_ACTIONS {
        let action = props.get(&format!("{TAG_ASSIGNABLE_FUNCTION_ACTIONS}{i}")).unwrap_or("").to_string();
        ctx.config.set_value(cfg::action_setting(i), &action);
    }
    for (id, tag, default_value) in KEYBOARD_TAGS {
        let value = props.get(tag).unwrap_or(default_value).to_string();
        ctx.config.set_value(id, &value);
    }
    let mpe = props.get(TAG_ENABLE_MPE).unwrap_or("Off").to_string();
    ctx.config.set_value(SettingId::ENABLED_MPE_ZONES, &mpe);
    match parse_int(props.get(TAG_MPE_PITCHBEND_RANGE), 48) {
        Ok(range) => ctx.config.set_mpe_pitchbend_range(range),
        Err(error) => {
            ctx.host.error(&format!("Could not import from file. {error}"));
            ctx.host.show_notification("Could not import from file. Check Script Console for detailed error.");
            return;
        }
    }

    clear_note_map(ctx);
    select_slot(ctx, "1");
}

/// Apply the slots of a mapping file. Stops at the first invalid number (as Java, the slots
/// before are already changed).
fn apply_slots(ctx: &mut Ctx, props: &Properties) -> Result<(), String> {
    for i in 0..cfg::NUM_SLOTS {
        let slot_name = format!("SLOT{i}_");
        let prop = |tag: &str| props.get(&format!("{slot_name}{tag}"));
        let Some(type_property) = prop(TAG_TYPE) else {
            continue;
        };
        let mut slot_type = parse_int(Some(type_property), 0)?;
        let command = FlexiCommand::lookup_by_name(prop(TAG_COMMAND).unwrap_or(""));

        // For backwards compatibility
        if command == FlexiCommand::Off {
            slot_type = TYPE_OFF;
        }

        let number = parse_int(prop(TAG_NUMBER), 0)?;
        let midi_channel = parse_int(prop(TAG_MIDI_CHANNEL), 0)?;
        let function_layer = parse_int(prop(TAG_FUNCTION_LAYER), 0)?;
        let is_true = |tag: &str| prop(tag).is_some_and(|v| v.eq_ignore_ascii_case("true"));
        let (is_high_res, send_value, send_value_when_received) = (is_true(TAG_RESOLUTION), is_true(TAG_SEND_VALUE), is_true(TAG_SEND_VALUE_WHEN_RECEIVED));
        let knob_mode = KnobMode::from_property(prop(TAG_KNOB_MODE));

        let slot = &mut flexi_mut(ctx).slots[i];
        slot.slot_type = slot_type;
        slot.number = number;
        slot.midi_channel = midi_channel;
        slot.is_high_res = is_high_res;
        slot.knob_mode = knob_mode;
        slot.function_layer = function_layer;
        slot.command = command;
        slot.send_value = send_value;
        slot.send_value_when_received = send_value_when_received;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------
// Program banks

/// The label of the program bank settings when no program is selected.
pub const PROGRAM_NONE: &str = "None";
/// The first ID of the program bank settings (one per bank).
pub const PROGRAM_BANK_FIRST: SettingId = SettingId::NEXT_SETTING_ID.offset(200);

/// The name of a file without its ending (former `FileEx.getNameWithoutType`).
fn name_without_type(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    match name.rfind('.') {
        Some(pos) => name[..pos].to_string(),
        None => name,
    }
}

/// Gets the file with program names, if present. This is the stored properties file name with
/// the ending "programs" (former `getProgramsFile`).
pub fn programs_file(ctx: &Ctx) -> Option<PathBuf> {
    let filename = cfg::filename(&ctx.config);
    if filename.trim().is_empty() {
        return None;
    }
    let file = Path::new(&filename);
    let programs_file = file.parent().unwrap_or(Path::new("")).join(format!("{}.programs", name_without_type(file)));
    let exists = programs_file.exists();
    let absolute = std::path::absolute(&programs_file).unwrap_or_else(|_| programs_file.clone());
    ctx.host.println(&format!("Scanning for: {} ({})", absolute.display(), if exists { "present" } else { "not present" }));
    exists.then_some(programs_file)
}

/// Load the program name file and create a document setting for each bank, if present (part
/// of the former `init`).
pub fn load_program_banks(ctx: &mut Ctx) {
    let Some(programs_file) = programs_file(ctx) else {
        return;
    };
    let text = match std::fs::read(&programs_file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(error) => {
            ctx.host.error(&format!("Could not load programs file. {error}"));
            return;
        }
    };
    let banks = match ProgramBank::parse(&text) {
        Ok(banks) => banks,
        Err(error) => {
            ctx.host.error(&format!("Could not parse programs file. {error}"));
            return;
        }
    };

    let category = format!("{} Program Banks", name_without_type(&programs_file));
    let mut ids = Vec::new();
    for (i, bank) in banks.iter().enumerate() {
        let id = PROGRAM_BANK_FIRST.offset(i as u16);
        let mut options = vec![PROGRAM_NONE.to_string()];
        options.extend(bank.programs.iter().cloned());
        ctx.config.activate_enum_setting(id, Scope::Document, &category, &bank.name, options, PROGRAM_NONE, false);
        ids.push(id);
    }
    let state = flexi_mut(ctx);
    state.banks = banks;
    state.bank_settings = ids;
}

/// Handle the change of a program bank setting: send the program change and reset the other
/// banks on the same MIDI channel. Returns false if the setting is not a program bank setting.
pub fn handle_bank_setting_change(ctx: &mut Ctx, id: SettingId) -> bool {
    let Some(bank_pos) = flexi(ctx).bank_settings.iter().position(|bank_id| *bank_id == id) else {
        return false;
    };
    let value = ctx.config.value(id).unwrap_or_default();
    let bank = flexi(ctx).banks[bank_pos].clone();
    let program = bank.lookup_program(&value);
    if program < 0 {
        return true;
    }
    ctx.surface.output.send_program_change(bank.channel, bank.msb, bank.lsb, program);

    let others: Vec<SettingId> = {
        let state = flexi(ctx);
        state.banks.iter().zip(&state.bank_settings).enumerate().filter(|(b, (other, _))| *b != bank_pos && other.channel == bank.channel).map(|(_, (_, id))| *id).collect()
    };
    for other in others {
        ctx.config.set_value(other, PROGRAM_NONE);
    }
    true
}
