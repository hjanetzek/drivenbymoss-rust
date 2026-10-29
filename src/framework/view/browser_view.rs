// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A view for browsing devices and presets with the pad grid (former `BrowserView`).

use crate::framework::color::BUTTON_STATE_OFF;
use crate::framework::color_ids::browser_view::{COLUMN1, COLUMN2, COLUMN3, COLUMN4, COLUMN5, COLUMN6, COLUMN7, COLUMN8, CONFIRM, DISCARD, OFF, PLAY};
use crate::framework::ctx::Ctx;
use crate::framework::view::base::ViewBase;
use crate::midi::CMD_NOTE_ON;

/// The name of the view.
pub const NAME_BROWSER: &str = "Browser";

const COLUMNS: [&str; 8] = [COLUMN1, COLUMN2, COLUMN3, COLUMN4, COLUMN5, COLUMN6, COLUMN7, COLUMN8];
const COLUMN_ORDER: [usize; 6] = [0, 1, 2, 3, 4, 5];

/// The generic browser view. Controller specific views own it and forward the
/// [`crate::framework::feature::View`] functions to it.
pub struct BrowserViewBase {
    pub base: ViewBase,
}

impl Default for BrowserViewBase {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserViewBase {
    pub fn new() -> Self {
        BrowserViewBase { base: ViewBase::new(NAME_BROWSER) }
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
    }

    pub fn draw_grid(&self, ctx: &mut Ctx) {
        let colors = &ctx.colors;
        let Some(grid) = ctx.surface.pad_grid.as_mut() else {
            return;
        };
        grid.light_id(colors, 36, DISCARD);
        grid.light_id(colors, 37, OFF);
        for i in 38..42 {
            grid.light_id(colors, i, PLAY);
        }
        grid.light_id(colors, 42, OFF);
        grid.light_id(colors, 43, CONFIRM);
        for i in 44..52 {
            grid.light_id(colors, i, OFF);
        }
        for start in [52, 60, 68, 76] {
            for i in start..start + 8 {
                grid.light_id(colors, i, COLUMNS[(i - start) as usize]);
            }
        }
        for i in 84..100 {
            grid.light_id(colors, i, OFF);
        }
    }

    pub fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.browser().is_some_and(|b| b.is_active()) {
            return;
        }

        match note {
            // Cancel
            36 => {
                if velocity == 0 {
                    return;
                }
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.stop_browsing(false);
                }
                crate::framework::view::deferred::restore_view_later(ctx);
            }
            // OK
            43 => {
                if velocity == 0 {
                    return;
                }
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.stop_browsing(true);
                }
                crate::framework::view::deferred::restore_view_later(ctx);
            }
            38 => ctx.surface.send_midi_event(CMD_NOTE_ON, 48, velocity),
            39 => ctx.surface.send_midi_event(CMD_NOTE_ON, 60, velocity),
            40 => ctx.surface.send_midi_event(CMD_NOTE_ON, 72, velocity),
            41 => ctx.surface.send_midi_event(CMD_NOTE_ON, 84, velocity),
            // Not used
            _ => {}
        }

        if velocity == 0 || !(52..84).contains(&note) {
            return;
        }
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };

        let n = note - 52;
        let row = n / 8;
        let col = (n % 8) as usize;
        match col {
            6 => {}
            7 => match row {
                0 => browser.select_next_result(),
                1 => (0..8).for_each(|_| browser.select_next_result()),
                2 => (0..8).for_each(|_| browser.select_previous_result()),
                3 => browser.select_previous_result(),
                _ => {}
            },
            _ => {
                let column = COLUMN_ORDER[col];
                match row {
                    0 => browser.select_next_filter_item(column),
                    1 => (0..8).for_each(|_| browser.select_next_filter_item(column)),
                    2 => (0..8).for_each(|_| browser.select_previous_filter_item(column)),
                    3 => browser.select_previous_filter_item(column),
                    _ => {}
                }
            }
        }
    }

    pub fn button_color_id(&self) -> &'static str {
        BUTTON_STATE_OFF
    }
}
