// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A view to select a color for a track, layer or clip (former `ColorView`). The DAW colors are
//! displayed on the pads; a pad release applies the color and restores the previous view.
//!
//! The mode is set with [`crate::framework::feature::View::set_color_select_mode`]. Former
//! `getLayerBank` (the bank of `DeviceLayerDetailsMode`, which follows the active view: the drum
//! pads of the cursor device in the DRUM view, the 64 pads of the drum device in DRUM64, the
//! layers otherwise) is resolved from the view which was active before the color view.

use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::grid::GRID_OFF;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::color_select_mode::ColorSelectMode;
use crate::impl_as_any;

/// The color selection view.
pub struct ColorView {
    page_size: usize,
    pages: usize,
    page: usize,
    mode: ColorSelectMode,
    /// The view which was active before the color view (for the layer bank).
    previous_view: Option<Views>,
}

impl ColorView {
    pub fn new(ctx: &Ctx) -> Self {
        let grid = ctx.surface.grid_ref();
        let page_size = (grid.cols() * grid.rows()).max(1) as usize;
        let pages = 1 + (DawColor::ALL.len() - 1) / page_size;
        ColorView { page_size, pages, page: 0, mode: ColorSelectMode::Track, previous_view: None }
    }

    /// Former `setMode`.
    pub fn set_mode(&mut self, mode: ColorSelectMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> ColorSelectMode {
        self.mode
    }

    /// Former `setPage`.
    pub fn set_page(&mut self, page: i32) {
        self.page = page.clamp(0, self.pages as i32 - 1) as usize;
    }

    /// Former `getPage`.
    pub fn page(&self) -> usize {
        self.page
    }

    fn apply_color(&self, ctx: &mut Ctx, color: DawColor) {
        let entry = color.color();
        match self.mode {
            ColorSelectMode::Track => {
                if ctx.model.cursor_track().does_exist() {
                    ctx.model.cursor_track_mut().set_color(entry);
                } else if ctx.model.master_track().is_selected() {
                    ctx.model.master_track_mut().set_color(entry);
                }
            }
            ColorSelectMode::Layer => match self.previous_view {
                Some(Views::Drum) => {
                    let bank = ctx.model.cursor_device_mut().drum_pad_bank_mut();
                    if let Some(index) = bank.selected_index() {
                        bank.item_mut(index).set_color(entry);
                    }
                }
                Some(Views::Drum64) => {
                    let device = if ctx.model.drum_device_sized(64).is_some() { ctx.model.drum_device_sized_mut(64) } else { ctx.model.drum_device_mut() };
                    if let Some(bank) = device.map(|d| d.drum_pad_bank_mut()) {
                        if let Some(index) = bank.selected_index() {
                            bank.item_mut(index).set_color(entry);
                        }
                    }
                }
                _ => {
                    if let Some(layer) = ctx.model.cursor_device_mut().layer_bank_mut().selected_item_mut() {
                        layer.set_color(entry);
                    }
                }
            },
            ColorSelectMode::Clip => {
                if let Some(clip) = ctx.model.cursor_clip_mut() {
                    if clip.does_exist() {
                        clip.set_color(entry);
                    }
                }
            }
        }
    }
}

impl View for ColorView {
    fn name(&self) -> &str {
        "Color"
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        let previous = ctx.views.previous_id();
        if previous != Some(Views::Color) {
            self.previous_view = previous;
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let offset = self.page * self.page_size;
        let Ctx { surface, colors, .. } = ctx;
        let grid = surface.grid();
        for i in 0..self.page_size {
            let index = offset + i;
            let id = DawColor::ALL.get(index).map(|c| c.id()).unwrap_or(GRID_OFF);
            grid.light_id(colors, 36 + i as i32, id);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity > 0 {
            return;
        }
        let color = self.page * self.page_size + (note - 36).max(0) as usize;
        if let Some(entry) = DawColor::ALL.get(color) {
            self.apply_color(ctx, *entry);
        }
        // The view is in use, restore it afterwards
        ctx.schedule(0, |ctx| ctx.restore_view());
    }

    fn button_color_id(&self, _ctx: &Ctx, _button: ButtonId) -> &'static str {
        BUTTON_COLOR_OFF
    }

    fn set_color_select_mode(&mut self, mode: ColorSelectMode) {
        self.mode = mode;
    }

    impl_as_any!();
}
