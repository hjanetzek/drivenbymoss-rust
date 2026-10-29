// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The user view of the Launchpad (former `UserView`): the columns of the pad grid are faders
//! for the project or track parameters, the scene buttons select the parameter page.
//!
//! Note: the Java version binds the parameters to the continuous controls FADER1-8, which are
//! not created by the Launchpad setup (the binding has no effect), therefore it is omitted.

use super::abstract_fader_view::{self, FaderView, FaderViewBase};
use crate::controller::launchpad::colors::*;
use crate::controller::launchpad::surface as lp;
use crate::daw::clip::SessionAlternative;
use crate::daw::parameter_bank::ParameterBank;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::framework::virtual_fader::VirtualFaderView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The user view.
pub struct UserView {
    fader: FaderViewBase,
    is_project_mode: bool,
}

impl UserView {
    pub fn new(_ctx: &Ctx) -> Self {
        UserView { fader: FaderViewBase::new("Project/Track Parameters"), is_project_mode: true }
    }

    fn bank<'a>(&self, ctx: &'a Ctx) -> Option<&'a ParameterBank> {
        if self.is_project_mode {
            ctx.model.project().parameter_bank()
        } else {
            ctx.model.cursor_track_parameter_bank()
        }
    }

    fn bank_mut<'a>(&self, ctx: &'a mut Ctx) -> Option<&'a mut ParameterBank> {
        if self.is_project_mode {
            ctx.model.project_mut().parameter_bank_mut()
        } else {
            ctx.model.cursor_track_parameter_bank_mut()
        }
    }

    /// Switch between the project (true) and the track parameters (false).
    pub fn set_mode(&mut self, ctx: &mut Ctx, is_project_mode: bool) {
        self.is_project_mode = is_project_mode;
        self.notify_page(ctx);
    }

    pub fn select_previous_page(&mut self, ctx: &mut Ctx) {
        if let Some(bank) = self.bank_mut(ctx) {
            bank.scroll_backwards();
        }
        self.notify_page(ctx);
    }

    pub fn select_next_page(&mut self, ctx: &mut Ctx) {
        if let Some(bank) = self.bank_mut(ctx) {
            bank.scroll_forwards();
        }
        self.notify_page(ctx);
    }

    /// Former `MVHelper.notifySelectedProjectParameterPage/notifySelectedTrackParameterPage`.
    fn notify_page(&self, ctx: &mut Ctx) {
        let is_project_mode = self.is_project_mode;
        mv_helper::delay_display(ctx, move |ctx| {
            let (prefix, bank) = if is_project_mode {
                ("Project ", ctx.model.project().parameter_bank())
            } else {
                ("Track ", ctx.model.cursor_track_parameter_bank())
            };
            let name = bank.map(|b| b.page_bank().selected_item()).unwrap_or_default();
            format!("{prefix}Page: {}", if name.trim().is_empty() { "None" } else { &name })
        });
    }
}

impl VirtualFaderView for UserView {
    fn fader_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.bank(ctx).map(|bank| bank.item(index).value()).unwrap_or(0)
    }

    fn on_value_knob(&self, ctx: &mut Ctx, index: usize, value: i32) {
        if let Some(bank) = self.bank_mut(ctx) {
            bank.item_mut(index).set_value_immediatly(value);
        }
    }
}

impl FaderView for UserView {
    fn setup_fader(&self, ctx: &mut Ctx, index: usize) {
        lp::setup_fader(ctx, index, DAW_INDICATOR_COLORS[index], false);
    }
}

impl View for UserView {
    fn name(&self) -> &str {
        &self.fader.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.fader.on_activate(ctx);
        abstract_fader_view::setup_faders(ctx, self);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        ctx.surface.rebind_grid();
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = scene_index(button) else {
            return LAUNCHPAD_COLOR_BLACK;
        };
        let Some(bank) = self.bank(ctx) else {
            return LAUNCHPAD_COLOR_BLACK;
        };
        let page_bank = bank.page_bank();
        if page_bank.selected_item_index() == index {
            return LAUNCHPAD_COLOR_ORCHID_HI;
        }
        if index < page_bank.item_count() {
            LAUNCHPAD_COLOR_ORCHID_LO
        } else {
            LAUNCHPAD_COLOR_BLACK
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        for i in 0..8 {
            let value = self.fader_value(ctx, i);
            lp::set_fader_value(ctx, i, value);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        abstract_fader_view::on_grid_note(ctx, self, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, _ctx: &mut Ctx, _note: i32) {
        // Prevent clip selection
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, _event: ButtonEvent, _velocity: i32) {
        // Note: Java reacts on all button events
        let Some(index) = scene_index(button) else {
            return;
        };
        if let Some(bank) = self.bank_mut(ctx) {
            bank.select_page(index);
        }
        self.notify_page(ctx);
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let (left, right) = self.bank(ctx).map(|bank| (bank.can_scroll_page_backwards(), bank.can_scroll_page_forwards())).unwrap_or((false, false));
        Some(ScrollStates { can_scroll_left: left, can_scroll_right: right, can_scroll_up: !self.is_project_mode, can_scroll_down: self.is_project_mode })
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
