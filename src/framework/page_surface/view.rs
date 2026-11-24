// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The generic page surface view (the grid) and mode (the encoders) which any grid controller can
//! use. The controller supplies a [`SurfaceMapping`] (cells, page and section lists, encoders),
//! registers the view and the mode, polls the client at its tick and wires its buttons (e.g. the
//! main encoder) to the client. See doc/SURFACE_LINK.md, doc/HOST_PAGE_PLAN.md §5.
//!
//! The RGB colors of the provider go to the controller's palette through its color manager,
//! always on the direct path ([`ColorManager::direct_color_id_for_role`]), since they never come
//! from Bitwig; a controller without a palette table takes the closest DAW color. On a palette
//! made of hues × levels (NI's) the view gives the role of each part ([`ColorRole`]): a normal
//! slot full, the selected slot pale (highlight), a disabled or covered slot dim, the inactive
//! row and page selectors medium; with explicit control sets the cells outside the active set
//! medium too, and the sections in their colours (the active one full). Other palettes take the nearest entry and draw disabled and
//! covered slots off (as before).
//!
//! [`ColorManager::direct_color_id_for_role`]: crate::framework::color::ColorManager::direct_color_id_for_role

use super::client::{PadFunction, SharedClient, SurfaceMapping};
use super::{LinkState, PageModel, Rgb, SlotKind, GRID_SIZE};
use crate::daw::color::ColorEx;
use crate::framework::color::ColorRole;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{Mode, View};

/// The palette index of an RGB color (in the normal role).
pub fn palette_index(ctx: &Ctx, color: Rgb) -> i32 {
    palette_index_for_role(ctx, color, ColorRole::Normal)
}

/// The palette index of an RGB color in a role; black is off.
pub fn palette_index_for_role(ctx: &Ctx, color: Rgb, role: ColorRole) -> i32 {
    if color.is_black() {
        return 0;
    }
    let id = ctx.colors.direct_color_id_for_role(ColorEx::from(color), role);
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

/// The color of a row selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowColor {
    Active,
    /// An inactive row of an even control set, and every inactive row of a surface whose
    /// encoders cover only one row.
    SetA,
    /// An inactive row of an odd control set (surfaces whose encoders cover a whole set).
    SetB,
}

/// The color of the selector of a row: the active row white; the others alternate per control
/// set only if the surface's slot encoders drive a whole control set at once
/// ([`SurfaceMapping::covers_control_set`]), otherwise the grouping does not matter and they all
/// have the same color.
pub fn row_color(mapping: &SurfaceMapping, model: &PageModel, row: usize) -> RowColor {
    if row == model.active_row {
        RowColor::Active
    } else if !mapping.covers_control_set(model.rows_per_set) || model.control_set(row).is_multiple_of(2) {
        RowColor::SetA
    } else {
        RowColor::SetB
    }
}

/// The view of the grid.
pub struct PageSurfaceView {
    client: SharedClient,
}

impl PageSurfaceView {
    pub fn new(client: SharedClient) -> Self {
        PageSurfaceView { client }
    }

    /// The colors of all pads (x, y with y = 0 at the top), as the view draws them: the color
    /// and the blink color (-1 for none). Public for the tests and other renderers.
    pub fn pad_colors(ctx: &Ctx, client: &SharedClient) -> Vec<(usize, usize, i32, i32)> {
        let client = client.borrow();
        let mapping: &SurfaceMapping = client.mapping();
        let model = client.model();
        let state = client.state();
        let ui = mapping.ui_colors;
        let ui_color = |index: Option<i32>, default: Rgb, role: ColorRole| index.unwrap_or_else(|| palette_index_for_role(ctx, default, role));
        let has_roles = ctx.colors.has_role_levels();
        let page_offset = client.page_offset();
        let mut colors = Vec::with_capacity(mapping.columns * mapping.rows);
        for y in 0..mapping.rows {
            for x in 0..mapping.columns {
                let function = mapping.pad_function(x, y);
                let (color, blink) = if state != LinkState::Attached {
                    // "No provider": a cross over the slots
                    match function {
                        PadFunction::Cell { row, column } => {
                            let on_cross = row < GRID_SIZE && column < GRID_SIZE && (row == column || row + column == GRID_SIZE - 1);
                            let color = if state == LinkState::Incompatible { ui_color(ui.incompatible, INCOMPATIBLE, ColorRole::Normal) } else { ui_color(ui.no_provider, NO_PROVIDER, ColorRole::Inactive) };
                            (if on_cross { color } else { 0 }, -1)
                        }
                        _ => (0, -1),
                    }
                } else {
                    match function {
                        PadFunction::None => (0, -1),
                        PadFunction::Cell { row, column } => match model.slot_id(row, column).map(|id| (id, &model.slots[id])) {
                            Some((id, slot)) if slot.kind != SlotKind::Empty && slot.enabled => {
                                let selected = model.selected_slot == Some(id);
                                // With explicit control sets, the cells outside the active one a level lower
                                let outside = model.has_sets() && slot.set != model.active_set;
                                let role = if selected {
                                    ColorRole::Highlight
                                } else if outside && has_roles {
                                    ColorRole::Inactive
                                } else {
                                    ColorRole::Normal
                                };
                                let color = palette_index_for_role(ctx, slot.color, role);
                                // Black slots (e.g. a pad which is off) are still visible
                                let color = if color == 0 { ui_color(ui.black_slot, NAVIGATION, ColorRole::Inactive) } else { color };
                                if selected {
                                    (color, ui_color(ui.selected, SELECTED, ColorRole::Normal))
                                } else {
                                    (color, -1)
                                }
                            }
                            // Disabled (hidden) and covered slots: dim on a palette with levels
                            Some((_, slot)) if !slot.enabled && has_roles => (palette_index_for_role(ctx, slot.color, ColorRole::Disabled), -1),
                            _ => (0, -1),
                        },
                        PadFunction::Row(row) => {
                            let color = match row_color(mapping, model, row) {
                                RowColor::Active => ui_color(ui.row_active, ROW_ACTIVE, ColorRole::Normal),
                                RowColor::SetA => ui_color(ui.row_set_a, ROW_SET_A, ColorRole::Inactive),
                                RowColor::SetB => ui_color(ui.row_set_b, ROW_SET_B, ColorRole::Inactive),
                            };
                            (color, -1)
                        }
                        PadFunction::Page(index) => {
                            let page = page_offset + index;
                            if page >= model.pages.len() {
                                (0, -1)
                            } else {
                                (if page == model.page { ui_color(ui.page_active, PAGE_ACTIVE, ColorRole::Normal) } else { ui_color(ui.page, PAGE, ColorRole::Inactive) }, -1)
                            }
                        }
                        PadFunction::PagePrev => (if model.page > 0 { ui_color(ui.navigation, NAVIGATION, ColorRole::Inactive) } else { 0 }, -1),
                        PadFunction::PageNext => (if model.page + 1 < model.pages.len() { ui_color(ui.navigation, NAVIGATION, ColorRole::Inactive) } else { 0 }, -1),
                        PadFunction::Section(section) => match model.sections.get(section) {
                            Some(s) => {
                                let role = if model.active_section == Some(section) || !has_roles { ColorRole::Normal } else { ColorRole::Inactive };
                                let color = palette_index_for_role(ctx, s.color, role);
                                (if color == 0 { ui_color(ui.black_slot, NAVIGATION, ColorRole::Inactive) } else { color }, -1)
                            }
                            None => (0, -1),
                        },
                    }
                };
                colors.push((x, y, color, blink));
            }
        }
        colors
    }
}

impl View for PageSurfaceView {
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
pub struct PageSurfaceMode {
    client: SharedClient,
}

impl PageSurfaceMode {
    pub fn new(client: SharedClient) -> Self {
        PageSurfaceMode { client }
    }

    pub fn client(&self) -> &SharedClient {
        &self.client
    }
}

impl Mode for PageSurfaceMode {
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

/// The text a controller shows (or notifies) for a state of the source.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::page_surface::client::{EncoderFunction, EncoderKind};

    fn rows(mapping: &SurfaceMapping, model: &PageModel) -> Vec<RowColor> {
        (0..6).map(|row| row_color(mapping, model, row)).collect()
    }

    #[test]
    fn row_colors_group_only_for_a_whole_control_set() {
        let model = PageModel { active_row: 2, ..PageModel::default() };
        // One row of encoders (the JAM: 6 strips for the row, 2 others): all inactive rows alike
        let mut encoders: Vec<EncoderFunction> = (0..6).map(EncoderFunction::RowSlot).collect();
        encoders.push(EncoderFunction::Selected { fine: true });
        encoders.push(EncoderFunction::Extra(0));
        let one_row = SurfaceMapping::new(8, 8).row_selectors_at(0, 1).encoders(EncoderKind::Absolute, encoders);
        assert!(!one_row.covers_control_set(model.rows_per_set));
        use RowColor::*;
        assert_eq!(rows(&one_row, &model), vec![SetA, SetA, Active, SetA, SetA, SetA]);
        // 12 slot encoders drive the 2 rows of a control set: the sets alternate
        let two_rows = SurfaceMapping::new(8, 8).row_selectors_at(0, 1).encoders(EncoderKind::Absolute, (0..12).map(EncoderFunction::RowSlot).collect());
        assert!(two_rows.covers_control_set(model.rows_per_set));
        assert_eq!(rows(&two_rows, &model), vec![SetA, SetA, Active, SetB, SetA, SetA]);
        // Sets of one row: no grouping
        let single = PageModel { rows_per_set: 1, ..model.clone() };
        assert!(!two_rows.covers_control_set(single.rows_per_set));
    }
}
