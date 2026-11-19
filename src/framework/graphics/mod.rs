// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Graphics displays (former `framework.graphics` and
//! `framework.controller.display.AbstractGraphicDisplay`): a RGB bitmap with a simple
//! graphics context, an embedded bitmap font, the display components and the display which
//! renders the components and a notification into the bitmap.
//!
//! The controller converts the bitmap into its own format (e.g. the 1 bit OLED stripes of the
//! Akai Fire). See [`bitmap`] for the differences in drawing compared to the Java version
//! (no anti-aliasing, bitmap font instead of AWT/Cairo fonts) and [`component`] for the ported
//! components.
//!
//! # API overview (former `IGraphicDisplay`)
//!
//! A mode adds the components of one frame (the columns, drawn side by side, and overlays) to
//! the [`GraphicDisplay`], the flush renders them:
//!
//! - Columns: [`GraphicDisplay::add_empty_element`], `add_channel_selector_element`,
//!   `add_channel_element` (+ `_typed` with a `GRID_ELEMENT_CHANNEL_*` value),
//!   `add_sends_element` (with [`SendData`]), `add_parameter_element` (+ `_plain`,
//!   `_with_plain_menu`, `_with_device_name`), `add_option_element` (+ `_colored`,
//!   `_colored_with_header_selection`), `add_list_element` (+ `_sized`),
//!   `add_scene_list_element` (with [`SceneInfo`]), `add_slot_list_element` (with
//!   [`SlotInfo`]), `set_midi_clip_element` (with [`MidiClipInfo`]), `set_message` and
//!   `add_element` for any [`Component`].
//! - Overlays: [`GraphicDisplay::add_graph_overlay`].
//! - [`GraphicDisplay::set_notification_message`] shows a text on top for
//!   [`NOTIFICATION_TIME`].
//! - [`GraphicDisplay::render`] (former `send`) renders the image if anything changed;
//!   [`GraphicDisplay::has_changed`] and [`GraphicDisplay::generation`] tell the transport if
//!   there is a new image.
//!
//! The model objects of Java (`IScene`, `ITrack`/`ISlot`, `INoteClip`) are replaced by plain
//! data structs which the mode fills (e.g. [`SceneInfo::from_scene`]). The channel type is
//! [`ChannelType`] (`None` for Java `null`).
//!
//! Displays with anti-aliasing ([`GraphicsConfiguration::is_antialias_enabled`], e.g.
//! [`GraphicsConfiguration::push_default`]) are drawn with alpha blending, the embedded DejaVu
//! Sans vector font ([`vector_font`]) and the pre-rendered SVG icons ([`icons`]); the others
//! with the 5x8 bitmap font.
//!
//! Further differences to Java:
//! - The rendering is not done in a separate thread; [`GraphicDisplay::render`] (former
//!   `send`) renders synchronously and only if the components or the notification changed.
//!   Sending the image is up to the controller (e.g. `controller::push::usb_display`).
//! - The notification is shown for [`NOTIFICATION_TIME`] milliseconds (Java: a counter of 1
//!   which is decreased every second, i.e. 1 to 2 seconds).
//! - The debug window (former `showDebugWindow`) does not exist; use
//!   [`Bitmap::to_ascii_art`].

pub mod bitmap;
pub mod canvas;
pub mod component;
pub mod font;
pub mod font_data;
pub mod icon_data;
pub mod icons;
pub mod raster;
pub mod vector_font;

use std::time::{Duration, Instant};

pub use bitmap::{Align, Bitmap, GraphicsContext};
pub use canvas::{
    ChannelComponent, ChannelEditType, ChannelSelectComponent, ClipListComponent, GraphOverlayComponent, LabelComponent, MenuComponent, MidiClipComponent, MidiClipInfo, MidiStepInfo,
    OptionsComponent, ParameterComponent, SceneInfo, SceneListComponent, SendData, SendsComponent, SlotInfo, GRID_ELEMENT_CHANNEL_ALL, GRID_ELEMENT_CHANNEL_CROSSFADER, GRID_ELEMENT_CHANNEL_PAN,
    GRID_ELEMENT_CHANNEL_SELECTION, GRID_ELEMENT_CHANNEL_SENDS, GRID_ELEMENT_CHANNEL_VOLUME, GRID_ELEMENT_LIST, GRID_ELEMENT_OPTIONS, GRID_ELEMENT_PARAMETERS,
};
pub use component::{Component, LabelLayout};
pub use icons::Icon;

use crate::daw::clip::NotePosition;
use crate::daw::color::ColorEx;
pub use crate::daw::constants::ChannelType;

/// The time a notification is shown, in milliseconds.
pub const NOTIFICATION_TIME: u64 = 1500;

/// The colors of a graphics display (former `IGraphicsConfiguration`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphicsConfiguration {
    pub color_text: ColorEx,
    pub color_background: ColorEx,
    pub color_background_darker: ColorEx,
    pub color_background_lighter: ColorEx,
    pub color_border: ColorEx,
    pub color_edit: ColorEx,
    pub color_fader: ColorEx,
    pub color_vu: ColorEx,
    pub color_record: ColorEx,
    pub color_solo: ColorEx,
    pub color_mute: ColorEx,
    /// Draw with anti-aliasing, the vector font and alpha blended icons (former
    /// `isAntialiasEnabled`).
    pub is_antialias_enabled: bool,
}

impl GraphicsConfiguration {
    /// The same colors with anti-aliasing turned on or off.
    pub fn with_antialias(self, is_antialias_enabled: bool) -> Self {
        GraphicsConfiguration { is_antialias_enabled, ..self }
    }

    /// The default colors of the Push 2/3 display (former `DEFAULT_COLOR_*` of
    /// `AbstractConfiguration`) with anti-aliasing (former `PushConfiguration`). The Push
    /// configuration overrides the colors with its color settings:
    /// `GraphicsConfiguration { color_text: ..., ..GraphicsConfiguration::push_default() }`.
    pub fn push_default() -> Self {
        GraphicsConfiguration {
            color_text: ColorEx::BLACK,
            color_background: ColorEx::from_rgb(83, 83, 83),
            color_background_darker: ColorEx::from_rgb(39, 39, 39),
            color_background_lighter: ColorEx::from_rgb(200, 200, 200),
            color_border: ColorEx::BLACK,
            color_edit: ColorEx::from_rgb(240, 127, 17),
            color_fader: ColorEx::from_rgb(69, 44, 19),
            color_vu: ColorEx::GREEN,
            color_record: ColorEx::RED,
            color_solo: ColorEx::YELLOW,
            color_mute: ColorEx::from_rgb(245, 129, 17),
            is_antialias_enabled: true,
        }
    }

    /// White on black for monochrome displays (former `ChromaticGraphicsConfiguration`).
    pub fn chromatic() -> Self {
        GraphicsConfiguration {
            color_text: ColorEx::WHITE,
            color_background: ColorEx::BLACK,
            color_background_darker: ColorEx::BLACK,
            color_background_lighter: ColorEx::BLACK,
            color_border: ColorEx::BLACK,
            color_edit: ColorEx::WHITE,
            color_fader: ColorEx::WHITE,
            color_vu: ColorEx::WHITE,
            color_record: ColorEx::WHITE,
            color_solo: ColorEx::WHITE,
            color_mute: ColorEx::WHITE,
            is_antialias_enabled: false,
        }
    }
}

/// The dimensions of a graphics display (former `DefaultGraphicsDimensions`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphicsDimensions {
    pub width: i32,
    pub height: i32,
    pub separator_size: f64,
    pub max_parameter_value: i32,
}

impl GraphicsDimensions {
    pub fn new(width: i32, height: i32, max_parameter_value: i32) -> Self {
        GraphicsDimensions { width, height, separator_size: 2.0, max_parameter_value }
    }

    pub fn unit(&self) -> f64 {
        self.height as f64 / 12.0
    }

    pub fn half_unit(&self) -> f64 {
        self.unit() / 2.0
    }

    pub fn double_unit(&self) -> f64 {
        2.0 * self.unit()
    }

    pub fn menu_height(&self) -> f64 {
        self.unit() + 2.0 * self.separator_size
    }

    pub fn inset(&self) -> f64 {
        self.separator_size / 2.0 + self.half_unit()
    }

    pub fn controls_top(&self) -> f64 {
        self.menu_height() + self.inset()
    }
}

/// The data which was rendered last (former `ModelInfo`).
#[derive(Debug, Clone, PartialEq)]
struct ModelInfo {
    notification: Option<String>,
    components: Vec<Component>,
    overlays: Vec<GraphOverlayComponent>,
}

/// A graphics display (former `AbstractGraphicDisplay`).
pub struct GraphicDisplay {
    configuration: GraphicsConfiguration,
    dimensions: GraphicsDimensions,
    columns: Vec<Component>,
    overlays: Vec<GraphOverlayComponent>,
    info: Option<ModelInfo>,
    notification: Option<String>,
    notification_end: Option<Instant>,
    image: Bitmap,
    changed: bool,
    generation: u64,
}

impl GraphicDisplay {
    pub fn new(configuration: GraphicsConfiguration, dimensions: GraphicsDimensions) -> Self {
        GraphicDisplay {
            configuration,
            dimensions,
            columns: Vec::new(),
            overlays: Vec::new(),
            info: None,
            notification: None,
            notification_end: None,
            image: Bitmap::new(dimensions.width, dimensions.height),
            changed: false,
            generation: 0,
        }
    }

    pub fn configuration(&self) -> &GraphicsConfiguration {
        &self.configuration
    }

    /// Change the colors (e.g. after a color setting changed); the next render draws the image
    /// again.
    pub fn set_configuration(&mut self, configuration: GraphicsConfiguration) {
        if self.configuration != configuration {
            self.configuration = configuration;
            self.force_render();
        }
    }

    pub fn dimensions(&self) -> &GraphicsDimensions {
        &self.dimensions
    }

    /// Add a component; the components are drawn side by side (former `addElement`).
    pub fn add_element(&mut self, component: Component) {
        self.columns.push(component);
    }

    /// Add a list of which the given items are selected (former `addListElement (items,
    /// selected)`).
    pub fn add_list_element(&mut self, items: &[String], selected: &[bool]) {
        let items = items.iter().enumerate().map(|(i, item)| (item.clone(), selected.get(i).copied().unwrap_or(false))).collect();
        self.add_element(Component::List { items });
    }

    /// Add a list which shows `display_size` items around the selected one (former
    /// `addListElement (displaySize, elements, selectedIndex)`).
    pub fn add_list_element_sized(&mut self, display_size: usize, elements: &[String], selected_index: i32) {
        let start = (elements.len() as i64 - display_size as i64).clamp(0, selected_index.max(0) as i64) as usize;
        let items = (0..display_size)
            .map(|i| {
                let pos = start + i;
                (elements.get(pos).cloned().unwrap_or_default(), pos as i32 == selected_index)
            })
            .collect();
        self.add_element(Component::List { items });
    }

    /// Show a text in one of 8 columns, the others are empty (former `setMessage`).
    pub fn set_message(&mut self, column: usize, text: &str) -> &mut Self {
        for i in 0..8 {
            self.add_option_element(if column == i { text } else { "" }, "", false, "", "", false, false);
        }
        self
    }

    /// Add an empty column (former `addEmptyElement ()`).
    pub fn add_empty_element(&mut self) {
        self.add_option_element("", "", false, "", "", false, false);
    }

    /// Add an empty column with an empty small top menu (former `addEmptyElement (boolean)`).
    /// As in Java the parameter is ignored: the small menu is always used.
    pub fn add_empty_element_with_small_menu(&mut self, _has_small_empty_menu: bool) {
        self.add_option_element("", " ", false, "", "", false, true);
    }

    /// Add a channel selector (former `addChannelSelectorElement`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_channel_selector_element(&mut self, top_menu: &str, is_top_menu_on: bool, bottom_menu: &str, channel_type: Option<ChannelType>, bottom_menu_color: ColorEx, is_bottom_menu_on: bool, is_active: bool) {
        self.add_element(Component::ChannelSelect(ChannelSelectComponent::new(channel_type, top_menu, is_top_menu_on, bottom_menu, bottom_menu_color, is_bottom_menu_on, is_active, false)));
    }

    /// Add a channel which edits everything (former `addChannelElement` without
    /// `channelType`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_channel_element(
        &mut self,
        top_menu: &str,
        is_top_menu_on: bool,
        bottom_menu: &str,
        channel_type: Option<ChannelType>,
        bottom_menu_color: ColorEx,
        is_bottom_menu_on: bool,
        volume: i32,
        modulated_volume: i32,
        volume_str: &str,
        pan: i32,
        modulated_pan: i32,
        pan_str: &str,
        vu_left: i32,
        vu_right: i32,
        mute: bool,
        solo: bool,
        recarm: bool,
        is_active: bool,
        crossfade_mode: i32,
        is_pinned: bool,
    ) {
        self.add_channel_element_typed(
            GRID_ELEMENT_CHANNEL_ALL,
            top_menu,
            is_top_menu_on,
            bottom_menu,
            channel_type,
            bottom_menu_color,
            is_bottom_menu_on,
            volume,
            modulated_volume,
            volume_str,
            pan,
            modulated_pan,
            pan_str,
            vu_left,
            vu_right,
            mute,
            solo,
            recarm,
            is_active,
            crossfade_mode,
            is_pinned,
        );
    }

    /// Add a channel; `grid_element` (Java `channelType`) is one of the `GRID_ELEMENT_CHANNEL_*`
    /// values and selects what is edited (former `addChannelElement (int channelType, ...)`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_channel_element_typed(
        &mut self,
        grid_element: i32,
        top_menu: &str,
        is_top_menu_on: bool,
        bottom_menu: &str,
        channel_type: Option<ChannelType>,
        bottom_menu_color: ColorEx,
        is_bottom_menu_on: bool,
        volume: i32,
        modulated_volume: i32,
        volume_str: &str,
        pan: i32,
        modulated_pan: i32,
        pan_str: &str,
        vu_left: i32,
        vu_right: i32,
        mute: bool,
        solo: bool,
        recarm: bool,
        is_active: bool,
        crossfade_mode: i32,
        is_pinned: bool,
    ) {
        let icon = icons::channel_icon(channel_type, is_pinned);
        self.add_element(Component::Channel(Box::new(ChannelComponent {
            menu: MenuComponent::new(top_menu, is_top_menu_on, bottom_menu, icon, Some(bottom_menu_color), is_bottom_menu_on, is_active, LabelLayout::Colored),
            edit_type: ChannelEditType::from_grid_element(grid_element),
            channel_type,
            volume_value: volume,
            modulated_volume_value: modulated_volume,
            volume_text: volume_str.to_string(),
            pan_value: pan,
            modulated_pan_value: modulated_pan,
            pan_text: pan_str.to_string(),
            vu_value_left: vu_left,
            vu_value_right: vu_right,
            is_mute: mute,
            is_solo: solo,
            is_arm: recarm,
            is_active,
            crossfade_mode,
        })));
    }

    /// Add the sends of a channel (former `addSendsElement`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_sends_element(
        &mut self,
        top_menu: &str,
        is_top_menu_on: bool,
        bottom_menu: &str,
        channel_type: Option<ChannelType>,
        bottom_menu_color: ColorEx,
        is_bottom_menu_on: bool,
        send_data: &[SendData],
        is_track_mode: bool,
        is_send_active: bool,
        is_channel_label_active: bool,
    ) {
        self.add_element(Component::Sends(Box::new(SendsComponent {
            channel: ChannelSelectComponent::new(channel_type, top_menu, is_top_menu_on, bottom_menu, bottom_menu_color, is_bottom_menu_on, is_channel_label_active, false),
            send_data: send_data.to_vec(),
            is_ex_mode: is_track_mode,
            is_send_active,
        })));
    }

    /// Add a parameter without menus (former `addParameterElement (parameterName, ...)`).
    pub fn add_parameter_element_plain(&mut self, parameter_name: &str, parameter_value: i32, parameter_value_str: &str, parameter_is_active: bool, parameter_modulated_value: i32) {
        self.add_parameter_element("", false, "", None, ColorEx::BLACK, false, parameter_name, parameter_value, parameter_value_str, parameter_is_active, parameter_modulated_value);
    }

    /// Add a parameter with menus and the icon of a channel type (former `addParameterElement
    /// (topMenu, ..., ChannelType type, ...)`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_parameter_element(
        &mut self,
        top_menu: &str,
        is_top_menu_on: bool,
        bottom_menu: &str,
        channel_type: Option<ChannelType>,
        bottom_menu_color: ColorEx,
        is_bottom_menu_on: bool,
        parameter_name: &str,
        parameter_value: i32,
        parameter_value_str: &str,
        parameter_is_active: bool,
        parameter_modulated_value: i32,
    ) {
        let icon = icons::channel_icon(channel_type, false);
        let menu = MenuComponent::new(top_menu, is_top_menu_on, bottom_menu, icon, Some(bottom_menu_color), is_bottom_menu_on, true, LabelLayout::Colored);
        self.add_parameter_component(menu, parameter_name, parameter_value, parameter_value_str, parameter_is_active, parameter_modulated_value);
    }

    /// Add a parameter with a plain bottom menu without icon (former
    /// `addParameterElementWithPlainMenu`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_parameter_element_with_plain_menu(
        &mut self,
        top_menu: &str,
        is_top_menu_on: bool,
        bottom_menu: &str,
        bottom_menu_color: ColorEx,
        is_bottom_menu_on: bool,
        parameter_name: &str,
        parameter_value: i32,
        parameter_value_str: &str,
        parameter_is_active: bool,
        parameter_modulated_value: i32,
    ) {
        let menu = MenuComponent::new(top_menu, is_top_menu_on, bottom_menu, None, Some(bottom_menu_color), is_bottom_menu_on, true, LabelLayout::Plain);
        self.add_parameter_component(menu, parameter_name, parameter_value, parameter_value_str, parameter_is_active, parameter_modulated_value);
    }

    /// Add a parameter with menus and the icon of a device (former `addParameterElement
    /// (topMenu, ..., String deviceName, ...)`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_parameter_element_with_device_name(
        &mut self,
        top_menu: &str,
        is_top_menu_on: bool,
        bottom_menu: &str,
        device_name: &str,
        bottom_menu_color: ColorEx,
        is_bottom_menu_on: bool,
        parameter_name: &str,
        parameter_value: i32,
        parameter_value_str: &str,
        parameter_is_active: bool,
        parameter_modulated_value: i32,
    ) {
        let icon = Some(icons::device_icon(device_name));
        let menu = MenuComponent::new(top_menu, is_top_menu_on, bottom_menu, icon, Some(bottom_menu_color), is_bottom_menu_on, true, LabelLayout::Colored);
        self.add_parameter_component(menu, parameter_name, parameter_value, parameter_value_str, parameter_is_active, parameter_modulated_value);
    }

    fn add_parameter_component(&mut self, menu: MenuComponent, parameter_name: &str, parameter_value: i32, parameter_value_str: &str, parameter_is_active: bool, parameter_modulated_value: i32) {
        self.add_element(Component::Parameter(Box::new(ParameterComponent {
            menu,
            param_name: parameter_name.to_string(),
            param_value_text: parameter_value_str.to_string(),
            param_value: parameter_value,
            modulated_param_value: parameter_modulated_value,
            is_touched: parameter_is_active,
        })));
    }

    /// Add an options column: a top and a bottom menu with headers (former `addOptionElement`
    /// without colors).
    #[allow(clippy::too_many_arguments)]
    pub fn add_option_element(&mut self, header_top_name: &str, menu_top_name: &str, is_menu_top_selected: bool, header_bottom_name: &str, menu_bottom_name: &str, is_menu_bottom_selected: bool, use_small_top_menu: bool) {
        self.add_option_element_colored(header_top_name, menu_top_name, is_menu_top_selected, None, header_bottom_name, menu_bottom_name, is_menu_bottom_selected, None, use_small_top_menu);
    }

    /// Add an options column with menu colors (former `addOptionElement` with colors).
    #[allow(clippy::too_many_arguments)]
    pub fn add_option_element_colored(
        &mut self,
        header_top_name: &str,
        menu_top_name: &str,
        is_menu_top_selected: bool,
        menu_top_color: Option<ColorEx>,
        header_bottom_name: &str,
        menu_bottom_name: &str,
        is_menu_bottom_selected: bool,
        menu_bottom_color: Option<ColorEx>,
        use_small_top_menu: bool,
    ) {
        self.add_option_element_colored_with_header_selection(
            header_top_name,
            menu_top_name,
            is_menu_top_selected,
            menu_top_color,
            header_bottom_name,
            menu_bottom_name,
            is_menu_bottom_selected,
            menu_bottom_color,
            use_small_top_menu,
            false,
        );
    }

    /// Add an options column with menu colors and a selectable bottom header (former
    /// `addOptionElement (..., isBottomHeaderSelected)`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_option_element_colored_with_header_selection(
        &mut self,
        header_top_name: &str,
        menu_top_name: &str,
        is_menu_top_selected: bool,
        menu_top_color: Option<ColorEx>,
        header_bottom_name: &str,
        menu_bottom_name: &str,
        is_menu_bottom_selected: bool,
        menu_bottom_color: Option<ColorEx>,
        use_small_top_menu: bool,
        is_bottom_header_selected: bool,
    ) {
        self.add_element(Component::Options(Box::new(OptionsComponent::new(
            header_top_name,
            menu_top_name,
            is_menu_top_selected,
            menu_top_color,
            header_bottom_name,
            menu_bottom_name,
            is_menu_bottom_selected,
            menu_bottom_color,
            use_small_top_menu,
            is_bottom_header_selected,
        ))));
    }

    /// Add a list of scenes (former `addSceneListElement`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_scene_list_element(&mut self, scenes: Vec<SceneInfo>, channel_type: Option<ChannelType>, name: &str, color: ColorEx, is_selected: bool, is_active: bool, is_pinned: bool) {
        let channel = ChannelSelectComponent::new(channel_type, "", false, name, color, is_selected, is_active, is_pinned);
        self.add_element(Component::SceneList(Box::new(SceneListComponent { channel, scenes })));
    }

    /// Add a list of clip slots (former `addSlotListElement`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_slot_list_element(&mut self, slots: Vec<SlotInfo>, channel_type: Option<ChannelType>, name: &str, color: ColorEx, is_selected: bool, is_active: bool, is_pinned: bool) {
        let channel = ChannelSelectComponent::new(channel_type, "", false, name, color, is_selected, is_active, is_pinned);
        self.add_element(Component::ClipList(Box::new(ClipListComponent { channel, slots })));
    }

    /// Add the piano roll of a note clip (former `setMidiClipElement`).
    pub fn set_midi_clip_element(&mut self, clip: MidiClipInfo, quarters_per_measure: i32, active_position: Option<NotePosition>) {
        self.add_element(Component::MidiClip(Box::new(MidiClipComponent { clip, quarters_per_measure, active_position })));
    }

    /// Add a line graph on top of the columns (former `addGraphOverlay`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_graph_overlay(&mut self, x: i32, y: i32, width: i32, height: i32, color: ColorEx, data: &[i32], max_value: i32) {
        self.overlays.push(GraphOverlayComponent { x: x as f64, y: y as f64, width: width as f64, height: height as f64, color, data: data.to_vec(), max_value });
    }

    /// Show a notification on top of the components (former `setNotificationMessage`).
    pub fn set_notification_message(&mut self, message: &str, now: Instant) {
        self.notification = Some(message.to_string());
        self.notification_end = Some(now + Duration::from_millis(NOTIFICATION_TIME));
    }

    pub fn cancel_notification(&mut self) {
        self.notification = None;
        self.notification_end = None;
    }

    pub fn is_notification_active(&self, now: Instant) -> bool {
        self.notification_end.is_some_and(|end| now < end)
    }

    /// The current notification message, if any.
    pub fn notification(&self) -> Option<&str> {
        self.notification.as_deref()
    }

    /// The components which were added since the last render.
    pub fn components(&self) -> &[Component] {
        &self.columns
    }

    /// Render the added components and the notification into the bitmap if anything changed,
    /// clear the components and return the bitmap (former `send`). [`Self::has_changed`] tells
    /// if the image was rendered.
    pub fn render(&mut self, now: Instant) -> &Bitmap {
        if !self.is_notification_active(now) {
            self.notification = None;
            self.notification_end = None;
        }
        let new_info = ModelInfo { notification: self.notification.clone(), components: std::mem::take(&mut self.columns), overlays: std::mem::take(&mut self.overlays) };
        self.changed = self.info.as_ref() != Some(&new_info);
        if self.changed {
            self.render_image(&new_info);
            self.info = Some(new_info);
            self.generation += 1;
        }
        &self.image
    }

    /// True if the last [`Self::render`] produced a new image.
    pub fn has_changed(&self) -> bool {
        self.changed
    }

    /// The number of images rendered so far; changes with every new image.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Render the next time even if nothing changed.
    pub fn force_render(&mut self) {
        self.info = None;
    }

    /// The last rendered bitmap.
    pub fn image(&self) -> &Bitmap {
        &self.image
    }

    fn render_image(&mut self, info: &ModelInfo) {
        let configuration = self.configuration;
        let dimensions = self.dimensions;
        let width = dimensions.width;
        let height = dimensions.height;
        let mut gc = self.image.context_with_antialias(configuration.is_antialias_enabled);

        // Clear display
        gc.fill_rectangle(0.0, 0.0, width as f64, height as f64, configuration.color_border);

        // Draw the component elements
        let size = info.components.len() as i32;
        if size > 0 {
            let grid_width = width / size;
            let separator_size = dimensions.separator_size;
            let paint_width = grid_width as f64 - separator_size;
            let offset_x = separator_size / 2.0;
            for (i, component) in info.components.iter().enumerate() {
                let bounds = ((i as i32 * grid_width) as f64 + offset_x, 0.0, paint_width, height as f64);
                component.draw(&mut gc, bounds, &configuration, &dimensions);
            }
        }

        // Draw overlays (Java draws them only if there are components)
        if size > 0 {
            let mut draw_info = canvas::DrawInfo { gc: &mut gc, configuration: &configuration, dimensions: &dimensions };
            for overlay in &info.overlays {
                overlay.draw(&mut draw_info);
            }
        }

        // Draw an overlay notification
        if let Some(notification) = &info.notification {
            let color_text = configuration.color_text;
            let background = ColorEx::calc_contrast_color(color_text);
            gc.draw_text_in_bounds(notification, 0.0, 0.0, width as f64, height as f64, Align::Center, color_text, Some(background), height as f64 / 4.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display() -> GraphicDisplay {
        GraphicDisplay::new(GraphicsConfiguration::chromatic(), GraphicsDimensions::new(128, 64, 1024))
    }

    #[test]
    fn render_components() {
        let mut display = display();
        let now = Instant::now();
        assert_eq!(display.render(now), &Bitmap::new(128, 64));

        // A volume fader at 50%
        display.add_element(Component::title_value("1: Drums", "Vol: 0 dB", 512, false));
        let image = display.render(now).clone();
        assert_eq!(image.pixel(1, 44), [255, 255, 255]);
        assert_eq!(image.pixel(30, 50), [255, 255, 255]);
        assert_eq!(image.pixel(100, 50), [0, 0, 0]);
        assert!((0..20).any(|y| (0..60).any(|x| image.pixel(x, y) != [0, 0, 0])), "The title is drawn");
        assert!(display.components().is_empty());

        // Pan left of center
        display.add_element(Component::title_value("1: Drums", "Pan: L50", 256, true));
        let image = display.render(now).clone();
        assert_eq!(image.pixel(40, 50), [255, 255, 255]);
        assert_eq!(image.pixel(20, 50), [0, 0, 0]);

        // Channels
        display.add_element(Component::title_channels("Mixer", vec![true, false], vec![1024, 512], false));
        let image = display.render(now).clone();
        assert_eq!(image.pixel(10, 62), [255, 255, 255]);
        assert_eq!(image.pixel(64, 62), [0, 0, 0]);
        assert_eq!(image.pixel(65, 62), [255, 255, 255]);

        // A list with a selected item
        display.add_list_element_sized(3, &["A".to_string(), "B".to_string(), "C".to_string(), "D".to_string()], 3);
        let image = display.render(now).clone();
        assert_eq!(image.pixel(126, 60), [255, 255, 255]);
        assert_eq!(image.pixel(126, 10), [0, 0, 0]);

        // Label and menu
        display.add_element(Component::label("Goodbye", None, false, false, LabelLayout::Plain));
        let image = display.render(now).clone();
        assert!((0..64).any(|y| (0..128).any(|x| image.pixel(x, y) != [0, 0, 0])));
        display.add_element(Component::TitleValueMenu {
            label1: "A".into(),
            label2: "B".into(),
            menu: ["1".into(), "2".into(), "3".into(), "4".into()],
            value: 100,
            vu_left: 500,
            vu_right: 600,
            is_pan: false,
        });
        assert_eq!(display.render(now).pixel(0, 63), [255, 255, 255]);
    }

    #[test]
    fn notification() {
        let mut display = display();
        let now = Instant::now();
        display.set_notification_message("Undo", now);
        assert!(display.is_notification_active(now));
        let image = display.render(now).clone();
        assert!((20..44).any(|y| (0..128).any(|x| image.pixel(x, y) != [0, 0, 0])));
        // The notification ends
        let later = now + Duration::from_millis(NOTIFICATION_TIME + 1);
        assert!(!display.is_notification_active(later));
        assert_eq!(display.render(later), &Bitmap::new(128, 64));
        display.set_notification_message("Redo", now);
        display.cancel_notification();
        assert_eq!(display.notification(), None);
    }
}

/// Golden tests of the Push 2/3 displays (960x160 with anti-aliasing).
///
/// Every scene is checked with pixel samples and a hash of the whole image
/// ([`Bitmap::fnv1a_hash`]). If the drawing changes deliberately, regenerate the hashes:
/// run `GRAPHICS_DUMP_DIR=/tmp/push cargo test --lib framework::graphics::push_tests`, look at
/// the dumped PPM images (e.g. `magick /tmp/push/channels.ppm channels.png`) and copy the new
/// hashes from the failure messages into the tests.
#[cfg(test)]
mod push_tests {
    use super::*;
    use crate::daw::clip::StepState;

    fn display() -> GraphicDisplay {
        GraphicDisplay::new(GraphicsConfiguration::push_default(), GraphicsDimensions::new(960, 160, 1024))
    }

    /// Render, dump the image if requested and compare the hash.
    fn check(name: &str, display: &mut GraphicDisplay, expected_hash: u64) -> Bitmap {
        let image = display.render(Instant::now()).clone();
        if let Ok(dir) = std::env::var("GRAPHICS_DUMP_DIR") {
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(format!("{dir}/{name}.ppm"), image.to_ppm()).unwrap();
        }
        assert_eq!(image.fnv1a_hash(), expected_hash, "{name}: hash 0x{:016x}", image.fnv1a_hash());
        image
    }

    fn rgb(c: ColorEx) -> [u8; 3] {
        let [r, g, b] = c.to_int_rgb255();
        [r as u8, g as u8, b as u8]
    }

    #[test]
    fn channels() {
        let mut display = display();
        let types = [ChannelType::Audio, ChannelType::Instrument, ChannelType::Group, ChannelType::Effect, ChannelType::Master, ChannelType::Hybrid, ChannelType::Layer, ChannelType::GroupOpen];
        let colors = [ColorEx::RED, ColorEx::GREEN, ColorEx::BLUE, ColorEx::YELLOW, ColorEx::ORANGE, ColorEx::PINK, ColorEx::SKY_BLUE, ColorEx::PURPLE];
        for i in 0..8 {
            display.add_channel_element(
                if i == 0 { "Volume" } else { "" },
                i == 0,
                &format!("Track {}", i + 1),
                Some(types[i]),
                colors[i],
                i == 1,
                i as i32 * 140,
                -1,
                if i == 2 { "-3.2 dB" } else { "" },
                i as i32 * 140,
                -1,
                if i == 3 { "L 20" } else { "" },
                800,
                600,
                i == 4,
                i == 5,
                i == 6,
                i != 7,
                (i % 3) as i32,
                i == 7,
            );
        }
        let image = check("channels", &mut display, 0x4C1D5D5D4308174C);
        // The track color section at the bottom of the first channel
        assert_eq!(image.pixel(110, 150), rgb(ColorEx::RED));
    }

    #[test]
    fn parameters_and_sends() {
        let mut display = display();
        display.add_parameter_element_with_device_name("Device", true, "Polysynth", "Polysynth", ColorEx::GRAY, false, "Cutoff", 600, "1.2 kHz", true, 700);
        display.add_parameter_element("Track", false, "Bass", Some(ChannelType::Instrument), ColorEx::RED, true, "Volume", 1023, "0.0 dB", false, -1);
        display.add_parameter_element_with_plain_menu("Pan", false, "Plain", ColorEx::DARK_GRAY, false, "Pan", 512, "C", false, -1);
        display.add_parameter_element_plain("No value", -1, "Off", false, -1);
        let sends: Vec<SendData> = (0..4).map(|i| SendData::new(i != 2, &format!("FX {}", i + 1), if i == 1 { "-6 dB" } else { "" }, i * 250, -1, i == 1)).collect();
        display.add_sends_element("Sends", false, "Lead", Some(ChannelType::Audio), ColorEx::BLUE, false, &sends, false, true, true);
        display.add_sends_element("", false, "Pad", Some(ChannelType::Hybrid), ColorEx::GREEN, true, &sends, true, true, true);
        display.add_channel_selector_element("Select", false, "Drums", Some(ChannelType::Group), ColorEx::ORANGE, false, true);
        display.add_empty_element();
        let image = check("parameters", &mut display, 0xC39779442A85D4C5);
        let configuration = GraphicsConfiguration::push_default();
        // The slider of the touched first parameter (background lighter)
        assert_eq!(image.pixel(60, 40), rgb(configuration.color_background_lighter));
        assert_eq!(image.pixel(200, 150), rgb(ColorEx::RED));
    }

    #[test]
    fn options_and_lists() {
        let mut display = display();
        display.add_option_element("Header", "Top", true, "Bottom header", "Bottom", false, false);
        display.add_option_element_colored("", "Small", false, Some(ColorEx::RED), "", "Blue", true, Some(ColorEx::BLUE), true);
        display.add_option_element_colored_with_header_selection("", "", false, None, "Selected", "", false, None, false, true);
        display.add_empty_element_with_small_menu(true);
        let items: Vec<String> = ["One", "Two", "Three", "Four", "Five", "Six"].iter().map(|s| s.to_string()).collect();
        display.add_list_element_sized(6, &items, 2);
        display.add_list_element(&items[..3], &[false, true, false]);
        display.add_element(Component::Menu(MenuComponent::new("Menu", true, "Footer", None, Some(ColorEx::MINT), false, true, LabelLayout::Colored)));
        display.add_element(Component::LabelWithIcon(LabelComponent::new("User", icons::get_svg_image("user.svg"), Some(ColorEx::ROSE), true, true, LabelLayout::Colored)));
        let image = check("options", &mut display, 0x88F9FA2460E80CE3);
        // The selected top menu of the first column: the contrast of the text color (black)
        assert_eq!(image.pixel(5, 5), [255, 255, 255]);
        assert_eq!(image.pixel(5, 145), rgb(GraphicsConfiguration::push_default().color_background_darker));
    }

    #[test]
    fn scenes_and_clips() {
        let mut display = display();
        let scenes: Vec<SceneInfo> = (0..8).map(|i| SceneInfo { name: format!("Scene {}", i + 1), color: if i % 2 == 0 { ColorEx::SKY_BLUE } else { ColorEx::DARK_GRAY }, exists: i < 7, is_selected: i == 1 }).collect();
        display.add_scene_list_element(scenes, None, "Scenes", ColorEx::GRAY, false, true, false);
        let slot = |name: &str, color: ColorEx| SlotInfo { track_exists: true, exists: true, name: name.to_string(), color, has_content: true, ..Default::default() };
        let slots = vec![
            SlotInfo { is_playing: true, ..slot("Playing", ColorEx::RED) },
            SlotInfo { is_playing_queued: true, ..slot("Queued", ColorEx::GREEN) },
            SlotInfo { is_recording: true, ..slot("Recording", ColorEx::BLUE) },
            SlotInfo { is_stop_queued: true, ..slot("Stop", ColorEx::YELLOW) },
            SlotInfo { has_content: false, is_selected: true, ..slot("", ColorEx::DARK_GRAY) },
            SlotInfo { has_content: false, track_is_rec_arm: true, ..slot("", ColorEx::DARK_GRAY) },
            SlotInfo { track_is_group: true, position: 6, ..slot("", ColorEx::ORANGE) },
            SlotInfo { track_exists: false, ..slot("Hidden", ColorEx::PINK) },
        ];
        display.add_slot_list_element(slots, Some(ChannelType::Audio), "Audio", ColorEx::RED, true, true, false);
        let image = check("scenes", &mut display, 0xC37EE60A36BCCB39);
        assert_eq!(image.pixel(100, 10), rgb(ColorEx::SKY_BLUE));
    }

    #[test]
    fn midi_clip_and_overlay() {
        let mut display = display();
        let mut steps = Vec::new();
        for step in 0..16 {
            if step % 3 == 0 {
                steps.push(MidiStepInfo { channel: 0, step, note: 36 + step % 5, state: StepState::Start, is_selected: step == 3, is_muted: step == 6 });
                steps.push(MidiStepInfo { channel: 0, step: step + 1, note: 36 + step % 5, state: StepState::Continue, is_selected: false, is_muted: false });
            }
        }
        steps.sort_by_key(|s| (s.note, s.step, s.channel));
        let clip = MidiClipInfo {
            color: ColorEx::SKY_BLUE,
            num_steps: 16,
            step_length: 0.25,
            edit_page: 0,
            is_loop_enabled: true,
            loop_start: 1.0,
            loop_length: 2.0,
            play_start: 0.0,
            play_end: 4.0,
            lowest_row_with_data: 36,
            highest_row_with_data: 40,
            current_step: 5,
            steps,
        };
        display.set_midi_clip_element(clip, 4, Some(NotePosition::new(0, 9, 40)));
        display.add_graph_overlay(600, 40, 200, 80, ColorEx::WHITE, &[0, 30, 90, 60, 100, 10], 100);
        let image = check("midiclip", &mut display, 0xB2BFCB5A3BE4F33A);
        // The play cursor
        assert_eq!(image.pixel(5 * 60, 100), [255, 255, 255]);
    }

    #[test]
    fn notification_and_message() {
        let mut display = display();
        display.set_message(3, "Please start Bitwig Studio to play...");
        display.set_notification_message("Tempo: 120.00", Instant::now());
        let image = check("notification", &mut display, 0xC3CB38F2CA583AB1);
        assert!(display.has_changed());
        let generation = display.generation();
        // The same frame again: nothing changed
        display.set_message(3, "Please start Bitwig Studio to play...");
        display.render(Instant::now());
        assert!(!display.has_changed());
        assert_eq!(display.generation(), generation);
        // The notification is centered on the text color (black) with a white background
        assert_eq!(image.pixel(330, 60), [255, 255, 255]);
        display.force_render();
        display.set_message(3, "Please start Bitwig Studio to play...");
        display.render(Instant::now());
        assert!(display.has_changed());
        assert_eq!(display.generation(), generation + 1);
    }
}
