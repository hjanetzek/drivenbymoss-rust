// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The browser mode (former `BrowseMode`). The Reaper browser has no filter columns, therefore
//! there only the last column (content type and result) is available.

use super::{shorten_and_fix_ascii, with_display, MAIN_ENCODER};
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::impl_as_any;

/// Browse presets with the knobs.
pub struct BrowseMode {
    sel_column: i32,
}

impl BrowseMode {
    pub fn new(_ctx: &Ctx) -> Self {
        BrowseMode { sel_column: 7 }
    }

    pub fn select_parameter(&mut self, index: i32) {
        self.sel_column = index;
    }

    pub fn selected_parameter(&self) -> i32 {
        if self.sel_column == -1 {
            0
        } else {
            self.sel_column
        }
    }

    /// The index of the filter column shown in the given display column, if it exists (former
    /// `getFilterColumn` with `doesExist`).
    fn existing_filter_column(ctx: &Ctx, index: i32) -> Option<usize> {
        let browser = ctx.model.browser()?;
        let column = browser.filter_column(usize::try_from(index).ok()?)?;
        column.exists.then_some(column.index)
    }

    fn select_next(ctx: &mut Ctx, index: i32, count: usize) {
        let filter_column = if index < 7 { Self::existing_filter_column(ctx, index) } else { None };
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        if index < 7 {
            if let Some(column) = filter_column {
                for _ in 0..count {
                    browser.select_next_filter_item(column);
                }
                if browser.selected_filter_item_index(column) == -1 {
                    browser.next_filter_item_page(column);
                }
            }
        } else {
            for _ in 0..count {
                browser.select_next_result();
            }
        }
    }

    fn select_previous(ctx: &mut Ctx, index: i32, count: usize) {
        let filter_column = if index < 7 { Self::existing_filter_column(ctx, index) } else { None };
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        // Note: same as in Java the item is selected count * count times
        for _ in 0..count {
            if index < 7 {
                if let Some(column) = filter_column {
                    for _ in 0..count {
                        browser.select_previous_filter_item(column);
                    }
                    if browser.selected_filter_item_index(column) == -1 {
                        browser.previous_filter_item_page(column);
                    }
                }
            } else {
                for _ in 0..count {
                    browser.select_previous_result();
                }
            }
        }
    }
}

impl Mode for BrowseMode {
    fn name(&self) -> &str {
        "Browser"
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.stop_browsing(true);
        }
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let speed = ctx.model.value_changer().calc_stepped_knob_change(value);
        let column = if index == MAIN_ENCODER { self.sel_column } else { index as i32 };
        if speed < 0 {
            Self::select_previous(ctx, column, speed.unsigned_abs() as usize);
        } else {
            Self::select_next(ctx, column, speed.unsigned_abs() as usize);
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let Some(browser) = ctx.model.browser() else {
            return;
        };
        let is_preset_session = browser.is_preset_content_type();
        let device_exists = ctx.model.cursor_device().does_exist();
        if is_preset_session && !(browser.is_active() && device_exists) {
            ctx.notify("No active Browsing Session. Select device and press Browser...");
            return;
        }

        let selected = self.selected_parameter();
        let columns: Vec<(String, String)> = (0..7)
            .map(|i| match browser.filter_column(i) {
                Some(column) => {
                    let value = if !column.cursor_exists {
                        String::new()
                    } else if column.cursor_name == column.wildcard {
                        "-".to_string()
                    } else {
                        column.cursor_name.clone()
                    };
                    (shorten_and_fix_ascii(&column.name, 6), value)
                }
                None => (String::new(), String::new()),
            })
            .collect();
        let selected_result = browser.selected_result().map(|s| s.to_string()).unwrap_or_default();
        let mut selected_content_type = browser.selected_content_type().to_string();
        if selected == 7 {
            selected_content_type = format!(">{selected_content_type}");
        }
        with_display(ctx, |d| {
            d.clear();
            for (i, (name, value)) in columns.iter().enumerate() {
                let name = if i as i32 == selected { format!(">{name}") } else { name.clone() };
                d.set_cell(0, i, &name).set_cell(1, i, value);
            }
            d.set_cell(0, 7, &selected_content_type).set_cell(1, 7, if selected_result.is_empty() { "-" } else { &selected_result });
            d.all_done();
        });
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if ctx.surface.is_shift_pressed() {
            self.select_previous_item_page(ctx);
            return;
        }
        let selected = self.selected_parameter();
        if selected > 0 {
            self.select_parameter(selected - 1);
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        if ctx.surface.is_shift_pressed() {
            self.select_next_item_page(ctx);
            return;
        }
        let selected = self.selected_parameter();
        if selected < 7 {
            self.select_parameter(selected + 1);
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.previous_content_type();
        }
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.next_content_type();
        }
    }

    fn has_previous_item(&self, _ctx: &Ctx) -> bool {
        self.selected_parameter() > 0
    }

    fn has_next_item(&self, _ctx: &Ctx) -> bool {
        self.selected_parameter() < 7
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        ctx.model.browser().is_some_and(|b| b.has_previous_content_type())
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        ctx.model.browser().is_some_and(|b| b.has_next_content_type())
    }

    impl_as_any!();
}
