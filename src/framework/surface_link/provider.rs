// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The provider side as a reusable helper (doc/SURFACE_LINK.md §API): a controller fills
//! [`LinkProvider::model_mut`] at its tick, calls [`LinkProvider::poll`] for the input of the
//! surfaces and [`LinkProvider::publish`] to send the changes. The helper sends a snapshot to
//! every new surface and on page switches, and only the changes otherwise.

use super::{provide, Input, PageModel, ProvideError, ProviderEvent, ProviderHandle};

/// A provider with its model.
pub struct LinkProvider {
    handle: Option<ProviderHandle>,
    model: PageModel,
    /// What the attached surfaces have.
    published: PageModel,
}

impl LinkProvider {
    /// Register as the provider of the link. Fails if the link has a provider already; the
    /// controller then works without (see [`LinkProvider::is_providing`]).
    pub fn new(link: &str, name: &str) -> Result<Self, ProvideError> {
        Ok(Self::with_handle(provide(link, name)?))
    }

    pub fn with_handle(handle: ProviderHandle) -> Self {
        LinkProvider { handle: Some(handle), model: PageModel::default(), published: PageModel::default() }
    }

    /// A provider which is not registered (the link had one already); everything is a no-op.
    pub fn inactive() -> Self {
        LinkProvider { handle: None, model: PageModel::default(), published: PageModel::default() }
    }

    pub fn is_providing(&self) -> bool {
        self.handle.is_some()
    }

    pub fn has_surfaces(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| h.surface_count() > 0)
    }

    pub fn handle(&self) -> Option<&ProviderHandle> {
        self.handle.as_ref()
    }

    pub fn model(&self) -> &PageModel {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut PageModel {
        &mut self.model
    }

    /// Welcome new surfaces (with a snapshot of the current model), notice the ones which left
    /// and return all events; [`LinkProvider::inputs`] if only the input is of interest.
    pub fn poll(&mut self) -> Vec<ProviderEvent> {
        let Some(handle) = &mut self.handle else {
            return Vec::new();
        };
        let model = &self.model;
        handle.poll(|| model.clone())
    }

    /// The input of the events.
    pub fn inputs(events: &[ProviderEvent]) -> Vec<Input> {
        events.iter().filter_map(|e| if let ProviderEvent::Input { input, .. } = e { Some(*input) } else { None }).collect()
    }

    /// Send what changed since the last call to all surfaces. Returns the number of messages.
    pub fn publish(&mut self) -> usize {
        let Some(handle) = &self.handle else {
            return 0;
        };
        if handle.surface_count() == 0 {
            // A new surface gets a snapshot anyway
            self.published.clone_from(&self.model);
            return 0;
        }
        let changes = self.model.diff(&self.published);
        for change in &changes {
            handle.broadcast(change);
        }
        if !changes.is_empty() {
            self.published.clone_from(&self.model);
        }
        changes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{attach, Capabilities, LinkState, ModelChange, SlotKind, SurfaceEvent};
    use super::*;

    #[test]
    fn publishes_changes() {
        let link = "test-provider-helper";
        let mut provider = LinkProvider::new(link, "P").unwrap();
        assert!(LinkProvider::new(link, "Q").is_err());
        provider.model_mut().pages = vec!["A".into(), "B".into()];
        provider.model_mut().slots[3].kind = SlotKind::Fader;
        // Without surfaces nothing is sent
        assert_eq!(provider.publish(), 0);

        let mut surface = attach(link, Capabilities::default());
        provider.poll();
        let mut mirror = PageModel::default();
        for event in surface.poll() {
            if let SurfaceEvent::Model(change) = event {
                mirror.apply(&change);
            }
        }
        assert_eq!(surface.state(), LinkState::Attached);
        assert_eq!(mirror, *provider.model());

        provider.model_mut().slots[3].value = 0.25;
        provider.model_mut().active_row = 1;
        assert_eq!(provider.publish(), 2);
        assert_eq!(provider.publish(), 0);
        provider.model_mut().page = 1;
        assert_eq!(provider.publish(), 1);
        let events = surface.poll();
        assert!(matches!(events.last(), Some(SurfaceEvent::Model(ModelChange::Snapshot(m))) if m.page == 1));

        surface.send(Input::PageSelect { page: 0 });
        assert_eq!(LinkProvider::inputs(&provider.poll()), [Input::PageSelect { page: 0 }]);

        let inactive = LinkProvider::inactive();
        assert!(!inactive.is_providing() && !inactive.has_surfaces());
    }
}
