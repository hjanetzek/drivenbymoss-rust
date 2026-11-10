// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cursor device, the first instrument (`/primary`) and the equalizer (`/eq`) with their
//! parameters, siblings, layers and drum pads (former `DeviceModule`).
//!
//! Differences to Java: the core has no commands to rename a layer or to enter its device chain:
//! `layer/{n}/name` and `layer/{n}/enter` do nothing. The parent of a layer (`layer/parent`) is
//! not supported by the model either.

use super::super::address::{Dir, DeviceIn, DeviceKeys, DeviceRef, DeviceToggle, EqParam, FaderOp, LayerCmd, LayerKeys, ParamKeys};
use super::project::{execute_param_op, flush_parameter_bank};
use super::{flush_parameter_data, flush_send_data, is_trigger, layer_index, to_integer, to_string, toggle_or_set, OscError, OscResult, OscShared, Value};
use crate::daw::bank::BankItem;
use crate::daw::channel::Channel;
use crate::daw::constants::DeviceId;
use crate::daw::device::{EqualizerBandType, SpecificDevice};
use crate::daw::events::ParameterBankId;
use crate::daw::parameter::Parameter;
use crate::framework::commands::general::focused_parameter;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::{Key, KeyGroup, KeyList, OscWriter};
use crate::framework::parameterprovider::{parameter_bank_mut, resolve, resolve_mut, ParamRef};

/// The device of a command.
pub fn device(ctx: &Ctx, which: DeviceRef) -> Option<&SpecificDevice> {
    match which {
        DeviceRef::Cursor => Some(ctx.model.cursor_device()),
        DeviceRef::Primary => ctx.model.specific_device(DeviceId::FirstInstrument),
        DeviceRef::Eq => ctx.model.specific_device(DeviceId::Eq),
    }
}

fn device_mut(ctx: &mut Ctx, which: DeviceRef) -> Option<&mut SpecificDevice> {
    match which {
        DeviceRef::Cursor => Some(ctx.model.cursor_device_mut()),
        DeviceRef::Primary => ctx.model.specific_device_mut(DeviceId::FirstInstrument),
        DeviceRef::Eq => ctx.model.specific_device_mut(DeviceId::Eq),
    }
}

/// The names of the equalizer band types (the Java enum names in lower case).
fn band_type_name(band_type: EqualizerBandType) -> &'static str {
    match band_type {
        EqualizerBandType::Off => "off",
        EqualizerBandType::Lowcut => "lowcut",
        EqualizerBandType::Lowshelf => "lowshelf",
        EqualizerBandType::Bell => "bell",
        EqualizerBandType::Highcut => "highcut",
        EqualizerBandType::Highshelf => "highshelf",
        EqualizerBandType::Notch => "notch",
    }
}

/// The equalizer parameters of a band (former `getGainParameter` etc.).
fn eq_parameter(index: usize) -> ParamRef {
    ParamRef::BankItem { bank: ParameterBankId::Eq, index }
}

/// Former `DeviceModule`.
pub struct DeviceModule {
    /// The size of a page of the parameter banks, for the empty pages if there is no bank.
    page_size: usize,
    cursor: DeviceKeys,
    pinned: Key<bool>,
    drum_pads: KeyList<LayerKeys>,
    layers: KeyList<LayerKeys>,
    selected_layer: LayerKeys,
    primary: DeviceKeys,
    eq: DeviceKeys,
    last_param: ParamKeys,
}

impl DeviceModule {
    pub fn new(w: &mut OscWriter, page_size: usize) -> Self {
        DeviceModule {
            page_size,
            cursor: DeviceKeys::new(w, "/device/"),
            pinned: w.key("/device/pinned"),
            drum_pads: KeyList::new("/device/drumpad/".into()),
            layers: KeyList::new("/device/layer/".into()),
            selected_layer: LayerKeys::new(w, "/device/layer/selected/"),
            primary: DeviceKeys::new(w, "/primary/"),
            eq: DeviceKeys::new(w, "/eq/"),
            last_param: ParamKeys::new(w, "/device/lastparam/"),
        }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: DeviceIn, value: Value) -> OscResult {
        match command {
            DeviceIn::SiblingSelect { sibling } => {
                if is_trigger(value) {
                    let index = super::super::address::index_of(sibling)?;
                    ctx.model.cursor_device_mut().device_bank_mut().item_mut(index).select();
                }
            }
            DeviceIn::BankPage { dir } => {
                let device_bank = ctx.model.cursor_device_mut().device_bank_mut();
                match dir {
                    Dir::Next => device_bank.select_next_page(),
                    Dir::Previous => device_bank.select_previous_page(),
                }
            }
            DeviceIn::LastParam { op } => {
                let focused = focused_parameter(ctx);
                if let Some(parameter) = resolve_mut(&mut ctx.model, &focused) {
                    execute_param_op(parameter, op, value)?;
                }
            }
            DeviceIn::SelectAdjacent { dir } => {
                if is_trigger(value) {
                    match dir {
                        Dir::Next => ctx.model.cursor_device_mut().select_next(),
                        Dir::Previous => ctx.model.cursor_device_mut().select_previous(),
                    }
                }
            }
            DeviceIn::Pinned {} => toggle_or_set(ctx.model.cursor_device_mut(), value, |d| d.toggle_pinned(), |d, on| d.set_pinned(on)),
            DeviceIn::Toggle { device, toggle } => {
                if let Some(device) = device_mut(ctx, device) {
                    match toggle {
                        DeviceToggle::Duplicate => device.duplicate(),
                        DeviceToggle::Remove => device.remove(),
                        DeviceToggle::Bypass => device.toggle_enabled_state(),
                        DeviceToggle::Expand => device.toggle_expanded(),
                        DeviceToggle::Parameters => device.toggle_parameter_page_section_visible(),
                        DeviceToggle::Window => device.toggle_window_open(),
                    }
                }
            }
            DeviceIn::IndicateParams { device } => {
                if let Some(bank) = parameter_bank_mut(&mut ctx.model, super::super::address::ParamBank::Device(device).id()) {
                    for i in 0..bank.page_size() {
                        bank.item_mut(i).set_indication(is_trigger(value));
                    }
                }
            }
            DeviceIn::Unknown { command } => ctx.host.println(&format!("Unknown Device command: {command}")),
            DeviceIn::Layer { device: which, layer, cmd } => {
                // The address table checked the index and the device
                let Ok(layer_index) = usize::try_from(layer_index(ctx, which, layer)) else {
                    return Ok(());
                };
                let Some(device) = device_mut(ctx, which) else {
                    return Ok(());
                };
                execute_layer(device.layer_bank_mut().item_mut(layer_index), cmd, value)?;
            }
            DeviceIn::LayerParent { device: which } => {
                if let Some(device) = device_mut(ctx, which) {
                    if device.does_exist() && which == DeviceRef::Cursor {
                        device.select_parent();
                        device.select_channel();
                    }
                }
            }
            DeviceIn::LayerSelectAdjacent { device, dir } => {
                if let Some(device) = device_mut(ctx, device) {
                    match dir {
                        Dir::Next => device.layer_bank_mut().select_next_item(),
                        Dir::Previous => device.layer_bank_mut().select_previous_item(),
                    }
                }
            }
            DeviceIn::LayerPage { device, dir } => {
                let Some(dir) = dir else {
                    ctx.host.println("Missing Layer/Drumpad Page subcommand: page");
                    return Ok(());
                };
                if let Some(device) = device_mut(ctx, device) {
                    match dir {
                        Dir::Next => device.layer_bank_mut().select_next_page(),
                        Dir::Previous => device.layer_bank_mut().select_previous_page(),
                    }
                }
            }
            DeviceIn::EqType { band } => {
                let name = to_string(value)?.to_uppercase();
                let band_type = EqualizerBandType::ALL
                    .iter()
                    .copied()
                    .find(|t| band_type_name(*t).to_uppercase() == name)
                    .ok_or_else(|| OscError::Runtime(format!("No enum constant EqualizerBandType.{name}")))?;
                if let Some(device) = ctx.model.specific_device_mut(DeviceId::Eq) {
                    device.set_type(band, band_type);
                }
            }
            DeviceIn::EqParam { band, param } => {
                let index = match param {
                    EqParam::Gain => SpecificDevice::gain_parameter_index(band),
                    EqParam::Freq => SpecificDevice::frequency_parameter_index(band),
                    EqParam::Q => SpecificDevice::q_parameter_index(band),
                };
                let new_value = to_integer(value)?;
                if let Some(parameter) = resolve_mut(&mut ctx.model, &eq_parameter(index)) {
                    parameter.set_value(new_value);
                }
            }
            DeviceIn::EqAdd {} => {
                if ctx.model.cursor_track().does_exist() && is_trigger(value) {
                    ctx.model.cursor_track_mut().add_equalizer_device();
                }
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let cd = ctx.model.cursor_device();
        flush_device(ctx, w, &mut self.cursor, DeviceRef::Cursor, self.page_size, dump);
        w.put(self.pinned, cd.is_pinned(), dump);
        let vu = ctx.config.is_enable_vu_meters();
        if cd.has_drum_pads() {
            let drum_pads = cd.drum_pad_bank();
            for i in 0..drum_pads.page_size() {
                let keys = self.drum_pads.at(w, i);
                flush_device_layer(w, keys, drum_pads.item(i), vu, dump);
            }
        }
        let layers = cd.layer_bank();
        for i in 0..layers.page_size() {
            let keys = self.layers.at(w, i);
            flush_device_layer(w, keys, layers.item(i), vu, dump);
        }
        match layers.selected_item() {
            Some(layer) => flush_device_layer(w, &mut self.selected_layer, layer, vu, dump),
            None => flush_device_layer(w, &mut self.selected_layer, &Channel::empty(ctx.model.ctx(), layers.page_size()), vu, dump),
        }

        flush_device(ctx, w, &mut self.primary, DeviceRef::Primary, self.page_size, dump);
        flush_device(ctx, w, &mut self.eq, DeviceRef::Eq, self.page_size, dump);

        // Last hovered/clicked parameter
        let empty = Parameter::empty(ctx.model.ctx());
        let focused = focused_parameter(ctx);
        let parameter = resolve(&ctx.model, &focused).unwrap_or(&empty);
        flush_parameter_data(w, &self.last_param, parameter, dump);
    }
}

/// Former `flushDevice`.
fn flush_device(ctx: &Ctx, w: &mut OscWriter, k: &mut DeviceKeys, which: DeviceRef, page_size: usize, dump: bool) {
    let device = device(ctx, which);
    w.put(k.exists, device.is_some_and(|d| d.does_exist()), dump);
    w.put(k.name, device.map(|d| d.name()).unwrap_or_default(), dump);
    w.put(k.bypass, !device.is_some_and(|d| d.is_enabled()), dump);
    w.put(k.expand, device.is_some_and(|d| d.is_expanded()), dump);
    w.put(k.parameters, device.is_some_and(|d| d.is_parameter_page_section_visible()), dump);
    w.put(k.window, device.is_some_and(|d| d.is_window_open()), dump);

    if which == DeviceRef::Eq {
        let empty = Parameter::empty(ctx.model.ctx());
        let band_count = device.map(|d| d.band_count()).unwrap_or(crate::daw::device::EQ_NUMBER_OF_BANDS);
        for i in 0..band_count {
            let band_type = device.map(|d| d.type_id(i)).unwrap_or_default();
            let keys = k.band_types.at(w, i);
            w.put(keys.kind, band_type_name(band_type), dump);
            let parameter = |index: usize| resolve(&ctx.model, &eq_parameter(index)).unwrap_or(&empty);
            let keys = k.gains.at(w, i);
            flush_parameter_data(w, keys, parameter(SpecificDevice::gain_parameter_index(i)), dump);
            let keys = k.freqs.at(w, i);
            flush_parameter_data(w, keys, parameter(SpecificDevice::frequency_parameter_index(i)), dump);
            let keys = k.qs.at(w, i);
            flush_parameter_data(w, keys, parameter(SpecificDevice::q_parameter_index(i)), dump);
        }
        return;
    }

    if let (DeviceRef::Cursor, Some(cursor_device)) = (which, device) {
        let position_in_bank = cursor_device.index();
        let device_bank = cursor_device.device_bank();
        for i in 0..device_bank.page_size() {
            let sibling = device_bank.item(i);
            let s = k.siblings.at(w, i);
            w.put(s.exists, sibling.does_exist(), dump);
            w.put(s.name, sibling.name(), dump);
            w.put(s.bypass, !sibling.is_enabled(), dump);
            w.put(s.selected, i as i32 == position_in_bank, dump);
        }
    }

    flush_parameter_bank(w, &mut k.bank, device.and_then(|d| d.parameter_bank()), page_size, &ctx.model, dump);
}

/// Former `flushDeviceLayer`.
fn flush_device_layer(w: &mut OscWriter, k: &mut LayerKeys, channel: &Channel, vu: bool, dump: bool) {
    w.put(k.exists, channel.does_exist(), dump);
    w.put(k.activated, channel.is_activated(), dump);
    w.put(k.selected, channel.is_selected(), dump);
    w.put(k.name, channel.name(), dump);
    w.put(k.volume_str, &channel.volume_str(), dump);
    w.put(k.volume, channel.volume(), dump);
    w.put(k.pan_str, &channel.pan_str(), dump);
    w.put(k.pan, channel.pan(), dump);
    w.put(k.mute, channel.is_mute(), dump);
    w.put(k.solo, channel.is_solo(), dump);
    let send_bank = channel.send_bank();
    for i in 0..send_bank.page_size() {
        let keys = k.sends.at(w, i);
        flush_send_data(w, keys, send_bank.item(i), dump);
    }
    if vu {
        w.put(k.vu, channel.vu(), dump);
    }
    w.put(k.color, channel.color(), dump);
}

/// The commands of a layer (former `parseDeviceLayerValue`).
fn execute_layer(layer: &mut Channel, cmd: LayerCmd, value: Value) -> OscResult {
    match cmd {
        LayerCmd::Activated => layer.set_is_activated(to_integer(value)? > 0),
        LayerCmd::Select => layer.select(),
        // The core cannot rename a layer
        LayerCmd::Name => {}
        LayerCmd::Volume(op) => match op {
            FaderOp::Set => layer.set_volume(to_integer(value)?),
            FaderOp::Indicate => layer.set_volume_indication(is_trigger(value)),
            FaderOp::Reset => layer.reset_volume(),
            FaderOp::Touched => layer.touch_volume(is_trigger(value)),
            FaderOp::Ignore => {}
        },
        LayerCmd::Pan(op) => match op {
            FaderOp::Set => layer.set_pan(to_integer(value)?),
            FaderOp::Indicate => layer.set_pan_indication(is_trigger(value)),
            FaderOp::Reset => layer.reset_pan(),
            FaderOp::Touched => layer.touch_pan(is_trigger(value)),
            FaderOp::Ignore => {}
        },
        LayerCmd::Mute => toggle_or_set(layer, value, |l| l.toggle_mute(), |l, on| l.set_mute(on)),
        LayerCmd::Solo => toggle_or_set(layer, value, |l| l.toggle_solo(), |l, on| l.set_solo(on)),
        LayerCmd::SendVolume(send, op) => {
            let send = layer.send_bank_mut().item_mut(send);
            match op {
                FaderOp::Set => send.set_value(to_integer(value)?),
                FaderOp::Indicate => send.set_indication(is_trigger(value)),
                FaderOp::Touched => send.touch_value(is_trigger(value)),
                FaderOp::Reset | FaderOp::Ignore => {}
            }
        }
        LayerCmd::SendActivated(send) => {
            if is_trigger(value) {
                layer.send_bank_mut().item_mut(send).toggle_enabled();
            }
        }
        LayerCmd::SendNothing(_) => {}
        // The core cannot enter the device chain of a layer
        LayerCmd::Enter => {}
    }
    Ok(())
}
