// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A RGB bitmap (former `IBitmap`) and the graphics context which draws into it (former
//! `IGraphicsContext`, implemented with AWT in Reaper and Cairo in Bitwig).
//!
//! The context draws with or without anti-aliasing (former `IGraphicsConfiguration.
//! isAntialiasEnabled`, see [`Bitmap::context_with_antialias`]).
//!
//! Without anti-aliasing (the monochrome OLEDs, e.g. the Akai Fire):
//! - A pixel is filled if its center is inside of the filled rectangle (Cairo without
//!   anti-aliasing); other shapes (circles, triangles, rounded rectangles) fill the pixels
//!   which are covered by at least 50%.
//! - Texts are drawn with the embedded bitmap font of [`super::font`] (scaled by an integer
//!   factor) instead of a vector font. Text widths therefore differ slightly from Java.
//! - The background of a text is a plain rectangle (Java: rounded with a radius of 12).
//!
//! With anti-aliasing (the color displays, e.g. Push 2/3; Cairo with `AntialiasMode.BEST` in
//! Java):
//! - Shapes are filled with the exact area coverage of the pixels (see [`super::raster`]) and
//!   alpha blending; lines are 1 pixel wide (Cairo uses the current line width, 2 by default).
//! - Texts are drawn with the embedded DejaVu Sans outlines of [`super::vector_font`], not
//!   hinted. Clipping (of texts in bounds) is done on whole pixels.
//! - Icons are drawn at whole pixel positions (Cairo interpolates at fractional positions).
//!
//! In both modes a stroked rectangle is drawn without anti-aliasing (as in Java): a frame of
//! `line_width` pixels starting at the columns `floor(left - line_width / 2 + 0.5)` and
//! `floor(left + width - line_width / 2 + 0.5)` and the rows accordingly; for a width of 1 that
//! is `floor(left)` and `floor(left + width)` (like AWT `drawRect`).

use super::font::{self, ADVANCE, CAP_HEIGHT, GLYPH_WIDTH};
use super::icons::IconData;
use super::raster::{Mask, Path};
use super::vector_font;
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

    /// The RGB pixels, row by row.
    pub fn pixels(&self) -> &[[u8; 3]] {
        &self.pixels
    }

    /// A stable hash of the size and the pixels (64 bit FNV-1a) for golden tests.
    pub fn fnv1a_hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |byte: u8| {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for b in self.width.to_le_bytes().into_iter().chain(self.height.to_le_bytes()) {
            feed(b);
        }
        for p in &self.pixels {
            for &b in p {
                feed(b);
            }
        }
        hash
    }

    /// The bitmap as a binary PPM (P6) image, e.g. to look at it while debugging: `std::fs::write
    /// ("/tmp/display.ppm", bitmap.to_ppm ())`.
    pub fn to_ppm(&self) -> Vec<u8> {
        let mut data = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        for p in &self.pixels {
            data.extend_from_slice(p);
        }
        data
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

    /// Start drawing into the bitmap without anti-aliasing (former `IBitmap.render`).
    pub fn context(&mut self) -> GraphicsContext<'_> {
        self.context_with_antialias(false)
    }

    /// Start drawing into the bitmap with or without anti-aliasing (former `IBitmap.render
    /// (enableAntialias, ...)`).
    pub fn context_with_antialias(&mut self, antialias: bool) -> GraphicsContext<'_> {
        let clip = (0, 0, self.width, self.height);
        GraphicsContext { bitmap: self, clip, antialias }
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
    antialias: bool,
}

/// Blend a color into a pixel with the given opacity (0..1).
fn blend(dst: [u8; 3], src: [u8; 3], alpha: f32) -> [u8; 3] {
    if alpha >= 1.0 {
        return src;
    }
    let mix = |d: u8, s: u8| (d as f32 + (s as f32 - d as f32) * alpha).round().clamp(0.0, 255.0) as u8;
    [mix(dst[0], src[0]), mix(dst[1], src[1]), mix(dst[2], src[2])]
}

/// The coverage of the pixel `p` by the range `start..end` (0..1).
fn span_coverage(p: i32, start: f64, end: f64) -> f64 {
    (end.min(p as f64 + 1.0) - start.max(p as f64)).clamp(0.0, 1.0)
}

impl GraphicsContext<'_> {
    /// True if the context draws with anti-aliasing.
    pub fn is_antialias(&self) -> bool {
        self.antialias
    }

    fn in_clip(&self, x: i32, y: i32) -> bool {
        let (left, top, right, bottom) = self.clip;
        x >= left && x < right && y >= top && y < bottom
    }

    /// Blend a color into a pixel inside of the clipping area.
    fn plot_alpha(&mut self, x: i32, y: i32, rgb: [u8; 3], alpha: f32) {
        if alpha <= 0.0 || !self.in_clip(x, y) {
            return;
        }
        let dst = self.bitmap.pixel(x, y);
        self.bitmap.set_pixel(x, y, blend(dst, rgb, alpha));
    }

    /// Fill the pixels of a mask with the color of its row; without anti-aliasing the pixels
    /// which are covered by at least 50%.
    fn fill_mask(&mut self, mask: &Mask, color_of_row: impl Fn(i32) -> [u8; 3]) {
        for row in 0..mask.height {
            let y = mask.top + row as i32;
            let rgb = color_of_row(y);
            for column in 0..mask.width {
                let coverage = mask.coverage[row * mask.width + column];
                let alpha = if self.antialias {
                    coverage
                } else if coverage >= 0.5 {
                    1.0
                } else {
                    0.0
                };
                self.plot_alpha(mask.left + column as i32, y, rgb, alpha);
            }
        }
    }

    fn fill_path(&mut self, path: &Path, color: ColorEx) {
        if let Some(mask) = path.rasterize(self.clip) {
            let rgb = to_rgb(color);
            self.fill_mask(&mask, |_| rgb);
        }
    }

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

    /// Fill a rectangle; with anti-aliasing the partially covered pixels at the edges are
    /// blended.
    pub fn fill_rectangle(&mut self, x: f64, y: f64, width: f64, height: f64, color: ColorEx) {
        if !self.antialias {
            self.fill_pixels(x, y, width, height, to_rgb(color));
            return;
        }
        if !(width > 0.0 && height > 0.0) {
            return;
        }
        let rgb = to_rgb(color);
        let (x2, y2) = (x + width, y + height);
        let (left, top, right, bottom) = self.clip;
        let x0 = (x.floor() as i32).max(left);
        let x1 = (x2.ceil() as i32).min(right);
        let y0 = (y.floor() as i32).max(top);
        let y1 = (y2.ceil() as i32).min(bottom);
        for py in y0..y1 {
            let cy = span_coverage(py, y, y2);
            for px in x0..x1 {
                self.plot_alpha(px, py, rgb, (cy * span_coverage(px, x, x2)) as f32);
            }
        }
    }

    /// Draw the frame of a rectangle with a line width of 1, always without anti-aliasing (as
    /// in Java).
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

    /// Draw the frame of a rectangle with the given line width, always without anti-aliasing
    /// (former `strokeRectangle (..., lineWidth)`).
    pub fn stroke_rectangle_with_width(&mut self, left: f64, top: f64, width: f64, height: f64, color: ColorEx, line_width: f64) {
        let thickness = line_width.round().max(1.0) as i32;
        if thickness == 1 {
            self.stroke_rectangle(left, top, width, height, color);
            return;
        }
        let rgb = to_rgb(color);
        let offset = 0.5 - line_width / 2.0;
        let x0 = (left + offset).floor() as i32;
        let x1 = (left + width + offset).floor() as i32;
        let y0 = (top + offset).floor() as i32;
        let y1 = (top + height + offset).floor() as i32;
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        for t in 0..thickness {
            for x in x0..x1 + thickness {
                self.plot(x, y0 + t, rgb);
                self.plot(x, y1 + t, rgb);
            }
            for y in y0..y1 + thickness {
                self.plot(x0 + t, y, rgb);
                self.plot(x1 + t, y, rgb);
            }
        }
    }

    /// Fill a rectangle with rounded corners (former `fillRoundedRectangle`).
    pub fn fill_rounded_rectangle(&mut self, left: f64, top: f64, width: f64, height: f64, radius: f64, color: ColorEx) {
        let mut path = Path::new();
        path.rounded_rectangle(left, top, width, height, radius);
        self.fill_path(&path, color);
    }

    /// Fill a rectangle with rounded corners with a vertical linear gradient from `color1` at
    /// the top to `color2` at the bottom (former `fillGradientRoundedRectangle`).
    #[allow(clippy::too_many_arguments)]
    pub fn fill_gradient_rounded_rectangle(&mut self, left: f64, top: f64, width: f64, height: f64, radius: f64, color1: ColorEx, color2: ColorEx) {
        let mut path = Path::new();
        path.rounded_rectangle(left, top, width, height, radius);
        let Some(mask) = path.rasterize(self.clip) else {
            return;
        };
        let (c1, c2) = (color1.to_double_rgb(), color2.to_double_rgb());
        self.fill_mask(&mask, |y| {
            // The color at the center of the pixel row
            let t = if height > 0.0 { ((y as f64 + 0.5 - top) / height).clamp(0.0, 1.0) } else { 0.0 };
            to_rgb(ColorEx::new(c1[0] + (c2[0] - c1[0]) * t, c1[1] + (c2[1] - c1[1]) * t, c1[2] + (c2[2] - c1[2]) * t))
        });
    }

    /// Fill a triangle (former `fillTriangle`).
    #[allow(clippy::too_many_arguments)]
    pub fn fill_triangle(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, color: ColorEx) {
        let mut path = Path::new();
        path.move_to(x1, y1);
        path.line_to(x2, y2);
        path.line_to(x3, y3);
        self.fill_path(&path, color);
    }

    /// Draw the outline of a triangle (former `strokeTriangle`).
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_triangle(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, color: ColorEx) {
        self.draw_line(x1, y1, x2, y2, color);
        self.draw_line(x2, y2, x3, y3, color);
        self.draw_line(x3, y3, x1, y1, color);
    }

    /// Fill a circle (former `fillCircle`).
    pub fn fill_circle(&mut self, x: f64, y: f64, radius: f64, color: ColorEx) {
        let mut path = Path::new();
        path.circle(x, y, radius.max(0.0));
        self.fill_path(&path, color);
    }

    /// Draw an icon in the given color; the icon is used as an alpha mask (former `maskImage`).
    /// The position is rounded to whole pixels.
    pub fn mask_image(&mut self, icon: &IconData, x: f64, y: f64, color: ColorEx) {
        let rgb = to_rgb(color);
        let (left, top) = (x.round() as i32, y.round() as i32);
        for row in 0..icon.height {
            for column in 0..icon.width {
                let a = icon.alpha[(row * icon.width + column) as usize];
                let alpha = if self.antialias {
                    a as f32 / 255.0
                } else if a >= 128 {
                    1.0
                } else {
                    0.0
                };
                self.plot_alpha(left + column, top + row, rgb, alpha);
            }
        }
    }

    /// Calculate the largest font size (in steps of 1, starting at the minimum) for which the
    /// text fits into the width and which is smaller than the maximum height; -1 if even the
    /// minimum does not fit (former `calculateFontSize`).
    pub fn calculate_font_size(&self, text: &str, max_height: f64, max_width: f64, minimum_font_size: f64) -> f64 {
        let mut size = minimum_font_size;
        let mut fitting_size = -1.0;
        while size < max_height {
            if self.text_width(text, size) > max_width {
                break;
            }
            fitting_size = size;
            size += 1.0;
        }
        fitting_size
    }

    /// The width of a text at the given font size (former `getTextExtents (text).getWidth ()`).
    pub fn text_width(&self, text: &str, font_size: f64) -> f64 {
        if self.antialias {
            vector_font::text_width(text, font_size)
        } else {
            font::text_width(&fix_ascii(text), font::scale_for_font_size(font_size)) as f64
        }
    }

    /// The height of the letter "T" at the given font size.
    fn cap_height(&self, font_size: f64) -> f64 {
        if self.antialias {
            vector_font::cap_height(font_size)
        } else {
            (CAP_HEIGHT * font::scale_for_font_size(font_size)) as f64
        }
    }

    /// Draw the background of a text (rounded with anti-aliasing).
    #[allow(clippy::too_many_arguments)]
    fn fill_text_background(&mut self, x: f64, y: f64, width: f64, height: f64, radius: f64, color: ColorEx) {
        if self.antialias {
            self.fill_rounded_rectangle(x, y, width, height, radius, color);
        } else {
            self.fill_rectangle(x, y, width, height, color);
        }
    }

    /// Draw a text with its origin at the given position with the font of the mode.
    fn show_text_at(&mut self, text: &str, x: f64, baseline: f64, font_size: f64, color: ColorEx) {
        if self.antialias {
            let mut path = Path::new();
            vector_font::append_text(&mut path, text, x, baseline, font_size);
            self.fill_path(&path, color);
        } else {
            self.show_text(&fix_ascii(text), x.round() as i32, baseline.round() as i32, font::scale_for_font_size(font_size), to_rgb(color));
        }
    }

    /// Draw a line (Bresenham; with anti-aliasing 1 pixel wide).
    pub fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: ColorEx) {
        if self.antialias {
            let mut path = Path::new();
            path.stroke_line(x1, y1, x2, y2, 1.0);
            self.fill_path(&path, color);
            return;
        }
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
        let h = self.cap_height(font_size);
        let w = self.text_width(text, font_size);
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
            self.fill_text_background(pos_x - inset, pos_y - h - inset, w + 2.0 * inset, h + 2.0 * inset, inset, background);
        }
        // As in Java the origin of the text is placed at the position (the ink starts at the left
        // side bearing of the first glyph) although the width is the ink width
        self.show_text_at(text, pos_x, pos_y, font_size, color);
        self.clip = old_clip;
    }

    /// Draw a text left aligned and vertically centered in the given height, without clipping
    /// (former `drawTextInHeight`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_in_height(&mut self, text: &str, x: f64, y: f64, height: f64, color: ColorEx, background: Option<ColorEx>, font_size: f64) {
        if text.is_empty() {
            return;
        }
        let h = self.cap_height(font_size);
        let pos_y = y + (height + h) / 2.0;
        if let Some(background) = background {
            let w = self.text_width(text, font_size);
            let inset = 12.0;
            self.fill_text_background(x - inset, pos_y - h - inset, w + 2.0 * inset, h + 2.0 * inset, inset, background);
        }
        self.show_text_at(text, x, pos_y, font_size, color);
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

    #[test]
    fn antialias() {
        // A fractional rectangle blends the edge pixels
        let mut bitmap = Bitmap::new(4, 2);
        bitmap.context_with_antialias(true).fill_rectangle(0.5, 0.0, 2.0, 1.0, ColorEx::WHITE);
        assert_eq!(bitmap.pixel(0, 0), [128, 128, 128]);
        assert_eq!(bitmap.pixel(1, 0), [255, 255, 255]);
        assert_eq!(bitmap.pixel(2, 0), [128, 128, 128]);
        assert_eq!(bitmap.pixel(1, 1), [0, 0, 0]);
        // Without anti-aliasing: pixel centers
        let mut bitmap = Bitmap::new(4, 2);
        bitmap.context().fill_rectangle(0.5, 0.0, 2.0, 1.0, ColorEx::WHITE);
        assert_eq!(bitmap.to_ascii_art(), "##..\n....\n");

        // Rounded corners are cut off
        let mut bitmap = Bitmap::new(20, 20);
        bitmap.context_with_antialias(true).fill_rounded_rectangle(0.0, 0.0, 20.0, 20.0, 8.0, ColorEx::WHITE);
        assert_eq!(bitmap.pixel(0, 0), [0, 0, 0]);
        assert_eq!(bitmap.pixel(10, 0), [255, 255, 255]);
        // Gradient from black to white
        let mut bitmap = Bitmap::new(10, 10);
        bitmap.context_with_antialias(true).fill_gradient_rounded_rectangle(0.0, 0.0, 10.0, 10.0, 0.0, ColorEx::BLACK, ColorEx::WHITE);
        assert!(bitmap.pixel(5, 1)[0] < 50 && bitmap.pixel(5, 8)[0] > 200);

        // A stroke of width 2
        let mut bitmap = Bitmap::new(6, 6);
        bitmap.context_with_antialias(true).stroke_rectangle_with_width(1.0, 1.0, 4.0, 4.0, ColorEx::WHITE, 2.0);
        assert_eq!(bitmap.to_ascii_art(), "######\n######\n##..##\n##..##\n######\n######\n");

        // An icon as a mask
        let icon = super::super::icons::get_svg_image("channel/solo.svg").unwrap();
        let mut bitmap = Bitmap::new(20, 20);
        bitmap.context_with_antialias(true).mask_image(icon, 2.4, 3.0, ColorEx::RED);
        let set = bitmap.pixels().iter().filter(|p| p[0] > 0).count();
        assert!(set > 20, "{set}");
        assert!(bitmap.pixels().iter().all(|p| p[1] == 0 && p[2] == 0));

        // Vector text and font size calculation
        let mut bitmap = Bitmap::new(60, 20);
        let mut gc = bitmap.context_with_antialias(true);
        assert!(gc.text_width("Hello", 12.0) > 20.0);
        assert_eq!(gc.calculate_font_size("G#5", 30.0, 1.0, 12.0), -1.0);
        let size = gc.calculate_font_size("G#5", 30.0, 40.0, 12.0);
        assert!((12.0..30.0).contains(&size) && gc.text_width("G#5", size) <= 40.0);
        gc.draw_text_in_bounds("Hi", 0.0, 0.0, 60.0, 20.0, Align::Center, ColorEx::WHITE, None, 14.0);
        assert!(bitmap.pixels().iter().any(|p| p[0] == 255));
        assert!(bitmap.pixels().iter().any(|p| p[0] > 0 && p[0] < 255), "anti-aliased edges");
    }
}
