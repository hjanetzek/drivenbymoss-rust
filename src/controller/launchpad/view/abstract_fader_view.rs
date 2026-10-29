// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Base of the views which simulate 8 faders with the columns of the pad grid (former
//! `AbstractFaderView`).
//!
//! In Java the fader views extend the session view but override nearly all of its functions;
//! only the scroll states and the session alternative flag remain. A concrete fader view owns a
//! [`FaderViewBase`], implements [`FaderView`] and forwards the functions of the
//! [`crate::framework::feature::View`] trait to the functions of this module:
//!
//! * `on_activate` → [`FaderViewBase::on_activate`]
//! * `on_grid_note` → [`on_grid_note`]
//! * `on_grid_note_long_press` → nothing (prevents clip selection)
//! * `button_color` → [`BUTTON_COLOR`] (or its own)
//! * `scroll_states` → [`super::session_view::session_scroll_states`]
//! * `as_virtual_fader_view` → `Some(self)`
//! * `session_alternative(_mut)` → the [`FaderViewBase`]

use crate::controller::launchpad::colors::LAUNCHPAD_COLOR_BLACK;
use crate::controller::launchpad::surface as lp;
use crate::daw::clip::SessionAlternative;
use crate::framework::ctx::Ctx;
use crate::framework::view::base::ViewBase;
use crate::framework::virtual_fader::{VirtualFaderView, VirtualFaderViewAdapter};

/// The default color of the buttons of the fader views.
pub const BUTTON_COLOR: i32 = LAUNCHPAD_COLOR_BLACK;

/// A view which uses the virtual faders of the surface.
pub trait FaderView: VirtualFaderView {
    /// Set the color and value of the fader (former `setupFader`).
    fn setup_fader(&self, ctx: &mut Ctx, index: usize);
}

/// The state of a fader view.
pub struct FaderViewBase {
    pub base: ViewBase,
    was_alternate_interaction_used: bool,
}

impl FaderViewBase {
    pub fn new(name: &str) -> Self {
        FaderViewBase { base: ViewBase::new(name), was_alternate_interaction_used: false }
    }

    /// Block the notes of the pads (the Java session view super call).
    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
    }
}

impl SessionAlternative for FaderViewBase {
    fn was_alternate_interaction_used(&self) -> bool {
        self.was_alternate_interaction_used
    }

    fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.was_alternate_interaction_used = was_used;
    }
}

/// Reset all faders and set up the 8 faders of the view (second part of the Java
/// `onActivate`).
pub fn setup_faders(ctx: &mut Ctx, view: &dyn FaderView) {
    lp::clear_faders(ctx);
    for i in 0..8 {
        view.setup_fader(ctx, i);
    }
}

/// Simulate faders: a pad moves the fader of its column to the value of its row.
pub fn on_grid_note(ctx: &mut Ctx, view: &dyn VirtualFaderView, note: i32, velocity: i32) {
    if velocity == 0 {
        return;
    }
    let num = note - 36;
    let index = (num % 8) as usize;
    let row = num / 8;
    // The view is borrowed, therefore it is handed in as the callback for the first step
    let direct = VirtualFaderViewAdapter { view, index };
    lp::move_fader(ctx, index, row, velocity, Some(&direct));
}
