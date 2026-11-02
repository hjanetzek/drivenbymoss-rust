// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The components which are drawn on a graphics display (former `IComponent` and its
//! implementations in `framework.graphics.canvas.component`).
//!
//! Only the components used by the ported controllers exist: `LabelComponent` (without icons),
//! `ListComponent` and the simple components for small displays `TitleValueComponent`,
//! `TitleValueMenuComponent` and `TitleChannelsComponent`. The Java classes are an enum here;
//! the equality (former `equals`) decides if the display needs to be rendered again.

use super::bitmap::{Align, GraphicsContext};
use super::{GraphicsConfiguration, GraphicsDimensions};
use crate::daw::color::ColorEx;

/// The layout of a label (former `LabelComponent.LabelLayout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelLayout {
    Plain,
    SmallHeader,
    Colored,
}

// Former AbstractBaseComponent constants
const DEFAULT_WIDTH: i32 = 128;
const DEFAULT_HEIGHT: i32 = 64;
const DEFAULT_ROW_HEIGHT: i32 = 20;
const CENTER: i32 = DEFAULT_WIDTH / 2;
const TOP: i32 = 44;
const RESOLUTION: i32 = 1024;

/// A component of a graphics display.
#[derive(Debug, Clone, PartialEq)]
pub enum Component {
    /// A label (former `LabelComponent`; icons are not supported).
    Label { text: String, color: Option<ColorEx>, is_selected: bool, is_active: bool, layout: LabelLayout },
    /// A list of items of which some are selected (former `ListComponent`).
    List { items: Vec<(String, bool)> },
    /// Two text rows, a horizontal fader and an optional stereo VU meter (former
    /// `TitleValueComponent`); a negative value hides the fader.
    TitleValue { label1: String, label2: String, value: i32, vu_left: i32, vu_right: i32, is_pan: bool },
    /// Like `TitleValue` with smaller rows and a menu of 4 items at the bottom (former
    /// `TitleValueMenuComponent`).
    TitleValueMenu { label1: String, label2: String, menu: [String; 4], value: i32, vu_left: i32, vu_right: i32, is_pan: bool },
    /// A title and vertical faders of several channels (former `TitleChannelsComponent`).
    TitleChannels { label: String, selected: Vec<bool>, values: Vec<i32>, is_pan: bool },
}

impl Component {
    /// A plain label.
    pub fn label(text: &str, color: Option<ColorEx>, is_selected: bool, is_active: bool, layout: LabelLayout) -> Self {
        Component::Label { text: text.to_string(), color, is_selected, is_active, layout }
    }

    /// A title-value component without VU meter.
    pub fn title_value(label1: &str, label2: &str, value: i32, is_pan: bool) -> Self {
        Self::title_value_vu(label1, label2, value, -1, -1, is_pan)
    }

    /// A title-value component with VU meter.
    pub fn title_value_vu(label1: &str, label2: &str, value: i32, vu_left: i32, vu_right: i32, is_pan: bool) -> Self {
        Component::TitleValue { label1: label1.to_string(), label2: label2.to_string(), value, vu_left, vu_right, is_pan }
    }

    /// A title-channels component.
    pub fn title_channels(label: &str, selected: Vec<bool>, values: Vec<i32>, is_pan: bool) -> Self {
        Component::TitleChannels { label: label.to_string(), selected, values, is_pan }
    }

    /// Draw the component into the given bounds (left, top, width, height).
    pub fn draw(&self, gc: &mut GraphicsContext, bounds: (f64, f64, f64, f64), configuration: &GraphicsConfiguration, dimensions: &GraphicsDimensions) {
        match self {
            Component::Label { text, color, is_selected, is_active, layout } => {
                draw_label(gc, bounds, configuration, dimensions, text, *color, *is_selected, *is_active, *layout)
            }
            Component::List { items } => draw_list(gc, bounds, configuration, dimensions, items),
            Component::TitleValue { label1, label2, value, vu_left, vu_right, is_pan } => {
                let row_height = DEFAULT_ROW_HEIGHT as f64;
                let title = TitleValue { label1, label2, value: *value, vu_left: *vu_left, vu_right: *vu_right, is_pan: *is_pan, row_height };
                title.draw(gc, configuration, TOP as f64, row_height);
            }
            Component::TitleValueMenu { label1, label2, menu, value, vu_left, vu_right, is_pan } => {
                let row_height = (DEFAULT_HEIGHT / 4 - 2) as f64;
                let title = TitleValue { label1, label2, value: *value, vu_left: *vu_left, vu_right: *vu_right, is_pan: *is_pan, row_height };
                title.draw(gc, configuration, 2.0 * row_height + 4.0, row_height - 4.0);
                draw_menu(gc, menu, row_height);
            }
            Component::TitleChannels { label, selected, values, is_pan } => draw_title_channels(gc, configuration, label, selected, values, *is_pan),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_label(
    gc: &mut GraphicsContext,
    bounds: (f64, f64, f64, f64),
    configuration: &GraphicsConfiguration,
    dimensions: &GraphicsDimensions,
    text: &str,
    color: Option<ColorEx>,
    is_selected: bool,
    is_active: bool,
    layout: LabelLayout,
) {
    let (left, top, width, height) = bounds;
    let separator_size = dimensions.separator_size;
    let menu_height = dimensions.menu_height();
    if layout == LabelLayout::Colored {
        // Former drawColoredLayout (without icon)
        let unit = dimensions.unit();
        let inset = dimensions.inset();
        let text_height = 1.2 * unit + 1.0;
        let text_top = top + height - text_height;
        let info_color = color.unwrap_or(configuration.color_background);
        gc.fill_rectangle(left, text_top, width, text_height, if is_active { info_color } else { ColorEx::even_darker(info_color) });
        let mut color_text = configuration.color_text;
        if is_selected {
            color_text = ColorEx::calc_contrast_color(color_text);
        }
        if !is_active {
            color_text = ColorEx::dim_to_gray(color_text);
        }
        gc.draw_text_in_bounds(text, left, text_top, width - 1.4 * inset, text_height, Align::Left, color_text, None, unit);
        return;
    }

    // Former getBackgroundColor
    let bg_color = if is_selected {
        configuration.color_text
    } else if let Some(color) = color {
        color
    } else if layout == LabelLayout::Plain {
        configuration.color_background_darker
    } else {
        configuration.color_border
    };
    let text_color = ColorEx::calc_contrast_color(bg_color);

    if text.is_empty() {
        if layout == LabelLayout::SmallHeader {
            // Remove the 2 pixels of the previous menus border line
            gc.fill_rectangle(left - separator_size, menu_height - 2.0, separator_size, 1.0, configuration.color_border);
        }
        return;
    }

    if layout == LabelLayout::SmallHeader {
        gc.fill_rectangle(left, top, width, menu_height - 1.0, if is_selected { text_color } else { bg_color });
        gc.fill_rectangle(left, menu_height - 2.0, width + separator_size, 1.0, text_color);
        let unit = dimensions.unit();
        gc.draw_text_in_bounds(text, left, 1.0, width, unit + separator_size, Align::Center, if is_selected { bg_color } else { text_color }, None, unit);
    } else {
        gc.fill_rectangle(left, top, width, height, if is_selected { text_color } else { bg_color });
        gc.draw_text_in_bounds(text, left, top, width, height, Align::Center, if is_selected { bg_color } else { text_color }, None, height / 2.0);
    }
}

fn draw_list(gc: &mut GraphicsContext, bounds: (f64, f64, f64, f64), configuration: &GraphicsConfiguration, dimensions: &GraphicsDimensions, items: &[(String, bool)]) {
    let (left, _top, width, height) = bounds;
    let separator_size = dimensions.separator_size;
    let inset = dimensions.inset();
    let size = items.len();
    if size == 0 {
        return;
    }
    let item_left = left + separator_size;
    let item_width = width - separator_size;
    let item_height = height / size as f64;
    let mut text_color = configuration.color_text;
    let border_color = configuration.color_border;
    if text_color == border_color {
        text_color = ColorEx::calc_contrast_color(text_color);
    }
    for (i, (text, is_selected)) in items.iter().enumerate() {
        let item_top = i as f64 * item_height;
        gc.fill_rectangle(item_left, item_top + separator_size, item_width, item_height - 2.0 * separator_size, if *is_selected { text_color } else { border_color });
        let color = if *is_selected { border_color } else { text_color };
        gc.draw_text_in_bounds(text, item_left + inset, item_top, item_width - 2.0 * inset, item_height, Align::Left, color, None, item_height * 0.6);
    }
}

/// The data of the title-value components.
struct TitleValue<'a> {
    label1: &'a str,
    label2: &'a str,
    value: i32,
    vu_left: i32,
    vu_right: i32,
    is_pan: bool,
    row_height: f64,
}

impl TitleValue<'_> {
    fn draw(&self, gc: &mut GraphicsContext, configuration: &GraphicsConfiguration, fader_top: f64, fader_height: f64) {
        let color_text = configuration.color_text;
        let color_fader = configuration.color_fader;
        // Former drawLabels
        gc.draw_text_in_height(self.label1, 0.0, 0.0, self.row_height, color_text, None, self.row_height);
        gc.draw_text_in_height(self.label2, 0.0, self.row_height, self.row_height, color_text, None, self.row_height);
        self.draw_fader(gc, color_fader, fader_top, fader_height);
        self.draw_vu(gc, color_fader);
    }

    fn draw_fader(&self, gc: &mut GraphicsContext, color_fader: ColorEx, fader_top: f64, fader_height: f64) {
        if self.value < 0 {
            return;
        }
        let width = self.value * 128 / RESOLUTION;
        gc.stroke_rectangle(1.0, fader_top, 127.0, fader_height, color_fader);
        if self.is_pan {
            if width == CENTER {
                gc.fill_rectangle(CENTER as f64 + 1.0, fader_top, 1.0, fader_height, color_fader);
            } else if width > CENTER {
                gc.fill_rectangle(CENTER as f64 + 1.0, fader_top, (width - CENTER) as f64, fader_height, color_fader);
            } else {
                gc.fill_rectangle(width as f64, fader_top, (CENTER - width) as f64, fader_height, color_fader);
            }
        } else {
            gc.fill_rectangle(1.0, fader_top, width as f64, fader_height, color_fader);
        }
    }

    fn draw_vu(&self, gc: &mut GraphicsContext, color: ColorEx) {
        if self.vu_left <= 0 && self.vu_right <= 0 {
            return;
        }
        let height = 2.0 * self.row_height;
        let height_left = self.vu_left as f64 * height / RESOLUTION as f64;
        let top_left = (height - height_left - 1.0).max(0.0);
        gc.stroke_rectangle(118.0, top_left, 5.0, height_left, ColorEx::BLACK);
        gc.fill_rectangle(119.0, height - height_left, 3.0, height_left, color);
        let height_right = self.vu_right as f64 * height / RESOLUTION as f64;
        let top_right = (height - height_right - 1.0).max(0.0);
        gc.stroke_rectangle(123.0, top_right, 4.0, height - top_right, ColorEx::BLACK);
        gc.fill_rectangle(124.0, height - height_right, 3.0, height_right, color);
    }
}

fn draw_menu(gc: &mut GraphicsContext, menu: &[String; 4], row_height: f64) {
    let menu_height = row_height - 4.0;
    let menu_top = DEFAULT_HEIGHT as f64 - menu_height;
    let item_width = (DEFAULT_WIDTH / 4) as f64;
    gc.fill_rectangle(0.0, menu_top, DEFAULT_WIDTH as f64, menu_height, ColorEx::WHITE);
    for (i, item) in menu.iter().enumerate() {
        gc.draw_text_in_bounds(item, i as f64 * item_width, menu_top, item_width, menu_height, Align::Center, ColorEx::BLACK, None, menu_height + 2.0);
    }
}

fn draw_title_channels(gc: &mut GraphicsContext, configuration: &GraphicsConfiguration, label: &str, selected: &[bool], values: &[i32], is_pan: bool) {
    let row_height = DEFAULT_ROW_HEIGHT;
    let color_text = configuration.color_text;
    let color_fader = configuration.color_fader;
    gc.draw_text_in_height(label, 0.0, 0.0, row_height as f64, color_text, None, row_height as f64);
    if values.is_empty() {
        return;
    }
    let channel_width = DEFAULT_WIDTH / values.len() as i32;
    let half_channel_width = (channel_width / 2) as f64;
    let lower_height = DEFAULT_HEIGHT - row_height - 1;
    let height = DEFAULT_HEIGHT as f64;
    for (i, value) in values.iter().enumerate() {
        let left = (i as i32 * channel_width) as f64;
        let fader_height = value * lower_height / RESOLUTION;
        let is_selected = selected.get(i).copied().unwrap_or(false);
        if is_pan {
            let center = lower_height / 2;
            let (x, y, h) = if fader_height == center {
                (left, height - center as f64 + 1.0, 1.0)
            } else if fader_height > center {
                (left, height - center as f64, (fader_height - center) as f64)
            } else {
                (left, height - lower_height as f64 + fader_height as f64, (center - fader_height) as f64 + 2.0)
            };
            if is_selected {
                gc.fill_rectangle(x, y, half_channel_width + 1.0, h, color_fader);
            } else {
                gc.stroke_rectangle(x + 1.0, y, half_channel_width, h, color_fader);
            }
        } else if is_selected {
            gc.fill_rectangle(left, height - fader_height as f64, half_channel_width + 1.0, fader_height as f64, color_fader);
        } else {
            gc.stroke_rectangle(left + 1.0, height - fader_height as f64, half_channel_width, fader_height as f64, color_fader);
        }
    }
}
