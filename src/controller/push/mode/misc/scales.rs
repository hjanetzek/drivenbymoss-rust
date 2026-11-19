// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The scale and scale layout modes (former `ScalesMode` and `ScaleLayoutMode`).

use super::{forward_base, row_index};
use crate::controller::push::colors::{PUSH1_COLOR2_AMBER, PUSH2_COLOR2_AMBER, PUSH_ORANGE_LO};
use crate::controller::push::display1::{create_menu_list, SELECT_ARROW};
use crate::controller::push::mode::{BaseModeState, PushDisplayMode};
use crate::controller::push::surface;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR2_HI, BUTTON_COLOR2_ON, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::framework::setup_helpers::update_scale_properties;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::{Scale, ScaleLayout, Scales};

fn scale_names() -> Vec<String> {
    Scale::names().iter().map(|n| n.to_string()).collect()
}

fn select_arrow(selected: bool) -> String {
    if selected {
        SELECT_ARROW.to_string()
    } else {
        " ".to_string()
    }
}

/// Former `ScalesMode.update`.
fn update(ctx: &mut Ctx) {
    ctx.update_view_note_mapping();
    update_scale_properties(ctx);
}

/// Select the scale (knob 1, first button), the base note (buttons) and chromatic/in key.
pub struct ScalesMode {
    base: ParameterModeBase,
    state: BaseModeState,
}

impl ScalesMode {
    pub fn new() -> Self {
        ScalesMode { base: super::base("Scale"), state: BaseModeState::default() }
    }
}

impl Default for ScalesMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for ScalesMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let scales = ctx.model.scales();
        let sel_index = scales.scale().ordinal() as i32;
        for (pos, (name, selected)) in create_menu_list(4, &scale_names(), sel_index).into_iter().enumerate() {
            display.set_block(pos, 0, &format!("{}{name}", select_arrow(selected)));
        }
        display.set_block(0, 3, &scales.range_text());
        let offset = scales.scale_offset_index();
        for i in 0..6 {
            display.set_cell(2, i + 1, &format!("  {}{}", select_arrow(offset == i), Scales::BASES[i]));
            display.set_cell(3, i + 1, &format!("  {}{}", select_arrow(offset == 6 + i), Scales::BASES[6 + i]));
        }
        display.set_cell(3, 7, if scales.is_chromatic() { "Chromatc" } else { "In Key" });
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let scales = ctx.model.scales();
        let sel_index = scales.scale().ordinal() as i32;
        display.add_list_element_sized(6, &scale_names(), sel_index);
        let offset = scales.scale_offset_index();
        let range_text = scales.range_text();
        for i in 0..6 {
            let header = if i == 3 { format!("Note range: {range_text}") } else { String::new() };
            display.add_option_element(&header, Scales::BASES[6 + i], offset == 6 + i, "", Scales::BASES[i], offset == i, false);
        }
        let is_chromatic = scales.is_chromatic();
        display.add_option_element("", if is_chromatic { "Chromatc" } else { "In Key" }, is_chromatic, "", "", false, false);
    }
}

impl Mode for ScalesMode {
    forward_base!();

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if index != 0 || !self.state.increase_knob_movement() {
            return;
        }
        ctx.model.scales_mut().change_scale(value);
        update(ctx);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let is_modern = surface::is_modern(ctx);
        let scales = ctx.model.scales_mut();
        if row == 0 {
            if index == 0 {
                if is_modern {
                    scales.next_scale();
                } else {
                    scales.prev_scale();
                }
            } else if index > 0 && index < 7 {
                scales.set_scale_offset_by_index(index as i32 - 1);
            }
        } else {
            match index {
                0 => {
                    if is_modern {
                        scales.prev_scale();
                    } else {
                        scales.next_scale();
                    }
                }
                7 => scales.toggle_chromatic(),
                _ => scales.set_scale_offset_by_index(index as i32 + 5),
            }
        }
        update(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if let Some(index) = row_index(0, button) {
            if index == 0 {
                return ctx.colors.color_index(PUSH_ORANGE_LO);
            }
            if index == 7 {
                return ctx.colors.color_index(BUTTON_COLOR_OFF);
            }
            let offset = ctx.model.scales().scale_offset_index();
            return ctx.colors.color_index(if offset == index - 1 { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON });
        }
        if let Some(index) = row_index(1, button) {
            if index == 0 || index == 7 {
                return if surface::is_modern(ctx) { PUSH2_COLOR2_AMBER } else { PUSH1_COLOR2_AMBER };
            }
            let offset = ctx.model.scales().scale_offset_index();
            return ctx.colors.color_index(if offset == index - 1 + 6 { BUTTON_COLOR2_HI } else { BUTTON_COLOR2_ON });
        }
        ctx.colors.color_index(BUTTON_COLOR_OFF)
    }

    impl_as_any!();
}

/// Select the layout of the scale grid.
pub struct ScaleLayoutMode {
    base: ParameterModeBase,
}

impl ScaleLayoutMode {
    pub fn new() -> Self {
        ScaleLayoutMode { base: super::base("Scale Layout") }
    }
}

impl Default for ScaleLayoutMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for ScaleLayoutMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let sl = ctx.model.scales().scale_layout().ordinal();
        let pos = sl / 2;
        let names = ScaleLayout::names();
        display.set_block(1, 0, "Scale layout:");
        for i in (0..names.len()).step_by(2) {
            display.set_cell(3, i / 2, &format!("{}{}", select_arrow(pos == i / 2), names[i].replace(" ^", "")));
        }
        display.set_cell(3, 7, if sl.is_multiple_of(2) { "Horizontal" } else { "Vertical" });
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let sl = ctx.model.scales().scale_layout().ordinal();
        let pos = sl / 2;
        let names = ScaleLayout::names();
        for i in (0..names.len()).step_by(2) {
            display.add_option_element("", "", false, if i == 0 { "Scale layout" } else { "" }, &names[i].replace(" ^", ""), pos == i / 2, false);
        }
        display.add_option_element("", "", false, "", "", false, false);
        display.add_option_element("", "", false, "", if sl.is_multiple_of(2) { "Horizontal" } else { "Vertical" }, false, false);
    }
}

impl Mode for ScaleLayoutMode {
    forward_base!();

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 || event != ButtonEvent::Up {
            return;
        }
        let sl = ctx.model.scales().scale_layout().ordinal();
        let names = ScaleLayout::names();
        let name = if index < names.len() / 2 {
            names[index * 2 + sl % 2]
        } else if index == 7 {
            names[sl / 2 * 2 + if sl.is_multiple_of(2) { 1 } else { 0 }]
        } else {
            return;
        };
        if let Some(layout) = ScaleLayout::by_name(name) {
            ctx.model.scales_mut().set_scale_layout(layout);
        }
        ctx.update_view_note_mapping();
        let layout_name = ctx.model.scales().scale_layout().name();
        ctx.config.set_scale_layout(layout_name);
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            let pos = ctx.model.scales().scale_layout().ordinal() / 2;
            if index < ScaleLayout::names().len() / 2 {
                return if pos == index { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
            }
            return if index == 7 { BUTTON_COLOR_ON } else { BUTTON_COLOR_OFF };
        }
        BUTTON_COLOR_OFF
    }

    impl_as_any!();
}
