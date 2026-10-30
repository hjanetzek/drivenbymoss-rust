// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Host services: logging, notifications and delayed task execution (former `IHost`).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Receives log output.
pub trait LogSink {
    fn log(&self, message: &str);
}

/// Writes log output to stderr.
pub struct StderrLog;

impl LogSink for StderrLog {
    fn log(&self, message: &str) {
        eprintln!("[DrivenByMoss] {message}");
    }
}

/// A log which keeps the most recent messages, e.g. to display them in a UI.
#[derive(Default)]
pub struct MemoryLog {
    pub lines: RefCell<VecDeque<String>>,
}

impl MemoryLog {
    const MAX_LINES: usize = 1000;
}

impl LogSink for MemoryLog {
    fn log(&self, message: &str) {
        let mut lines = self.lines.borrow_mut();
        if lines.len() >= Self::MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(message.to_string());
    }
}

/// Logging and notification functions shared by all parts of an instance.
#[derive(Clone)]
pub struct Host {
    log: Rc<dyn LogSink>,
    notifications: Rc<RefCell<Vec<String>>>,
    /// The time of the current call into the core (see [`Host::now`]).
    time: Rc<Cell<Instant>>,
}

impl Host {
    pub fn new(log: Rc<dyn LogSink>) -> Self {
        Host { log, notifications: Rc::new(RefCell::new(Vec::new())), time: Rc::new(Cell::new(Instant::now())) }
    }

    /// The time of the current call into the core. The core only runs when the host calls it
    /// (tick, MIDI, model updates); the app sets the time at each of these calls, a replay of a
    /// recorded session sets it to the recorded time. All timing of the core uses it.
    pub fn now(&self) -> Instant {
        self.time.get()
    }

    /// Set the time of the current call into the core (see [`Host::now`]).
    pub fn set_time(&self, time: Instant) {
        self.time.set(time);
    }

    pub fn println(&self, message: &str) {
        self.log.log(message);
    }

    pub fn error(&self, message: &str) {
        self.log.log(&format!("ERROR: {message}"));
    }

    /// Show a short notification to the user (popup window in the Java version).
    pub fn show_notification(&self, message: &str) {
        self.log.log(message);
        self.notifications.borrow_mut().push(message.to_string());
    }

    pub fn take_notifications(&self) -> Vec<String> {
        std::mem::take(&mut *self.notifications.borrow_mut())
    }
}

/// A task which is executed with the context `T`.
pub type Task<T> = Box<dyn FnOnce(&mut T)>;

/// A queue of tasks which are executed after a delay. The tasks get the context `T`.
pub struct Scheduler<T> {
    tasks: Vec<(Instant, Task<T>)>,
}

impl<T> Default for Scheduler<T> {
    fn default() -> Self {
        Scheduler { tasks: Vec::new() }
    }
}

impl<T> Scheduler<T> {
    /// Schedule a task to be executed the given number of milliseconds after `now`.
    pub fn schedule(&mut self, now: Instant, delay_ms: u64, task: impl FnOnce(&mut T) + 'static) {
        self.tasks.push((now + Duration::from_millis(delay_ms), Box::new(task)));
    }

    /// Remove all tasks which are due.
    pub fn take_due(&mut self, now: Instant) -> Vec<Task<T>> {
        let mut due = Vec::new();
        let mut i = 0;
        while i < self.tasks.len() {
            if self.tasks[i].0 <= now {
                due.push(self.tasks.remove(i).1);
            } else {
                i += 1;
            }
        }
        due
    }

    /// Remove all tasks regardless of their due time (used in tests to fast forward).
    pub fn take_all(&mut self) -> Vec<Task<T>> {
        self.tasks.drain(..).map(|(_, task)| task).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}
