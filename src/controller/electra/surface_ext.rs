// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Electra side of the surface link: the active row and the selected slot on the device
//! (not part of Java). The contract is the one of the electra-one emulator,
//! `docs/surface-link.md` §6b; doc/SURFACE_LINK.md §6 describes it from this side. Everything
//! the device must know beyond the standard SysEx API is in this file, so that a change of the
//! contract is a change here only.
//!
//! - Capability: the device information (`01 7F`) has `"surface": {"rows": true, "selection":
//!   true}`. A real Electra does not have it: the provider then only switches the control set
//!   (`09 0B [set]`, the upper row of the set) and shows no selection.
//! - Row r = 1..6 (control set ceil(r/2), odd = upper half): set `09 50 [r]`, read `02 50` →
//!   `01 50 [r]`.
//! - Selection (a control ID of the current page, 0 = none): set `14 51 [lsb] [msb]`, read
//!   `02 51` → `01 51 [lsb] [msb]`.
//! - Events (with the control set events subscribed, flag 0x02 of `14 79`): `7E 60 [r] [origin]`
//!   and `7E 61 [lsb] [msb] [origin]`; origin 0 = the device (panel, keys, touch), 1 = SysEx, 2
//!   = Lua. A page switch resets the row to the upper row of the page's default control set and
//!   the selection to none, and reports both.
//! - A NACK for `09 50` means the device does not have the rows after all (fall back to `09
//!   0B`); for `14 51` that the selection is unchanged.
//!
//! The generic provider (`docs/surface-link.md` §6c, `firmware-mk1/SYSEX-API.md` "The generic
//! provider"; the Mk2 emulator only), for any preset other than DrivenByMoss's:
//! - Capability: `"surface": {…, "view": true, "input": true}`.
//! - The page view: `02 52` → `01 52 <JSON>`; `7E 62 [page id]` when it changed (with the
//!   control set events).
//! - Turn a knob of the current control set: `14 53 [pot 1-12] [lsb] [msb] [flags]`, a 14 bit
//!   two's complement delta in panel steps (1/64 turn), flags bit 0 fine (1/8 step), bit 1 a
//!   touch around the turn. Touch: `14 54 [pot] [0|1]`.
//! - Set a value: `14 55 {"controlId","valueId","value"|"step"|"default"}`; the active value
//!   (handle): `14 56 {"controlId","valueId"|"next"|"prev"}`.
//! - Value changes: `7E 63 {"controlId","valueId","value","midiValue","origin","text"}`, with
//!   the second flags byte of `14 79` (bit 0).

use super::definition::{sysex, CMD_CONTROLLER, CMD_INFO, SYSEX_HEADER, SYSEX_RUNTIME_SUBSCRIBE_EVENTS, SUB_CMD_START_POS};
use crate::json::{JsonValue, JsonWriter};

pub const SYSEX_SET_ROW: [u8; 2] = [0x09, 0x50];
pub const SYSEX_GET_ROW: [u8; 2] = [0x02, 0x50];
pub const SYSEX_SET_SELECTION: [u8; 2] = [0x14, 0x51];
pub const SYSEX_GET_SELECTION: [u8; 2] = [0x02, 0x51];
/// Former `SYSEX_RUNTIME_SWITCH_CONTROL_SET` (not used by Java).
pub const SYSEX_SWITCH_CONTROL_SET: [u8; 2] = [0x09, 0x0B];

pub const SYSEX_GET_PAGE_VIEW: [u8; 2] = [0x02, 0x52];
pub const SYSEX_TURN_KNOB: [u8; 2] = [0x14, 0x53];
pub const SYSEX_TOUCH_KNOB: [u8; 2] = [0x14, 0x54];
pub const SYSEX_SET_VALUE: [u8; 2] = [0x14, 0x55];
pub const SYSEX_ACTIVE_VALUE: [u8; 2] = [0x14, 0x56];

pub const OBJECT_PAGE_VIEW: u8 = 0x52;
pub const EVENT_VALUE_CHANGED: u8 = 0x63;
pub const EVENT_PAGE_VIEW_CHANGED: u8 = 0x62;
/// The flag of the value changes (`7E 63`) in the second flags byte of `14 79`.
pub const SUBSCRIBE_VALUE_CHANGES: u8 = 0x01;
/// `14 53` flags: fine steps, a touch around the turn.
pub const TURN_FINE: u8 = 0x01;
pub const TURN_WITH_TOUCH: u8 = 0x02;

pub const OBJECT_ROW: u8 = 0x50;
pub const OBJECT_SELECTION: u8 = 0x51;
pub const EVENT_NACK: u8 = 0x00;
pub const EVENT_ACK: u8 = 0x01;
pub const EVENT_CONTROL_SET_SWITCH: u8 = 0x07;
pub const EVENT_ROW_CHANGED: u8 = 0x60;
pub const EVENT_SELECTION_CHANGED: u8 = 0x61;
/// The flag of the control set events in `14 79`.
pub const SUBSCRIBE_CONTROL_SET: u8 = 0x02;

/// Where a change on the device came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The device itself: panel, keys, touch.
    Device,
    /// SysEx (DrivenByMoss itself, it is the only one which sends SysEx to CTRL).
    Sysex,
    Lua,
}

impl Origin {
    fn from_byte(byte: u8) -> Origin {
        match byte {
            1 => Origin::Sysex,
            2 => Origin::Lua,
            _ => Origin::Device,
        }
    }
}

/// A message of the device which concerns the surface link.
#[derive(Debug, Clone, PartialEq)]
pub enum Report {
    Ack,
    Nack,
    /// `7E 07`: the control set (0..2).
    ControlSet(usize),
    /// `7E 60` (row 0..5).
    Row { row: usize, origin: Origin },
    /// `7E 61` (control ID, 0 = none).
    Selection { control_id: i32, origin: Origin },
    /// `01 50`: the answer to the read of the row.
    RowState(usize),
    /// `01 51`: the answer to the read of the selection.
    SelectionState(i32),
    /// `01 52`: the page view (§6c), the JSON.
    PageView(JsonValue),
    /// `7E 62 [page id]`: the page view changed.
    PageViewChanged,
    /// `7E 63`: a value changed.
    ValueChanged(ValueChange),
}

/// A value change of the device (`7E 63`).
#[derive(Debug, Clone, PartialEq)]
pub struct ValueChange {
    pub control_id: i32,
    pub value_id: String,
    /// The value as the control shows it (a list: the index of the item).
    pub value: f64,
    pub text: Option<String>,
}

impl ValueChange {
    fn from_json(json: &JsonValue) -> Option<ValueChange> {
        let JsonValue::Number(value) = json.get("value")? else {
            return None;
        };
        Some(ValueChange {
            control_id: json.get("controlId")?.as_int() as i32,
            value_id: json.get("valueId").map_or_else(|| "value".to_string(), JsonValue::as_text),
            value: *value,
            text: json.get("text").filter(|t| matches!(t, JsonValue::String(_))).map(JsonValue::as_text),
        })
    }
}

/// The JSON after the command of a message.
fn json_content(data: &[u8]) -> Option<JsonValue> {
    let content = data.get(SUB_CMD_START_POS + 1..data.len().checked_sub(1)?)?;
    let text: String = content.iter().map(|&b| b as char).collect();
    JsonValue::parse(&text).ok()
}

/// Parse a (complete) SysEx message of the CTRL port.
pub fn parse(data: &[u8]) -> Option<Report> {
    if data.len() <= SUB_CMD_START_POS || data[..SYSEX_HEADER.len()] != SYSEX_HEADER {
        return None;
    }
    let byte = |offset: usize| data.get(SUB_CMD_START_POS + offset).copied().filter(|b| *b < 0x80);
    let id = |offset: usize| Some(byte(offset)? as i32 | ((byte(offset + 1)? as i32) << 7));
    let row = |offset: usize| byte(offset).filter(|r| (1..=6).contains(r)).map(|r| r as usize - 1);
    match (data[SUB_CMD_START_POS - 1], data[SUB_CMD_START_POS]) {
        (CMD_CONTROLLER, EVENT_ACK) => Some(Report::Ack),
        (CMD_CONTROLLER, EVENT_NACK) => Some(Report::Nack),
        (CMD_CONTROLLER, EVENT_CONTROL_SET_SWITCH) => Some(Report::ControlSet(byte(1)? as usize)),
        (CMD_CONTROLLER, EVENT_ROW_CHANGED) => Some(Report::Row { row: row(1)?, origin: Origin::from_byte(byte(2).unwrap_or(0)) }),
        (CMD_CONTROLLER, EVENT_SELECTION_CHANGED) => Some(Report::Selection { control_id: id(1)?, origin: Origin::from_byte(byte(3).unwrap_or(0)) }),
        (CMD_INFO, OBJECT_ROW) => Some(Report::RowState(row(1)?)),
        (CMD_INFO, OBJECT_SELECTION) => Some(Report::SelectionState(id(1)?)),
        (CMD_INFO, OBJECT_PAGE_VIEW) => Some(Report::PageView(json_content(data)?)),
        (CMD_CONTROLLER, EVENT_PAGE_VIEW_CHANGED) => Some(Report::PageViewChanged),
        (CMD_CONTROLLER, EVENT_VALUE_CHANGED) => Some(Report::ValueChanged(ValueChange::from_json(&json_content(data)?)?)),
        _ => None,
    }
}

/// What the device can do beyond the standard API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capability {
    pub rows: bool,
    pub selection: bool,
    /// The page view (`02 52`, `7E 62`).
    pub view: bool,
    /// The knob and value input (`14 53`-`14 56`) and the value changes (`7E 63`).
    pub input: bool,
}

impl Capability {
    /// From the device information (`01 7F`).
    pub fn from_device_info(info: &JsonValue) -> Capability {
        let flag = |name: &str| info.get("surface").and_then(|s| s.get(name)).is_some_and(|v| matches!(v, JsonValue::Bool(true)));
        Capability { rows: flag("rows"), selection: flag("selection"), view: flag("view"), input: flag("input") }
    }

    pub fn any(self) -> bool {
        self.rows || self.selection
    }

    /// True if the device can be the generic provider for any preset (§6c).
    pub fn generic(self) -> bool {
        self.view && self.input && self.rows && self.selection
    }
}

/// The messages which make `row` (0..5) the active row on the device; `old_row` is the one the
/// device has.
pub fn set_row_messages(capability: Capability, old_row: usize, row: usize) -> Vec<Vec<u8>> {
    if capability.rows {
        vec![sysex(&SYSEX_SET_ROW, &[row as u8 + 1])]
    } else if old_row / 2 != row / 2 {
        vec![sysex(&SYSEX_SWITCH_CONTROL_SET, &[(row / 2) as u8])]
    } else {
        Vec::new()
    }
}

/// The message which highlights a control (0 = none), `None` without the capability.
pub fn set_selection_message(capability: Capability, control_id: i32) -> Option<Vec<u8>> {
    capability.selection.then(|| sysex(&SYSEX_SET_SELECTION, &[(control_id & 0x7F) as u8, ((control_id >> 7) & 0x7F) as u8]))
}

/// The reads of the row and the selection (the state after going online). The parameter map
/// dump `02 41` (the initial values) will follow here once its format is fixed.
pub fn initial_state_requests(capability: Capability) -> Vec<Vec<u8>> {
    let mut requests = Vec::new();
    if capability.rows {
        requests.push(sysex(&SYSEX_GET_ROW, &[]));
    }
    if capability.selection {
        requests.push(sysex(&SYSEX_GET_SELECTION, &[]));
    }
    requests
}

/// The subscription of the events: `flags` and, for the generic provider, the second flags
/// byte with the value changes (`00` turns them off again).
pub fn subscribe_message(flags: u8, value_changes: Option<bool>) -> Vec<u8> {
    match value_changes {
        None => sysex(&SYSEX_RUNTIME_SUBSCRIBE_EVENTS, &[flags]),
        Some(on) => sysex(&SYSEX_RUNTIME_SUBSCRIBE_EVENTS, &[flags, if on { SUBSCRIBE_VALUE_CHANGES } else { 0 }]),
    }
}

pub fn page_view_request() -> Vec<u8> {
    sysex(&SYSEX_GET_PAGE_VIEW, &[])
}

/// Turn the knob `pot` (1..12) of the current control set by `delta` panel steps (or eighths of
/// them with `fine`); the delta is clamped to 14 bits.
pub fn turn_knob_message(pot: u8, delta: i32, fine: bool) -> Vec<u8> {
    let delta = delta.clamp(-8192, 8191) & 0x3FFF;
    sysex(&SYSEX_TURN_KNOB, &[pot, (delta & 0x7F) as u8, ((delta >> 7) & 0x7F) as u8, if fine { TURN_FINE } else { 0 }])
}

/// Touch (or let go) the knob `pot` (1..12).
pub fn touch_knob_message(pot: u8, touched: bool) -> Vec<u8> {
    sysex(&SYSEX_TOUCH_KNOB, &[pot, touched as u8])
}

/// What `14 55` sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetValue {
    /// A display value; for a list the value of an overlay item.
    Value(i64),
    /// Positions (a list: items).
    Step(i64),
    Default,
}

/// Set a value of a control (`14 55`); without `value_id` the control's active value.
pub fn set_value_message(control_id: i32, value_id: Option<&str>, set: SetValue) -> Vec<u8> {
    let mut json = JsonWriter::new();
    json.begin_object().key("controlId").number(control_id as i64);
    if let Some(value_id) = value_id {
        json.key("valueId").string(value_id);
    }
    match set {
        SetValue::Value(value) => json.key("value").number(value),
        SetValue::Step(step) => json.key("step").number(step),
        SetValue::Default => json.key("default").boolean(true),
    };
    json.end_object();
    sysex(&SYSEX_SET_VALUE, json.finish().as_bytes())
}

/// The next (+1) or previous (-1) active value (handle) of a control (`14 56`).
pub fn step_active_value_message(control_id: i32, step: i32) -> Vec<u8> {
    let mut json = JsonWriter::new();
    json.begin_object().key("controlId").number(control_id as i64).key(if step > 0 { "next" } else { "prev" }).boolean(true).end_object();
    sysex(&SYSEX_ACTIVE_VALUE, json.finish().as_bytes())
}

/// The pending own changes, to undo them on a NACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pending {
    /// The row before the own change.
    pub row: Option<usize>,
    /// The selection before the own change.
    pub selection: Option<Option<usize>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(command: &[u8], content: &[u8]) -> Vec<u8> {
        sysex(command, content)
    }

    #[test]
    fn capability() {
        let info = JsonValue::parse(r#"{"versionSeq":1,"surface":{"rows":true,"selection":true}}"#).unwrap();
        assert_eq!(Capability::from_device_info(&info), Capability { rows: true, selection: true, view: false, input: false });
        assert!(!Capability::from_device_info(&info).generic());
        let info = JsonValue::parse(r#"{"versionSeq":1,"surface":{"rows":true}}"#).unwrap();
        assert_eq!(Capability::from_device_info(&info), Capability { rows: true, selection: false, view: false, input: false });
        let info = JsonValue::parse(r#"{"surface":{"rows":true,"selection":true,"view":true,"input":true}}"#).unwrap();
        assert!(Capability::from_device_info(&info).generic());
        let info = JsonValue::parse(r#"{"versionSeq":1}"#).unwrap();
        assert!(!Capability::from_device_info(&info).any());
    }

    #[test]
    fn messages() {
        let full = Capability { rows: true, selection: true, ..Capability::default() };
        assert_eq!(set_row_messages(full, 0, 3), [vec![0xF0, 0x00, 0x21, 0x45, 0x09, 0x50, 4, 0xF7]]);
        // Without the capability: the control set, only if it changes
        assert_eq!(set_row_messages(Capability::default(), 0, 3), [vec![0xF0, 0x00, 0x21, 0x45, 0x09, 0x0B, 1, 0xF7]]);
        assert!(set_row_messages(Capability::default(), 2, 3).is_empty());
        assert_eq!(set_selection_message(full, 200), Some(vec![0xF0, 0x00, 0x21, 0x45, 0x14, 0x51, 0x48, 0x01, 0xF7]));
        assert_eq!(set_selection_message(Capability::default(), 200), None);
        assert_eq!(initial_state_requests(full), [message(&[0x02, 0x50], &[]), message(&[0x02, 0x51], &[])]);
        assert!(initial_state_requests(Capability::default()).is_empty());
    }

    #[test]
    fn reports() {
        assert_eq!(parse(&message(&[0x7E, 0x60], &[4, 0])), Some(Report::Row { row: 3, origin: Origin::Device }));
        assert_eq!(parse(&message(&[0x7E, 0x60], &[1, 1])), Some(Report::Row { row: 0, origin: Origin::Sysex }));
        assert_eq!(parse(&message(&[0x7E, 0x60], &[7, 0])), None);
        assert_eq!(parse(&message(&[0x7E, 0x61], &[0x48, 0x01, 2])), Some(Report::Selection { control_id: 200, origin: Origin::Lua }));
        assert_eq!(parse(&message(&[0x7E, 0x07], &[2])), Some(Report::ControlSet(2)));
        assert_eq!(parse(&message(&[0x7E, 0x00], &[])), Some(Report::Nack));
        assert_eq!(parse(&message(&[0x7E, 0x01], &[])), Some(Report::Ack));
        assert_eq!(parse(&message(&[0x01, 0x50], &[6])), Some(Report::RowState(5)));
        assert_eq!(parse(&message(&[0x01, 0x51], &[0, 0])), Some(Report::SelectionState(0)));
        assert_eq!(parse(&message(&[0x7E, 0x06], &[1])), None);
        assert_eq!(parse(&message(&[0x7E, 0x62], &[1])), Some(Report::PageViewChanged));
        let change = br#"{"controlId":3,"valueId":"value","value":65,"midiValue":65,"origin":"internal","text":"Cut 65"}"#;
        assert_eq!(
            parse(&message(&[0x7E, 0x63], change)),
            Some(Report::ValueChanged(ValueChange { control_id: 3, value_id: "value".into(), value: 65.0, text: Some("Cut 65".into()) }))
        );
        assert!(matches!(parse(&message(&[0x01, 0x52], br#"{"version":1}"#)), Some(Report::PageView(_))));
        assert_eq!(parse(&message(&[0x01, 0x52], b"{bad")), None);
    }

    #[test]
    fn generic_messages() {
        let m = |c: &[u8], content: &[u8]| message(c, content);
        assert_eq!(turn_knob_message(3, -1, false), m(&[0x14, 0x53], &[3, 0x7F, 0x7F, 0]));
        assert_eq!(turn_knob_message(1, 8, true), m(&[0x14, 0x53], &[1, 8, 0, 1]));
        assert_eq!(turn_knob_message(12, -8, false), m(&[0x14, 0x53], &[12, 0x78, 0x7F, 0]));
        assert_eq!(turn_knob_message(1, 200, false), m(&[0x14, 0x53], &[1, 0x48, 0x01, 0]));
        assert_eq!(touch_knob_message(9, true), m(&[0x14, 0x54], &[9, 1]));
        assert_eq!(set_value_message(3, Some("value"), SetValue::Value(100)), m(&[0x14, 0x55], br#"{"controlId":3,"valueId":"value","value":100}"#));
        assert_eq!(set_value_message(3, Some("a"), SetValue::Step(-7)), m(&[0x14, 0x55], br#"{"controlId":3,"valueId":"a","step":-7}"#));
        assert_eq!(set_value_message(1, None, SetValue::Default), m(&[0x14, 0x55], br#"{"controlId":1,"default":true}"#));
        assert_eq!(step_active_value_message(1, 1), m(&[0x14, 0x56], br#"{"controlId":1,"next":true}"#));
        assert_eq!(step_active_value_message(1, -1), m(&[0x14, 0x56], br#"{"controlId":1,"prev":true}"#));
        assert_eq!(subscribe_message(0x3B, Some(true)), m(&[0x14, 0x79], &[0x3B, 0x01]));
        assert_eq!(subscribe_message(0x3B, Some(false)), m(&[0x14, 0x79], &[0x3B, 0x00]));
        assert_eq!(subscribe_message(0x39, None), m(&[0x14, 0x79], &[0x39]));
        assert_eq!(page_view_request(), m(&[0x02, 0x52], &[]));
    }
}
