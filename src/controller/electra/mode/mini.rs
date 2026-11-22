// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Electra One Mini (former `ElectraOneMiniMixerMode`,
//! `ElectraOneMiniDeviceMode` and `ElectraOneMiniTransportMode`): a page of 8 knobs (row 1, with
//! their pushes) and the 4 function buttons (row 2); the 4th function button is Shift.

use super::transport::record_selected_slot;
use super::{custom, electra_mode_common, play_position_parameter, tempo_parameter, track_exists, value_text, ElectraModeBase, TrackInfo};
use crate::config::values::fix_ascii;
use crate::controller::electra::definition::ElectraVariant;
use crate::controller::electra::surface;
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::daw::constants::GrooveParameterId;
use crate::daw::events::ParameterBankId;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::general::FocusedParameter;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::Modes;
use crate::framework::parameterprovider::{
    parameter_bank, parameter_bank_mut, provider, BankParameterProvider, CombinedParameterProvider, CueVolumeParameter, EmptyParameterProvider, FixedParameterProvider,
    GrooveParameter, PanParameterProvider, ParamRef, SelectedTrackParameterProvider, SharedProvider, VolumeParameterProvider,
};
use crate::midi::ButtonEvent;

const TAG_PAN: &str = "PAN";
const TAG_VOLUME: &str = "VOLUME";

fn is_shift_pressed(ctx: &Ctx) -> bool {
    surface::state(ctx).is_shift_pressed
}

/// The 4th function button: the emulated Shift (all Mini modes).
fn handle_shift_button(ctx: &mut Ctx, event: ButtonEvent) {
    if event != ButtonEvent::Long {
        surface::update_shift(ctx, event == ButtonEvent::Down);
    }
}

// ---------------------------------------------------------------------------------------------

/// The mixer page of the Mini (former `ElectraOneMiniMixerMode`): volume, panorama or the
/// selected track's volume, panorama and sends on the knobs; mute, solo or rec arm on the pushes.
pub struct MiniMixerMode {
    pub base: ElectraModeBase,
    volume_provider: SharedProvider,
    pan_provider: SharedProvider,
    selected_track_provider: SharedProvider,
    /// Is the volume provider active (the Java identity check of the providers)?
    is_volume_provider: bool,
    push_mode: Modes,
    turn_mode: Modes,
}

impl MiniMixerMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = ElectraModeBase::new(ElectraVariant::Mini, 0, "Volume");
        let volume_provider = provider(VolumeParameterProvider::new(ctx));
        base.base.set_parameter_provider(ctx, volume_provider.clone());
        MiniMixerMode {
            base,
            volume_provider,
            pan_provider: provider(PanParameterProvider::new(ctx)),
            selected_track_provider: provider(SelectedTrackParameterProvider::new(ctx)),
            is_volume_provider: true,
            push_mode: Modes::Mute,
            turn_mode: Modes::Volume,
        }
    }

    fn set_provider(&mut self, ctx: &mut Ctx, provider: SharedProvider, is_volume: bool) {
        self.base.base.set_parameter_provider(ctx, provider);
        self.base.base.bind_controls(ctx);
        self.is_volume_provider = is_volume;
    }

    fn handle_function_buttons(&mut self, ctx: &mut Ctx, column: usize, event: ButtonEvent) {
        match column {
            // Toggle volume/pan
            0 => {
                if event == ButtonEvent::Down {
                    let is_volume_mode = self.is_volume_provider && self.turn_mode != Modes::Track;
                    let provider = if is_volume_mode { self.pan_provider.clone() } else { self.volume_provider.clone() };
                    self.set_provider(ctx, provider, !is_volume_mode);
                    self.turn_mode = if is_volume_mode { Modes::Pan } else { Modes::Volume };
                }
            }
            // Toggle mute/solo
            1 => {
                if event == ButtonEvent::Down {
                    self.push_mode = if is_shift_pressed(ctx) {
                        Modes::RecArm
                    } else if self.push_mode != Modes::Mute {
                        Modes::Mute
                    } else {
                        Modes::Solo
                    };
                }
            }
            // Move track page left/right
            2 => {
                if event == ButtonEvent::Down {
                    if is_shift_pressed(ctx) {
                        ctx.model.current_track_bank_mut().select_previous_page();
                    } else {
                        ctx.model.current_track_bank_mut().select_next_page();
                    }
                }
            }
            // Toggle Shift
            3 => handle_shift_button(ctx, event),
            _ => {}
        }
    }

    fn main_mode(mode: Modes) -> Option<(&'static str, ColorEx)> {
        match mode {
            Modes::Volume => Some((TAG_VOLUME, ColorEx::BLUE)),
            Modes::Pan => Some((TAG_PAN, ColorEx::DARK_PURPLE)),
            Modes::Track => Some(("TRACK", ColorEx::DARK_YELLOW)),
            _ => None,
        }
    }

    fn push_mode_label(mode: Modes) -> Option<(&'static str, ColorEx)> {
        match mode {
            Modes::Mute => Some(("MUTE", ColorEx::ORANGE)),
            Modes::Solo => Some(("SOLO", ColorEx::YELLOW)),
            Modes::RecArm => Some(("REC ARM", ColorEx::RED)),
            _ => None,
        }
    }
}

impl Mode for MiniMixerMode {
    electra_mode_common!(base);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        // The 4 function buttons of the Mini
        if row == 1 {
            self.handle_function_buttons(ctx, column, event);
            return;
        }
        // The push buttons of the knobs
        if event != ButtonEvent::Down || !track_exists(ctx, column) {
            return;
        }
        if is_shift_pressed(ctx) {
            let track = ctx.model.current_track_bank_mut().item_mut(column);
            if !track.is_selected() {
                track.select();
            }
            let provider = self.selected_track_provider.clone();
            self.set_provider(ctx, provider, false);
            self.turn_mode = Modes::Track;
            return;
        }
        let track = ctx.model.current_track_bank_mut().item_mut(column);
        match self.push_mode {
            Modes::RecArm => track.toggle_rec_arm(),
            Modes::Mute => track.toggle_mute(),
            Modes::Solo => track.toggle_solo(),
            _ => {}
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let bank = ctx.model.current_track_bank();
        for column in 0..8 {
            let indexed = TrackInfo::from_track(bank.item(column));
            let group_label = indexed.group_label();
            let track = if self.turn_mode == Modes::Track { bank.selected_item().map(TrackInfo::from_track).unwrap_or_else(TrackInfo::empty) } else { indexed };
            let mut value = 0;
            let mut value_label = " ".to_string();
            let mut label = " ".to_string();
            let mut color = ColorEx::BLACK;
            if track.exists {
                color = track.color;
                match self.turn_mode {
                    Modes::Pan => {
                        value = track.pan;
                        value_label = value_text(&track.pan_str);
                        label = TAG_PAN.to_string();
                    }
                    Modes::Track => match column {
                        0 => {
                            value = track.volume;
                            value_label = value_text(&track.volume_str);
                            label = track.group_label();
                        }
                        1 => {
                            value = track.pan;
                            value_label = value_text(&track.pan_str);
                            label = TAG_PAN.to_string();
                        }
                        _ => {
                            let selected = bank.selected_item();
                            let send = selected.map(|t| t.send_bank()).filter(|s| column - 2 < s.item_count()).map(|s| s.item(column - 2));
                            match send.filter(|s| s.does_exist()) {
                                Some(send) => {
                                    value = send.value();
                                    value_label = send.displayed_value_limited(15);
                                    label = format!("S{}: {}", column + 1, send.name_limited(11));
                                }
                                None => color = ColorEx::BLACK,
                            }
                        }
                    },
                    // Volume (the default)
                    _ => {
                        value = track.volume;
                        value_label = value_text(&track.volume_str);
                        label = TAG_VOLUME.to_string();
                    }
                }
            }
            let cache = &mut self.base.page_cache;
            cache.update_value(0, column, value, Some(&value_label));
            cache.update_element(0, column, Some(&label), Some(color), Some(track.exists));
            cache.update_group_label(100 + column as i32, &group_label);
        }
        let (main_label, main_color) = Self::main_mode(self.turn_mode).map_or((None, None), |(l, c)| (Some(l), Some(c)));
        let (push_label, push_color) = Self::push_mode_label(self.push_mode).map_or((None, None), |(l, c)| (Some(l), Some(c)));
        let cache = &mut self.base.page_cache;
        cache.update_element(1, 0, main_label, main_color, Some(true));
        cache.update_element(1, 1, push_label, push_color, Some(true));
        cache.flush(ctx);
    }
}

// ---------------------------------------------------------------------------------------------

/// The device page of the Mini (former `ElectraOneMiniDeviceMode`): the parameters of the cursor
/// device, the project or the cursor track.
pub struct MiniDeviceMode {
    pub base: ElectraModeBase,
    device_provider: SharedProvider,
    project_provider: SharedProvider,
    track_provider: SharedProvider,
    parameter_bank: ParameterBankId,
    turn_mode: Modes,
}

impl MiniDeviceMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = ElectraModeBase::new(ElectraVariant::Mini, 1, "Parameters");
        let device_provider = provider(BankParameterProvider::new(ctx, ParameterBankId::CursorDevice));
        base.base.set_parameter_provider(ctx, device_provider.clone());
        MiniDeviceMode {
            base,
            device_provider,
            project_provider: provider(BankParameterProvider::new(ctx, ParameterBankId::Project)),
            track_provider: provider(BankParameterProvider::new(ctx, ParameterBankId::CursorTrack)),
            parameter_bank: ParameterBankId::CursorDevice,
            turn_mode: Modes::DeviceParams,
        }
    }

    fn set_provider(&mut self, ctx: &mut Ctx, provider: SharedProvider) {
        self.base.base.set_parameter_provider(ctx, provider);
        self.base.base.bind_controls(ctx);
    }

    fn handle_function_buttons(&mut self, ctx: &mut Ctx, column: usize, event: ButtonEvent) {
        match column {
            // Toggle the device, track and project parameters
            0 => {
                if event == ButtonEvent::Down {
                    let (provider, mode, bank) = match self.turn_mode {
                        Modes::DeviceParams => (self.project_provider.clone(), Modes::ProjectParameters, ParameterBankId::Project),
                        Modes::ProjectParameters => (self.track_provider.clone(), Modes::TrackParameters, ParameterBankId::CursorTrack),
                        _ => (self.device_provider.clone(), Modes::DeviceParams, ParameterBankId::CursorDevice),
                    };
                    self.turn_mode = mode;
                    self.parameter_bank = bank;
                    self.set_provider(ctx, provider);
                }
            }
            // Toggle the device window (if any)
            1 => {
                if event == ButtonEvent::Down && ctx.model.cursor_device().does_exist() {
                    if is_shift_pressed(ctx) {
                        ctx.model.cursor_device_mut().toggle_enabled_state();
                    } else {
                        ctx.model.cursor_device_mut().toggle_window_open();
                    }
                }
            }
            // Move the parameter page left/right
            2 => {
                if event == ButtonEvent::Down {
                    let shift = is_shift_pressed(ctx);
                    if let Some(bank) = parameter_bank_mut(&mut ctx.model, self.parameter_bank) {
                        if shift {
                            bank.select_previous_item();
                        } else {
                            bank.select_next_item();
                        }
                    }
                }
            }
            // Toggle Shift
            3 => handle_shift_button(ctx, event),
            _ => {}
        }
    }
}

impl Mode for MiniDeviceMode {
    electra_mode_common!(base);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        // The 4 function buttons of the Mini
        if row == 1 {
            self.handle_function_buttons(ctx, column, event);
            return;
        }
        // The push buttons of the knobs
        if event != ButtonEvent::Down || !track_exists(ctx, column) {
            return;
        }
        if is_shift_pressed(ctx) {
            // Select device 1-8 on the current track
            let device = ctx.model.cursor_device().device_bank().item(column);
            if device.does_exist() && !device.is_selected() {
                ctx.model.cursor_device_mut().device_bank_mut().item_mut(column).select();
            }
            let provider = self.device_provider.clone();
            self.set_provider(ctx, provider);
            self.turn_mode = Modes::DeviceParams;
            return;
        }
        self.base.base.parameter(ctx, column).reset_value(ctx);
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let page = parameter_bank(&ctx.model, self.parameter_bank).map(|b| b.page_bank().selected_item()).unwrap_or_default();
        let device_bank = ctx.model.cursor_device().device_bank();
        for column in 0..8 {
            let parameter = self.base.base.parameter(ctx, column);
            let exists = !page.is_empty() && parameter.does_exist(ctx);
            let label = if exists { fix_ascii(&format!("{page}: {}", parameter.name(ctx))) } else { " ".to_string() };
            let value_label = if exists { value_text(&parameter.displayed_value(ctx)) } else { " ".to_string() };
            let value = parameter.value(ctx);
            // Set the device names
            let device = device_bank.item(column);
            let group_label = if device.does_exist() { format!("{}: {}", device.position() + 1, fix_ascii(device.name())) } else { "None".to_string() };
            let cache = &mut self.base.page_cache;
            cache.update_element(0, column, Some(&label), Some(if exists { ColorEx::ORANGE } else { ColorEx::BLACK }), Some(exists));
            cache.update_value(0, column, value, Some(&value_label));
            cache.update_group_label(108 + column as i32, &group_label);
        }
        // As in Java, there are no colors for these modes (MAIN_COLORS has the ones of the mixer)
        let label = match self.turn_mode {
            Modes::DeviceParams => Some("SEL. DEVICE"),
            Modes::ProjectParameters => Some("PROJECT"),
            Modes::TrackParameters => Some("TRACK"),
            _ => None,
        };
        let cache = &mut self.base.page_cache;
        cache.update_element(1, 0, label, None, Some(true));
        cache.flush(ctx);
    }
}

// ---------------------------------------------------------------------------------------------

/// The transport page of the Mini (former `ElectraOneMiniTransportMode`).
pub struct MiniTransportMode {
    pub base: ElectraModeBase,
    new_command: NewCommand,
    play_command: PlayCommand,
}

impl MiniTransportMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = ElectraModeBase::new(ElectraVariant::Mini, 2, "Transport");
        let fixed = |parameter: ParamRef| provider(FixedParameterProvider::single(parameter));
        let parameters = CombinedParameterProvider::new(vec![
            // Slot 1: Position
            fixed(play_position_parameter()),
            // Slot 2: -
            provider(EmptyParameterProvider::new(1)),
            // Slot 3: Swing
            fixed(custom(GrooveParameter(GrooveParameterId::ShuffleAmount))),
            // Slot 4: Tempo
            fixed(tempo_parameter()),
            // Slot 5: Last Parameter
            fixed(custom(FocusedParameter)),
            // Slot 6: Metronome volume
            fixed(ParamRef::MetronomeVolume),
            // Slot 7: Cue volume
            fixed(custom(CueVolumeParameter)),
            // Slot 8: Master volume
            fixed(ParamRef::MasterVolume),
        ]);
        base.base.set_parameter_provider(ctx, provider(parameters));
        MiniTransportMode { base, new_command: NewCommand::new(), play_command: PlayCommand::new() }
    }

    fn handle_function_buttons(&mut self, ctx: &mut Ctx, column: usize, event: ButtonEvent) {
        match column {
            0 => {
                if event == ButtonEvent::Down {
                    ctx.model.transport_mut().toggle_metronome();
                }
            }
            1 => {
                if event == ButtonEvent::Down {
                    ctx.model.transport_mut().tap_tempo();
                }
            }
            2 => {
                if event == ButtonEvent::Down {
                    if is_shift_pressed(ctx) {
                        ctx.model.transport_mut().start_recording();
                    } else {
                        self.play_command.execute(ctx, ButtonEvent::Up, 127);
                    }
                }
            }
            // Toggle Shift
            3 => handle_shift_button(ctx, event),
            _ => {}
        }
    }
}

impl Mode for MiniTransportMode {
    electra_mode_common!(base);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        // The 4 function buttons of the Mini
        if row == 1 {
            self.handle_function_buttons(ctx, column, event);
            return;
        }
        // The push buttons of the knobs
        if event != ButtonEvent::Down {
            return;
        }
        match column {
            0 => ctx.model.application_mut().redo(),
            1 => ctx.model.application_mut().undo(),
            2 => ctx.model.transport_mut().toggle_write_arranger_automation(),
            3 => ctx.model.toggle_overdub(),
            4 => self.new_command.execute_now(ctx),
            5 => record_selected_slot(ctx),
            6 => {
                if let Some(clip) = ctx.model.cursor_clip() {
                    clip.quantize(1.0);
                }
            }
            7 => ctx.model.toggle_launcher_overdub(),
            _ => {}
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let transport = ctx.model.transport();
        let groove = ParamRef::Custom(std::rc::Rc::new(GrooveParameter(GrooveParameterId::ShuffleAmount)));
        let focused = crate::framework::commands::general::focused_parameter(ctx);
        let master = ctx.model.master_track();
        let cue = ctx.model.project();
        let is_shifted = is_shift_pressed(ctx);

        let cache = &mut self.base.page_cache;
        cache.update_value(0, 0, 0, Some(&value_text(transport.beat_text())));
        cache.update_value(0, 2, groove.value(ctx), Some(&groove.displayed_value(ctx)));
        cache.update_value(0, 3, 0, Some(&transport.format_tempo(transport.tempo())));
        cache.update_value(0, 4, focused.value(ctx), Some(&value_text(&focused.displayed_value(ctx))));
        cache.update_value(0, 5, transport.metronome_volume(), Some(&value_text(transport.metronome_volume_str())));
        cache.update_value(0, 6, cue.cue_volume(), Some(&value_text(&cue.cue_volume_str())));
        cache.update_value(0, 7, master.volume(), Some(&value_text(&master.volume_str())));
        cache.update_element(1, 2, Some(if is_shifted { "RECORD" } else { "PLAY" }), Some(if is_shifted { ColorEx::RED } else { ColorEx::GREEN }), Some(true));
        cache.flush(ctx);
    }
}
