// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device browser mode (former `framework.mode.device.BrowserMode`). The knobs control the
//! selection of the filter rows, knob 8 the result. Changes are always relative.

use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ButtonId;

/// The device browser mode.
pub struct BrowserMode {
    name: String,
}

impl Default for BrowserMode {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserMode {
    pub fn new() -> Self {
        BrowserMode { name: "Browser".to_string() }
    }
}

impl Mode for BrowserMode {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let increase = ctx.model.value_changer().is_increase(value);
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        if index == 7 {
            if increase {
                browser.select_next_result();
            } else {
                browser.select_previous_result();
            }
            return;
        }
        if increase {
            browser.select_next_filter_item(index);
        } else {
            browser.select_previous_filter_item(index);
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            if let Some(browser) = ctx.model.browser_mut() {
                browser.reset_filter_column(index);
            }
        }
    }

    fn selected_item_name(&self, ctx: &Ctx) -> Option<String> {
        ctx.model.browser().and_then(|browser| browser.selected_result().map(str::to_string))
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.previous_content_type();
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.next_content_type();
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            if browser.is_active() {
                browser.stop_browsing(false);
            }
        }
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            if browser.is_active() {
                browser.stop_browsing(true);
            }
        }
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        if index == 7 {
            browser.select_next_result();
        } else {
            browser.select_next_filter_item(index);
        }
    }

    crate::impl_as_any!();
}
