// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The mode to add tracks and devices (former `AddTrackMode` and `AddMode`).
//!
//! Divergence: the device favorites of the Java version (the settings "Add Track - Device
//! Favorites", `IDeviceMetadata`) do not exist since the host does not report its device
//! database. The favorite columns 2-8 are empty; as in Java (without a favorite) pressing them
//! adds an empty instrument/audio/effect track and does nothing in the device mode.

use crate::daw::color::ColorEx;
use crate::daw::constants::ChannelType;
use crate::framework::commands::misc::BrowserCommand;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_OFF};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::button_row_index;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::surface as push;
use super::super::{update_display, PushDisplayMode};
use super::color;

/// What to add (former `AddMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddMode {
    Instrument,
    Audio,
    Effect,
    Device,
}

impl AddMode {
    pub fn label(self) -> &'static str {
        match self {
            AddMode::Instrument => "Instrument",
            AddMode::Audio => "Audio",
            AddMode::Effect => "Effect",
            AddMode::Device => "Device",
        }
    }

    pub fn color(self) -> ColorEx {
        match self {
            AddMode::Instrument => ColorEx::YELLOW,
            AddMode::Audio => ColorEx::GREEN,
            AddMode::Effect => ColorEx::BLUE,
            AddMode::Device => ColorEx::ORANGE,
        }
    }

    pub fn channel_type(self) -> ChannelType {
        match self {
            AddMode::Instrument => ChannelType::Instrument,
            AddMode::Audio => ChannelType::Audio,
            AddMode::Effect => ChannelType::Effect,
            AddMode::Device => ChannelType::Unknown,
        }
    }

    /// The button colors (former `buttonColorsHiFirst`, `LoFirst`, `HiSecond`, `LoSecond`).
    fn button_color(self, is_modern: bool, is_hi: bool, is_first_row: bool) -> i32 {
        match (self, is_modern, is_hi, is_first_row) {
            (AddMode::Instrument, true, true, _) => PUSH2_COLOR2_YELLOW_HI,
            (AddMode::Instrument, true, false, _) => PUSH2_COLOR2_YELLOW_LO,
            (AddMode::Audio, true, true, _) => PUSH2_COLOR2_GREEN_HI,
            (AddMode::Audio, true, false, _) => PUSH2_COLOR2_GREEN_LO,
            (AddMode::Effect, true, true, _) => PUSH2_COLOR2_BLUE_HI,
            (AddMode::Effect, true, false, _) => PUSH2_COLOR2_BLUE_LO,
            (AddMode::Device, true, true, _) => PUSH2_COLOR2_AMBER_HI,
            (AddMode::Device, true, false, _) => PUSH2_COLOR2_AMBER_LO,
            (AddMode::Instrument, false, true, true) => PUSH1_COLOR_YELLOW_MD,
            (AddMode::Instrument, false, false, true) => PUSH1_COLOR_YELLOW_LO,
            (AddMode::Audio, false, true, true) => PUSH1_COLOR_GREEN_HI,
            (AddMode::Audio, false, false, true) => PUSH1_COLOR_GREEN_LO,
            (AddMode::Effect, false, true, true) => PUSH1_COLOR_RED_HI,
            (AddMode::Effect, false, false, true) => PUSH1_COLOR_RED_LO,
            (AddMode::Device, false, true, true) => PUSH1_COLOR_ORANGE_HI,
            (AddMode::Device, false, false, true) => PUSH1_COLOR_ORANGE_LO,
            (AddMode::Instrument, false, true, false) => PUSH1_COLOR2_YELLOW_HI,
            (AddMode::Instrument, false, false, false) => PUSH1_COLOR2_YELLOW_LO,
            (AddMode::Audio, false, true, false) => PUSH1_COLOR2_GREEN_HI,
            (AddMode::Audio, false, false, false) => PUSH1_COLOR2_GREEN_LO,
            (AddMode::Effect, false, true, false) => PUSH1_COLOR2_RED_HI,
            (AddMode::Effect, false, false, false) => PUSH1_COLOR2_RED_LO,
            (AddMode::Device, false, true, false) => PUSH1_COLOR2_AMBER_HI,
            (AddMode::Device, false, false, false) => PUSH1_COLOR2_AMBER_LO,
        }
    }
}

const TOP_MENU: [Option<AddMode>; 8] = [Some(AddMode::Instrument), Some(AddMode::Audio), Some(AddMode::Effect), None, Some(AddMode::Device), None, None, None];

const SUB_MENU: [&str; 8] = ["Add Track", "", "", "", "Add Device", "", "", ""];

/// The mode to add tracks and devices (former `AddTrackMode`).
pub struct AddTrackMode {
    add_mode: AddMode,
}

impl Default for AddTrackMode {
    fn default() -> Self {
        Self::new()
    }
}

impl AddTrackMode {
    pub fn new() -> Self {
        AddTrackMode { add_mode: AddMode::Instrument }
    }

    /// Former `setAddMode` (used by the Add Effect command).
    pub fn set_add_mode(&mut self, add_mode: AddMode) {
        self.add_mode = add_mode;
    }

    pub fn add_mode(&self) -> AddMode {
        self.add_mode
    }

    /// The name of the favorite at the index (former `getFavorite`): there are no favorites.
    fn favorite(&self, _index: usize) -> Option<String> {
        None
    }

    fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }

        // Former modeManager.restore (): this mode is in use, therefore later
        ctx.schedule(0, |ctx| ctx.restore_mode());

        let channel_type = self.add_mode.channel_type();
        if index == 0 && self.add_mode == AddMode::Device {
            BrowserCommand::start_browser(ctx, true, false);
            return;
        }

        // There are no favorites (no device metadata)
        let channel_name = if index == 0 { None } else { self.favorite(index - 1) };
        if channel_type == ChannelType::Unknown {
            // Adding a favorite device to the cursor track is not possible without the device
            // metadata
            return;
        }
        ctx.model.track_bank_mut().add_channel(channel_type, channel_name.as_deref(), None, &[]);
    }

    fn on_second_row(&mut self, index: usize, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            if let Some(mode) = TOP_MENU[index] {
                self.add_mode = mode;
            }
        }
    }

    fn lower_menu(&self, index: usize) -> String {
        if index == 0 {
            return if self.add_mode == AddMode::Device { "Browse" } else { "Empty" }.to_string();
        }
        self.favorite(index - 1).map(|f| f.chars().take(13).collect()).unwrap_or_default()
    }
}

impl PushDisplayMode for AddTrackMode {
    fn update_display1(&mut self, _ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_block(1, 0, SUB_MENU[0]);
        display.set_block(1, 2, SUB_MENU[4]);
        for (i, top) in TOP_MENU.iter().enumerate() {
            let lower_menu = self.lower_menu(i);
            display.set_cell(0, i, top.map_or("", |m| m.label()));
            display.set_cell(2, i, if i == 0 { self.add_mode.label() } else { "" });
            display.set_cell(3, i, &lower_menu);
        }
    }

    fn update_display2(&mut self, _ctx: &mut Ctx, display: &mut GraphicDisplay) {
        for (i, top) in TOP_MENU.iter().enumerate() {
            let lower_menu = self.lower_menu(i);
            let (lower_label, lower_menu_color) = if i == 0 { (self.add_mode.label(), Some(self.add_mode.color())) } else { ("", None) };
            let top_label = top.map_or("", |m| m.label());
            let top_color = top.map(|m| m.color());
            display.add_option_element_colored_with_header_selection(SUB_MENU[i], top_label, false, top_color, lower_label, &lower_menu, false, lower_menu_color, false, false);
        }
    }
}

impl Mode for AddTrackMode {
    fn name(&self) -> &str {
        "Add Track"
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let is_modern = push::is_modern(ctx);
        if let Some(index) = button_row_index(0, button) {
            return self.add_mode.button_color(is_modern, index == 0, true);
        }
        if let Some(index) = button_row_index(1, button) {
            return match TOP_MENU[index] {
                None => color(ctx, BUTTON_COLOR_OFF),
                Some(mode) => mode.button_color(is_modern, mode == self.add_mode, false),
            };
        }
        color(ctx, BUTTON_COLOR_OFF)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(index, event);
        }
    }

    impl_as_any!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_modes() {
        assert_eq!(AddMode::Device.channel_type(), ChannelType::Unknown);
        assert_eq!(AddMode::Effect.button_color(false, true, true), PUSH1_COLOR_RED_HI);
        assert_eq!(AddMode::Effect.button_color(true, false, false), PUSH2_COLOR2_BLUE_LO);
        assert_eq!(AddMode::Audio.button_color(false, true, false), PUSH1_COLOR2_GREEN_HI);
    }
}
