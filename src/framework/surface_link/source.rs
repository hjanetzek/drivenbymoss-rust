// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The surface link as a page source (doc/HOST_PAGE_PLAN.md §3.2): the pages of a provider in
//! another controller instance (the Electra One), through the registry.

use super::{attach, Capabilities, Input, LinkState, SurfaceHandle};
use crate::daw::model::Model;
use crate::framework::page_surface::{PageSource, SourceEvent};

/// The surface side of a link.
pub struct LinkSource {
    handle: SurfaceHandle,
}

impl LinkSource {
    /// Attach to the link (whether the provider exists or not).
    pub fn new(link: &str, capabilities: Capabilities) -> Self {
        LinkSource { handle: attach(link, capabilities) }
    }

    pub fn handle(&self) -> &SurfaceHandle {
        &self.handle
    }
}

impl PageSource for LinkSource {
    fn state(&self) -> LinkState {
        self.handle.state()
    }

    fn name(&self) -> &str {
        self.handle.provider_name()
    }

    fn poll(&mut self, _model: &Model) -> Vec<SourceEvent> {
        self.handle.poll()
    }

    fn send(&mut self, input: Input) {
        self.handle.send(input);
    }
}
