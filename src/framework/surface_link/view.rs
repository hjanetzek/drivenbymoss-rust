// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The generic surface view (the grid) and mode (the encoders) which any grid controller can
//! use. The controller supplies a [`SurfaceMapping`] (grid offsets, row and page selectors,
//! encoders), registers the view and the mode, polls the client at its tick and wires its
//! buttons (e.g. the main encoder) to the client. See doc/SURFACE_LINK.md.
//!
//! The RGB colors of the provider go to the controller's palette through its color manager
//! (the closest DAW color, as for track colors).

use super::client::{PadFunction, SharedClient, SurfaceMapping};
use super::{LinkState, Rgb, SlotKind};
use crate::daw::color::{ColorEx, DawColor};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{Mode, View};

/// The palette index of an RGB color.
pub fn palette_index(ctx: &Ctx, color: Rgb) -> i32 {
    if color.is_black() {
        return 0;
    }
    let id = DawColor::color_id(ColorEx::from(color));
    if ctx.colors.has_color_index(id) {
        ctx.colors.color_index(id)
    } else {
        0
    }
}

/// The colors of the parts which are not slots.
const ROW_ACTIVE: Rgb = Rgb::WHITE;
const ROW_SET_A: Rgb = Rgb::new(40, 40, 120);
const ROW_SET_B: Rgb = Rgb::new(40, 100, 100);
const PAGE_ACTIVE: Rgb = Rgb::new(255, 140, 0);
const PAGE: Rgb = Rgb::new(110, 60, 0);
const NAVIGATION: Rgb = Rgb::new(90, 90, 90);
const NO_PROVIDER: Rgb = Rgb::new(128, 0, 0);
const INCOMPATIBLE: Rgb = Rgb::new(255, 80, 0);
const SELECTED: Rgb = Rgb::WHITE;

/// The view of the grid.
pub struct SurfaceLinkView {
    client: SharedClient,
}

impl SurfaceLinkView {
    pub fn new(client: SharedClient) -> Self {
        SurfaceLinkView { client }
    }

    /// The colors of all pads (x, y with y = 0 at the top), as the view draws them: the color
    /// and the blink color (-1 for none). Public for the tests and other renderers.
    pub fn pad_colors(ctx: &Ctx, client: &SharedClient) -> Vec<(usize, usize, i32, i32)> {
        let client = client.borrow();
        let mapping: &SurfaceMapping = client.mapping();
        let model = client.model();
        let state = client.state();
        let mut colors = Vec::with_capacity(mapping.columns * mapping.rows);
        for y in 0..mapping.rows {
            for x in 0..mapping.columns {
                let function = mapping.pad_function(x, y);
                let (color, blink) = if state != LinkState::Attached {
                    // "No provider": a cross over the slots
                    match function {
                        PadFunction::Slot(id) => {
                            let (row, column) = (id / 6, id % 6);
                            let on_cross = row == column || row + column == 5;
                            let color = if state == LinkState::Incompatible { INCOMPATIBLE } else { NO_PROVIDER };
                            (if on_cross { palette_index(ctx, color) } else { 0 }, -1)
                        }
                        _ => (0, -1),
                    }
                } else {
                    match function {
                        PadFunction::None => (0, -1),
                        PadFunction::Slot(id) => match model.slot(id) {
                            Some(slot) if slot.kind != SlotKind::Empty && slot.enabled => {
                                let color = palette_index(ctx, slot.color);
                                // Black slots (e.g. a pad which is off) are still visible
                                let color = if color == 0 { palette_index(ctx, NAVIGATION) } else { color };
                                if model.selected_slot == Some(id) {
                                    (color, palette_index(ctx, SELECTED))
                                } else {
                                    (color, -1)
                                }
                            }
                            _ => (0, -1),
                        },
                        PadFunction::Row(row) => {
                            let color = if row == model.active_row {
                                ROW_ACTIVE
                            } else if model.control_set(row).is_multiple_of(2) {
                                ROW_SET_A
                            } else {
                                ROW_SET_B
                            };
                            (palette_index(ctx, color), -1)
                        }
                        PadFunction::Page(page) => {
                            if page >= model.pages.len() {
                                (0, -1)
                            } else {
                                (palette_index(ctx, if page == model.page { PAGE_ACTIVE } else { PAGE }), -1)
                            }
                        }
                        PadFunction::PagePrev => (if model.page > 0 { palette_index(ctx, NAVIGATION) } else { 0 }, -1),
                        PadFunction::PageNext => (if model.page + 1 < model.pages.len() { palette_index(ctx, NAVIGATION) } else { 0 }, -1),
                    }
                };
                colors.push((x, y, color, blink));
            }
        }
        colors
    }
}

impl View for SurfaceLinkView {
    fn name(&self) -> &str {
        "Surface"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let colors = Self::pad_colors(ctx, &self.client);
        let grid = ctx.surface.grid();
        for (x, y, color, blink) in colors {
            if blink >= 0 {
                grid.light_ex_blink(x as i32, y as i32, color, blink, false);
            } else {
                grid.light_ex(x as i32, y as i32, color);
            }
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let grid = ctx.surface.grid_ref();
        let index = note - grid.start_note();
        let columns = grid.cols();
        if index < 0 || index >= columns * grid.rows() {
            return;
        }
        let x = (index % columns) as usize;
        let y = (grid.rows() - 1 - index / columns) as usize;
        self.client.borrow_mut().on_pad(x, y, velocity > 0);
    }

    crate::impl_as_any!();
}

/// The mode of the encoders: they turn the slots of the active row (and what else the mapping
/// says); their values are the values of these slots.
pub struct SurfaceLinkMode {
    client: SharedClient,
}

impl SurfaceLinkMode {
    pub fn new(client: SharedClient) -> Self {
        SurfaceLinkMode { client }
    }

    pub fn client(&self) -> &SharedClient {
        &self.client
    }
}

impl Mode for SurfaceLinkMode {
    fn name(&self) -> &str {
        "Surface"
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let fine = ctx.surface.is_shift_pressed();
        let now = ctx.host.now();
        self.client.borrow_mut().on_encoder(index, value, fine, now);
    }

    fn knob_value(&self, _ctx: &Ctx, index: usize) -> i32 {
        self.client.borrow().encoder_value(index).map_or(0, |(value, _)| value)
    }

    fn knob_color(&self, ctx: &Ctx, index: usize) -> i32 {
        self.client.borrow().encoder_value(index).map_or(0, |(_, color)| palette_index(ctx, color))
    }

    fn on_knob_touch(&mut self, _ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.client.borrow_mut().on_encoder_touch(index, is_touched);
    }

    fn selected_item_name(&self, _ctx: &Ctx) -> Option<String> {
        let client = self.client.borrow();
        client.is_attached().then(|| client.model().page_name().to_string())
    }

    crate::impl_as_any!();
}

/// The text a controller shows (or notifies) for a state of the link.
pub fn state_text(state: LinkState, provider: &str, page: &str) -> String {
    match state {
        LinkState::Missing => "Surface: no provider".to_string(),
        LinkState::Attached => {
            if page.is_empty() {
                format!("Surface: {provider}")
            } else {
                format!("Surface: {provider} - {page}")
            }
        }
        LinkState::Detached => "Surface: provider detached".to_string(),
        LinkState::Incompatible => "Surface: provider has another version".to_string(),
    }
}
