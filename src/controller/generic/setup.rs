// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of Generic Flexi (former `GenericFlexiControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`]; the browser observer
//!   checks the active state of the browser on every call.
//! - The settings are notified at startup like Java's `notifyAllObservers`: the slot change,
//!   the selected mode and the MPE settings.
//! - The slot editor settings loaded from the configuration file are not applied to the first
//!   slot (in Java their observers are called when the settings are created and apply them to
//!   the selected slot; the mapping file overwrites all slots anyway).
//! - MPE: as in the Java Reaper version, enabling MPE on the note input only stores the state;
//!   the MPE configuration messages are sent to the device.

use std::rc::Rc;

use super::command::FlexiCommand;
use super::editor;
use super::mapping;
use super::surface::{self as fs, flexi, flexi_mut, FlexiState};
use crate::backend::Sender;
use crate::config::generic_flexi::{self as cfg};
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::constants::DeviceId;
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::ColorManager;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::browser::BrowserMode;
use crate::framework::mode::device::ParameterMode;
use crate::framework::mode::track::{TrackMode, TrackPanMode, TrackSendMode, TrackVolumeMode};
use crate::framework::surface::Surface;
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{MidiInput, MidiOutput};
use crate::scale::Scales;

/// The delay of the MPE configuration after a change of the MPE settings.
const MPE_DELAY_MS: u64 = 2000;
/// The delay of loading the mapping file at startup.
const LOAD_DELAY_MS: u64 = 2000;

/// MPE zone 1 (status of the CC messages).
const ZONE_1: i32 = 0xB0;
/// MPE zone 2.
const ZONE_2: i32 = 0xBF;
const REGISTERED_PARAMETER_NUMBER_MSB: i32 = 0x65;
const REGISTERED_PARAMETER_NUMBER_LSB: i32 = 0x64;
const DATA_ENTRY_MPE_MSB: i32 = 0x06;
const DATA_ENTRY_MPE_LSB: i32 = 0x26;

/// The modes in the order of the mode selection (former `ModesHandler.MODE_IDS`).
pub const MODE_IDS: [Modes; 12] = [
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
];

/// The MIDI filters of the keyboard note input (former `getMidiFilters`).
pub fn midi_filters(config: &Configuration) -> Vec<String> {
    let is_mpe_enabled = config.is_mpe_enabled();
    let keyboard_channel = cfg::keyboard_channel(config);

    // Keyboard is off?
    if keyboard_channel < 0 && !is_mpe_enabled {
        return Vec::new();
    }

    let midi_channel = if is_mpe_enabled || keyboard_channel >= 16 { "?".to_string() } else { format!("{keyboard_channel:X}") };
    let mut filters: Vec<String> = ["8", "9", "A", "D"].iter().map(|status| format!("{status}{midi_channel}????")).collect();
    if cfg::is_keyboard_route_modulation(config) {
        filters.push(format!("B{midi_channel}01??"));
    }
    if cfg::is_keyboard_route_expression(config) {
        filters.push(format!("B{midi_channel}0B??"));
    }
    if cfg::is_keyboard_route_sustain(config) {
        filters.push(format!("B{midi_channel}40??"));
    }
    if cfg::is_keyboard_route_timbre(config) {
        filters.push(format!("B{midi_channel}4A??"));
    }
    if cfg::is_keyboard_route_program_change(config) {
        filters.push(format!("C{midi_channel}????"));
    }
    if cfg::is_keyboard_route_pitchbend(config) || is_mpe_enabled {
        filters.push(format!("E{midi_channel}????"));
    }
    filters
}

/// Create a Generic Flexi instance.
pub fn create(host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let value_changer = ValueChanger::twos_complement(16384, 100);

    // createScales
    let mut scales = Scales::new(&value_changer, 0, 128, 128, 1);
    scales.set_chromatic(true);

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.wants_focused_parameter = true;
    model_setup.num_results = 1;
    model_setup.num_filter_column_entries = 1;
    model_setup.enable_main_drum_device(false);
    model_setup.num_markers = 8;
    model_setup.enable_device(DeviceId::Eq);
    model_setup.enable_device(DeviceId::FirstInstrument);
    model_setup.wants_clip_launcher_navigator = true;
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer.clone(), scales);
    model.track_bank_mut().set_indication(true);

    // createSurface: without a keyboard channel and MPE no note input, as the Java version on
    // Bitwig (the Java Reaper version creates the default note input with its default filters,
    // which passes all notes also with the keyboard off; the manual says the keyboard channel can
    // be turned off)
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let filters = midi_filters(&configuration);
    let input = if filters.is_empty() {
        MidiInput::without_note_input(sender.clone(), Some(ports.input))
    } else {
        let filters: Vec<&str> = filters.iter().map(String::as_str).collect();
        MidiInput::new(sender.clone(), Some(ports.input), &filters)?
    };
    let mut surface = Surface::new(output, input, None);
    // The virtual Shift button (see the surface module)
    surface.create_button(ButtonId::Shift, "Shift");

    let keyboard_channel = cfg::keyboard_channel(&configuration);
    // As the Bitwig version (createSurface): the note input only with MPE or a keyboard channel
    let has_note_input = configuration.is_mpe_enabled() || keyboard_channel >= 0;
    let mut ctx = Ctx::new(host, sender, model, configuration, surface, ColorManager::new());
    let mut state = FlexiState::new(keyboard_channel, value_changer);
    state.has_note_input = has_note_input;
    ctx.ext = Box::new(state);
    ctx.midi_handler = Some(Rc::new(fs::handle_midi));
    ctx.sysex_handler = Some(Rc::new(fs::handle_sysex));

    Ok(InstanceParts { ctx, setup: Box::new(GenericFlexiSetup) })
}

/// The Generic Flexi controller setup.
pub struct GenericFlexiSetup;

impl ControllerSetup for GenericFlexiSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        // The changes of loading the configuration: the values of the slot editor are not
        // applied to a slot
        let changes = ctx.config.take_changed_settings();
        for id in changes {
            if !is_slot_editor_setting(id) {
                ctx.config.mark_changed(id);
            }
        }

        create_modes(ctx);

        // Former createObservers
        ctx.modes.add_change_listener(Rc::new(|ctx: &mut Ctx, _, _| update_indication(ctx)));

        ctx.config.set_value(cfg::LEARN_TYPE, cfg::OPTIONS_TYPE[0]);
        let selected = ctx.config.value(cfg::SLOT_SELECTED).unwrap_or_default();
        editor::select_slot(ctx, &selected);

        // Former notifyAllObservers for the settings which are not observable
        ctx.config.mark_changed(cfg::SLOT_CHANGE);
        ctx.config.mark_changed(SettingId::ENABLED_MPE_ZONES);
        ctx.config.mark_changed(SettingId::MPE_PITCHBEND_RANGE);
        fs::update_relative_knob_sensitivity(ctx);

        // Load program name file and create selection list if present
        mapping::load_program_banks(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        fs::clear_note_map(ctx);

        // Load last configuration
        ctx.schedule(LOAD_DELAY_MS, |ctx| {
            let filename = cfg::filename(&ctx.config);
            let message = mapping::load_file(ctx, &filename);
            ctx.host.println(&message);
        });
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        fs::flush(ctx);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        for event in ctx.model.take_events() {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                handle_track_change(ctx, is_selected);
            }
        }

        // Former activateBrowserObserver
        let browser_active = ctx.model.browser().is_some_and(|browser| browser.is_active());
        if browser_active != flexi(ctx).browser_active {
            flexi_mut(ctx).browser_active = browser_active;
            if browser_active {
                ctx.set_temporary_mode(Modes::Browser);
            } else if ctx.modes.is_active(Modes::Browser) {
                ctx.restore_mode();
            }
        }

        for id in ctx.config.take_changed_settings() {
            handle_setting_change(ctx, id);
        }
    }

    fn exit(&mut self, _ctx: &mut Ctx) {}
}

fn is_slot_editor_setting(id: SettingId) -> bool {
    id == cfg::SLOT_SELECTED || (cfg::SLOT_TYPE.0..=cfg::SLOT_SEND_VALUE_WHEN_RECEIVED.0).contains(&id.0) || cfg::function_setting_category(id).is_some()
}

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    let mode = TrackMode::new(ctx, true);
    ctx.register_mode(Modes::Track, mode);
    let mode = TrackVolumeMode::new(ctx, true);
    ctx.register_mode(Modes::Volume, mode);
    let mode = TrackPanMode::new(ctx, true);
    ctx.register_mode(Modes::Pan, mode);
    for i in 0..8 {
        let mode = TrackSendMode::new(i, ctx, true);
        ctx.register_mode(Modes::get(Modes::Send1, i as usize), mode);
    }
    // The alternative function is Shift (the default, former `surface::isShiftPressed`)
    let mode = ParameterMode::with_name("Device", ctx, true, Vec::new());
    ctx.register_mode(Modes::DeviceParams, mode);
    ctx.register_mode(Modes::Browser, BrowserMode::new());
    ctx.modes.set_default_id(Modes::Volume);
}

/// Configure the MPE zones of the device after a change of the MPE setting.
fn schedule_mpe_configuration(ctx: &mut Ctx) {
    ctx.schedule(MPE_DELAY_MS, |ctx| {
        // Without the note input the Java version returns (getDefaultNoteInput () == null)
        if !flexi(ctx).has_note_input {
            return;
        }
        let mpe_enabled = ctx.config.is_mpe_enabled();
        // Enable MPE zone 1 with all 15 channels
        configure_mpe(ctx, ZONE_1, if mpe_enabled { 15 } else { 0 });
        // Disable MPE zone
        configure_mpe(ctx, ZONE_2, 0);
    });
}

/// React on a changed setting (former `createObservers` and the setting observers of the
/// configuration).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    // On every change of the type, also from the editor itself (the type observer of Java)
    if id == cfg::SLOT_TYPE {
        cfg::update_slot_visibility(&mut ctx.config);
    }
    if editor::handle_setting_change(ctx, id) || mapping::handle_bank_setting_change(ctx, id) {
        return;
    }
    match id {
        // The buttons (signal settings); without a file dialog Save and Load use the file of the
        // Filename setting
        cfg::BUTTON_SET => editor::learn_set(ctx),
        cfg::BUTTON_SAVE => {
            let filename = cfg::filename(&ctx.config);
            if filename.trim().is_empty() {
                ctx.host.show_notification("No file name set.");
            } else {
                mapping::save_file(ctx, &filename);
            }
        }
        cfg::BUTTON_LOAD => {
            let filename = cfg::filename(&ctx.config);
            mapping::load_and_select_file(ctx, &filename);
        }
        cfg::SLOT_CHANGE => fs::update_key_translation(ctx),
        cfg::SELECTED_MODE => select_mode(ctx),
        SettingId::ENABLED_MPE_ZONES => {
            cfg::update_mpe_dependent_settings(&mut ctx.config);
            schedule_mpe_configuration(ctx);
        }
        SettingId::MPE_PITCHBEND_RANGE => ctx.schedule(MPE_DELAY_MS, |ctx| {
            if !flexi(ctx).has_note_input {
                return;
            }
            let range = ctx.config.mpe_pitch_bend_range();
            send_mpe_pitchbend_range(ctx, ZONE_1, range);
        }),
        SettingId::EXCLUDE_DEACTIVATED_ITEMS => {
            let exclude = ctx.config.are_deactivated_items_excluded();
            ctx.model.track_bank_mut().set_skip_disabled_items(exclude);
        }
        SettingId::KNOB_SENSITIVITY_DEFAULT | SettingId::KNOB_SENSITIVITY_SLOW => fs::update_relative_knob_sensitivity(ctx),

        // Former createNoteRepeatObservers (Reaper supports length and mode but not octaves)
        SettingId::NOTEREPEAT_ACTIVE => {
            let active = ctx.config.is_note_repeat_active();
            ctx.surface.input.default_note_input_mut().note_repeat.set_active(active);
        }
        SettingId::NOTEREPEAT_PERIOD => {
            let period = ctx.config.note_repeat_period().value();
            ctx.surface.input.default_note_input_mut().note_repeat.set_period(period);
        }
        SettingId::NOTEREPEAT_LENGTH => {
            let length = ctx.config.note_repeat_length().value();
            ctx.surface.input.default_note_input_mut().note_repeat.set_note_length(length);
        }
        SettingId::NOTEREPEAT_MODE => {
            let mode = ctx.config.note_repeat_mode().and_then(|mode| ctx.config.arpeggiator_modes().iter().position(|m| *m == mode));
            if let Some(index) = mode {
                ctx.surface.input.default_note_input_mut().note_repeat.set_mode(index);
            }
        }
        _ => {}
    }
}

/// Configure an MPE zone (former `AbstractMidiOutput.configureMPE`).
fn configure_mpe(ctx: &Ctx, zone: i32, number_of_channels: i32) {
    send_rpn(ctx, zone, 0x00, 0x06, number_of_channels);
}

/// Send the pitchbend range of an MPE zone (former `AbstractMidiOutput.sendMPEPitchbendRange`).
fn send_mpe_pitchbend_range(ctx: &Ctx, zone: i32, range: i32) {
    send_rpn(ctx, zone, 0x00, 0x00, range);
}

fn send_rpn(ctx: &Ctx, zone: i32, msb: i32, lsb: i32, value: i32) {
    let output = &ctx.surface.output;
    output.send_midi_short(zone, REGISTERED_PARAMETER_NUMBER_MSB, msb);
    output.send_midi_short(zone, REGISTERED_PARAMETER_NUMBER_LSB, lsb);
    output.send_midi_short(zone, DATA_ENTRY_MPE_MSB, value);
    output.send_midi_short(zone, DATA_ENTRY_MPE_LSB, 0);
    output.send_midi_short(zone, REGISTERED_PARAMETER_NUMBER_MSB, 127);
    output.send_midi_short(zone, REGISTERED_PARAMETER_NUMBER_LSB, 127);
}

/// Former `handleTrackChange`.
fn handle_track_change(ctx: &mut Ctx, is_selected: bool) {
    if is_selected {
        update_indication(ctx);
    }
}

/// Activate the mode of the "Selected Mode" setting (former `selectMode`).
fn select_mode(ctx: &mut Ctx) {
    let selected_mode_name = cfg::selected_mode_name(&ctx.config);
    let mode_id = MODE_IDS.iter().copied().chain([Modes::Browser]).find(|id| {
        ctx.modes.get(*id).is_some_and(|mode| mode.try_borrow().is_ok_and(|mode| mode.name() == selected_mode_name))
    });
    if let Some(mode_id) = mode_id {
        fs::activate_mode(ctx, mode_id);
    }
}

/// Show the parameters which are mapped to a command in the DAW (former `updateIndication`).
pub fn update_indication(ctx: &mut Ctx) {
    let commands = flexi(ctx).mapped_commands();
    let has = |command: Option<FlexiCommand>| command.is_some_and(|command| commands.contains(&command));
    let at = |first: FlexiCommand, offset: usize| FlexiCommand::from_ordinal(first.ordinal() + offset);
    let modes = &ctx.modes;
    let is_mode_active = |mode: Modes| modes.is_active(mode);

    let track_bank = ctx.model.track_bank();
    let selected_index = track_bank.selected_item().map(|track| track.index());
    let page_size = track_bank.page_size();
    let mut track_indications = Vec::with_capacity(page_size);
    for i in 0..page_size {
        let has_track_sel = selected_index == Some(i as i32);

        // Former testVolumeIndication
        let volume = has_track_sel && has(Some(FlexiCommand::TrackSelectedSetVolumeTrack))
            || has(at(FlexiCommand::Track1SetVolume, i))
            || has(at(FlexiCommand::ModesKnob1, i)) && is_mode_active(Modes::Volume);

        // Former testPanIndication
        let pan = has_track_sel && has(Some(FlexiCommand::TrackSelectedSetPanning))
            || has(at(FlexiCommand::Track1SetPanning, i))
            || has(at(FlexiCommand::ModesKnob1, i)) && is_mode_active(Modes::Pan);

        // Former testSendIndication
        let send_page_size = track_bank.item(i).send_bank().page_size();
        let sends: Vec<bool> = (0..send_page_size)
            .map(|j| {
                if has_track_sel && has(at(FlexiCommand::TrackSelectedSetSend1, j)) || is_mode_active(Modes::Track) && j < 6 {
                    return true;
                }
                if has(at(FlexiCommand::Track1SetSend1, j * send_page_size + i)) {
                    return true;
                }
                is_mode_active(Modes::get(Modes::Send1, j))
            })
            .collect();
        track_indications.push((volume, pan, sends));
    }

    let master_volume = has(Some(FlexiCommand::MasterSetVolume));
    let master_pan = has(Some(FlexiCommand::MasterSetPanning));

    // Former testParameterIndication
    let parameter_page_size = ctx.model.cursor_device().parameter_bank().map_or(0, |bank| bank.page_size());
    let parameters: Vec<bool> = (0..parameter_page_size)
        .map(|i| has(at(FlexiCommand::DeviceSetParameter1, i)) || has(at(FlexiCommand::ModesKnob1, i)) && is_mode_active(Modes::DeviceParams))
        .collect();

    let track_bank = ctx.model.track_bank_mut();
    for (i, (volume, pan, sends)) in track_indications.into_iter().enumerate() {
        let track = track_bank.item_mut(i);
        track.set_volume_indication(volume);
        track.set_pan_indication(pan);
        let send_bank = track.send_bank_mut();
        for (j, indicate) in sends.into_iter().enumerate() {
            send_bank.item_mut(j).set_indication(indicate);
        }
    }
    let master = ctx.model.master_track_mut();
    master.set_volume_indication(master_volume);
    master.set_pan_indication(master_pan);
    if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
        for (i, indicate) in parameters.into_iter().enumerate() {
            bank.item_mut(i).set_indication(indicate);
        }
    }
}
