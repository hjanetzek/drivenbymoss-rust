// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cursor device, the first instrument (`/primary`) and the equalizer (`/eq`) with their
//! parameters, siblings, layers and drum pads (former `DeviceModule`).
//!
//! Differences to Java: the core has no commands to rename a layer or to enter its device chain:
//! `layer/{n}/name` and `layer/{n}/enter` do nothing. The parent of a layer (`layer/parent`) is
//! not supported by the model either.

use super::project::flush_parameter_bank;
use super::{
    first, flush_parameter_data, flush_send_data, index_of, is_trigger, parse_fx_param_value, parse_int, parse_int_or_throw, send_color, sub_command, to_integer,
    to_string, OscError, OscModule, OscResult, OscShared, Path, Value,
};
use super::{TAG_ACTIVATED, TAG_BYPASS, TAG_DUPLICATE, TAG_EXISTS, TAG_INDICATE, TAG_NAME, TAG_PAGE, TAG_PARAM, TAG_REMOVE, TAG_RESET, TAG_SELECT, TAG_SELECTED};
use super::{TAG_TOUCHED, TAG_VOLUME};
use crate::daw::bank::BankItem;
use crate::daw::channel::Channel;
use crate::daw::constants::DeviceId;
use crate::daw::device::{EqualizerBandType, SpecificDevice};
use crate::daw::events::ParameterBankId;
use crate::daw::parameter::Parameter;
use crate::framework::commands::general::focused_parameter;
use crate::framework::ctx::Ctx;
use crate::framework::osc::OscWriter;
use crate::framework::parameterprovider::{parameter_bank_mut, resolve, resolve_mut, ParamRef};

/// The device which a command addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    Cursor,
    Primary,
    Eq,
}

impl Which {
    fn bank(self) -> ParameterBankId {
        match self {
            Which::Cursor => ParameterBankId::CursorDevice,
            Which::Primary => ParameterBankId::FirstInstrument,
            Which::Eq => ParameterBankId::Eq,
        }
    }
}

fn device(ctx: &Ctx, which: Which) -> Option<&SpecificDevice> {
    match which {
        Which::Cursor => Some(ctx.model.cursor_device()),
        Which::Primary => ctx.model.specific_device(DeviceId::FirstInstrument),
        Which::Eq => ctx.model.specific_device(DeviceId::Eq),
    }
}

fn device_mut(ctx: &mut Ctx, which: Which) -> Option<&mut SpecificDevice> {
    match which {
        Which::Cursor => Some(ctx.model.cursor_device_mut()),
        Which::Primary => ctx.model.specific_device_mut(DeviceId::FirstInstrument),
        Which::Eq => ctx.model.specific_device_mut(DeviceId::Eq),
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
}

impl DeviceModule {
    pub fn new(page_size: usize) -> Self {
        DeviceModule { page_size }
    }
}

impl OscModule for DeviceModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["device", "primary", "eq"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        match command {
            "device" => parse_cursor_device_value(ctx, path, value),
            "primary" => parse_device_value(ctx, Which::Primary, path, value),
            "eq" => {
                if !parse_eq_value(ctx, path, value)? {
                    parse_device_value(ctx, Which::Eq, path, value)?;
                }
                Ok(())
            }
            _ => Err(OscError::UnknownCommand(command.to_string())),
        }
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let cd = ctx.model.cursor_device();
        self.flush_device(ctx, w, "/device/", Which::Cursor, dump);
        w.send_bool("/device/pinned", cd.is_pinned(), dump);
        let vu = ctx.config.is_enable_vu_meters();
        if cd.has_drum_pads() {
            let drum_pads = cd.drum_pad_bank();
            for i in 0..drum_pads.page_size() {
                flush_device_layer(w, &format!("/device/drumpad/{}/", i + 1), drum_pads.item(i), vu, dump);
            }
        }
        let layers = cd.layer_bank();
        for i in 0..layers.page_size() {
            flush_device_layer(w, &format!("/device/layer/{}/", i + 1), layers.item(i), vu, dump);
        }
        match layers.selected_item() {
            Some(layer) => flush_device_layer(w, "/device/layer/selected/", layer, vu, dump),
            None => flush_device_layer(w, "/device/layer/selected/", &Channel::empty(ctx.model.ctx(), layers.page_size()), vu, dump),
        }

        self.flush_device(ctx, w, "/primary/", Which::Primary, dump);
        self.flush_device(ctx, w, "/eq/", Which::Eq, dump);

        // Last hovered/clicked parameter
        let empty = Parameter::empty(ctx.model.ctx());
        let focused = focused_parameter(ctx);
        let parameter = resolve(&ctx.model, &focused).unwrap_or(&empty);
        flush_parameter_data(w, "/device/lastparam/", parameter, dump);
    }
}

impl DeviceModule {
    /// Former `flushDevice`.
    fn flush_device(&self, ctx: &Ctx, w: &mut OscWriter, address: &str, which: Which, dump: bool) {
        let device = device(ctx, which);
        let a = |tag: &str| format!("{address}{tag}");
        w.send_bool(&a(TAG_EXISTS), device.is_some_and(|d| d.does_exist()), dump);
        w.send_str(&a(TAG_NAME), device.map(|d| d.name()).unwrap_or_default(), dump);
        w.send_bool(&a(TAG_BYPASS), !device.is_some_and(|d| d.is_enabled()), dump);
        w.send_bool(&a("expand"), device.is_some_and(|d| d.is_expanded()), dump);
        w.send_bool(&a("parameters"), device.is_some_and(|d| d.is_parameter_page_section_visible()), dump);
        w.send_bool(&a("window"), device.is_some_and(|d| d.is_window_open()), dump);

        if which == Which::Eq {
            let empty = Parameter::empty(ctx.model.ctx());
            let band_count = device.map(|d| d.band_count()).unwrap_or(crate::daw::device::EQ_NUMBER_OF_BANDS);
            for i in 0..band_count {
                let band = i + 1;
                let band_type = device.map(|d| d.type_id(i)).unwrap_or_default();
                w.send_str(&a(&format!("type/{band}/value")), band_type_name(band_type), dump);
                let parameter = |index: usize| resolve(&ctx.model, &eq_parameter(index)).unwrap_or(&empty);
                flush_parameter_data(w, &a(&format!("gain/{band}/")), parameter(SpecificDevice::gain_parameter_index(i)), dump);
                flush_parameter_data(w, &a(&format!("freq/{band}/")), parameter(SpecificDevice::frequency_parameter_index(i)), dump);
                flush_parameter_data(w, &a(&format!("q/{band}/")), parameter(SpecificDevice::q_parameter_index(i)), dump);
            }
            return;
        }

        if let (Which::Cursor, Some(cursor_device)) = (which, device) {
            let position_in_bank = cursor_device.index();
            let device_bank = cursor_device.device_bank();
            for i in 0..device_bank.page_size() {
                let sibling = device_bank.item(i);
                let s = format!("{address}sibling/{}/", i + 1);
                w.send_bool(&format!("{s}{TAG_EXISTS}"), sibling.does_exist(), dump);
                w.send_str(&format!("{s}{TAG_NAME}"), sibling.name(), dump);
                w.send_bool(&format!("{s}{TAG_BYPASS}"), !sibling.is_enabled(), dump);
                w.send_bool(&format!("{s}{TAG_SELECTED}"), i as i32 == position_in_bank, dump);
            }
        }

        flush_parameter_bank(w, address, device.and_then(|d| d.parameter_bank()), self.page_size, &ctx.model, dump);
    }
}

/// Former `flushDeviceLayer`.
fn flush_device_layer(w: &mut OscWriter, address: &str, channel: &Channel, vu: bool, dump: bool) {
    let a = |tag: &str| format!("{address}{tag}");
    w.send_bool(&a(TAG_EXISTS), channel.does_exist(), dump);
    w.send_bool(&a(TAG_ACTIVATED), channel.is_activated(), dump);
    w.send_bool(&a(TAG_SELECTED), channel.is_selected(), dump);
    w.send_str(&a(TAG_NAME), channel.name(), dump);
    w.send_str(&a("volumeStr"), &channel.volume_str(), dump);
    w.send_int(&a(TAG_VOLUME), channel.volume(), dump);
    w.send_str(&a("panStr"), &channel.pan_str(), dump);
    w.send_int(&a("pan"), channel.pan(), dump);
    w.send_bool(&a("mute"), channel.is_mute(), dump);
    w.send_bool(&a("solo"), channel.is_solo(), dump);
    let send_bank = channel.send_bank();
    for i in 0..send_bank.page_size() {
        flush_send_data(w, &format!("{address}send/{}/", i + 1), send_bank.item(i), dump);
    }
    if vu {
        w.send_int(&a("vu"), channel.vu(), dump);
    }
    send_color(w, &a("color"), channel.color(), dump);
}

/// The commands of the cursor device (former `parseCursorDeviceValue`).
fn parse_cursor_device_value(ctx: &mut Ctx, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    match command.as_str() {
        "sibling" => {
            let sibling_no = parse_int_or_throw(&sub_command(path)?)?;
            let sub = sub_command(path)?;
            match sub.as_str() {
                TAG_SELECT | TAG_SELECTED => {
                    if is_trigger(value) {
                        let index = index_of(sibling_no)?;
                        ctx.model.cursor_device_mut().device_bank_mut().item_mut(index).select();
                    }
                }
                _ => return Err(OscError::UnknownCommand(sub)),
            }
        }
        "bank" => {
            let sub = sub_command(path)?;
            if sub != TAG_PAGE {
                return Err(OscError::UnknownCommand(sub));
            }
            let direction = sub_command(path)?;
            let device_bank = ctx.model.cursor_device_mut().device_bank_mut();
            if direction == "+" {
                device_bank.select_next_page();
            } else {
                device_bank.select_previous_page();
            }
        }
        "lastparam" => {
            let focused = focused_parameter(ctx);
            if focused.does_exist(ctx) {
                if let Some(parameter) = resolve_mut(&mut ctx.model, &focused) {
                    parse_fx_param_value(parameter, path, value)?;
                }
            }
        }
        "+" => {
            if is_trigger(value) {
                ctx.model.cursor_device_mut().select_next();
            }
        }
        "-" => {
            if is_trigger(value) {
                ctx.model.cursor_device_mut().select_previous();
            }
        }
        "pinned" => {
            let device = ctx.model.cursor_device_mut();
            if value.is_null() {
                device.toggle_pinned();
            } else {
                device.set_pinned(is_trigger(value));
            }
        }
        _ => {
            path.push_front(command);
            parse_device_value(ctx, Which::Cursor, path, value)?;
        }
    }
    Ok(())
}

/// Select a parameter page by its index in the page bank page; negative does nothing.
fn select_page(ctx: &mut Ctx, which: Which, index: i32) {
    super::project::select_page(ctx, which.bank(), index);
}

/// The commands of a device (former `parseDeviceValue`).
fn parse_device_value(ctx: &mut Ctx, which: Which, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    match command.as_str() {
        TAG_PAGE => {
            let sub = sub_command(path)?;
            match sub.as_str() {
                TAG_SELECT | TAG_SELECTED => select_page(ctx, which, to_integer(value)? - 1),
                _ => match parse_int(&sub) {
                    Some(number) => select_page(ctx, which, number - 1),
                    None => return Err(OscError::UnknownCommand(sub)),
                },
            }
        }
        TAG_DUPLICATE | TAG_REMOVE | TAG_BYPASS | "expand" | "parameters" | "window" => {
            if let Some(device) = device_mut(ctx, which) {
                match command.as_str() {
                    TAG_DUPLICATE => device.duplicate(),
                    TAG_REMOVE => device.remove(),
                    TAG_BYPASS => device.toggle_enabled_state(),
                    "expand" => device.toggle_expanded(),
                    "parameters" => device.toggle_parameter_page_section_visible(),
                    _ => device.toggle_window_open(),
                }
            }
        }
        TAG_INDICATE => {
            let sub = sub_command(path)?;
            if sub != TAG_PARAM {
                return Err(OscError::UnknownCommand(sub));
            }
            if let Some(bank) = parameter_bank_mut(&mut ctx.model, which.bank()) {
                for i in 0..bank.page_size() {
                    bank.item_mut(i).set_indication(is_trigger(value));
                }
            }
        }
        TAG_PARAM => {
            let sub = sub_command(path)?;
            if let Some(number) = parse_int(&sub) {
                let index = index_of(number)?;
                if let Some(bank) = parameter_bank_mut(&mut ctx.model, which.bank()) {
                    parse_fx_param_value(bank.item_mut(index), path, value)?;
                }
                return Ok(());
            }
            if !is_trigger(value) {
                return Ok(());
            }
            match sub.as_str() {
                "+" => {
                    if let Some(bank) = parameter_bank_mut(&mut ctx.model, which.bank()) {
                        bank.page_bank_scroll_forwards();
                    }
                }
                "-" => {
                    if let Some(bank) = parameter_bank_mut(&mut ctx.model, which.bank()) {
                        bank.page_bank_scroll_backwards();
                    }
                }
                "bank" => {
                    let sub2 = sub_command(path)?;
                    if sub2 != TAG_PAGE {
                        return Err(OscError::UnknownCommand(sub2));
                    }
                    let direction = sub_command(path)?;
                    if let Some(bank) = parameter_bank_mut(&mut ctx.model, which.bank()) {
                        if direction == "+" {
                            bank.page_bank_select_next_page();
                        } else {
                            bank.page_bank_select_previous_page();
                        }
                    }
                }
                _ => return Err(OscError::UnknownCommand(sub)),
            }
        }
        "drumpad" => {
            if device(ctx, which).is_some_and(|d| d.has_drum_pads()) {
                parse_layer_or_drumpad(ctx, which, path, value)?;
            }
        }
        "layer" => parse_layer_or_drumpad(ctx, which, path, value)?,
        _ => ctx.host.println(&format!("Unknown Device command: {command}")),
    }
    Ok(())
}

/// The commands of the equalizer (former `parseEqValue`). Returns false if the command is one
/// of the general device commands.
fn parse_eq_value(ctx: &mut Ctx, path: &mut Path, value: Value) -> Result<bool, OscError> {
    let command = sub_command(path)?;
    let band = |path: &mut Path| -> Result<usize, OscError> {
        let sub = sub_command(path)?;
        match parse_int(&sub) {
            Some(number) => index_of(number),
            None => Err(OscError::UnknownCommand(sub)),
        }
    };
    match command.as_str() {
        "type" => {
            let band = band(path)?;
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
        "gain" | "freq" | "q" => {
            let band = band(path)?;
            let index = match command.as_str() {
                "gain" => SpecificDevice::gain_parameter_index(band),
                "freq" => SpecificDevice::frequency_parameter_index(band),
                _ => SpecificDevice::q_parameter_index(band),
            };
            let new_value = to_integer(value)?;
            if let Some(parameter) = resolve_mut(&mut ctx.model, &eq_parameter(index)) {
                parameter.set_value(new_value);
            }
        }
        "add" => {
            if ctx.model.cursor_track().does_exist() && is_trigger(value) {
                ctx.model.cursor_track_mut().add_equalizer_device();
            }
        }
        _ => {
            // Let this be handled by the normal device parser
            path.push_front(command);
            return Ok(false);
        }
    }
    Ok(true)
}

/// The commands of the layers or drum pads of a device (former `parseLayerOrDrumpad`; as in
/// Java both address the layer bank).
fn parse_layer_or_drumpad(ctx: &mut Ctx, which: Which, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    let layer_index = if command == TAG_SELECTED || command == TAG_SELECT {
        Some(device(ctx, which).and_then(|d| d.layer_bank().selected_item()).map_or(-1, |l| l.index()))
    } else {
        parse_int(&command).map(|number| number - 1)
    };
    if let Some(layer_index) = layer_index {
        return parse_device_layer_value(ctx, which, layer_index, path, value);
    }
    let Some(device) = device_mut(ctx, which) else {
        return Ok(());
    };
    match command.as_str() {
        "parent" => {
            if device.does_exist() && which == Which::Cursor {
                device.select_parent();
                device.select_channel();
            }
        }
        "+" => device.layer_bank_mut().select_next_item(),
        "-" => device.layer_bank_mut().select_previous_item(),
        TAG_PAGE => {
            let Some(direction) = path.front() else {
                ctx.host.println(&format!("Missing Layer/Drumpad Page subcommand: {command}"));
                return Ok(());
            };
            if direction == "+" {
                device.layer_bank_mut().select_next_page();
            } else {
                device.layer_bank_mut().select_previous_page();
            }
        }
        _ => return Err(OscError::UnknownCommand(command)),
    }
    Ok(())
}

/// The commands of a layer (former `parseDeviceLayerValue`).
fn parse_device_layer_value(ctx: &mut Ctx, which: Which, layer_index: i32, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    let page_size = device(ctx, which).map_or(0, |d| d.layer_bank().page_size());
    if layer_index >= page_size as i32 {
        ctx.host.println(&format!("Layer or drumpad index larger than page size: {layer_index}"));
        return Ok(());
    }
    let layer_index = usize::try_from(layer_index).map_err(|_| OscError::Runtime(format!("Index {layer_index} out of bounds for length {page_size}")))?;
    let Some(device) = device_mut(ctx, which) else {
        return Ok(());
    };
    let layer = device.layer_bank_mut().item_mut(layer_index);
    match command.as_str() {
        TAG_ACTIVATED => layer.set_is_activated(to_integer(value)? > 0),
        TAG_SELECT | TAG_SELECTED => layer.select(),
        // The core cannot rename a layer
        TAG_NAME => {}
        TAG_VOLUME => {
            if path.is_empty() {
                layer.set_volume(to_integer(value)?);
            } else {
                match first(path)? {
                    TAG_INDICATE => layer.set_volume_indication(is_trigger(value)),
                    TAG_RESET => layer.reset_volume(),
                    TAG_TOUCHED => layer.touch_volume(is_trigger(value)),
                    _ => {}
                }
            }
        }
        "pan" => {
            if path.is_empty() {
                layer.set_pan(to_integer(value)?);
            } else {
                match first(path)? {
                    TAG_INDICATE => layer.set_pan_indication(is_trigger(value)),
                    TAG_RESET => layer.reset_pan(),
                    TAG_TOUCHED => layer.touch_pan(is_trigger(value)),
                    _ => {}
                }
            }
        }
        "mute" => {
            if value.is_null() {
                layer.toggle_mute();
            } else {
                layer.set_mute(is_trigger(value));
            }
        }
        "solo" => {
            if value.is_null() {
                layer.toggle_solo();
            } else {
                layer.set_solo(is_trigger(value));
            }
        }
        "send" => {
            let number = path.pop_front().ok_or_else(|| OscError::Runtime("NoSuchElementException".into()))?;
            let send_index = index_of(parse_int_or_throw(&number)?)?;
            let Some(cmd) = path.pop_front() else {
                return Ok(());
            };
            let send = layer.send_bank_mut().item_mut(send_index);
            match cmd.as_str() {
                TAG_VOLUME => {
                    if path.is_empty() {
                        send.set_value(to_integer(value)?);
                    } else {
                        match first(path)? {
                            TAG_INDICATE => send.set_indication(is_trigger(value)),
                            TAG_TOUCHED => send.touch_value(is_trigger(value)),
                            _ => {}
                        }
                    }
                }
                TAG_ACTIVATED => {
                    if is_trigger(value) {
                        send.toggle_enabled();
                    }
                }
                _ => return Err(OscError::UnknownCommand(cmd)),
            }
        }
        // The core cannot enter the device chain of a layer
        "enter" => {}
        _ => return Err(OscError::UnknownCommand(command)),
    }
    Ok(())
}
