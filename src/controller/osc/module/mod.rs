// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The OSC address modules (former `controller.osc.module`): each one handles the commands of
//! some top level addresses (`/track/...`, `/device/...`) and sends the values of its part of
//! the model (`flush`).
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

use std::collections::VecDeque;
use std::fmt;

use crate::daw::color::ColorEx;
use crate::daw::parameter::{Parameter, Send};
use crate::framework::ctx::Ctx;
use crate::framework::osc::{OscArg, OscWriter};
use crate::scale::KeyManager;

/// The remaining parts of the address (former `LinkedList<String> path`).
pub type Path = VecDeque<String>;

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

/// An OSC address module (former `IModule`).
pub trait OscModule {
    /// The top level addresses the module handles (former `getSupportedCommands`).
    fn supported_commands(&self) -> &'static [&'static str];

    /// Execute a command (former `execute`).
    fn execute(&mut self, ctx: &mut Ctx, shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult;

    /// Send the values of the module, all of them if `dump` is set, otherwise only the changed
    /// ones (former `flush`).
    fn flush(&mut self, _ctx: &Ctx, _shared: &mut OscShared, _dump: bool) {}
}

// ---------------------------------------------------------------------------------------------
// Helpers (former static methods of AbstractModule)

pub const TAG_EXISTS: &str = "exists";
pub const TAG_ACTIVATED: &str = "activated";
pub const TAG_NAME: &str = "name";
pub const TAG_SELECTED: &str = "selected";
pub const TAG_SELECT: &str = "select";
pub const TAG_DUPLICATE: &str = "duplicate";
pub const TAG_INSERT_FILE: &str = "insertFile";
pub const TAG_REMOVE: &str = "remove";
pub const TAG_VOLUME: &str = "volume";
pub const TAG_PAGE: &str = "page";
pub const TAG_INDICATE: &str = "indicate";
pub const TAG_TOUCHED: &str = "touched";
pub const TAG_COLOR: &str = "color";
pub const TAG_BYPASS: &str = "bypass";
pub const TAG_PARAM: &str = "param";
pub const TAG_MIXER: &str = "mixer";
pub const TAG_PREROLL: &str = "preroll";
pub const TAG_RESET: &str = "reset";

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

/// Former `getSubCommand`: remove the next part of the address.
pub fn sub_command(path: &mut Path) -> Result<String, OscError> {
    path.pop_front().ok_or(OscError::MissingCommand)
}

/// Former `path.get (0)`, which throws if the path is empty.
pub fn first(path: &Path) -> Result<&str, OscError> {
    path.front().map(String::as_str).ok_or_else(|| OscError::Runtime("Index 0 out of bounds for length 0".into()))
}

/// Former `Integer.parseInt`.
pub fn parse_int(text: &str) -> Option<i32> {
    text.parse::<i32>().ok()
}

/// Former `Integer.parseInt` where the `NumberFormatException` is not caught.
pub fn parse_int_or_throw(text: &str) -> Result<i32, OscError> {
    parse_int(text).ok_or_else(|| OscError::Runtime(format!("For input string: \"{text}\"")))
}

/// A 1-based index of the address as a 0-based index of a page, None if it is negative (Java
/// throws an `IndexOutOfBoundsException` for it).
pub fn index_of(one_based: i32) -> Result<usize, OscError> {
    usize::try_from(i64::from(one_based) - 1).map_err(|_| OscError::Runtime(format!("Index {} out of bounds", i64::from(one_based) - 1)))
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

/// Send the color of a model item.
pub fn send_color(writer: &mut OscWriter, address: &str, color: ColorEx, dump: bool) {
    writer.send_color(address, color.red(), color.green(), color.blue(), dump);
}

/// Send the data of a parameter (former `flushParameterData` for a non-send parameter).
pub fn flush_parameter_data(writer: &mut OscWriter, address: &str, parameter: &Parameter, dump: bool) {
    writer.send_bool(&format!("{address}{TAG_EXISTS}"), parameter.does_exist(), dump);
    writer.send_str(&format!("{address}{TAG_NAME}"), parameter.name(), dump);
    writer.send_str(&format!("{address}valueStr"), &parameter.displayed_value(), dump);
    writer.send_int(&format!("{address}value"), parameter.value(), dump);
    writer.send_int(&format!("{address}modulatedValue"), parameter.modulated_value(), dump);
}

/// Send the data of a send (former `flushParameterData` for an `ISend`).
pub fn flush_send_data(writer: &mut OscWriter, address: &str, send: &Send, dump: bool) {
    writer.send_bool(&format!("{address}{TAG_ACTIVATED}"), send.is_enabled(), dump);
    writer.send_bool(&format!("{address}{TAG_EXISTS}"), send.does_exist(), dump);
    writer.send_str(&format!("{address}{TAG_NAME}"), send.name(), dump);
    writer.send_str(&format!("{address}volumeStr"), &send.displayed_value(), dump);
    writer.send_int(&format!("{address}{TAG_VOLUME}"), send.value(), dump);
    writer.send_int(&format!("{address}modulatedValue"), send.modulated_value(), dump);
}

/// Execute a parameter command (former `parseFXParamValue` of the modules).
pub fn parse_fx_param_value(parameter: &mut Parameter, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    match command.as_str() {
        "value" => parameter.set_value(to_integer(value)?),
        TAG_INDICATE => parameter.set_indication(is_trigger(value)),
        TAG_RESET => parameter.reset_value(),
        TAG_TOUCHED => parameter.touch_value(is_trigger(value)),
        _ => return Err(OscError::UnknownCommand(command)),
    }
    Ok(())
}

/// Send the pages of a parameter bank (the common part of the Java modules' flush methods):
/// `<address>page/<n>/exists|name|selected`, `<address>page/<n>/` (the name) and
/// `<address>page/selected/name`. Without a bank (e.g. no project parameters on the host)
/// `page_size` empty pages are sent.
pub fn flush_parameter_pages(writer: &mut OscWriter, address: &str, bank: Option<&crate::daw::parameter_bank::ParameterBank>, page_size: usize, dump: bool) {
    let page_bank = bank.map(|b| b.page_bank());
    let selected = page_bank.as_ref().map(|p| p.selected_item_index());
    let page_size = page_bank.as_ref().map(|p| p.page_size()).unwrap_or(page_size);
    for i in 0..page_size {
        let page_name = page_bank.as_ref().map(|p| p.item(i)).unwrap_or_default();
        let page_address = format!("{address}page/{}/", i + 1);
        writer.send_bool(&format!("{page_address}{TAG_EXISTS}"), !page_name.trim().is_empty(), dump);
        writer.send_str(&page_address, &page_name, dump);
        writer.send_str(&format!("{page_address}{TAG_NAME}"), &page_name, dump);
        writer.send_bool(&format!("{page_address}{TAG_SELECTED}"), selected == Some(i), dump);
    }
    let selected_name = page_bank.as_ref().map(|p| p.selected_item()).unwrap_or_default();
    writer.send_str(&format!("{address}page/selected/{TAG_NAME}"), &selected_name, dump);
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
    fn paths_and_numbers() {
        let mut path: Path = ["a".to_string(), "b".to_string()].into();
        assert_eq!(first(&path), Ok("a"));
        assert_eq!(sub_command(&mut path), Ok("a".into()));
        assert_eq!(sub_command(&mut path), Ok("b".into()));
        assert_eq!(sub_command(&mut path), Err(OscError::MissingCommand));
        assert!(first(&path).is_err());
        assert_eq!(parse_int("12"), Some(12));
        assert_eq!(parse_int("-3"), Some(-3));
        assert_eq!(parse_int("+"), None);
        assert_eq!(parse_int("1.5"), None);
        assert_eq!(index_of(1), Ok(0));
        assert!(index_of(0).is_err());
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
