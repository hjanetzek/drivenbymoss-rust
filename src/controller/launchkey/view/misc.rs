// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The further views of the Launchkey (former `DeviceConfigView`, `BrowserView`, `UserPadView`
//! and the framework's `DummyView`).

use super::super::colors::*;
use super::super::definition::*;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::grid::GRID_OFF;
use crate::framework::ids::{ButtonId, Views};
use crate::midi::ButtonEvent;

/// The device select view of the Mk3: the pads select the parameter pages of the cursor device,
/// the scene and arrow buttons toggle and select the device (former `DeviceConfigView`).
pub struct DeviceConfigView;

impl View for DeviceConfigView {
    fn name(&self) -> &str {
        "Device Configuration"
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let n = note - 36;
        let page = if n < 8 { n + 8 } else { n - 8 };
        if ctx.model.cursor_device().does_exist() {
            if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                bank.select_page(page.max(0) as usize);
            }
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let exists = ctx.model.cursor_device().does_exist();
        let (sel, last_page) = match ctx.model.cursor_device().parameter_bank() {
            Some(bank) => {
                let page_bank = bank.page_bank();
                (page_bank.selected_item_index() as i32, page_bank.item_count().min(16) as i32)
            }
            None => (-1, 0),
        };
        let grid = ctx.surface.grid();
        if !exists {
            for i in 0..16 {
                grid.light(36 + i, LAUNCHKEY_COLOR_BLACK);
            }
        }
        for i in 0..last_page {
            grid.light_ex(i % 8, i / 8, if i == sel { LAUNCHKEY_COLOR_MAGENTA_HI } else { LAUNCHKEY_COLOR_MAGENTA_LO });
        }
        for i in last_page..16 {
            grid.light_ex(i % 8, i / 8, LAUNCHKEY_COLOR_GREY_LO);
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let cursor_device = ctx.model.cursor_device();
        match button {
            ButtonId::Scene1 => {
                if cursor_device.is_enabled() {
                    LAUNCHKEY_COLOR_LIME_HI
                } else {
                    LAUNCHKEY_COLOR_GREY_LO
                }
            }
            ButtonId::ArrowUp => {
                if cursor_device.can_select_previous() {
                    LAUNCHKEY_COLOR_MAGENTA_LO
                } else {
                    LAUNCHKEY_COLOR_GREY_LO
                }
            }
            ButtonId::ArrowDown => {
                if cursor_device.can_select_next() {
                    LAUNCHKEY_COLOR_MAGENTA_LO
                } else {
                    LAUNCHKEY_COLOR_GREY_LO
                }
            }
            _ => LAUNCHKEY_COLOR_BLACK,
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let cursor_device = ctx.model.cursor_device_mut();
        match button {
            ButtonId::Scene1 => cursor_device.toggle_enabled_state(),
            ButtonId::ArrowUp => cursor_device.select_previous(),
            ButtonId::ArrowDown => cursor_device.select_next(),
            _ => {}
        }
    }

    crate::impl_as_any!();
}

/// The pad mode of the device for a view (former `BrowserView.VIEW_COMMANDS`).
fn view_command(view: Views) -> Option<i32> {
    Some(match view {
        Views::Session => PAD_MODE_SESSION,
        Views::Drum => PAD_MODE_DRUM,
        Views::Dummy1 => PAD_MODE_SCALE_CHORDS,
        Views::Dummy2 => PAD_MODE_USER_CHORDS,
        Views::Dummy3 => PAD_MODE_CUSTOM_MODE0,
        Views::Dummy4 => PAD_MODE_CUSTOM_MODE1,
        Views::Dummy5 => PAD_MODE_CUSTOM_MODE2,
        Views::Dummy6 => PAD_MODE_CUSTOM_MODE3,
        Views::Device => PAD_MODE_DEVICE_SELECT,
        Views::Browser => PAD_MODE_NAVIGATION,
        _ => return None,
    })
}

/// The browser of the Mk3 (former `BrowserView`): the device navigates the browser with its
/// pads itself; the scene buttons confirm or discard, the arrow buttons select the content type.
pub struct BrowserView;

impl View for BrowserView {
    fn name(&self) -> &str {
        "Browser"
    }

    fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {
        // Provided by the hardware
    }

    fn draw_grid(&mut self, _ctx: &mut Ctx) {
        // Provided by the hardware
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let browser = ctx.model.browser();
        let is_active = browser.is_some_and(|b| b.is_active());
        match button {
            ButtonId::Scene1 => LAUNCHKEY_COLOR_GREEN_HI,
            ButtonId::Scene2 => LAUNCHKEY_COLOR_RED_LO,
            ButtonId::ArrowUp => {
                if is_active && browser.is_some_and(|b| b.has_next_content_type()) {
                    LAUNCHKEY_COLOR_GREEN_LO
                } else {
                    LAUNCHKEY_COLOR_GREY_LO
                }
            }
            ButtonId::ArrowDown => {
                if is_active && browser.is_some_and(|b| b.has_previous_content_type()) {
                    LAUNCHKEY_COLOR_GREEN_LO
                } else {
                    LAUNCHKEY_COLOR_GREY_LO
                }
            }
            _ => LAUNCHKEY_COLOR_BLACK,
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        if !browser.is_active() {
            return;
        }
        match button {
            ButtonId::Scene1 => browser.stop_browsing(true),
            ButtonId::Scene2 => browser.stop_browsing(false),
            ButtonId::ArrowUp => browser.next_content_type(),
            ButtonId::ArrowDown => browser.previous_content_type(),
            _ => {}
        }
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        let is_active = ctx.model.browser().is_some_and(|b| b.is_active());
        if is_active {
            // Browser is already opened, normally this means the user opened it in the DAW.
            // Set the Navigation mode on the Launchkey
            ctx.surface.output.send_cc_ex(15, LAUNCHKEY_VIEW_SELECT, PAD_MODE_NAVIGATION);
        } else {
            let device = ctx.model.cursor_device();
            let (exists, name, position) = (device.does_exist(), device.name().to_string(), device.position());
            let insert = ctx.surface.is_shift_pressed() || !exists;
            if let Some(browser) = ctx.model.browser_mut() {
                if insert {
                    browser.insert_after_cursor_device(&name, position);
                } else {
                    browser.replace_device(&name, position);
                }
            }
        }
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        // Close the browser if still open
        if let Some(browser) = ctx.model.browser_mut() {
            if browser.is_active() {
                browser.stop_browsing(false);
            }
        }
        if ctx.views.active_id() == Some(Views::Browser) {
            // Browse (...) button does not send an 'up' event
            if let Some(button) = ctx.surface.button_mut(ButtonId::Browse) {
                button.state = None;
            }
            let previous = ctx.views.active_id_ignore_temporary();
            let command = match previous.and_then(view_command) {
                Some(command) if previous != Some(Views::Browser) => command,
                _ => PAD_MODE_SESSION,
            };
            ctx.surface.output.send_cc_ex(15, LAUNCHKEY_VIEW_SELECT, command);
        }
    }

    crate::impl_as_any!();
}

/// The view of the custom pad modes, the pads are handled by the device itself (former
/// `UserPadView`).
pub struct UserPadView;

impl View for UserPadView {
    fn name(&self) -> &str {
        "User Mode"
    }

    fn draw_grid(&mut self, _ctx: &mut Ctx) {
        // Drawn by the device itself
    }

    fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {
        // Handled by the device itself
    }

    fn button_color(&self, _ctx: &Ctx, _button: ButtonId) -> i32 {
        LAUNCHKEY_COLOR_BLACK
    }

    crate::impl_as_any!();
}

/// A view which does nothing (former `DummyView`), for the pad modes of the Mk3 which the device
/// handles itself.
pub struct DummyView {
    name: String,
}

impl DummyView {
    pub fn new(name: &str) -> Self {
        DummyView { name: name.to_string() }
    }
}

impl View for DummyView {
    fn name(&self) -> &str {
        &self.name
    }

    fn draw_grid(&mut self, _ctx: &mut Ctx) {
        // Intentionally empty
    }

    fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {
        // Intentionally empty
    }

    fn button_color(&self, ctx: &Ctx, _button: ButtonId) -> i32 {
        ctx.colors.color_index(GRID_OFF)
    }

    crate::impl_as_any!();
}
