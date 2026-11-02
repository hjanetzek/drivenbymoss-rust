// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A RGB bitmap (former `IBitmap`) and the graphics context which draws into it (former
//! `IGraphicsContext`, implemented with AWT in Reaper and Cairo in Bitwig).
//!
//! Differences to Java:
//! - There is no anti-aliasing (the only display which uses this module, the Akai Fire OLED,
//!   has it turned off as well).
//! - A pixel is filled if its center is inside of the filled rectangle (Cairo without
//!   anti-aliasing). A stroked rectangle draws a 1 pixel frame on the columns `floor(left)` and
//!   `floor(left + width)` and the rows `floor(top)` and `floor(top + height)` (like AWT
//!   `drawRect`).
//! - Texts are drawn with the embedded bitmap font of [`super::font`] (scaled by an integer
//!   factor) instead of a vector font. Text widths therefore differ slightly from Java.
//! - Rounded rectangles (the background of a text) are drawn as plain rectangles.

use super::font::{self, ADVANCE, CAP_HEIGHT, GLYPH_WIDTH};
use crate::config::values::fix_ascii;
use crate::daw::color::ColorEx;

/// The alignment of a text (former `Align`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
}

/// A RGB bitmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    width: i32,
    height: i32,
    pixels: Vec<[u8; 3]>,
}

impl Bitmap {
    pub fn new(width: i32, height: i32) -> Self {
        Bitmap { width, height, pixels: vec![[0, 0, 0]; (width * height).max(0) as usize] }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// Get the RGB values of a pixel; black if outside of the bitmap.
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 3] {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return [0, 0, 0];
        }
        self.pixels[(y * self.width + x) as usize]
    }

    /// Set a pixel; ignored if outside of the bitmap.
    pub fn set_pixel(&mut self, x: i32, y: i32, rgb: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        self.pixels[(y * self.width + x) as usize] = rgb;
    }

    /// The pixels in the order of the Java `IBitmap.encode` buffer (row by row, each pixel as
    /// blue, green, red, alpha).
    pub fn encode_bgra(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.pixels.len() * 4);
        for [r, g, b] in &self.pixels {
            data.extend_from_slice(&[*b, *g, *r, 0xFF]);
        }
        data
    }

    /// A text representation for debugging and tests: '#' for a pixel which is not black, '.'
    /// otherwise, one line per row.
    pub fn to_ascii_art(&self) -> String {
        let mut text = String::with_capacity(((self.width + 1) * self.height) as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                text.push(if self.pixel(x, y) == [0, 0, 0] { '.' } else { '#' });
            }
            text.push('\n');
        }
        text
    }

    /// Start drawing into the bitmap (former `IBitmap.render`).
    pub fn context(&mut self) -> GraphicsContext<'_> {
        let clip = (0, 0, self.width, self.height);
        GraphicsContext { bitmap: self, clip }
    }
}

fn to_rgb(color: ColorEx) -> [u8; 3] {
    let c = color.to_int_rgb255();
    [c[0].clamp(0, 255) as u8, c[1].clamp(0, 255) as u8, c[2].clamp(0, 255) as u8]
}

/// Draws into a bitmap (former `IGraphicsContext`).
pub struct GraphicsContext<'a> {
    bitmap: &'a mut Bitmap,
    /// The clipping area: left, top, right and bottom (exclusive).
    clip: (i32, i32, i32, i32),
}

impl GraphicsContext<'_> {
    fn plot(&mut self, x: i32, y: i32, rgb: [u8; 3]) {
        let (left, top, right, bottom) = self.clip;
        if x >= left && x < right && y >= top && y < bottom {
            self.bitmap.set_pixel(x, y, rgb);
        }
    }

    /// Fill all pixels of which the centers are in the range.
    fn fill_pixels(&mut self, x: f64, y: f64, width: f64, height: f64, rgb: [u8; 3]) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let x0 = (x - 0.5).ceil() as i32;
        let x1 = (x + width - 0.5).ceil() as i32;
        let y0 = (y - 0.5).ceil() as i32;
        let y1 = (y + height - 0.5).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                self.plot(px, py, rgb);
            }
        }
    }

    /// Fill a rectangle.
    pub fn fill_rectangle(&mut self, x: f64, y: f64, width: f64, height: f64, color: ColorEx) {
        self.fill_pixels(x, y, width, height, to_rgb(color));
    }

    /// Draw the frame of a rectangle with a line width of 1.
    pub fn stroke_rectangle(&mut self, left: f64, top: f64, width: f64, height: f64, color: ColorEx) {
        let rgb = to_rgb(color);
        let x0 = left.floor() as i32;
        let x1 = (left + width).floor() as i32;
        let y0 = top.floor() as i32;
        let y1 = (top + height).floor() as i32;
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        for x in x0..=x1 {
            self.plot(x, y0, rgb);
            self.plot(x, y1, rgb);
        }
        for y in y0..=y1 {
            self.plot(x0, y, rgb);
            self.plot(x1, y, rgb);
        }
    }

    /// Draw a line (Bresenham).
    pub fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: ColorEx) {
        let rgb = to_rgb(color);
        let (mut x, mut y) = (x1.floor() as i32, y1.floor() as i32);
        let (xe, ye) = (x2.floor() as i32, y2.floor() as i32);
        let dx = (xe - x).abs();
        let dy = -(ye - y).abs();
        let sx = if x < xe { 1 } else { -1 };
        let sy = if y < ye { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.plot(x, y, rgb);
            if x == xe && y == ye {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a text with its baseline at the given position.
    fn show_text(&mut self, text: &str, x: i32, baseline: i32, scale: i32, rgb: [u8; 3]) {
        let top = baseline - CAP_HEIGHT * scale;
        let mut left = x;
        for c in text.chars() {
            let glyph = font::glyph(c);
            for (column, bits) in glyph.iter().enumerate().take(GLYPH_WIDTH as usize) {
                for row in 0..8 {
                    if bits & (1 << row) == 0 {
                        continue;
                    }
                    for sy in 0..scale {
                        for sx in 0..scale {
                            self.plot(left + column as i32 * scale + sx, top + row * scale + sy, rgb);
                        }
                    }
                }
            }
            left += ADVANCE * scale;
        }
    }

    /// Draw a text vertically centered and horizontally aligned in the given bounds; the text
    /// is clipped to the bounds. If a background color is given, a rectangle with an inset of
    /// 12 pixels is drawn behind the text (former `drawTextInBounds`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_in_bounds(&mut self, text: &str, x: f64, y: f64, width: f64, height: f64, alignment: Align, color: ColorEx, background: Option<ColorEx>, font_size: f64) {
        if text.is_empty() {
            return;
        }
        let text = fix_ascii(text);
        let scale = font::scale_for_font_size(font_size);
        let h = (CAP_HEIGHT * scale) as f64;
        let w = font::text_width(&text, scale) as f64;
        let pos_x = if alignment == Align::Center { x + (width - w) / 2.0 } else { x };
        let pos_y = y + (height + h) / 2.0;

        let old_clip = self.clip;
        let left = (x - 0.5).ceil() as i32;
        let top = (y - 0.5).ceil() as i32;
        let right = (x + width - 0.5).ceil() as i32;
        let bottom = (y + height - 0.5).ceil() as i32;
        self.clip = (left.max(old_clip.0), top.max(old_clip.1), right.min(old_clip.2), bottom.min(old_clip.3));
        if let Some(background) = background {
            let inset = 12.0;
            self.fill_rectangle(pos_x - inset, pos_y - h - inset, w + 2.0 * inset, h + 2.0 * inset, background);
        }
        self.show_text(&text, pos_x.round() as i32, pos_y.round() as i32, scale, to_rgb(color));
        self.clip = old_clip;
    }

    /// Draw a text left aligned and vertically centered in the given height, without clipping
    /// (former `drawTextInHeight`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_in_height(&mut self, text: &str, x: f64, y: f64, height: f64, color: ColorEx, background: Option<ColorEx>, font_size: f64) {
        if text.is_empty() {
            return;
        }
        let text = fix_ascii(text);
        let scale = font::scale_for_font_size(font_size);
        let h = (CAP_HEIGHT * scale) as f64;
        let pos_y = y + (height + h) / 2.0;
        if let Some(background) = background {
            let w = font::text_width(&text, scale) as f64;
            let inset = 12.0;
            self.fill_rectangle(x - inset, pos_y - h - inset, w + 2.0 * inset, h + 2.0 * inset, background);
        }
        self.show_text(&text, x.round() as i32, pos_y.round() as i32, scale, to_rgb(color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_and_stroke() {
        let mut bitmap = Bitmap::new(8, 4);
        bitmap.context().fill_rectangle(1.0, 1.0, 2.0, 2.0, ColorEx::WHITE);
        assert_eq!(bitmap.to_ascii_art(), "........\n.##.....\n.##.....\n........\n");

        let mut bitmap = Bitmap::new(8, 4);
        bitmap.context().stroke_rectangle(1.0, 0.0, 3.0, 3.0, ColorEx::WHITE);
        assert_eq!(bitmap.to_ascii_art(), ".####...\n.#..#...\n.#..#...\n.####...\n");

        // Zero or negative sizes draw nothing
        let mut bitmap = Bitmap::new(4, 4);
        bitmap.context().fill_rectangle(1.0, 1.0, 0.0, 2.0, ColorEx::WHITE);
        bitmap.context().fill_rectangle(1.0, 1.0, -3.0, 2.0, ColorEx::WHITE);
        assert_eq!(bitmap, Bitmap::new(4, 4));

        let mut bitmap = Bitmap::new(4, 4);
        bitmap.context().draw_line(0.0, 0.0, 3.0, 3.0, ColorEx::RED);
        assert_eq!(bitmap.pixel(2, 2), [255, 0, 0]);
        assert_eq!(bitmap.encode_bgra()[0..4], [0, 0, 255, 255]);
    }

    #[test]
    fn text() {
        let mut bitmap = Bitmap::new(12, 9);
        bitmap.context().draw_text_in_height("T", 0.0, 0.0, 9.0, ColorEx::WHITE, None, 5.0);
        let art = bitmap.to_ascii_art();
        assert_eq!(art.lines().nth(1).unwrap(), "#####.......", "{art}");
        assert_eq!(art.lines().nth(7).unwrap(), "..#.........", "{art}");

        // Clipped to the bounds
        let mut bitmap = Bitmap::new(12, 9);
        bitmap.context().draw_text_in_bounds("TT", 0.0, 0.0, 3.0, 9.0, Align::Left, ColorEx::WHITE, None, 5.0);
        for y in 0..9 {
            for x in 3..12 {
                assert_eq!(bitmap.pixel(x, y), [0, 0, 0]);
            }
        }
        // Centered with background
        let mut bitmap = Bitmap::new(40, 30);
        bitmap.context().draw_text_in_bounds("A", 0.0, 0.0, 40.0, 30.0, Align::Center, ColorEx::BLACK, Some(ColorEx::WHITE), 5.0);
        assert_eq!(bitmap.pixel(20, 4), [255, 255, 255]);
        assert_eq!(bitmap.pixel(0, 0), [0, 0, 0]);
    }
}
