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
//! Further differences to Java:
//! - The rendering is not done in a separate thread; [`GraphicDisplay::render`] (former
//!   `send`) renders synchronously and only if the components or the notification changed.
//! - The notification is shown for [`NOTIFICATION_TIME`] milliseconds (Java: a counter of 1
//!   which is decreased every second, i.e. 1 to 2 seconds).
//! - The debug window (former `showDebugWindow`) does not exist; use
//!   [`Bitmap::to_ascii_art`].

pub mod bitmap;
pub mod component;
pub mod font;

use std::time::{Duration, Instant};

pub use bitmap::{Align, Bitmap, GraphicsContext};
pub use component::{Component, LabelLayout};

use crate::daw::color::ColorEx;

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
}

impl GraphicsConfiguration {
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
}

/// A graphics display (former `AbstractGraphicDisplay`).
pub struct GraphicDisplay {
    configuration: GraphicsConfiguration,
    dimensions: GraphicsDimensions,
    columns: Vec<Component>,
    info: Option<ModelInfo>,
    notification: Option<String>,
    notification_end: Option<Instant>,
    image: Bitmap,
}

impl GraphicDisplay {
    pub fn new(configuration: GraphicsConfiguration, dimensions: GraphicsDimensions) -> Self {
        GraphicDisplay {
            configuration,
            dimensions,
            columns: Vec::new(),
            info: None,
            notification: None,
            notification_end: None,
            image: Bitmap::new(dimensions.width, dimensions.height),
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
    /// clear the components and return the bitmap (former `send`).
    pub fn render(&mut self, now: Instant) -> &Bitmap {
        if !self.is_notification_active(now) {
            self.notification = None;
            self.notification_end = None;
        }
        let new_info = ModelInfo { notification: self.notification.clone(), components: std::mem::take(&mut self.columns) };
        if self.info.as_ref() != Some(&new_info) {
            self.render_image(&new_info);
            self.info = Some(new_info);
        }
        &self.image
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
        let mut gc = self.image.context();

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
