// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device browser mode (former `DeviceBrowserMode`). Works with the browser of the host
//! (Bitwig: filter columns and results, 16 entries on the Push 1, 48 on the Push 2/3); on Reaper
//! the browser has no filter columns, only the presets of the cursor device.

use crate::daw::browser::{Browser, BrowserColumn};
use crate::daw::color::ColorEx;
use crate::daw::constants::Capability;
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR2_ON, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::button_row_index;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::display1;
use super::super::super::surface as push;
use super::super::track::{host_supports, shorten_and_fix_ascii, Push3Encoder};
use super::super::{update_display, BaseModeState, PushDisplayMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selection {
    Off,
    Preset,
    Filter,
}

/// The device browser mode (former `DeviceBrowserMode`).
pub struct DeviceBrowserMode {
    state: BaseModeState,
    touched: [bool; 8],
    selection_mode: Selection,
    /// The selected filter column, -1 for the results.
    filter_column: i32,
}

impl Default for DeviceBrowserMode {
    fn default() -> Self {
        Self::new()
    }
}

/// Former `getFilterColumn`: the filter column for a knob/button index.
fn filter_column(browser: &Browser, index: usize) -> Option<&BrowserColumn> {
    if index < browser.filter_column_count() {
        browser.filter_column(index)
    } else {
        None
    }
}

/// Former `getColumnName`.
fn column_name(column: Option<&BrowserColumn>) -> String {
    match column {
        Some(column) if column.cursor_exists => {
            if column.cursor_name == column.wildcard {
                " ".to_string()
            } else {
                optimize_name(&column.cursor_name, 12)
            }
        }
        _ => String::new(),
    }
}

fn substring(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end.saturating_sub(start)).collect()
}

impl DeviceBrowserMode {
    pub fn new() -> Self {
        DeviceBrowserMode { state: BaseModeState::default(), touched: [false; 8], selection_mode: Selection::Off, filter_column: -1 }
    }

    fn is_knob_touched(&self, index: usize) -> bool {
        self.touched.get(index).copied().unwrap_or(false)
    }

    /// Former `changeSelectedColumnValue` (used by the rastered knob command).
    pub fn change_selected_column_value(&mut self, ctx: &mut Ctx, value: i32) {
        let index = if self.filter_column == -1 { 7 } else { self.filter_column as usize };
        self.change_value(ctx, index, value);
    }

    /// Former `resetFilterColumn`.
    pub fn reset_filter_column(&mut self) {
        self.filter_column = -1;
    }

    fn select_next(&mut self, ctx: &mut Ctx, index: usize, count: usize) {
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        if index < 7 {
            let Some(fi) = filter_column(browser, index).filter(|c| c.exists).map(|c| c.index) else {
                return;
            };
            self.filter_column = fi as i32;
            for _ in 0..count {
                browser.select_next_filter_item(fi);
            }
        } else {
            for _ in 0..count {
                browser.select_next_result();
            }
        }
    }

    fn select_previous(&mut self, ctx: &mut Ctx, index: usize, count: usize) {
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        if index < 7 {
            let Some(fi) = filter_column(browser, index).filter(|c| c.exists).map(|c| c.index) else {
                return;
            };
            self.filter_column = fi as i32;
            for _ in 0..count {
                browser.select_previous_filter_item(fi);
            }
        } else {
            for _ in 0..count {
                browser.select_previous_result();
            }
        }
    }

    fn change_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let mut speed = ctx.model.value_changer().calc_stepped_knob_change(value);
        let direction = speed > 0;
        if ctx.surface.is_shift_pressed() {
            speed *= 4;
        }
        let speed = speed.unsigned_abs() as usize;
        if direction {
            self.select_next(ctx, index, speed);
        } else {
            self.select_previous(ctx, index, speed);
        }
    }

    fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_shift_pressed() && index == 7 {
            if let Some(browser) = ctx.model.browser_mut() {
                browser.toggle_preview_enabled();
            }
            return;
        }
        if push::is_modern(ctx) {
            self.select_next(ctx, index, 1);
        } else {
            self.select_previous(ctx, index, 1);
        }
    }

    fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if push::is_modern(ctx) {
            self.select_previous(ctx, index, 1);
        } else {
            self.select_next(ctx, index, 1);
        }
    }

    fn filter_items_of_selected(&self, browser: &Browser) -> Vec<crate::daw::browser::BrowserColumnItem> {
        if self.filter_column < 0 {
            return Vec::new();
        }
        browser.filter_column(self.filter_column as usize).map(|c| c.items.clone()).unwrap_or_default()
    }
}

impl PushDisplayMode for DeviceBrowserMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let has_preview = host_supports(ctx, Capability::HasBrowserPreview);
        let Some(browser) = ctx.model.browser() else {
            return;
        };
        if !browser.is_active() {
            return;
        }

        match self.selection_mode {
            Selection::Off => {
                let mut selected_content_type = browser.selected_content_type().to_string();
                if self.filter_column == -1 {
                    selected_content_type = format!("{}{selected_content_type}", display1::SELECT_ARROW);
                }
                display.set_cell(0, 7, &selected_content_type);
                display.set_block(2, 2, "Selection:");
                let selected_result = match browser.selected_result() {
                    Some(result) if !result.trim().is_empty() => shorten_and_fix_ascii(result, 18),
                    _ => "None".to_string(),
                };

                let info_text = browser.info_text();
                let mut parts = info_text.split(':');
                let info0 = format!("{}:", parts.next().unwrap_or(""));
                let info1 = parts.next().map_or(String::new(), |s| s.trim().to_string());
                let length0 = info0.chars().count();
                let (a, b) = if length0 > 17 { (substring(&info0, 0, 17), substring(&info0, 17, length0)) } else { (info0.clone(), String::new()) };
                display.set_block(2, 0, &a).set_block(2, 1, &b);
                let length1 = info1.chars().count();
                let (a, b) = if length1 > 17 { (substring(&info1, 0, 17), substring(&info1, 17, length1)) } else { (info1.clone(), String::new()) };
                display.set_block(3, 0, &a).set_block(3, 1, &b);

                let length = selected_result.chars().count();
                let (sel1, sel2) = if length > 17 { (substring(&selected_result, 0, 17), substring(&selected_result, 17, length)) } else { (selected_result.clone(), String::new()) };
                display.set_block(3, 2, &sel1).set_cell(3, 6, &sel2);

                if has_preview {
                    display.set_cell(2, 7, "Preview:");
                    display.set_cell(3, 7, if browser.is_preview_enabled() { "On" } else { "Off" });
                }

                for i in 0..7 {
                    let column = filter_column(browser, i);
                    let mut name = column.map_or(String::new(), |c| shorten_and_fix_ascii(&c.name, 8));
                    if i as i32 == self.filter_column {
                        name = format!("{}{name}", display1::SELECT_ARROW);
                    }
                    display.set_cell(0, i, &name).set_cell(1, i, &column_name(column));
                }
            }

            Selection::Preset => {
                let results = browser.result_items();
                if !results.first().is_some_and(|r| r.exists) {
                    display.clear().set_block(1, 1, "       No results").set_block(1, 2, "available...");
                    return;
                }
                for i in 0..16 {
                    let text = match results.get(i) {
                        Some(result) => format!("{}{}", if result.is_selected { display1::SELECT_ARROW } else { ' ' }, shorten_and_fix_ascii(&result.name, 16)),
                        None => String::new(),
                    };
                    display.set_block(i % 4, i / 4, &text);
                }
            }

            Selection::Filter => {
                let items = self.filter_items_of_selected(browser);
                for i in 0..16 {
                    let Some(item) = items.get(i) else {
                        display.set_block(i % 4, i / 4, "");
                        continue;
                    };
                    let mut text = format!("{}{}                ", if item.is_selected { display1::SELECT_ARROW } else { ' ' }, shorten_and_fix_ascii(&item.name, 18));
                    if !item.name.is_empty() {
                        let hit_str = format!("({})", item.hit_count);
                        text = format!("{}{hit_str}", substring(&text, 0, 17usize.saturating_sub(hit_str.chars().count())));
                    }
                    display.set_block(i % 4, i / 4, &text);
                }
            }
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let supports_preview = host_supports(ctx, Capability::HasBrowserPreview);
        let Some(browser) = ctx.model.browser() else {
            return;
        };
        if !browser.is_active() {
            return;
        }

        match self.selection_mode {
            Selection::Off => {
                let selected_result = match browser.selected_result() {
                    Some(result) if !result.trim().is_empty() => format!("Selection: {result}"),
                    _ => "Selection: None".to_string(),
                };
                for i in 0..7 {
                    let column = filter_column(browser, i);
                    let header_top_name = if i == 0 { browser.info_text() } else { "" };
                    let header_bottom_name = if i == 0 { selected_result.as_str() } else { "" };
                    let menu_bottom_name = column_name(column);
                    display.add_option_element(header_top_name, column.map_or("", |c| c.name.as_str()), i as i32 == self.filter_column, header_bottom_name, &menu_bottom_name, menu_bottom_name != " ", false);
                }
                let bottom_menu = if supports_preview { "Preview" } else { "" };
                let menu_bottom_color = if browser.is_preview_enabled() { ColorEx::ORANGE } else { ColorEx::GRAY };
                display.add_option_element_colored("", browser.selected_content_type(), self.filter_column == -1, None, "", bottom_menu, false, supports_preview.then_some(menu_bottom_color), false);
            }

            Selection::Preset => {
                let results = browser.result_items();
                if !results.first().is_some_and(|r| r.exists) {
                    for i in 0..8 {
                        display.add_option_element(if i == 3 { "No results available..." } else { "" }, "", false, "", "", false, false);
                    }
                    return;
                }
                for i in 0..8 {
                    let mut items = Vec::with_capacity(6);
                    let mut selected = Vec::with_capacity(6);
                    for item in 0..6 {
                        let pos = i * 6 + item;
                        items.push(results.get(pos).map_or(String::new(), |r| optimize_name(&r.name, 14)));
                        selected.push(results.get(pos).is_some_and(|r| r.is_selected));
                    }
                    display.add_list_element(&items, &selected);
                }
            }

            Selection::Filter => {
                let column_items = self.filter_items_of_selected(browser);
                for i in 0..8 {
                    let mut items = Vec::with_capacity(6);
                    let mut selected = Vec::with_capacity(6);
                    for item_index in 0..6 {
                        let pos = i * 6 + item_index;
                        let Some(item) = column_items.get(pos) else {
                            items.push(String::new());
                            selected.push(false);
                            continue;
                        };
                        let hit_text = format!(" ({})", item.hit_count);
                        let mut text = optimize_name(&item.name, 12usize.saturating_sub(hit_text.chars().count()));
                        if !text.is_empty() {
                            text.push_str(&hit_text);
                        }
                        items.push(text);
                        selected.push(item.is_selected);
                    }
                    display.add_list_element(&items, &selected);
                }
            }
        }
    }
}

impl Mode for DeviceBrowserMode {
    fn name(&self) -> &str {
        "Browser"
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.stop_browsing(true);
        }
        self.selection_mode = Selection::Off;
        self.filter_column = -1;
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let browser = ctx.model.browser();
        let column_exists = |index: usize| browser.and_then(|b| filter_column(b, index)).is_some_and(|c| c.exists);

        if let Some(index) = button_row_index(0, button) {
            if index == 7 {
                if ctx.surface.is_shift_pressed() && host_supports(ctx, Capability::HasBrowserPreview) {
                    return if browser.is_some_and(|b| b.is_preview_enabled()) { PUSH_ORANGE_HI } else { PUSH_ORANGE_LO };
                }
                return BUTTON_COLOR_ON;
            }
            return if column_exists(index) { BUTTON_COLOR_ON } else { BUTTON_COLOR_OFF };
        }

        if let Some(index) = button_row_index(1, button) {
            if index == 7 {
                return BUTTON_COLOR2_ON;
            }
            return if column_exists(index) { BUTTON_COLOR2_ON } else { BUTTON_COLOR_OFF };
        }

        BUTTON_COLOR_OFF
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !self.is_knob_touched(index) {
            return;
        }
        if self.state.increase_knob_movement() {
            self.change_value(ctx, index, value);
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        // Make sure that only 1 knob gets changed in browse mode to prevent weird behavior
        if self.is_any_knob_touched() && !self.is_knob_touched(index) {
            return;
        }
        if let Some(touched) = self.touched.get_mut(index) {
            *touched = is_touched;
        }

        if !is_touched {
            self.selection_mode = Selection::Off;
            return;
        }

        if ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            if let Some(browser) = ctx.model.browser_mut() {
                if let Some(fi) = filter_column(browser, index).filter(|c| c.exists).map(|c| c.index) {
                    browser.reset_filter_column(fi);
                }
            }
            return;
        }

        if index == 7 {
            self.selection_mode = Selection::Preset;
            self.filter_column = -1;
        } else if let Some(fi) = ctx.model.browser().and_then(|b| filter_column(b, index)).filter(|c| c.exists).map(|c| c.index) {
            self.selection_mode = Selection::Filter;
            self.filter_column = fi as i32;
        }
    }

    fn is_any_knob_touched(&self) -> bool {
        self.touched.iter().any(|t| *t)
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.reset_filter_column();
        if let Some(browser) = ctx.model.browser_mut() {
            browser.previous_content_type();
        }
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.reset_filter_column();
        if let Some(browser) = ctx.model.browser_mut() {
            browser.next_content_type();
        }
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        ctx.model.browser().is_some_and(|b| b.has_previous_content_type())
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        ctx.model.browser().is_some_and(|b| b.has_next_content_type())
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.has_previous_item(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.has_next_item(ctx)
    }

    impl_as_any!();
}

impl Push3Encoder for DeviceBrowserMode {
    fn encoder_turn(&mut self, ctx: &mut Ctx, value: i32) {
        self.change_selected_column_value(ctx, value);
    }

    fn encoder_press(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let commit = !ctx.surface.is_shift_pressed();
        if let Some(browser) = ctx.model.browser_mut() {
            browser.stop_browsing(commit);
        }
        // Divergence: the Java workaround for the drum page scroll bug (repositionBankPage of
        // the drum view) is not needed, the drum pad bank keeps its position
    }

    fn encoder_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down || self.filter_column == 0 {
            return;
        }
        let Some(browser) = ctx.model.browser() else {
            return;
        };
        if self.filter_column == -1 {
            self.filter_column = browser.filter_column_count() as i32 - 1;
        } else {
            self.filter_column -= 1;
        }
        while self.filter_column > 0 && !browser.filter_column(self.filter_column as usize).is_some_and(|c| c.exists) {
            self.filter_column -= 1;
        }
    }

    fn encoder_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let Some(browser) = ctx.model.browser() else {
            return;
        };
        let max = browser.filter_column_count() as i32 - 1;
        loop {
            if self.filter_column >= max {
                self.filter_column = -1;
                break;
            }
            self.filter_column += 1;
            if !(self.filter_column >= max || !browser.filter_column(self.filter_column as usize).is_some_and(|c| c.exists)) {
                break;
            }
        }
    }

    fn arrow_center(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let device = ctx.model.cursor_device();
        let (name, position) = (device.name().to_string(), device.position());
        if let Some(browser) = ctx.model.browser_mut() {
            browser.toggle_insertion_point(&name, position);
        }
    }
}
