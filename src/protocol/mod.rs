// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The typed protocol between the core and its hosts (doc/TYPED_PROTOCOL_PLAN.md).
//!
//! - Step 0a: the commands of the core ([`Cmd`]), generated from the table in [`cmd`] by the
//!   `commands!` macro. The core sends `Cmd` values; the backends turn them into the legacy text
//!   form ([`Cmd::legacy`]), so the text on the wire is unchanged.
//! - Step 0b: the reports of the host ([`Msg`]), generated from the table in [`msg`] by the
//!   `messages!` macro. The parser turns a report line into a `Msg` ([`Msg::from_text`]) and
//!   applies it (`Parser::apply`).

#[macro_use]
mod macros;
#[macro_use]
pub mod types;
pub mod bin;
pub mod cmd;
pub mod gen;
pub mod legacy;
pub mod msg;
pub mod text;

pub use cmd::{Cmd, CmdId, CmdInfo, ProcSpec, COMMANDS};
pub use legacy::{Arg, Enable, Legacy};
pub use msg::{BrowserItem, Ch, Dev, HostParam, Msg, MsgId, MsgInfo, PadRef, PageOwner, ParamOwner, MESSAGES};
pub use text::Miss;
pub use types::*;

/// The hash of the tables (FNV-1a over a normalized text of every message and command: ID,
/// name, template, index slots, value type), `DBM_PROTOCOL_HASH` of the generated bindings.
pub fn protocol_hash() -> u64 {
    let mut text = String::new();
    for info in MESSAGES {
        text.push_str(&format!("{:04x} {} {} {:?} {}\n", info.id as u16, info.name, info.template, info.id.index_slots(), info.id.value_type()));
    }
    for info in COMMANDS {
        text.push_str(&format!("{:04x} {} {:?} {} {:?} {}\n", info.id as u16, info.name, info.processor, info.template, info.id.index_slots(), info.id.value_type()));
    }
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3))
}
