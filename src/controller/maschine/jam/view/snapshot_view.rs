// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The snapshot view of the Maschine JAM (doc/HOST_BRIDGE_GUIDE.md §4.14; not part of Java):
//! the 8 x 8 pads are the slots 0-63 of the host's snapshots, from the top left in rows.
//! Select + SONG opens and closes it ([`toggle`]); any other view button leaves it.
//!
//! - Pad: load the snapshot. Shift + pad: store into it (an empty pad with appendOnly: a new
//!   one). Clear + pad: remove it (no confirmation).
//! - Colours: empty off, stored the snapshot's colour dimmed (dim white without one), active
//!   bright green, pressed white; Shift held: the store targets dim red (appendOnly: only the
//!   one at count); Clear held: the occupied pads dim red.

use crate::controller::maschine::core::colors::{dim_or_highlight_color, COLOR_BLACK, COLOR_DARK_GREY, COLOR_GREEN, COLOR_RED_LO, COLOR_WHITE};
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::snapshot_grid::{SnapshotGrid, SnapshotModifier, SnapshotPad};
use crate::impl_as_any;
use crate::protocol::ControllerViewKind;

use super::super::surface as jam;

/// The lowest note of the grid (the bottom left pad).
const FIRST_NOTE: i32 = 36;

/// The slot of a grid note (0 top left, 63 bottom right), None outside of the grid.
pub fn slot_of_note(note: i32) -> Option<usize> {
    let index = usize::try_from(note - FIRST_NOTE).ok().filter(|&index| index < 64)?;
    Some((7 - index / 8) * 8 + index % 8)
}

/// The grid note of a slot.
pub fn note_of_slot(slot: usize) -> i32 {
    FIRST_NOTE + ((7 - slot / 8) * 8 + slot % 8) as i32
}

/// The JAM's colour of a pad state.
pub fn pad_color(ctx: &Ctx, pad: SnapshotPad) -> i32 {
    match pad {
        SnapshotPad::Off => COLOR_BLACK,
        SnapshotPad::Pressed => COLOR_WHITE,
        SnapshotPad::StoreTarget | SnapshotPad::RemoveTarget => COLOR_RED_LO,
        SnapshotPad::Active => COLOR_GREEN,
        SnapshotPad::Stored(None) => COLOR_DARK_GREY,
        SnapshotPad::Stored(Some(color)) => dim_or_highlight_color(&ctx.colors, ColorEx::from_array(color), false),
    }
}

/// The modifier held on the JAM: Shift stores, Clear removes.
fn modifier(ctx: &Ctx) -> SnapshotModifier {
    if ctx.surface.is_shift_pressed() {
        SnapshotModifier::Store
    } else if ctx.surface.is_pressed(ButtonId::Delete) {
        SnapshotModifier::Remove
    } else {
        SnapshotModifier::None
    }
}

/// The snapshot view.
pub struct SnapshotView {
    grid: SnapshotGrid,
}

impl SnapshotView {
    pub fn new(_ctx: &Ctx) -> Self {
        SnapshotView { grid: SnapshotGrid::new() }
    }
}

impl View for SnapshotView {
    fn name(&self) -> &str {
        "Snapshots"
    }

    fn on_deactivate(&mut self, _ctx: &mut Ctx) {
        self.grid.release_all();
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let modifier = modifier(ctx);
        let colors: Vec<(i32, i32)> = (0..64).map(|slot| (note_of_slot(slot), pad_color(ctx, self.grid.pad(ctx, slot, modifier)))).collect();
        let grid = ctx.surface.grid();
        for (note, color) in colors {
            grid.light(note, color);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let Some(slot) = slot_of_note(note) else {
            return;
        };
        let modifier = modifier(ctx);
        if self.grid.on_pad(ctx, slot, modifier, velocity > 0) {
            // The modifier did its work: no Shift view on its release, no Clear function
            match modifier {
                SnapshotModifier::Store => ctx.surface.set_trigger_consumed(ButtonId::Shift),
                SnapshotModifier::Remove => ctx.surface.set_trigger_consumed(ButtonId::Delete),
                SnapshotModifier::None => {}
            }
        }
    }

    impl_as_any!();
}

/// Select + SONG: open the snapshot view (and tell the host), or return to the view before.
pub fn toggle(ctx: &mut Ctx) {
    if ctx.views.active_id_ignore_temporary() == Some(Views::Snapshots) {
        let previous = jam::ext_mut(ctx).snapshot_previous.take().filter(|&view| view != Views::Snapshots);
        ctx.set_active_view(previous.unwrap_or(Views::Session));
        ctx.report_active_controller_view();
        let name = ctx.active_view().and_then(|view| view.try_borrow().ok().map(|view| view.name().to_string())).unwrap_or_default();
        ctx.notify(&name);
        return;
    }
    // The page surface goes first (back to its view and mode before)
    super::super::surface_link::leave(ctx);
    jam::ext_mut(ctx).snapshot_previous = ctx.views.active_id_ignore_temporary();
    ctx.set_active_view(Views::Snapshots);
    ctx.report_controller_view(ControllerViewKind::Snapshots);
    SnapshotGrid::notify_entered(ctx);
}
