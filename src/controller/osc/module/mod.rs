// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The OSC address modules (former `controller.osc.module`): each one executes the commands of
//! its part of the address table ([`super::address`]; the setup parses the address and
//! dispatches the typed command to the module, which `match`es its commands exhaustively) and
//! sends the values of its part of the model (`flush`) by the keys of its addresses.
//!
//! Differences to Java:
//! - The exceptions (`IllegalParameterException`, `UnknownCommandException`,
//!   `MissingCommandException`) are the variants of [`OscError`]; the Java runtime exceptions
//!   which escape the modules (e.g. a `NumberFormatException` of a send index or an
//!   `IndexOutOfBoundsException` of `path.get (0)`) are [`OscError::Runtime`] and are logged.
//! - The model returns an empty item for an index outside of a page instead of throwing.

pub mod action;
pub mod browser;
pub mod clip;
pub mod device;
pub mod global;
pub mod layout;
pub mod marker;
pub mod midi;
pub mod project;
pub mod scene;
pub mod track;
pub mod transport;

use std::fmt;

use super::address::{Guard, LayerIndex};
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::daw::parameter::{Parameter, Send};
use crate::framework::commands::general::focused_parameter;
use crate::framework::ctx::Ctx;
use crate::framework::osc::{OscArg, OscWriter};
use crate::framework::parameterprovider::{parameter_bank, resolve};
use crate::scale::KeyManager;

/// The result of executing a command.
pub type OscResult = Result<(), OscError>;

/// Why a command could not be executed (former exceptions of `controller.osc.exception`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscError {
    /// Former `IllegalParameterException`.
    IllegalParameter(String),
    /// Former `UnknownCommandException`.
    UnknownCommand(String),
    /// Former `MissingCommandException`.
    MissingCommand,
    /// A Java runtime exception which is not caught by the parser (logged by the host).
    Runtime(String),
}

impl fmt::Display for OscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OscError::IllegalParameter(message) | OscError::UnknownCommand(message) | OscError::Runtime(message) => f.write_str(message),
            OscError::MissingCommand => f.write_str("Missing command"),
        }
    }
}

/// The value of a command as the Java parser passes it to the modules: `null` without a value
/// (or with a nil), the value itself, or an array if there is more than one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    Null,
    One(&'a OscArg),
    Many(&'a [OscArg]),
}

impl<'a> Value<'a> {
    /// The value of the arguments of a message (former `OSCParser.handle`).
    pub fn from_args(args: &'a [OscArg]) -> Self {
        match args {
            [] | [OscArg::Nil] => Value::Null,
            [single] => Value::One(single),
            many => Value::Many(many),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Former `value.toString ()`, None for `null`.
    pub fn text(&self) -> Option<String> {
        match self {
            Value::Null => None,
            Value::One(arg) => Some(arg.java_string()),
            Value::Many(args) => Some(format!("[{}]", args.iter().map(OscArg::java_string).collect::<Vec<_>>().join(", "))),
        }
    }

    fn number(&self) -> Option<f64> {
        match self {
            Value::One(arg) => arg.as_number(),
            _ => None,
        }
    }
}

/// The state which the modules share (in Java the writer and the key manager are given to the
/// constructors of the modules).
pub struct OscShared {
    pub writer: OscWriter,
    pub key_manager: KeyManager,
}

impl OscShared {
    pub fn new() -> Self {
        OscShared { writer: OscWriter::new(), key_manager: KeyManager::new() }
    }
}

impl Default for OscShared {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers (former static methods of AbstractModule)

/// True for no value or a number larger than 0 (former `isTrigger`). Booleans and strings are
/// no triggers.
pub fn is_trigger(value: Value) -> bool {
    value.is_null() || value.number().is_some_and(|n| n > 0.0)
}

/// Former `toNumber`.
pub fn to_number(value: Value) -> Result<f64, OscError> {
    if value.is_null() {
        return Err(OscError::IllegalParameter("Number parameter missing".into()));
    }
    value.number().ok_or_else(|| OscError::IllegalParameter("Parameter is not a Number".into()))
}

/// Former `toNumber (value, defaultValue)`.
pub fn to_number_or(value: Value, default_value: f64) -> Result<f64, OscError> {
    if value.is_null() {
        return Ok(default_value);
    }
    value.number().ok_or_else(|| OscError::IllegalParameter("Parameter is not a Number".into()))
}

/// Former `toInteger`: the number truncated towards zero (Java `(int)` of a double).
pub fn to_integer(value: Value) -> Result<i32, OscError> {
    to_number(value).map(|n| n as i32)
}

/// Former `toString`.
pub fn to_string(value: Value) -> Result<String, OscError> {
    value.text().ok_or_else(|| OscError::IllegalParameter("String parameter missing".into()))
}

/// Java `value.toString ()`, which throws for no value.
pub fn text_or_throw(value: Value) -> Result<String, OscError> {
    value.text().ok_or_else(|| OscError::Runtime("Cannot invoke \"Object.toString()\" because \"value\" is null".into()))
}

/// Parse a color in the format `rgb(r,g,b)` with r, g, b in the range of [0..255] (former
/// `matchColor`). The numbers may have fractions.
pub fn match_color(value: &str) -> Option<ColorEx> {
    let inner = value.strip_prefix("rgb(").or_else(|| value.strip_prefix("RGB("))?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let is_number = |part: &str| {
        let (int, fraction) = match part.split_once('.') {
            Some((int, fraction)) => (int, Some(fraction)),
            None => (part, None),
        };
        !int.is_empty() && int.bytes().all(|b| b.is_ascii_digit()) && fraction.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
    };
    if !parts.iter().all(|p| is_number(p)) {
        return None;
    }
    let component = |part: &str| part.parse::<f64>().unwrap_or(0.0) / 255.0;
    Some(ColorEx::new(component(parts[0]), component(parts[1]), component(parts[2])))
}

/// Toggle a value without a value, set it to the trigger state otherwise (the common pattern
/// `value == null ? toggle () : set (isTrigger (value))`).
pub fn toggle_or_set<T: ?Sized>(target: &mut T, value: Value, toggle: impl FnOnce(&mut T), set: impl FnOnce(&mut T, bool)) {
    if value.is_null() {
        toggle(target);
    } else {
        set(target, is_trigger(value));
    }
}

/// Send the data of a parameter (former `flushParameterData` for a non-send parameter).
pub fn flush_parameter_data(w: &mut OscWriter, keys: &super::address::ParamKeys, parameter: &Parameter, dump: bool) {
    w.put(keys.exists, parameter.does_exist(), dump);
    w.put(keys.name, parameter.name(), dump);
    w.put(keys.value_str, &parameter.displayed_value(), dump);
    w.put(keys.value, parameter.value(), dump);
    w.put(keys.modulated_value, parameter.modulated_value(), dump);
}

/// Send the data of a send (former `flushParameterData` for an `ISend`).
pub fn flush_send_data(w: &mut OscWriter, keys: &super::address::SendKeys, send: &Send, dump: bool) {
    w.put(keys.activated, send.is_enabled(), dump);
    w.put(keys.exists, send.does_exist(), dump);
    w.put(keys.name, send.name(), dump);
    w.put(keys.volume_str, &send.displayed_value(), dump);
    w.put(keys.volume, send.value(), dump);
    w.put(keys.modulated_value, send.modulated_value(), dump);
}

// ---------------------------------------------------------------------------------------------
// The guards of the address table

/// The index of a layer or drum pad of a device: the selected one (-1 for none) or the number
/// of the address.
pub fn layer_index(ctx: &Ctx, device: super::address::DeviceRef, layer: LayerIndex) -> i32 {
    match layer {
        LayerIndex::Selected => device::device(ctx, device).and_then(|d| d.layer_bank().selected_item()).map_or(-1, |l| l.index()),
        LayerIndex::Number(index) => index,
    }
}

/// Check a guard of a command (see [`super::address`]): false if the command does nothing; an
/// error if Java fails there.
pub fn check_guard(ctx: &mut Ctx, guard: &Guard, value: Value) -> Result<bool, OscError> {
    Ok(match guard {
        Guard::CursorTrackExists => ctx.model.cursor_track().does_exist(),
        Guard::HasDrumPads(which) => device::device(ctx, *which).is_some_and(|d| d.has_drum_pads()),
        Guard::DeviceExists(which) => device::device(ctx, *which).is_some(),
        Guard::LayerInRange(which, layer) => {
            let layer_index = layer_index(ctx, *which, *layer);
            let page_size = device::device(ctx, *which).map_or(0, |d| d.layer_bank().page_size());
            if layer_index >= page_size as i32 {
                ctx.host.println(&format!("Layer or drumpad index larger than page size: {layer_index}"));
                return Ok(false);
            }
            if layer_index < 0 {
                return Err(OscError::Runtime(format!("Index {layer_index} out of bounds for length {page_size}")));
            }
            device::device(ctx, *which).is_some()
        }
        Guard::Trigger => is_trigger(value),
        Guard::ParamBankExists(bank) => parameter_bank(&ctx.model, bank.id()).is_some(),
        Guard::FocusedParamExists => {
            let focused = focused_parameter(ctx);
            focused.does_exist(ctx) && resolve(&ctx.model, &focused).is_some()
        }
        Guard::BrowserActive => ctx.model.browser().is_some_and(|b| b.is_active()),
        Guard::BrowserFilterColumn(column) => (1..=6).contains(column) && ctx.model.browser().is_some_and(|b| b.is_active()),
        Guard::CursorSlotSelected => ctx.model.cursor_track().slot_bank().selected_item().is_some_and(|s| s.index() >= 0),
        Guard::IntValue => {
            to_integer(value)?;
            true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        assert_eq!(Value::from_args(&[]), Value::Null);
        assert_eq!(Value::from_args(&[OscArg::Nil]), Value::Null);
        let one = [OscArg::Float(0.5)];
        assert_eq!(Value::from_args(&one), Value::One(&one[0]));
        let two = [OscArg::Int(60), OscArg::Int(127)];
        assert_eq!(Value::from_args(&two), Value::Many(&two));
        assert_eq!(Value::from_args(&two).text().as_deref(), Some("[60, 127]"));

        assert!(is_trigger(Value::Null));
        assert!(is_trigger(Value::One(&OscArg::Int(1))));
        assert!(is_trigger(Value::One(&OscArg::Float(0.1))));
        assert!(!is_trigger(Value::One(&OscArg::Int(0))));
        assert!(!is_trigger(Value::One(&OscArg::Int(-1))));
        assert!(!is_trigger(Value::One(&OscArg::Bool(true))), "a boolean is no number in Java");
        assert!(!is_trigger(Value::One(&OscArg::Str("1".into()))));
        assert!(!is_trigger(Value::Many(&two)));

        assert_eq!(to_integer(Value::One(&OscArg::Float(99.9))), Ok(99));
        assert_eq!(to_integer(Value::One(&OscArg::Double(-1.7))), Ok(-1));
        assert_eq!(to_integer(Value::Null), Err(OscError::IllegalParameter("Number parameter missing".into())));
        assert_eq!(to_integer(Value::One(&OscArg::Str("5".into()))), Err(OscError::IllegalParameter("Parameter is not a Number".into())));
        assert_eq!(to_number_or(Value::Null, 1.0), Ok(1.0));
        assert_eq!(to_string(Value::One(&OscArg::Float(2.0))), Ok("2.0".into()));
        assert!(to_string(Value::Null).is_err());
    }

    #[test]
    fn colors() {
        let color = match_color("rgb(255,0,127.5)").unwrap();
        assert_eq!(color.to_double_rgb(), [1.0, 0.0, 0.5]);
        assert!(match_color("RGB(1,2,3)").is_some());
        assert!(match_color("rgb(1, 2, 3)").is_none(), "no spaces");
        assert!(match_color("rgb(1,2)").is_none());
        assert!(match_color("rgb(1,2,3").is_none());
        assert!(match_color("rgb(1.,2,3)").is_none());
        assert!(match_color("#ff0000").is_none());
    }
}
