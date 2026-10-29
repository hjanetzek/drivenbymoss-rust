// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The panorama view of the Launchpad (former `PanView`): the columns of the pad grid are
//! the panorama faders of the 8 tracks, the scene buttons are the panorama fader of the master track.

use std::cell::RefCell;
use std::rc::Rc;

use super::abstract_fader_view::{self, FaderView, FaderViewBase, BUTTON_COLOR};
use super::session_view::session_scroll_states;
use crate::daw::bank::BankItem;
use crate::daw::clip::SessionAlternative;
use crate::daw::color::DawColor;
use crate::controller::launchpad::surface as lp;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::framework::virtual_fader::{self, VirtualFader, VirtualFaderCallback, VirtualFaderView};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The panorama of the master track.
struct MasterPanCallback;

impl VirtualFaderCallback for MasterPanCallback {
    fn value(&self, ctx: &Ctx) -> i32 {
        ctx.model.master_track().pan()
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        ctx.model.master_track_mut().set_pan(value);
    }
}

/// The panorama view.
pub struct PanView {
    fader: FaderViewBase,
    master_fader: Rc<RefCell<VirtualFader>>,
}

impl PanView {
    pub fn new(_ctx: &Ctx) -> Self {
        PanView { fader: FaderViewBase::new("Panning"), master_fader: VirtualFader::new(Rc::new(MasterPanCallback), None).shared() }
    }
}

impl VirtualFaderView for PanView {
    fn fader_value(&self, ctx: &Ctx, index: usize) -> i32 {
        ctx.model.current_track_bank().item(index).pan()
    }

    fn on_value_knob(&self, ctx: &mut Ctx, index: usize, value: i32) {
        ctx.model.current_track_bank_mut().item_mut(index).set_pan(value);
    }
}

impl FaderView for PanView {
    fn setup_fader(&self, ctx: &mut Ctx, index: usize) {
        let track = ctx.model.current_track_bank().item(index);
        let color = if track.does_exist() { ctx.colors.color_index(DawColor::color_id(track.color())) } else { 0 };
        let value = track.pan();
        lp::setup_fader(ctx, index, color, true);
        lp::set_fader_value(ctx, index, value);
    }
}

impl View for PanView {
    fn name(&self) -> &str {
        &self.fader.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.fader.on_activate(ctx);
        abstract_fader_view::setup_faders(ctx, self);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(scene) = scene_index(button) else {
            return BUTTON_COLOR;
        };
        let track = ctx.model.master_track();
        let color = if track.does_exist() { ctx.colors.color_index(DawColor::color_id(track.color())) } else { 0 };
        let mut master_fader = self.master_fader.borrow_mut();
        master_fader.setup(color, true);
        master_fader.set_value(None, track.pan());
        master_fader.color_state(7 - scene)
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

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, velocity: i32) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down {
            return;
        }
        virtual_fader::move_to(&self.master_fader, ctx, 7 - index as i32, velocity, None);
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
