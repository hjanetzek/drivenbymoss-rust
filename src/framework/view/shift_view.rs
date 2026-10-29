// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Base of shift views (former `AbstractShiftView`).

use crate::framework::view::base::ViewBase;

/// The state of a shift view: remembers if a function of the view was used while it was active
/// (then the shift button does not toggle the view on release).
pub struct ShiftViewBase {
    pub base: ViewBase,
    was_used: bool,
}

impl ShiftViewBase {
    pub fn new(name: &str) -> Self {
        ShiftViewBase { base: ViewBase::new(name), was_used: false }
    }

    pub fn was_used(&self) -> bool {
        self.was_used
    }

    pub fn set_was_used(&mut self) {
        self.was_used = true;
    }

    /// Call on activation of the view.
    pub fn on_activate(&mut self) {
        self.was_used = false;
    }
}
