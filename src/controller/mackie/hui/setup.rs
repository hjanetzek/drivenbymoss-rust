// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Mackie HUI protocol with up to 2 extenders (former `HUIControllerSetup`
//! and the commands of `hui.command.trigger`).
//!
//! Each device is a surface of the context with its own MIDI port pair (see
//! `doc/MULTI_SURFACE_PLAN.md`); the mode managers are connected and the button states are
//! shared, as in Java. The buttons are not bound to MIDI; the MIDI handler of the surface
//! triggers them by their zone and port (see `super::surface::handle_midi`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - On shutdown the display shows "Please start Reaper..." (Java: the name of the host).
//! - `layoutControls` (the simulator GUI) is not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::definition::*;
use super::display::{create_segment_display, create_text_display};
use super::mode::{HuiMode, HuiModeKind};
use super::surface::{handle_midi, hui, hui_mut, set_trigger, update_fader, update_vu_meters, HuiSurfaceState};
use crate::backend::Sender;
use crate::config::hui::{self as cfg, HuiConfig};
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::constants::{AutomationMode, DeviceId};
use crate::daw::events::{ModelEvent, ParameterBankId};
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::ColorManager;
use crate::framework::command::{shared, ContinuousCommand, FnTrigger, SharedTrigger, TriggerCommand};
use crate::framework::commands::application::UndoCommand;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::general::{
    AutomationModeCommand, JogWheelCommand, NopCommand, PunchInCommand, PunchOutCommand, RecArmAllCommand, RecordCommand, SaveCommand, SelectTrackCommand,
    ShiftCommand, TrackState, TrackStateCommand,
};
use crate::framework::commands::misc::{AddEffectCommand, AddTrackCommand, FootswitchCommand, PaneCommand, PanelLayoutCommand, Panels};
use crate::framework::commands::mode::{ButtonRowModeCommand, KnobRowModeCommand, ModeSelectCommand};
use crate::framework::commands::transport::{MetronomeCommand, PlayCommand};
use crate::framework::commands::transport_ext::{StopCommand, TapTempoCommand, ToggleLoopCommand, WindCommand};
use crate::framework::ctx::{Ctx, MAIN_MODE_MANAGER};
use crate::framework::feature::{BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::surface::{ButtonAction, ContinuousKind, LightSupplier, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::framework::view::control_only_view::ControlOnlyView;
use crate::host::Host;
use crate::midi::{ButtonEvent, MidiInput, MidiOutput};
use crate::scale::Scales;

/// Create a HUI instance with one surface per port pair (main device and extenders).
pub fn create(host: Host, sender: Sender, ports: &[MidiPorts], configuration: Configuration) -> Result<InstanceParts, String> {
    let num_devices = ports.len().clamp(1, 4);
    let value_changer = ValueChanger::twos_complement(16384, 100);
    let mut colors = ColorManager::new();
    colors.register_color(0, (0.0, 0.0, 0.0));
    colors.register_color(127, (1.0, 0.0, 0.0));
    colors.register_color_index(BUTTON_COLOR_OFF, 0);
    colors.register_color_index(BUTTON_COLOR_ON, 127);
    let scales = Scales::new(&value_changer, 36, 100, 8, 8);

    // createModel
    let mut ms = ModelSetup::new();
    ms.enable_main_drum_device(false);
    ms.enable_device(DeviceId::Eq);
    ms.set_has_full_flat_track_list(true);
    ms.set_num_tracks(8 * num_devices);
    ms.set_num_sends(6);
    ms.num_filter_column_entries = 8;
    ms.set_num_results(8);
    ms.num_device_layers = 0;
    ms.set_num_drum_pad_layers(0);
    // This is required to make the new clip function work!
    ms.set_num_scenes(8);
    ms.set_num_markers(8 * num_devices);
    let mut model = Model::new(host.clone(), sender.clone(), ms, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface
    let mut surfaces = Vec::with_capacity(num_devices);
    for port in &ports[..num_devices] {
        let output = MidiOutput::new(sender.clone(), Some(port.output));
        let input = MidiInput::new(sender.clone(), Some(port.input), &[])?;
        surfaces.push(Surface::new(output, input, None));
    }
    let mut surfaces = surfaces.into_iter();
    let first = surfaces.next().ok_or("No MIDI ports")?;
    let mut ctx = Ctx::new(host, sender, model, configuration, first, colors);
    for surface in surfaces {
        ctx.add_surface(surface);
    }
    ctx.for_each_surface(|ctx| {
        let output = ctx.surface.output.clone();
        ctx.text_display = Some(create_text_display(output.clone()));
        ctx.ext = Box::new(HuiSurfaceState::new(ctx.surface_index(), create_segment_display(output)));
        ctx.midi_handler = Some(Rc::new(handle_midi));
        ctx.modes.set_default_id(Modes::Volume);
    });
    ctx.share_button_states(true);

    Ok(InstanceParts { ctx, setup: Box::new(HuiSetup { num_devices, current_mode: None, previous_modes: vec![None; num_devices] }) })
}

/// The HUI controller setup.
pub struct HuiSetup {
    num_devices: usize,
    /// The mode for which the indications were set (former `currentMode`).
    current_mode: Option<Modes>,
    /// The active modes of the surfaces at the last check (replaces the mode change listeners).
    previous_modes: Vec<Option<Modes>>,
}

impl ControllerSetup for HuiSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        // createModes
        ctx.for_each_surface(|ctx| {
            ctx.register_mode(Modes::Track, HuiMode::new(HuiModeKind::Track));
            ctx.register_mode(Modes::Volume, HuiMode::new(HuiModeKind::Volume));
            ctx.register_mode(Modes::Pan, HuiMode::new(HuiModeKind::Pan));
            for i in 0..5 {
                ctx.register_mode(Modes::get(Modes::Send1, i), HuiMode::new(HuiModeKind::Send(i)));
            }
            ctx.register_mode(Modes::DeviceParams, HuiMode::new(HuiModeKind::DeviceParams));
            ctx.register_mode(Modes::EqDeviceParams, HuiMode::new(HuiModeKind::EqParams));
            // createViews
            ctx.register_view(Views::Control, ControlOnlyView);
        });
        let num_devices = self.num_devices;
        for index in 0..num_devices {
            ctx.with_surface(index, register_trigger_commands);
        }
        ctx.for_each_surface(register_continuous_commands);
        // createObservers: connect all modes
        ctx.connect_surface_mode_managers();
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let should_send_ping = ctx.config.should_send_ping();
        ctx.for_each_surface(|ctx| {
            ctx.set_active_view(Views::Control);
            ctx.set_active_mode(Modes::Pan);
            if should_send_ping {
                send_ping(ctx);
            }
        });
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        for event in ctx.model.take_events() {
            if let ModelEvent::TrackSelectionChanged { index, is_selected } = event {
                if index < 0 {
                    continue;
                }
                // DeviceParamsMode.trackSelectionChanged of the equalizer modes
                ctx.for_each_surface(|ctx| {
                    if let Some(mode) = ctx.modes.get(Modes::EqDeviceParams) {
                        if let Ok(mode) = mode.try_borrow() {
                            if let Some(mode) = mode.as_any().downcast_ref::<HuiMode>() {
                                mode.track_selection_changed(ctx, is_selected);
                            }
                        }
                    }
                });
            }
        }
        for id in ctx.config.take_changed_settings() {
            match id {
                cfg::PROFILE => {
                    cfg::apply_profile(&mut ctx.config);
                }
                SettingId::VU_METER_TYPE => ctx.for_each_surface(|ctx| {
                    ctx.update_active_mode_display();
                    if let Some(display) = ctx.text_display.as_mut() {
                        display.force_flush();
                    }
                }),
                SettingId::EXCLUDE_DEACTIVATED_ITEMS => {
                    let exclude = ctx.config.are_deactivated_items_excluded();
                    ctx.model.track_bank_mut().set_skip_disabled_items(exclude);
                }
                _ => {}
            }
        }
        // Former mode change listener of each surface: updateMode (surface, newMode)
        for index in 0..self.num_devices {
            let mode = ctx.with_surface(index, |ctx| ctx.modes.active_id());
            if mode != self.previous_modes[index] {
                self.previous_modes[index] = mode;
                self.update_mode(ctx, index, mode);
            }
        }
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.for_each_surface(|ctx| {
            ctx.flush_surface();
            hui_mut(ctx).segment_display.flush();
        });
        for index in 0..self.num_devices {
            let mode = ctx.with_surface(index, |ctx| ctx.modes.active_id());
            self.update_mode(ctx, index, mode);
            ctx.with_surface(index, update_segment_display);
        }
        self.update_vu_and_faders(ctx);
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        // Former surface.shutdown: turn off the LEDs, show the message on the display
        ctx.for_each_surface(|ctx| {
            ctx.turn_off_lights();
            if let Some(display) = ctx.text_display.as_mut() {
                display.notify("Please start Reaper...");
            }
        });
    }
}

impl HuiSetup {
    /// Former `updateMode`: the knob LEDs of the mode and the indications.
    fn update_mode(&mut self, ctx: &mut Ctx, surface: usize, mode: Option<Modes>) {
        let Some(mode) = mode else {
            return;
        };
        ctx.with_surface(surface, |ctx| {
            if let Some(m) = ctx.modes.get(mode) {
                if let Ok(m) = m.try_borrow() {
                    if let Some(m) = m.as_any().downcast_ref::<HuiMode>() {
                        m.update_knob_leds(ctx);
                    }
                }
            }
        });
        self.update_indication(ctx, mode);
    }

    /// Former `updateIndication`.
    fn update_indication(&mut self, ctx: &mut Ctx, mode: Modes) {
        if self.current_mode == Some(mode) {
            return;
        }
        self.current_mode = Some(mode);

        let is_track = mode == Modes::Track;
        let is_volume = mode == Modes::Volume;
        let is_pan = mode == Modes::Pan;
        let is_device = mode == Modes::DeviceParams;
        let is_eq_device = mode == Modes::EqDeviceParams;
        let selected = ctx.model.track_bank().selected_index();
        for i in 0..8 {
            let has_track_sel = selected == Some(i);
            let track = ctx.model.track_bank_mut().item_mut(i);
            track.set_volume_indication(is_track && has_track_sel || is_volume);
            track.set_pan_indication(is_track && has_track_sel || is_pan);
            let send_bank = track.send_bank_mut();
            for j in 0..6.min(send_bank.item_count()) {
                send_bank.item_mut(j).set_indication(is_track && has_track_sel || mode == Modes::get(Modes::Send1, j));
            }
            for (bank, enable) in [(ParameterBankId::CursorDevice, is_device), (ParameterBankId::Eq, is_eq_device)] {
                if let Some(bank) = crate::framework::parameterprovider::parameter_bank_mut(&mut ctx.model, bank) {
                    if i < bank.page_size() {
                        bank.item_mut(i).set_indication(enable);
                    }
                }
            }
        }
    }

    /// Former `updateVUandFaders`.
    fn update_vu_and_faders(&mut self, ctx: &mut Ctx) {
        let upper_bound = ctx.model.value_changer().upper_bound() as f64;
        let enable_vu_meters = ctx.config.is_enable_vu_meters();
        let has_motor_faders = ctx.config.has_motor_faders();
        for index in 0..self.num_devices {
            ctx.with_surface(index, |ctx| {
                for channel in 0..8 {
                    let position = index * 8 + channel;
                    let tb = ctx.model.current_track_bank();
                    let (vu_left, vu_right, volume) =
                        if position < tb.page_size() { (tb.item(position).vu_left(), tb.item(position).vu_right(), tb.item(position).volume()) } else { (0, 0, 0) };
                    // Update VU LEDs of channel
                    if enable_vu_meters {
                        update_vu_meters(ctx, channel, vu_left, vu_right, upper_bound);
                    }
                    // Update motor fader of channel
                    if has_motor_faders {
                        update_fader(ctx, channel, volume);
                    }
                }
            });
        }
    }
}

/// Send a ping every second (former `sendPing`).
fn send_ping(ctx: &mut Ctx) {
    ctx.surface.output.send_note(0, 0);
    ctx.schedule(1000, send_ping);
}

/// Former `updateSegmentDisplay`: the position on the segment display of the surface.
fn update_segment_display(ctx: &mut Ctx) {
    if !ctx.config.has_segment_display() {
        return;
    }
    let position_text = ctx.model.transport().position_text().replace('.', ":");
    hui_mut(ctx).segment_display.set_row(0, &position_text).all_done();
}

// ------------------------------------------------------------------------------------------
// Commands

/// Selects the channel and activates the volume mode while the fader is touched (former
/// `hui.command.trigger.FaderTouchCommand`).
struct FaderTouchCommand {
    select: SelectTrackCommand,
}

impl TriggerCommand for FaderTouchCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        match event {
            ButtonEvent::Down => ctx.set_active_mode(Modes::Volume),
            ButtonEvent::Up => ctx.restore_mode(),
            ButtonEvent::Long => {}
        }
        if ctx.config.is_touch_channel() {
            self.select.execute_normal(ctx, event);
        }
    }
}

/// Zooms or moves the cursor (former `ZoomAndKeysCursorCommand`).
struct ZoomAndKeysCursorCommand(Direction);

impl TriggerCommand for ZoomAndKeysCursorCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let zoom = ctx.config.is_zoom_state();
        let application = ctx.model.application_mut();
        match (self.0, zoom) {
            (Direction::Left, true) => application.zoom_out(),
            (Direction::Left, false) => application.arrow_key_left(),
            (Direction::Right, true) => application.zoom_in(),
            (Direction::Right, false) => application.arrow_key_right(),
            (Direction::Up, true) => application.dec_track_height(),
            (Direction::Up, false) => application.arrow_key_up(),
            (Direction::Down, true) => application.inc_track_height(),
            (Direction::Down, false) => application.arrow_key_down(),
        }
    }
}

/// Toggles the zoom state (former `hui.command.trigger.ZoomCommand`).
struct ZoomCommand;

impl TriggerCommand for ZoomCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            ctx.config.toggle_zoom_state();
        }
    }
}

/// The functions of the footswitches and the function keys (former `AssignableCommand`).
struct AssignableCommand {
    index: usize,
    inner: FootswitchCommand,
}

impl TriggerCommand for AssignableCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        let setting = ctx.config.assignable(self.index);
        self.inner.execute_setting(ctx, event, setting);
    }
}

/// Sets the volume of a track with a high resolution value (former `WorkaroundFader` with
/// `FaderAbsoluteCommand`): shows how to move the fader with Shift.
pub struct FaderAbsoluteCommand {
    index: usize,
}

impl ContinuousCommand for FaderAbsoluteCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        let tb = ctx.model.current_track_bank();
        if self.index >= tb.page_size() || !tb.item(self.index).does_exist() {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            let volume = tb.item(self.index).volume();
            let text = if volume < value {
                "Move down"
            } else if volume > value {
                "Move up"
            } else {
                "Perfect!"
            };
            ctx.notify(text);
        } else {
            ctx.model.current_track_bank_mut().item_mut(self.index).set_volume(value);
        }
    }
}

/// Sets the master volume with a high resolution value (former `WorkaroundMasterFader` with
/// `MasterFaderAbsoluteCommand`).
pub struct MasterFaderAbsoluteCommand;

impl ContinuousCommand for MasterFaderAbsoluteCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        ctx.model.master_track_mut().set_volume(value);
    }
}

// ------------------------------------------------------------------------------------------
// Buttons

fn lit(f: impl Fn(&Ctx) -> bool + 'static) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| if f(ctx) { HUI_BUTTON_STATE_ON } else { HUI_BUTTON_STATE_OFF })
}

/// Create a button which is triggered by its HUI control (zone * 8 + port), with a light
/// (former `addButtonHUI`).
fn add_hui(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, control: i32, supplier: Option<LightSupplier>) {
    ctx.surface.create_button(id, label).action = ButtonAction::Command(command);
    hui_mut(ctx).hui_buttons.insert(control, id);
    let supplier = supplier.unwrap_or_else(|| lit(move |ctx: &Ctx| ctx.surface.button(id).is_some_and(|b| b.is_pressed())));
    ctx.surface.create_light(None, Some(id), supplier, Rc::new(move |ctx: &mut Ctx, state| set_trigger(ctx, control, state)));
}

fn mode_color(id: ButtonId) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| ctx.mode_button_color_in(MAIN_MODE_MANAGER, id))
}

fn mode_select(ctx: &mut Ctx, id: ButtonId, label: &str, mode: Modes, control: i32) {
    add_hui(ctx, id, label, shared(ModeSelectCommand::new(mode)), control, Some(lit(move |ctx| ctx.modes.is_active(mode))));
}

fn automation(ctx: &mut Ctx, id: ButtonId, label: &str, mode: AutomationMode, control: i32, light: bool) {
    let supplier = if light { lit(move |ctx: &Ctx| ctx.model.transport().automation_write_mode() == mode) } else { lit(|_| false) };
    add_hui(ctx, id, label, shared(AutomationModeCommand::new(mode, true)), control, Some(supplier));
}

/// Register the buttons of the current surface (former `registerTriggerCommands`).
fn register_trigger_commands(ctx: &mut Ctx) {
    let index = hui(ctx).index;

    // Channel commands
    for channel in 0..8 {
        let channel_index = index * 8 + channel;
        let base = channel as i32 * 8;
        add_hui(ctx, ButtonId::get(ButtonId::FaderTouch1, channel), &format!("Fader {}", channel + 1), shared(FaderTouchCommand { select: SelectTrackCommand { index: channel_index } }), HUI_FADER1 + base, None);
        let select = ButtonId::get(ButtonId::RowSelect1, channel);
        add_hui(ctx, select, &format!("Select {}", channel + 1), shared(SelectTrackCommand { index: channel_index }), HUI_SELECT1 + base, Some(mode_color(select)));
        let knob_press: SharedTrigger = shared(ButtonRowModeCommand::new(0, channel));
        add_hui(ctx, ButtonId::get(ButtonId::Row11, channel), &format!("VSelect {}", channel + 1), knob_press.clone(), HUI_VSELECT1 + base, None);
        let rec_arm = ButtonId::get(ButtonId::Row21, channel);
        add_hui(ctx, rec_arm, &format!("Arm {}", channel + 1), shared(TrackStateCommand { index: channel_index, state: TrackState::RecArm }), HUI_ARM1 + base, Some(mode_color(rec_arm)));
        let solo = ButtonId::get(ButtonId::Row31, channel);
        add_hui(ctx, solo, &format!("Solo {}", channel + 1), shared(TrackStateCommand { index: channel_index, state: TrackState::Solo }), HUI_SOLO1 + base, Some(mode_color(solo)));
        let mute = ButtonId::get(ButtonId::Row41, channel);
        add_hui(ctx, mute, &format!("Mute {}", channel + 1), shared(TrackStateCommand { index: channel_index, state: TrackState::Mute }), HUI_MUTE1 + base, Some(mode_color(mute)));
        // HUI_INSERT1 is used on icon for selection
        let insert = ButtonId::get(ButtonId::Row51, channel);
        add_hui(ctx, insert, &format!("Insert {}", channel + 1), shared(SelectTrackCommand { index: channel_index }), HUI_INSERT1 + base, Some(mode_color(insert)));
        // HUI_AUTO1 is used for knob press as well (e.g. used like this with X-Touch)
        add_hui(ctx, ButtonId::get(ButtonId::Row61, channel), &format!("Auto {}", channel + 1), knob_press, HUI_AUTO1 + base, None);
    }

    // Key commands
    add_hui(ctx, ButtonId::Control, "Control", shared(NopCommand), HUI_KEY_CTRL_CLT, None);
    add_hui(ctx, ButtonId::Shift, "Shift", shared(ShiftCommand), HUI_KEY_SHIFT_AD, None);
    add_hui(ctx, ButtonId::Undo, "Undo", shared(UndoCommand::new()), HUI_KEY_UNDO, None);
    add_hui(ctx, ButtonId::Alt, "Alt", shared(NopCommand), HUI_KEY_ALT_FINE, None);
    add_hui(ctx, ButtonId::Select, "Select", shared(NopCommand), HUI_KEY_OPTION_A, None);
    add_hui(ctx, ButtonId::Save, "Save", shared(SaveCommand), HUI_KEY_SAVE, None);

    // Window commands
    add_hui(ctx, ButtonId::Mixer, "Mixer", shared(PaneCommand::new(Panels::Mixer)), HUI_WINDOW_MIX, None);
    add_hui(ctx, ButtonId::NoteEditor, "Note", shared(PaneCommand::new(Panels::Note)), HUI_WINDOW_EDIT, None);
    add_hui(ctx, ButtonId::AutomationEditor, "Automation", shared(PaneCommand::new(Panels::Automation)), HUI_WINDOW_TRANSPRT, None);
    add_hui(ctx, ButtonId::ToggleDevice, "Device", shared(PanelLayoutCommand::new()), HUI_WINDOW_STATUS, None);

    // Bank navigation
    add_hui(ctx, ButtonId::MoveTrackLeft, "Channel Left", shared(CursorCommand::new_mode_cursor(Direction::Left, true)), HUI_CHANL_LEFT, None);
    add_hui(ctx, ButtonId::MoveBankLeft, "Bank Left", shared(CursorCommand::new_mode_cursor(Direction::Down, true)), HUI_BANK_LEFT, None);
    add_hui(ctx, ButtonId::MoveTrackRight, "Channel Right", shared(CursorCommand::new_mode_cursor(Direction::Right, true)), HUI_CHANL_RIGHT, None);
    add_hui(ctx, ButtonId::MoveBankRight, "Bank Right", shared(CursorCommand::new_mode_cursor(Direction::Up, true)), HUI_BANK_RIGHT, None);

    // Assignment (mode selection)
    mode_select(ctx, ButtonId::Track, "Track", Modes::Track, HUI_ASSIGN1_INPUT);
    mode_select(ctx, ButtonId::PanSend, "Panning", Modes::Pan, HUI_ASSIGN1_PAN);
    mode_select(ctx, ButtonId::Send1, "Send 1", Modes::Send1, HUI_ASSIGN1_SEND_A);
    mode_select(ctx, ButtonId::Send2, "Send 2", Modes::Send2, HUI_ASSIGN1_SEND_B);
    mode_select(ctx, ButtonId::Send3, "Send 3", Modes::Send3, HUI_ASSIGN1_SEND_C);
    mode_select(ctx, ButtonId::Send4, "Send 4", Modes::Send4, HUI_ASSIGN1_SEND_D);
    mode_select(ctx, ButtonId::Send5, "Send 5", Modes::Send5, HUI_ASSIGN1_SEND_E);
    mode_select(ctx, ButtonId::PageLeft, "EQ", Modes::EqDeviceParams, HUI_ASSIGN1_OUTPUT);

    // Assignment 2
    mode_select(ctx, ButtonId::Device, "Device", Modes::DeviceParams, HUI_ASSIGN2_BYPASS);
    add_hui(ctx, ButtonId::RecArmAll, "RecRdyAll", shared(RecArmAllCommand), HUI_ASSIGN2_RECRDYAL, None);

    // Cursor arrows
    add_hui(ctx, ButtonId::ArrowDown, "Down", shared(ZoomAndKeysCursorCommand(Direction::Down)), HUI_CURSOR_DOWN, None);
    add_hui(ctx, ButtonId::ArrowLeft, "Left", shared(ZoomAndKeysCursorCommand(Direction::Left)), HUI_CURSOR_LEFT, None);
    add_hui(ctx, ButtonId::Zoom, "Toggle", shared(ZoomCommand), HUI_CURSOR_MODE, Some(lit(|ctx| ctx.config.is_zoom_state())));
    add_hui(ctx, ButtonId::ArrowRight, "Right", shared(ZoomAndKeysCursorCommand(Direction::Right)), HUI_CURSOR_RIGHT, None);
    add_hui(ctx, ButtonId::ArrowUp, "Up", shared(ZoomAndKeysCursorCommand(Direction::Up)), HUI_CURSOR_UP, None);

    // Navigation
    let rewind = Rc::new(RefCell::new(WindCommand::new(false)));
    let r = rewind.clone();
    add_hui(ctx, ButtonId::Rewind, "<<", rewind, HUI_TRANSPORT_REWIND, Some(lit(move |_| r.try_borrow().is_ok_and(|c| c.is_rewinding()))));
    let forward = Rc::new(RefCell::new(WindCommand::new(true)));
    let f = forward.clone();
    add_hui(ctx, ButtonId::Forward, ">>", forward, HUI_TRANSPORT_FAST_FWD, Some(lit(move |_| f.try_borrow().is_ok_and(|c| c.is_forwarding()))));
    add_hui(ctx, ButtonId::Stop, "Stop", shared(StopCommand::new()), HUI_TRANSPORT_STOP, Some(lit(|ctx| !ctx.model.transport().is_playing())));
    add_hui(ctx, ButtonId::Play, "Play", shared(PlayCommand::new()), HUI_TRANSPORT_PLAY, Some(lit(|ctx| ctx.model.transport().is_playing())));
    add_hui(ctx, ButtonId::Record, "Record", shared(RecordCommand::new()), HUI_TRANSPORT_RECORD, Some(lit(|ctx| ctx.model.transport().is_recording())));
    add_hui(
        ctx,
        ButtonId::ReturnToZero,
        "Return to Zero",
        shared(FnTrigger(|ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                ctx.model.transport_mut().set_position(0.0);
            }
        })),
        HUI_TRANSPORT_RETURN_TO_ZERO,
        None,
    );
    add_hui(
        ctx,
        ButtonId::AudioEngine,
        "Audio Engine",
        shared(FnTrigger(|ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                ctx.model.application_mut().toggle_engine_active();
            }
        })),
        HUI_TRANSPORT_ON_LINE,
        Some(lit(|ctx| ctx.model.application().is_engine_active())),
    );
    add_hui(ctx, ButtonId::Loop, "Loop", shared(ToggleLoopCommand::new()), HUI_TRANSPORT_LOOP, Some(lit(|ctx| ctx.model.transport().is_loop())));
    add_hui(ctx, ButtonId::Overdub, "Quick Punch", shared(PunchInCommand), HUI_TRANSPORT_QICK_PUNCH, Some(lit(|ctx| ctx.model.transport().is_punch_in_enabled())));
    add_hui(ctx, ButtonId::Metronome, "Metronome", shared(MetronomeCommand::new(false)), HUI_TRANSPORT_PRE, Some(lit(|ctx| ctx.model.transport().is_metronome_on())));
    // Note: The following two punch commands should actually set the punch point, which is
    // currently not possible with the Bitwig API
    add_hui(ctx, ButtonId::PunchIn, "In", shared(PunchInCommand), HUI_TRANSPORT_IN, Some(lit(|ctx| ctx.model.transport().is_punch_in_enabled())));
    add_hui(ctx, ButtonId::PunchOut, "Out", shared(PunchOutCommand), HUI_TRANSPORT_OUT, Some(lit(|ctx| ctx.model.transport().is_punch_out_enabled())));
    add_hui(ctx, ButtonId::TapTempo, "Tap Tempo", shared(TapTempoCommand::new()), HUI_TRANSPORT_POST, Some(lit(|_| false)));

    // Automation modes
    automation(ctx, ButtonId::AutomationOff, "Off", AutomationMode::TrimRead, HUI_AUTO_MODE_OFF, false);
    automation(ctx, ButtonId::AutomationTrim, "Trim", AutomationMode::TrimRead, HUI_AUTO_MODE_TRIM, true);
    automation(ctx, ButtonId::AutomationRead, "Read", AutomationMode::Read, HUI_AUTO_MODE_READ, true);
    automation(ctx, ButtonId::AutomationLatch, "Latch", AutomationMode::Latch, HUI_AUTO_MODE_LATCH, true);
    automation(ctx, ButtonId::AutomationWrite, "Write", AutomationMode::Write, HUI_AUTO_MODE_WRITE, true);
    automation(ctx, ButtonId::AutomationTouch, "Touch", AutomationMode::Touch, HUI_AUTO_MODE_TOUCH, true);

    // Edit
    add_hui(ctx, ButtonId::AddTrack, "Paste", shared(AddTrackCommand::new()), HUI_EDIT_PASTE, None);
    add_hui(ctx, ButtonId::AddEffect, "Copy", shared(AddEffectCommand::new()), HUI_EDIT_COPY, None);

    // Function keys and foot switches
    for i in 0..8 {
        add_hui(ctx, ButtonId::get(ButtonId::F1, i), &format!("F{}", i + 1), shared(AssignableCommand { index: 2 + i, inner: FootswitchCommand::new(2 + i) }), HUI_F1 + i as i32, None);
    }
    add_hui(ctx, ButtonId::Footswitch1, "Footswitch 1", shared(AssignableCommand { index: 0, inner: FootswitchCommand::new(0) }), HUI_FS_RLAY1, None);
    add_hui(ctx, ButtonId::Footswitch2, "Footswitch 2", shared(AssignableCommand { index: 1, inner: FootswitchCommand::new(1) }), HUI_FS_RLAY2, None);

    // Additional command for foot controllers (no MIDI and no light)
    ctx.add_button(ButtonId::New, "New", Some(shared(NewCommand::new())), crate::midi::BindType::Note, 0, -1, None);
}

/// Register the faders, knobs and the jog wheel of the current surface; they are not bound to
/// MIDI, the MIDI handler executes their commands (former `registerContinuousCommands`).
fn register_continuous_commands(ctx: &mut Ctx) {
    let index = hui(ctx).index;
    for channel in 0..8 {
        let fader = ctx.surface.create_continuous(ContinuousId::get(ContinuousId::Fader1, channel), &format!("Fader {}", channel + 1), ContinuousKind::Fader);
        fader.command = Some(Rc::new(RefCell::new(FaderAbsoluteCommand { index: index * 8 + channel })));
        let knob = ctx.surface.create_continuous(ContinuousId::get(ContinuousId::Knob1, channel), &format!("Knob {}", channel + 1), ContinuousKind::RelativeKnob);
        knob.command = Some(Rc::new(RefCell::new(KnobRowModeCommand::new(channel))));
    }
    let master = ctx.surface.create_continuous(ContinuousId::FaderMaster, "Master", ContinuousKind::Fader);
    master.command = Some(Rc::new(RefCell::new(MasterFaderAbsoluteCommand)));
    let value_changer = ctx.model.value_changer().clone();
    let jog = ctx.surface.create_continuous(ContinuousId::PlayPosition, "Jog", ContinuousKind::RelativeKnob);
    jog.command = Some(Rc::new(RefCell::new(JogWheelCommand::new(value_changer, Rc::new(std::cell::Cell::new(false))))));
}
