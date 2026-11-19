// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The components of the large color displays (Push 2/3), former
//! `framework.graphics.canvas.component`: `LabelComponent` (with icons), `MenuComponent`,
//! `ChannelSelectComponent`, `ChannelComponent`, `SendsComponent`, `ParameterComponent`,
//! `OptionsComponent`, `SceneListGridElement`, `ClipListComponent`, `MidiClipComponent` and
//! `GraphOverlayComponent`. The layout math is the one of Java.
//!
//! Differences to Java:
//! - The components hold plain owned data instead of model objects (`IScene`, `ITrack`/`ISlot`,
//!   `INoteClip`): [`SceneInfo`], [`SlotInfo`] and [`MidiClipInfo`], which the modes fill from
//!   the model (e.g. with [`SceneInfo::from_scene`]). The equality (former `equals`) of the
//!   data decides if the display is rendered again; Java compares the model objects by
//!   reference for the clip list and the MIDI clip, i.e. renders them always.
//! - The class hierarchy (`ChannelComponent extends ChannelSelectComponent extends
//!   MenuComponent`) is composition: every component contains its [`MenuComponent`].
//! - `GraphOverlayComponent` does nothing with less than 1 data value (Java throws).

use super::bitmap::{Align, GraphicsContext};
use super::component::draw_label;
use super::icons::{self, Icon};
use super::{GraphicsConfiguration, GraphicsDimensions};
use crate::daw::bank::BankItem;
use crate::daw::clip::{CursorClip, NotePosition, StepState, NUM_CHANNELS};
use crate::daw::color::ColorEx;
use crate::daw::constants::ChannelType;
use crate::daw::scene::Scene;
use crate::daw::slot::Slot;
use crate::daw::track::Track;
use crate::framework::string_utils;
use crate::scale::Scales;

pub use super::component::LabelLayout;

/// The bounds of a component: left, top, width and height.
pub type Bounds = (f64, f64, f64, f64);

/// The drawing context of a component (former `IGraphicsInfo`).
pub struct DrawInfo<'a, 'b> {
    pub gc: &'a mut GraphicsContext<'b>,
    pub configuration: &'a GraphicsConfiguration,
    pub dimensions: &'a GraphicsDimensions,
}

/// The same left and width with a different top and height (former `withBounds (top,
/// height)`).
fn with_top_height(bounds: Bounds, top: f64, height: f64) -> Bounds {
    (bounds.0, top, bounds.2, height)
}

// ----------------------------------------------------------------------------------------------
// Label and menu

/// A label with an optional icon (former `LabelComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct LabelComponent {
    pub text: String,
    pub icon: Option<Icon>,
    pub color: Option<ColorEx>,
    pub is_selected: bool,
    pub is_active: bool,
    pub layout: LabelLayout,
}

impl LabelComponent {
    pub fn new(text: &str, icon: Option<Icon>, color: Option<ColorEx>, is_selected: bool, is_active: bool, layout: LabelLayout) -> Self {
        LabelComponent { text: text.to_string(), icon, color, is_selected, is_active, layout }
    }

    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        draw_label(info.gc, bounds, info.configuration, info.dimensions, &self.text, self.icon, self.color, self.is_selected, self.is_active, self.layout);
    }
}

/// A small header at the top and a label at the bottom (former `MenuComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuComponent {
    pub header: LabelComponent,
    pub footer: LabelComponent,
}

impl MenuComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(menu_name: &str, is_menu_selected: bool, name: &str, icon: Option<Icon>, color: Option<ColorEx>, is_selected: bool, is_active: bool, lower_layout: LabelLayout) -> Self {
        MenuComponent {
            header: LabelComponent::new(menu_name, None, None, is_menu_selected, true, LabelLayout::SmallHeader),
            footer: LabelComponent::new(name, icon, color, is_selected, is_active, lower_layout),
        }
    }

    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        self.header.draw(info, bounds);
        // Element is off if the name is empty
        if self.footer.text.is_empty() {
            return;
        }
        let menu_height = 2.0 * info.dimensions.menu_height();
        self.footer.draw(info, with_top_height(bounds, bounds.3 - menu_height, menu_height));
    }
}

/// A menu with the icon of a channel type (former `ChannelSelectComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelSelectComponent {
    pub menu: MenuComponent,
}

impl ChannelSelectComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(channel_type: Option<ChannelType>, menu_name: &str, is_menu_selected: bool, name: &str, color: ColorEx, is_selected: bool, is_active: bool, is_pinned: bool) -> Self {
        let icon = icons::channel_icon(channel_type, is_pinned);
        ChannelSelectComponent { menu: MenuComponent::new(menu_name, is_menu_selected, name, icon, Some(color), is_selected, is_active, LabelLayout::Colored) }
    }

    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        self.menu.draw(info, bounds);
    }
}

// ----------------------------------------------------------------------------------------------
// Channel

/// What a channel component edits (former `ChannelComponent.EDIT_TYPE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelEditType {
    Volume,
    Pan,
    Crossfader,
    All,
}

impl ChannelEditType {
    /// The edit type of the `channelType` of `addChannelElement` (former
    /// `AbstractGraphicDisplay.GRID_ELEMENT_CHANNEL_*`).
    pub fn from_grid_element(grid_element: i32) -> Self {
        match grid_element {
            GRID_ELEMENT_CHANNEL_VOLUME => ChannelEditType::Volume,
            GRID_ELEMENT_CHANNEL_PAN => ChannelEditType::Pan,
            GRID_ELEMENT_CHANNEL_CROSSFADER => ChannelEditType::Crossfader,
            _ => ChannelEditType::All,
        }
    }
}

// Former AbstractGraphicDisplay.GRID_ELEMENT_*
pub const GRID_ELEMENT_CHANNEL_SELECTION: i32 = 0;
pub const GRID_ELEMENT_CHANNEL_VOLUME: i32 = 1;
pub const GRID_ELEMENT_CHANNEL_PAN: i32 = 2;
pub const GRID_ELEMENT_CHANNEL_CROSSFADER: i32 = 3;
pub const GRID_ELEMENT_CHANNEL_SENDS: i32 = 4;
pub const GRID_ELEMENT_CHANNEL_ALL: i32 = 5;
pub const GRID_ELEMENT_PARAMETERS: i32 = 6;
pub const GRID_ELEMENT_OPTIONS: i32 = 7;
pub const GRID_ELEMENT_LIST: i32 = 8;

/// A channel with crossfader, pan, volume fader, VU meter and record arm, solo and mute
/// buttons (former `ChannelComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelComponent {
    pub menu: MenuComponent,
    pub edit_type: ChannelEditType,
    pub channel_type: Option<ChannelType>,
    pub volume_value: i32,
    pub modulated_volume_value: i32,
    pub volume_text: String,
    pub pan_value: i32,
    pub modulated_pan_value: i32,
    pub pan_text: String,
    pub vu_value_left: i32,
    pub vu_value_right: i32,
    pub is_mute: bool,
    pub is_solo: bool,
    pub is_arm: bool,
    pub is_active: bool,
    /// 0 = A, 1 = AB, 2 = B, -1 = no crossfader.
    pub crossfade_mode: i32,
}

impl ChannelComponent {
    fn modify_if_off(&self, color: ColorEx) -> ColorEx {
        if self.is_active {
            color
        } else {
            ColorEx::dim_to_gray(color)
        }
    }

    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        self.menu.draw(info, bounds);

        let dimensions = info.dimensions;
        let configuration = info.configuration;
        let (left, _, width, height) = bounds;
        let half_width = width / 2.0;
        let separator_size = dimensions.separator_size;
        let menu_height = dimensions.menu_height();
        let unit = dimensions.unit();
        let half_unit = dimensions.half_unit();
        let controls_top = dimensions.controls_top();
        let inset = dimensions.inset();

        let track_row_height = 1.2 * unit + 1.0;
        let track_row_top = height - track_row_height - separator_size;
        let control_width = half_width - half_unit - half_unit / 2.0;
        let control_start = left + half_width + half_unit - half_unit / 2.0;
        let pan_width = control_width - 2.0;
        let pan_start = control_start + 1.0;
        let pan_top = controls_top + 1.0;
        let pan_height = unit - separator_size;
        let pan_text_top = pan_top + pan_height;
        let fader_offset = control_width / 4.0;
        let fader_top = pan_top + pan_height + separator_size + 1.0;
        let vu_x = control_start + separator_size;
        let fader_left = vu_x + fader_offset;
        let fader_height = track_row_top - fader_top - inset + 1.0;
        let fader_inner_height = fader_height - 2.0 * separator_size;
        let volume_text_width = 1.4 * control_width;
        let volume_text_left = fader_left - volume_text_width - 2.0;

        //
        // Drawing
        //

        let text_color = self.modify_if_off(configuration.color_text);
        // Element is off if the name is empty
        if self.menu.footer.text.is_empty() {
            return;
        }

        let background_color = self.modify_if_off(configuration.color_background);

        // Draw the background
        let gc = &mut *info.gc;
        let fill = if self.menu.footer.is_selected { self.modify_if_off(configuration.color_background_lighter) } else { background_color };
        gc.fill_rectangle(left, menu_height + 1.0, width, track_row_top - (menu_height - 1.0), fill);

        // Background of pan and slider area
        let border_color = self.modify_if_off(configuration.color_border);
        gc.fill_rectangle(control_start, controls_top, half_width - unit + half_unit / 2.0, unit, border_color);
        gc.fill_rectangle(control_start, fader_top, control_width, fader_height, border_color);

        let background_darker = self.modify_if_off(configuration.color_background_darker);
        let edit_color = self.modify_if_off(configuration.color_edit);

        // Crossfader A|B
        let left_column = left + inset - 1.0;
        let channel_type = self.channel_type;
        let is_master_cue_layer = matches!(channel_type, Some(ChannelType::Master | ChannelType::Cue | ChannelType::Layer));
        if !is_master_cue_layer && self.crossfade_mode != -1 {
            let sel_color = if matches!(self.edit_type, ChannelEditType::Crossfader | ChannelEditType::All) { edit_color } else { ColorEx::darker(edit_color) };
            let cross_opt_width = control_width / 3.0;
            let on_color = self.modify_if_off(sel_color);
            let crossfade_icons = [&super::icon_data::CROSSFADE_A, &super::icon_data::CROSSFADE_AB, &super::icon_data::CROSSFADE_B];
            for (i, icon) in crossfade_icons.into_iter().enumerate() {
                let x = left_column + i as f64 * cross_opt_width;
                self.draw_button(gc, configuration, (x, controls_top, cross_opt_width, pan_height + 2.0), background_color, on_color, text_color, self.crossfade_mode == i as i32, icon, 0.0);
            }
        }

        // Panning
        gc.fill_rectangle(pan_start, pan_top, pan_width, pan_height, background_darker);
        let pan_range = pan_width / 2.0;
        let pan_middle = pan_start + pan_range;
        gc.draw_line(pan_middle, pan_top, pan_middle, pan_top + pan_height, border_color);
        let max_value = dimensions.max_parameter_value as f64;
        let half_max = max_value / 2.0;
        let is_pan_touched = !self.pan_text.is_empty();
        // Panned to the left or right?
        let pan_value = self.pan_value as f64;
        let modulated_pan_value = self.modulated_pan_value as f64;
        let is_right = pan_value > half_max;
        let is_modulated_right = modulated_pan_value > half_max;
        let v = if is_right { (pan_value - half_max) * pan_range / half_max } else { pan_range - pan_value * pan_range / half_max };
        let is_pan_modulated = self.modulated_pan_value != -1;
        let v_mod = if !is_pan_modulated {
            v
        } else if is_modulated_right {
            (modulated_pan_value - half_max) * pan_range / half_max
        } else {
            pan_range - modulated_pan_value * pan_range / half_max
        };
        let fader_color = self.modify_if_off(configuration.color_fader);
        let right_mod = if is_pan_modulated { is_modulated_right } else { is_right };
        gc.fill_rectangle(if right_mod { pan_middle + 1.0 } else { pan_middle - v_mod }, controls_top + 1.0, v_mod, pan_height, fader_color);
        if matches!(self.edit_type, ChannelEditType::Pan | ChannelEditType::All) {
            let w = if is_pan_touched { 3.0 } else { 1.0 };
            let start = if is_right { (pan_middle + pan_range - w).min(pan_middle + v) } else { (pan_middle - pan_range).max(pan_middle - v) };
            gc.fill_rectangle(start, controls_top + 1.0, w, pan_height, edit_color);
        }

        // Volume slider
        // Ensure that maximum value is reached even if rounding errors happen
        let volume_value = self.volume_value as f64;
        let volume_width = control_width - 2.0 * separator_size - fader_offset;
        let scaled_height = |value: f64| if value >= max_value - 1.0 { fader_inner_height } else { fader_inner_height * value / max_value };
        let volume_height = scaled_height(volume_value);
        let is_volume_modulated = self.modulated_volume_value != -1;
        let modulated_volume_height = if is_volume_modulated { scaled_height(self.modulated_volume_value as f64) } else { volume_height };
        let volume_top = fader_top + separator_size + fader_inner_height - volume_height;
        let modulated_volume_top = if is_volume_modulated { fader_top + separator_size + fader_inner_height - modulated_volume_height } else { volume_top };
        gc.fill_rectangle(fader_left, modulated_volume_top, volume_width, modulated_volume_height, fader_color);

        let is_volume_touched = !self.volume_text.is_empty();
        if matches!(self.edit_type, ChannelEditType::Volume | ChannelEditType::All) {
            let h = if is_volume_touched { 3.0 } else { 1.0 };
            gc.fill_rectangle(fader_left, (volume_top + volume_height - h).min(volume_top), volume_width, h, edit_color);
        }

        // VU
        let vu_height_left = scaled_height(self.vu_value_left as f64);
        let vu_height_right = scaled_height(self.vu_value_right as f64);
        let vu_offset_left = fader_inner_height - vu_height_left;
        let vu_offset_right = fader_inner_height - vu_height_right;
        let vu_width = fader_offset - separator_size;
        gc.fill_rectangle(vu_x, fader_top + separator_size, vu_width + 1.0, fader_inner_height, background_darker);
        let mut color_vu = self.modify_if_off(configuration.color_vu);
        if self.is_mute {
            color_vu = configuration.color_mute;
        }
        gc.fill_rectangle(vu_x, fader_top + separator_size + vu_offset_left, vu_width / 2.0, vu_height_left, color_vu);
        gc.fill_rectangle(vu_x + vu_width / 2.0, fader_top + separator_size + vu_offset_right, vu_width / 2.0, vu_height_right, color_vu);

        // Draw record arm, solo and mute
        if channel_type != Some(ChannelType::Cue) {
            let button_offset = 2.0 * unit;
            let mut button_top = fader_top + button_offset;
            let button_height = (fader_height - button_offset - 4.0 * separator_size) / 3.0;

            if channel_type != Some(ChannelType::Layer) {
                // Record Arm
                let on_color = self.modify_if_off(configuration.color_record);
                self.draw_button(gc, configuration, (left_column, button_top, control_width, button_height - 1.0), background_color, on_color, text_color, self.is_arm, &super::icon_data::RECORD_ARM, 2.0);
            }

            // Solo
            button_top += button_height + 2.0 * separator_size;
            let on_color = self.modify_if_off(configuration.color_solo);
            self.draw_button(gc, configuration, (left_column, button_top, control_width, button_height - 1.0), background_color, on_color, text_color, self.is_solo, &super::icon_data::SOLO, 2.0);

            // Mute
            button_top += button_height + 2.0 * separator_size;
            let on_color = self.modify_if_off(configuration.color_mute);
            self.draw_button(gc, configuration, (left_column, button_top, control_width, button_height - 1.0), background_color, on_color, text_color, self.is_mute, &super::icon_data::MUTE, 2.0);
        }

        // Draw panning text on top if set
        if is_pan_touched {
            gc.fill_rectangle(control_start, pan_text_top, control_width, unit, background_darker);
            gc.stroke_rectangle(control_start, pan_text_top, control_width, unit, border_color);
            gc.draw_text_in_bounds(&self.pan_text, control_start, pan_text_top, control_width, unit, Align::Center, text_color, None, unit);
        }

        // Draw volume text on top if set
        if is_volume_touched {
            let volume_text_top = if volume_value >= max_value - 1.0 { fader_top } else { (volume_top - 1.0).min(fader_top + fader_inner_height + separator_size - unit + 1.0) };
            gc.fill_rectangle(volume_text_left, volume_text_top, volume_text_width, unit, background_darker);
            gc.stroke_rectangle(volume_text_left, volume_text_top, volume_text_width, unit, border_color);
            gc.draw_text_in_bounds(&self.volume_text, volume_text_left, volume_text_top, volume_text_width, unit, Align::Center, text_color, None, unit);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_button(&self, gc: &mut GraphicsContext, configuration: &GraphicsConfiguration, bounds: Bounds, background_color: ColorEx, is_on_color: ColorEx, text_color: ColorEx, is_on: bool, icon: Icon, radius: f64) {
        let (left, top, width, height) = bounds;
        let border_color = self.modify_if_off(configuration.color_border);
        gc.fill_rounded_rectangle(left, top, width, height, radius, border_color);
        if is_on {
            gc.fill_rounded_rectangle(left + 1.0, top + 1.0, width - 2.0, height - 2.0, radius, is_on_color);
        } else {
            gc.fill_gradient_rounded_rectangle(left + 1.0, top + 1.0, width - 2.0, height - 2.0, radius, background_color, ColorEx::brighter(background_color));
        }
        let x = left + (width - icon.width as f64) / 2.0;
        let y = top + (height - icon.height as f64) / 2.0;
        gc.mask_image(icon, x, y, if is_on { border_color } else { text_color });
    }
}

// ----------------------------------------------------------------------------------------------
// Sends

/// The data of a send (former `SendData`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SendData {
    pub enabled: bool,
    pub name: String,
    /// The value text, shown if not empty (the send is touched).
    pub text: String,
    pub value: i32,
    /// The modulated value or -1.
    pub modulated_value: i32,
    pub edited: bool,
}

impl SendData {
    pub fn new(enabled: bool, name: &str, text: &str, value: i32, modulated_value: i32, edited: bool) -> Self {
        SendData { enabled, name: name.to_string(), text: text.to_string(), value, modulated_value, edited }
    }
}

/// The sends of a channel: 4 rows of texts and faders (former `SendsComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct SendsComponent {
    pub channel: ChannelSelectComponent,
    pub send_data: Vec<SendData>,
    pub is_ex_mode: bool,
    pub is_send_active: bool,
}

impl SendsComponent {
    fn modify_if_off(&self, is_enabled: bool, color: ColorEx) -> ColorEx {
        if self.is_send_active && is_enabled {
            color
        } else {
            ColorEx::dim_to_gray(color)
        }
    }

    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        self.channel.draw(info, bounds);

        let footer = &self.channel.menu.footer;
        // Element is off if the name is empty
        if footer.text.is_empty() && !self.is_ex_mode {
            return;
        }

        let dimensions = info.dimensions;
        let configuration = info.configuration;
        let gc = &mut *info.gc;
        let (left, _, width, height) = bounds;
        let separator_size = dimensions.separator_size;
        let menu_height = dimensions.menu_height();
        let unit = dimensions.unit();
        let inset = dimensions.inset();
        let upper_bound = dimensions.max_parameter_value as f64;

        let track_row_height = (1.6 * unit).floor();
        let track_row_top = height - track_row_height - unit - separator_size;
        let slider_width = width - 2.0 * inset - 1.0;
        let t = menu_height + 1.0;
        let h = track_row_top - t;
        let slider_area_height = h;
        // 4 rows of Texts and 4 rows of faders
        let send_row_height = slider_area_height / 8.0;
        let slider_height = send_row_height - 2.0 * separator_size;

        // Background of slider area
        let background_color = self.modify_if_off(true, configuration.color_background);
        let fill = if footer.is_selected || self.is_ex_mode { self.modify_if_off(true, configuration.color_background_lighter) } else { background_color };
        if self.is_ex_mode {
            gc.fill_rectangle(left - separator_size, t, width + separator_size, h - 2.0, fill);
        } else {
            gc.fill_rectangle(left, t, width, h, fill);
        }

        let mut topy = menu_height + if self.is_ex_mode { 0.0 } else { separator_size };
        let fader_left = left + inset;
        for send in &self.send_data {
            if send.name.is_empty() {
                break;
            }
            let text_color = self.modify_if_off(send.enabled, configuration.color_text);
            let border_color = self.modify_if_off(send.enabled, configuration.color_border);
            let fader_color = self.modify_if_off(send.enabled, configuration.color_fader);
            let edit_color = self.modify_if_off(send.enabled, configuration.color_edit);

            gc.draw_text_in_bounds(&send.name, fader_left, topy + separator_size, slider_width, send_row_height, Align::Left, text_color, None, send_row_height);
            topy += send_row_height;
            gc.fill_rectangle(fader_left, topy + separator_size, slider_width, slider_height, border_color);

            let value_width = send.value as f64 * slider_width / upper_bound;
            let is_send_modulated = send.modulated_value != -1;
            let modulated_value_width = if is_send_modulated { send.modulated_value as f64 * slider_width / upper_bound } else { value_width };
            let fader_top = topy + separator_size + 1.0;
            gc.fill_rectangle(fader_left + 1.0, fader_top, modulated_value_width - 1.0, slider_height - 2.0, fader_color);

            if send.edited {
                let is_touched = !send.text.is_empty();
                let w = if is_touched { 3.0 } else { 1.0 };
                gc.fill_rectangle((fader_left + slider_width - w - 1.0).min(fader_left + value_width + 1.0), fader_top, w, slider_height - 2.0, edit_color);
            }

            topy += send_row_height;
        }

        // Draw volume text on top if set
        let box_width = slider_width / 2.0;
        let box_left = fader_left + slider_width - box_width;
        topy = menu_height;
        let background_darker = self.modify_if_off(true, configuration.color_background_darker);
        for send in &self.send_data {
            topy += send_row_height;
            if !send.text.is_empty() {
                let text_color = self.modify_if_off(send.enabled, configuration.color_text);
                let border_color = self.modify_if_off(send.enabled, configuration.color_border);
                let volume_text_top = topy + slider_height + 1.0 + if self.is_ex_mode { 0.0 } else { separator_size };
                gc.fill_rectangle(box_left, volume_text_top, box_width, unit, background_darker);
                gc.stroke_rectangle(box_left, volume_text_top, box_width - 1.0, unit, border_color);
                gc.draw_text_in_bounds(&send.text, box_left, volume_text_top, box_width, unit, Align::Center, text_color, None, unit);
            }
            topy += send_row_height;
        }
    }
}

// ----------------------------------------------------------------------------------------------
// Parameter

/// A parameter with name, value text and a horizontal slider (former `ParameterComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterComponent {
    pub menu: MenuComponent,
    pub param_name: String,
    pub param_value_text: String,
    /// The value or -1 if there is no value (no slider is drawn).
    pub param_value: i32,
    /// The modulated value or -1.
    pub modulated_param_value: i32,
    pub is_touched: bool,
}

impl ParameterComponent {
    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        self.menu.draw(info, bounds);

        let dimensions = info.dimensions;
        let configuration = info.configuration;
        let gc = &mut *info.gc;
        let (left, _, width, height) = bounds;
        let separator_size = dimensions.separator_size;
        let menu_height = dimensions.menu_height();
        let unit = dimensions.unit();
        let controls_top = dimensions.controls_top();
        let inset = dimensions.inset();

        let is_value_missing = self.param_value == -1;
        let is_modulated = self.modulated_param_value != -1;

        let track_row_height = (1.6 * unit).floor();
        let track_row_top = height - track_row_height - unit - separator_size;

        // Component is off if the name is empty
        if self.param_name.is_empty() {
            return;
        }

        let element_width = width - 2.0 * inset;
        let element_height = (track_row_top - controls_top - inset) / 3.0;

        // Draw the background
        let background_color = configuration.color_background;
        let fill = if self.is_touched { configuration.color_background_lighter } else { background_color };
        gc.fill_rectangle(left, menu_height + 1.0, width, track_row_top - if is_value_missing { controls_top + element_height } else { menu_height + 1.0 }, fill);

        // Draw the name and value texts
        let text_color = configuration.color_text;
        let font_size = element_height * 2.0 / 3.0;
        let label_font_size = element_height / 2.0;
        gc.draw_text_in_bounds(&self.param_name, left + inset - 1.0, controls_top - inset, element_width, element_height, Align::Center, text_color, None, label_font_size);
        gc.draw_text_in_bounds(&self.param_value_text, left + inset - 1.0, controls_top - inset + element_height, element_width, element_height, Align::Center, text_color, None, font_size);

        // Value slider
        if is_value_missing {
            return;
        }
        let element_inner_width = element_width - 2.0;
        let max_value = dimensions.max_parameter_value as f64;
        let value = if is_modulated { self.modulated_param_value } else { self.param_value } as f64;
        let value_slider_width = if value >= max_value - 1.0 { element_inner_width } else { element_inner_width * value / max_value };
        let inner_top = controls_top + 2.0 * element_height + 1.0;
        let border_color = configuration.color_border;
        gc.fill_rectangle(left + inset - 1.0, controls_top + 2.0 * element_height, element_width, element_height, border_color);
        gc.fill_rectangle(left + inset, inner_top, value_slider_width, element_height - 2.0, configuration.color_fader);

        let w = if self.is_touched { 3.0 } else { 1.0 };
        let param_value = self.param_value as f64;
        let value_width = if param_value >= max_value - 1.0 { element_inner_width } else { element_inner_width * param_value / max_value };
        gc.fill_rectangle(left + inset + (value_width - w).max(0.0), inner_top, w, element_height - 2.0, configuration.color_edit);
    }
}

// ----------------------------------------------------------------------------------------------
// Options

/// A top and a bottom menu with optional header texts above them (former `OptionsComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct OptionsComponent {
    pub header: LabelComponent,
    pub footer: LabelComponent,
    pub header_top: String,
    pub header_bottom: String,
    pub is_bottom_header_selected: bool,
}

impl OptionsComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        header_top: &str,
        menu_top_name: &str,
        is_menu_top_selected: bool,
        menu_top_color: Option<ColorEx>,
        header_bottom: &str,
        menu_bottom_name: &str,
        is_menu_bottom_selected: bool,
        menu_bottom_color: Option<ColorEx>,
        use_small_top_menu: bool,
        is_bottom_header_selected: bool,
    ) -> Self {
        let top_layout = if use_small_top_menu { LabelLayout::SmallHeader } else { LabelLayout::Plain };
        OptionsComponent {
            header: LabelComponent::new(menu_top_name, None, menu_top_color, is_menu_top_selected, true, top_layout),
            footer: LabelComponent::new(menu_bottom_name, None, menu_bottom_color, is_menu_bottom_selected, true, LabelLayout::Plain),
            header_top: header_top.to_string(),
            header_bottom: header_bottom.to_string(),
            is_bottom_header_selected,
        }
    }

    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        let menu_height = 2.0 * info.dimensions.menu_height();
        let configuration = info.configuration;

        self.header.draw(info, with_top_height(bounds, 0.0, menu_height));
        let (left, _, _, height) = bounds;
        self.footer.draw(info, with_top_height(bounds, height - menu_height, menu_height));

        let has_top_header = !self.header_top.is_empty();
        let has_bottom_header = !self.header_bottom.is_empty();
        if !has_top_header && !has_bottom_header {
            return;
        }

        let header_height = (height - 2.0 * menu_height) / 2.0;
        let mut text_color = configuration.color_text;
        let border_color = configuration.color_border;
        if text_color == border_color {
            text_color = ColorEx::calc_contrast_color(text_color);
        }
        let gc = &mut *info.gc;
        if has_top_header {
            gc.draw_text_in_height(&self.header_top, left, menu_height, header_height, text_color, None, header_height / 2.0);
        }
        if has_bottom_header {
            if self.is_bottom_header_selected {
                gc.draw_text_in_height(&self.header_bottom, left, menu_height + header_height, header_height, ColorEx::calc_contrast_color(text_color), Some(text_color), header_height / 2.0);
            } else {
                gc.draw_text_in_height(&self.header_bottom, left, menu_height + header_height, header_height, text_color, None, header_height / 2.0);
            }
        }
    }
}

// ----------------------------------------------------------------------------------------------
// Scene and clip lists

/// The data of a scene for the scene list (the values of `IScene` which
/// `SceneListGridElement` uses).
#[derive(Debug, Clone, PartialEq)]
pub struct SceneInfo {
    pub name: String,
    pub color: ColorEx,
    pub exists: bool,
    pub is_selected: bool,
}

impl SceneInfo {
    pub fn from_scene(scene: &Scene) -> Self {
        SceneInfo { name: scene.name(), color: scene.color(), exists: scene.does_exist(), is_selected: scene.is_selected() }
    }
}

/// A list of scenes (former `SceneListGridElement`).
#[derive(Debug, Clone, PartialEq)]
pub struct SceneListComponent {
    pub channel: ChannelSelectComponent,
    pub scenes: Vec<SceneInfo>,
}

impl SceneListComponent {
    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        self.channel.draw(info, bounds);

        let dimensions = info.dimensions;
        let configuration = info.configuration;
        let gc = &mut *info.gc;
        let (left, _, width, height) = bounds;
        let separator_size = dimensions.separator_size;
        let inset = dimensions.inset();

        let size = self.scenes.len();
        let item_left = left + separator_size;
        let item_width = width - separator_size;
        let item_height = height / (size + 1) as f64;
        let text_color = configuration.color_text;
        let border_color = configuration.color_background_lighter;
        let font_height = if item_height > 30.0 { item_height / 2.0 } else { item_height * 2.0 / 3.0 };

        for (i, scene) in self.scenes.iter().enumerate() {
            let item_top = i as f64 * item_height;
            let background_color = scene.color;
            gc.fill_rectangle(item_left, item_top + separator_size, item_width, item_height - 2.0 * separator_size, background_color);
            if scene.exists {
                gc.draw_text_in_bounds(&scene.name, item_left + inset, item_top - 1.0, item_width - 2.0 * inset, item_height, Align::Left, ColorEx::calc_contrast_color(background_color), None, font_height);
            }
            let (color, line_width) = if scene.is_selected { (text_color, 2.0) } else { (border_color, 1.0) };
            gc.stroke_rectangle_with_width(item_left, item_top + separator_size, item_width, item_height - 2.0 * separator_size, color, line_width);
        }
    }
}

/// The data of a track and one of its clip slots for the clip list (the values of `ITrack` and
/// `ISlot` which `ClipListComponent` uses).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlotInfo {
    pub track_exists: bool,
    pub track_is_group: bool,
    pub track_is_rec_arm: bool,
    pub exists: bool,
    pub name: String,
    pub color: ColorEx,
    /// The absolute position of the slot (for the "Scene n" name of group slots).
    pub position: i32,
    pub is_selected: bool,
    pub has_content: bool,
    pub is_playing: bool,
    pub is_playing_queued: bool,
    pub is_recording: bool,
    pub is_recording_queued: bool,
    pub is_stop_queued: bool,
}

impl SlotInfo {
    pub fn from_track_slot(track: &Track, slot: &Slot) -> Self {
        SlotInfo {
            track_exists: track.does_exist(),
            track_is_group: track.is_group(),
            track_is_rec_arm: track.is_rec_arm(),
            exists: slot.does_exist(),
            name: slot.name().to_string(),
            color: slot.color(),
            position: slot.position(),
            is_selected: slot.is_selected(),
            has_content: slot.has_content(),
            is_playing: slot.is_playing(),
            is_playing_queued: slot.is_playing_queued(),
            is_recording: slot.is_recording(),
            is_recording_queued: slot.is_recording_queued(),
            is_stop_queued: slot.is_stop_queued(),
        }
    }
}

/// A list of clip slots (former `ClipListComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClipListComponent {
    pub channel: ChannelSelectComponent,
    pub slots: Vec<SlotInfo>,
}

impl ClipListComponent {
    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        self.channel.draw(info, bounds);

        let dimensions = info.dimensions;
        let configuration = info.configuration;
        let gc = &mut *info.gc;
        let (left, _, width, height) = bounds;
        let separator_size = dimensions.separator_size / 2.0;
        let inset = dimensions.inset();

        let size = self.slots.len();
        let item_left = left;
        let item_width = width;
        let item_height = height / (size + 1) as f64;
        let font_height = if item_height > 30.0 { item_height / 2.0 } else { item_height * 2.0 / 3.0 };
        let box_left = item_left + inset;
        let box_width = font_height - 2.0 * separator_size;
        let radius = box_width / 2.0;
        let text_color = configuration.color_text;

        for (i, slot) in self.slots.iter().enumerate() {
            if !slot.track_exists {
                continue;
            }
            let item_top = i as f64 * item_height;
            let box_top = item_top + (item_height - font_height) / 2.0;

            // Draw the background
            let clip_background_color = slot.color;
            let mut name = slot.name.clone();
            if slot.track_is_group {
                if name.is_empty() {
                    name = format!("Scene {}", slot.position + 1);
                }
                gc.fill_rectangle(item_left, item_top + separator_size, item_width, item_height - 2.0 * separator_size, ColorEx::darker(ColorEx::DARK_GRAY));
                gc.fill_rectangle(item_left + item_width - 2.0 * inset, item_top + separator_size, 2.0 * inset, item_height - 2.0 * separator_size, clip_background_color);
            } else {
                gc.fill_rectangle(item_left, item_top + separator_size, item_width, item_height - 2.0 * separator_size, clip_background_color);
            }

            if slot.exists {
                // Draw the play/record state indicator box
                let is_playing = slot.is_playing;
                let is_playing_queued = slot.is_playing_queued;
                let is_recording = slot.is_recording;
                let is_recording_queued = slot.is_recording_queued;
                let is_stop_queued = slot.is_stop_queued;
                if is_playing || is_playing_queued || is_recording || is_recording_queued || is_stop_queued {
                    gc.fill_rectangle(box_left, box_top, font_height, font_height, ColorEx::BLACK);
                }

                // Draw the play, record or stop symbol depending on the slots state
                let mut fill_color = ColorEx::darker(clip_background_color);
                if is_recording || is_recording_queued {
                    fill_color = if is_stop_queued { ColorEx::WHITE } else { ColorEx::RED };
                    gc.fill_circle(box_left + separator_size + radius, box_top + separator_size + radius, radius, fill_color);
                } else if slot.has_content {
                    if is_stop_queued {
                        fill_color = ColorEx::WHITE;
                    } else if is_playing || is_playing_queued {
                        fill_color = if is_playing_queued { ColorEx::DARKER_GREEN } else { ColorEx::GREEN };
                    }
                    gc.fill_triangle(
                        box_left + separator_size,
                        box_top + separator_size,
                        box_left + separator_size,
                        box_top + font_height - separator_size,
                        box_left + font_height - separator_size,
                        box_top + font_height / 2.0,
                        fill_color,
                    );
                } else if slot.track_is_rec_arm {
                    gc.fill_circle(box_left + separator_size + radius, box_top + separator_size + radius, radius, ColorEx::DARK_GRAY);
                } else {
                    gc.fill_rectangle(box_left + separator_size, box_top + separator_size, box_width, box_width, ColorEx::DARK_GRAY);
                }

                // Draw the text
                let pad_left = 2.0 * inset + font_height;
                let color = if slot.is_selected { ColorEx::calc_contrast_color(text_color) } else { text_color };
                gc.draw_text_in_bounds(&name, item_left + pad_left, item_top - 1.0, item_width - pad_left - separator_size - inset, item_height, Align::Left, color, None, font_height);
            }

            // Draw the border if selected
            if slot.is_selected {
                gc.stroke_rectangle_with_width(item_left, item_top + separator_size, item_width, item_height - 2.0 * separator_size, ColorEx::calc_contrast_color(text_color), 2.0);
            }
        }
    }
}

// ----------------------------------------------------------------------------------------------
// MIDI clip

/// A step of a note clip which is not off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiStepInfo {
    pub channel: i32,
    /// The step relative to the edit page.
    pub step: i32,
    pub note: i32,
    pub state: StepState,
    pub is_selected: bool,
    pub is_muted: bool,
}

/// The data of a note clip for the MIDI clip component (the values of `INoteClip` which
/// `MidiClipComponent` uses).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MidiClipInfo {
    pub color: ColorEx,
    pub num_steps: i32,
    pub step_length: f64,
    pub edit_page: i32,
    pub is_loop_enabled: bool,
    pub loop_start: f64,
    pub loop_length: f64,
    pub play_start: f64,
    pub play_end: f64,
    /// The lowest note with data or -1.
    pub lowest_row_with_data: i32,
    pub highest_row_with_data: i32,
    /// The played step or -1.
    pub current_step: i32,
    /// The steps which are not off, sorted by note, step and channel.
    pub steps: Vec<MidiStepInfo>,
}

impl MidiClipInfo {
    /// Get the data of a cursor clip (of its edit page).
    pub fn from_clip(clip: &CursorClip) -> Self {
        let num_steps = clip.num_steps() as i32;
        let lowest = clip.lowest_row_with_data();
        let highest = clip.highest_row_with_data();
        let mut steps = Vec::new();
        if lowest >= 0 {
            for note in lowest..=highest {
                for step in 0..num_steps {
                    for channel in 0..NUM_CHANNELS as i32 {
                        let info = clip.step(&NotePosition::new(channel, step, note));
                        if info.state() != StepState::Off {
                            steps.push(MidiStepInfo { channel, step, note, state: info.state(), is_selected: info.is_selected(), is_muted: info.is_muted() });
                        }
                    }
                }
            }
        }
        MidiClipInfo {
            color: clip.color(),
            num_steps,
            step_length: clip.step_length(),
            edit_page: clip.edit_page(),
            is_loop_enabled: clip.is_loop_enabled(),
            loop_start: clip.loop_start(),
            loop_length: clip.loop_length(),
            play_start: clip.play_start(),
            play_end: clip.play_end(),
            lowest_row_with_data: lowest,
            highest_row_with_data: highest,
            current_step: clip.current_step(),
            steps,
        }
    }
}

/// The notes of a clip page as a piano roll (former `MidiClipComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct MidiClipComponent {
    pub clip: MidiClipInfo,
    pub quarters_per_measure: i32,
    pub active_position: Option<NotePosition>,
}

impl MidiClipComponent {
    pub fn draw(&self, info: &mut DrawInfo, bounds: Bounds) {
        let configuration = info.configuration;
        let clip = &self.clip;
        let grid_background = configuration.color_background_lighter;
        let measure_text_color = ColorEx::WHITE;
        let dividers_color = configuration.color_background_darker;
        let clip_color = clip.color;
        let note_color = clip.color;
        let note_active_color = ColorEx::WHITE;
        let note_muted_color = ColorEx::DARK_GRAY;
        let note_grid_loop_color = configuration.color_background;
        let note_border_color = ColorEx::BLACK;

        let gc = &mut *info.gc;
        let (left, _, width, height) = bounds;

        let top = 14.0;
        let note_area_height = height - top;

        // Draw the background
        gc.fill_rectangle(left, top, width, note_area_height, grid_background);

        // Draw the loop, if any and ...
        let num_steps = clip.num_steps.max(1);
        let step_length = clip.step_length;
        let page_length = num_steps as f64 * step_length;
        let edit_page = clip.edit_page as f64;
        let start_pos = edit_page * page_length;
        let end_pos = (edit_page + 1.0) * page_length;
        let len = top - 1.0;
        if clip.is_loop_enabled {
            let loop_start = clip.loop_start;
            let loop_length = clip.loop_length;
            // ... the loop is visible in the current page
            if loop_start < end_pos && loop_start + loop_length > start_pos {
                let start = (loop_start - start_pos).max(0.0);
                let end = end_pos.min(loop_start + loop_length) - start_pos;
                let x = width * start / page_length;
                let w = width * end / page_length - x;
                // The header loop
                gc.fill_rectangle(x + 1.0, 0.0, w, len, clip_color);
                // Background in note area
                gc.fill_rectangle(x + 1.0, top, w, note_area_height, note_grid_loop_color);
            }
        }

        // Draw play start in header
        let play_start = clip.play_start;
        if play_start >= start_pos && play_start <= end_pos {
            let x = width * (play_start - start_pos) / page_length;
            gc.fill_triangle(x + 1.0, 0.0, x + 1.0 + len, len / 2.0, x + 1.0, len, clip_color);
            gc.stroke_triangle(x + 1.0, 0.0, x + 1.0 + len, len / 2.0, x + 1.0, len, ColorEx::even_darker(clip_color));
        }

        // Draw play end in header
        let play_end = clip.play_end;
        if play_end >= start_pos && play_end <= end_pos {
            let x = width * (play_end - start_pos) / page_length;
            gc.fill_triangle(x + 1.0, 0.0, x + 1.0, len, x + 1.0 - top, len / 2.0, clip_color);
            gc.stroke_triangle(x + 1.0, 0.0, x + 1.0, len, x + 1.0 - top, len / 2.0, ColorEx::even_darker(clip_color));
        }

        // Draw dividers
        let step_width = width / num_steps as f64;
        for step in 0..=num_steps {
            let x = left + step as f64 * step_width;
            gc.fill_rectangle(x, top, 1.0, note_area_height, dividers_color);

            // Draw measure texts
            if step % 4 == 0 {
                let time = start_pos + step as f64 * step_length;
                let measure_text = string_utils::format_measures(self.quarters_per_measure, time, 1, false);
                gc.draw_text_in_height(&measure_text, x, 0.0, top - 1.0, measure_text_color, None, top);
            }
        }

        // Draw the notes
        let lower_row_with_data = clip.lowest_row_with_data;
        if lower_row_with_data == -1 {
            return;
        }
        let upper_row_with_data = clip.highest_row_with_data;
        // Display at least 4 rows
        let range = 4.max(1 + upper_row_with_data - lower_row_with_data);
        let step_height = note_area_height / range as f64;
        let font_size = gc.calculate_font_size("G#5", step_height, step_width, 12.0);

        for row in 0..range {
            let note = lower_row_with_data + row;
            let row_top = top + (range - row - 1) as f64 * step_height;
            gc.fill_rectangle(left, row_top, width, 1.0, dividers_color);

            for step_info in clip.steps.iter().filter(|s| s.note == note && s.step >= 0 && s.step < num_steps) {
                let mut x = left + step_info.step as f64 * step_width - 1.0;
                let mut w = step_width + 2.0;
                let is_start = step_info.state == StepState::Start;
                if is_start {
                    x += 2.0;
                    w -= 2.0;
                }

                let position = NotePosition::new(step_info.channel, step_info.step, note);
                let step_note_background_color = if self.active_position == Some(position) {
                    note_active_color
                } else if step_info.is_muted {
                    note_muted_color
                } else if step_info.is_selected {
                    ColorEx::even_darker(note_color)
                } else {
                    note_color
                };

                gc.stroke_rectangle(x, row_top + 2.0, w, step_height - 2.0, note_border_color);
                let offset = if is_start { 0.0 } else { -2.0 };
                gc.fill_rectangle(x + offset, row_top + 2.0, w - 1.0 - offset, step_height - 3.0, step_note_background_color);

                if is_start && font_size > 0.0 {
                    let text = format!("{}: {}", step_info.channel + 1, Scales::format_drum_note(note));
                    let text_color = ColorEx::calc_contrast_color(note_color);
                    gc.draw_text_in_bounds(&text, x, row_top + 2.0, w - 1.0, step_height - 3.0, Align::Center, text_color, None, font_size);
                }
            }
        }

        // Draw the play cursor
        let play_step = clip.current_step;
        if play_step >= 0 {
            gc.fill_rectangle(left + play_step as f64 * step_width - 1.0, 0.0, 3.0, height, measure_text_color);
        }
    }
}

// ----------------------------------------------------------------------------------------------
// Graph overlay

/// A line graph drawn on top of the components (former `GraphOverlayComponent`).
#[derive(Debug, Clone, PartialEq)]
pub struct GraphOverlayComponent {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: ColorEx,
    pub data: Vec<i32>,
    pub max_value: i32,
}

impl GraphOverlayComponent {
    pub fn draw(&self, info: &mut DrawInfo) {
        let gc = &mut *info.gc;
        gc.fill_rectangle(self.x - 1.0, self.y - 1.0, self.width + 2.0, self.height + 2.0, info.configuration.color_border);
        let Some(&first) = self.data.first() else {
            return;
        };

        // Step sizes for scaling
        let x_step = self.width / (self.data.len() as f64 - 1.0);
        let y_scale = self.height / self.max_value as f64;

        // Previous point (scaled)
        let mut prev_x = self.x;
        let mut prev_y = self.y + self.height - (first as f64 * y_scale).round();
        for (i, &value) in self.data.iter().enumerate().skip(1) {
            let curr_x = self.x + (i as f64 * x_step).round();
            let curr_y = self.y + self.height - (value as f64 * y_scale).round();
            gc.draw_line(prev_x, prev_y, curr_x, curr_y, self.color);
            prev_x = curr_x;
            prev_y = curr_y;
        }
    }
}
