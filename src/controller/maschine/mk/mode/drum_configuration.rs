// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration of the drum view (former `DrumConfigurationMode`): the drum offset.

use super::{with_display, BaseMode};
use crate::controller::maschine::mk::view::drum_view::DrumView;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::sequencer::abstract_sequencer_view::with_note_editor;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// Edit the drum offset with the knobs.
pub struct DrumConfigurationMode {
    base: BaseMode,
}

impl DrumConfigurationMode {
    pub fn new(_ctx: &Ctx) -> Self {
        DrumConfigurationMode { base: BaseMode::new(6) }
    }

    fn with_drum_view(ctx: &mut Ctx, f: impl FnOnce(&mut DrumView, &mut Ctx)) {
        if let Some(view) = ctx.views.get(Views::Drum) {
            if let Ok(mut view) = view.try_borrow_mut() {
                if let Some(drum_view) = view.as_any_mut().downcast_mut::<DrumView>() {
                    f(drum_view, ctx);
                }
            }
        }
    }
}

impl Mode for DrumConfigurationMode {
    fn name(&self) -> &str {
        "Drum Configuration"
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let idx = self.base.index(index);
        let inc = ctx.model.value_changer().is_increase(value);
        if idx == 6 || idx == 7 {
            let offset = ctx.model.scales().drum_default_offset();
            Self::with_drum_view(ctx, |view, ctx| {
                view.change_octave(ctx, ButtonEvent::Down, inc, offset, true, false);
                crate::framework::feature::View::update_note_mapping(view, ctx);
            });
            with_note_editor(ctx, |editor| editor.clear_notes());
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if !is_touched || !ctx.surface.is_delete_pressed() {
            return;
        }
        ctx.surface.set_trigger_consumed(ButtonId::Delete);
        let idx = self.base.index(index);
        if idx == 6 || idx == 7 {
            Self::with_drum_view(ctx, |view, ctx| {
                crate::framework::view::transpose_view::TransposeView::reset_octave(view, ctx);
                crate::framework::feature::View::update_note_mapping(view, ctx);
            });
            with_note_editor(ctx, |editor| editor.clear_notes());
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let octave = ctx.model.scales().drum_offset();
        let offset = format!("{}{octave}", if octave > 0 { "+" } else { "" });
        let base = self.base;
        with_display(ctx, |d| {
            d.clear();
            d.set_block(0, 3, &base.mark("Drum Offset", 6)).set_block(1, 3, &offset);
            d.all_done();
        });
    }

    // The selection of the parameter is not supported
    fn select_previous_item(&mut self, _ctx: &mut Ctx) {}

    fn select_next_item(&mut self, _ctx: &mut Ctx) {}

    fn select_item(&mut self, _ctx: &mut Ctx, index: usize) {
        self.base.select_item(index);
    }

    impl_as_any!();
}
