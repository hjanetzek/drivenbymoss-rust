// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The generic tempo, shuffle and browser views as used by the Maschine JAM (former
//! `TempoView`, `ShuffleView` and `BrowserView`).

use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::ButtonId;
use crate::framework::view::browser_view::BrowserViewBase;
use crate::framework::view::number_display_view::{shuffle_number, tempo_number, NumberDisplayViewBase};
use crate::impl_as_any;

/// Displays the tempo or the shuffle amount as a number on the grid.
pub struct NumberView {
    display: NumberDisplayViewBase,
    number: fn(&Ctx) -> i32,
}

impl NumberView {
    pub fn new_tempo(_ctx: &Ctx, text_color1: i32, text_color2: i32, background_color: i32) -> Self {
        NumberView { display: NumberDisplayViewBase::new("Tempo", text_color1, text_color2, background_color), number: tempo_number }
    }

    pub fn new_shuffle(_ctx: &Ctx, text_color1: i32, text_color2: i32, background_color: i32) -> Self {
        NumberView { display: NumberDisplayViewBase::new("Shuffle", text_color1, text_color2, background_color), number: shuffle_number }
    }
}

impl View for NumberView {
    fn name(&self) -> &str {
        &self.display.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.display.on_activate(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let number = (self.number)(ctx);
        self.display.draw_number(ctx, number);
        self.display.fill_bottom(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.display.on_grid_note(ctx, note, velocity);
    }

    impl_as_any!();
}

/// The browser view.
pub struct BrowserView {
    browser: BrowserViewBase,
}

impl BrowserView {
    pub fn new(_ctx: &Ctx) -> Self {
        BrowserView { browser: BrowserViewBase::new() }
    }
}

impl View for BrowserView {
    fn name(&self) -> &str {
        &self.browser.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.browser.on_activate(ctx);
    }

    fn button_color_id(&self, _ctx: &Ctx, _button: ButtonId) -> &'static str {
        self.browser.button_color_id()
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.browser.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.browser.on_grid_note(ctx, note, velocity);
    }

    impl_as_any!();
}
