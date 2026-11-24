// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Page surfaces (doc/SURFACE_LINK.md, doc/HOST_PAGE_PLAN.md; not part of Java): a controller with
//! a grid and encoders shows the pages of a **source** and sends its input back.
//!
//! - [`model`]: the page model (cells, control sets, sections, the selection), its changes and
//!   the input of a surface.
//! - [`source`]: the [`PageSource`](source::PageSource) trait. The sources are the surface link
//!   ([`crate::framework::surface_link::LinkSource`]: a provider in another controller instance,
//!   e.g. the Electra One) and the host ([`host::HostSource`]: pages which the host app reports,
//!   e.g. zynthian's control grid).
//! - [`client`]: the surface side of a controller (its mapping, its sources, the gestures).
//! - [`view`]: the generic view (the grid) and mode (the encoders).

pub mod client;
pub mod host;
pub mod model;
pub mod source;
pub mod view;

pub use model::{Capabilities, ColorDepth, ControlSet, Extra, Input, ModelChange, PageModel, Rgb, Section, Slot, SlotKind, TurnUnit, GRID_SIZE, MAX_SIZE, NUM_SLOTS};
pub use source::{LinkState, PageSource, SourceEvent};
