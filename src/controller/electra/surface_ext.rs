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

use super::definition::{sysex, CMD_CONTROLLER, CMD_INFO, SYSEX_HEADER, SUB_CMD_START_POS};
use crate::json::JsonValue;

pub const SYSEX_SET_ROW: [u8; 2] = [0x09, 0x50];
pub const SYSEX_GET_ROW: [u8; 2] = [0x02, 0x50];
pub const SYSEX_SET_SELECTION: [u8; 2] = [0x14, 0x51];
pub const SYSEX_GET_SELECTION: [u8; 2] = [0x02, 0x51];
/// Former `SYSEX_RUNTIME_SWITCH_CONTROL_SET` (not used by Java).
pub const SYSEX_SWITCH_CONTROL_SET: [u8; 2] = [0x09, 0x0B];

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        _ => None,
    }
}

/// What the device can do beyond the standard API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capability {
    pub rows: bool,
    pub selection: bool,
}

impl Capability {
    /// From the device information (`01 7F`).
    pub fn from_device_info(info: &JsonValue) -> Capability {
        let flag = |name: &str| info.get("surface").and_then(|s| s.get(name)).is_some_and(|v| matches!(v, JsonValue::Bool(true)));
        Capability { rows: flag("rows"), selection: flag("selection") }
    }

    pub fn any(self) -> bool {
        self.rows || self.selection
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
        assert_eq!(Capability::from_device_info(&info), Capability { rows: true, selection: true });
        let info = JsonValue::parse(r#"{"versionSeq":1,"surface":{"rows":true}}"#).unwrap();
        assert_eq!(Capability::from_device_info(&info), Capability { rows: true, selection: false });
        let info = JsonValue::parse(r#"{"versionSeq":1}"#).unwrap();
        assert!(!Capability::from_device_info(&info).any());
    }

    #[test]
    fn messages() {
        let full = Capability { rows: true, selection: true };
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
    }
}
