// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The surface link (doc/SURFACE_LINK.md): a generic link between two controller instances.
//! A **provider** publishes a 6x6 page model ([`model::PageModel`]) and executes input;
//! **surfaces** (any controller with a grid and encoders) render it and send input back.
//!
//! Mechanics (§5 of the design):
//! - A process-wide registry: [`provide`] and [`attach`] by a link name. Either side may come
//!   first. There is one provider per name and any number of surfaces.
//! - Two queues per surface, one per direction, each with its own lock. A side only pushes to
//!   the queue of the other direction and drains its own on its own tick; it never calls into
//!   the other instance. The registry lock is only held to add and remove the two sides (and
//!   to tell the surfaces that the provider left); the lock order is always registry, then a
//!   queue, so there is no deadlock between the threads of two hosts.
//! - Explicit states on the surface side ([`LinkState`]), the protocol version in the attach.
//!
//! The registry is a static of this library. It is shared by all controller instances which
//! run on the same copy of the library in one process: the instances of an [`App`], all
//! Bitwig extensions of one JVM (they load the same file, see `NativeLibraryLoader`), the
//! controllers of one Python host or of Zynthian. Two different copies of the library (e.g.
//! `DBM_DEV_LIBRARY` next to a bundled one) have two registries and cannot find each other: the
//! surface stays in [`LinkState::Missing`] ("no provider").
//!
//! [`App`]: crate::app::App

pub mod provider;
pub mod source;

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

pub use crate::framework::page_surface::model::{self, Capabilities, ColorDepth, ControlSet, Extra, Input, ModelChange, PageModel, Rgb, Section, Slot, SlotKind, TurnUnit, GRID_SIZE, NUM_SLOTS};
pub use crate::framework::page_surface::{client, view, LinkState};
pub use source::LinkSource;

/// The version of the link protocol. Surfaces and providers with the same major version work
/// together; a different minor version only adds messages or fields which the other side
/// ignores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16) -> Self {
        Version { major, minor }
    }

    /// True if the two sides can talk to each other.
    pub fn is_compatible(self, other: Version) -> bool {
        self.major == other.major
    }
}

/// The version of this build.
/// 1.1: `Input::ValueNext` / `ValuePrev`.
/// 1.2: the page model's size, control sets, sections, turn units, `Shown`
/// (doc/HOST_PAGE_PLAN.md §3.3).
/// 1.3: `Input::SlotAssign` (the host's remote control pages).
/// 1.4: `Input::PagePrevLast`.
pub const PROTOCOL_VERSION: Version = Version::new(1, 4);

/// The name of the link which the controllers of this crate use.
pub const DEFAULT_LINK: &str = "pages";

/// A message to a surface.
#[derive(Debug, Clone, PartialEq)]
enum Down {
    /// The provider accepted the surface (its name and version).
    Welcome { provider: u64, name: String, version: Version },
    /// The provider refused the surface (another major version).
    Rejected { provider: u64, version: Version },
    Model { provider: u64, change: ModelChange },
    /// The provider left.
    Gone { provider: u64 },
}

/// The two queues of a surface.
#[derive(Debug)]
struct Conn {
    id: u64,
    capabilities: Capabilities,
    version: Version,
    /// Surface → provider.
    up: Mutex<VecDeque<Input>>,
    /// Provider → surface.
    down: Mutex<VecDeque<Down>>,
}

#[derive(Debug)]
struct ProviderEntry {
    id: u64,
}

#[derive(Debug, Default)]
struct Hub {
    provider: Option<ProviderEntry>,
    surfaces: Vec<Arc<Conn>>,
}

static REGISTRY: Mutex<BTreeMap<String, Hub>> = Mutex::new(BTreeMap::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Lock a mutex; a panic of another thread while it held the lock does not make the link
/// unusable (the data are plain values).
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------------------------
// Provider

/// Why [`provide`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvideError {
    /// The link has a provider already.
    AlreadyProvided,
}

/// What a provider learns at its tick.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// A surface attached (and got the welcome).
    Attached { surface: u64, capabilities: Capabilities },
    /// A surface with another major version was refused.
    Refused { surface: u64, version: Version },
    Detached { surface: u64 },
    Input { surface: u64, input: Input },
}

/// The provider side of a link; dropping it detaches all surfaces.
#[derive(Debug)]
pub struct ProviderHandle {
    link: String,
    id: u64,
    name: String,
    version: Version,
    /// The surfaces which got the welcome.
    known: Vec<Arc<Conn>>,
    /// The surfaces which were refused (another major version).
    refused: Vec<u64>,
}

/// Register as the provider of a link. `name` is shown by the surfaces.
pub fn provide(link: &str, name: &str) -> Result<ProviderHandle, ProvideError> {
    provide_with_version(link, name, PROTOCOL_VERSION)
}

/// [`provide`] with another protocol version (tests).
pub fn provide_with_version(link: &str, name: &str, version: Version) -> Result<ProviderHandle, ProvideError> {
    let mut registry = lock(&REGISTRY);
    let hub = registry.entry(link.to_string()).or_default();
    if hub.provider.is_some() {
        return Err(ProvideError::AlreadyProvided);
    }
    let id = next_id();
    hub.provider = Some(ProviderEntry { id });
    Ok(ProviderHandle { link: link.to_string(), id, name: name.to_string(), version, known: Vec::new(), refused: Vec::new() })
}

impl ProviderHandle {
    pub fn link(&self) -> &str {
        &self.link
    }

    /// The number of attached surfaces (as of the last [`ProviderHandle::poll`]).
    pub fn surface_count(&self) -> usize {
        self.known.len()
    }

    /// The capabilities of the attached surfaces.
    pub fn surfaces(&self) -> impl Iterator<Item = (u64, Capabilities)> + '_ {
        self.known.iter().map(|c| (c.id, c.capabilities))
    }

    /// The tick of the provider: welcome the new surfaces, notice the ones which left, drain
    /// the input. `snapshot` is called for every new surface; its model goes to that surface.
    pub fn poll(&mut self, mut snapshot: impl FnMut() -> PageModel) -> Vec<ProviderEvent> {
        let current: Vec<Arc<Conn>> = {
            let registry = lock(&REGISTRY);
            registry.get(&self.link).map(|hub| hub.surfaces.clone()).unwrap_or_default()
        };
        let mut events = Vec::new();
        self.known.retain(|conn| {
            let present = current.iter().any(|c| c.id == conn.id);
            if !present {
                events.push(ProviderEvent::Detached { surface: conn.id });
            }
            present
        });
        self.refused.retain(|id| current.iter().any(|c| c.id == *id));
        for conn in current {
            if self.known.iter().any(|c| c.id == conn.id) || self.refused.contains(&conn.id) {
                continue;
            }
            let mut down = lock(&conn.down);
            if !self.version.is_compatible(conn.version) {
                // Refused once, it gets nothing else
                down.push_back(Down::Rejected { provider: self.id, version: self.version });
                events.push(ProviderEvent::Refused { surface: conn.id, version: conn.version });
                self.refused.push(conn.id);
                continue;
            }
            down.push_back(Down::Welcome { provider: self.id, name: self.name.clone(), version: self.version });
            down.push_back(Down::Model { provider: self.id, change: ModelChange::Snapshot(Box::new(snapshot())) });
            drop(down);
            events.push(ProviderEvent::Attached { surface: conn.id, capabilities: conn.capabilities });
            self.known.push(conn);
        }
        for conn in &self.known {
            let inputs: Vec<Input> = lock(&conn.up).drain(..).collect();
            events.extend(inputs.into_iter().map(|input| ProviderEvent::Input { surface: conn.id, input }));
        }
        events
    }

    /// Send a change to all attached surfaces.
    pub fn broadcast(&self, change: &ModelChange) {
        for conn in &self.known {
            lock(&conn.down).push_back(Down::Model { provider: self.id, change: change.clone() });
        }
    }

    /// Send a change to one surface.
    pub fn send_to(&self, surface: u64, change: ModelChange) {
        if let Some(conn) = self.known.iter().find(|c| c.id == surface) {
            lock(&conn.down).push_back(Down::Model { provider: self.id, change });
        }
    }
}

impl Drop for ProviderHandle {
    fn drop(&mut self) {
        let mut registry = lock(&REGISTRY);
        let Some(hub) = registry.get_mut(&self.link) else {
            return;
        };
        if hub.provider.as_ref().is_some_and(|p| p.id == self.id) {
            hub.provider = None;
        }
        // Under the registry lock, so that a new provider cannot welcome a surface before it
        // got this
        for conn in &hub.surfaces {
            lock(&conn.down).push_back(Down::Gone { provider: self.id });
        }
        if hub.provider.is_none() && hub.surfaces.is_empty() {
            registry.remove(&self.link);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Surface

/// What a surface learns at its tick: the state changes (with the provider's name when
/// attached) and the model changes.
pub type SurfaceEvent = crate::framework::page_surface::SourceEvent;

/// The surface side of a link; dropping it detaches the surface.
#[derive(Debug)]
pub struct SurfaceHandle {
    link: String,
    conn: Arc<Conn>,
    state: LinkState,
    /// The provider which welcomed the surface.
    provider: Option<u64>,
    provider_name: String,
    provider_version: Option<Version>,
}

/// Attach a surface to a link; works whether the provider exists or not.
pub fn attach(link: &str, capabilities: Capabilities) -> SurfaceHandle {
    attach_with_version(link, capabilities, PROTOCOL_VERSION)
}

/// [`attach`] with another protocol version (tests).
pub fn attach_with_version(link: &str, capabilities: Capabilities, version: Version) -> SurfaceHandle {
    let conn = Arc::new(Conn { id: next_id(), capabilities, version, up: Mutex::new(VecDeque::new()), down: Mutex::new(VecDeque::new()) });
    lock(&REGISTRY).entry(link.to_string()).or_default().surfaces.push(conn.clone());
    SurfaceHandle { link: link.to_string(), conn, state: LinkState::Missing, provider: None, provider_name: String::new(), provider_version: None }
}

impl SurfaceHandle {
    pub fn id(&self) -> u64 {
        self.conn.id
    }

    pub fn state(&self) -> LinkState {
        self.state
    }

    pub fn is_attached(&self) -> bool {
        self.state == LinkState::Attached
    }

    /// The name of the provider (empty if none attached yet).
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    pub fn provider_version(&self) -> Option<Version> {
        self.provider_version
    }

    /// Send input to the provider; dropped if not attached.
    pub fn send(&self, input: Input) {
        if self.state == LinkState::Attached {
            lock(&self.conn.up).push_back(input);
        }
    }

    /// The tick of the surface: drain the messages of the provider.
    pub fn poll(&mut self) -> Vec<SurfaceEvent> {
        let messages: Vec<Down> = lock(&self.conn.down).drain(..).collect();
        let mut events = Vec::new();
        for message in messages {
            match message {
                Down::Welcome { provider, name, version } => {
                    self.provider = Some(provider);
                    self.provider_name = name;
                    self.provider_version = Some(version);
                    self.set_state(LinkState::Attached, &mut events);
                }
                Down::Rejected { provider, version } => {
                    self.provider = Some(provider);
                    self.provider_version = Some(version);
                    self.set_state(LinkState::Incompatible, &mut events);
                }
                Down::Model { provider, change } => {
                    if self.state == LinkState::Attached && self.provider == Some(provider) {
                        events.push(SurfaceEvent::Model(change));
                    }
                }
                Down::Gone { provider } => {
                    if self.provider == Some(provider) {
                        self.provider = None;
                        // The input for the old provider is of no use for a new one
                        lock(&self.conn.up).clear();
                        self.set_state(LinkState::Detached, &mut events);
                    }
                }
            }
        }
        events
    }

    fn set_state(&mut self, state: LinkState, events: &mut Vec<SurfaceEvent>) {
        if self.state != state {
            self.state = state;
            events.push(SurfaceEvent::State(state));
        }
    }
}

impl Drop for SurfaceHandle {
    fn drop(&mut self) {
        let mut registry = lock(&REGISTRY);
        if let Some(hub) = registry.get_mut(&self.link) {
            hub.surfaces.retain(|c| c.id != self.conn.id);
            if hub.provider.is_none() && hub.surfaces.is_empty() {
                registry.remove(&self.link);
            }
        }
    }
}

/// True if the link has a provider (for diagnostics).
pub fn has_provider(link: &str) -> bool {
    lock(&REGISTRY).get(link).is_some_and(|hub| hub.provider.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(test: &str) -> String {
        format!("test-{test}")
    }

    fn page(page: usize) -> PageModel {
        PageModel { page, pages: vec!["One".into(), "Two".into()], ..PageModel::default() }
    }

    fn states(events: &[SurfaceEvent]) -> Vec<LinkState> {
        events.iter().filter_map(|e| if let SurfaceEvent::State(s) = e { Some(*s) } else { None }).collect()
    }

    #[test]
    fn provider_first() {
        let link = name("provider-first");
        let mut provider = provide(&link, "P").unwrap();
        assert!(provider.poll(|| page(0)).is_empty());
        let mut surface = attach(&link, Capabilities::default());
        assert_eq!(surface.state(), LinkState::Missing);
        // Nothing before the provider's tick
        assert!(surface.poll().is_empty());
        let events = provider.poll(|| page(1));
        assert!(matches!(events.as_slice(), [ProviderEvent::Attached { surface: id, .. }] if *id == surface.id()));
        let events = surface.poll();
        assert_eq!(states(&events), [LinkState::Attached]);
        assert!(matches!(&events[1], SurfaceEvent::Model(ModelChange::Snapshot(m)) if m.page == 1));
        assert_eq!(surface.provider_name(), "P");
        assert_eq!(surface.provider_version(), Some(PROTOCOL_VERSION));

        surface.send(Input::RowSelect { row: 3 });
        surface.send(Input::PageNext);
        let events = provider.poll(|| page(1));
        assert_eq!(events, [ProviderEvent::Input { surface: surface.id(), input: Input::RowSelect { row: 3 } }, ProviderEvent::Input { surface: surface.id(), input: Input::PageNext }]);

        provider.broadcast(&ModelChange::ActiveRow(3));
        assert_eq!(surface.poll(), [SurfaceEvent::Model(ModelChange::ActiveRow(3))]);
    }

    #[test]
    fn surface_first_and_reattach() {
        let link = name("surface-first");
        let mut surface = attach(&link, Capabilities::default());
        // Input without a provider is dropped
        surface.send(Input::PageNext);
        assert!(surface.poll().is_empty());
        assert!(!has_provider(&link));

        let mut provider = provide(&link, "P").unwrap();
        assert!(has_provider(&link));
        assert_eq!(provider.poll(|| page(0)).len(), 1);
        assert_eq!(states(&surface.poll()), [LinkState::Attached]);

        // The provider leaves: detached, the input is dropped
        provider.broadcast(&ModelChange::ActiveRow(1));
        drop(provider);
        let events = surface.poll();
        // The change sent before the provider left still arrives, then the detach
        assert_eq!(events, [SurfaceEvent::Model(ModelChange::ActiveRow(1)), SurfaceEvent::State(LinkState::Detached)]);
        surface.send(Input::PageNext);

        // A new provider welcomes it again
        let mut provider = provide(&link, "Q").unwrap();
        let events = provider.poll(|| page(1));
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(states(&surface.poll()), [LinkState::Attached]);
        assert_eq!(surface.provider_name(), "Q");
    }

    #[test]
    fn one_provider_per_link() {
        let link = name("one-provider");
        let first = provide(&link, "A").unwrap();
        assert_eq!(provide(&link, "B").unwrap_err(), ProvideError::AlreadyProvided);
        drop(first);
        assert!(provide(&link, "B").is_ok());
        // Another link is independent
        assert!(provide(&name("one-provider-other"), "C").is_ok());
    }

    #[test]
    fn several_surfaces() {
        let link = name("several");
        let mut provider = provide(&link, "P").unwrap();
        let mut a = attach(&link, Capabilities { encoders: 8, ..Capabilities::default() });
        let mut b = attach(&link, Capabilities::default());
        let events = provider.poll(|| page(0));
        assert_eq!(events.len(), 2);
        assert_eq!(provider.surface_count(), 2);
        assert!(provider.surfaces().any(|(id, c)| id == a.id() && c.encoders == 8));
        a.poll();
        b.poll();
        provider.broadcast(&ModelChange::SelectedSlot(Some(4)));
        provider.send_to(b.id(), ModelChange::ActiveRow(5));
        assert_eq!(a.poll(), [SurfaceEvent::Model(ModelChange::SelectedSlot(Some(4)))]);
        assert_eq!(b.poll().len(), 2);

        a.send(Input::SlotPress { id: 1 });
        b.send(Input::SlotPress { id: 2 });
        let events = provider.poll(|| page(0));
        assert!(events.contains(&ProviderEvent::Input { surface: a.id(), input: Input::SlotPress { id: 1 } }));
        assert!(events.contains(&ProviderEvent::Input { surface: b.id(), input: Input::SlotPress { id: 2 } }));

        // A surface leaves, the other stays
        let a_id = a.id();
        drop(a);
        assert_eq!(provider.poll(|| page(0)), [ProviderEvent::Detached { surface: a_id }]);
        assert_eq!(provider.surface_count(), 1);
        provider.broadcast(&ModelChange::ActiveRow(0));
        assert_eq!(b.poll().len(), 1);
    }

    #[test]
    fn versions() {
        let link = name("versions");
        let mut provider = provide(&link, "P").unwrap();
        let mut old = attach_with_version(&link, Capabilities::default(), Version::new(0, 9));
        let mut newer = attach_with_version(&link, Capabilities::default(), Version::new(PROTOCOL_VERSION.major, PROTOCOL_VERSION.minor + 1));
        let events = provider.poll(|| page(0));
        assert!(events.iter().any(|e| matches!(e, ProviderEvent::Refused { surface, .. } if *surface == old.id())));
        assert!(events.iter().any(|e| matches!(e, ProviderEvent::Attached { surface, .. } if *surface == newer.id())));
        // Refused only once
        assert!(provider.poll(|| page(0)).is_empty());
        assert_eq!(states(&old.poll()), [LinkState::Incompatible]);
        assert!(provider.poll(|| page(0)).is_empty());
        assert!(old.poll().is_empty());
        assert_eq!(states(&newer.poll()), [LinkState::Attached]);
        // An incompatible surface sends nothing and gets no model
        old.send(Input::PageNext);
        provider.broadcast(&ModelChange::ActiveRow(2));
        assert!(old.poll().is_empty());
        assert!(provider.poll(|| page(0)).is_empty());
    }

    #[test]
    fn stale_provider_messages_are_ignored() {
        let link = name("stale");
        let mut surface = attach(&link, Capabilities::default());
        let mut first = provide(&link, "A").unwrap();
        first.poll(|| page(0));
        drop(first);
        let mut second = provide(&link, "B").unwrap();
        second.poll(|| page(1));
        // The queue holds: welcome A, snapshot A, gone A, welcome B, snapshot B
        let events = surface.poll();
        assert_eq!(states(&events), [LinkState::Attached, LinkState::Detached, LinkState::Attached]);
        assert_eq!(surface.state(), LinkState::Attached);
        assert_eq!(surface.provider_name(), "B");
    }

    #[test]
    fn threads() {
        // Both sides on their own thread, each draining its own queue
        let link = name("threads");
        let mut provider = provide(&link, "P").unwrap();
        let surface_link = link.clone();
        let surface = std::thread::spawn(move || {
            let mut surface = attach(&surface_link, Capabilities::default());
            let mut received = 0;
            for _ in 0..2000 {
                for event in surface.poll() {
                    if matches!(event, SurfaceEvent::Model(_)) {
                        received += 1;
                    }
                }
                if surface.is_attached() {
                    surface.send(Input::PageNext);
                }
                if received >= 11 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            received
        });
        let mut inputs = 0;
        for i in 0..2000 {
            for event in provider.poll(|| page(0)) {
                if matches!(event, ProviderEvent::Input { .. }) {
                    inputs += 1;
                }
            }
            if provider.surface_count() > 0 && i % 2 == 0 {
                provider.broadcast(&ModelChange::ActiveRow(i % 6));
            }
            if surface.is_finished() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(surface.join().unwrap() >= 11);
        assert!(inputs > 0);
    }

    #[test]
    fn registry_is_cleaned_up() {
        let link = name("cleanup");
        let surface = attach(&link, Capabilities::default());
        let provider = provide(&link, "P").unwrap();
        drop(surface);
        drop(provider);
        assert!(!lock(&REGISTRY).contains_key(&link));
    }
}
