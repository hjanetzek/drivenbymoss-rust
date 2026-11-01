// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The tempo and shuffle views of the Launchpad (former `LaunchpadTempoView` and
//! `LaunchpadShuffleView`).

use crate::controller::launchpad::colors::*;
use crate::daw::constants::GrooveParameterId;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::view::deferred::restore_view_later;
use crate::framework::view::number_display_view::{shuffle_number, tempo_number, NumberDisplayViewBase};
use crate::impl_as_any;

fn all_scroll_states() -> Option<ScrollStates> {
    let mut states = ScrollStates::default();
    states.set_all(true);
    Some(states)
}

/// Displays the tempo.
pub struct LaunchpadTempoView {
    display: NumberDisplayViewBase,
}

impl LaunchpadTempoView {
    /// In the setup: `LaunchpadTempoView::new(ctx, LAUNCHPAD_COLOR_BLUE_HI, LAUNCHPAD_COLOR_WHITE, LAUNCHPAD_COLOR_BLACK)`.
    pub fn new(_ctx: &Ctx, text_color1: i32, text_color2: i32, background_color: i32) -> Self {
        LaunchpadTempoView { display: NumberDisplayViewBase::new("Tempo", text_color1, text_color2, background_color) }
    }
}

impl View for LaunchpadTempoView {
    fn name(&self) -> &str {
        &self.display.base.name
    }

    // AbstractNumberDisplayView
    crate::forward_view!(display => on_activate, on_grid_note);

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.display.draw_number(ctx, tempo_number(ctx));
        self.display.fill_bottom(ctx);
    }

    fn scroll_states(&self, _ctx: &Ctx) -> Option<ScrollStates> {
        all_scroll_states()
    }

    impl_as_any!();
}

/// Displays the shuffle amount, the lowest row toggles shuffle and its rate.
pub struct LaunchpadShuffleView {
    display: NumberDisplayViewBase,
}

impl LaunchpadShuffleView {
    /// In the setup: `LaunchpadShuffleView::new(ctx, LAUNCHPAD_COLOR_PINK_HI, LAUNCHPAD_COLOR_WHITE, LAUNCHPAD_COLOR_BLACK)`.
    pub fn new(_ctx: &Ctx, text_color1: i32, text_color2: i32, background_color: i32) -> Self {
        LaunchpadShuffleView { display: NumberDisplayViewBase::new("Shuffle", text_color1, text_color2, background_color) }
    }

    fn fill_bottom(&self, ctx: &mut Ctx) {
        self.display.fill_bottom(ctx);

        let groove = ctx.model.groove();
        let enabled = groove.parameter(GrooveParameterId::Enabled);
        if !enabled.does_exist() {
            return;
        }
        let enabled_color = if enabled.value() == 0 { LAUNCHPAD_COLOR_GREY_HI } else { LAUNCHPAD_COLOR_GREEN_HI };
        let rate = groove.parameter(GrooveParameterId::ShuffleRate);
        let rate_colors = if rate.does_exist() {
            let is_eight = rate.value() == 0;
            Some((if is_eight { LAUNCHPAD_COLOR_BLUE_HI } else { LAUNCHPAD_COLOR_GREY_HI }, if !is_eight { LAUNCHPAD_COLOR_BLUE_HI } else { LAUNCHPAD_COLOR_GREY_HI }))
        } else {
            None
        };

        let grid = ctx.surface.grid();
        grid.light_ex(0, 7, enabled_color);
        if let Some((eight, sixteen)) = rate_colors {
            grid.light_ex(2, 7, eight);
            grid.light_ex(3, 7, sixteen);
        }
    }

    /// Toggle a groove parameter between 0 and 1. Returns false if the parameter does not exist.
    fn toggle(ctx: &mut Ctx, id: GrooveParameterId) -> bool {
        let parameter = ctx.model.groove_mut().parameter_mut(id);
        if !parameter.does_exist() {
            return false;
        }
        let value = if parameter.value() == 0 { 1.0 } else { 0.0 };
        parameter.set_normalized_value(value);
        true
    }
}

impl View for LaunchpadShuffleView {
    fn name(&self) -> &str {
        &self.display.base.name
    }

    // AbstractNumberDisplayView
    crate::forward_view!(display => on_activate);

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.display.draw_number(ctx, shuffle_number(ctx));
        self.fill_bottom(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity != 0 {
            return;
        }

        // Toggle shuffle on/off
        if note == 36 && Self::toggle(ctx, GrooveParameterId::Enabled) {
            return;
        }

        // Shuffle rate
        if (note == 38 || note == 39) && Self::toggle(ctx, GrooveParameterId::ShuffleRate) {
            return;
        }

        restore_view_later(ctx);
    }

    fn scroll_states(&self, _ctx: &Ctx) -> Option<ScrollStates> {
        all_scroll_states()
    }

    impl_as_any!();
}
