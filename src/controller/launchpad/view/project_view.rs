// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The project view of the Launchpad (former `ProjectView`): application and project functions.

use crate::controller::launchpad::colors::*;
use crate::daw::application::{PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_EDIT, PANEL_LAYOUT_MIX};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::base::ViewBase;
use crate::impl_as_any;

/// The project view.
pub struct ProjectView {
    base: ViewBase,
}

impl ProjectView {
    pub fn new(_ctx: &Ctx) -> Self {
        ProjectView { base: ViewBase::new("Project") }
    }
}

impl View for ProjectView {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let is_engine_active = ctx.model.application().is_engine_active();
        let grid = ctx.surface.grid();

        for i in 36..100 {
            grid.light(i, LAUNCHPAD_COLOR_BLACK);
        }

        // First row (from bottom)
        // Toggle Note Editor
        grid.light(37, LAUNCHPAD_COLOR_AMBER_HI);
        // Toggle Automation Editor
        grid.light(38, LAUNCHPAD_COLOR_RED_HI);
        // Toggle Devices
        grid.light(40, LAUNCHPAD_COLOR_TURQUOISE_HI);
        // Toggle Mixer
        grid.light(42, LAUNCHPAD_COLOR_LIME_HI);

        // Row 3
        // Select layout 1, 2 and 3
        grid.light(54, LAUNCHPAD_COLOR_BLUE_LO);
        grid.light(56, LAUNCHPAD_COLOR_BLUE);
        grid.light(58, LAUNCHPAD_COLOR_BLUE_HI);

        // Row 5
        // Toggle inspector
        grid.light(68, LAUNCHPAD_COLOR_PINK_HI);
        // Toggle browser
        grid.light(75, LAUNCHPAD_COLOR_MAGENTA);

        // Row 8
        // Select Previous Project
        grid.light(93, LAUNCHPAD_COLOR_OCEAN_HI);
        grid.light(94, LAUNCHPAD_COLOR_OCEAN_HI);
        let engine_color = if is_engine_active { LAUNCHPAD_COLOR_GREEN_HI } else { LAUNCHPAD_COLOR_RED_LO };
        grid.light(95, engine_color);
        grid.light(96, engine_color);
        // Select Next Project
        grid.light(97, LAUNCHPAD_COLOR_OCEAN_HI);
        grid.light(98, LAUNCHPAD_COLOR_OCEAN_HI);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let application = ctx.model.application_mut();
        match note {
            37 => application.toggle_note_editor(),
            38 => application.toggle_automation_editor(),
            40 => application.toggle_devices(),
            42 => application.toggle_mixer(),
            54 => application.set_panel_layout(PANEL_LAYOUT_ARRANGE),
            56 => application.set_panel_layout(PANEL_LAYOUT_MIX),
            58 => application.set_panel_layout(PANEL_LAYOUT_EDIT),
            68 => application.toggle_inspector(),
            75 => application.toggle_browser_visibility(),
            93 | 94 => ctx.model.project_mut().previous(),
            95 | 96 => application.toggle_engine_active(),
            97 | 98 => ctx.model.project_mut().next(),
            // Not used
            _ => {}
        }
    }

    fn button_color(&self, _ctx: &Ctx, _button: ButtonId) -> i32 {
        LAUNCHPAD_COLOR_BLACK
    }

    fn scroll_states(&self, _ctx: &Ctx) -> Option<ScrollStates> {
        let mut states = ScrollStates::default();
        states.set_all(true);
        Some(states)
    }

    impl_as_any!();
}
