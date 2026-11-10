// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Launchkey (former `LaunchkeyMk3VolumeMode`, `LaunchkeyMk3PanningMode`,
//! `LaunchkeyMk3SendMode`, `LaunchkeyMk3ParameterMode`, `CustomMode` and
//! `LaunckeyMiniMk3ProjectParamsMode`).
//!
//! The Mk3 modes are the framework modes with the display of the Launchkey: [`Mk3Mode`] wraps a
//! mode and writes the display. The knob modes write the base screen and the knob screens, the
//! fader modes the fader screens (including the master fader).
//!
//! Difference to Java: the parameter mode shows the name of the selected parameter page after
//! the device name. Java appends the `Optional` of the page name to the text, which shows
//! "Optional[...]" (UPSTREAM_CANDIDATES.md).

use crate::controller::launchkey::display::{SCREEN_ROW_BASE, SCREEN_ROW_FADERS, SCREEN_ROW_POTS};
use crate::daw::bank::BankItem;
use crate::daw::util::optimize_name;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::ContinuousId;
use crate::framework::mode::device::{ParameterMode, ProjectParamsMode};
use crate::framework::mode::parameter_mode::{sequential_controls, ModeBank};
use crate::framework::mode::track::{TrackPanMode, TrackSendMode, TrackVolumeMode};
use crate::daw::events::ParameterBankId;

/// What a Mk3 mode shows on the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mk3Display {
    Volume,
    Pan,
    Send(usize),
    Parameters,
}

/// A mode of the Mk3 with the display of the Launchkey (former `LaunchkeyMk3VolumeMode`,
/// `LaunchkeyMk3PanningMode`, `LaunchkeyMk3SendMode` and `LaunchkeyMk3ParameterMode`).
pub struct Mk3Mode {
    inner: Box<dyn Mode>,
    display: Mk3Display,
    are_knobs: bool,
}

impl Mk3Mode {
    fn new(inner: Box<dyn Mode>, display: Mk3Display, controls: &[ContinuousId]) -> Self {
        Mk3Mode { inner, display, are_knobs: controls.first() == Some(&ContinuousId::Knob1) }
    }

    pub fn volume(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let mode = TrackVolumeMode::new_with_controls(ctx, true, controls.clone());
        Self::new(Box::new(mode), Mk3Display::Volume, &controls)
    }

    pub fn pan(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let mode = TrackPanMode::new_with_controls(ctx, true, controls.clone());
        Self::new(Box::new(mode), Mk3Display::Pan, &controls)
    }

    pub fn send(send_index: usize, ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let mode = TrackSendMode::new_with_controls(send_index as i32, ctx, true, controls.clone());
        Self::new(Box::new(mode), Mk3Display::Send(send_index), &controls)
    }

    pub fn parameters(ctx: &Ctx, controls: Vec<ContinuousId>) -> Self {
        let mode = ParameterMode::new(ctx, true, controls.clone());
        Self::new(Box::new(mode), Mk3Display::Parameters, &controls)
    }

    pub fn display_kind(&self) -> Mk3Display {
        self.display
    }
}

/// The text of the cursor track for the display.
fn cursor_track_text(ctx: &Ctx) -> String {
    let cursor_track = ctx.model.cursor_track();
    if cursor_track.does_exist() {
        format!("{}: {}", cursor_track.position() + 1, cursor_track.name())
    } else {
        "No sel. track".to_string()
    }
}

/// Former `formatPageRange (" %d - %d")` of the track modes (the bank is the current track bank).
fn track_page_range(ctx: &Ctx) -> String {
    let first = ModeBank::TrackBank.scroll_position(ctx);
    if first < 0 {
        return String::new();
    }
    let last = ModeBank::TrackBank.position_of_last_item(ctx);
    format!(" {} - {}", first + 1, last + 1)
}

/// Set a row of the display.
fn set_row(ctx: &mut Ctx, row: usize, text: &str) {
    if let Some(display) = &mut ctx.text_display {
        display.set_cell(row, 0, text);
    }
}

fn all_done(ctx: &mut Ctx) {
    if let Some(display) = &mut ctx.text_display {
        display.all_done();
    }
}

impl Mk3Mode {
    fn update_mk3_display(&self, ctx: &mut Ctx) {
        let row = if self.are_knobs { SCREEN_ROW_POTS } else { SCREEN_ROW_FADERS };
        let mut rows: Vec<(usize, String)> = Vec::with_capacity(20);
        match self.display {
            Mk3Display::Volume | Mk3Display::Pan => {
                let is_volume = self.display == Mk3Display::Volume;
                if self.are_knobs || !is_volume {
                    let title = if is_volume { "Volume" } else { "Pan   " };
                    rows.push((SCREEN_ROW_BASE, format!("{title}{}", track_page_range(ctx))));
                    rows.push((SCREEN_ROW_BASE + 1, cursor_track_text(ctx)));
                }
                // Format track names
                let tb = ctx.model.current_track_bank();
                for i in 0..8 {
                    let t = tb.item(i);
                    let exists = t.does_exist();
                    let offset = i * 2;
                    rows.push((row + offset, if exists { format!("{}: {}", t.position() + 1, t.name()) } else { "No track".to_string() }));
                    let value = if !exists {
                        String::new()
                    } else if is_volume {
                        format!("Vol: {}", t.volume_str())
                    } else {
                        format!("Pan: {}", t.pan_str())
                    };
                    rows.push((row + offset + 1, value));
                }
            }
            Mk3Display::Send(send_index) => {
                let track_text = cursor_track_text(ctx);
                let tb = ctx.model.current_track_bank();
                if self.are_knobs {
                    let send_name = tb.edit_send_name(send_index);
                    let send_name = if send_name.is_empty() { format!("Send {}", send_index + 1) } else { send_name };
                    let send_name = optimize_name(&send_name, 6);
                    rows.push((SCREEN_ROW_BASE, format!("{send_name}{}", track_page_range(ctx))));
                    rows.push((SCREEN_ROW_BASE + 1, track_text));
                }
                // Format track names
                for i in 0..8 {
                    let s = tb.item(i).send_bank().item(send_index);
                    let exists = s.does_exist();
                    let offset = i * 2;
                    rows.push((row + offset, if exists { format!("{}: {}", s.position() + 1, s.name()) } else { "No send".to_string() }));
                    rows.push((row + offset + 1, if exists { format!("S-Vol: {}", s.displayed_value()) } else { String::new() }));
                }
            }
            Mk3Display::Parameters => {
                let cursor_device = ctx.model.cursor_device();
                if self.are_knobs {
                    // Display device and current parameter page
                    let text = if cursor_device.does_exist() {
                        let page = cursor_device.parameter_bank().map(|bank| bank.page_bank().selected_item()).unwrap_or_default();
                        format!("{} {page}", cursor_device.name_limited(8))
                    } else {
                        "No device".to_string()
                    };
                    rows.push((SCREEN_ROW_BASE, text));
                    rows.push((SCREEN_ROW_BASE + 1, cursor_track_text(ctx)));
                }
                // Format parameter names
                for i in 0..8 {
                    let offset = i * 2;
                    let parameter = cursor_device.parameter_bank().map(|bank| bank.item(i)).filter(|p| p.does_exist());
                    rows.push((row + offset, parameter.map_or_else(|| "No parameter".to_string(), |p| p.name().to_string())));
                    rows.push((row + offset + 1, parameter.map(|p| p.displayed_value()).unwrap_or_default()));
                }
            }
        }
        // Add master fader
        if !self.are_knobs {
            rows.push((row + 16, "Master".to_string()));
            rows.push((row + 16 + 1, format!("Vol: {}", ctx.model.master_track().volume_str())));
        }
        for (row, text) in rows {
            set_row(ctx, row, &text);
        }
        all_done(ctx);
    }
}

impl Mode for Mk3Mode {
    crate::forward_mode!(inner => name, on_activate, on_deactivate, button_color, button_color_id, on_knob_value,
        knob_value, knob_color, on_knob_touch, is_any_knob_touched, on_button, select_item, selected_item_name,
        select_previous_item_page, select_next_item_page, select_item_page, has_previous_item, has_next_item,
        has_previous_item_page, has_next_item_page);

    fn update_display(&mut self, ctx: &mut Ctx) {
        self.update_mk3_display(ctx);
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        // The parameter mode triggers parameter pages instead of devices
        if self.display == Mk3Display::Parameters {
            self.inner.select_previous_item_page(ctx);
        } else {
            self.inner.select_previous_item(ctx);
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        // The parameter mode triggers parameter pages instead of devices
        if self.display == Mk3Display::Parameters {
            self.inner.select_next_item_page(ctx);
        } else {
            self.inner.select_next_item(ctx);
        }
    }

    crate::impl_as_any!();
}

/// The modes which use the custom modes of the Mk3 (former `CustomMode`): the controls are
/// handled by the device itself.
pub struct CustomMode {
    number: i32,
    name: String,
}

impl CustomMode {
    pub fn new(number: i32) -> Self {
        CustomMode { number, name: format!("Custom{number}") }
    }
}

impl Mode for CustomMode {
    fn name(&self) -> &str {
        &self.name
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        set_row(ctx, SCREEN_ROW_BASE, &format!("Custom mode {}", self.number));
        set_row(ctx, SCREEN_ROW_BASE + 1, "");
        all_done(ctx);
    }

    crate::impl_as_any!();
}

/// The project / track parameters on the user knobs of the Mini (former
/// `LaunckeyMiniMk3ProjectParamsMode`). Previous/next item toggle between project and track
/// parameters, the program change buttons select the parameter page.
pub struct MiniProjectParamsMode {
    pub base: ProjectParamsMode,
}

impl MiniProjectParamsMode {
    pub fn new(ctx: &Ctx) -> Self {
        MiniProjectParamsMode { base: ProjectParamsMode::new(ctx, true, sequential_controls(ContinuousId::DeviceKnob1, 8)).with_alternative_function(|_| false) }
    }

    /// Former `selectPreviousParameterPage` (the `selectPreviousItem` of `ProjectParamsMode`).
    pub fn select_previous_parameter_page(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item(ctx);
    }

    /// Former `selectNextParameterPage`.
    pub fn select_next_parameter_page(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item(ctx);
    }

    /// Former `notifyPage`.
    pub fn notify_page(&self, ctx: &mut Ctx) {
        if self.base.is_project_mode {
            mv_helper::notify_selected_project_parameter_page(ctx);
        } else {
            mv_helper::notify_selected_track_parameter_page(ctx);
        }
    }
}

impl Mode for MiniProjectParamsMode {
    crate::forward_mode!(base => name, on_activate, on_deactivate, on_knob_value, knob_value, on_knob_touch,
        is_any_knob_touched, on_button, select_item, select_previous_item_page, select_next_item_page,
        select_item_page, has_previous_item, has_next_item, has_previous_item_page, has_next_item_page);

    fn selected_item_name(&self, ctx: &Ctx) -> Option<String> {
        let prefix = if self.base.is_project_mode { "Project " } else { "Track " };
        let bank = if self.base.is_project_mode { ParameterBankId::Project } else { ParameterBankId::CursorTrack };
        let page = crate::framework::parameterprovider::parameter_bank(&ctx.model, bank).map(|b| b.page_bank().selected_item()).filter(|p| !p.is_empty());
        Some(match page {
            Some(page) => format!("{prefix}Page: {page}"),
            None => format!("{prefix}Page: None"),
        })
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.toggle_mode(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.toggle_mode(ctx);
    }

    crate::impl_as_any!();
}

/// A track mode of the Mini, which has no alternative function (the Java modes get
/// `() -> false` as `isAlternativeFunction`): previous/next item always select the
/// previous/next track, also with Shift.
pub struct MiniTrackMode {
    inner: Box<dyn Mode>,
}

impl MiniTrackMode {
    pub fn new(inner: impl Mode + 'static) -> Self {
        MiniTrackMode { inner: Box::new(inner) }
    }
}

impl Mode for MiniTrackMode {
    crate::forward_mode!(inner => name, on_activate, on_deactivate, button_color, button_color_id, update_display,
        on_knob_value, knob_value, knob_color, on_knob_touch, is_any_knob_touched, on_button, select_item,
        selected_item_name, select_previous_item_page, select_next_item_page, select_item_page, has_previous_item,
        has_next_item, has_previous_item_page, has_next_item_page);

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        ctx.model.current_track_bank_mut().select_previous_item();
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        ctx.model.current_track_bank_mut().select_next_item();
    }

    crate::impl_as_any!();
}
