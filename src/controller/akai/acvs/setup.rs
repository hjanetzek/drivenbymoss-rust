// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Akai MPC/Force with the ACVS protocol (former `ACVSControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The second MIDI input of the definition ("Pads (Remote MIDI Port)", which forwards the
//!   notes of the pads to the DAW) is not opened: the Rust instances have one MIDI input. The
//!   port can be enabled as an ordinary MIDI input in Reaper instead.
//! - When the device reports a different device type, the instance requests a restart with
//!   [`Ctx::restart_requested`] (former `IHost.restart`).
//! - The display does not show notifications (as in Java, where `notifyOnDisplay` is empty).
//! - `layoutControls` is not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::colors::{create_color_manager, BUTTON_UNDO_STATE_HI, BUTTON_UNDO_STATE_ON};
use super::mode::{crossfade_a_mode, crossfade_b_mode, ControlMode, ControlView};
use super::surface::{self as acvs, *};
use crate::backend::Sender;
use crate::config::{acvs as config, Configuration};
use crate::controller::akai::{add_button_without_light, bool_light, state_light};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::application::{PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_MIX};
use crate::daw::bank::BankItem;
use crate::daw::constants::LaunchQuantization;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::command::{shared, ContinuousCommand, FnContinuous, FnTrigger, SharedTrigger};
use crate::framework::commands::application::{DuplicateCommand, UndoCommand};
use crate::framework::commands::channel::PlayPositionCommand;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::misc::{OverdubCommand, PanelLayoutCommand};
use crate::framework::commands::mode::{ButtonRowModeCommand, ModeSelectCommand};
use crate::framework::commands::transport::PlayCommand;
use crate::framework::commands::transport_ext::{AutomationCommand, StopCommand, TapTempoCommand};
use crate::framework::commands::transport_more::{LoadCommand, LoopLengthCommand, LoopPositionCommand, RecordCommand, RestartCommand, SaveCommand};
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::display::{LineWriter, TextDisplay};
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::mode::track::{TrackMuteMode, TrackRecArmMode, TrackSoloMode, TrackStopClipMode};
use crate::framework::parameterprovider::ParamRef;
use crate::framework::setup_helpers;
use crate::framework::surface::{ContinuousKind, LightSupplier, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};
use crate::scale::Scales;

/// A text display without lines, which swallows the notifications (the MPC/Force cannot show
/// them, former empty `notifyOnDisplay`).
struct NoNotifications;

impl LineWriter for NoNotifications {
    fn write_line(&mut self, _output: &MidiOutput, _row: usize, _text: &str, _previous_text: Option<&str>) {}
}

/// Create an ACVS instance.
pub fn create(host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let colors = create_color_manager();
    let value_changer = ValueChanger::twos_complement(128, 1);
    let scales = Scales::new(&value_changer, 36, 52, 4, 4);

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.set_has_full_flat_track_list(true);
    model_setup.set_num_sends(4);
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface (former createInput (null): the default note input filters)
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &[])?;
    let surface = Surface::new(output.clone(), input, None);

    let device = config::active_device(&configuration);
    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    let track_modes = ctx.add_mode_manager();
    ctx.ext = Box::new(AcvsState::new(device, track_modes));
    ctx.text_display = Some(TextDisplay::new(output, Box::new(NoNotifications), 0, 1, 16));
    ctx.sysex_handler = Some(Rc::new(acvs::handle_sysex));
    // All MIDI is bound to controls
    ctx.midi_handler = Some(Rc::new(|_: &mut Ctx, _, _, _| {}));

    Ok(InstanceParts { ctx, setup: Box::new(AcvsSetup) })
}

/// The ACVS controller setup.
pub struct AcvsSetup;

impl ControllerSetup for AcvsSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx);
        ctx.register_view(Views::Control, ControlView::new());
        register_trigger_commands(ctx);
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        ctx.set_active_view(Views::Control);
        ctx.set_active_mode(Modes::User);
        let track_modes = acvs::track_modes(ctx);
        ctx.set_active_mode_in(track_modes, Modes::StopClip);
        ctx.schedule(2000, |ctx| {
            ctx.host.println("Disconnected.");
            acvs::send_ping(ctx);
        });
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
        acvs::flush_display(ctx);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for id in ctx.config.take_changed_settings() {
            setup_helpers::handle_deactivated_items_setting(ctx, id);
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        acvs::shutdown(ctx);
        ctx.turn_off_lights();
    }
}

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    let mode = ControlMode::new(ctx);
    ctx.register_mode(Modes::User, mode);
    let track_modes = acvs::track_modes(ctx);
    let mode = TrackMuteMode::new(ctx);
    ctx.register_mode_in(track_modes, Modes::Mute, mode);
    let mode = TrackSoloMode::new(ctx);
    ctx.register_mode_in(track_modes, Modes::Solo, mode);
    let mode = TrackRecArmMode::new(ctx);
    ctx.register_mode_in(track_modes, Modes::RecArm, mode);
    let mode = TrackStopClipMode::new(ctx);
    ctx.register_mode_in(track_modes, Modes::StopClip, mode);
    ctx.register_mode_in(track_modes, Modes::CrossfadeModeA, crossfade_a_mode());
    ctx.register_mode_in(track_modes, Modes::CrossfadeModeB, crossfade_b_mode());
}

/// Add a button without a light, executed on press (and release if `is_up_down`) (former
/// `addReceiveButton` with a `ButtonExec`).
fn add_receive_exec(ctx: &mut Ctx, id: ButtonId, label: &str, exec: impl Fn(&mut Ctx) + 'static, channel: i32, control: i32, is_up_down: bool) {
    let command = shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
        if event == ButtonEvent::Down || is_up_down && event == ButtonEvent::Up {
            exec(ctx);
        }
    }));
    add_button_without_light(ctx, id, label, command, BindType::Note, channel, control);
}

/// Add a button without a light (former `addReceiveButton` with a command).
fn add_receive_command(ctx: &mut Ctx, id: ButtonId, label: &str, command: impl FnMut(&mut Ctx, ButtonEvent, i32) + 'static, channel: i32, control: i32) {
    add_button_without_light(ctx, id, label, shared(FnTrigger(command)), BindType::Note, channel, control);
}

/// Add a button with a light (former `addButton`); without a supplier the light shows if the
/// button is pressed (BUTTON_STATE_ON / BUTTON_STATE_HI).
fn add_button(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, control: i32, supplier: Option<LightSupplier>) {
    let supplier = supplier.unwrap_or_else(|| bool_light(move |ctx| ctx.surface.is_pressed(id)));
    ctx.add_button(id, label, Some(command), BindType::Note, 0x0C, control, Some(supplier));
}

/// A command which does nothing (former `NopCommand`).
fn nop() -> SharedTrigger {
    shared(FnTrigger(|_: &mut Ctx, _, _| {}))
}

/// Former `handleClip`.
fn handle_clip(ctx: &mut Ctx, track_index: usize, slot_index: usize, event: ButtonEvent) {
    if event == ButtonEvent::Long {
        return;
    }
    if !ctx.model.track_bank().item(track_index).slot_bank().item(slot_index).does_exist() {
        return;
    }
    let is_shift = ctx.surface.is_shift_pressed();
    fn slot(ctx: &mut Ctx, track_index: usize, slot_index: usize) -> &mut crate::daw::slot::Slot {
        ctx.model.track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index)
    }
    if event == ButtonEvent::Down {
        slot(ctx, track_index, slot_index).select();
        if ctx.surface.is_select_pressed() {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::F2) {
            slot(ctx, track_index, slot_index).remove();
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Duplicate) {
            ctx.surface.set_trigger_consumed(ButtonId::Duplicate);
            slot(ctx, track_index, slot_index).duplicate();
            return;
        }
    }
    slot(ctx, track_index, slot_index).launch(event == ButtonEvent::Down, is_shift);
}

/// Former `handleScene`.
fn handle_scene(ctx: &mut Ctx, scene_index: usize, event: ButtonEvent) {
    if event == ButtonEvent::Long {
        return;
    }
    let is_pressed = event == ButtonEvent::Down;
    if is_pressed {
        if ctx.surface.is_pressed(ButtonId::F2) {
            ctx.model.scene_bank_mut().item_mut(scene_index).remove();
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Duplicate) {
            ctx.surface.set_trigger_consumed(ButtonId::Duplicate);
            ctx.model.scene_bank_mut().item_mut(scene_index).duplicate();
            return;
        }
        ctx.model.scene_bank_mut().item_mut(scene_index).select();
        if ctx.surface.is_select_pressed() {
            return;
        }
    }
    let is_shift = ctx.surface.is_shift_pressed();
    ctx.model.scene_bank_mut().item_mut(scene_index).launch(is_pressed, is_shift);
}

/// A pad of the MPC/Force which launches clips or scenes (the pads of both devices).
fn handle_pad(ctx: &mut Ctx, note_index: usize, event: ButtonEvent) {
    let track_index = note_index % 8;
    let clip_index = note_index / 8;
    // The MPC seems to send notes until the pad is released (looks like a bug to me). Quick &
    // dirty workaround to block them...
    let note_blocker = &mut acvs::state_mut(ctx).note_blocker;
    if event == ButtonEvent::Down {
        if note_blocker[note_index] {
            return;
        }
        note_blocker[note_index] = true;
    }
    if event == ButtonEvent::Up {
        note_blocker[note_index] = false;
    }
    if config::is_launch_clips(&ctx.config) {
        handle_clip(ctx, track_index, clip_index, event);
        return;
    }
    if clip_index < 2 {
        handle_scene(ctx, clip_index * 4 + track_index, event);
    }
}

/// Register the commands of all buttons (former `registerTriggerCommands`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    for i in 0..8 {
        add_receive_exec(
            ctx,
            ButtonId::get(ButtonId::RowSelect1, i),
            &format!("Track Select {}", i + 1),
            move |ctx| ctx.model.track_bank_mut().item_mut(i).select_or_expand_group(),
            0,
            NOTE_TRACK1_SELECT + i as i32,
            false,
        );
        add_receive_exec(
            ctx,
            ButtonId::get(ButtonId::Row11, i),
            &format!("Track Stop {}", i + 1),
            move |ctx| {
                let is_shift = ctx.surface.is_shift_pressed();
                ctx.model.track_bank_mut().item_mut(i).stop(is_shift);
            },
            0,
            NOTE_TRACK1_STOP + i as i32,
            false,
        );
        for j in 0..8 {
            let pos = i + j * 8;
            add_receive_command(
                ctx,
                ButtonId::get(ButtonId::Pad20, pos),
                &format!("Pad {}", pos + 1),
                move |ctx, event, _velocity| handle_clip(ctx, i, j, event),
                0,
                NOTE_CLIP1_LAUNCH + pos as i32,
            );
        }
        // Scene launch
        add_receive_command(ctx, ButtonId::get(ButtonId::Scene1, i), &format!("Scene {}", i + 1), move |ctx, event, _| handle_scene(ctx, i, event), 0, NOTE_SCENE1 + i as i32);
        let channel = i as i32 + 1;
        // Track solo
        add_receive_exec(ctx, ButtonId::get(ButtonId::Row21, i), &format!("Solo {}", i + 1), move |ctx| ctx.model.track_bank_mut().item_mut(i).toggle_solo(), channel, NOTE_SOLO, false);
        // Track mute
        add_receive_exec(ctx, ButtonId::get(ButtonId::Row31, i), &format!("Mute {}", i + 1), move |ctx| ctx.model.track_bank_mut().item_mut(i).toggle_mute(), channel, NOTE_MUTE, false);
        // NOTE_CUE - not supported
        // Track crossfade mode: the model has no crossfade parameter of the tracks (Reaper does
        // not support it), the values are ignored
        add_receive_command(ctx, ButtonId::get(ButtonId::Row41, i), &format!("Crossfader {}", i + 1), |_ctx, _event, _velocity| {}, channel, NOTE_CROSSFADER);
        // Track Record Arm
        add_receive_exec(
            ctx,
            ButtonId::get(ButtonId::Row51, i),
            &format!("Rec Arm {}", i + 1),
            move |ctx| ctx.model.track_bank_mut().item_mut(i).toggle_rec_arm(),
            channel,
            NOTE_REC_ARM,
            false,
        );
    }

    add_receive_exec(ctx, ButtonId::DeviceOnOff, "Device on/off", |ctx| ctx.model.cursor_device_mut().toggle_enabled_state(), 0x09, NOTE_TOGGLE_DEVICE, true);
    add_receive_exec(ctx, ButtonId::DeviceLeft, "Device prev", |ctx| ctx.model.cursor_device_mut().select_previous(), 0x09, NOTE_PREV_DEVICE, false);
    add_receive_exec(ctx, ButtonId::DeviceRight, "Device next", |ctx| ctx.model.cursor_device_mut().select_next(), 0x09, NOTE_NEXT_DEVICE, false);
    add_receive_exec(
        ctx,
        ButtonId::BankLeft,
        "Bank prev",
        |ctx| {
            if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                bank.select_previous_item();
            }
        },
        0x09,
        NOTE_PREV_BANK,
        false,
    );
    add_receive_exec(
        ctx,
        ButtonId::BankRight,
        "Bank next",
        |ctx| {
            if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                bank.select_next_item();
            }
        },
        0x09,
        NOTE_NEXT_BANK,
        false,
    );

    register_mpc_touch_display_trigger_commands(ctx);
    if acvs::is_force(ctx) {
        register_force_trigger_commands(ctx);
    } else {
        register_mpc_trigger_commands(ctx);
    }
}

/// Former `convertLaunchQuantization (int)`.
fn launch_quantization_from_value(value: i32) -> LaunchQuantization {
    match value {
        11..=13 => LaunchQuantization::Res1_16,
        9 | 10 => LaunchQuantization::Res1_8,
        7 | 8 => LaunchQuantization::Res1_4,
        5 | 6 => LaunchQuantization::Res1_2,
        4 => LaunchQuantization::Res1,
        3 => LaunchQuantization::Res2,
        2 => LaunchQuantization::Res4,
        1 => LaunchQuantization::Res8,
        _ => LaunchQuantization::ResNone,
    }
}

/// The buttons of the touch display (former `registerMPCTouchDisplayTriggerCommands`).
fn register_mpc_touch_display_trigger_commands(ctx: &mut Ctx) {
    add_receive_exec(ctx, ButtonId::Metronome, "Metronome", |ctx| ctx.model.transport_mut().toggle_metronome(), 0x0A, NOTE_METRONOME, true);
    // NOTE_MPC_CAPTURE_MIDI - not supported
    // NOTE_MPC_ABLETON_LINK - not supported
    add_receive_exec(
        ctx,
        ButtonId::F4,
        "Overdub",
        |ctx| {
            if ctx.surface.is_shift_pressed() {
                ctx.model.toggle_launcher_overdub();
            } else {
                ctx.model.toggle_overdub();
            }
        },
        0x0A,
        NOTE_ARRANGE_OVERDUB,
        true,
    );
    add_receive_exec(
        ctx,
        ButtonId::AutomationWrite,
        "Automation",
        |ctx| {
            if ctx.surface.is_shift_pressed() {
                ctx.model.transport_mut().toggle_write_clip_launcher_automation();
            } else {
                ctx.model.transport_mut().toggle_write_arranger_automation();
            }
        },
        0x0A,
        NOTE_ARRANGER_AUTOMATION_ARM,
        true,
    );
    add_receive_exec(ctx, ButtonId::Loop, "Loop", |ctx| ctx.model.transport_mut().toggle_loop(), 0x0A, NOTE_LOOP_SWITCH, true);
    add_receive_command(
        ctx,
        ButtonId::LaunchQuantization,
        "Launch Quantize",
        |ctx, event, velocity| {
            if event == ButtonEvent::Down || event == ButtonEvent::Up {
                ctx.model.transport_mut().set_default_launch_quantization(launch_quantization_from_value(velocity));
            }
        },
        0x0A,
        NOTE_LAUNCH_QUANTIZE,
    );
    add_receive_command(
        ctx,
        ButtonId::LayoutArrange,
        "Arrange / Session",
        |ctx, event, velocity| {
            if event == ButtonEvent::Down {
                ctx.model.application_mut().set_panel_layout(if velocity == 0 { PANEL_LAYOUT_ARRANGE } else { PANEL_LAYOUT_MIX });
            }
        },
        0x0A,
        NOTE_TOGGLE_ARRANGE_SESSION,
    );
    add_receive_exec(ctx, ButtonId::Follow, "Follow", |ctx| ctx.model.arranger_mut().toggle_playback_follow(), 0x0A, NOTE_FOLLOW, true);
    // NOTE_MPC_CLIP_DEV_VIEW - from where is this triggered?
    add_receive_exec(ctx, ButtonId::PinDevice, "Pin Device", |ctx| ctx.model.cursor_device_mut().toggle_pinned(), 0x0A, NOTE_DEVICE_LOCK, true);
    // NOTE_MPC_DETAILED_VIEW - from where is this triggered?
    add_receive_exec(
        ctx,
        ButtonId::NudgeMinus,
        "Nudge Down",
        |ctx| {
            let tempo = ctx.model.transport().tempo();
            ctx.model.transport_mut().set_tempo(tempo - 1.0);
        },
        0x0A,
        NOTE_NUDGE_DOWN,
        false,
    );
    add_receive_exec(
        ctx,
        ButtonId::NudgePlus,
        "Nudge Up",
        |ctx| {
            let tempo = ctx.model.transport().tempo();
            ctx.model.transport_mut().set_tempo(tempo + 1.0);
        },
        0x0A,
        NOTE_NUDGE_UP,
        false,
    );
    add_receive_exec(
        ctx,
        ButtonId::Delete,
        "Delete",
        |ctx| {
            let Some(track) = ctx.model.track_bank().selected_index() else {
                return;
            };
            let Some(slot) = ctx.model.track_bank().item(track).slot_bank().selected_item().map(|s| s.index()) else {
                return;
            };
            ctx.model.track_bank_mut().item_mut(track).slot_bank_mut().item_mut(slot.max(0) as usize).remove();
        },
        0x0A,
        NOTE_DELETE,
        true,
    );
    add_receive_command(
        ctx,
        ButtonId::F3,
        "QUANTIZE INTERVAL",
        |ctx, event, velocity| {
            if event != ButtonEvent::Long {
                ctx.config.set_quantize_amount((10 - velocity) * 10);
            }
        },
        0x0A,
        NOTE_QUANTIZE_INTERVAL,
    );
    add_receive_exec(
        ctx,
        ButtonId::Quantize,
        "Quantize",
        |ctx| {
            let amount = ctx.config.quantize_amount() as f64 / 100.0;
            if let Some(clip) = ctx.model.cursor_clip() {
                clip.quantize(amount);
            }
        },
        0x0A,
        NOTE_QUANTIZE,
        true,
    );
    // NOTE_MPC_DOUBLE, NOTE_MPC_NEW, NOTE_MPC_BACK_TO_ARRANGEMENT - from where are these triggered?
    add_receive_exec(
        ctx,
        ButtonId::StopAllClips,
        "Stop all clips",
        |ctx| {
            let is_shift = ctx.surface.is_shift_pressed();
            ctx.model.track_bank_mut().stop(is_shift);
        },
        0x0A,
        NOTE_STOP_ALL_CLIPS,
        false,
    );
    add_receive_exec(
        ctx,
        ButtonId::InsertScene,
        "Insert scene",
        |ctx| {
            let length = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
            ctx.model.project_mut().create_scene(length);
        },
        0x0A,
        NOTE_INSERT_SCENE,
        false,
    );
    let new_command = NewCommand::new();
    add_receive_exec(ctx, ButtonId::New, "REC (New)", move |ctx| new_command.execute_now(ctx), 0x0A, NOTE_ARRANGE_RECORD, false);
    // NOTE_MPC_TOGGLE_CLIP_SCENE_LAUNCH - from where is this triggered?
}

/// The undo light: off if nothing can be undone (redone with Shift), hi if pressed.
fn undo_light() -> LightSupplier {
    state_light(&[BUTTON_STATE_OFF, BUTTON_UNDO_STATE_ON, BUTTON_UNDO_STATE_HI], |ctx| {
        let application = ctx.model.application();
        let can = if ctx.surface.is_shift_pressed() { application.can_redo() } else { application.can_undo() };
        if !can {
            return 0;
        }
        if ctx.surface.is_pressed(ButtonId::Undo) {
            2
        } else {
            1
        }
    })
}

/// The buttons of the MPC (former `registerMPCTriggerCommands`).
fn register_mpc_trigger_commands(ctx: &mut Ctx) {
    type Function = fn(&mut Ctx);
    let arrows: [(ButtonId, &str, i32, Function); 4] = [
        (ButtonId::ArrowDown, "Bank A", NOTE_MPC_CURSOR_DOWN, |ctx| ctx.model.scene_bank_mut().select_next_page()),
        (ButtonId::ArrowUp, "Bank B", NOTE_MPC_CURSOR_UP, |ctx| ctx.model.scene_bank_mut().select_previous_page()),
        (ButtonId::ArrowLeft, "Bank C", NOTE_MPC_CURSOR_LEFT, |ctx| ctx.model.track_bank_mut().select_previous_page()),
        (ButtonId::ArrowRight, "Bank D", NOTE_MPC_CURSOR_RIGHT, |ctx| ctx.model.track_bank_mut().select_next_page()),
    ];
    for (id, label, control, function) in arrows {
        let command = shared(FnTrigger(move |ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                function(ctx);
            }
        }));
        add_button(ctx, id, label, command, control, None);
    }

    add_button(ctx, ButtonId::Repeat, "NOTE REPEAT", shared(PanelLayoutCommand::new()), NOTE_MPC_NOTE_REPEAT, Some(bool_light(|ctx| ctx.model.cursor_device().is_window_open())));
    add_button(
        ctx,
        ButtonId::Automation,
        "FULL LEVEL",
        shared(AutomationCommand::new()),
        NOTE_MPC_FULL_LEVEL,
        Some(state_light(&[BUTTON_STATE_OFF, BUTTON_UNDO_STATE_ON, BUTTON_UNDO_STATE_HI], |ctx| {
            let transport = ctx.model.transport();
            if ctx.surface.is_shift_pressed() {
                return i32::from(transport.is_writing_clip_launcher_automation());
            }
            if transport.is_writing_arranger_automation() {
                2
            } else {
                0
            }
        })),
    );
    add_button(
        ctx,
        ButtonId::Clip,
        "16 LEVEL",
        shared(FnTrigger(|ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                let launch_clips = !config::is_launch_clips(&ctx.config);
                config::set_launch_clips_or_scenes(&mut ctx.config, launch_clips);
                ctx.host.show_notification(&format!("Launch {}", if launch_clips { "Clips" } else { "Scenes" }));
            }
        })),
        NOTE_MPC_16_LEVEL,
        Some(state_light(&[BUTTON_STATE_ON, BUTTON_UNDO_STATE_ON], |ctx| i32::from(!config::is_launch_clips(&ctx.config)))),
    );
    add_button(ctx, ButtonId::F2, "ERASE", nop(), NOTE_MPC_ERASE, None);
    add_button(ctx, ButtonId::Shift, "SHIFT", shift_command(), NOTE_MPC_SHIFT, None);
    // NOTE_MPC_MAIN - blocked by hardware, not sent
    add_button(ctx, ButtonId::Undo, "Undo", shared(UndoCommand::new()), NOTE_MPC_UNDO, Some(undo_light()));
    add_button(ctx, ButtonId::Duplicate, "COPY", shared(DuplicateCommand::new()), NOTE_MPC_COPY, None);
    add_button(
        ctx,
        ButtonId::TapTempo,
        "Tap Tempo",
        shared(TapTempoCommand::new()),
        NOTE_MPC_TAP,
        Some(state_light(&[BUTTON_UNDO_STATE_ON, BUTTON_UNDO_STATE_HI], |ctx| i32::from(ctx.surface.is_pressed(ButtonId::TapTempo)))),
    );
    add_button(ctx, ButtonId::Record, "REC", shared(RecordCommand::new()), NOTE_MPC_REC, Some(bool_light(|ctx| ctx.model.transport().is_recording())));
    add_button(
        ctx,
        ButtonId::Overdub,
        "OVERDUB",
        shared(OverdubCommand::new()),
        NOTE_MPC_OVERDUB,
        Some(bool_light(|ctx| if ctx.surface.is_shift_pressed() { ctx.model.is_launcher_overdub() } else { ctx.model.is_arranger_overdub() })),
    );
    add_button(ctx, ButtonId::Stop, "STOP", shared(StopCommand::new()), NOTE_MPC_STOP, Some(bool_light(|ctx| !ctx.model.transport().is_playing())));
    add_button(ctx, ButtonId::Play, "PLAY", shared(PlayCommand::new()), NOTE_MPC_PLAY, Some(bool_light(|ctx| ctx.model.transport().is_playing())));
    add_button(ctx, ButtonId::ReturnToZero, "PLAY START", shared(RestartCommand), NOTE_MPC_PLAY_START, None);

    for i in 0..64 {
        add_button_without_light(
            ctx,
            ButtonId::get(ButtonId::MorePads1, i),
            &format!("PAD {}", i + 1),
            shared(FnTrigger(move |ctx: &mut Ctx, event, _| handle_pad(ctx, i, event))),
            BindType::Note,
            0x0C,
            NOTE_MPC_LAUNCH_CLIP_OR_SCENE1 + i as i32,
        );
    }
}

/// Former `ShiftCommand`: the knobs are slow while Shift is pressed.
fn shift_command() -> SharedTrigger {
    shared(FnTrigger(|ctx: &mut Ctx, _event, _velocity| {
        let is_shift = ctx.surface.is_shift_pressed();
        ctx.surface.set_knob_sensitivity_is_slow(is_shift);
    }))
}

/// The buttons of the Force (former `registerForceTriggerCommands`).
fn register_force_trigger_commands(ctx: &mut Ctx) {
    let track_modes = acvs::track_modes(ctx);
    for i in 0..8 {
        // Select one of the current eight tracks. Sets the Global Launch Quantization to the
        // value shown beneath the corresponding button while Shift is held.
        add_button_without_light(
            ctx,
            ButtonId::get(ButtonId::TrackSelect1, i),
            &format!("TRACK SELECT {}", i + 1),
            shared(FnTrigger(move |ctx: &mut Ctx, event, _| {
                if event != ButtonEvent::Down {
                    return;
                }
                if ctx.surface.is_shift_pressed() {
                    let launch_quantization = LaunchQuantization::ALL[i];
                    ctx.model.transport_mut().set_default_launch_quantization(launch_quantization);
                    ctx.host.show_notification(&format!("Launch Quantization: {}", launch_quantization.name()));
                    return;
                }
                if ctx.surface.is_pressed(ButtonId::F2) {
                    ctx.model.track_bank_mut().item_mut(i).remove();
                } else if ctx.surface.is_pressed(ButtonId::Duplicate) {
                    ctx.surface.set_trigger_consumed(ButtonId::Duplicate);
                    ctx.model.track_bank_mut().item_mut(i).duplicate();
                } else {
                    ctx.model.track_bank_mut().item_mut(i).select_or_expand_group();
                }
            })),
            BindType::Note,
            0x0C,
            NOTE_FORCE_TRACK_SELECT1 + i as i32,
        );

        // Performs the selected action (Mute, Solo, Record Arm or Clip Stop) for the current
        // eight tracks. With Shift: quantize, new clip, duplicate content and metronome.
        let mut row_mode_command = ButtonRowModeCommand::with_manager(track_modes, 0, i);
        add_button_without_light(
            ctx,
            ButtonId::get(ButtonId::TrackAssign1, i),
            &format!("TRACK ASSIGN {}", i + 1),
            shared(FnTrigger(move |ctx: &mut Ctx, event, velocity| {
                if ctx.surface.is_shift_pressed() {
                    if event != ButtonEvent::Down {
                        return;
                    }
                    match i {
                        0 => {
                            let amount = ctx.config.quantize_amount() as f64 / 100.0;
                            if let Some(clip) = ctx.model.cursor_clip() {
                                clip.quantize(amount);
                            }
                        }
                        1 => NewCommand::new().execute_now(ctx),
                        2 => {
                            if let Some(clip) = ctx.model.cursor_clip() {
                                clip.duplicate_content();
                            }
                        }
                        4 => ctx.model.transport_mut().toggle_metronome(),
                        _ => {}
                    }
                    return;
                }
                row_mode_command.execute(ctx, event, velocity);
            })),
            BindType::Note,
            0x0C,
            NOTE_FORCE_TRACK_ASSIGN1 + i as i32,
        );
    }

    for i in 0..64 {
        add_button_without_light(
            ctx,
            ButtonId::get(ButtonId::MorePads1, i),
            &format!("PAD {}", i + 1),
            shared(FnTrigger(move |ctx: &mut Ctx, event, _| handle_pad(ctx, i, event))),
            BindType::Note,
            0x0C,
            NOTE_FORCE_LAUNCH_CLIP_OR_SCENE1 + i as i32,
        );
    }

    for i in 0..8 {
        add_button_without_light(
            ctx,
            ButtonId::get(ButtonId::Row61, i),
            &format!("SCENE {}", i + 1),
            shared(FnTrigger(move |ctx: &mut Ctx, event, _| handle_scene(ctx, i, event))),
            BindType::Note,
            0x0C,
            NOTE_FORCE_LAUNCH_SCENE1 + i as i32,
        );
    }

    add_button(ctx, ButtonId::Mastertrack, "MASTER", shared(AcvsMasterCommand::default()), NOTE_FORCE_MASTER, Some(bool_light(|ctx| ctx.model.master_track().is_selected())));
    add_button(
        ctx,
        ButtonId::F5,
        "STOP ALL CLIPS",
        shared(FnTrigger(|ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                let is_shift = ctx.surface.is_shift_pressed();
                ctx.model.track_bank_mut().stop(is_shift);
            }
        })),
        NOTE_FORCE_STOP_ALL_CLIPS,
        None,
    );
    // The following buttons do currently nothing but could be used...
    add_button(ctx, ButtonId::Session, "LAUNCH", nop(), NOTE_FORCE_LAUNCH, None);
    add_button(ctx, ButtonId::Note, "NOTE", nop(), NOTE_FORCE_NOTE, None);
    add_button(ctx, ButtonId::Sequencer, "SEQUENCER", nop(), NOTE_FORCE_STEP_SEQ, None);
    add_button(ctx, ButtonId::Select, "SELECT", nop(), NOTE_FORCE_CLIP_SELECT, None);
    add_button(ctx, ButtonId::NoteEditor, "EDIT", nop(), NOTE_FORCE_EDIT, None);
    add_button(ctx, ButtonId::Duplicate, "COPY", shared(DuplicateCommand::new()), NOTE_FORCE_COPY, None);
    add_button(ctx, ButtonId::F2, "ERASE", nop(), NOTE_FORCE_DELETE, None);
    add_button(ctx, ButtonId::Repeat, "ARP", nop(), NOTE_FORCE_ARP, None);
    add_button(
        ctx,
        ButtonId::TapTempo,
        "Tap Tempo",
        shared(TapTempoCommand::new()),
        NOTE_FORCE_TAP_TEMPO,
        Some(state_light(&[BUTTON_UNDO_STATE_ON, BUTTON_UNDO_STATE_HI], |ctx| i32::from(ctx.surface.is_pressed(ButtonId::TapTempo)))),
    );
    let track_mode_buttons = [
        (ButtonId::Mute, "MUTE", Modes::Mute, NOTE_FORCE_MUTE),
        (ButtonId::Solo, "SOLO", Modes::Solo, NOTE_FORCE_SOLO),
        (ButtonId::RecArm, "REC ARM", Modes::RecArm, NOTE_FORCE_REC_ARM),
        (ButtonId::StopClip, "STOP CLIP", Modes::StopClip, NOTE_FORCE_CLIP_STOP),
    ];
    for (id, label, mode, control) in track_mode_buttons {
        add_button(
            ctx,
            id,
            label,
            shared(ModeSelectCommand::with_options(track_modes, mode, false, true, false)),
            control,
            Some(bool_light(move |ctx| ctx.mode_manager(track_modes).is_active(mode))),
        );
    }
    add_button(ctx, ButtonId::Play, "PLAY", shared(PlayCommand::new()), NOTE_FORCE_PLAY, Some(bool_light(|ctx| ctx.model.transport().is_playing())));
    add_button(ctx, ButtonId::Stop, "STOP", shared(StopCommand::new()), NOTE_FORCE_STOP, Some(bool_light(|ctx| !ctx.model.transport().is_playing())));
    add_button(ctx, ButtonId::Record, "REC", shared(RecordCommand::new()), NOTE_FORCE_REC, Some(bool_light(|ctx| ctx.model.transport().is_recording())));
    add_button(ctx, ButtonId::Undo, "UNDO", shared(UndoCommand::new()), NOTE_FORCE_UNDO, Some(undo_light()));
    add_button(ctx, ButtonId::Load, "LOAD", shared(LoadCommand), NOTE_FORCE_LOAD, None);
    add_button(ctx, ButtonId::Save, "SAVE", shared(SaveCommand), NOTE_FORCE_SAVE, None);
    // NOTE_FORCE_MATRIX, NOTE_FORCE_CLIP, NOTE_FORCE_MIXER - not sent controlled by hardware
    add_button(ctx, ButtonId::Browse, "NAVIGATE", nop(), NOTE_FORCE_NAVIGATE, None);
    add_button(ctx, ButtonId::Shift, "SHIFT", shift_command(), NOTE_FORCE_SHIFT, None);

    let cursors = [
        (ButtonId::ArrowUp, "Up", Direction::Up, NOTE_FORCE_CURSOR_UP),
        (ButtonId::ArrowDown, "Down", Direction::Down, NOTE_FORCE_CURSOR_DOWN),
        (ButtonId::ArrowLeft, "Left", Direction::Left, NOTE_FORCE_CURSOR_LEFT),
        (ButtonId::ArrowRight, "Right", Direction::Right, NOTE_FORCE_CURSOR_RIGHT),
    ];
    for (id, label, direction, control) in cursors {
        let command = Rc::new(RefCell::new(CursorCommand::new(direction, false)));
        let light_command = command.clone();
        let supplier = state_light(&[BUTTON_STATE_OFF, BUTTON_UNDO_STATE_ON], move |ctx| match light_command.try_borrow() {
            Ok(command) => i32::from(command.can_scroll(ctx)),
            Err(_) => 0,
        });
        add_button(ctx, id, label, command, control, Some(supplier));
    }

    let crossfades = [(ButtonId::CrossfadeA, "Crossfade A", Modes::CrossfadeModeA, NOTE_FORCE_ASSIGN_A), (ButtonId::CrossfadeB, "Crossfade B", Modes::CrossfadeModeB, NOTE_FORCE_ASSIGN_B)];
    for (id, label, mode, control) in crossfades {
        add_button(
            ctx,
            id,
            label,
            shared(FnTrigger(move |ctx: &mut Ctx, event, _| {
                if event == ButtonEvent::Down {
                    ctx.set_active_mode_in(track_modes, mode);
                } else if event == ButtonEvent::Up && ctx.mode_manager(track_modes).is_active(mode) {
                    ctx.restore_mode_in(track_modes);
                }
            })),
            control,
            None,
        );
    }
}

/// Toggles between the master track and the last selected track (former `ACVSMasterCommand`).
#[derive(Default)]
pub struct AcvsMasterCommand {
    position: i32,
}

impl TriggerCommand for AcvsMasterCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.model.master_track().is_selected() {
            ctx.model.track_bank_mut().select_item_at_position(self.position.max(0) as usize);
        } else {
            self.position = ctx.model.track_bank().selected_item().map_or(0, |t| t.position());
            ctx.model.master_track_mut().select();
        }
    }
}

/// Add a continuous control with a command.
fn add_continuous(ctx: &mut Ctx, id: ContinuousId, label: &str, kind: ContinuousKind, command: impl ContinuousCommand + 'static, channel: i32, control: i32) {
    ctx.add_continuous(id, label, kind, Rc::new(RefCell::new(command)), BindType::Cc, channel, control);
}

/// Register the faders and knobs (former `registerContinuousCommands`).
pub fn register_continuous_commands(ctx: &mut Ctx) {
    for i in 0..8 {
        let channel = i as i32 + 1;
        add_continuous(
            ctx,
            ContinuousId::get(ContinuousId::Fader1, i),
            &format!("Volume {}", i + 1),
            ContinuousKind::Fader,
            FnContinuous(move |ctx: &mut Ctx, value| ctx.model.track_bank_mut().item_mut(i).set_volume(value)),
            channel,
            CC_VOLUME,
        );
        add_continuous(
            ctx,
            ContinuousId::get(ContinuousId::Knob1, i),
            &format!("Pan {}", i + 1),
            ContinuousKind::AbsoluteKnob,
            FnContinuous(move |ctx: &mut Ctx, value| ctx.model.track_bank_mut().item_mut(i).set_pan(value)),
            channel,
            CC_PAN,
        );
        let sends = [ContinuousId::Send1Knob1, ContinuousId::Send2Knob1, ContinuousId::Send3Knob1, ContinuousId::Send4Knob1];
        for (send, first) in sends.iter().enumerate() {
            add_continuous(
                ctx,
                ContinuousId::get(*first, i),
                &format!("Send {} - {}", send + 1, i + 1),
                ContinuousKind::AbsoluteKnob,
                FnContinuous(move |ctx: &mut Ctx, value| ctx.model.track_bank_mut().item_mut(i).send_bank_mut().item_mut(send).set_value(value)),
                channel,
                CC_SEND1_LEVEL + send as i32,
            );
        }
        add_continuous(
            ctx,
            ContinuousId::get(ContinuousId::DeviceKnob1, i),
            &format!("Device Knob {}", i + 1),
            ContinuousKind::AbsoluteKnob,
            FnContinuous(move |ctx: &mut Ctx, value| {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    bank.item_mut(i).set_value(value);
                }
            }),
            0x09,
            CC_PARAM1_VALUE + i as i32,
        );

        // Indications
        let track = ctx.model.track_bank_mut().item_mut(i);
        track.set_volume_indication(true);
        track.set_pan_indication(true);
        let send_bank = track.send_bank_mut();
        for send in 0..4.min(send_bank.page_size()) {
            send_bank.item_mut(send).set_indication(true);
        }
        if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            if i < bank.page_size() {
                bank.item_mut(i).set_indication(true);
            }
        }
    }

    add_continuous(ctx, ContinuousId::PlayPosition, "Position", ContinuousKind::RelativeKnob, PlayPositionCommand::new(), 0x0A, CC_PLAY_POSITION);
    add_continuous(ctx, ContinuousId::MoveLoop, "Loop Start", ContinuousKind::RelativeKnob, LoopPositionCommand, 0x0A, CC_MOVE_LOOP);
    add_continuous(ctx, ContinuousId::LoopLength, "Loop Length", ContinuousKind::RelativeKnob, LoopLengthCommand, 0x0A, CC_LOOP_LENGTH);

    for i in 0..8 {
        let volume_knob = ContinuousId::get(ContinuousId::VolumeKnob1, i);
        add_continuous(
            ctx,
            volume_knob,
            &format!("Volume {}", i + 1),
            ContinuousKind::RelativeKnob,
            FnContinuous(move |ctx: &mut Ctx, value| ctx.model.track_bank_mut().item_mut(i).change_volume(value)),
            0x0D,
            i as i32,
        );
        bind_touch(ctx, volume_knob, i as i32, move |ctx, is_touched| {
            let parameter = ParamRef::TrackVolume { index: i };
            parameter.touch_value(ctx, is_touched);
            if is_touched && ctx.surface.is_pressed(ButtonId::F2) {
                parameter.reset_value(ctx);
            }
        });

        let param_knob = ContinuousId::get(ContinuousId::ParamKnob1, i);
        add_continuous(
            ctx,
            param_knob,
            &format!("Param {}", i + 1),
            ContinuousKind::RelativeKnob,
            FnContinuous(move |ctx: &mut Ctx, value| {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    bank.item_mut(i).change_value(value);
                }
            }),
            0x0D,
            8 + i as i32,
        );
        bind_touch(ctx, param_knob, 8 + i as i32, move |ctx, is_touched| {
            let parameter = ParamRef::BankItem { bank: crate::daw::events::ParameterBankId::CursorDevice, index: i };
            parameter.touch_value(ctx, is_touched);
            if is_touched && ctx.surface.is_pressed(ButtonId::F2) {
                parameter.reset_value(ctx);
            }
        });
    }

    ctx.surface.create_continuous(ContinuousId::Crossfader, "Crossfader", ContinuousKind::Fader);
    ctx.surface.bind_continuous(ContinuousId::Crossfader, BindType::Cc, 0x0D, 16);
    ctx.surface.bind_continuous_parameter(ContinuousId::Crossfader, Some(ParamRef::Crossfader));
}

/// Bind the touch of a knob to a note on MIDI channel 14 (former `bindTouch`).
fn bind_touch(ctx: &mut Ctx, id: ContinuousId, note: i32, touch: impl Fn(&mut Ctx, bool) + 'static) {
    let command = shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
        if event != ButtonEvent::Long {
            touch(ctx, event == ButtonEvent::Down);
        }
    }));
    if let Some(control) = ctx.surface.continuous_mut(id) {
        control.touch_command = Some(command);
    }
    ctx.surface.input.bind_touch(id, BindType::Note, 0x0D, note);
}
