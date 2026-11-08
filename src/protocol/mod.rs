// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The typed protocol between the core and its hosts (doc/TYPED_PROTOCOL_PLAN.md).
//!
//! Step 0a: the commands of the core ([`Cmd`]), generated from the table in [`cmd`] by the
//! `commands!` macro. The core sends `Cmd` values; the backends turn them into the legacy text
//! form ([`Cmd::legacy`]), so the text on the wire is unchanged.

#[macro_use]
mod macros;
pub mod cmd;
pub mod legacy;
pub mod types;

pub use cmd::{Cmd, CmdId, CmdInfo, ProcSpec, COMMANDS};
pub use legacy::{Arg, Enable, Legacy};
pub use types::*;
