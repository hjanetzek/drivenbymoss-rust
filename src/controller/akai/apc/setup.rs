// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the APC40 Mk1 and Mk2 (former `APCControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The faders and device knobs are bound once by a track volume and a parameter mode which
//!   are not registered (as in Java); the bindings are stored in the surface.
//! - The Send + track select combination of the Mk2 uses the send selection command directly
//!   (Java casts the wrapped command and fails).
//! - `layoutControls` and the simulator colors of the buttons are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::colors::*;
use super::command::*;
use super::definition::ApcDefinition;
use super::mode::ApcMode;
use super::surface::{self as apc, *};
use super::view::*;
use crate::backend::Sender;
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::bank::BankItem;
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::command::{shared, ContinuousCommand, SharedTrigger};
use crate::framework::commands::application::UndoCommand;
use crate::framework::commands::channel::{CrossfadeModeCommand, MasterCommand, MuteCommand, RecArmCommand, RedoCommand, SoloCommand, StopAllClipsCommand};
use crate::framework::commands::cursor::Direction;
use crate::framework::commands::device_nav::{DeviceLayerLeftCommand, DeviceLayerRightCommand, SelectNextDeviceOrParamPageCommand, SelectPreviousDeviceOrParamPageCommand};
use crate::framework::commands::misc::{FootswitchCommand, PaneCommand, PanelLayoutCommand, Panels};
use crate::framework::commands::mode::{is_send_mode, KnobRowModeCommand, ModeMultiSelectCommand, ModeSelectCommand};
use crate::framework::commands::transport::{MetronomeCommand, PlayCommand};
use crate::framework::commands::transport_ext::StopCommand;
use crate::framework::commands::view::{ToggleShiftViewCommand, ViewButtonCommand};
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::mode::device::ParameterMode;
use crate::framework::mode::parameter_mode::sequential_controls;
use crate::framework::mode::track::TrackVolumeMode;
use crate::framework::parameterprovider::ParamRef;
use crate::framework::setup_helpers::{self, BrowserObserver};
use crate::framework::surface::{ContinuousKind, LightSupplier, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{BindType, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The drum matrix of the APC40 (former `APCScales.APC40_DRUM_MATRIX`).
#[rustfmt::skip]
const APC40_DRUM_MATRIX: [i32; 40] = [
     0,  1,  2,  3, -1, -1, -1, -1,
     4,  5,  6,  7, -1, -1, -1, -1,
     8,  9, 10, 11, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
];

/// Create the scales (former `APCScales` and `createScales`).
pub fn create_scales(value_changer: &ValueChanger) -> Scales {
    let mut scales = Scales::new(value_changer, 36, 76, 8, 5).with_grid_translator(Scales::offset_grid_translator(36, 76));
    scales.set_drum_note_end(76);
    scales.set_drum_matrix(APC40_DRUM_MATRIX.to_vec());
    scales.set_drum_default_offset(12);
    scales
}

/// Create an APC40 instance.
pub fn create(definition: ApcDefinition, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let is_mk2 = definition.is_mk2;
    let colors = create_color_manager(is_mk2);
    let value_changer = ValueChanger::twos_complement(128, 1);
    let scales = create_scales(&value_changer);

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.set_num_scenes(5);
    model_setup.set_num_drum_pad_layers(12);
    model_setup.set_num_markers(8);
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &["80????", "90????"])?;
    let surface = Surface::new(output, input, Some(create_pad_grid(&colors, is_mk2)));

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    ctx.ext = Box::new(ApcState::new(definition));
    // Set Mode 2
    ctx.surface.output.send_sysex(&definition.mode2_sysex());
    apc::create_pads(&mut ctx);

    Ok(InstanceParts { ctx, setup: Box::new(ApcSetup::default()) })
}

/// The APC40 controller setup.
#[derive(Default)]
pub struct ApcSetup {
    browser_observer: BrowserObserver,
}

impl ControllerSetup for ApcSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx);
        create_views(ctx);
        register_trigger_commands(ctx);
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        ctx.set_active_mode(Modes::Pan);
        let view = if ctx.config.should_start_with_session_view() { Views::Session } else { ctx.config.startup_view() };
        ctx.set_active_view(view);
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
        if ctx.active_view().is_none() {
            return;
        }
        // The LEDs of the device knobs
        for i in 0..8 {
            let (exists, value) = match ctx.model.cursor_device().parameter_bank() {
                Some(bank) => {
                    let item = bank.item(i);
                    (item.does_exist(), item.value())
                }
                None => (false, 0),
            };
            set_led(ctx, APC_KNOB_DEVICE_KNOB_LED_1 + i as i32, if exists { LED_MODE_SINGLE } else { LED_MODE_VOLUME });
            set_led(ctx, APC_KNOB_DEVICE_KNOB_1 + i as i32, if exists { value } else { 0 });
        }
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for event in &events {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                setup_helpers::handle_track_change(ctx, *is_selected);
            }
        }

        let changed = ctx.config.take_changed_settings();
        for id in &changed {
            handle_setting_change(ctx, *id);
        }
        ctx.dispatch_setting_changes_to_views(&changed);

        self.browser_observer.check_mode(ctx, Modes::Browser);
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        ctx.turn_off_lights();
    }
}

/// React on a changed setting (former `createObservers`).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    let _ = setup_helpers::handle_scale_setting(ctx, id) || setup_helpers::handle_note_repeat_setting(ctx, id) || setup_helpers::handle_deactivated_items_setting(ctx, id);
}

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    let mode = ApcMode::pan(ctx);
    ctx.register_mode(Modes::Pan, mode);
    for i in 0..8 {
        let mode = ApcMode::send(ctx, i);
        ctx.register_mode(Modes::get(Modes::Send1, i), mode);
    }
    ctx.register_mode(Modes::Note, ApcMode::note());
    let mode = ApcMode::user(ctx);
    ctx.register_mode(Modes::User, mode);
    ctx.register_mode(Modes::Browser, ApcMode::browser());
}

/// Register the views (former `createViews`).
pub fn create_views(ctx: &mut Ctx) {
    let is_mk2 = apc::is_mk2(ctx);
    ctx.register_view(Views::Play, ApcPlayView::new(is_mk2));
    ctx.register_view(Views::Session, ApcSessionView::new(is_mk2));
    ctx.register_view(Views::Sequencer, ApcSequencerView::new(is_mk2));
    ctx.register_view(Views::Drum, ApcDrumView::new(is_mk2));
    ctx.register_view(Views::Raindrops, ApcRaindropsView::new(is_mk2));
    ctx.register_view(Views::Shift, ApcShiftView::new());
    let (color1, color2) = if is_mk2 { (APC_MKII_COLOR_BLUE, APC_MKII_COLOR_WHITE) } else { (APC_COLOR_GREEN, APC_COLOR_YELLOW) };
    ctx.register_view(Views::Tempo, ApcTempoView::new(color1, color2, APC_COLOR_BLACK));
}

/// The bind type of a button (former `getTriggerBindType`).
fn trigger_bind_type(button: ButtonId) -> BindType {
    if button == ButtonId::Footswitch1 || button == ButtonId::Footswitch2 {
        BindType::Cc
    } else {
        BindType::Note
    }
}

/// A light which maps a state to a color ID (former `addButton` with color IDs): a negative
/// state is off.
fn state_light(color_ids: &'static [&'static str], state: impl Fn(&Ctx) -> i32 + 'static) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| {
        let state = state(ctx);
        let id = if state < 0 { BUTTON_STATE_OFF } else { color_ids[(state as usize).min(color_ids.len() - 1)] };
        ctx.colors.color_index(id)
    })
}

const OFF_ON: &[&str] = &[BUTTON_STATE_OFF, BUTTON_STATE_ON];

/// An on/off light (color IDs off and on).
fn on_off_light(is_on: impl Fn(&Ctx) -> bool + 'static) -> LightSupplier {
    state_light(OFF_ON, move |ctx| i32::from(is_on(ctx)))
}

/// The default light of a button without a supplier: on if not pressed, hi if pressed (former
/// `ButtonPressedSupplier` with `BUTTON_STATE_ON` and `BUTTON_STATE_HI`).
fn pressed_light(id: ButtonId) -> LightSupplier {
    state_light(&[BUTTON_STATE_ON, BUTTON_STATE_HI], move |ctx| i32::from(ctx.surface.is_pressed(id)))
}

/// Add a button whose command closes the shift view if Shift was used in combination with it
/// (former overridden `addButton`).
fn add_button(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, channel: i32, control: i32, supplier: Option<LightSupplier>) {
    let command = if id == ButtonId::Shift { command } else { shared(ShiftCloserCommand::new(command)) };
    let supplier = supplier.unwrap_or_else(|| pressed_light(id));
    ctx.add_button(id, label, Some(command), trigger_bind_type(id), channel, control, Some(supplier));
}

/// The state of the buttons of a track (former `getButtonState`).
fn button_state(ctx: &Ctx, index: usize, button: i32) -> bool {
    let tb = ctx.model.current_track_bank();
    let track = tb.item(index);
    let track_exists = track.does_exist();
    let is_shift = ctx.surface.is_shift_pressed();
    match button {
        APC_BUTTON_TRACK_SELECTION => {
            if is_shift {
                return index == ctx.config.new_clip_length();
            }
            let sel_index = tb.selected_item().map_or(-1, |t| t.index());
            // Handle user mode selection
            if apc::is_mk2(ctx) && ctx.surface.is_pressed(ButtonId::Send2) {
                return ctx.modes.get(Modes::User).is_some_and(|mode| {
                    mode.try_borrow().is_ok_and(|m| m.as_any().downcast_ref::<ApcMode>().is_some_and(|u| u.is_page_selected(ctx, index)))
                });
            }
            // Handle send mode selection
            if ctx.surface.is_pressed(ButtonId::Send1) {
                ctx.modes.is_active(Modes::get(Modes::Send1, index))
            } else {
                index as i32 == sel_index
            }
        }
        APC_BUTTON_SOLO => track_exists && if is_shift { track.is_auto_monitor() } else { track.is_solo() },
        APC_BUTTON_ACTIVATOR => track_exists && if is_shift { track.is_monitor() } else { !track.is_mute() },
        APC_BUTTON_RECORD_ARM => {
            if is_shift {
                return crossfade_button_color(ctx, index) > 0;
            }
            track_exists && track.is_rec_arm()
        }
        _ => false,
    }
}

/// The state of the crossfade button of a track: 0 = AB, 1 = A, 2 = B (former
/// `getCrossfadeButtonColor`).
fn crossfade_button_color(ctx: &Ctx, index: usize) -> i32 {
    if !ctx.model.current_track_bank().item(index).does_exist() {
        return 0;
    }
    match CrossfadeModeCommand::mode(ctx, index).as_str() {
        "AB" => 0,
        "A" => 1,
        _ => 2,
    }
}

/// Register the commands of all buttons (former `registerTriggerCommands`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    let is_mk2 = apc::is_mk2(ctx);

    add_button(ctx, ButtonId::Shift, "SHIFT", shared(ToggleShiftViewCommand::new()), 0, APC_BUTTON_SHIFT, None);
    add_button(ctx, ButtonId::Play, "PLAY", shared(PlayCommand::new()), 0, APC_BUTTON_PLAY, Some(on_off_light(|ctx| ctx.model.transport().is_playing())));
    add_button(
        ctx,
        ButtonId::Record,
        "RECORD",
        shared(ApcRecordCommand::default()),
        0,
        APC_BUTTON_RECORD,
        Some(on_off_light(|ctx| ctx.model.transport().is_recording())),
    );
    add_button(ctx, ButtonId::TapTempo, "TEMPO", shared(ApcTapTempoCommand::default()), 0, APC_BUTTON_TAP_TEMPO, None);
    add_button(
        ctx,
        ButtonId::Quantize,
        if is_mk2 { "DEV.LOCK" } else { "REC QUANTIZATION" },
        shared(ApcQuantizeCommand::default()),
        0,
        APC_BUTTON_REC_QUANT,
        Some(on_off_light(|ctx| ctx.surface.is_pressed(ButtonId::Quantize))),
    );
    add_button(
        ctx,
        ButtonId::Mastertrack,
        "MASTER",
        shared(MasterCommand::new()),
        0,
        APC_BUTTON_MASTER,
        Some(on_off_light(|ctx| ctx.model.master_track().is_selected())),
    );
    // Note: the stop-all-clips button has no LED
    add_button(
        ctx,
        ButtonId::StopAllClips,
        "STOP CLIPS",
        shared(StopAllClipsCommand::new()),
        0,
        APC_BUTTON_STOP_ALL_CLIPS,
        Some(on_off_light(|ctx| ctx.surface.is_pressed(ButtonId::StopAllClips))),
    );
    add_button(
        ctx,
        ButtonId::PanSend,
        "PAN",
        shared(ModeSelectCommand::new(Modes::Pan)),
        0,
        APC_BUTTON_PAN,
        Some(on_off_light(|ctx| ctx.modes.is_active(Modes::Pan))),
    );

    let mut send_command = None;
    if is_mk2 {
        let sends = [Modes::Send1, Modes::Send2, Modes::Send3, Modes::Send4, Modes::Send5, Modes::Send6, Modes::Send7, Modes::Send8];
        let command = Rc::new(RefCell::new(ModeMultiSelectCommand::new(&sends)));
        send_command = Some(command.clone());
        add_button(ctx, ButtonId::Send1, "SENDS", command, 0, APC_BUTTON_SEND_A, Some(on_off_light(|ctx| ctx.modes.active_id().is_some_and(is_send_mode))));
        add_button(
            ctx,
            ButtonId::Send2,
            "USER",
            shared(ApcUserModeCommand),
            0,
            APC_BUTTON_SEND_B,
            Some(on_off_light(|ctx| ctx.modes.is_active(Modes::User))),
        );
    } else {
        let sends = [
            (ButtonId::Send1, "SEND A", 0, APC_BUTTON_SEND_A, [Modes::Send1, Modes::Send4, Modes::Send7].to_vec()),
            (ButtonId::Send2, "SEND B", 1, APC_BUTTON_SEND_B, [Modes::Send2, Modes::Send5, Modes::Send8].to_vec()),
            (ButtonId::Send3, "SEND C", 2, APC_BUTTON_SEND_C, [Modes::Send3, Modes::Send6].to_vec()),
        ];
        for (id, label, send_index, control, modes) in sends {
            add_button(ctx, id, label, shared(SendModeCommand::new(send_index)), 0, control, Some(on_off_light(move |ctx| ctx.modes.is_any_active(&modes))));
        }
    }

    for i in 0..8 {
        let channel = i as i32;
        add_button(
            ctx,
            ButtonId::get(ButtonId::Row11, i),
            &format!("Select {}", i + 1),
            shared(SelectTrackSendOrClipLengthCommand::new(i, send_command.clone())),
            channel,
            APC_BUTTON_TRACK_SELECTION,
            Some(on_off_light(move |ctx| button_state(ctx, i, APC_BUTTON_TRACK_SELECTION))),
        );
        add_button(
            ctx,
            ButtonId::get(ButtonId::Row21, i),
            &format!("Solo {}", i + 1),
            shared(SoloCommand::new(i as i32)),
            channel,
            APC_BUTTON_SOLO,
            Some(on_off_light(move |ctx| button_state(ctx, i, APC_BUTTON_SOLO))),
        );
        add_button(
            ctx,
            ButtonId::get(ButtonId::Row31, i),
            &format!("{}{}", if is_mk2 { "Mute " } else { "Activator " }, i + 1),
            shared(MuteCommand::new(i as i32)),
            channel,
            APC_BUTTON_ACTIVATOR,
            Some(on_off_light(move |ctx| button_state(ctx, i, APC_BUTTON_ACTIVATOR))),
        );
        // The Mk1 has no crossfade buttons: Shift + Arm cycles the crossfade mode
        let rec_arm = RecArmCommand::with_shifted(i as i32, move |ctx, event| {
            if !is_mk2 && event == crate::midi::ButtonEvent::Down {
                CrossfadeModeCommand::cycle(ctx, i);
            }
        });
        add_button(
            ctx,
            ButtonId::get(ButtonId::Row41, i),
            &format!("Arm {}", i + 1),
            shared(rec_arm),
            channel,
            APC_BUTTON_RECORD_ARM,
            Some(on_off_light(move |ctx| button_state(ctx, i, APC_BUTTON_RECORD_ARM))),
        );
        if is_mk2 {
            add_button(
                ctx,
                ButtonId::get(ButtonId::Row51, i),
                &format!("X-fade {}", i + 1),
                shared(CrossfadeModeCommand::new(i)),
                channel,
                APC_BUTTON_A_B,
                Some(state_light(&[BUTTON_STATE_OFF, BUTTON_STATE_ON, BUTTON_STATE_BLINK], move |ctx| crossfade_button_color(ctx, i))),
            );
        }
        let stop_button = ButtonId::get(ButtonId::Row61, i);
        add_button(
            ctx,
            stop_button,
            &format!("Stop {}", i + 1),
            shared(ApcStopClipCommand::new(i)),
            channel,
            APC_BUTTON_CLIP_STOP,
            Some(state_light(OFF_ON, move |ctx| ApcStopClipCommand::button_color(ctx, i, stop_button))),
        );
    }

    if is_mk2 {
        add_button(
            ctx,
            ButtonId::DeviceLeft,
            "<- DEVICE",
            shared(DeviceLayerLeftCommand::new()),
            0,
            APC_BUTTON_CLIP_TRACK,
            Some(on_off_light(DeviceLayerLeftCommand::can_execute)),
        );
        add_button(
            ctx,
            ButtonId::DeviceRight,
            "DEVICE ->",
            shared(DeviceLayerRightCommand::new()),
            0,
            APC_BUTTON_DEVICE_ON_OFF,
            Some(on_off_light(DeviceLayerRightCommand::can_execute)),
        );
        add_button(
            ctx,
            ButtonId::Browse,
            "BANK",
            shared(ApcBrowserCommand),
            0,
            APC_BUTTON_BANK,
            Some(on_off_light(|ctx| ctx.model.browser().is_some_and(|b| b.is_active()))),
        );
    } else {
        add_button(ctx, ButtonId::Stop, "STOP", shared(StopCommand::new()), 0, APC_BUTTON_STOP, Some(on_off_light(|ctx| !ctx.model.transport().is_playing())));
    }

    add_button(
        ctx,
        ButtonId::Clip,
        if is_mk2 { "SESSION" } else { "MIDI OVERDUB" },
        shared(SessionRecordCommand),
        0,
        if is_mk2 { APC_BUTTON_SESSION } else { APC_BUTTON_MIDI_OVERDUB },
        Some(on_off_light(|ctx| ctx.model.is_launcher_overdub())),
    );
    add_button(
        ctx,
        ButtonId::Metronome,
        "METRONOME",
        shared(MetronomeCommand::new(false)),
        0,
        if is_mk2 { APC_BUTTON_SEND_C } else { APC_BUTTON_METRONOME },
        Some(on_off_light(|ctx| ctx.model.transport().is_metronome_on())),
    );
    add_button(ctx, ButtonId::NudgePlus, "NUDGE+", shared(RedoCommand::new()), 0, if is_mk2 { APC_BUTTON_NUDGE_MINUS } else { APC_BUTTON_NUDGE_PLUS }, None);
    add_button(ctx, ButtonId::NudgeMinus, "NUDGE-", shared(UndoCommand::new()), 0, if is_mk2 { APC_BUTTON_NUDGE_PLUS } else { APC_BUTTON_NUDGE_MINUS }, None);
    add_button(
        ctx,
        ButtonId::DeviceOnOff,
        "DEV. ON/OFF",
        shared(ApcDeviceOnOffCommand),
        0,
        if is_mk2 { APC_BUTTON_DETAIL_VIEW } else { APC_BUTTON_DEVICE_ON_OFF },
        Some(on_off_light(|ctx| ctx.model.cursor_device().is_enabled())),
    );
    add_button(
        ctx,
        ButtonId::ToggleDevicesPane,
        if is_mk2 { "CLIP/DEV.VIEW" } else { "CLIP/TRACK" },
        shared(PaneCommand::new(Panels::Device)),
        0,
        if is_mk2 { APC_BUTTON_MIDI_OVERDUB } else { APC_BUTTON_CLIP_TRACK },
        Some(on_off_light(|ctx| ctx.surface.is_pressed(ButtonId::ToggleDevicesPane))),
    );
    add_button(
        ctx,
        ButtonId::Layout,
        "DETAIL VIEW",
        shared(PanelLayoutCommand::new()),
        0,
        if is_mk2 { APC_BUTTON_METRONOME } else { APC_BUTTON_DETAIL_VIEW },
        Some(on_off_light(|ctx| !ctx.surface.is_shift_pressed() && ctx.model.cursor_device().is_window_open())),
    );
    add_button(
        ctx,
        ButtonId::BankLeft,
        "<- BANK",
        shared(SelectPreviousDeviceOrParamPageCommand::new()),
        0,
        APC_BUTTON_DEVICE_LEFT,
        Some(on_off_light(SelectPreviousDeviceOrParamPageCommand::can_execute)),
    );
    add_button(
        ctx,
        ButtonId::BankRight,
        "BANK ->",
        shared(SelectNextDeviceOrParamPageCommand::new()),
        0,
        APC_BUTTON_DEVICE_RIGHT,
        Some(on_off_light(SelectNextDeviceOrParamPageCommand::can_execute)),
    );

    let arrows = [
        (ButtonId::ArrowDown, "Arrow Down", Direction::Down, APC_BUTTON_DOWN),
        (ButtonId::ArrowUp, "Arrow Up", Direction::Up, APC_BUTTON_UP),
        (ButtonId::ArrowLeft, "Arrow Left", Direction::Left, APC_BUTTON_LEFT),
        (ButtonId::ArrowRight, "Arrow Right", Direction::Right, APC_BUTTON_RIGHT),
    ];
    for (id, label, direction, control) in arrows {
        add_button(ctx, id, label, shared(ApcCursorCommand::new(direction)), 0, control, None);
    }

    for i in 0..5 {
        let scene = ButtonId::get(ButtonId::Scene1, i);
        add_button(
            ctx,
            scene,
            &format!("Scene {}", i + 1),
            shared(ViewButtonCommand::new(scene)),
            0,
            APC_BUTTON_SCENE_LAUNCH_1 + i as i32,
            Some(Rc::new(move |ctx: &Ctx| ctx.button_color_from_active_view(scene))),
        );
    }

    add_button(ctx, ButtonId::Footswitch1, "Foot Controller 1", shared(FootswitchCommand::new(0)), 0, APC_FOOTSWITCH_1, None);
    if !is_mk2 {
        add_button(ctx, ButtonId::Footswitch2, "Foot Controller 2", shared(FootswitchCommand::new(1)), 0, APC_FOOTSWITCH_2, None);
    }
}

/// Add a fader or knob without a command (it is bound to a parameter).
fn add_bound_control(ctx: &mut Ctx, id: ContinuousId, label: &str, kind: ContinuousKind, channel: i32, control: i32) {
    ctx.surface.create_continuous(id, label, kind);
    ctx.surface.bind_continuous(id, BindType::Cc, channel, control);
}

/// Register the faders and knobs (former `registerContinuousCommands`).
pub fn register_continuous_commands(ctx: &mut Ctx) {
    add_bound_control(ctx, ContinuousId::FaderMaster, "Master", ContinuousKind::Fader, 0, APC_KNOB_MASTER_LEVEL);
    ctx.surface.bind_continuous_parameter(ContinuousId::FaderMaster, Some(ParamRef::MasterVolume));
    add_bound_control(ctx, ContinuousId::Crossfader, "Crossfader", ContinuousKind::Fader, 0, APC_KNOB_CROSSFADER);
    ctx.surface.bind_continuous_parameter(ContinuousId::Crossfader, Some(ParamRef::Crossfader));

    let play_position: Rc<RefCell<dyn ContinuousCommand>> = Rc::new(RefCell::new(ApcPlayPositionCommand::default()));
    ctx.add_continuous(ContinuousId::PlayPosition, "Play Position", ContinuousKind::RelativeKnob, play_position, BindType::Cc, 0, APC_KNOB_CUE_LEVEL);

    for i in 0..8 {
        add_bound_control(ctx, ContinuousId::get(ContinuousId::Fader1, i), &format!("Fader{}", i + 1), ContinuousKind::Fader, i as i32, APC_KNOB_TRACK_LEVEL);
        ctx.add_continuous(
            ContinuousId::get(ContinuousId::Knob1, i),
            &format!("Knob {}", i + 1),
            ContinuousKind::AbsoluteKnob,
            Rc::new(RefCell::new(KnobRowModeCommand::new(i))),
            BindType::Cc,
            0,
            APC_KNOB_TRACK_KNOB_1 + i as i32,
        );
        add_bound_control(ctx, ContinuousId::get(ContinuousId::DeviceKnob1, i), &format!("Device Knob {}", i + 1), ContinuousKind::AbsoluteKnob, 0, APC_KNOB_DEVICE_KNOB_1 + i as i32);
    }

    // The faders and the device knobs are always bound to the track volumes and the parameters
    // of the cursor device
    let mut volume_mode = TrackVolumeMode::new_with_controls(ctx, true, sequential_controls(ContinuousId::Fader1, 8));
    volume_mode.on_activate(ctx);
    let mut parameter_mode = ParameterMode::new(ctx, true, sequential_controls(ContinuousId::DeviceKnob1, 8));
    parameter_mode.on_activate(ctx);

    if apc::is_mk2(ctx) {
        let tempo: Rc<RefCell<dyn ContinuousCommand>> = Rc::new(RefCell::new(ApcTempoCommand::default()));
        ctx.add_continuous(ContinuousId::Tempo, "Tempo", ContinuousKind::RelativeKnob, tempo, BindType::Cc, 0, APC_KNOB_TEMPO);
    }
}
