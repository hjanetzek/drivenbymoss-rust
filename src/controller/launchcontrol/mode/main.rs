// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The main modes of the factory templates (former `launchcontrol.mode.main`:
//! `XLAbstractMainMode`, `XLAbstractTrackMode`, `XLTrackMixMode`, `XLEqMode`,
//! `XLLayerMixMode`, `XLBaseNoteEditMode`, `XLDrumPadEditMode`, `XLNoteEditMode`) and the
//! dummy mode of the user templates (former `XLDummyMode`).
//!
//! The Java class hierarchy is merged into [`XlMainMode`]; the differences of the concrete
//! modes are selected by its [`MainKind`]. The knob LEDs are set by
//! [`XlMainMode::set_knob_color`] (former `IXLMode.setKnobColor`).

use super::super::colors::*;
use super::super::surface::{self as xl, has_been_used, set_active_track_mode_used, track_modes};
use super::note_parameter::{note_position, XlNoteAttribute, XlNoteParameter};
use super::CLIP_KEY;
use crate::daw::bank::BankItem;
use crate::daw::clip::{NotePosition, StepState};
use crate::daw::constants::DeviceId;
use crate::daw::device::{EqualizerBandType, SpecificDevice};
use crate::daw::events::ParameterBankId;
use crate::framework::color_ids::abstract_drum_view as drum;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::mv_helper;
use crate::framework::commands::transport_ext::TempoCommand;
use crate::framework::ctx::{Ctx, MAIN_MODE_MANAGER};
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, ContinuousId, Modes};
use crate::framework::mode::dummy::DummyMode;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{
    parameter_bank, provider, BankParameterProvider, CombinedParameterProvider, DrumDeviceRef, FixedParameterProvider, LayerOrDrumPadParameterProvider,
    PanParameterProvider, ParamRef, SendParameterProvider, SharedProvider,
};
use crate::midi::ButtonEvent;

/// The names of the parameter banks of the track mix mode (former `MODE_NAMES`).
const MODE_NAMES: [&str; 3] = ["Cursor Device Parameters", "Track Parameters", "Project Parameters"];

/// The parameter banks of the track mix mode.
const TRACK_MIX_BANKS: [ParameterBankId; 3] = [ParameterBankId::CursorDevice, ParameterBankId::CursorTrack, ParameterBankId::Project];

/// The LED intensity (0-3) for a knob value.
fn intensity(value: i32) -> i32 {
    if value == 0 {
        0
    } else {
        value / 42 + 1
    }
}

/// Former `StringUtils.formatPercentage`.
fn format_percentage(value: f64) -> String {
    let formatted = format!("{:.1}", value * 100.0);
    let formatted = formatted.strip_suffix(".0").unwrap_or(&formatted).to_string();
    format!("{formatted}%")
}

/// The mode specific parts of the main modes.
pub enum MainKind {
    /// Former `XLTrackMixMode`: send A, send B and panning (or device parameters).
    TrackMix {
        selected_device_param_provider_index: usize,
        device_param_providers: [SharedProvider; 3],
        send_parameter_provider1: SharedProvider,
        send_parameter_provider2: SharedProvider,
    },
    /// Former `XLEqMode`: equalizer type, Q and frequency (or device parameters).
    Eq,
    /// Former `XLLayerMixMode`: layer send A, send B and panning (or device parameters).
    LayerMix,
    /// Former `XLDrumPadEditMode`.
    DrumPadEdit,
    /// Former `XLNoteEditMode`.
    NoteEdit,
}

/// A main mode of the factory templates (former `XLAbstractMainMode` with its sub-classes).
pub struct XlMainMode {
    pub base: ParameterModeBase,
    pub kind: MainKind,
    default_mode: Modes,
    track_mode: Option<Modes>,
    /// True between activation and deactivation: a repeated activation (to rebind the knobs when
    /// the parameters of the device change) keeps the track button mode.
    is_active: bool,
    parameter_provider: SharedProvider,
    parameter_provider_with_device_params: SharedProvider,
    was_long: bool,
    transport_used: bool,
    new_command: NewCommand,
    tempo_down: TempoCommand,
    tempo_up: TempoCommand,
}

impl XlMainMode {
    fn create(name: &str, kind: MainKind, bank: ModeBank, controls: Vec<ContinuousId>, default_mode: Modes, providers: (SharedProvider, SharedProvider)) -> Self {
        XlMainMode {
            base: ParameterModeBase::new(name, true, bank, controls),
            kind,
            default_mode,
            track_mode: None,
            is_active: false,
            parameter_provider: providers.0,
            parameter_provider_with_device_params: providers.1,
            was_long: false,
            transport_used: false,
            new_command: NewCommand::new(),
            tempo_down: TempoCommand::button(false),
            tempo_up: TempoCommand::button(true),
        }
    }

    /// Former `XLTrackMixMode`.
    pub fn track_mix(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let send1 = provider(SendParameterProvider::new(ctx, 0, 0));
        let send2 = provider(SendParameterProvider::new(ctx, 1, 0));
        let pan = provider(PanParameterProvider::new(ctx));
        let device_param_providers = TRACK_MIX_BANKS.map(|bank| provider(BankParameterProvider::new(ctx, bank)));
        // Control sends and pan
        let first = provider(CombinedParameterProvider::new(vec![send1.clone(), send2.clone(), pan]));
        // Control sends and device parameters
        let second = provider(CombinedParameterProvider::new(vec![send1.clone(), send2.clone(), device_param_providers[0].clone()]));
        let kind = MainKind::TrackMix {
            selected_device_param_provider_index: 0,
            device_param_providers,
            send_parameter_provider1: send1,
            send_parameter_provider2: send2,
        };
        let mut mode = Self::create("Send A, B & Panning", kind, ModeBank::TrackBank, controls, Modes::Mute, (first.clone(), second.clone()));
        mode.set_parameter_providers(ctx, first, second);
        mode
    }

    /// Former `XLEqMode`.
    pub fn eq(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let mut eq_parameters: Vec<ParamRef> = (0..8).map(|band| ParamRef::EqBandType { band }).collect();
        eq_parameters.extend((0..8).map(|i| ParamRef::BankItem { bank: ParameterBankId::Eq, index: SpecificDevice::q_parameter_index(i) }));
        let frequency_parameters = (0..8).map(|i| ParamRef::BankItem { bank: ParameterBankId::Eq, index: SpecificDevice::frequency_parameter_index(i) }).collect();
        let eq_parameter_provider = provider(FixedParameterProvider::new(eq_parameters));
        let frequency_parameter_provider = provider(FixedParameterProvider::new(frequency_parameters));
        let device_parameter_provider = provider(BankParameterProvider::new(ctx, ParameterBankId::CursorDevice));
        // Control EQ type, Q and frequency
        let first = provider(CombinedParameterProvider::new(vec![eq_parameter_provider.clone(), frequency_parameter_provider]));
        // Control EQ type, Q and device parameters
        let second = provider(CombinedParameterProvider::new(vec![eq_parameter_provider, device_parameter_provider]));
        let mut mode = Self::create("Equalizer", MainKind::Eq, ModeBank::TrackBank, controls, Modes::Mute, (first.clone(), second.clone()));
        mode.set_parameter_providers(ctx, first, second);
        mode
    }

    /// Former `XLLayerMixMode`. Reaper always has drum pads, therefore the bank is the drum pad
    /// bank of the first instrument (former `parametersAdjusted`).
    pub fn layer_mix(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let send1 = provider(LayerOrDrumPadParameterProvider::send(ctx, 0));
        let send2 = provider(LayerOrDrumPadParameterProvider::send(ctx, 1));
        let pan = provider(LayerOrDrumPadParameterProvider::pan(ctx));
        let device = provider(BankParameterProvider::new(ctx, ParameterBankId::FirstInstrument));
        // Control sends and pan
        let first = provider(CombinedParameterProvider::new(vec![send1.clone(), send2.clone(), pan]));
        // Control sends and device parameters
        let second = provider(CombinedParameterProvider::new(vec![send1, send2, device]));
        let bank = ModeBank::DrumPadBank(DrumDeviceRef::FirstInstrument);
        let mut mode = Self::create("Layer", MainKind::LayerMix, bank, controls, Modes::DeviceLayerMute, (first.clone(), second.clone()));
        mode.set_parameter_providers(ctx, first, second);
        mode
    }

    /// The note parameter providers of the note edit modes (former `XLBaseNoteEditMode`).
    fn note_providers(is_drum: bool, attribute: XlNoteAttribute) -> SharedProvider {
        let parameters = (0..8).map(|i| ParamRef::Custom(std::rc::Rc::new(XlNoteParameter::new(i, attribute, is_drum)))).collect();
        provider(FixedParameterProvider::new(parameters))
    }

    /// Former `XLBaseNoteEditMode` + `XLDrumPadEditMode` / `XLNoteEditMode`. The Rec Arm button
    /// must already exist.
    fn note_edit(ctx: &mut Ctx, controls: Vec<ContinuousId>, is_drum: bool) -> Self {
        ctx.model.scales_mut().set_drum_default_offset(8);
        let chance = Self::note_providers(is_drum, XlNoteAttribute::Chance);
        let repeat = Self::note_providers(is_drum, XlNoteAttribute::Repeat);
        let velocity_spread = Self::note_providers(is_drum, XlNoteAttribute::VelocitySpread);
        let pan = Self::note_providers(is_drum, XlNoteAttribute::Panning);
        let device = provider(BankParameterProvider::new(ctx, ParameterBankId::FirstInstrument));

        let first_row = if is_drum { chance.clone() } else { Self::note_providers(is_drum, XlNoteAttribute::Pitch) };
        let note_edit_provider = provider(CombinedParameterProvider::new(vec![first_row.clone(), repeat.clone(), pan.clone()]));
        let note_edit_with_device_params_provider = provider(CombinedParameterProvider::new(vec![first_row, repeat, device]));
        let shifted_parameter_provider = provider(CombinedParameterProvider::new(vec![chance, velocity_spread, pan]));

        let (name, kind, default_mode) =
            if is_drum { ("Drum Sequencer", MainKind::DrumPadEdit, Modes::DrumSequencer) } else { ("Note Sequencer", MainKind::NoteEdit, Modes::NoteSequencer) };
        let mut mode =
            Self::create(name, kind, ModeBank::None, controls, default_mode, (note_edit_provider.clone(), note_edit_with_device_params_provider.clone()));
        mode.set_parameter_providers(ctx, note_edit_provider, note_edit_with_device_params_provider);
        mode.base.set_button_parameter_provider(ctx, ButtonId::RecArm, shifted_parameter_provider);

        if is_drum {
            let drum_offset = ctx.model.scales().drum_offset();
            xl::xl_mut(ctx).drum_note = drum_offset;
        }
        // Force clip creation
        ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
        mode
    }

    /// Former `XLDrumPadEditMode`.
    pub fn drum_pad_edit(ctx: &mut Ctx, controls: Vec<ContinuousId>) -> Self {
        Self::note_edit(ctx, controls, true)
    }

    /// Former `XLNoteEditMode`.
    pub fn note_edit_mode(ctx: &mut Ctx, controls: Vec<ContinuousId>) -> Self {
        Self::note_edit(ctx, controls, false)
    }

    fn is_note_edit(&self) -> bool {
        matches!(self.kind, MainKind::DrumPadEdit | MainKind::NoteEdit)
    }

    fn is_track(&self) -> bool {
        matches!(self.kind, MainKind::TrackMix { .. } | MainKind::Eq)
    }

    // --------------------------------------------------------------------------------------
    // XLAbstractMainMode

    /// Former `setParameterProviders`.
    fn set_parameter_providers(&mut self, ctx: &Ctx, parameter_provider: SharedProvider, parameter_provider_with_device_params: SharedProvider) {
        self.parameter_provider = parameter_provider;
        self.parameter_provider_with_device_params = parameter_provider_with_device_params;
        self.update_provider(ctx);
    }

    fn update_provider(&mut self, ctx: &Ctx) {
        let provider =
            if xl::xl(ctx).is_device_active { self.parameter_provider_with_device_params.clone() } else { self.parameter_provider.clone() };
        self.base.set_parameter_provider(ctx, provider);
    }

    /// Former `toggleDeviceActive`.
    pub fn toggle_device_active(&mut self, ctx: &mut Ctx) {
        xl::toggle_device_active(ctx);
        self.update_provider(ctx);
        self.base.bind_controls(ctx);
    }

    /// Former `handleRow0`.
    fn handle_row0(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        let fader_modes = xl::fader_modes(ctx);
        match event {
            ButtonEvent::Down => {
                if ctx.surface.is_pressed(ButtonId::RecArm) {
                    self.transport_used = true;
                    self.handle_transport(ctx, index, event);
                    return;
                }
                self.transport_used = false;
                self.was_long = false;
                ctx.set_temporary_mode_in(fader_modes, Modes::Master);
            }
            ButtonEvent::Long => self.was_long = true,
            ButtonEvent::Up => {
                if self.transport_used {
                    self.handle_transport(ctx, index, event);
                    return;
                }
                ctx.restore_mode_in(fader_modes);
                if !self.was_long {
                    self.execute_row0(ctx, index);
                }
            }
        }
    }

    /// Former `alternativeModeSelect`.
    fn alternative_mode_select(ctx: &mut Ctx, event: ButtonEvent, mode_id: Option<Modes>, alt_mode_id: Modes) {
        let track_modes = track_modes(ctx);
        if event == ButtonEvent::Down {
            ctx.set_temporary_mode_in(track_modes, alt_mode_id);
            return;
        }
        if event == ButtonEvent::Up {
            ctx.restore_mode_in(track_modes);
            if has_been_used(ctx, alt_mode_id) {
                return;
            }
            if let Some(mode_id) = mode_id {
                ctx.set_active_mode_in(track_modes, mode_id);
            }
        }
    }

    /// Former `handleTransport`.
    fn handle_transport(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        let is_down = event == ButtonEvent::Down;
        match index {
            // New clip
            0 if is_down => {
                self.new_command.handle_execute(ctx, false);
            }
            // Toggle Launcher Overdub
            1 if is_down => {
                ctx.model.toggle_launcher_overdub();
            }
            // Decrease tempo
            2 => self.tempo_down.execute_button(ctx, event),
            // Increase tempo
            3 => self.tempo_up.execute_button(ctx, event),
            // Undo
            4 if is_down => {
                ctx.model.application_mut().undo();
            }
            // Redo
            5 if is_down => {
                ctx.model.application_mut().redo();
            }
            // Toggle Clip Automation Write
            6 if is_down => {
                ctx.model.transport_mut().toggle_write_clip_launcher_automation();
            }
            // Toggle plug-in window
            7 if is_down => {
                ctx.model.cursor_device_mut().toggle_window_open();
            }
            _ => {}
        }
    }

    /// Former `getTransportButtonColor`.
    fn transport_button_color(ctx: &Ctx, index: usize) -> i32 {
        let transport = ctx.model.transport();
        match index {
            // New clip
            0 => LAUNCHCONTROL_COLOR_YELLOW_LO,
            // Toggle Launcher Overdub
            1 => {
                if ctx.model.is_launcher_overdub() {
                    LAUNCHCONTROL_COLOR_AMBER
                } else {
                    LAUNCHCONTROL_COLOR_AMBER_LO
                }
            }
            // Tempo
            2 | 3 => LAUNCHCONTROL_COLOR_GREEN_LO,
            // Undo
            4 => {
                if ctx.model.application().can_undo() {
                    LAUNCHCONTROL_COLOR_YELLOW_LO
                } else {
                    LAUNCHCONTROL_COLOR_BLACK
                }
            }
            // Redo
            5 => {
                if ctx.model.application().can_redo() {
                    LAUNCHCONTROL_COLOR_YELLOW_LO
                } else {
                    LAUNCHCONTROL_COLOR_BLACK
                }
            }
            // Toggle Clip Automation Write
            6 => {
                if transport.is_writing_clip_launcher_automation() {
                    LAUNCHCONTROL_COLOR_RED
                } else {
                    LAUNCHCONTROL_COLOR_RED_LO
                }
            }
            7 => {
                if ctx.model.cursor_device().is_window_open() {
                    LAUNCHCONTROL_COLOR_GREEN
                } else {
                    LAUNCHCONTROL_COLOR_GREEN_LO
                }
            }
            _ => LAUNCHCONTROL_COLOR_BLACK,
        }
    }

    // --------------------------------------------------------------------------------------
    // Row 0 and 2 of the concrete modes

    /// Former `executeRow0`.
    fn execute_row0(&mut self, ctx: &mut Ctx, index: usize) {
        match self.kind {
            MainKind::TrackMix { .. } | MainKind::Eq => {
                let track = ctx.model.track_bank().item(index);
                if !track.does_exist() {
                    return;
                }
                if track.is_selected() && track.is_group() {
                    ctx.model.track_bank_mut().item_mut(index).toggle_group_expanded();
                    return;
                }
                ctx.model.track_bank_mut().item_mut(index).select();
                mv_helper::notify_selected_track(ctx);
            }
            MainKind::LayerMix => {
                if !self.layer_exists(ctx, index) {
                    return;
                }
                self.base.select_item(ctx, index);
                mv_helper::notify_selected_layer(ctx);
            }
            MainKind::DrumPadEdit => {
                let note = ctx.model.scales().drum_offset() + index as i32;
                let state = xl::xl_mut(ctx);
                state.drum_note = note;
                state.selected_pad = index;
                if let Some(device) = ctx.model.drum_device() {
                    let pad = device.drum_pad_bank().item(index);
                    if pad.does_exist() {
                        ctx.host.show_notification(&format!("Drum Pad: {}", pad.name()));
                    }
                }
            }
            MainKind::NoteEdit => {
                let channel = ctx.config.midi_edit_channel();
                let note_row = note_position(ctx, false, index).note;
                let scale_offset = ctx.model.scales().scale_offset();
                let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
                if note_row == -1 {
                    // Use the note of the currently selected scale base
                    clip.toggle_step(&NotePosition::new(channel, index as i32, 60 + scale_offset), 127);
                } else {
                    clip.clear_step(&NotePosition::new(channel, index as i32, note_row));
                }
            }
        }
    }

    /// Handle the device button with the given temporary device parameter page mode.
    fn handle_device_button(&mut self, ctx: &mut Ctx, event: ButtonEvent, temp_mode: Modes) {
        let track_modes = track_modes(ctx);
        if event == ButtonEvent::Down {
            ctx.set_temporary_mode_in(track_modes, temp_mode);
            return;
        }
        if event == ButtonEvent::Up {
            if !has_been_used(ctx, temp_mode) {
                self.toggle_device_active(ctx);
            }
            ctx.restore_mode_in(track_modes);
        }
    }

    /// Former `handleRow2`.
    fn handle_row2(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        let track_modes = track_modes(ctx);
        match self.kind {
            MainKind::TrackMix { .. } | MainKind::Eq => match index {
                // Device button
                0 => self.handle_device_button(ctx, event, Modes::DeviceParams),
                // Mute button
                1 => {
                    if event == ButtonEvent::Down {
                        ctx.set_active_mode_in(track_modes, Modes::Mute);
                    }
                }
                // Solo button
                2 => Self::alternative_mode_select(ctx, event, Some(Modes::Solo), Modes::Clip),
                // Record Arm button
                3 => Self::alternative_mode_select(ctx, event, Some(Modes::RecArm), Modes::Transport),
                _ => {}
            },
            MainKind::LayerMix => match index {
                0 => self.handle_device_button(ctx, event, Modes::InstrumentDeviceParams),
                1 => {
                    if event == ButtonEvent::Down {
                        ctx.set_active_mode_in(track_modes, Modes::DeviceLayerMute);
                    }
                }
                2 => Self::alternative_mode_select(ctx, event, Some(Modes::DeviceLayerSolo), Modes::Clip),
                3 => Self::alternative_mode_select(ctx, event, None, Modes::Transport),
                _ => {}
            },
            MainKind::DrumPadEdit | MainKind::NoteEdit => {
                let active = ctx.mode_manager(track_modes).active_id_ignore_temporary();
                let sequencer_mode = self.default_mode;
                let toggle = |mode: Modes| if active == Some(mode) { sequencer_mode } else { mode };
                match index {
                    0 => self.handle_device_button(ctx, event, Modes::InstrumentDeviceParams),
                    1 => Self::alternative_mode_select(ctx, event, Some(toggle(Modes::DeviceLayerMute)), Modes::Configuration),
                    2 => Self::alternative_mode_select(ctx, event, Some(toggle(Modes::DeviceLayerSolo)), Modes::Clip),
                    3 => Self::alternative_mode_select(ctx, event, Some(toggle(Modes::LoopLength)), Modes::Transport),
                    _ => {}
                }
            }
        }
    }

    // --------------------------------------------------------------------------------------
    // Knobs

    fn layer_exists(&self, ctx: &Ctx, index: usize) -> bool {
        crate::framework::mode::layer::layer(ctx, index).is_some_and(|l| l.does_exist())
    }

    fn eq_device(ctx: &Ctx) -> Option<&SpecificDevice> {
        ctx.model.specific_device(DeviceId::Eq)
    }

    fn bank_value(ctx: &Ctx, bank: ParameterBankId, index: usize) -> i32 {
        parameter_bank(&ctx.model, bank).map_or(0, |b| b.item(index).value())
    }

    /// Former `getParamBank` of the track mix mode.
    fn track_mix_param_bank(&self) -> ParameterBankId {
        match self.kind {
            MainKind::TrackMix { selected_device_param_provider_index, .. } => TRACK_MIX_BANKS[selected_device_param_provider_index],
            _ => ParameterBankId::CursorDevice,
        }
    }

    /// Former `XLTrackMixMode.selectParameterMode`.
    pub fn select_parameter_mode(&mut self, ctx: &mut Ctx, increase: bool) {
        let MainKind::TrackMix { selected_device_param_provider_index, device_param_providers, send_parameter_provider1, send_parameter_provider2 } =
            &mut self.kind
        else {
            return;
        };
        let index = if increase { (*selected_device_param_provider_index + 1).min(2) } else { selected_device_param_provider_index.saturating_sub(1) };
        *selected_device_param_provider_index = index;
        // Former updateDeviceParametersProvider
        let second = provider(CombinedParameterProvider::new(vec![
            send_parameter_provider1.clone(),
            send_parameter_provider2.clone(),
            device_param_providers[index].clone(),
        ]));
        let first = self.parameter_provider.clone();
        self.set_parameter_providers(ctx, first, second);
        self.base.bind_controls(ctx);
        ctx.host.show_notification(MODE_NAMES[index]);
    }

    /// Former `IXLMode.setKnobColor`.
    pub fn set_knob_color(&self, ctx: &Ctx, row: usize, column: usize, value: i32) {
        let is_device_active = xl::xl(ctx).is_device_active;
        let (mut green, mut red) = (0, 0);
        match row {
            // Send A / Chance intensity in green; EQ type
            0 => {
                if let MainKind::Eq = self.kind {
                    // Center is Bell (green). Left side chooses between different low-cuts (red)
                    // and right side different high-cuts (yellow). Fully left turns the band
                    // off (black). Fully right sets it to notch (amber).
                    let band_type = Self::eq_device(ctx).map_or(EqualizerBandType::Off, |d| d.type_id(column));
                    (green, red) = match band_type {
                        EqualizerBandType::Bell => (3, 0),
                        EqualizerBandType::Highcut => (3, 1),
                        EqualizerBandType::Highshelf => (2, 1),
                        EqualizerBandType::Lowshelf => (0, 3),
                        EqualizerBandType::Lowcut => (0, 2),
                        EqualizerBandType::Notch => (3, 3),
                        EqualizerBandType::Off => (0, 0),
                    };
                } else {
                    green = intensity(value);
                }
            }
            // Send B / Repeat intensity in red, EQ Q in green
            1 => {
                if let MainKind::Eq = self.kind {
                    green = intensity(value);
                } else {
                    red = intensity(value);
                }
            }
            // Panning (EQ frequency) in amber or Device parameters yellowish
            2 => {
                green = intensity(value);
                red = if is_device_active {
                    if green == 0 {
                        0
                    } else {
                        1
                    }
                } else {
                    green
                };
            }
            // Not used
            _ => return,
        }
        xl::set_knob_led_color(ctx, row, column, green, red);
    }

    /// Former `getFirstRowColor` of the note edit modes.
    fn first_row_color(&self, ctx: &Ctx, index: usize) -> i32 {
        match self.kind {
            MainKind::DrumPadEdit => ctx.colors.color_index(Self::drum_pad_color(ctx, index)),
            _ => {
                if note_position(ctx, false, index).note < 0 {
                    LAUNCHCONTROL_COLOR_BLACK
                } else {
                    LAUNCHCONTROL_COLOR_AMBER
                }
            }
        }
    }

    /// Former `XLDrumPadEditMode.getDrumPadColor`. The key manager of the Java version is never
    /// updated (the controller has no pads), therefore the 'playing' state is not used.
    fn drum_pad_color(ctx: &Ctx, index: usize) -> &'static str {
        // Selected?
        if xl::xl(ctx).selected_pad == index {
            return drum::COLOR_PAD_SELECTED;
        }
        // Exists and active?
        let Some(device) = ctx.model.drum_device() else {
            return drum::COLOR_PAD_NO_CONTENT;
        };
        let drum_pad_bank = device.drum_pad_bank();
        let drum_pad = drum_pad_bank.item(index);
        if !drum_pad.does_exist() || !drum_pad.is_activated() {
            return if ctx.config.is_turn_off_empty_drum_pads() { drum::COLOR_PAD_OFF } else { drum::COLOR_PAD_NO_CONTENT };
        }
        // Muted or soloed?
        if drum_pad.is_mute() || drum_pad_bank.has_soloed_pads() && !drum_pad.is_solo() {
            return drum::COLOR_PAD_MUTED;
        }
        drum::COLOR_PAD_HAS_CONTENT
    }

    /// Former `XLDrumPadEditMode.adjustDrumDevice`.
    fn adjust_drum_device(ctx: &mut Ctx) {
        let offset = ctx.model.scales().drum_offset().max(0) as usize;
        if let Some(device) = ctx.model.drum_device_mut() {
            device.drum_pad_bank_mut().scroll_to(offset, false);
        }
        let text = ctx.model.scales().drum_range_text();
        ctx.host.show_notification(&text);
    }

    fn update_scale_properties(ctx: &mut Ctx) {
        let name = ctx.model.scales().scale().name();
        ctx.config.set_scale(name);
    }

    // --------------------------------------------------------------------------------------
    // Send pages of tracks and layers

    fn select_send_page(ctx: &mut Ctx, is_layer: bool, next: bool) {
        for i in 0..8 {
            if is_layer {
                if let Some(layer) = crate::framework::mode::layer::layer_mut(ctx, i) {
                    let send_bank = layer.send_bank_mut();
                    if next {
                        send_bank.select_next_page();
                    } else {
                        send_bank.select_previous_page();
                    }
                }
            } else {
                let send_bank = ctx.model.track_bank_mut().item_mut(i).send_bank_mut();
                if next {
                    send_bank.select_next_page();
                } else {
                    send_bank.select_previous_page();
                }
            }
        }
        if is_layer {
            mv_helper::notify_selected_layer_sends(ctx, 0);
        } else {
            mv_helper::notify_selected_sends(ctx, 0);
        }
    }

    fn can_scroll_send_page(ctx: &Ctx, is_layer: bool, next: bool) -> bool {
        (0..8).any(|i| {
            let send_bank = if is_layer {
                match crate::framework::mode::layer::layer(ctx, i) {
                    Some(layer) => layer.send_bank(),
                    None => return false,
                }
            } else {
                ctx.model.track_bank().item(i).send_bank()
            };
            if next {
                send_bank.can_scroll_page_forwards()
            } else {
                send_bank.can_scroll_page_backwards()
            }
        })
    }

    /// Select the previous/next parameter bank of the track mix mode while Device is pressed.
    fn select_parameter_mode_of_track_mix(&mut self, ctx: &mut Ctx, increase: bool) {
        set_active_track_mode_used(ctx);
        if let MainKind::TrackMix { .. } = self.kind {
            self.select_parameter_mode(ctx, increase);
            return;
        }
        if let Some(mode) = ctx.mode_manager(MAIN_MODE_MANAGER).get(Modes::Send) {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                if let Some(mix_mode) = mode.as_any_mut().downcast_mut::<XlMainMode>() {
                    mix_mode.select_parameter_mode(ctx, increase);
                }
            }
        }
    }

    /// Handle the Device and Solo combinations of the page navigation. Returns true if one was
    /// executed.
    fn handle_page_combination(&mut self, ctx: &mut Ctx, next: bool, consume: bool) -> bool {
        let pressed = |ctx: &mut Ctx, button: ButtonId| if consume { ctx.is_button_combination(button) } else { ctx.surface.is_pressed(button) };
        if !matches!(self.kind, MainKind::LayerMix) && pressed(ctx, ButtonId::Device) {
            set_active_track_mode_used(ctx);
            if next {
                ctx.model.cursor_device_mut().select_next();
            } else {
                ctx.model.cursor_device_mut().select_previous();
            }
            mv_helper::notify_selected_device(ctx);
            return true;
        }
        if pressed(ctx, ButtonId::Solo) {
            set_active_track_mode_used(ctx);
            {
                let mut scene_bank = ctx.model.scene_bank_mut();
                if next {
                    scene_bank.select_next_page();
                } else {
                    scene_bank.select_previous_page();
                }
            }
            mv_helper::notify_scene_page(ctx);
            return true;
        }
        false
    }

    fn select_item_page(&mut self, ctx: &mut Ctx, next: bool) {
        // As in Java, the previous page consumes the combination button, the next page not
        if self.handle_page_combination(ctx, next, !next) {
            return;
        }
        match self.kind {
            MainKind::TrackMix { .. } | MainKind::Eq | MainKind::LayerMix => {
                if next {
                    self.base.select_next_item_page(ctx);
                } else {
                    self.base.select_previous_item_page(ctx);
                }
                if let MainKind::LayerMix = self.kind {
                    mv_helper::notify_selected_layer(ctx);
                } else {
                    mv_helper::notify_selected_track(ctx);
                }
            }
            MainKind::DrumPadEdit | MainKind::NoteEdit => {
                let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
                if next {
                    clip.scroll_steps_page_forward();
                } else {
                    clip.scroll_steps_page_backwards();
                }
                mv_helper::notify_edit_page(ctx, CLIP_KEY);
            }
        }
    }

    fn has_item_page(&self, ctx: &Ctx, next: bool) -> bool {
        if !matches!(self.kind, MainKind::LayerMix) && ctx.surface.is_pressed(ButtonId::Device) {
            let device = ctx.model.cursor_device();
            return if next { device.can_select_next() } else { device.can_select_previous() };
        }
        if ctx.surface.is_pressed(ButtonId::Solo) {
            let scene_bank = ctx.model.scene_bank();
            return if next { scene_bank.can_scroll_page_forwards() } else { scene_bank.can_scroll_page_backwards() };
        }
        match self.kind {
            MainKind::DrumPadEdit | MainKind::NoteEdit => true,
            _ => {
                if next {
                    self.base.has_next_item_page(ctx)
                } else {
                    self.base.has_previous_item_page(ctx)
                }
            }
        }
    }

    fn select_item(&mut self, ctx: &mut Ctx, next: bool) {
        match self.kind {
            MainKind::TrackMix { .. } | MainKind::Eq => {
                if ctx.surface.is_pressed(ButtonId::Device) {
                    self.select_parameter_mode_of_track_mix(ctx, next);
                    return;
                }
                Self::select_send_page(ctx, false, next);
            }
            MainKind::LayerMix => Self::select_send_page(ctx, true, next),
            MainKind::DrumPadEdit => {
                let scales = ctx.model.scales_mut();
                if next {
                    scales.inc_drum_octave();
                } else {
                    scales.dec_drum_octave();
                }
                Self::adjust_drum_device(ctx);
            }
            MainKind::NoteEdit => {
                let scales = ctx.model.scales_mut();
                if next {
                    scales.next_scale();
                } else {
                    scales.prev_scale();
                }
                Self::update_scale_properties(ctx);
                mv_helper::notify_scale(ctx);
            }
        }
    }

    fn has_item(&self, ctx: &Ctx, next: bool) -> bool {
        match self.kind {
            MainKind::TrackMix { .. } | MainKind::Eq => ctx.surface.is_pressed(ButtonId::Device) || Self::can_scroll_send_page(ctx, false, next),
            MainKind::LayerMix => Self::can_scroll_send_page(ctx, true, next),
            MainKind::DrumPadEdit => {
                let scales = ctx.model.scales();
                if next {
                    scales.can_scroll_drum_octave_up()
                } else {
                    scales.can_scroll_drum_octave_down()
                }
            }
            MainKind::NoteEdit => {
                let scales = ctx.model.scales();
                if next {
                    scales.has_next_scale()
                } else {
                    scales.has_prev_scale()
                }
            }
        }
    }
}

impl Mode for XlMainMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        // Make sure there is the correct provider if the settings has changed in another main mode
        self.update_provider(ctx);
        // The app activates the mode again when the parameters of the device change (former
        // rebindKnobs of the Java version for Reaper, which then also resets the track button
        // mode; the Java version for Bitwig does not do this)
        if !self.is_active {
            self.is_active = true;
            // XLEqMode.onActivate; not on the repeated activations, the equalizer added by the
            // first might not be reported yet
            if let MainKind::Eq = self.kind {
                crate::controller::launchcontrol::setup::add_missing_equalizer(ctx);
            }
            let track_modes = track_modes(ctx);
            ctx.set_active_mode_in(track_modes, self.track_mode.unwrap_or(self.default_mode));
        }
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.is_active = false;
        self.track_mode = ctx.mode_manager(track_modes(ctx)).active_id();
        self.base.on_deactivate(ctx);
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !self.is_note_edit() {
            return;
        }
        // Row 2 starts with index 8, row 4 with 16. This is only triggered from the faders.
        if index / 8 != 3 {
            return;
        }
        let note_position = note_position(ctx, matches!(self.kind, MainKind::DrumPadEdit), index);
        if note_position.note < 0 {
            return;
        }
        let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
        if clip.step(&note_position).state() == StepState::Off {
            return;
        }
        let normalized_value = ctx.model.value_changer().to_normalized_value(value as f64);
        ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1).update_step_velocity(&note_position, normalized_value);
        ctx.notify(&format!("Velocity: {}", format_percentage(normalized_value)));
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        let row = index / 8;
        let column = index % 8;
        let is_device_active = xl::xl(ctx).is_device_active;
        match self.kind {
            MainKind::TrackMix { .. } => {
                let track = ctx.model.track_bank().item(column);
                match row {
                    0 => track.send_bank().item(0).value(),
                    1 => track.send_bank().item(1).value(),
                    2 => {
                        if is_device_active {
                            Self::bank_value(ctx, self.track_mix_param_bank(), column)
                        } else {
                            track.pan()
                        }
                    }
                    _ => 0,
                }
            }
            MainKind::Eq => match row {
                0 => ParamRef::EqBandType { band: column }.value(ctx),
                1 => Self::bank_value(ctx, ParameterBankId::Eq, SpecificDevice::q_parameter_index(column)),
                2 => {
                    if is_device_active {
                        Self::bank_value(ctx, ParameterBankId::CursorDevice, column)
                    } else {
                        Self::bank_value(ctx, ParameterBankId::Eq, SpecificDevice::frequency_parameter_index(column))
                    }
                }
                _ => 0,
            },
            MainKind::LayerMix => {
                let Some(layer) = crate::framework::mode::layer::layer(ctx, column) else {
                    return 0;
                };
                match row {
                    0 => layer.send_bank().item(0).value(),
                    1 => layer.send_bank().item(1).value(),
                    2 => {
                        if is_device_active {
                            Self::bank_value(ctx, ParameterBankId::FirstInstrument, column)
                        } else {
                            layer.pan()
                        }
                    }
                    _ => 0,
                }
            }
            MainKind::DrumPadEdit | MainKind::NoteEdit => self.base.knob_value(ctx, index),
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.handle_row0(ctx, index, event);
        } else if row == 2 {
            self.handle_row2(ctx, index, event);
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let track_modes = ctx.mode_manager(track_modes(ctx));
        let on_off = |on: bool| if on { 127 } else { 0 };
        match button {
            ButtonId::Device => on_off(xl::xl(ctx).is_device_active),
            ButtonId::Mute => match self.kind {
                MainKind::TrackMix { .. } | MainKind::Eq => on_off(track_modes.is_active(Modes::Mute)),
                _ => on_off(track_modes.is_active(Modes::DeviceLayerMute)),
            },
            ButtonId::Solo => match self.kind {
                MainKind::TrackMix { .. } | MainKind::Eq => on_off(track_modes.is_active(Modes::Solo)),
                _ => on_off(track_modes.is_active(Modes::DeviceLayerSolo)),
            },
            ButtonId::RecArm => match self.kind {
                MainKind::TrackMix { .. } | MainKind::Eq => on_off(track_modes.is_active(Modes::RecArm)),
                MainKind::LayerMix => LAUNCHCONTROL_COLOR_BLACK,
                _ => on_off(track_modes.is_active(Modes::LoopLength)),
            },
            _ if button >= ButtonId::Row11 && button <= ButtonId::Row18 => {
                let index = button.ordinal() - ButtonId::Row11.ordinal();
                if ctx.surface.is_pressed(ButtonId::RecArm) {
                    return Self::transport_button_color(ctx, index);
                }
                if self.is_track() {
                    let track = ctx.model.track_bank().item(index);
                    if track.does_exist() {
                        return if track.is_selected() { LAUNCHCONTROL_COLOR_YELLOW } else { LAUNCHCONTROL_COLOR_YELLOW_LO };
                    }
                    return LAUNCHCONTROL_COLOR_BLACK;
                }
                if let MainKind::LayerMix = self.kind {
                    return match crate::framework::mode::layer::layer(ctx, index) {
                        Some(layer) if layer.does_exist() => {
                            if layer.is_selected() {
                                LAUNCHCONTROL_COLOR_YELLOW
                            } else {
                                LAUNCHCONTROL_COLOR_YELLOW_LO
                            }
                        }
                        _ => LAUNCHCONTROL_COLOR_BLACK,
                    };
                }
                self.first_row_color(ctx, index)
            }
            _ => LAUNCHCONTROL_COLOR_BLACK,
        }
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.select_item(ctx, false);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.select_item(ctx, true);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.has_item(ctx, false)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.has_item(ctx, true)
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.select_item_page(ctx, false);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.select_item_page(ctx, true);
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.has_item_page(ctx, false)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.has_item_page(ctx, true)
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// Dummy mode

/// The mode of the user templates: the knobs do nothing and their LEDs are off (former
/// `XLDummyMode`).
pub struct XlDummyMode {
    mode: DummyMode,
}

impl XlDummyMode {
    pub fn new(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        XlDummyMode { mode: DummyMode::new(ctx, controls) }
    }

    /// Former `setKnobColor`: turns the LED off.
    pub fn set_knob_color(&self, ctx: &Ctx, row: usize, column: usize, _value: i32) {
        xl::set_knob_led_color(ctx, row, column, 0, 0);
    }
}

impl Mode for XlDummyMode {
    fn name(&self) -> &str {
        self.mode.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.mode.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.mode.on_deactivate(ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.mode.knob_value(ctx, index)
    }

    crate::impl_as_any!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(intensity(0), 0);
        assert_eq!(intensity(1), 1);
        assert_eq!(intensity(42), 2);
        assert_eq!(intensity(127), 4);
        assert_eq!(format_percentage(0.5), "50%");
        assert_eq!(format_percentage(0.123), "12.3%");
    }
}
