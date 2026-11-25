// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tells a host which follows the controller (zynthian) what the user does on the surface
//! (doc/HOST_BRIDGE_GUIDE.md §4.13, not part of Java):
//!
//! - `ControllerView`: the view the user picked with a button ([`Ctx::report_controller_view`],
//!   called by the view buttons of a controller, not by the view activation, so that a startup,
//!   a recalled preferred view or a refresh sends nothing).
//! - `ClipEditWindow`: the visible window of the step sequencer
//!   ([`View::clip_edit_window`](super::feature::View::clip_edit_window)). Sent after a
//!   sequencer's `ControllerView` and then on every change of the window or of the edited clip
//!   ([`Ctx::sync_clip_edit_window`], called with every flush), as long as the last picked view
//!   is a sequencer and the active view has a window.
//!
//! Only a host which handles the commands gets them ([`HostFeature::ControllerViews`],
//! [`HostFeature::ClipEditWindow`]).

use super::ctx::Ctx;
use super::ids::Views;
use crate::daw::bank::BankItem;
use crate::daw::context::HostFeature;
use crate::protocol::{Cmd, ControllerViewKind, EditWindow};

/// The state of the reports: the last picked view and the last sent window.
#[derive(Debug, Default)]
pub struct ControllerViewSync {
    /// True while the last view which the user picked is a sequencer: the window is sent.
    sequencer_picked: bool,
    /// The last sent window and the edited clip (track position, slot position).
    last_window: Option<(EditWindow, (i32, i32))>,
}

/// The coarse kind of a view (for the view buttons which select one of several views).
pub fn kind_of_view(view: Views) -> ControllerViewKind {
    match view {
        Views::Session | Views::ScenePlay => ControllerViewKind::Launcher,
        Views::Sequencer | Views::PolySequencer | Views::Raindrops => ControllerViewKind::Sequencer,
        Views::Drum | Views::DrumXox | Views::Drum4 | Views::Drum8 => ControllerViewKind::DrumSequencer,
        // The 64 pads of Drum64 play, they do not sequence
        Views::Play | Views::Chords | Views::Piano | Views::Drum64 => ControllerViewKind::Play,
        Views::Mix | Views::TrackVolume | Views::TrackPan | Views::TrackSends => ControllerViewKind::Mixer,
        Views::Device => ControllerViewKind::Device,
        _ => ControllerViewKind::Other,
    }
}

impl Ctx {
    /// The user picked a view of the kind with a button (also the one which is already active):
    /// tell a host which handles `ControllerView`, and for a sequencer send its edit window
    /// right after it (also an unchanged one). Call it from the button commands only.
    pub fn report_controller_view(&mut self, kind: ControllerViewKind) {
        if self.model.ctx().host_supports(HostFeature::ControllerViews) {
            self.model.ctx().send(Cmd::ControllerView { kind });
        }
        self.controller_view.sequencer_picked = kind.is_sequencer();
        self.controller_view.last_window = None;
        self.sync_clip_edit_window();
    }

    /// [`Ctx::report_controller_view`] with the kind of the active view ([`kind_of_view`]).
    pub fn report_active_controller_view(&mut self) {
        if let Some(view) = self.views.active_id() {
            self.report_controller_view(kind_of_view(view));
        }
    }

    /// Send the edit window of the active sequencer view if it or the edited clip changed since
    /// it was sent last; nothing if the last picked view is no sequencer, the active view has no
    /// window or the host does not handle `ClipEditWindow`.
    pub fn sync_clip_edit_window(&mut self) {
        if !self.controller_view.sequencer_picked || !self.model.ctx().host_supports(HostFeature::ClipEditWindow) {
            return;
        }
        let Some(view) = self.active_view() else {
            return;
        };
        let Some(window) = view.try_borrow().ok().and_then(|view| view.clip_edit_window(self)) else {
            return;
        };
        let track = self.model.cursor_track();
        let clip = (if track.does_exist() { track.position() } else { -1 }, self.model.selected_slot().map_or(-1, |slot| slot.position()));
        if self.controller_view.last_window == Some((window, clip)) {
            return;
        }
        self.controller_view.last_window = Some((window, clip));
        self.model.ctx().send(Cmd::ClipEditWindow { window });
    }
}
