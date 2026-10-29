// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The sends view of the Launchpad (former `SendsView`): the columns of the pad grid are the
//! faders of the selected send of the 8 tracks, the scene buttons select the send.

use super::abstract_fader_view::{self, FaderView, FaderViewBase};
use super::session_view::session_scroll_states;
use crate::controller::launchpad::colors::*;
use crate::controller::launchpad::surface as lp;
use crate::daw::clip::SessionAlternative;
use crate::daw::color::DawColor;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::framework::virtual_fader::VirtualFaderView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The sends view.
pub struct SendsView {
    fader: FaderViewBase,
    selected_send: usize,
}

impl SendsView {
    pub fn new(_ctx: &Ctx) -> Self {
        SendsView { fader: FaderViewBase::new("Sends"), selected_send: 0 }
    }

    /// The index of the send which is edited.
    pub fn selected_send(&self) -> usize {
        self.selected_send
    }
}

impl VirtualFaderView for SendsView {
    fn fader_value(&self, ctx: &Ctx, index: usize) -> i32 {
        ctx.model.track_bank().item(index).send_bank().item(self.selected_send).value()
    }

    fn on_value_knob(&self, ctx: &mut Ctx, index: usize, value: i32) {
        ctx.model.track_bank_mut().item_mut(index).send_bank_mut().item_mut(self.selected_send).set_value(value);
    }
}

impl FaderView for SendsView {
    fn setup_fader(&self, ctx: &mut Ctx, index: usize) {
        let track = ctx.model.current_track_bank().item(index);
        let color = ctx.colors.color_index(DawColor::color_id(track.color()));
        let value = track.send_bank().item(self.selected_send).value();
        lp::setup_fader(ctx, index, color, false);
        lp::set_fader_value(ctx, index, value);
    }
}

impl View for SendsView {
    fn name(&self) -> &str {
        &self.fader.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.fader.on_activate(ctx);
        abstract_fader_view::setup_faders(ctx, self);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(scene) = scene_index(button) else {
            return 0;
        };
        if self.selected_send == scene {
            return LAUNCHPAD_COLOR_ORCHID;
        }
        if ctx.model.track_bank().can_edit_send(scene) {
            return LAUNCHPAD_COLOR_GREY_LO;
        }
        LAUNCHPAD_COLOR_BLACK
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        for i in 0..8 {
            self.setup_fader(ctx, i);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        abstract_fader_view::on_grid_note(ctx, self, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, _ctx: &mut Ctx, _note: i32) {
        // Prevent clip selection
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down {
            return;
        }
        self.selected_send = index;
        let send_name = ctx.model.track_bank().edit_send_name(self.selected_send);
        let message = format!("Send {}: {}", self.selected_send + 1, if send_name.is_empty() { "None" } else { &send_name });
        ctx.notify(&message);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(session_scroll_states(ctx))
    }

    fn as_virtual_fader_view(&self) -> Option<&dyn VirtualFaderView> {
        Some(self)
    }

    fn session_alternative(&self) -> Option<&dyn SessionAlternative> {
        Some(&self.fader)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn SessionAlternative> {
        Some(&mut self.fader)
    }

    impl_as_any!();
}
