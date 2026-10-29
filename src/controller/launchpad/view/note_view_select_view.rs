// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Selects one of the play/sequencer views with the pads (former `NoteViewSelectView`).

use crate::controller::launchpad::colors::*;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::base::ViewBase;
use crate::impl_as_any;

/// The pads which select a view and their colors.
const VIEW_PADS: [(i32, Views, i32); 10] = [
    (44, Views::ClipLength, COLOR_VIEW_CLIP),
    (60, Views::Sequencer, COLOR_VIEW_NOTE_SEQUENCER),
    (61, Views::PolySequencer, COLOR_VIEW_NOTE_SEQUENCER),
    (62, Views::Raindrops, COLOR_VIEW_NOTE_SEQUENCER),
    (76, Views::Drum, COLOR_VIEW_DRUM_SEQUENCER),
    (77, Views::Drum4, COLOR_VIEW_DRUM_SEQUENCER),
    (78, Views::Drum8, COLOR_VIEW_DRUM_SEQUENCER),
    (92, Views::Play, COLOR_VIEW_PLAY),
    (93, Views::Piano, COLOR_VIEW_PLAY),
    (94, Views::Drum64, COLOR_VIEW_PLAY),
];

/// The view selection view.
pub struct NoteViewSelectView {
    base: ViewBase,
}

impl NoteViewSelectView {
    pub fn new(_ctx: &Ctx) -> Self {
        NoteViewSelectView { base: ViewBase::new("View select") }
    }
}

impl View for NoteViewSelectView {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let previous_view_id = ctx.views.previous_id();
        let grid = ctx.surface.grid();

        // From bottom to top
        for i in 36..44 {
            grid.light(i, LAUNCHPAD_COLOR_BLACK);
        }
        for i in 44..100 {
            grid.light(i, COLOR_VIEW_OFF);
        }
        for (pad, view, color) in VIEW_PADS {
            grid.light(pad, if previous_view_id == Some(view) { COLOR_VIEW_SELECTED } else { color });
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity != 0 {
            return;
        }
        let Some(&(_, view_id, _)) = VIEW_PADS.iter().find(|(pad, _, _)| *pad == note) else {
            // Not used
            return;
        };
        // This view is borrowed, therefore it is deactivated after the current event
        ctx.schedule(0, move |ctx| {
            ctx.activate_preferred_view(view_id);
            if let Some(view) = ctx.views.get(view_id) {
                let name = view.borrow().name().to_string();
                ctx.notify(&name);
            }
        });
    }

    fn button_color(&self, _ctx: &Ctx, _button: ButtonId) -> i32 {
        COLOR_VIEW_SELECTED
    }

    impl_as_any!();
}
