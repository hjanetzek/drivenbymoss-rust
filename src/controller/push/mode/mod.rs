// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Push (former `controller.ableton.push.mode`).
//!
//! Every Push mode draws on the text display of the Push 1 ([`PushDisplayMode::update_display1`])
//! and on the graphics display of the Push 2/3 ([`PushDisplayMode::update_display2`]); its
//! [`crate::framework::feature::Mode::update_display`] calls [`update_display`], which takes the
//! display of the model out of the context while the mode draws on it (former
//! `BaseMode.updateDisplay`). The graphics display is rendered and sent in the flush of the
//! setup (Java: `display.send ()` at the end of `updateDisplay`).

pub mod configuration;
pub mod device;
pub mod misc;
pub mod track;

use crate::config::push as config;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::graphics::GraphicDisplay;
use crate::midi::ButtonEvent;

/// The rate of the knob movements which scroll one item (former `BaseMode.SCROLL_RATE`).
pub const SCROLL_RATE: i32 = 8;

/// The two display functions of a Push mode (former abstract methods of `BaseMode`).
pub trait PushDisplayMode {
    /// Draw on the text display of the Push 1. The display is cleared before and `all_done` is
    /// called after.
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay);

    /// Add the components of the graphics display of the Push 2/3.
    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay);
}

/// Former `BaseMode.updateDisplay`: let the mode draw on the display of the model.
pub fn update_display<M: PushDisplayMode + ?Sized>(mode: &mut M, ctx: &mut Ctx) {
    if super::surface::is_modern(ctx) {
        if let Some(mut display) = ctx.graphics_display.take() {
            mode.update_display2(ctx, &mut display);
            ctx.graphics_display = Some(display);
        }
        return;
    }
    if let Some(mut display) = ctx.text_display.take() {
        display.clear();
        mode.update_display1(ctx, &mut display);
        display.all_done();
        ctx.text_display = Some(display);
    }
}

/// The state of the former `BaseMode` which all Push modes share.
#[derive(Debug, Default)]
pub struct BaseModeState {
    movement_counter: i32,
}

impl BaseModeState {
    /// Former `increaseKnobMovement`: true every [`SCROLL_RATE`] calls.
    pub fn increase_knob_movement(&mut self) -> bool {
        self.movement_counter += 1;
        if self.movement_counter < SCROLL_RATE {
            return false;
        }
        self.movement_counter = 0;
        true
    }
}

/// Former `BaseMode.onFirstRow` (the default): select the track.
pub fn on_first_row_default(ctx: &mut Ctx, index: usize, event: ButtonEvent) {
    if event == ButtonEvent::Up {
        ctx.model.current_track_bank_mut().item_mut(index).select();
    }
}

/// Former `BaseMode.checkStopAutomationOnKnobRelease`.
pub fn check_stop_automation_on_knob_release(ctx: &mut Ctx, is_touched: bool) {
    if !config::is_stop_automation_on_knob_release(&ctx.config) || is_touched {
        return;
    }
    if ctx.model.transport().is_writing_arranger_automation() {
        ctx.model.transport_mut().toggle_write_arranger_automation();
    }
    if ctx.model.transport().is_writing_clip_launcher_automation() {
        ctx.model.transport_mut().toggle_write_clip_launcher_automation();
    }
}

/// Register all modes (former `createModes`).
pub fn register_modes(ctx: &mut Ctx) {
    track::register_track_modes(ctx);
    device::register_device_modes(ctx);
    misc::register_misc_modes(ctx);
    configuration::register_configuration_modes(ctx);
}

/// React on the model events which Java modes observe (e.g. the browser).
pub fn handle_model_events(ctx: &mut Ctx, events: &[crate::daw::events::ModelEvent]) {
    let _ = (ctx, events);
}
