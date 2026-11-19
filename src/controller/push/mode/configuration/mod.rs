// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration modes of the Push (former `mode.configuration`): `ConfigurationMode` (Push
//! 1), `InfoMode`, `SetupMode`, `MPEConfigurationMode` and `AudioConfigurationMode` (Push 2/3)
//! with their base `AbstractConfigurationMode`, ported as one [`ConfigPageMode`] per page.
//!
//! The modes only change the settings; the setup sends them to the device when a setting
//! changed.

use crate::config::push::{self as config, PushVersion};
use crate::controller::push::mode::misc::{add_parameter, forward_base, row_index, set_temporary_mode_later, touch_only};
use crate::controller::push::mode::PushDisplayMode;
use crate::controller::push::surface;
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::framework::string_utils::pad;
use crate::framework::value_changer::ValueChanger;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Register CONFIGURATION (Push 1) or INFO, SETUP, CONFIGURATION (MPE), AUDIO (Push 2/3).
pub fn register_configuration_modes(ctx: &mut Ctx) {
    let version = surface::version(ctx);
    if version == PushVersion::Version1 {
        ctx.register_mode(Modes::Configuration, ConfigurationMode::new());
    } else {
        ctx.register_mode(Modes::Info, ConfigPageMode::new(ConfigPage::Info, version));
        ctx.register_mode(Modes::Setup, ConfigPageMode::new(ConfigPage::Setup, version));
        ctx.register_mode(Modes::Configuration, ConfigPageMode::new(ConfigPage::Mpe, version));
        ctx.register_mode(Modes::Audio, ConfigPageMode::new(ConfigPage::Audio, version));
    }
}

fn value_changer(ctx: &Ctx) -> ValueChanger {
    ctx.model.value_changer().clone()
}

/// The configuration of the Push 1: pad threshold and velocity curve (former
/// `ConfigurationMode`).
pub struct ConfigurationMode {
    base: ParameterModeBase,
}

impl ConfigurationMode {
    pub fn new() -> Self {
        ConfigurationMode { base: crate::controller::push::mode::misc::base("Configuration") }
    }
}

impl Default for ConfigurationMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for ConfigurationMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_block(0, 0, "Pad Threshold").set_block(1, 0, &surface::selected_pad_threshold(ctx));
        display.set_block(0, 1, "Velocity Curve").set_block(1, 1, surface::selected_velocity_curve(ctx));
        let state = surface::state(ctx);
        display.set_block(0, 3, &format!("Firmware: {}.{}", state.major_version, state.minor_version));
        if config::pad_threshold_push1(&ctx.config) < 20 {
            display.set_row(3, &pad("Low threshold maycause stuck pads!", 68));
        }
    }

    fn update_display2(&mut self, _ctx: &mut Ctx, _display: &mut GraphicDisplay) {
        // Intentionally empty - mode is only for Push 1
    }
}

impl Mode for ConfigurationMode {
    forward_base!();
    touch_only!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let vc = value_changer(ctx);
        match index {
            0 | 1 => config::change_pad_threshold(&mut ctx.config, &vc, value),
            2 | 3 => config::change_velocity_curve(&mut ctx.config, &vc, value),
            _ => {}
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        // Former BaseMode.onFirstRow: select the track
        if row == 0 {
            crate::controller::push::mode::on_first_row_default(ctx, index, event);
        }
    }

    impl_as_any!();
}

/// The pages of the configuration of the Push 2/3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPage {
    /// Former `InfoMode`.
    Info,
    /// Former `SetupMode`.
    Setup,
    /// Former `MPEConfigurationMode` (registered as CONFIGURATION).
    Mpe,
    /// Former `AudioConfigurationMode` (registered as AUDIO).
    Audio,
}

impl ConfigPage {
    /// Former `page`.
    fn index(self) -> usize {
        match self {
            ConfigPage::Info => 0,
            ConfigPage::Setup => 1,
            ConfigPage::Mpe => 2,
            ConfigPage::Audio => 3,
        }
    }

    fn name(self) -> &'static str {
        match self {
            ConfigPage::Info => "Info",
            ConfigPage::Setup => "Setup",
            ConfigPage::Mpe => "MPE Configuration",
            ConfigPage::Audio => "Audio Configuration",
        }
    }
}

const MODES_PUSH2: [Modes; 2] = [Modes::Info, Modes::Setup];
const MODES_PUSH3: [Modes; 4] = [Modes::Info, Modes::Setup, Modes::Configuration, Modes::Audio];

/// A configuration page of the Push 2/3 (former `AbstractConfigurationMode` and its
/// sub-classes).
pub struct ConfigPageMode {
    base: ParameterModeBase,
    page: ConfigPage,
    version: PushVersion,
    menu: [&'static str; 4],
    modes: &'static [Modes],
}

impl ConfigPageMode {
    pub fn new(page: ConfigPage, version: PushVersion) -> Self {
        let (menu, modes): ([&'static str; 4], &'static [Modes]) =
            if version == PushVersion::Version3 { (["Info", "Setup", "MPE", "Audio"], &MODES_PUSH3) } else { (["Info", "Setup", " ", " "], &MODES_PUSH2) };
        ConfigPageMode { base: crate::controller::push::mode::misc::base(page.name()), page, version, menu, modes }
    }

    /// The page of the mode.
    pub fn page(&self) -> ConfigPage {
        self.page
    }

    fn touched(&self, index: usize) -> bool {
        self.base.is_knob_touched(index)
    }

    fn update_info(&self, ctx: &Ctx, display: &mut GraphicDisplay) {
        let state = surface::state(ctx);
        display.add_option_element(&format!("  Firmware: {}.{} Build {}", state.major_version, state.minor_version, state.build_number), self.menu[0], true, "", "", false, true);
        display.add_option_element("", self.menu[1], false, "", "", false, true);
        display.add_option_element("", self.menu[2], false, "", "", false, true);
        display.add_option_element(&format!("Board Revision: {}", state.board_revision), self.menu[3], false, "", "", false, true);
        display.add_empty_element_with_small_menu(true);
        display.add_option_element(&format!("        Serial Number: {}", state.serial_number), " ", false, "", "", false, true);
        display.add_empty_element_with_small_menu(true);
        display.add_empty_element_with_small_menu(true);
    }

    fn update_setup(&self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let c = &ctx.config;
        let display_brightness = config::display_brightness(c);
        let led_brightness = config::led_brightness(c);
        display.add_option_element("Brightness", self.menu[0], false, "", "", false, true);
        add_parameter(display, self.menu[1], true, "", "Display", display_brightness * 1023 / 100, &format!("{display_brightness}%"), self.touched(1), -1);
        add_parameter(display, self.menu[2], false, "", "LEDs", led_brightness * 1023 / 100, &format!("{led_brightness}%"), self.touched(2), -1);
        display.add_option_element("        Pads", self.menu[3], false, "", "", false, true);
        if self.version == PushVersion::Version2 {
            let sensitivity = config::pad_sensitivity_push2(c);
            let gain = config::pad_gain_push2(c);
            let dynamics = config::pad_dynamics_push2(c);
            add_parameter(display, " ", false, "", "Sensitivity", sensitivity * 1023 / 10, &sensitivity.to_string(), self.touched(4), -1);
            add_parameter(display, " ", false, "", "Gain", gain * 1023 / 10, &gain.to_string(), self.touched(5), -1);
            add_parameter(display, " ", false, "", "Dynamics", dynamics * 1023 / 10, &dynamics.to_string(), self.touched(6), -1);
            display.add_option_element("", " ", false, "", "", false, true);
            let curve = surface::create_pad_sensitivity_curve_push2(ctx);
            display.add_graph_overlay(380, 84, 90, 34, ColorEx::WHITE, &curve, 128);
        } else {
            let threshold = config::pad_curve_threshold_push3(c);
            let drive = config::pad_curve_drive_push3(c);
            let compand = config::pad_curve_compand_push3(c);
            let range = config::pad_curve_range_push3(c);
            add_parameter(display, " ", false, "", "Threshold", threshold * 1023 / 100, &threshold.to_string(), self.touched(4), -1);
            add_parameter(display, " ", false, "", "Drive", (drive + 50) * 1023 / 100, &drive.to_string(), self.touched(5), -1);
            add_parameter(display, " ", false, "", "Compand", (compand + 50) * 1023 / 100, &compand.to_string(), self.touched(6), -1);
            add_parameter(display, " ", false, "", "Range", range * 1023 / 100, &range.to_string(), self.touched(7), -1);
            let curve = surface::create_pad_sensitivity_curve_push3(ctx);
            display.add_graph_overlay(380, 84, 90, 34, ColorEx::WHITE, &curve, 128);
        }
    }

    fn update_mpe(&self, ctx: &Ctx, display: &mut GraphicDisplay) {
        display.add_option_element("", self.menu[0], false, "", "", false, true);
        let c = &ctx.config;
        let is_mpe_enabled = c.is_mpe_enabled();
        let range = c.mpe_pitch_bend_range();
        let per_pad = config::is_per_pad_pitchbend(c);
        let location = config::in_tune_location(c);
        let width = config::in_tune_width(c);
        let height = config::in_tune_slide_height(c);
        let on_off = |on: bool| if on { "On" } else { "Off" };
        let scaled = |value: usize, len: usize| value as i32 * 1023 / (len as i32 - 1);
        add_parameter(display, self.menu[1], false, "", "MPE", if is_mpe_enabled { 1023 } else { 0 }, on_off(is_mpe_enabled), self.touched(1), -1);
        // Former host.supports (Capability.MPE_PITCH_RANGE): the setting is always active
        add_parameter(display, self.menu[2], true, "", "PB Range", range * 1023 / 96, &range.to_string(), self.touched(2), -1);
        add_parameter(display, self.menu[3], false, "", "Per-Pad PB", if per_pad { 1023 } else { 0 }, on_off(per_pad), self.touched(3), -1);
        add_parameter(display, " ", false, "", "Location", if location == 0 { 0 } else { 1023 }, config::IN_TUNE_LOCATION_OPTIONS[location], self.touched(4), -1);
        add_parameter(display, " ", false, "", "Width (mm)", scaled(width, config::IN_TUNE_WIDTH_OPTIONS.len()), config::IN_TUNE_WIDTH_OPTIONS[width], self.touched(5), -1);
        add_parameter(display, " ", false, "", "Height (mm)", scaled(height, config::SLIDE_HEIGHT_OPTIONS.len()), config::SLIDE_HEIGHT_OPTIONS[height], self.touched(6), -1);
        display.add_option_element("", " ", false, "", "", false, true);
    }

    fn update_audio(&self, ctx: &Ctx, display: &mut GraphicDisplay) {
        let c = &ctx.config;
        let cv1 = config::pedal1(c);
        let cv2 = config::pedal2(c);
        let type1 = config::preamp1_type(c);
        let type2 = config::preamp2_type(c);
        let gain1 = config::preamp1_gain(c);
        let gain2 = config::preamp2_gain(c);
        let outputs = config::audio_outputs(c);
        let scaled = |value: usize, len: usize| value as i32 * 1023 / (len as i32 - 1);
        add_parameter(display, self.menu[0], false, "", "Pedal&CV 1", if cv1 > 0 { 1023 } else { 0 }, config::FOOT_CV_OPTIONS[cv1], self.touched(0), -1);
        add_parameter(display, self.menu[1], false, "", "Pedal&CV 2", if cv2 > 0 { 1023 } else { 0 }, config::FOOT_CV_OPTIONS[cv2], self.touched(1), -1);
        display.add_option_element("   Preamp", self.menu[2], false, "", "", false, true);
        add_parameter(display, self.menu[3], true, "", "P1: Type", scaled(type1, config::PREAMP_TYPE_OPTIONS.len()), config::PREAMP_TYPE_OPTIONS[type1], self.touched(3), -1);
        add_parameter(display, " ", false, "", "P1: Gain", scaled(gain1, config::PREAMP_GAIN_OPTIONS.len()), config::PREAMP_GAIN_OPTIONS[gain1], self.touched(4), -1);
        add_parameter(display, " ", false, "", "P2: Type", scaled(type2, config::PREAMP_TYPE_OPTIONS.len()), config::PREAMP_TYPE_OPTIONS[type2], self.touched(5), -1);
        add_parameter(display, " ", false, "", "P2: Gain", scaled(gain2, config::PREAMP_GAIN_OPTIONS.len()), config::PREAMP_GAIN_OPTIONS[gain2], self.touched(6), -1);
        add_parameter(display, " ", false, "", "Outputs", scaled(outputs, config::OUTPUT_CONFIGURATION_SHORT.len()), config::OUTPUT_CONFIGURATION_SHORT[outputs], self.touched(7), -1);
    }

    fn on_knob_setup(&self, ctx: &mut Ctx, index: usize, value: i32) {
        let vc = value_changer(ctx);
        let c = &mut ctx.config;
        match index {
            1 => config::change_display_brightness(c, &vc, value),
            2 => config::change_led_brightness(c, &vc, value),
            _ => {}
        }
        if self.version == PushVersion::Version2 {
            match index {
                4 => config::change_pad_sensitivity(c, &vc, value),
                5 => config::change_pad_gain(c, &vc, value),
                6 => config::change_pad_dynamics(c, &vc, value),
                _ => {}
            }
        } else {
            match index {
                4 => config::change_pad_curve_threshold_push3(c, &vc, value),
                5 => config::change_pad_curve_drive_push3(c, &vc, value),
                6 => config::change_pad_curve_compand_push3(c, &vc, value),
                7 => config::change_pad_curve_range_push3(c, &vc, value),
                _ => {}
            }
        }
    }

    fn reset_setup(&self, ctx: &mut Ctx, index: usize) {
        let c = &mut ctx.config;
        match index {
            1 => return config::set_display_brightness(c, 100),
            2 => return config::set_led_brightness(c, 100),
            _ => {}
        }
        if self.version == PushVersion::Version2 {
            match index {
                4 => config::set_pad_sensitivity_push2(c, 5),
                5 => config::set_pad_gain_push2(c, 5),
                6 => config::set_pad_dynamics_push2(c, 5),
                _ => {}
            }
        } else {
            match index {
                4 => config::set_pad_curve_threshold_push3(c, 7),
                5 => config::set_pad_curve_drive_push3(c, 4),
                6 => config::set_pad_curve_compand_push3(c, -26),
                7 => config::set_pad_curve_range_push3(c, 35),
                _ => {}
            }
        }
    }
}

impl PushDisplayMode for ConfigPageMode {
    fn update_display1(&mut self, _ctx: &mut Ctx, _display: &mut TextDisplay) {
        // Intentionally empty - mode is only for Push 2
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        match self.page {
            ConfigPage::Info => self.update_info(ctx, display),
            ConfigPage::Setup => self.update_setup(ctx, display),
            ConfigPage::Mpe => self.update_mpe(ctx, display),
            ConfigPage::Audio => self.update_audio(ctx, display),
        }
    }
}

impl Mode for ConfigPageMode {
    forward_base!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let vc = value_changer(ctx);
        match self.page {
            ConfigPage::Info => {}
            ConfigPage::Setup => self.on_knob_setup(ctx, index, value),
            ConfigPage::Mpe => {
                let c = &mut ctx.config;
                match index {
                    1 => c.change_mpe_enabled(&vc, value),
                    2 => c.change_mpe_pitchbend_range(&vc, value),
                    3 => config::change_per_pad_pitchbend_enabled(c, &vc, value),
                    4 => config::change_in_tune_location(c, &vc, value),
                    5 => config::change_in_tune_width(c, &vc, value),
                    6 => config::change_slide_height(c, &vc, value),
                    _ => {}
                }
            }
            ConfigPage::Audio => {
                let c = &mut ctx.config;
                match index {
                    0 => config::change_pedal1(c, &vc, value),
                    1 => config::change_pedal2(c, &vc, value),
                    3 => config::change_preamp1_type(c, &vc, value),
                    4 => config::change_preamp1_gain(c, &vc, value),
                    5 => config::change_preamp2_type(c, &vc, value),
                    6 => config::change_preamp2_gain(c, &vc, value),
                    7 => config::change_audio_outputs(c, &vc, value),
                    _ => {}
                }
            }
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        if self.page == ConfigPage::Info || !is_touched || !ctx.surface.is_delete_pressed() {
            return;
        }
        ctx.surface.set_trigger_consumed(ButtonId::Delete);
        match self.page {
            ConfigPage::Info => {}
            ConfigPage::Setup => self.reset_setup(ctx, index),
            ConfigPage::Mpe => {
                let c = &mut ctx.config;
                match index {
                    1 => c.set_mpe_enabled(true),
                    2 => c.set_mpe_pitchbend_range(48),
                    3 => config::set_per_pad_pitchbend_enabled(c, true),
                    4 => config::set_in_tune_location(c, 1),
                    5 => config::set_in_tune_width(c, 9),
                    6 => config::set_slide_height(c, 3),
                    _ => {}
                }
            }
            ConfigPage::Audio => {
                let c = &mut ctx.config;
                match index {
                    0 => config::set_pedal1(c, 0),
                    1 => config::set_pedal2(c, 0),
                    3 => config::set_preamp1_type(c, 0),
                    4 => config::set_preamp1_gain(c, 0),
                    5 => config::set_preamp2_type(c, 0),
                    6 => config::set_preamp2_gain(c, 0),
                    7 => config::set_audio_outputs(c, 0),
                    _ => {}
                }
            }
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        // The first row selects the track (former BaseMode.onFirstRow)
        if row == 0 {
            crate::controller::push::mode::on_first_row_default(ctx, index, event);
            return;
        }
        if event == ButtonEvent::Up {
            if let Some(mode) = self.modes.get(index) {
                set_temporary_mode_later(ctx, *mode);
            }
        }
    }

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        match row_index(1, button) {
            Some(index) if index < self.modes.len() => {
                if index == self.page.index() {
                    BUTTON_COLOR_HI
                } else {
                    BUTTON_COLOR_ON
                }
            }
            _ => BUTTON_COLOR_OFF,
        }
    }

    impl_as_any!();
}
