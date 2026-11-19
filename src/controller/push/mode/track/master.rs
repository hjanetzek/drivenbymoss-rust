// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The master mode (former `MasterMode`), registered as MASTER and MASTER_TEMP.

use std::rc::Rc;

use crate::config::push::LockState;
use crate::daw::color::ColorEx;
use crate::daw::constants::{Capability, ChannelType};
use crate::daw::parameter::Parameter;
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::{Mode, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::{button_row_index, default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, CustomParameter, FixedParameterProvider, ParamRef};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::surface as push;
use super::super::{check_stop_automation_on_knob_release, update_display, PushDisplayMode};
use super::{color, format_volume, host_supports, set_cell_value, trigger_button_later};

const TAG_VOLUME: &str = "Volume";

/// The cue volume or cue mix of the project (former `IProject.getCueVolumeParameter` /
/// `getCueMixParameter`), the generic parameter references do not have them.
struct CueParameter {
    is_mix: bool,
}

impl CueParameter {
    fn get<'a>(&self, ctx: &'a Ctx) -> &'a Parameter {
        let project = ctx.model.project();
        if self.is_mix {
            project.cue_mix_parameter()
        } else {
            project.cue_volume_parameter()
        }
    }

    fn get_mut<'a>(&self, ctx: &'a mut Ctx) -> &'a mut Parameter {
        let project = ctx.model.project_mut();
        if self.is_mix {
            project.cue_mix_parameter_mut()
        } else {
            project.cue_volume_parameter_mut()
        }
    }
}

impl CustomParameter for CueParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool {
        self.get(ctx).does_exist()
    }

    fn name(&self, ctx: &Ctx) -> String {
        self.get(ctx).name().to_string()
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        self.get(ctx).value()
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        self.get(ctx).displayed_value()
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        self.get_mut(ctx).set_value(value);
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        self.get_mut(ctx).set_normalized_value(value);
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        if self.is_mix {
            ctx.model.project_mut().change_cue_mix(control);
        } else {
            ctx.model.project_mut().change_cue_volume(control);
        }
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        self.get_mut(ctx).reset_value();
    }

    fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        self.get_mut(ctx).touch_value(is_touched);
    }
}

/// The master mode (former `MasterMode`).
pub struct MasterMode {
    pub base: ParameterModeBase,
    /// Former constructor parameter `isTemporary` (unused in Java as well).
    pub is_temporary: bool,
}

impl MasterMode {
    pub fn new(ctx: &Ctx, is_temporary: bool) -> Self {
        let mut base = ParameterModeBase::new("Master", true, ModeBank::None, default_knob_ids());
        let parameters = vec![
            ParamRef::MasterVolume,
            ParamRef::MasterPan,
            ParamRef::Custom(Rc::new(CueParameter { is_mix: false })),
            ParamRef::Custom(Rc::new(CueParameter { is_mix: true })),
            ParamRef::Empty,
            ParamRef::Empty,
            ParamRef::Empty,
            ParamRef::Empty,
        ];
        base.set_parameter_provider(ctx, provider(FixedParameterProvider::new(parameters)));
        MasterMode { base, is_temporary }
    }

    /// Former `setActive`: the indication of the master volume and panorama.
    fn set_active(ctx: &mut Ctx, enable: bool) {
        let master = ctx.model.master_track_mut();
        master.set_volume_indication(enable);
        master.set_pan_indication(enable);
    }

    fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Record) {
            ctx.surface.set_trigger_consumed(ButtonId::Record);
            ctx.model.master_track_mut().toggle_rec_arm();
            return;
        }
        match index {
            0 => trigger_button_later(ctx, ButtonId::Device),
            4 => ctx.model.application_mut().toggle_engine_active(),
            6 => ctx.model.project_mut().previous(),
            7 => ctx.model.project_mut().next(),
            _ => {}
        }
    }

    fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match index {
            0 => {
                if !push::is_modern(ctx) {
                    if push::lock_state(ctx) == LockState::Mute {
                        ctx.model.master_track_mut().toggle_mute();
                    } else {
                        ctx.model.master_track_mut().toggle_solo();
                    }
                }
            }
            6 => ctx.model.project_mut().load(),
            7 => ctx.model.project_mut().save(),
            _ => {}
        }
    }

    /// Former `getTrackButtonColor`.
    fn track_button_color(ctx: &Ctx) -> i32 {
        let is_modern = push::is_modern(ctx);
        let master = ctx.model.master_track();
        if !master.is_activated() {
            return if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };
        }
        if master.is_rec_arm() {
            return if is_modern { PUSH2_COLOR_RED_HI } else { PUSH1_COLOR_RED_HI };
        }
        if is_modern {
            PUSH2_COLOR_ORANGE_HI
        } else {
            PUSH1_COLOR_ORANGE_HI
        }
    }
}

impl PushDisplayMode for MasterMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let ctx = &*ctx;
        let can_edit_cue_volume = host_supports(ctx, Capability::CueVolume);
        let master = ctx.model.master_track();
        let project = ctx.model.project();

        display.set_cell(0, 0, TAG_VOLUME).set_cell(0, 1, "Pan");
        if can_edit_cue_volume {
            display.set_cell(0, 2, TAG_VOLUME).set_cell(0, 3, "Mix");
        }
        display.set_cell(0, 6, "Load").set_cell(0, 7, "Save").set_cell(1, 6, "Project:");
        display.set_cell(1, 0, &master.volume_str_limited(8)).set_cell(1, 1, &master.pan_str_limited(8));
        if can_edit_cue_volume {
            display.set_cell(1, 2, &project.cue_volume_str_limited(8)).set_cell(1, 3, &project.cue_mix_str_limited(8));
        }
        display.set_block(1, 2, "Audio Engine").set_block(2, 3, project.name());
        display.set_cell(2, 0, &format_volume(ctx, master.volume(), master.vu()));
        set_cell_value(ctx, display, 2, 1, master.pan(), Format::Pan);
        if can_edit_cue_volume {
            set_cell_value(ctx, display, 2, 2, project.cue_volume(), Format::Value);
            set_cell_value(ctx, display, 2, 3, project.cue_mix(), Format::Value);
            display.set_cell(3, 0, master.name()).set_cell(3, 2, "Cue");
        }
        display.set_cell(3, 4, if ctx.model.application().is_engine_active() { "Active" } else { "Off" });
        display.set_cell(3, 6, "Previous").set_cell(3, 7, "Next");
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let ctx = &*ctx;
        let vc = ctx.model.value_changer();
        let master = ctx.model.master_track();
        let project = ctx.model.project();
        let enable_vu_meters = ctx.config.is_enable_vu_meters();
        let vu_r = vc.to_display_value(if enable_vu_meters { master.vu_right() } else { 0 });
        let vu_l = vc.to_display_value(if enable_vu_meters { master.vu_left() } else { 0 });
        let is_pinned = ctx.model.is_cursor_track_pinned();

        display.add_channel_element(
            TAG_VOLUME,
            false,
            master.name(),
            Some(ChannelType::Master),
            master.color(),
            master.is_selected(),
            vc.to_display_value(master.volume()),
            vc.to_display_value(master.modulated_volume()),
            &if self.base.is_knob_touched(0) { master.volume_str_limited(8) } else { String::new() },
            vc.to_display_value(master.pan()),
            vc.to_display_value(master.modulated_pan()),
            &if self.base.is_knob_touched(1) { master.pan_str_limited(8) } else { String::new() },
            vu_l,
            vu_r,
            master.is_mute(),
            master.is_solo(),
            master.is_rec_arm(),
            master.is_activated(),
            0,
            master.is_selected() && is_pinned,
        );
        display.add_channel_selector_element("Pan", false, "", None, ColorEx::BLACK, false, master.is_activated());

        if host_supports(ctx, Capability::CueVolume) {
            let cue_volume = vc.to_display_value(project.cue_volume());
            let cue_mix = vc.to_display_value(project.cue_mix());
            display.add_parameter_element_plain("Cue Volume", cue_volume, &project.cue_volume_str_limited(8), true, cue_volume);
            display.add_parameter_element_plain("Cue Mix", cue_mix, &project.cue_mix_str_limited(8), true, cue_mix);
        } else {
            display.add_option_element("", " ", false, "", "", false, true);
            display.add_option_element("", " ", false, "", "", false, true);
        }

        display.add_option_element("", " ", false, "Audio Engine", if ctx.model.application().is_engine_active() { "Active" } else { "Off" }, false, true);
        display.add_option_element("", " ", false, "", "", false, true);
        display.add_option_element("Project:", "Load", false, project.name(), "Previous", false, true);
        display.add_option_element("", "Save", false, "", "Next", false, true);
    }
}

impl Mode for MasterMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
        Self::set_active(ctx, true);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
        Self::set_active(ctx, false);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let is_modern = push::is_modern(ctx);
        if let Some(index) = button_row_index(0, button) {
            if index == 0 {
                return Self::track_button_color(ctx);
            }
            if index < 4 || index == 5 {
                return color(ctx, BUTTON_COLOR_OFF);
            }
            if index > 5 {
                return color(ctx, BUTTON_COLOR_ON);
            }
            let red = if is_modern { PUSH2_COLOR_RED_HI } else { PUSH1_COLOR_RED_HI };
            return if ctx.model.application().is_engine_active() { color(ctx, BUTTON_COLOR_ON) } else { red };
        }

        if let Some(index) = button_row_index(1, button) {
            let off = if is_modern { PUSH2_COLOR_BLACK } else { PUSH1_COLOR_BLACK };
            let master = ctx.model.master_track();
            match index {
                0 => {
                    if !is_modern {
                        if push::lock_state(ctx) == LockState::Mute {
                            return if master.is_mute() { off } else { PUSH1_COLOR2_YELLOW_HI };
                        }
                        return if master.is_solo() { PUSH1_COLOR2_BLUE_HI } else { PUSH1_COLOR2_GREY_LO };
                    }
                }
                6 => return if is_modern { PUSH2_COLOR_GREEN_HI } else { PUSH1_COLOR_GREEN_HI },
                7 => {
                    if ctx.model.project().is_dirty() {
                        return if is_modern { PUSH2_COLOR_ORANGE_HI } else { PUSH1_COLOR_ORANGE_HI };
                    }
                    return if is_modern { PUSH2_COLOR_GREEN_LO } else { PUSH1_COLOR_GREEN_LO };
                }
                _ => {}
            }
            return off;
        }

        color(ctx, BUTTON_COLOR_OFF)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);

        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            match index {
                0 => ctx.model.master_track_mut().reset_volume(),
                1 => ctx.model.master_track_mut().reset_pan(),
                2 => ctx.model.project_mut().reset_cue_volume(),
                3 => ctx.model.project_mut().reset_cue_mix(),
                _ => {}
            }
        }

        match index {
            0 => ctx.model.master_track_mut().touch_volume(is_touched),
            1 => ctx.model.master_track_mut().touch_pan(is_touched),
            2 => ctx.model.project_mut().touch_cue_volume(is_touched),
            3 => ctx.model.project_mut().touch_cue_mix(is_touched),
            _ => {}
        }

        check_stop_automation_on_knob_release(ctx, is_touched);
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    impl_as_any!();
}
