// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The browser view of the Launchpad (former `LaunchpadBrowserView`).

use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::browser_view::BrowserViewBase;
use crate::impl_as_any;

/// The browser view.
pub struct LaunchpadBrowserView {
    browser: BrowserViewBase,
}

impl LaunchpadBrowserView {
    pub fn new(_ctx: &Ctx) -> Self {
        LaunchpadBrowserView { browser: BrowserViewBase::new() }
    }
}

impl View for LaunchpadBrowserView {
    fn name(&self) -> &str {
        &self.browser.base.name
    }

    // BrowserView
    crate::forward_view!(browser => on_activate, draw_grid, on_grid_note);

    fn button_color_id(&self, _ctx: &Ctx, _button: ButtonId) -> &'static str {
        self.browser.button_color_id()
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let (left, right) = ctx.model.browser().map(|b| (b.has_previous_content_type(), b.has_next_content_type())).unwrap_or((false, false));
        Some(ScrollStates { can_scroll_left: left, can_scroll_right: right, can_scroll_up: true, can_scroll_down: true })
    }

    impl_as_any!();
}
