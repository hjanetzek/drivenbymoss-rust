// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device view of the Launchpad (former `DeviceView`): the columns of the pad grid are the
//! faders of the 8 parameters of the cursor device, the scene buttons control the device.

use super::abstract_fader_view::{self, FaderView, FaderViewBase};
use crate::controller::launchpad::colors::*;
use crate::controller::launchpad::surface as lp;
use crate::daw::clip::SessionAlternative;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::virtual_fader::VirtualFaderView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Reaper does not support the parameter page section (`Capability.HAS_PARAMETER_PAGE_SECTION`).
const HAS_PARAMETER_PAGE_SECTION: bool = false;
/// Reaper supports pinning (`Capability.HAS_PINNING`).
const HAS_PINNING: bool = true;

/// The device view.
pub struct DeviceView {
    fader: FaderViewBase,
}

impl DeviceView {
    pub fn new(_ctx: &Ctx) -> Self {
        DeviceView { fader: FaderViewBase::new("Device") }
    }
}

impl VirtualFaderView for DeviceView {
    fn fader_value(&self, ctx: &Ctx, index: usize) -> i32 {
        ctx.model.cursor_device().parameter_bank().map(|bank| bank.item(index).value()).unwrap_or(0)
    }

    fn on_value_knob(&self, ctx: &mut Ctx, index: usize, value: i32) {
        // Set immediately to prevent issue with relative scaling mode
        if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            bank.item_mut(index).set_value_immediatly(value);
        }
    }
}

impl FaderView for DeviceView {
    fn setup_fader(&self, ctx: &mut Ctx, index: usize) {
        lp::setup_fader(ctx, index, DAW_INDICATOR_COLORS[index], false);
        // Prevent issue with catch mode by initializing fader value at setup
        let value = self.fader_value(ctx, index);
        self.on_value_knob(ctx, index, value);
    }
}

impl View for DeviceView {
    fn name(&self) -> &str {
        &self.fader.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.fader.on_activate(ctx);
        abstract_fader_view::setup_faders(ctx, self);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let device = ctx.model.cursor_device();
        match button {
            ButtonId::Scene1 => {
                if device.is_enabled() {
                    LAUNCHPAD_COLOR_GREEN_HI
                } else {
                    LAUNCHPAD_COLOR_GREY_LO
                }
            }
            ButtonId::Scene2 => {
                if device.is_window_open() {
                    LAUNCHPAD_COLOR_AMBER_HI
                } else {
                    LAUNCHPAD_COLOR_AMBER_LO
                }
            }
            ButtonId::Scene3 => {
                if !HAS_PARAMETER_PAGE_SECTION {
                    LAUNCHPAD_COLOR_BLACK
                } else if device.is_parameter_page_section_visible() {
                    LAUNCHPAD_COLOR_PINK_HI
                } else {
                    LAUNCHPAD_COLOR_PINK_LO
                }
            }
            ButtonId::Scene4 => {
                if device.is_expanded() {
                    LAUNCHPAD_COLOR_MAGENTA_HI
                } else {
                    LAUNCHPAD_COLOR_MAGENTA_LO
                }
            }
            ButtonId::Scene5 => {
                if !HAS_PINNING {
                    LAUNCHPAD_COLOR_BLACK
                } else if device.is_pinned() {
                    LAUNCHPAD_COLOR_TURQUOISE_HI
                } else {
                    LAUNCHPAD_COLOR_TURQUOISE_LO
                }
            }
            ButtonId::Scene8 => LAUNCHPAD_COLOR_YELLOW,
            // Not used
            _ => LAUNCHPAD_COLOR_BLACK,
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

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        match button {
            ButtonId::Scene1 => ctx.model.cursor_device_mut().toggle_enabled_state(),
            ButtonId::Scene2 => ctx.model.cursor_device_mut().toggle_window_open(),
            ButtonId::Scene3 => ctx.model.cursor_device_mut().toggle_parameter_page_section_visible(),
            ButtonId::Scene4 => ctx.model.cursor_device_mut().toggle_expanded(),
            ButtonId::Scene5 => {
                let is_pinned = !ctx.model.cursor_device().is_pinned();
                ctx.model.cursor_device_mut().set_pinned(is_pinned);
                ctx.model.set_cursor_track_pinned(is_pinned);
                mv_helper::delay_display(ctx, |ctx| {
                    let device = ctx.model.cursor_device();
                    format!("{}: {}", device.name(), if device.is_pinned() { "Pinned" } else { "Not pinned" })
                });
            }
            ButtonId::Scene8 => {
                if !ctx.model.cursor_device().does_exist() {
                    let name = ctx.model.cursor_device().name().to_string();
                    let position = ctx.model.cursor_device().position();
                    if let Some(browser) = ctx.model.browser_mut() {
                        browser.insert_after_cursor_device(&name, position);
                    }
                } else {
                    ctx.model.browse_cursor_device_presets();
                }
            }
            // Not used
            _ => {}
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let device = ctx.model.cursor_device();
        let (left, right) = device.parameter_bank().map(|bank| (bank.can_scroll_page_backwards(), bank.can_scroll_page_forwards())).unwrap_or((false, false));
        Some(ScrollStates { can_scroll_left: left, can_scroll_right: right, can_scroll_up: device.can_select_previous(), can_scroll_down: device.can_select_next() })
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

