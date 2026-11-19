// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The metronome / pre-roll mode (former `MetronomeMode`, registered as TRANSPORT) and the
//! accent mode (former `AccentMode`).

use super::{forward_base, row_index, set_cell_value, touch_only};
use crate::controller::push::display1::SELECT_ARROW;
use crate::controller::push::mode::PushDisplayMode;
use crate::daw::constants::ChannelType;
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::{Mode, BUTTON_COLOR2_HI, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::framework::parameterprovider::{provider, CombinedParameterProvider, EmptyParameterProvider, FixedParameterProvider, ParamRef};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

const PREROLL_MEASURE: [i32; 4] = [0, 1, 2, 4];
const PREROLL_NAMES: [&str; 4] = ["None", "1 Bar", "2 Bars", "4 Bars"];

/// Pre-roll settings and the metronome volume (knob 8).
pub struct MetronomeMode {
    base: ParameterModeBase,
}

impl MetronomeMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = super::base("Transport");
        let parameters = provider(CombinedParameterProvider::new(vec![provider(EmptyParameterProvider::new(7)), provider(FixedParameterProvider::single(ParamRef::MetronomeVolume))]));
        base.set_parameter_provider(ctx, parameters);
        MetronomeMode { base }
    }
}

impl PushDisplayMode for MetronomeMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_block(2, 0, "Pre-roll");
        let transport = ctx.model.transport();
        let preroll_measures = transport.preroll_measures();
        for (i, measure) in PREROLL_MEASURE.iter().enumerate() {
            let arrow = if *measure == preroll_measures { SELECT_ARROW.to_string() } else { String::new() };
            display.set_cell(3, i, &format!("{arrow}{}", PREROLL_NAMES[i]));
        }
        display.set_cell(0, 7, "Volume");
        display.set_cell(1, 7, transport.metronome_volume_str());
        let volume = transport.metronome_volume();
        let preroll_metronome = transport.is_preroll_metronome_enabled();
        set_cell_value(ctx, display, 2, 7, volume, Format::Value);
        display.set_block(1, 2, "Play Metronome").set_block(2, 2, "during Pre-roll?");
        display.set_cell(3, 5, if preroll_metronome { "  Yes" } else { "  No" });
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let transport = ctx.model.transport();
        let preroll_measures = transport.preroll_measures();
        for (i, measure) in PREROLL_MEASURE.iter().enumerate() {
            display.add_option_element("", "", false, if i == 0 { "Pre-roll" } else { "" }, PREROLL_NAMES[i], *measure == preroll_measures, false);
        }
        display.add_empty_element();
        let enabled = transport.is_preroll_metronome_enabled();
        display.add_option_element("Play Metronome", "", false, "during Pre-Roll?", if enabled { "Yes" } else { "No" }, enabled, false);
        display.add_empty_element();
        let volume = transport.metronome_volume();
        let volume_str = transport.metronome_volume_str().to_string();
        display.add_parameter_element("", false, "", Some(ChannelType::Audio), crate::daw::color::ColorEx::BLACK, false, "Volume", volume, &volume_str, false, volume);
    }
}

impl Mode for MetronomeMode {
    forward_base!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.base.on_knob_value(ctx, index, value);
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.on_knob_touch(ctx, index, is_touched);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 {
            return;
        }
        if event != ButtonEvent::Up {
            return;
        }
        if let Some(measures) = PREROLL_MEASURE.get(index) {
            ctx.model.transport_mut().set_preroll_measures(*measures);
        } else if index == 5 {
            ctx.model.transport_mut().toggle_preroll_metronome();
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            let transport = ctx.model.transport();
            if let Some(measures) = PREROLL_MEASURE.get(index) {
                return if *measures == transport.preroll_measures() { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
            }
            if index == 5 {
                return if transport.is_preroll_metronome_enabled() { BUTTON_COLOR2_HI } else { BUTTON_COLOR_ON };
            }
        }
        BUTTON_COLOR_OFF
    }

    impl_as_any!();
}

const TAG_ACCENT: &str = "Accent";

/// Edit the fixed accent value with knob 8 (all knobs change it).
pub struct AccentMode {
    base: ParameterModeBase,
}

impl AccentMode {
    pub fn new() -> Self {
        AccentMode { base: super::base(TAG_ACCENT) }
    }
}

impl Default for AccentMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for AccentMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let fixed_accent_value = ctx.config.fixed_accent_value();
        let daw_value = ctx.model.value_changer().to_daw_value(fixed_accent_value);
        display.set_cell(0, 7, TAG_ACCENT).set_cell(1, 7, &fixed_accent_value.to_string());
        set_cell_value(ctx, display, 2, 7, daw_value, Format::Value);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let fixed_accent_value = ctx.config.fixed_accent_value();
        let value_changer = ctx.model.value_changer();
        let value = value_changer.to_display_value(value_changer.to_daw_value(fixed_accent_value));
        for i in 0..8 {
            let is_last = i == 7;
            let text = if is_last { fixed_accent_value.to_string() } else { String::new() };
            display.add_parameter_element_plain(if is_last { TAG_ACCENT } else { "" }, if is_last { value } else { 0 }, &text, self.base.is_knob_touched(i), -1);
        }
    }
}

impl Mode for AccentMode {
    forward_base!();
    touch_only!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, _index: usize, value: i32) {
        // Will never need fine increments on accent velocity since they are integers
        let fixed_accent_value = ctx.config.fixed_accent_value();
        let new_value = ctx.model.value_changer().change_value_with(value, fixed_accent_value, -100.0, 128);
        ctx.config.set_fixed_accent_value(new_value.max(1));
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        // Former BaseMode.onFirstRow: select the track
        if row == 0 {
            crate::controller::push::mode::on_first_row_default(ctx, index, event);
        }
    }

    impl_as_any!();
}

