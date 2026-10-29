// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration of the play view (former `PlayConfigurationMode`): scale, base, layout,
//! in-key and octave.

use super::{with_display, BaseMode};
use crate::controller::maschine::mk::surface::maschine;
use crate::controller::maschine::mk::view::play_view::update_scale_properties;
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::sequencer::abstract_sequencer_view::with_note_editor;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::{Scale, ScaleLayout, Scales};

/// Edit the scale settings with the knobs.
pub struct PlayConfigurationMode {
    base: BaseMode,
}

impl PlayConfigurationMode {
    pub fn new(_ctx: &Ctx) -> Self {
        PlayConfigurationMode { base: BaseMode::new(0) }
    }

    /// Update the note mapping of the play view and store the scale settings (former
    /// `update`).
    fn update(ctx: &mut Ctx) {
        if let Some(view) = ctx.views.get(Views::Play) {
            if let Ok(mut view) = view.try_borrow_mut() {
                view.update_note_mapping(ctx);
            }
        }
        update_scale_properties(ctx);
    }

    fn with_play_view(ctx: &mut Ctx, f: impl FnOnce(&mut dyn crate::framework::view::transpose_view::TransposeView, &mut Ctx)) {
        if let Some(view) = ctx.views.get(Views::Play) {
            if let Ok(mut view) = view.try_borrow_mut() {
                if let Some(transpose) = view.as_transpose_view_mut() {
                    f(transpose, ctx);
                }
            }
        }
    }
}

impl Mode for PlayConfigurationMode {
    fn name(&self) -> &str {
        "Play Configuration"
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let idx = self.base.index(index);
        let inc = ctx.model.value_changer().is_increase(value);
        match idx {
            0..=2 => {
                if inc {
                    ctx.model.scales_mut().next_scale();
                } else {
                    ctx.model.scales_mut().prev_scale();
                }
                if !maschine(ctx).has_mcu_display() {
                    let name = ctx.model.scales().scale().name();
                    ctx.notify(name);
                }
            }
            3 => {
                if inc {
                    ctx.model.scales_mut().next_scale_offset();
                } else {
                    ctx.model.scales_mut().prev_scale_offset();
                }
            }
            4 | 5 => {
                if inc {
                    ctx.model.scales_mut().next_scale_layout();
                } else {
                    ctx.model.scales_mut().prev_scale_layout();
                }
            }
            6 => {
                ctx.model.scales_mut().set_chromatic(!inc);
                let is_chromatic = ctx.model.scales().is_chromatic();
                ctx.config.set_scale_in_key(!is_chromatic);
            }
            7 => {
                Self::with_play_view(ctx, |view, ctx| {
                    if inc {
                        view.on_octave_up(ctx, ButtonEvent::Down);
                    } else {
                        view.on_octave_down(ctx, ButtonEvent::Down);
                    }
                });
                with_note_editor(ctx, |editor| editor.clear_notes());
            }
            _ => {}
        }
        Self::update(ctx);
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if !is_touched || !ctx.surface.is_delete_pressed() {
            return;
        }
        ctx.surface.set_trigger_consumed(ButtonId::Delete);
        let idx = self.base.index(index);
        match idx {
            0..=2 => {
                ctx.model.scales_mut().set_scale(Scale::Major);
                if !maschine(ctx).has_mcu_display() {
                    let name = ctx.model.scales().scale().name();
                    ctx.notify(name);
                }
            }
            3 => ctx.model.scales_mut().set_scale_offset_by_index(0),
            4 | 5 => ctx.model.scales_mut().set_scale_layout(ScaleLayout::SequentUp),
            6 => {
                ctx.model.scales_mut().set_chromatic(false);
                let is_chromatic = ctx.model.scales().is_chromatic();
                ctx.config.set_scale_in_key(!is_chromatic);
            }
            7 => Self::with_play_view(ctx, |view, ctx| view.reset_octave(ctx)),
            _ => {}
        }
        Self::update(ctx);
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let scales = ctx.model.scales();
        let scale = scales.scale().name().to_string();
        let base_name = Scales::BASES[scales.scale_offset_index()].to_string();
        let layout = optimize_name(scales.scale_layout().name(), 12);
        let in_key = if scales.is_chromatic() { "Off" } else { "On" };
        let octave = scales.octave();
        let octave = format!("{}{octave}", if octave > 0 { "+" } else { "" });
        let base = self.base;
        with_display(ctx, |d| {
            d.clear();
            d.set_block(0, 0, &base.mark("Scale", 0)).set_block(1, 0, &scale);
            d.set_cell(0, 3, &base.mark("Base", 3)).set_cell(1, 3, &base_name);
            d.set_block(0, 2, &base.mark("Layout", 4)).set_block(1, 2, &layout);
            d.set_cell(0, 6, &base.mark("In-Key", 6)).set_cell(1, 6, in_key);
            d.set_cell(0, 7, &base.mark("Octave", 7)).set_cell(1, 7, &octave);
            d.all_done();
        });
    }

    fn select_previous_item(&mut self, _ctx: &mut Ctx) {
        self.base.select_previous_item();
        if self.base.selected_param == 1 || self.base.selected_param == 2 {
            self.base.selected_param = 0;
        } else if self.base.selected_param == 5 {
            self.base.selected_param = 4;
        }
    }

    fn select_next_item(&mut self, _ctx: &mut Ctx) {
        self.base.select_next_item();
        if self.base.selected_param == 1 || self.base.selected_param == 2 {
            self.base.selected_param = 3;
        } else if self.base.selected_param == 5 {
            self.base.selected_param = 6;
        }
    }

    fn select_item(&mut self, _ctx: &mut Ctx, index: usize) {
        self.base.select_item(index);
    }

    impl_as_any!();
}

