// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The source of the pages of a surface (doc/HOST_PAGE_PLAN.md §3.2).

use super::{Input, ModelChange};
use crate::daw::model::Model;

/// The state of a source, as the surface sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// No source (yet): no provider is registered (or it is in another copy of the library), the
    /// host does not provide pages.
    Missing,
    /// The source is there; the model is valid.
    Attached,
    /// The source left (unloaded or restarted); it may come back.
    Detached,
    /// The source has another major protocol version.
    Incompatible,
}

/// What a surface learns from its source at its tick.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceEvent {
    /// The state changed.
    State(LinkState),
    Model(ModelChange),
}

/// A source of pages: it gives the surface the changes of its model and takes its input.
pub trait PageSource {
    fn state(&self) -> LinkState;

    /// The name of the source (for the notifications), empty if unknown.
    fn name(&self) -> &str;

    /// The tick of the surface: the state changes and the model changes since the last call; a
    /// source may also send the queued input here.
    fn poll(&mut self, model: &Model) -> Vec<SourceEvent>;

    /// The input of the surface (dropped if the source is not attached).
    fn send(&mut self, input: Input);
}
