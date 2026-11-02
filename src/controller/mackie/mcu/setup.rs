// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Mackie MCU protocol with up to 3 extenders (former `MCUControllerSetup`).
//!
//! Each device (main device and extenders) is a surface of the context with its own MIDI port
//! pair, mode manager, view manager, displays and state (`McuSurfaceState` in `ctx.ext`), see
//! `doc/MULTI_SURFACE_PLAN.md`. The mode managers are connected (a mode change on one device
//! changes all) and the button states are shared (Shift on the main device counts on the
//! extenders), as in Java.
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - Java updates the VU meters and the faders of all devices once per device in `flush`; here
//!   too (the values are cached, so only the first pass sends).
//! - `layoutControls` (the simulator GUI) is not ported.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::command::{
    handle_solo_defeat, solo_state, with_mcu_mode, AssignableCommand, FaderTouchCommand, Key, KeyCommand, McuCursorCommand, McuDevicesCommand, McuFlipCommand,
    McuMoveTrackBankCommand, McuMultiModeSwitcherCommand, McuWindCommand, ModeSwitcher, PanCommand, SelectCommand, SendSelectCommand, TempoTicksCommand,
    ToggleDisplayCommand, TracksCommand, ZoomCommand,
};
use super::definition::*;
use super::display::{create_assignment_display, create_segment_display, create_text_display, shutdown_assignment_display, shutdown_segment_display, DisplayOptions};
use super::mode::{is_layer_mode, is_layer_send_mode, is_send_mode, with_master_volume, DeviceKind, MasterVolume, McuMode, McuModeKind};
use super::surface::{internal_shutdown, mcu, mcu_mut, switch_vu_mode, McuShared, McuSurfaceState};
use crate::backend::Sender;
use crate::config::mcu::{self as cfg, MainDisplay, McuConfig, McuDeviceType, SecondDisplay, VuMeterStyle};
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::constants::{AutomationMode, DeviceId};
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{ColorManager, BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::command::{shared, FnTrigger, SharedTrigger};
use crate::framework::commands::application::UndoCommand;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::cursor::Direction;
use crate::framework::commands::general::{AutomationModeCommand, JogWheelCommand, MarkerCommand, NopCommand, RecordCommand, SaveCommand, ShiftCommand};
use crate::framework::commands::misc::{BrowserCommand, LayoutCommand, OverdubCommand, PaneCommand, PanelLayoutCommand, Panels};
use crate::framework::commands::mode::{ButtonRowModeCommand, KnobRowModeCommand};
use crate::framework::commands::transport::{MetronomeCommand, PlayCommand};
use crate::framework::commands::transport_ext::{StopCommand, TapTempoCommand, ToggleLoopCommand};
use crate::framework::ctx::{Ctx, MAIN_MODE_MANAGER};
use crate::framework::feature::{BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::ids::{ButtonId, ContinuousId, Modes, OutputId, Views};
use crate::framework::surface::{ContinuousKind, LightSupplier, Surface};
use crate::framework::value_changer::{RelativeEncoding, ValueChanger};
use crate::framework::view::control_only_view::ControlOnlyView;
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};
use crate::scale::Scales;

const TAG_RIGHT: &str = "Right";

/// The acronyms of the modes on the assignment display (former `MODE_ACRONYMS`).
pub fn mode_acronym(mode: Modes) -> Option<&'static str> {
    Some(match mode {
        Modes::Track => "TR",
        Modes::Volume => "VL",
        Modes::Pan => "PN",
        Modes::Send1 => "S1",
        Modes::Send2 => "S2",
        Modes::Send3 => "S3",
        Modes::Send4 => "S4",
        Modes::Send5 => "S5",
        Modes::Send6 => "S6",
        Modes::Send7 => "S7",
        Modes::Send8 => "S8",
        Modes::Master => "MT",
        Modes::DeviceLayer => "LA",
        Modes::DeviceLayerVolume => "LV",
        Modes::DeviceLayerPan => "LP",
        Modes::DeviceLayerSend1 => "L1",
        Modes::DeviceLayerSend2 => "L2",
        Modes::DeviceLayerSend3 => "L3",
        Modes::DeviceLayerSend4 => "L4",
        Modes::DeviceLayerSend5 => "L5",
        Modes::DeviceLayerSend6 => "L6",
        Modes::DeviceLayerSend7 => "L7",
        Modes::DeviceLayerSend8 => "L8",
        Modes::DeviceParams => "DC",
        Modes::Browser => "BR",
        Modes::Markers => "MK",
        Modes::EqDeviceParams => "EQ",
        Modes::InstrumentDeviceParams => "IT",
        Modes::ProjectParameters => "PP",
        Modes::TrackParameters => "TP",
        _ => return None,
    })
}

/// The modes which can control the faders if "use faders like knobs" is on (former
/// `VALUE_MODES`).
const VALUE_MODES: [Modes; 16] = [
    Modes::Volume,
    Modes::Pan,
    Modes::Track,
    Modes::Send1,
    Modes::Send2,
    Modes::Send3,
    Modes::Send4,
    Modes::Send5,
    Modes::Send6,
    Modes::Send7,
    Modes::Send8,
    Modes::DeviceParams,
    Modes::EqDeviceParams,
    Modes::InstrumentDeviceParams,
    Modes::ProjectParameters,
    Modes::TrackParameters,
];

/// The color manager (former `MCUColorManager`).
pub fn create_color_manager() -> ColorManager {
    let mut colors = ColorManager::new();
    colors.register_color_index(BUTTON_COLOR_OFF, 0);
    colors.register_color_index(BUTTON_COLOR_ON, 0);
    colors.register_color_index(BUTTON_COLOR_HI, 127);
    colors.register_color_index(BUTTON_STATE_OFF, 0);
    colors.register_color_index(BUTTON_STATE_ON, 0);
    colors.register_color_index(BUTTON_STATE_HI, 127);
    colors
}

/// Create a MCU instance with one surface per port pair (main device and extenders).
pub fn create(host: Host, sender: Sender, ports: &[MidiPorts], configuration: Configuration) -> Result<InstanceParts, String> {
    let num_devices = ports.len().clamp(1, 4);
    let value_changer = ValueChanger::new(RelativeEncoding::SignedBit2, 16241 + 1, 10);
    let colors = create_color_manager();
    let scales = Scales::new(&value_changer, 36, 100, 8, 8);

    // createModel
    let n = 8 * num_devices;
    let mut ms = ModelSetup::new();
    ms.enable_main_drum_device(false);
    ms.enable_device(DeviceId::Eq);
    ms.enable_device(DeviceId::FirstInstrument);
    ms.wants_focused_parameter = true;
    ms.set_num_tracks(n);
    ms.num_device_layers = n as i32;
    ms.set_num_drum_pad_layers(n as i32);
    ms.set_num_devices_in_bank(n as i32);
    ms.set_num_params(n as i32);
    ms.num_param_pages = n;
    ms.set_has_flat_track_list(configuration.is_track_navigation_flat());
    ms.set_has_full_flat_track_list(configuration.should_include_fx_tracks_in_track_bank());
    ms.set_num_sends(14);
    // This is required to make the new clip function work!
    ms.set_num_scenes(8);
    ms.num_filter_column_entries = 8;
    ms.set_num_results(8);
    ms.set_num_markers(n);
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

    let shared_state = Rc::new(RefCell::new(McuShared {
        num_surfaces: num_devices,
        display_options: Rc::new(Cell::new(DisplayOptions::default())),
        jog_wheel_last_param: Rc::new(Cell::new(false)),
        touched_faders: [false; 32],
    }));
    for index in 0..num_devices {
        let shared_state = shared_state.clone();
        ctx.with_surface(index, |ctx| {
            let device_type = ctx.config.device_type(index);
            let is_main_device = device_type == McuDeviceType::Main;
            let output = ctx.surface.output.clone();
            let options = shared_state.borrow().display_options.clone();
            ctx.text_display = Some(create_text_display(output.clone(), true, device_type == McuDeviceType::MackieExtender, false, options.clone()));
            let second = create_text_display(output.clone(), false, false, is_main_device, options);
            ctx.ext = Box::new(McuSurfaceState::new(index, device_type, shared_state, second, create_segment_display(output.clone()), create_assignment_display(output)));
            ctx.modes.set_default_id(Modes::Volume);
        });
    }
    ctx.share_button_states(true);

    Ok(InstanceParts { ctx, setup: Box::new(McuSetup::new(num_devices)) })
}

/// The MCU controller setup.
pub struct McuSetup {
    num_devices: usize,
    vu_values: [i32; 32],
    vu_values_right: [i32; 32],
    master_vu_values: [i32; 2],
    fader_values: [i32; 32],
    master_fader_value: i32,
    browser_active: bool,
}

impl McuSetup {
    pub fn new(num_devices: usize) -> Self {
        McuSetup {
            num_devices,
            vu_values: [-1; 32],
            vu_values_right: [-1; 32],
            master_vu_values: [-1; 2],
            fader_values: [-1; 32],
            master_fader_value: -1,
            browser_active: false,
        }
    }
}

impl ControllerSetup for McuSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx);
        // createViews
        ctx.for_each_surface(|ctx| ctx.register_view(Views::Control, ControlOnlyView));
        register_trigger_commands(ctx, self.num_devices);
        register_continuous_commands(ctx);
        // createObservers: connect all modes
        ctx.connect_surface_mode_managers();
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let mode = ctx.config.startup_mode();
        ctx.for_each_surface(|ctx| {
            ctx.set_active_view(Views::Control);
            ctx.set_active_mode(mode);
        });
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        for event in ctx.model.take_events() {
            if let ModelEvent::TrackSelectionChanged { index, is_selected } = event {
                if index < 0 {
                    // The master track (former selection observer of the master track)
                    if is_selected && ctx.modes.is_active(Modes::Track) {
                        ctx.set_active_mode(Modes::Master);
                    }
                    continue;
                }
                handle_track_change(ctx, is_selected);
                // DeviceParamsMode.trackSelectionChanged of the equalizer modes
                ctx.for_each_surface(|ctx| {
                    if let Some(mode) = ctx.modes.get(Modes::EqDeviceParams) {
                        if let Ok(mode) = mode.try_borrow() {
                            if let Some(mode) = mode.as_any().downcast_ref::<McuMode>() {
                                mode.track_selection_changed(ctx, is_selected);
                            }
                        }
                    }
                });
            }
        }

        let mut assignable_changed = false;
        for id in ctx.config.take_changed_settings() {
            if cfg::is_assignable_setting(id) {
                assignable_changed = true;
                continue;
            }
            handle_setting_change(ctx, id);
        }
        if assignable_changed {
            // Former observer of ASSIGNABLE_BUTTONS
            ctx.for_each_surface(|ctx| {
                mcu(ctx).shared.borrow().jog_wheel_last_param.set(false);
                with_master_volume(ctx, |mv, ctx| mv.set_control_last_param_active(ctx, false));
            });
        }

        // Browser observer (former activateBrowserObserver (Modes.BROWSER))
        let browser_active = ctx.model.browser().is_some_and(|browser| browser.is_active());
        if browser_active != self.browser_active {
            self.browser_active = browser_active;
            if browser_active {
                ctx.set_temporary_mode(Modes::Browser);
            } else if ctx.modes.is_active(Modes::Browser) {
                ctx.restore_mode();
            }
        }
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        // AbstractControllerSetup.flush: the lights, the views and the displays of all surfaces
        ctx.for_each_surface(|ctx| {
            ctx.flush_surface();
            let state = mcu_mut(ctx);
            state.second_display.flush();
            state.segment_display.flush();
            state.assignment_display.flush();
        });
        for index in 0..self.num_devices {
            let mode = ctx.with_surface(index, |ctx| ctx.modes.active_id());
            self.update_mode(ctx, mode);
            let Some(_) = mode else {
                continue;
            };
            self.update_vu_meters(ctx);
            let is_shift_pressed = ctx.with_surface(index, |ctx| ctx.surface.is_shift_pressed());
            self.update_faders(ctx, is_shift_pressed);
            self.update_segment_display(ctx);
            ctx.with_surface(index, |ctx| {
                if let Some(mode) = ctx.active_mode() {
                    if let Ok(mode) = mode.try_borrow() {
                        if let Some(mode) = mode.as_any().downcast_ref::<McuMode>() {
                            mode.update_knob_leds(ctx);
                        }
                    }
                }
            });
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        // Former surface.shutdown: internalShutdown of the MCU and of AbstractControlSurface
        ctx.for_each_surface(|ctx| {
            internal_shutdown(ctx);
            ctx.turn_off_lights();
            if let Some(display) = ctx.text_display.as_mut() {
                display.shutdown();
            }
            let output = ctx.surface.output.clone();
            let state = mcu_mut(ctx);
            state.second_display.shutdown();
            shutdown_segment_display(&output);
            shutdown_assignment_display(&mut state.assignment_display);
        });
    }
}

impl McuSetup {
    /// Former `updateMode`: the acronym of the mode on the assignment display.
    fn update_mode(&self, ctx: &mut Ctx, mode: Option<Modes>) {
        let Some(mode) = mode else {
            return;
        };
        if !ctx.config.has_assignment_display() {
            return;
        }
        for index in 0..self.num_devices {
            ctx.with_surface(index, |ctx| {
                if !mcu(ctx).is_main_device {
                    return;
                }
                let display = &mut mcu_mut(ctx).assignment_display;
                if let Some(acronym) = mode_acronym(mode) {
                    display.set_row(0, acronym);
                }
                display.all_done();
            });
        }
    }

    /// Former `updateSegmentDisplay`: the position and the tempo on the 7-segment display.
    fn update_segment_display(&self, ctx: &mut Ctx) {
        if !ctx.config.has_segment_display() {
            return;
        }
        let transport = ctx.model.transport();
        let is_display_time = ctx.config.is_display_time();
        let mut position_text = if is_display_time { transport.position_text() } else { transport.beat_text() }.replace('.', ":");
        if ctx.config.is_display_ticks() {
            if !is_display_time {
                position_text.push(' ');
            }
        } else {
            let mut tempo = transport.format_tempo_no_fraction(transport.tempo());
            if tempo.chars().count() < 3 {
                tempo = format!("0{tempo}");
            }
            let end = position_text.rfind(':').map_or(0, |pos| pos + 1);
            position_text = format!("{}{tempo}", &position_text[..end]);
        }
        for index in 0..self.num_devices {
            ctx.with_surface(index, |ctx| {
                if mcu(ctx).is_main_device {
                    mcu_mut(ctx).segment_display.set_row(0, &position_text).all_done();
                }
            });
        }
    }

    fn scale_vu(ctx: &Ctx, vu: i32) -> i32 {
        let range = if ctx.config.vu_meter_style() == VuMeterStyle::Mackie { 12.0 } else { 13.0 };
        (ctx.model.value_changer().to_normalized_value(vu as f64) * range).round() as i32
    }

    fn send_vu_value(ctx: &Ctx, track: i32, scaled_vu: i32, vu_clip_state: bool, is_master_or_right_channel: bool) {
        let output = &ctx.surface.output;
        let channel = i32::from(is_master_or_right_channel);
        output.send_channel_aftertouch(channel, 0x10 * track + scaled_vu, 0);
        // iCON devices do not support the clip state
        if ctx.config.vu_meter_style() != VuMeterStyle::Icon {
            output.send_channel_aftertouch(channel, 0x10 * track + if vu_clip_state { 0x0E } else { 0x0F }, 0);
        }
    }

    /// Former `updateVUMeters`.
    fn update_vu_meters(&mut self, ctx: &mut Ctx) {
        if !ctx.config.has_vu_meters() || !ctx.config.are_vu_meters_enabled() {
            return;
        }
        let is_layer = is_layer_mode(ctx.modes.active_id());
        let style = ctx.config.vu_meter_style();
        let always_send = ctx.config.always_send_vu_meters();
        for index in 0..self.num_devices {
            ctx.with_surface(index, |ctx| {
                let offset = mcu(ctx).extender_offset;
                for i in 0..8 {
                    let channel = offset + i;
                    // (vu, left, right, clip, left clip, right clip)
                    let values = if is_layer {
                        let device = ctx.model.cursor_device();
                        let c = if device.has_drum_pads() {
                            let bank = device.drum_pad_bank();
                            (channel < bank.page_size()).then(|| &**bank.item(channel))
                        } else {
                            let bank = device.layer_bank();
                            (channel < bank.page_size()).then(|| bank.item(channel))
                        };
                        c.map(|c| (c.vu(), c.vu_left(), c.vu_right(), c.vu_clip_state(), c.vu_left_clip_state(), c.vu_right_clip_state()))
                    } else {
                        let tb = ctx.model.current_track_bank();
                        (channel < tb.page_size()).then(|| {
                            let c = tb.item(channel);
                            (c.vu(), c.vu_left(), c.vu_right(), c.vu_clip_state(), c.vu_left_clip_state(), c.vu_right_clip_state())
                        })
                    };
                    let (vu, vu_left, vu_right, clip, clip_left, clip_right) = values.unwrap_or((0, 0, 0, false, false, false));
                    if style == VuMeterStyle::Asparion {
                        let scaled = Self::scale_vu(ctx, vu_left);
                        if self.vu_values[channel] != scaled || always_send {
                            self.vu_values[channel] = scaled;
                            Self::send_vu_value(ctx, i as i32, scaled, clip_left, false);
                        }
                        let scaled = Self::scale_vu(ctx, vu_right);
                        if self.vu_values_right[channel] != scaled || always_send {
                            self.vu_values_right[channel] = scaled;
                            Self::send_vu_value(ctx, i as i32, scaled, clip_right, true);
                        }
                    } else {
                        let scaled = Self::scale_vu(ctx, vu);
                        if self.vu_values[channel] != scaled || always_send {
                            self.vu_values[channel] = scaled;
                            Self::send_vu_value(ctx, i as i32, scaled, clip, false);
                        }
                    }
                }

                // Stereo VUs of master channel, only available on iCON devices
                if style == VuMeterStyle::Icon && mcu(ctx).is_main_device {
                    let master = ctx.model.master_track();
                    let (left, right) = (master.vu_left(), master.vu_right());
                    let scaled = Self::scale_vu(ctx, left);
                    if self.master_vu_values[0] != scaled {
                        self.master_vu_values[0] = scaled;
                        Self::send_vu_value(ctx, 0, scaled, false, true);
                    }
                    let scaled = Self::scale_vu(ctx, right);
                    if self.master_vu_values[1] != scaled {
                        self.master_vu_values[1] = scaled;
                        Self::send_vu_value(ctx, 1, scaled, false, true);
                    }
                }
            });
        }
    }

    /// Former `updateFaders`: the motor faders.
    fn update_faders(&mut self, ctx: &mut Ctx, is_shift_pressed: bool) {
        if !ctx.config.has_motor_faders() {
            return;
        }
        let active = ctx.modes.active_id();
        let mode_id = match active {
            Some(mode) if ctx.config.use_faders_as_knobs() && VALUE_MODES.contains(&mode) => mode,
            _ => {
                if is_layer_mode(active) {
                    Modes::DeviceLayerVolume
                } else {
                    Modes::Volume
                }
            }
        };
        for index in 0..self.num_devices {
            ctx.with_surface(index, |ctx| {
                let offset = mcu(ctx).extender_offset;
                let values: Vec<Option<i32>> = with_mcu_mode(ctx, mode_id, |mode| {
                    (0..8)
                        .map(|channel| {
                            // Don't update fader if the user touches and therefore 'stops' it
                            if mode.is_knob_touched(channel) {
                                None
                            } else {
                                Some(crate::framework::feature::Mode::knob_value(mode, ctx, channel).max(0))
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
                for (channel, value) in values.iter().enumerate() {
                    let Some(value) = *value else {
                        continue;
                    };
                    let position = offset + channel;
                    if value != self.fader_values[position] {
                        self.fader_values[position] = value;
                        // Same as Java: % 127 and / 127
                        ctx.surface.output.send_pitchbend(channel as i32, value % 127, value / 127);
                    }
                }

                // Update motor fader of master channel
                if !mcu(ctx).is_main_device {
                    return;
                }
                let control_last_param = mcu(ctx).master_volume.as_ref().is_some_and(|m| m.is_control_last_param_active());
                let value = if control_last_param {
                    let parameter = crate::framework::commands::general::focused_parameter(ctx);
                    if parameter.does_exist(ctx) {
                        parameter.value(ctx)
                    } else {
                        0
                    }
                } else if is_shift_pressed {
                    ctx.model.transport().metronome_volume()
                } else {
                    ctx.model.master_track().volume()
                };
                if value != self.master_fader_value {
                    self.master_fader_value = value;
                    ctx.surface.output.send_pitchbend(8, value % 127, value / 127);
                }
            });
        }
    }
}

/// Former `handleTrackChange` with `updateMode`: leave the master mode if the master track is
/// not selected anymore (the views are not changed, there is only the control view).
fn handle_track_change(ctx: &mut Ctx, is_selected: bool) {
    if !is_selected {
        return;
    }
    if ctx.modes.is_active(Modes::Master) && !ctx.model.master_track().is_selected() {
        // Former Modes.isTrackMode: TRACK .. STOP_CLIP
        let is_track_mode = ctx.modes.previous_id().is_some_and(|m| m >= Modes::Track && m <= Modes::StopClip);
        if is_track_mode {
            ctx.restore_mode();
        } else {
            ctx.set_active_mode(Modes::Track);
        }
    }
}

/// React on a changed setting (former setting observers of `createObservers`).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    match id {
        cfg::PROFILE => {
            cfg::apply_profile(&mut ctx.config);
        }
        cfg::VU_METER_STYLE => {
            let mode = if ctx.config.has_vu_meters() { VUMODE_LED_AND_LCD } else { VUMODE_OFF };
            ctx.for_each_surface(|ctx| {
                switch_vu_mode(ctx, mode);
                ctx.update_active_mode_display();
                if let Some(display) = ctx.text_display.as_mut() {
                    display.force_flush();
                }
            });
        }
        cfg::USE_FADERS_AS_KNOBS => ctx.for_each_surface(|ctx| {
            if let Some(mode) = ctx.active_mode() {
                if let Ok(mode) = mode.try_borrow() {
                    if let Some(mode) = mode.as_any().downcast_ref::<McuMode>() {
                        mode.parameters_adjusted(ctx);
                    }
                }
            }
        }),
        cfg::MAIN_DISPLAY => {
            let main_display = ctx.config.main_display_type();
            let is_asparion = main_display == MainDisplay::Asparion;
            update_display_options(ctx, |o| {
                o.insert_space = main_display != MainDisplay::Mackie7Characters;
                o.is_asparion = is_asparion;
            });
            ctx.for_each_surface(|ctx| {
                if let Some(display) = ctx.text_display.as_mut() {
                    let cells = display.number_of_cells();
                    display.set_number_of_cells(2, cells, (if is_asparion { 12 } else { 7 }) * 8);
                }
            });
        }
        cfg::SECOND_DISPLAY => {
            let second = ctx.config.second_display_type();
            let is_qcon = second == SecondDisplay::Qcon;
            update_display_options(ctx, |o| o.is_qcon_second_display = is_qcon);
            ctx.for_each_surface(|ctx| {
                let is_main_device = mcu(ctx).is_main_device;
                let display = &mut mcu_mut(ctx).second_display;
                // Former updateShortSecondDisplay and changeDisplaySize
                let cells = if is_main_device { if is_qcon { 9 } else { 8 } } else { display.number_of_cells() };
                display.set_number_of_cells(2, cells, (if second == SecondDisplay::Asparion { 8 } else { 7 }) * 8);
            });
        }
        SettingId::EXCLUDE_DEACTIVATED_ITEMS => {
            let exclude = ctx.config.are_deactivated_items_excluded();
            ctx.model.track_bank_mut().set_skip_disabled_items(exclude);
        }
        _ => {}
    }
}

fn update_display_options(ctx: &mut Ctx, f: impl FnOnce(&mut DisplayOptions)) {
    let options = mcu(ctx).shared.borrow().display_options.clone();
    let mut o = options.get();
    f(&mut o);
    options.set(o);
}

// ------------------------------------------------------------------------------------------
// Modes

/// Register the modes of all surfaces (former `createModes`).
fn create_modes(ctx: &mut Ctx) {
    ctx.for_each_surface(|ctx| {
        let register = |ctx: &mut Ctx, id: Modes, kind: McuModeKind| {
            let mode = McuMode::new(ctx, kind);
            ctx.register_mode(id, mode);
        };
        register(ctx, Modes::Track, McuModeKind::Track);
        register(ctx, Modes::Volume, McuModeKind::Volume);
        register(ctx, Modes::Pan, McuModeKind::Pan);
        for i in 0..8 {
            register(ctx, Modes::get(Modes::Send1, i), McuModeKind::Send(i));
        }
        register(ctx, Modes::Master, McuModeKind::Master);
        register(ctx, Modes::DeviceLayer, McuModeKind::Layer);
        register(ctx, Modes::DeviceLayerVolume, McuModeKind::LayerVolume);
        register(ctx, Modes::DeviceLayerPan, McuModeKind::LayerPan);
        for i in 0..8 {
            register(ctx, Modes::get(Modes::DeviceLayerSend1, i), McuModeKind::LayerSend(i));
        }
        register(ctx, Modes::DeviceParams, McuModeKind::Device(DeviceKind::Cursor));
        register(ctx, Modes::EqDeviceParams, McuModeKind::Device(DeviceKind::Eq));
        register(ctx, Modes::InstrumentDeviceParams, McuModeKind::Device(DeviceKind::FirstInstrument));
        register(ctx, Modes::ProjectParameters, McuModeKind::ProjectParameters);
        register(ctx, Modes::TrackParameters, McuModeKind::TrackParameters);
        register(ctx, Modes::Browser, McuModeKind::Browser);
        register(ctx, Modes::Markers, McuModeKind::Markers);
        if mcu(ctx).is_main_device {
            mcu_mut(ctx).master_volume = Some(MasterVolume::new());
        }
    });
}

// ------------------------------------------------------------------------------------------
// Buttons

/// 127 if the light is on (former `BooleanSupplier` with the colors `BUTTON_STATE_ON/HI`).
fn lit(f: impl Fn(&Ctx) -> bool + 'static) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| if f(ctx) { MCU_BUTTON_STATE_ON } else { MCU_BUTTON_STATE_OFF })
}

/// 127 while the button is pressed (former `ButtonPressedSupplier` with the colors
/// `BUTTON_STATE_ON/HI`).
fn pressed(id: ButtonId) -> LightSupplier {
    lit(move |ctx: &Ctx| ctx.surface.button(id).is_some_and(|b| b.is_pressed()))
}

/// Is the button of the current surface pressed (former `surface.getButton (id).isPressed ()`,
/// which does not check the other surfaces)?
fn own_pressed(ctx: &Ctx, id: ButtonId) -> bool {
    ctx.surface.button(id).is_some_and(|b| b.is_pressed())
}

fn add(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, control: i32, supplier: Option<LightSupplier>) {
    let supplier = supplier.unwrap_or_else(|| pressed(id));
    ctx.add_button(id, label, Some(command), BindType::Note, 0, control, Some(supplier));
}

fn mode_active(modes: &'static [Modes]) -> LightSupplier {
    lit(move |ctx: &Ctx| ctx.modes.is_any_active(modes))
}

fn automation(ctx: &mut Ctx, id: ButtonId, label: &str, mode: AutomationMode, control: i32) {
    add(ctx, id, label, shared(AutomationModeCommand::new(mode, false)), control, Some(lit(move |ctx: &Ctx| ctx.model.transport().automation_write_mode() == mode)));
}

/// Former `isRecordShifted`.
fn is_record_shifted(ctx: &Ctx) -> bool {
    ctx.surface.is_shift_pressed() != ctx.config.is_flip_record()
}

/// Register the commands of all buttons (former `registerTriggerCommands`).
fn register_trigger_commands(ctx: &mut Ctx, num_devices: usize) {
    for index in 0..num_devices {
        ctx.with_surface(index, |ctx| {
            if mcu(ctx).is_main_device {
                register_main_device_buttons(ctx);
            }
            for i in 0..8 {
                let row1 = ButtonId::get(ButtonId::Row21, i);
                let row2 = ButtonId::get(ButtonId::Row31, i);
                let row3 = ButtonId::get(ButtonId::Row41, i);
                let row4 = ButtonId::get(ButtonId::RowSelect1, i);
                let label_index = 8 * (num_devices - index - 1) + i + 1;
                let color = |id: ButtonId| -> LightSupplier { Rc::new(move |ctx: &Ctx| ctx.mode_button_color_in(MAIN_MODE_MANAGER, id)) };
                add(ctx, row1, &format!("Rec Arm {label_index}"), shared(ButtonRowModeCommand::new(1, i)), MCU_ARM1 + i as i32, Some(color(row1)));
                add(ctx, row2, &format!("Solo {label_index}"), shared(ButtonRowModeCommand::new(2, i)), MCU_SOLO1 + i as i32, Some(color(row2)));
                add(ctx, row3, &format!("Mute {label_index}"), shared(ButtonRowModeCommand::new(3, i)), MCU_MUTE1 + i as i32, Some(color(row3)));
                add(ctx, row4, &format!("Select {label_index}"), shared(SelectCommand::new(i)), MCU_SELECT1 + i as i32, Some(color(row4)));
            }
        });
    }
}

fn register_main_device_buttons(ctx: &mut Ctx) {
    // Navigation
    let rewind = Rc::new(RefCell::new(McuWindCommand::new(false)));
    let r = rewind.clone();
    add(ctx, ButtonId::Rewind, "<<", rewind, MCU_REWIND, Some(lit(move |_| r.try_borrow().is_ok_and(|c| c.inner.is_rewinding()))));
    let forward = Rc::new(RefCell::new(McuWindCommand::new(true)));
    let f = forward.clone();
    add(ctx, ButtonId::Forward, ">>", forward, MCU_FORWARD, Some(lit(move |_| f.try_borrow().is_ok_and(|c| c.inner.is_forwarding()))));
    add(ctx, ButtonId::Loop, "Loop", shared(ToggleLoopCommand::new()), MCU_REPEAT, Some(lit(|ctx| ctx.model.transport().is_loop())));
    add(ctx, ButtonId::Stop, "Stop", shared(StopCommand::new()), MCU_STOP, Some(lit(|ctx| !ctx.model.transport().is_playing())));
    add(ctx, ButtonId::Play, "Play", shared(PlayCommand::new()), MCU_PLAY, Some(lit(|ctx| ctx.model.transport().is_playing())));
    add(
        ctx,
        ButtonId::Record,
        "Record",
        shared(RecordCommand::new()),
        MCU_RECORD,
        Some(lit(|ctx| if is_record_shifted(ctx) { ctx.model.is_launcher_overdub() } else { ctx.model.transport().is_recording() })),
    );
    add(ctx, ButtonId::Scrub, "Scrub", shared(FnTrigger(|ctx: &mut Ctx, event, _| if event == ButtonEvent::Down && !ctx.surface.is_shift_pressed() { ModeSwitcher::scroll_up(ctx) })), MCU_SCRUB, Some(mode_active(&[Modes::DeviceParams])));
    add(ctx, ButtonId::ArrowLeft, "Left", shared(McuCursorCommand::new(Direction::Left)), MCU_ARROW_LEFT, None);
    add(ctx, ButtonId::ArrowRight, TAG_RIGHT, shared(McuCursorCommand::new(Direction::Right)), MCU_ARROW_RIGHT, None);
    add(ctx, ButtonId::ArrowUp, "Up", shared(McuCursorCommand::new(Direction::Up)), MCU_ARROW_UP, None);
    add(ctx, ButtonId::ArrowDown, "Down", shared(McuCursorCommand::new(Direction::Down)), MCU_ARROW_DOWN, None);
    add(ctx, ButtonId::Zoom, "Zoom", shared(ZoomCommand), MCU_ZOOM, Some(lit(|ctx| ctx.config.is_zoom_state())));

    // Display Mode
    add(ctx, ButtonId::ToggleDisplay, "Toggle Display", shared(ToggleDisplayCommand), MCU_NAME_VALUE, Some(lit(|ctx| ctx.config.is_display_track_names())));
    add(ctx, ButtonId::TempoTicks, "Tempo Ticks", shared(TempoTicksCommand), MCU_SMPTE_BEATS, Some(lit(|ctx| ctx.config.is_display_ticks())));

    // Functions
    add(ctx, ButtonId::Shift, "Shift", shared(ShiftCommand), MCU_SHIFT, None);
    add(ctx, ButtonId::Select, "Option", shared(NopCommand), MCU_OPTION, None);

    // Foot-switches and Function keys
    let jog_wheel_last_param = mcu(ctx).shared.borrow().jog_wheel_last_param.clone();
    add(ctx, ButtonId::Footswitch1, "Footswitch 1", shared(AssignableCommand::new(0, jog_wheel_last_param.clone())), MCU_USER_A, None);
    add(ctx, ButtonId::Footswitch2, "Footswitch 2", shared(AssignableCommand::new(1, jog_wheel_last_param.clone())), MCU_USER_B, None);
    for i in 0..8 {
        let last_param = jog_wheel_last_param.clone();
        let command = AssignableCommand::new(2 + i, jog_wheel_last_param.clone());
        add(ctx, ButtonId::get(ButtonId::F1, i), &format!("F{}", i + 1), shared(command), MCU_F1 + i as i32, Some(lit(move |ctx| AssignableCommand::is_active(ctx, 2 + i, last_param.get()))));
    }

    // Assignment - mode selection
    add(
        ctx,
        ButtonId::Track,
        "Track",
        shared(TracksCommand::new()),
        MCU_MODE_IO,
        Some(lit(|ctx| {
            if own_pressed(ctx, ButtonId::Select) {
                ctx.model.is_cursor_track_pinned()
            } else {
                ctx.modes.is_any_active(&[Modes::Track, Modes::Volume, Modes::DeviceLayer, Modes::DeviceLayerVolume])
            }
        })),
    );
    add(ctx, ButtonId::PanSend, "Pan", shared(PanCommand::new()), MCU_MODE_PAN, Some(mode_active(&[Modes::Pan, Modes::DeviceLayerPan])));
    add(
        ctx,
        ButtonId::Sends,
        "Sends",
        shared(SendSelectCommand::new()),
        MCU_MODE_SENDS,
        Some(lit(|ctx| is_send_mode(ctx.modes.active_id()) || is_layer_send_mode(ctx.modes.active_id()))),
    );
    add(
        ctx,
        ButtonId::Device,
        "Device",
        shared(McuDevicesCommand::new()),
        MCU_MODE_PLUGIN,
        Some(lit(|ctx| {
            if own_pressed(ctx, ButtonId::Select) {
                ctx.model.cursor_device().is_pinned()
            } else {
                ctx.modes.is_any_active(&[Modes::DeviceParams, Modes::ProjectParameters, Modes::TrackParameters])
            }
        })),
    );
    add(ctx, ButtonId::PageLeft, "EQ", shared(McuMultiModeSwitcherCommand::new(&[Modes::EqDeviceParams])), MCU_MODE_EQ, Some(mode_active(&[Modes::EqDeviceParams])));
    add(
        ctx,
        ButtonId::PageRight,
        "INST",
        shared(McuMultiModeSwitcherCommand::new(&[Modes::InstrumentDeviceParams])),
        MCU_MODE_DYN,
        Some(mode_active(&[Modes::InstrumentDeviceParams])),
    );
    add(ctx, ButtonId::MoveTrackLeft, "Left", shared(McuMoveTrackBankCommand::new(true, true)), MCU_TRACK_LEFT, None);
    add(ctx, ButtonId::MoveTrackRight, TAG_RIGHT, shared(McuMoveTrackBankCommand::new(true, false)), MCU_TRACK_RIGHT, None);

    // Automation
    automation(ctx, ButtonId::AutomationTrim, "Trim", AutomationMode::TrimRead, MCU_TRIM);
    automation(ctx, ButtonId::AutomationRead, "Read", AutomationMode::Read, MCU_READ);
    automation(ctx, ButtonId::AutomationWrite, "Write", AutomationMode::Write, MCU_WRITE);
    automation(ctx, ButtonId::AutomationGroup, "Group/Write", AutomationMode::LatchPreview, MCU_GROUP);
    automation(ctx, ButtonId::AutomationTouch, "Touch", AutomationMode::Touch, MCU_TOUCH);
    automation(ctx, ButtonId::AutomationLatch, "Latch", AutomationMode::Latch, MCU_LATCH);
    add(ctx, ButtonId::Undo, "Undo", shared(UndoCommand::new()), MCU_UNDO, None);

    // Panes
    add(ctx, ButtonId::NoteEditor, "Note Editor", shared(PaneCommand::new(Panels::Note)), MCU_MIDI_TRACKS, None);
    add(ctx, ButtonId::AutomationEditor, "Automation Editor", shared(PaneCommand::new(Panels::Automation)), MCU_INPUTS, None);
    add(
        ctx,
        ButtonId::ToggleDevice,
        "Toggle Device",
        shared(PanelLayoutCommand::new()),
        MCU_AUDIO_TRACKS,
        Some(lit(|ctx| !ctx.surface.is_shift_pressed() && ctx.model.cursor_device().is_window_open())),
    );
    add(ctx, ButtonId::Mixer, "Mixer", shared(PaneCommand::new(Panels::Mixer)), MCU_AUDIO_INSTR, None);

    // Layouts
    use crate::daw::application::{PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_EDIT, PANEL_LAYOUT_MIX};
    add(ctx, ButtonId::LayoutArrange, "Arrange", shared(LayoutCommand::with_layout(PANEL_LAYOUT_ARRANGE)), MCU_AUX, None);
    add(ctx, ButtonId::LayoutMix, "Mix", shared(LayoutCommand::with_layout(PANEL_LAYOUT_MIX)), MCU_BUSSES, None);
    add(ctx, ButtonId::LayoutEdit, "Edit", shared(LayoutCommand::with_layout(PANEL_LAYOUT_EDIT)), MCU_OUTPUTS, None);

    // Utilities
    add(ctx, ButtonId::Browse, "Browse", shared(BrowserCommand::new()), MCU_USER, Some(mode_active(&[Modes::Browser])));
    add(
        ctx,
        ButtonId::Metronome,
        "Metronome",
        shared(MetronomeCommand::new(false)),
        MCU_CLICK,
        Some(lit(|ctx| if own_pressed(ctx, ButtonId::Shift) { ctx.model.transport().is_metronome_ticks_on() } else { ctx.model.transport().is_metronome_on() })),
    );
    add(
        ctx,
        ButtonId::Groove,
        "Solo Defeat",
        shared(FnTrigger(|ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                handle_solo_defeat(ctx);
            }
        })),
        MCU_SOLO,
        Some(lit(solo_state)),
    );
    add(
        ctx,
        ButtonId::Overdub,
        "Overdub",
        shared(OverdubCommand::new()),
        MCU_REPLACE,
        Some(lit(|ctx| if own_pressed(ctx, ButtonId::Shift) { ctx.model.is_launcher_overdub() } else { ctx.model.is_arranger_overdub() })),
    );
    add(ctx, ButtonId::TapTempo, "Tap Tempo", shared(TapTempoCommand::new()), MCU_NUDGE, None);
    add(
        ctx,
        ButtonId::Duplicate,
        "Duplicate",
        shared(FnTrigger(|ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                ctx.model.cursor_track_mut().duplicate();
            }
        })),
        MCU_DROP,
        None,
    );

    // Currently not used but prevent error in console
    add(ctx, ButtonId::Control, "Control", shared(NopCommand), MCU_CONTROL, None);
    add(ctx, ButtonId::Alt, "Alt", shared(NopCommand), MCU_ALT, None);

    // Fader Controls
    add(
        ctx,
        ButtonId::Flip,
        "Flip",
        shared(McuFlipCommand),
        MCU_FLIP,
        Some(lit(|ctx| if ctx.surface.is_shift_pressed() { ctx.model.is_effect_track_bank_active() } else { ctx.config.use_faders_as_knobs() })),
    );
    add(ctx, ButtonId::Cancel, "Cancel", shared(KeyCommand(Key::Escape)), MCU_CANCEL, None);
    add(ctx, ButtonId::Enter, "Enter", shared(KeyCommand(Key::Enter)), MCU_ENTER, None);
    add(ctx, ButtonId::MoveBankLeft, "Bank Left", shared(McuMoveTrackBankCommand::new(false, true)), MCU_BANK_LEFT, None);
    add(ctx, ButtonId::MoveBankRight, "Bank Right", shared(McuMoveTrackBankCommand::new(false, false)), MCU_BANK_RIGHT, None);

    // Additional command for foot controllers
    add(ctx, ButtonId::New, "New", shared(NewCommand::new()), -1, None);

    // Only MCU
    add(ctx, ButtonId::Save, "Save", shared(SaveCommand), MCU_SAVE, Some(lit(|ctx| ctx.model.project().is_dirty())));
    add(
        ctx,
        ButtonId::Marker,
        "Marker",
        shared(MarkerCommand),
        MCU_MARKER,
        Some(lit(|ctx| if own_pressed(ctx, ButtonId::Shift) { ctx.model.arranger().are_cue_markers_visible() } else { ctx.modes.is_active(Modes::Markers) })),
    );
    add(
        ctx,
        ButtonId::ToggleVu,
        "Toggle VU",
        shared(FnTrigger(|ctx: &mut Ctx, event, _| {
            if event == ButtonEvent::Down {
                ctx.config.toggle_vu_meters_enabled();
            }
        })),
        MCU_EDIT,
        Some(lit(|ctx| ctx.config.are_vu_meters_enabled())),
    );

    // The LEDs are sent as CC (as in Java, which calls setTrigger without bind type)
    for (output_id, control, on) in [(OutputId::Led1, MCU_SMPTE_LED, true), (OutputId::Led2, MCU_BEATS_LED, false)] {
        ctx.surface.create_light(
            Some(output_id),
            None,
            Rc::new(move |ctx: &Ctx| if ctx.config.is_display_ticks() == on { 2 } else { 0 }),
            Rc::new(move |ctx: &mut Ctx, state| ctx.surface.set_trigger(BindType::Cc, 0, control, state)),
        );
    }
}

// ------------------------------------------------------------------------------------------
// Knobs and faders

/// Register the knobs, faders and the jog wheel (former `registerContinuousCommands`).
fn register_continuous_commands(ctx: &mut Ctx) {
    let encoder_value_changer = ValueChanger::new(RelativeEncoding::SignedBit2, 16241 + 1, 10);
    ctx.for_each_surface(|ctx| {
        if mcu(ctx).is_main_device {
            let last_param = mcu(ctx).shared.borrow().jog_wheel_last_param.clone();
            let jog_wheel = Rc::new(RefCell::new(JogWheelCommand::new(encoder_value_changer.clone(), last_param)));
            ctx.add_continuous(ContinuousId::PlayPosition, "Jog Wheel", ContinuousKind::RelativeKnob, jog_wheel, BindType::Cc, 0, MCU_CC_JOG);

            ctx.surface.create_continuous(ContinuousId::FaderMaster, "Master", ContinuousKind::Fader);
            ctx.surface.bind_continuous(ContinuousId::FaderMaster, BindType::PitchBend, 8, 0);
            bind_touch(ctx, ContinuousId::FaderMaster, shared(FaderTouchCommand::new(8)), MCU_FADER_MASTER);
            with_master_volume(ctx, |mv, ctx| mv.activate(ctx));
        }
        for i in 0..8 {
            let knob = ContinuousId::get(ContinuousId::Knob1, i);
            ctx.add_continuous(knob, &format!("Knob {i}"), ContinuousKind::RelativeKnob, Rc::new(RefCell::new(KnobRowModeCommand::new(i))), BindType::Cc, 0, MCU_CC_VPOT1 + i as i32);
            // Note: this is pressing the knobs' button not touching it!
            bind_touch(ctx, knob, shared(ButtonRowModeCommand::new(0, i)), MCU_VSELECT1 + i as i32);

            let fader = ContinuousId::get(ContinuousId::Fader1, i);
            ctx.surface.create_continuous(fader, &format!("Fader {}", i + 1), ContinuousKind::Fader);
            ctx.surface.bind_continuous(fader, BindType::PitchBend, i as i32, 0);
            bind_touch(ctx, fader, shared(FaderTouchCommand::new(i)), MCU_FADER_TOUCH1 + i as i32);
        }
    });
}

fn bind_touch(ctx: &mut Ctx, id: ContinuousId, command: SharedTrigger, note: i32) {
    if let Some(control) = ctx.surface.continuous_mut(id) {
        control.touch_command = Some(command);
    }
    ctx.surface.input.bind_touch(id, BindType::Note, 0, note);
}
