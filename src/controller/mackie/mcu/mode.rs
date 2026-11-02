// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the MCU (former `BaseMode` and its subclasses in `mcu.mode`: the track modes
//! `TrackMode`, `VolumeMode`, `PanMode`, `SendMode`, `MasterMode`, the layer modes `LayerMode`,
//! `LayerVolumeMode`, `LayerPanMode`, `LayerSendMode`, the device modes `DeviceParamsMode`,
//! `MCUProjectParametersMode`, `MCUTrackParametersMode`, `DeviceBrowserMode` and `MarkerMode`),
//! and the master fader mode (former framework `MasterVolumeMode`).
//!
//! Differences to Java:
//! - All mode classes are one type [`McuMode`] with a [`McuModeKind`]: the setup needs to
//!   access the knob LEDs, the touch states and the parameter providers of the active mode
//!   (former `instanceof BaseMode`, `getParameterProvider`), which is a downcast to one type.
//!   The overridden Java methods are the match arms of the kind.
//! - "Pin FX tracks to last device" is not supported (Reaper has no effect track bank, the
//!   setting is not activated there).
//! - Switching the mode from inside a mode (Master mode restores on a button press, the send
//!   modes switch to the track mode if the effect track bank is active) is deferred with
//!   `ctx.schedule (0, ..)`, since the mode is borrowed while it runs.

use std::rc::Rc;

use super::definition::{KNOB_LED_MODE_BOOST_CUT, KNOB_LED_MODE_SINGLE_DOT, KNOB_LED_MODE_WRAP};
use super::surface::{extender_offset, mcu, mcu_mut, send_display_color, set_item_indices, set_knob_led};
use crate::config::mcu::{MainDisplay, McuConfig, SecondDisplay};
use crate::controller::mackie::shorten_and_fix_ascii;
use crate::daw::bank::BankItem;
use crate::daw::channel::Channel;
use crate::daw::color::ColorEx;
use crate::daw::constants::DeviceId;
use crate::daw::events::ParameterBankId;
use crate::framework::commands::general::{FocusedParameter, ZoomParameter};
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, ContinuousId, Modes};
use crate::framework::mode::parameter_mode::{default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{
    provider, BankParameterProvider, DrumDeviceRef, EmptyParameterProvider, FixedParameterProvider, FnParameterProvider, LayerOrDrumPadParameterProvider,
    PanParameterProvider, ParamRef, RangeFilterParameterProvider, SelectedTrackParameterProvider, SendParameterProvider, SharedProvider,
    VolumeParameterProvider,
};
use crate::midi::ButtonEvent;

/// The specific device of a device parameter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Cursor,
    Eq,
    FirstInstrument,
}

impl DeviceKind {
    fn bank(self) -> ParameterBankId {
        match self {
            DeviceKind::Cursor => ParameterBankId::CursorDevice,
            DeviceKind::Eq => ParameterBankId::Eq,
            DeviceKind::FirstInstrument => ParameterBankId::FirstInstrument,
        }
    }

    fn name(self) -> &'static str {
        match self {
            DeviceKind::Cursor => "Device Parameters",
            DeviceKind::Eq => "Equalizer",
            DeviceKind::FirstInstrument => "First Instrument",
        }
    }

    fn device(self, ctx: &Ctx) -> Option<&crate::daw::device::SpecificDevice> {
        match self {
            DeviceKind::Cursor => Some(ctx.model.cursor_device()),
            DeviceKind::Eq => ctx.model.specific_device(DeviceId::Eq),
            DeviceKind::FirstInstrument => ctx.model.specific_device(DeviceId::FirstInstrument),
        }
    }
}

/// Which Java mode class a [`McuMode`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McuModeKind {
    Track,
    Volume,
    Pan,
    Send(usize),
    Master,
    Layer,
    LayerVolume,
    LayerPan,
    LayerSend(usize),
    Device(DeviceKind),
    ProjectParameters,
    TrackParameters,
    Browser,
    Markers,
}

impl McuModeKind {
    fn name(self) -> &'static str {
        match self {
            McuModeKind::Track => "Track",
            McuModeKind::Volume => "Volume",
            McuModeKind::Pan => "Panning",
            McuModeKind::Send(_) => "Sends",
            McuModeKind::Master => "Master",
            McuModeKind::Layer => "Layer",
            McuModeKind::LayerVolume => "Layer Volume",
            McuModeKind::LayerPan => "Layer Panning",
            McuModeKind::LayerSend(_) => "Layer Sends",
            McuModeKind::Device(device) => device.name(),
            McuModeKind::ProjectParameters => "Project Parameters",
            McuModeKind::TrackParameters => "Track Parameters",
            McuModeKind::Browser => "Browser",
            McuModeKind::Markers => "Marker",
        }
    }

    fn is_layer(self) -> bool {
        matches!(self, McuModeKind::Layer | McuModeKind::LayerVolume | McuModeKind::LayerPan | McuModeKind::LayerSend(_))
    }

    fn is_track(self) -> bool {
        matches!(self, McuModeKind::Track | McuModeKind::Volume | McuModeKind::Pan | McuModeKind::Send(_))
    }

    fn bank(self) -> ModeBank {
        match self {
            McuModeKind::Track | McuModeKind::Volume | McuModeKind::Pan | McuModeKind::Send(_) => ModeBank::TrackBank,
            McuModeKind::Layer | McuModeKind::LayerVolume | McuModeKind::LayerPan | McuModeKind::LayerSend(_) => ModeBank::DrumPadBank(DrumDeviceRef::FirstInstrument),
            McuModeKind::Device(device) => ModeBank::ParameterBank(device.bank()),
            McuModeKind::ProjectParameters => ModeBank::ParameterBank(ParameterBankId::Project),
            McuModeKind::TrackParameters => ModeBank::ParameterBank(ParameterBankId::CursorTrack),
            McuModeKind::Master | McuModeKind::Browser | McuModeKind::Markers => ModeBank::None,
        }
    }
}

/// Former `Modes.isLayerMode`.
pub fn is_layer_mode(mode: Option<Modes>) -> bool {
    mode.is_some_and(|m| m >= Modes::DeviceLayer && m <= Modes::DeviceLayerSend8)
}

/// Former `Modes.isSendMode`.
pub fn is_send_mode(mode: Option<Modes>) -> bool {
    mode.is_some_and(|m| m >= Modes::Send1 && m <= Modes::Send8)
}

/// Former `Modes.isLayerSendMode`.
pub fn is_layer_send_mode(mode: Option<Modes>) -> bool {
    mode.is_some_and(|m| m >= Modes::DeviceLayerSend1 && m <= Modes::DeviceLayerSend8)
}

/// The length of the texts on the main display (former `getTextLength`).
pub fn text_length(ctx: &Ctx) -> usize {
    match ctx.config.main_display_type() {
        MainDisplay::Asparion => 12,
        MainDisplay::Mackie6Characters => 6,
        MainDisplay::Mackie7Characters => 7,
        MainDisplay::None => 0,
    }
}

/// Former `preventBlack`.
fn prevent_black(does_exist: bool, color: ColorEx) -> ColorEx {
    if does_exist && color == ColorEx::BLACK {
        ColorEx::GRAY
    } else {
        color
    }
}

/// Run a function on the first text display of the current surface.
pub fn with_display1(ctx: &mut Ctx, f: impl FnOnce(&mut TextDisplay)) {
    if let Some(display) = ctx.text_display.as_mut() {
        f(display);
    }
}

/// Run a function on the second text display of the current surface.
pub fn with_display2(ctx: &mut Ctx, f: impl FnOnce(&mut TextDisplay)) {
    f(&mut mcu_mut(ctx).second_display);
}

fn with_display(ctx: &mut Ctx, second: bool, f: impl FnOnce(&mut TextDisplay)) {
    if second {
        with_display2(ctx, f);
    } else {
        with_display1(ctx, f);
    }
}

/// The layer or drum pad of the first instrument at the index of the page (former
/// `hasDrumPads () ? getDrumPadBank () : getLayerBank ()`).
pub fn layer_channel(ctx: &Ctx, index: usize) -> Option<&Channel> {
    let device = ctx.model.specific_device(DeviceId::FirstInstrument)?;
    if device.has_drum_pads() {
        let bank = device.drum_pad_bank();
        (index < bank.page_size()).then(|| &**bank.item(index))
    } else {
        let bank = device.layer_bank();
        (index < bank.page_size()).then(|| bank.item(index))
    }
}

/// Change the layer or drum pad of the first instrument at the index of the page.
pub fn with_layer_channel_mut(ctx: &mut Ctx, index: usize, f: impl FnOnce(&mut Channel)) {
    let Some(device) = ctx.model.specific_device_mut(DeviceId::FirstInstrument) else {
        return;
    };
    if device.has_drum_pads() {
        let bank = device.drum_pad_bank_mut();
        if index < bank.page_size() {
            f(&mut *bank.item_mut(index));
        }
    } else {
        let bank = device.layer_bank_mut();
        if index < bank.page_size() {
            f(bank.item_mut(index));
        }
    }
}

/// The index of the selected layer or drum pad of the first instrument.
fn selected_layer_index(ctx: &Ctx) -> Option<usize> {
    let device = ctx.model.specific_device(DeviceId::FirstInstrument)?;
    if device.has_drum_pads() {
        device.drum_pad_bank().selected_index()
    } else {
        device.layer_bank().selected_index()
    }
}

fn first_instrument_has_drum_pads(ctx: &Ctx) -> bool {
    ctx.model.specific_device(DeviceId::FirstInstrument).is_some_and(|d| d.has_drum_pads())
}

/// Toggle the enabled state of a send parameter (former `ISend.toggleEnabled`).
fn toggle_send_enabled(ctx: &mut Ctx, parameter: &ParamRef) {
    match *parameter {
        ParamRef::TrackSend { index, send } => {
            let bank = ctx.model.current_track_bank_mut().item_mut(index).send_bank_mut();
            if send < bank.item_count() {
                bank.item_mut(send).toggle_enabled();
            }
        }
        ParamRef::SelectedTrackSend { send } => {
            if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
                let bank = track.send_bank_mut();
                if send < bank.item_count() {
                    bank.item_mut(send).toggle_enabled();
                }
            }
        }
        ParamRef::DrumPadSend { index, send, .. } => {
            if let Some(device) = ctx.model.specific_device_mut(DeviceId::FirstInstrument) {
                let bank = device.drum_pad_bank_mut().item_mut(index).send_bank_mut();
                if send < bank.item_count() {
                    bank.item_mut(send).toggle_enabled();
                }
            }
        }
        _ => {}
    }
}

/// Reset a parameter, to 0 with Control, to the center with Shift, to the maximum with Alt,
/// toggle a send with Select (former `BaseMode.resetParameter (IParameter)`).
pub fn reset_parameter(ctx: &mut Ctx, parameter: &ParamRef) {
    if ctx.surface.is_pressed(ButtonId::Control) {
        parameter.set_normalized_value(ctx, 0.0);
    } else if ctx.surface.is_shift_pressed() {
        parameter.set_normalized_value(ctx, 0.5);
    } else if ctx.surface.is_pressed(ButtonId::Alt) {
        parameter.set_normalized_value(ctx, 1.0);
    } else if ctx.surface.is_select_pressed() {
        toggle_send_enabled(ctx, parameter);
    } else {
        parameter.reset_value(ctx);
    }
}

/// A selected layer parameter: volume, pan, then the sends (former
/// `SelectedLayerOrDrumPadParameterProvider`).
fn selected_layer_parameter(ctx: &Ctx, index: usize) -> ParamRef {
    let Some(layer) = selected_layer_index(ctx) else {
        return ParamRef::Empty;
    };
    let device = DrumDeviceRef::FirstInstrument;
    match index {
        0 => ParamRef::DrumPadVolume { device, index: layer },
        1 => ParamRef::DrumPadPan { device, index: layer },
        _ => ParamRef::DrumPadSend { device, index: layer, send: index - 2 },
    }
}

/// A MCU mode (former `BaseMode` and all its subclasses).
pub struct McuMode {
    pub kind: McuModeKind,
    pub base: ParameterModeBase,
    default_color: ColorEx,
    browser_selection_mode: i32,
    browser_filter_column: i32,
}

const SELECTION_OFF: i32 = 0;
const SELECTION_PRESET: i32 = 1;
const SELECTION_FILTER: i32 = 2;

const LED_MODES_WRAP: [i32; 8] = [KNOB_LED_MODE_WRAP; 8];
const LED_MODES_BOOST_CUT: [i32; 8] = [KNOB_LED_MODE_BOOST_CUT; 8];

impl McuMode {
    /// Create the mode for the current surface of the context.
    pub fn new(ctx: &Ctx, kind: McuModeKind) -> Self {
        let mut base = ParameterModeBase::new(kind.name(), true, kind.bank(), default_knob_ids());
        let surface_id = mcu(ctx).index;
        let offset = surface_id * 8;
        let range = |p: SharedProvider| Some(provider(RangeFilterParameterProvider::new(p, offset, 8)));
        let parameter_provider: Option<SharedProvider> = match kind {
            McuModeKind::Track => match surface_id {
                0 => Some(provider(RangeFilterParameterProvider::new(provider(SelectedTrackParameterProvider::new(ctx)), 0, 8))),
                1 => Some(provider(RangeFilterParameterProvider::new(provider(SendParameterProvider::new(ctx, -1, 6)), 0, 8))),
                _ => Some(provider(EmptyParameterProvider::new(8))),
            },
            McuModeKind::Volume => range(provider(VolumeParameterProvider::new(ctx))),
            McuModeKind::Pan => range(provider(PanParameterProvider::new(ctx))),
            McuModeKind::Send(send) => range(provider(SendParameterProvider::new(ctx, send as i32, 0))),
            McuModeKind::Master => {
                let mut parameters = vec![ParamRef::MasterVolume, ParamRef::MasterPan];
                parameters.resize(8, ParamRef::Empty);
                Some(provider(FixedParameterProvider::new(parameters)))
            }
            McuModeKind::Layer => match surface_id {
                0 => Some(provider(FnParameterProvider::new(8, selected_layer_parameter))),
                1 => Some(provider(RangeFilterParameterProvider::new(provider(LayerOrDrumPadParameterProvider::send(ctx, 6)), 0, 8))),
                _ => Some(provider(EmptyParameterProvider::new(8))),
            },
            McuModeKind::LayerVolume => range(provider(LayerOrDrumPadParameterProvider::volume(ctx))),
            McuModeKind::LayerPan => range(provider(LayerOrDrumPadParameterProvider::pan(ctx))),
            McuModeKind::LayerSend(send) => range(provider(LayerOrDrumPadParameterProvider::send(ctx, send))),
            McuModeKind::Device(device) => range(provider(BankParameterProvider::new(ctx, device.bank()))),
            McuModeKind::ProjectParameters => range(provider(BankParameterProvider::new(ctx, ParameterBankId::Project))),
            McuModeKind::TrackParameters => range(provider(BankParameterProvider::new(ctx, ParameterBankId::CursorTrack))),
            McuModeKind::Browser | McuModeKind::Markers => None,
        };
        if let Some(parameter_provider) = parameter_provider {
            base.set_parameter_provider(ctx, parameter_provider);
        }
        McuMode { kind, base, default_color: ColorEx::SKY_BLUE, browser_selection_mode: SELECTION_OFF, browser_filter_column: -1 }
    }

    /// Former `getParameterProvider`.
    pub fn parameter_provider(&self, ctx: &Ctx) -> Option<SharedProvider> {
        self.base.parameter_provider(ctx)
    }

    fn parameter(&self, ctx: &Ctx, index: usize) -> ParamRef {
        self.base.parameter(ctx, index)
    }

    /// Former `isKnobTouched`.
    pub fn is_knob_touched(&self, index: usize) -> bool {
        self.base.is_knob_touched(index)
    }

    /// The provider of the faders: the own one if the faders are used like the knobs, otherwise
    /// the one of the (layer) volume mode (former part of `BaseMode.bindControls`).
    fn fader_provider(&self, ctx: &Ctx) -> Option<SharedProvider> {
        if ctx.config.use_faders_as_knobs() {
            return self.parameter_provider(ctx);
        }
        // Always binds providers from (layer-) volume mode
        let is_layer = is_layer_mode(ctx.modes.active_id());
        let (target_kind, target) = if is_layer { (McuModeKind::LayerVolume, Modes::DeviceLayerVolume) } else { (McuModeKind::Volume, Modes::Volume) };
        if self.kind == target_kind {
            return self.parameter_provider(ctx);
        }
        let mode = ctx.modes.get(target)?;
        let mode = mode.try_borrow().ok()?;
        mode.as_any().downcast_ref::<McuMode>()?.parameter_provider(ctx)
    }

    /// Former `BaseMode.bindControls`: bind the knobs and the faders.
    pub fn bind_controls(&self, ctx: &mut Ctx) {
        if !self.base.is_active() || self.base.binding.default_provider().is_none() {
            return;
        }
        self.base.bind_controls(ctx);
        if let Some(fader_provider) = self.fader_provider(ctx) {
            for i in 0..8 {
                let parameter = fader_provider.get(ctx, i);
                ctx.surface.bind_continuous_parameter(ContinuousId::get(ContinuousId::Fader1, i), Some(parameter));
            }
        }
    }

    /// Former `parametersAdjusted`.
    pub fn parameters_adjusted(&self, ctx: &mut Ctx) {
        self.bind_controls(ctx);
    }

    fn reset_parameter_index(&mut self, ctx: &mut Ctx, index: usize) {
        let offset = extender_offset(ctx);
        match self.kind {
            // Not used
            McuModeKind::Master => {}
            McuModeKind::ProjectParameters => reset_parameter(ctx, &ParamRef::BankItem { bank: ParameterBankId::Project, index: offset + index }),
            McuModeKind::TrackParameters => reset_parameter(ctx, &ParamRef::BankItem { bank: ParameterBankId::CursorTrack, index: offset + index }),
            McuModeKind::Markers => {
                let bank = ctx.model.marker_bank();
                if offset + index >= bank.page_size() || !bank.item(offset + index).does_exist() {
                    return;
                }
                let shift = ctx.surface.is_shift_pressed();
                let marker = ctx.model.marker_bank_mut().item_mut(offset + index);
                if shift {
                    marker.select();
                } else {
                    marker.launch(true);
                }
            }
            McuModeKind::Browser => self.browser_reset_parameter(ctx, index),
            _ => {
                let parameter = self.parameter(ctx, index);
                reset_parameter(ctx, &parameter);
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Buttons

    fn track_on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize) {
        let channel = extender_offset(ctx) + index;
        if channel >= ctx.model.current_track_bank().page_size() {
            return;
        }
        let shift = ctx.surface.is_shift_pressed();
        let select = ctx.surface.is_select_pressed();
        let tb = ctx.model.current_track_bank_mut();
        match row {
            1 => {
                if shift {
                    tb.toggle_rec_arm();
                } else {
                    tb.item_mut(channel).toggle_rec_arm();
                }
            }
            2 => {
                if shift {
                    tb.item_mut(channel).toggle_auto_monitor();
                } else if select {
                    tb.clear_solo();
                } else {
                    tb.item_mut(channel).toggle_solo();
                }
            }
            3 => {
                if shift {
                    tb.item_mut(channel).toggle_monitor();
                } else if select {
                    tb.clear_mute();
                } else {
                    tb.item_mut(channel).toggle_mute();
                }
            }
            _ => {}
        }
    }

    fn layer_on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize) {
        let channel = extender_offset(ctx) + index;
        let select = ctx.surface.is_select_pressed();
        match row {
            2 => {
                if select {
                    ctx.model.current_track_bank_mut().clear_solo();
                } else {
                    with_layer_channel_mut(ctx, channel, |c| c.toggle_solo());
                }
            }
            3 => {
                if select {
                    ctx.model.current_track_bank_mut().clear_mute();
                } else {
                    with_layer_channel_mut(ctx, channel, |c| c.toggle_mute());
                }
            }
            _ => {}
        }
    }

    fn master_on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event == ButtonEvent::Down && row > 0 {
            ctx.schedule(0, |ctx| ctx.restore_mode());
            return;
        }
        if extender_offset(ctx) > 0 || event != ButtonEvent::Up || row > 0 {
            return;
        }
        match index {
            0 => reset_parameter(ctx, &ParamRef::MasterVolume),
            1 => reset_parameter(ctx, &ParamRef::MasterPan),
            2..=4 => ctx.model.application_mut().toggle_engine_active(),
            6 => ctx.model.project_mut().previous(),
            7 => ctx.model.project_mut().next(),
            _ => {}
        }
    }

    fn device_on_button(&self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) -> bool {
        if row != 0 {
            return false;
        }
        let offset = extender_offset(ctx);
        if ctx.surface.is_pressed(ButtonId::Control) {
            if event == ButtonEvent::Down && ctx.model.cursor_device().does_exist() {
                let device = ctx.model.cursor_device();
                let bank = device.device_bank();
                let page_size = bank.page_size().max(1) as i32;
                let pos = device.position() / page_size * page_size + (offset + index) as i32;
                // Former selectItemAtPosition: select the device if it is on the page
                let page_index = pos - bank.scroll_position();
                if page_index >= 0 && (page_index as usize) < bank.page_size() {
                    ctx.model.cursor_device_mut().device_bank_mut().item_mut(page_index as usize).select();
                }
            }
            return true;
        }
        if ctx.surface.is_select_pressed() {
            if event == ButtonEvent::Down && ctx.model.cursor_device().does_exist() {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    let page_bank = bank.page_bank();
                    let page_size = page_bank.page_size().max(1);
                    let pos = page_bank.selected_item_position() / page_size * page_size;
                    bank.page_scroll_to(pos + offset + index);
                }
            }
            return true;
        }
        false
    }

    // ---------------------------------------------------------------------------------------
    // Display

    /// Former `drawParameterHeader (display, row)`, including the overrides of the track and
    /// the layer mode.
    fn draw_parameter_header(&self, ctx: &mut Ctx, second: bool, row: usize) {
        let Some(parameter_provider) = self.parameter_provider(ctx) else {
            return;
        };
        let length = text_length(ctx);
        let names: Vec<Option<String>> = (0..8)
            .map(|i| {
                let parameter = parameter_provider.get(ctx, i);
                parameter.does_exist(ctx).then(|| shorten_and_fix_ascii(&parameter.name(ctx), length))
            })
            .collect();
        // Overwrite the label of the first cell
        let first = if extender_offset(ctx) == 0 && ctx.config.is_display_track_names() {
            match self.kind {
                McuModeKind::Track => Some(ctx.model.current_track_bank().selected_item().map(|t| shorten_and_fix_ascii(t.name(), length))),
                McuModeKind::Layer => Some(selected_layer_index(ctx).and_then(|i| layer_channel(ctx, i)).map(|c| shorten_and_fix_ascii(c.name(), length))),
                _ => None,
            }
        } else {
            None
        };
        with_display(ctx, second, |d| {
            d.clear();
            for (i, name) in names.iter().enumerate() {
                if let Some(name) = name {
                    d.set_cell(row, i, name);
                }
            }
            d.done(row);
            if let Some(Some(name)) = &first {
                d.set_cell(row, 0, name);
                d.done(row);
            }
        });
    }

    /// Former `drawTrackNameHeader`, including the overrides.
    fn draw_track_name_header(&self, ctx: &mut Ctx) {
        let length = text_length(ctx);
        let offset = extender_offset(ctx);
        let is_asparion_first = ctx.config.main_display_type() == MainDisplay::Asparion && mcu(ctx).index == 0;
        match self.kind {
            McuModeKind::Track => {
                self.draw_parameter_header(ctx, false, 0);
                if is_asparion_first {
                    let Some(name) = ctx.model.current_track_bank().selected_item().map(|t| shorten_and_fix_ascii(t.name(), length)) else {
                        return;
                    };
                    with_display1(ctx, |d| {
                        d.clear_row(0);
                        d.set_cell(0, 0, &name);
                        d.done(0);
                    });
                }
            }
            McuModeKind::Layer => self.draw_parameter_header(ctx, false, 0),
            McuModeKind::Device(device) => {
                self.draw_parameter_header(ctx, false, 0);
                if !is_asparion_first {
                    return;
                }
                let mut cells = Vec::new();
                let track = ctx.model.cursor_track();
                if track.does_exist() {
                    cells.push(shorten_and_fix_ascii(track.name(), length));
                    if let Some(d) = device.device(ctx).filter(|d| d.does_exist()) {
                        cells.push(shorten_and_fix_ascii(d.name(), length));
                        if let Some(bank) = d.parameter_bank() {
                            let page_bank = bank.page_bank();
                            if page_bank.item_count() > 0 {
                                cells.push(shorten_and_fix_ascii(&page_bank.selected_item(), length));
                                cells.push(format!("Page {}/{}", page_bank.selected_item_index() + 1, page_bank.item_count()));
                            }
                        }
                    }
                }
                with_display1(ctx, |d| {
                    d.clear_row(0);
                    for (i, cell) in cells.iter().enumerate() {
                        d.set_cell(0, i, cell);
                    }
                    d.done(0);
                });
            }
            McuModeKind::Send(send) => {
                let tb = ctx.model.current_track_bank();
                let names: Vec<Option<String>> = (0..8)
                    .map(|i| {
                        let channel = offset + i;
                        if channel >= tb.page_size() || !tb.item(channel).does_exist() {
                            return None;
                        }
                        let text = format!("S{}: ", send + 1);
                        let name = shorten_and_fix_ascii(tb.item(channel).name(), length.saturating_sub(text.len()));
                        Some(text + &name)
                    })
                    .collect();
                with_display1(ctx, |d| {
                    d.clear();
                    for (i, name) in names.iter().enumerate() {
                        if let Some(name) = name {
                            d.set_cell(0, i, name);
                        }
                    }
                    d.done(0);
                });
            }
            kind if kind.is_layer() => {
                let names: Vec<String> = (0..8).map(|i| layer_channel(ctx, offset + i).map(|c| shorten_and_fix_ascii(c.name(), length)).unwrap_or_default()).collect();
                with_display1(ctx, |d| {
                    d.clear();
                    for (i, name) in names.iter().enumerate() {
                        d.set_cell(0, i, name);
                    }
                    d.done(0);
                });
            }
            _ => {
                let tb = ctx.model.current_track_bank();
                let names: Vec<String> = (0..8)
                    .map(|i| {
                        let channel = offset + i;
                        if channel < tb.page_size() {
                            shorten_and_fix_ascii(tb.item(channel).name(), length)
                        } else {
                            String::new()
                        }
                    })
                    .collect();
                with_display1(ctx, |d| {
                    d.clear();
                    for (i, name) in names.iter().enumerate() {
                        d.set_cell(0, i, name);
                    }
                    d.done(0);
                });
            }
        }
    }

    /// Former `drawDisplay2`.
    fn draw_display2(&self, ctx: &mut Ctx) {
        let second_display = ctx.config.second_display_type();
        match second_display {
            SecondDisplay::Asparion => self.draw_parameter_header(ctx, true, 0),
            SecondDisplay::Qcon | SecondDisplay::V1m => {
                let is_main_device = mcu(ctx).is_main_device;
                let is_short = is_main_device && second_display == SecondDisplay::Qcon;
                let length = if is_short { 6 } else { 7 };
                let offset = extender_offset(ctx);
                let names: Vec<String> = if is_layer_mode(ctx.modes.active_id()) {
                    (0..8).map(|i| layer_channel(ctx, offset + i).map(|c| shorten_and_fix_ascii(c.name(), length)).unwrap_or_default()).collect()
                } else {
                    let tb = ctx.model.current_track_bank();
                    (0..8).map(|i| if offset + i < tb.page_size() { shorten_and_fix_ascii(tb.item(offset + i).name(), length) } else { String::new() }).collect()
                };
                let selected = if is_main_device {
                    let master = ctx.model.master_track();
                    let track_name = if master.is_selected() {
                        Some(master.name().to_string())
                    } else {
                        Some(ctx.model.current_track_bank().selected_item().map(|t| t.name().to_string()).unwrap_or_default())
                    };
                    let device = ctx.model.cursor_device();
                    let device_name = if device.does_exist() { shorten_and_fix_ascii(device.name(), 11) } else { "None".to_string() };
                    Some((track_name.map_or_else(|| "None".to_string(), |n| shorten_and_fix_ascii(&n, 11)), device_name))
                } else {
                    None
                };
                with_display2(ctx, |d2| {
                    for (i, name) in names.iter().enumerate() {
                        d2.set_cell(0, i, name);
                    }
                    if is_short {
                        d2.set_cell(0, 8, "Maste");
                    }
                    d2.done(0);
                    d2.clear_row(1);
                    if let Some((track_name, device_name)) = &selected {
                        d2.set_block(1, 0, "  Sel. track:").set_block(1, 1, track_name);
                        d2.set_block(1, 2, "  Sel. devce:").set_block(1, 3, device_name);
                    }
                    d2.done(1);
                });
            }
            SecondDisplay::None => {}
        }
    }

    /// Former `BaseMode.updateDisplay`.
    fn base_update_display(&self, ctx: &mut Ctx) {
        let main_display_type = ctx.config.main_display_type();
        if main_display_type == MainDisplay::None {
            return;
        }
        let is_asparion = main_display_type == MainDisplay::Asparion;
        self.draw_display2(ctx);
        if is_asparion || ctx.config.is_display_track_names() {
            self.draw_track_name_header(ctx);
        } else {
            self.draw_parameter_header(ctx, false, 0);
        }
        let Some(parameter_provider) = self.parameter_provider(ctx) else {
            return;
        };
        let length = text_length(ctx);
        let mut colors = [ColorEx::BLACK; 8];
        let mut values = Vec::with_capacity(8);
        for (i, color) in colors.iter_mut().enumerate() {
            let parameter = parameter_provider.get(ctx, i);
            values.push(shorten_and_fix_ascii(&parameter.displayed_value(ctx), length));
            let exists = parameter.does_exist(ctx);
            let c = parameter_provider.color(ctx, i).unwrap_or(self.default_color);
            *color = prevent_black(exists, if exists { c } else { ColorEx::BLACK });
        }
        with_display1(ctx, |d| {
            for (i, value) in values.iter().enumerate() {
                d.set_cell(1, i, value);
            }
            d.done(1);
        });
        send_display_color(ctx, &colors);
    }

    /// Former `updateItemIndices` of the track and layer modes.
    fn update_item_indices(&self, ctx: &mut Ctx) {
        let offset = extender_offset(ctx);
        let mut indices = [0; 8];
        match self.kind {
            McuModeKind::Track => {
                let track = ctx.model.cursor_track();
                if offset == 0 && track.does_exist() {
                    indices[0] = track.position() + 1;
                }
            }
            McuModeKind::Layer => {
                if offset == 0 {
                    if let Some(channel) = selected_layer_index(ctx).and_then(|i| layer_channel(ctx, i)) {
                        indices[0] = channel.position() + 1;
                    }
                }
            }
            kind if kind.is_layer() => {
                let add = if first_instrument_has_drum_pads(ctx) { 0 } else { 1 };
                for (i, index) in indices.iter_mut().enumerate() {
                    if let Some(channel) = layer_channel(ctx, offset + i).filter(|c| c.does_exist()) {
                        *index = channel.position() + add;
                    }
                }
            }
            _ => {
                let tb = ctx.model.current_track_bank();
                for (i, index) in indices.iter_mut().enumerate() {
                    if offset + i < tb.page_size() {
                        let item = tb.item(offset + i);
                        *index = if item.does_exist() { item.position() + 1 } else { 0 };
                    }
                }
            }
        }
        set_item_indices(ctx, &indices);
    }

    fn device_update_display(&self, ctx: &mut Ctx, device: DeviceKind) {
        let is_control_pressed = ctx.surface.is_pressed(ButtonId::Control);
        if is_control_pressed || ctx.surface.is_select_pressed() {
            let mut colors = [ColorEx::BLACK; 8];
            let mut names = vec![String::new(); 8];
            let length = text_length(ctx);
            let offset = extender_offset(ctx);
            let cursor_device = ctx.model.cursor_device();
            if is_control_pressed {
                let bank = cursor_device.device_bank();
                for i in 0..8 {
                    if offset + i < bank.page_size() && bank.item(offset + i).does_exist() {
                        names[i] = shorten_and_fix_ascii(bank.item(offset + i).name(), length);
                        colors[i] = ColorEx::WHITE;
                    }
                }
            } else if let Some(bank) = cursor_device.parameter_bank() {
                let page_bank = bank.page_bank();
                for i in 0..8 {
                    let page = page_bank.item(offset + i);
                    if !page.trim().is_empty() {
                        names[i] = shorten_and_fix_ascii(&page, length);
                        colors[i] = ColorEx::WHITE;
                    }
                }
            }
            with_display1(ctx, |d| {
                d.clear();
                for (i, name) in names.iter().enumerate() {
                    if !name.is_empty() {
                        d.set_cell(0, i, name);
                    }
                }
                d.all_done();
            });
            send_display_color(ctx, &colors);
            return;
        }

        self.base_update_display(ctx);

        let mut indices = [0; 8];
        if extender_offset(ctx) == 0 {
            let track = ctx.model.cursor_track();
            if track.does_exist() {
                indices[0] = track.position() + 1;
            }
            if let Some(d) = device.device(ctx).filter(|d| d.does_exist()) {
                indices[1] = d.position() + 1;
                indices[2] = d.parameter_bank().map_or(0, |b| b.page_bank().selected_item_index() as i32) + 1;
            }
        }
        set_item_indices(ctx, &indices);
    }

    fn master_update_display(&self, ctx: &mut Ctx) {
        let main_display_type = ctx.config.main_display_type();
        if main_display_type == MainDisplay::None {
            return;
        }
        self.draw_display2(ctx);
        let mut colors = [ColorEx::BLACK; 8];
        if extender_offset(ctx) == 0 {
            let length = text_length(ctx);
            let project_name = crate::config::values::fix_ascii(ctx.model.project().name());
            let master = ctx.model.master_track();
            let volume = master.volume_str_limited(length);
            let pan = master.pan_str_limited(length);
            let master_color = master.color();
            let is_engine_active = ctx.model.application().is_engine_active();
            let is_7 = main_display_type == MainDisplay::Mackie7Characters;
            with_display1(ctx, |d| {
                d.clear();
                d.set_cell(0, 0, "Volume").set_cell(0, 1, "Pan");
                if is_7 {
                    d.set_block(0, 1, "  AudioEngine:").set_cell(0, 4, if is_engine_active { "On" } else { "Off" });
                    d.set_cell(0, 5, "Prjct:").set_block(0, 3, &project_name);
                    d.set_cell(1, 0, &volume).set_cell(1, 1, &pan).set_block(1, 1, if is_engine_active { "   Turnoff" } else { "   Turnon" });
                    d.set_cell(1, 6, "   <<").set_cell(1, 7, "   >>").all_done();
                } else {
                    d.set_block(0, 1, "Audio Engine:").set_cell(0, 4, if is_engine_active { " On" } else { " Off" });
                    d.set_cell(0, 5, "Prjct:").set_block(0, 3, &project_name);
                    d.set_cell(1, 0, &volume).set_cell(1, 1, &pan).set_block(1, 1, if is_engine_active { "  Turn off" } else { "  Turn on" });
                    d.set_cell(1, 6, " <<").set_cell(1, 7, " >>").all_done();
                }
            });
            colors = [master_color, master_color, ColorEx::SKY_BLUE, ColorEx::SKY_BLUE, ColorEx::SKY_BLUE, ColorEx::WHITE, ColorEx::WHITE, ColorEx::WHITE];
        } else {
            with_display1(ctx, |d| {
                d.clear();
            });
        }
        send_display_color(ctx, &colors);
    }

    fn parameters_update_display(&self, ctx: &mut Ctx, bank: ParameterBankId, title: &str) {
        self.draw_display2(ctx);
        let mut colors = [ColorEx::BLACK; 8];
        let offset = extender_offset(ctx);
        let length = text_length(ctx);
        let mut cells = Vec::with_capacity(8);
        for (i, color) in colors.iter_mut().enumerate() {
            let parameter = ParamRef::BankItem { bank, index: offset + i };
            let exists = parameter.does_exist(ctx);
            let name = if exists { shorten_and_fix_ascii(&parameter.name(ctx), length) } else { String::new() };
            cells.push((name, shorten_and_fix_ascii(&parameter.displayed_value(ctx), length)));
            *color = if exists { ColorEx::WHITE } else { ColorEx::BLACK };
        }
        let asparion_header = if ctx.config.main_display_type() == MainDisplay::Asparion && mcu(ctx).index == 0 {
            let page = crate::framework::parameterprovider::parameter_bank(&ctx.model, bank).filter(|b| b.page_bank().item_count() > 0).map(|b| shorten_and_fix_ascii(&b.page_bank().selected_item(), length));
            Some(page)
        } else {
            None
        };
        with_display1(ctx, |d| {
            d.clear();
            for (i, (name, value)) in cells.iter().enumerate() {
                d.set_cell(0, i, name);
                d.set_cell(1, i, value);
            }
            if let Some(page) = &asparion_header {
                d.clear_row(0);
                d.set_cell(0, 0, title);
                d.set_cell(0, 1, "Parameters");
                if let Some(page) = page {
                    d.set_cell(0, 2, page);
                }
            }
            d.all_done();
        });
        send_display_color(ctx, &colors);
        let mut indices = [0; 8];
        if offset == 0 {
            if let Some(b) = crate::framework::parameterprovider::parameter_bank(&ctx.model, bank) {
                indices[2] = b.page_bank().selected_item_index() as i32 + 1;
            }
        }
        set_item_indices(ctx, &indices);
    }

    fn marker_update_display(&self, ctx: &mut Ctx) {
        let offset = extender_offset(ctx);
        let length = text_length(ctx);
        let bank = ctx.model.marker_bank();
        let mut colors = [ColorEx::BLACK; 8];
        let mut names = Vec::with_capacity(8);
        for (i, color) in colors.iter_mut().enumerate() {
            if offset + i < bank.page_size() {
                let marker = bank.item(offset + i);
                names.push(shorten_and_fix_ascii(&marker.name(), length));
                *color = prevent_black(marker.does_exist(), marker.color());
            } else {
                names.push(String::new());
            }
        }
        with_display1(ctx, |d| {
            d.clear();
            for (i, name) in names.iter().enumerate() {
                d.set_cell(0, i, name);
            }
            d.all_done();
        });
        send_display_color(ctx, &colors);
    }

    // ---------------------------------------------------------------------------------------
    // Browser

    /// Former `DeviceBrowserMode.getFilterColumn`.
    fn browser_filter_column(ctx: &Ctx, index: usize) -> Option<&crate::daw::browser::BrowserColumn> {
        let browser = ctx.model.browser()?;
        (index < browser.filter_column_count()).then(|| browser.filter_column(index)).flatten()
    }

    fn browser_update_display(&self, ctx: &mut Ctx) {
        send_display_color(ctx, &[ColorEx::SKY_BLUE; 8]);
        let Some(browser) = ctx.model.browser() else {
            return;
        };
        let is_preset_session = browser.is_preset_content_type();
        if is_preset_session && !(browser.is_active() && ctx.model.cursor_device().does_exist()) {
            ctx.notify("No active Browsing Session. Select device and press Browser...");
            return;
        }
        let length = text_length(ctx);
        let fix = crate::config::values::fix_ascii;
        let mut cells: Vec<(usize, usize, String)> = Vec::new();
        let mut blocks: Vec<(usize, usize, String)> = Vec::new();
        match self.browser_selection_mode {
            SELECTION_OFF => {
                for i in 0..7 {
                    let (name, value) = match Self::browser_filter_column(ctx, i) {
                        Some(column) => {
                            let value = if column.cursor_exists {
                                if column.cursor_name == column.wildcard {
                                    "-".to_string()
                                } else {
                                    column.cursor_name.clone()
                                }
                            } else {
                                String::new()
                            };
                            (shorten_and_fix_ascii(&column.name, length), value)
                        }
                        None => (String::new(), String::new()),
                    };
                    cells.push((0, i, name));
                    cells.push((1, i, value));
                }
                let selected_result = browser.selected_result().unwrap_or_default();
                cells.push((0, 7, browser.selected_content_type().to_string()));
                cells.push((1, 7, if selected_result.is_empty() { "-".to_string() } else { selected_result.to_string() }));
            }
            SELECTION_PRESET => {
                let results = browser.result_items();
                for i in 0..browser.num_filter_column_entries() {
                    let Some(result) = results.get(i) else {
                        break;
                    };
                    blocks.push((i / 4, i % 4, format!("{}{}", if result.is_selected { ">" } else { " " }, fix(&result.name))));
                }
            }
            SELECTION_FILTER => {
                if let Some(column) = browser.filter_column(self.browser_filter_column.max(0) as usize) {
                    for i in 0..browser.num_results() {
                        let Some(item) = column.items.get(i) else {
                            break;
                        };
                        blocks.push((i / 4, i % 4, format!("{}{}", if item.is_selected { ">" } else { " " }, fix(&item.name))));
                    }
                }
            }
            _ => {}
        }
        with_display1(ctx, |d| {
            d.clear();
            for (row, column, text) in &cells {
                d.set_cell(*row, *column, text);
            }
            for (row, block, text) in &blocks {
                if *row < 2 {
                    d.set_block(*row, *block, text);
                }
            }
            d.all_done();
        });
    }

    fn browser_reset_parameter(&mut self, ctx: &mut Ctx, index: usize) {
        if self.browser_selection_mode != SELECTION_OFF {
            self.browser_selection_mode = SELECTION_OFF;
            return;
        }
        if index == 7 {
            self.browser_selection_mode = SELECTION_PRESET;
            self.browser_filter_column = -1;
        } else if let Some(column) = Self::browser_filter_column(ctx, index).filter(|c| c.exists) {
            self.browser_selection_mode = SELECTION_FILTER;
            self.browser_filter_column = column.index as i32;
        }
    }

    fn browser_on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let mut speed = ctx.model.value_changer().calc_stepped_knob_change(value);
        let direction = speed > 0;
        if ctx.surface.is_shift_pressed() {
            speed *= 4;
        }
        let count = speed.unsigned_abs() as usize;
        if index < 7 {
            let Some(column) = Self::browser_filter_column(ctx, index).filter(|c| c.exists).map(|c| c.index) else {
                return;
            };
            self.browser_filter_column = column as i32;
            // Same as Java: the previous items are selected count * count times
            let repeat = if direction { 1 } else { count };
            let Some(browser) = ctx.model.browser_mut() else {
                return;
            };
            for _ in 0..repeat {
                for _ in 0..count {
                    if direction {
                        browser.select_next_filter_item(column);
                    } else {
                        browser.select_previous_filter_item(column);
                    }
                }
                if browser.selected_filter_item_index(column) == -1 {
                    if direction {
                        browser.next_filter_item_page(column);
                    } else {
                        browser.previous_filter_item_page(column);
                    }
                }
            }
        } else if let Some(browser) = ctx.model.browser_mut() {
            let repeat = if direction { count } else { count * count };
            for _ in 0..repeat {
                if direction {
                    browser.select_next_result();
                } else {
                    browser.select_previous_result();
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Knob LEDs

    /// Former `updateKnobLEDs (knobLedModes)`.
    fn update_knob_leds_with(&self, ctx: &mut Ctx, knob_led_modes: &[i32; 8]) {
        let Some(parameter_provider) = self.parameter_provider(ctx) else {
            return;
        };
        let upper_bound = ctx.model.value_changer().upper_bound();
        for (i, mode) in knob_led_modes.iter().enumerate() {
            let parameter = parameter_provider.get(ctx, i);
            if parameter.does_exist(ctx) {
                let mut value = parameter.value(ctx);
                // Prevent LEDs to be turned off when full left
                if *mode == KNOB_LED_MODE_BOOST_CUT && value == 0 {
                    value = 1;
                }
                set_knob_led(ctx, i, *mode, value, upper_bound);
            } else {
                set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, 0, upper_bound);
            }
        }
    }

    /// Update the LED rings of the knobs (former `updateKnobLEDs`).
    pub fn update_knob_leds(&self, ctx: &mut Ctx) {
        let upper_bound = ctx.model.value_changer().upper_bound();
        let offset = extender_offset(ctx);
        match self.kind {
            McuModeKind::Pan => self.update_knob_leds_with(ctx, &LED_MODES_BOOST_CUT),
            McuModeKind::Send(_) => {
                if ctx.model.is_effect_track_bank_active() {
                    ctx.schedule(0, |ctx| ctx.set_active_mode(Modes::Track));
                    return;
                }
                self.update_knob_leds_with(ctx, &LED_MODES_WRAP);
            }
            McuModeKind::Track | McuModeKind::Layer => {
                let mut modes = LED_MODES_WRAP;
                if offset == 0 && self.parameter(ctx, 1).does_exist(ctx) {
                    modes[1] = KNOB_LED_MODE_BOOST_CUT;
                }
                self.update_knob_leds_with(ctx, &modes);
            }
            McuModeKind::Master => {
                if offset > 0 {
                    for i in 0..8 {
                        set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, 0, 0);
                    }
                    return;
                }
                let master = ctx.model.master_track();
                let (volume, pan) = (master.volume(), master.pan());
                set_knob_led(ctx, 0, KNOB_LED_MODE_WRAP, volume, upper_bound);
                set_knob_led(ctx, 1, KNOB_LED_MODE_BOOST_CUT, pan, upper_bound);
                for i in 0..6 {
                    set_knob_led(ctx, 2 + i, KNOB_LED_MODE_WRAP, 0, upper_bound);
                }
            }
            McuModeKind::Device(_) => {
                let is_control_pressed = ctx.surface.is_pressed(ButtonId::Control);
                if !is_control_pressed && !ctx.surface.is_select_pressed() {
                    self.update_knob_leds_with(ctx, &LED_MODES_WRAP);
                    return;
                }
                let cursor_device = ctx.model.cursor_device();
                let values: Vec<i32> = if is_control_pressed {
                    let bank = cursor_device.device_bank();
                    let selected_position = cursor_device.position();
                    (0..8)
                        .map(|i| {
                            let exists = offset + i < bank.page_size() && bank.item(offset + i).does_exist() && bank.item(offset + i).position() == selected_position;
                            if exists {
                                upper_bound - 1
                            } else {
                                0
                            }
                        })
                        .collect()
                } else {
                    let page_bank = cursor_device.parameter_bank().map(|b| (b.page_bank().selected_item_position(), (0..8).map(|i| b.page_bank().item(offset + i)).collect::<Vec<_>>()));
                    match page_bank {
                        Some((selected_position, pages)) => {
                            pages.iter().enumerate().map(|(i, page)| if !page.trim().is_empty() && offset + i == selected_position { upper_bound - 1 } else { 0 }).collect()
                        }
                        None => vec![0; 8],
                    }
                };
                for (i, value) in values.iter().enumerate() {
                    set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, *value, upper_bound);
                }
            }
            McuModeKind::ProjectParameters | McuModeKind::TrackParameters => {
                let bank = if self.kind == McuModeKind::ProjectParameters { ParameterBankId::Project } else { ParameterBankId::CursorTrack };
                for i in 0..8 {
                    let parameter = ParamRef::BankItem { bank, index: offset + i };
                    let value = if parameter.does_exist(ctx) { parameter.value(ctx).max(1) } else { 0 };
                    set_knob_led(ctx, i, KNOB_LED_MODE_SINGLE_DOT, value, upper_bound);
                }
            }
            McuModeKind::Browser => {
                for i in 0..8 {
                    set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, 0, 1);
                }
            }
            McuModeKind::Markers => {
                let bank = ctx.model.marker_bank();
                let exists: Vec<bool> = (0..8).map(|i| offset + i < bank.page_size() && bank.item(offset + i).does_exist()).collect();
                for (i, exists) in exists.iter().enumerate() {
                    set_knob_led(ctx, i, KNOB_LED_MODE_WRAP, 0, i32::from(*exists));
                }
            }
            _ => self.update_knob_leds_with(ctx, &LED_MODES_WRAP),
        }
    }

    fn eq_track_selection_changed(&self, ctx: &mut Ctx) {
        // Add an equalizer if not present and this is the first device (no extender)
        if self.kind != McuModeKind::Device(DeviceKind::Eq) || !self.base.is_active() || mcu(ctx).index != 0 {
            return;
        }
        if ctx.model.specific_device(DeviceId::Eq).is_some_and(|d| d.does_exist()) {
            return;
        }
        if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
            track.add_equalizer_device();
        }
    }

    /// Former `DeviceParamsMode.trackSelectionChanged` (observer of the track bank).
    pub fn track_selection_changed(&self, ctx: &mut Ctx, is_selected: bool) {
        if is_selected {
            self.eq_track_selection_changed(ctx);
        }
    }

    fn set_master_indication(ctx: &mut Ctx, enable: bool) {
        let master = ctx.model.master_track_mut();
        master.set_volume_indication(enable);
        master.set_pan_indication(enable);
    }
}

impl Mode for McuMode {
    crate::forward_mode!(base => name, select_item, has_previous_item, has_next_item, has_previous_item_page, has_next_item_page,
        select_previous_item, select_next_item, select_previous_item_page, select_next_item_page, select_item_page, is_any_knob_touched);

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
        // Bind the faders as well
        self.bind_controls(ctx);
        match self.kind {
            McuModeKind::Master => Self::set_master_indication(ctx, true),
            McuModeKind::Device(DeviceKind::Eq) => self.eq_track_selection_changed(ctx),
            _ => {}
        }
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
        match self.kind {
            McuModeKind::Master => Self::set_master_indication(ctx, false),
            McuModeKind::Browser => {
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.stop_browsing(true);
                }
            }
            _ => {}
        }
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let offset = extender_offset(ctx);
        match self.kind {
            McuModeKind::Browser => self.browser_on_knob_value(ctx, index, value),
            // Intentionally empty
            McuModeKind::Markers => {}
            McuModeKind::ProjectParameters => ParamRef::BankItem { bank: ParameterBankId::Project, index: offset + index }.change_value(ctx, value),
            McuModeKind::TrackParameters => ParamRef::BankItem { bank: ParameterBankId::CursorTrack, index: offset + index }.change_value(ctx, value),
            _ => self.base.on_knob_value(ctx, index, value),
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        let parameter = match self.kind {
            McuModeKind::Master => match index {
                0 => ParamRef::MasterVolume,
                1 => ParamRef::MasterPan,
                _ => ParamRef::Empty,
            },
            // Same as Java: without the extender offset
            McuModeKind::ProjectParameters => ParamRef::BankItem { bank: ParameterBankId::Project, index },
            McuModeKind::TrackParameters => ParamRef::BankItem { bank: ParameterBankId::CursorTrack, index },
            _ => self.parameter(ctx, index),
        };
        if self.kind == McuModeKind::Master || parameter.does_exist(ctx) {
            parameter.touch_value(ctx, is_touched);
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        let kind = self.kind;
        match kind {
            McuModeKind::Master => return self.master_on_button(ctx, row, index, event),
            McuModeKind::Device(_) if self.device_on_button(ctx, row, index, event) => return,
            _ => {}
        }
        if event != ButtonEvent::Down {
            return;
        }
        // Mode specific
        if row == 0 {
            self.reset_parameter_index(ctx, index);
            return;
        }
        // Record Arm, Solo, Mute
        if self.kind.is_layer() {
            self.layer_on_button(ctx, row, index);
        } else {
            self.track_on_button(ctx, row, index);
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        use super::definition::{MCU_BUTTON_STATE_OFF as OFF, MCU_BUTTON_STATE_ON as ON};
        let offset = extender_offset(ctx);
        let state = |b: bool| if b { ON } else { OFF };
        for i in 0..8 {
            if self.kind.is_layer() {
                let channel = layer_channel(ctx, offset + i);
                let exists = channel.is_some_and(|c| c.does_exist());
                if button == ButtonId::get(ButtonId::RowSelect1, i) {
                    return state(exists && channel.is_some_and(|c| c.is_selected()));
                }
                if button == ButtonId::get(ButtonId::Row21, i) {
                    return OFF;
                }
                if button == ButtonId::get(ButtonId::Row31, i) {
                    return state(exists && channel.is_some_and(|c| c.is_solo()));
                }
                if button == ButtonId::get(ButtonId::Row41, i) {
                    return state(exists && channel.is_some_and(|c| c.is_mute()));
                }
                continue;
            }
            let tb = ctx.model.current_track_bank();
            if offset + i >= tb.page_size() {
                continue;
            }
            let track = tb.item(offset + i);
            let exists = track.does_exist();
            if button == ButtonId::get(ButtonId::RowSelect1, i) {
                return state(exists && track.is_selected());
            }
            if button == ButtonId::get(ButtonId::Row21, i) {
                return state(exists && track.is_rec_arm());
            }
            if button == ButtonId::get(ButtonId::Row31, i) {
                return state(exists && track.is_solo());
            }
            if button == ButtonId::get(ButtonId::Row41, i) {
                return state(exists && track.is_mute());
            }
        }
        OFF
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        match self.kind {
            McuModeKind::Master => self.master_update_display(ctx),
            McuModeKind::Device(device) => self.device_update_display(ctx, device),
            McuModeKind::ProjectParameters => self.parameters_update_display(ctx, ParameterBankId::Project, "Project"),
            McuModeKind::TrackParameters => self.parameters_update_display(ctx, ParameterBankId::CursorTrack, "Track"),
            McuModeKind::Browser => self.browser_update_display(ctx),
            McuModeKind::Markers => self.marker_update_display(ctx),
            kind if kind.is_track() || kind.is_layer() => {
                self.base_update_display(ctx);
                self.update_item_indices(ctx);
            }
            _ => self.base_update_display(ctx),
        }
    }

    crate::impl_as_any!();
}

/// The mode of the master fader of the main device (former `MasterVolumeMode`): the master
/// volume, the metronome volume with Shift, the zoom with Select, or the last touched parameter.
/// It is not registered in a mode manager (as in Java), it is activated once.
pub struct MasterVolume {
    pub base: ParameterModeBase,
    control_last_param_active: bool,
}

impl MasterVolume {
    pub fn new() -> Self {
        MasterVolume { base: ParameterModeBase::new("Master Volume", true, ModeBank::None, vec![ContinuousId::FaderMaster]), control_last_param_active: false }
    }

    fn master_volume_provider() -> SharedProvider {
        provider(FixedParameterProvider::single(ParamRef::MasterVolume))
    }

    fn focused_parameter_provider() -> SharedProvider {
        provider(FixedParameterProvider::single(ParamRef::Custom(Rc::new(FocusedParameter))))
    }

    /// Former `onActivate`.
    pub fn activate(&mut self, ctx: &mut Ctx) {
        self.base.set_parameter_provider(ctx, Self::master_volume_provider());
        self.base.set_button_parameter_provider(ctx, ButtonId::Shift, provider(FixedParameterProvider::single(ParamRef::MetronomeVolume)));
        self.base.set_button_parameter_provider(ctx, ButtonId::Select, provider(FixedParameterProvider::single(ParamRef::Custom(Rc::new(ZoomParameter)))));
        self.base.on_activate(ctx);
    }

    pub fn toggle_control_last_param_active(&mut self, ctx: &mut Ctx) {
        let active = !self.control_last_param_active;
        self.set_control_last_param_active(ctx, active);
    }

    pub fn set_control_last_param_active(&mut self, ctx: &mut Ctx, active: bool) {
        self.control_last_param_active = active;
        let parameter_provider = if active { Self::focused_parameter_provider() } else { Self::master_volume_provider() };
        self.base.set_parameter_provider(ctx, parameter_provider);
        self.base.bind_controls(ctx);
    }

    pub fn is_control_last_param_active(&self) -> bool {
        self.control_last_param_active
    }
}

impl Default for MasterVolume {
    fn default() -> Self {
        Self::new()
    }
}

/// Run a function with the master volume mode of the current surface (only on the main device).
pub fn with_master_volume<R>(ctx: &mut Ctx, f: impl FnOnce(&mut MasterVolume, &mut Ctx) -> R) -> Option<R> {
    let mut master_volume = mcu_mut(ctx).master_volume.take()?;
    let result = f(&mut master_volume, ctx);
    mcu_mut(ctx).master_volume = Some(master_volume);
    Some(result)
}

/// The master volume mode controls the last touched parameter (on any surface).
pub fn is_master_control_last_param_active(ctx: &Ctx) -> bool {
    if let Some(mv) = &mcu(ctx).master_volume {
        return mv.is_control_last_param_active();
    }
    (0..ctx.surface_count()).any(|i| {
        ctx.parked_surface(i).is_some_and(|p| p.ext.downcast_ref::<super::surface::McuSurfaceState>().and_then(|s| s.master_volume.as_ref()).is_some_and(|m| m.is_control_last_param_active()))
    })
}
