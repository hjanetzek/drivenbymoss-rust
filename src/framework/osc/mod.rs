// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Open Sound Control (former `framework.osc` and the OSC part of the host implementation):
//! the OSC 1.0 encoding ([`message`]), the writer which sends only changed values ([`writer`])
//! and the UDP server and client ([`udp`]). Without dependencies, `std::net` only.

pub mod java;
pub mod message;
pub mod udp;
pub mod writer;

pub use message::{OscArg, OscDecodeError, OscMessage, OscPacket};
pub use udp::{OscClient, OscServer};
pub use writer::OscWriter;
