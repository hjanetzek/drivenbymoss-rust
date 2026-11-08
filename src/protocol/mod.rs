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
pub mod cmd;
pub mod legacy;
pub mod msg;
pub mod text;

pub use cmd::{Cmd, CmdId, CmdInfo, ProcSpec, COMMANDS};
pub use legacy::{Arg, Enable, Legacy};
pub use msg::{BrowserItem, Ch, Dev, HostParam, Msg, MsgId, MsgInfo, PadRef, PageOwner, ParamOwner, MESSAGES};
pub use text::Miss;
pub use types::*;
