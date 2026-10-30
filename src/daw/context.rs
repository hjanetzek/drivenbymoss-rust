// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The context which is shared by all parts of the DAW model (former `DataSetupEx` + `BaseImpl`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use crate::backend::{Processor, Sender};
use crate::framework::value_changer::ValueChanger;
use crate::host::{Host, Scheduler};

use super::model::Model;
use super::events::ModelEvent;

/// State which is shared between all model items. Only used internally for state which is
/// required by item command methods but owned by another part of the model (e.g. whether
/// automation is currently written, which influences track commands).
/// A task which is executed with access to the whole model.
pub type ModelTask = Box<dyn FnOnce(&mut Model)>;

struct Shared {
    automation_rec_active: Cell<bool>,
    cursor_track_position: Cell<i32>,
    pinned_track: Cell<Option<i32>>,
    transport: Cell<TransportSnapshot>,
    scene_ranges: RefCell<Vec<(bool, f64, f64)>>,
    tasks: RefCell<Scheduler<Model>>,
    events: RefCell<Vec<ModelEvent>>,
}

/// Cheap to clone context which every model item holds to send commands to the backend.
#[derive(Clone)]
pub struct DawContext {
    sender: Sender,
    host: Host,
    value_changer: ValueChanger,
    shared: Rc<Shared>,
    muted: bool,
}

/// The transport state which is required by clip slots to calculate their playing state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TransportSnapshot {
    pub is_playing: bool,
    pub is_recording: bool,
    pub position: f64,
}

/// Marks the master track in places where a track position is stored.
pub(crate) const MASTER_POSITION: i32 = i32::MIN;

impl DawContext {
    pub fn new(host: Host, sender: Sender, value_changer: ValueChanger) -> Self {
        DawContext {
            sender,
            host,
            value_changer,
            shared: Rc::new(Shared {
                automation_rec_active: Cell::new(false),
                cursor_track_position: Cell::new(-1),
                pinned_track: Cell::new(None),
                transport: Cell::new(TransportSnapshot::default()),
                scene_ranges: RefCell::new(Vec::new()),
                tasks: RefCell::new(Scheduler::default()),
                events: RefCell::new(Vec::new()),
            }),
            muted: false,
        }
    }

    /// A copy of the context which does not send anything to the backend, does not schedule
    /// tasks and does not post events. Used for the "empty" placeholder items of the banks.
    pub fn muted(&self) -> Self {
        let mut ctx = self.clone();
        ctx.muted = true;
        ctx
    }

    /// True if this is the context of an "empty" placeholder item.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn sender(&self) -> &Sender {
        &self.sender
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn value_changer(&self) -> &ValueChanger {
        &self.value_changer
    }

    /// True if the transport currently writes arranger automation.
    pub fn is_automation_rec_active(&self) -> bool {
        self.shared.automation_rec_active.get()
    }

    pub(crate) fn set_automation_rec_active(&self, active: bool) {
        self.shared.automation_rec_active.set(active);
    }

    /// The position of the pinned or selected track (-1 if none, [`MASTER_POSITION`] for the
    /// master track). Used by parameters of the track processor.
    pub(crate) fn cursor_track_position(&self) -> i32 {
        self.shared.cursor_track_position.get()
    }

    pub(crate) fn set_cursor_track_position(&self, position: i32) {
        self.shared.cursor_track_position.set(position);
    }

    /// The position of the pinned track if the cursor track is pinned.
    pub(crate) fn pinned_track(&self) -> Option<i32> {
        self.shared.pinned_track.get()
    }

    pub(crate) fn set_pinned_track(&self, position: Option<i32>) {
        self.shared.pinned_track.set(position);
    }

    pub(crate) fn transport_snapshot(&self) -> TransportSnapshot {
        self.shared.transport.get()
    }

    pub(crate) fn update_transport_snapshot(&self, f: impl FnOnce(&mut TransportSnapshot)) {
        let mut snapshot = self.shared.transport.get();
        f(&mut snapshot);
        self.shared.transport.set(snapshot);
    }

    /// Get the (exists, begin, end) range of the scene at the given position.
    pub(crate) fn scene_range(&self, position: i32) -> Option<(bool, f64, f64)> {
        if position < 0 {
            return None;
        }
        self.shared.scene_ranges.borrow().get(position as usize).copied()
    }

    pub(crate) fn update_scene_range(&self, position: i32, f: impl FnOnce(&mut (bool, f64, f64))) {
        if position < 0 || self.muted {
            return;
        }
        let mut ranges = self.shared.scene_ranges.borrow_mut();
        let position = position as usize;
        if ranges.len() <= position {
            ranges.resize(position + 1, (false, 0.0, 0.0));
        }
        f(&mut ranges[position]);
    }

    /// Schedule a task which gets access to the whole model after the given delay (former
    /// `host.scheduleTask`). Executed by [`Model::run_due_tasks`].
    pub fn schedule(&self, delay_ms: u64, task: impl FnOnce(&mut Model) + 'static) {
        if self.muted {
            return;
        }
        self.shared.tasks.borrow_mut().schedule(self.host.now(), delay_ms, task);
    }

    pub(crate) fn take_due_tasks(&self, now: Instant) -> Vec<ModelTask> {
        self.shared.tasks.borrow_mut().take_due(now)
    }

    pub(crate) fn take_all_tasks(&self) -> Vec<ModelTask> {
        self.shared.tasks.borrow_mut().take_all()
    }

    pub(crate) fn has_tasks(&self) -> bool {
        !self.shared.tasks.borrow().is_empty()
    }

    /// Record an event for the controller layer.
    pub fn post_event(&self, event: ModelEvent) {
        if self.muted {
            return;
        }
        self.shared.events.borrow_mut().push(event);
    }

    pub(crate) fn take_events(&self) -> Vec<ModelEvent> {
        std::mem::take(&mut *self.shared.events.borrow_mut())
    }


    pub fn delay_updates(&self, processor: Processor) {
        if !self.muted {
            self.sender.delay_updates(processor);
        }
    }

    pub fn enable_updates(&self, processor: Processor, enable: bool) {
        if !self.muted {
            self.sender.enable_updates(processor, enable);
        }
    }

    pub fn send_strs(&self, processor: Processor, command: &str, values: &[&str]) {
        if !self.muted {
            self.sender.string_args(processor, command, values);
        }
    }

    // Shortcuts for sending to a processor (former `BaseImpl.sendOSC`).

    pub fn send(&self, processor: Processor, command: &str) {
        if !self.muted {
            self.sender.no_arg(processor, command);
        }
    }

    pub fn send_int(&self, processor: Processor, command: &str, value: i32) {
        if !self.muted {
            self.sender.int_arg(processor, command, value);
        }
    }

    pub fn send_double(&self, processor: Processor, command: &str, value: f64) {
        if !self.muted {
            self.sender.double_arg(processor, command, value);
        }
    }

    pub fn send_bool(&self, processor: Processor, command: &str, value: bool) {
        if !self.muted {
            self.sender.bool_arg(processor, command, value);
        }
    }

    pub fn send_str(&self, processor: Processor, command: &str, value: &str) {
        if !self.muted {
            self.sender.string_arg(processor, command, value);
        }
    }
}
