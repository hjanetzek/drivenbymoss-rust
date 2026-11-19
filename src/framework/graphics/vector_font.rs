// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The embedded vector font (DejaVu Sans outlines from [`super::font_data`]) for the
//! anti-aliased displays (Push 2/3). Java draws with the default font of Cairo (Bitwig), which
//! is "sans-serif", i.e. usually DejaVu Sans on Linux.
//!
//! The font size is the em size in pixels, like Cairo `setFontSize`. Texts are not hinted and
//! not kerned. Characters without a glyph are replaced like [`fix_ascii`] does or by '?'.

use super::font_data::{GlyphData, CAP_HEIGHT, CONTOURS, GLYPHS, POINTS, UNITS_PER_EM};
use super::raster::Path;
use crate::config::values::fix_ascii;

/// Get the glyph of a character.
pub fn glyph(c: char) -> Option<&'static GlyphData> {
    GLYPHS.binary_search_by_key(&(c as u32), |g| g.code).ok().map(|i| &GLYPHS[i])
}

/// The glyphs of a text.
fn glyphs(text: &str) -> Vec<&'static GlyphData> {
    let mut result = Vec::with_capacity(text.len());
    for c in text.chars() {
        if let Some(g) = glyph(c) {
            result.push(g);
            continue;
        }
        for r in fix_ascii(&c.to_string()).chars() {
            if let Some(g) = glyph(r).or_else(|| glyph('?')) {
                result.push(g);
            }
        }
    }
    result
}

/// The height of the letter "T" at the given font size (Java: `getTextExtents ("T").getHeight
/// ()`).
pub fn cap_height(font_size: f64) -> f64 {
    CAP_HEIGHT * font_size / UNITS_PER_EM
}

/// The ink width of a text (Java: `getTextExtents (text).getWidth ()`): from the left edge of
/// the first glyph to the right edge of the last one.
pub fn text_width(text: &str, font_size: f64) -> f64 {
    let (first, last) = ink_extent(&glyphs(text));
    ((last - first) * font_size / UNITS_PER_EM).max(0.0)
}

/// The horizontal ink extent in font units relative to the text origin.
fn ink_extent(glyphs: &[&GlyphData]) -> (f64, f64) {
    let mut pos = 0.0;
    let mut extent: Option<(f64, f64)> = None;
    for g in glyphs {
        if g.num_contours > 0 {
            let (l, r) = (pos + g.x_min as f64, pos + g.x_max as f64);
            extent = Some(extent.map_or((l, r), |(el, er)| (el.min(l), er.max(r))));
        }
        pos += g.advance as f64;
    }
    extent.unwrap_or((0.0, 0.0))
}

/// Add the outlines of a text with its origin (the start of the baseline) at the given position
/// to a path.
pub fn append_text(path: &mut Path, text: &str, x: f64, baseline: f64, font_size: f64) {
    let scale = font_size / UNITS_PER_EM;
    let mut pos = x;
    for g in glyphs(text) {
        let first = g.first_contour as usize;
        for &(start, end) in &CONTOURS[first..first + g.num_contours as usize] {
            append_contour(path, &POINTS[start as usize..end as usize], pos, baseline, scale);
        }
        pos += g.advance as f64 * scale;
    }
}

/// Add a TrueType contour (on-curve points and quadratic control points, two control points in
/// a row imply an on-curve point in the middle).
fn append_contour(path: &mut Path, points: &[(i16, i16, u8)], x: f64, baseline: f64, scale: f64) {
    if points.is_empty() {
        return;
    }
    let to_px = |p: &(i16, i16, u8)| (x + p.0 as f64 * scale, baseline - p.1 as f64 * scale);
    let n = points.len();
    // Find an on-curve start point (or the middle between the first two control points)
    let (start, first_index) = match points.iter().position(|p| p.2 != 0) {
        Some(i) => (to_px(&points[i]), i),
        None => {
            let (a, b) = (to_px(&points[0]), to_px(&points[1 % n]));
            (((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), 0)
        }
    };
    path.move_to(start.0, start.1);
    let mut control: Option<(f64, f64)> = None;
    for k in 1..=n {
        let p = &points[(first_index + k) % n];
        let pt = to_px(p);
        if p.2 != 0 {
            match control.take() {
                Some(c) => path.quad_to(c.0, c.1, pt.0, pt.1),
                None => path.line_to(pt.0, pt.1),
            }
        } else if let Some(c) = control {
            let mid = ((c.0 + pt.0) / 2.0, (c.1 + pt.1) / 2.0);
            path.quad_to(c.0, c.1, mid.0, mid.1);
            control = Some(pt);
        } else {
            control = Some(pt);
        }
    }
    if let Some(c) = control {
        path.quad_to(c.0, c.1, start.0, start.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics() {
        assert!(glyph('A').is_some());
        assert!(glyph('é').is_some());
        assert!(glyph('\u{4e00}').is_none());
        // DejaVu Sans: "T" is 1493 / 2048 em high
        assert!((cap_height(20.0) - 14.58).abs() < 0.01);
        assert!(text_width("WW", 20.0) > text_width("W", 20.0));
        assert_eq!(text_width("", 20.0), 0.0);
        assert_eq!(text_width("  ", 20.0), 0.0);
        // Unknown characters are replaced
        assert!(text_width("\u{4e00}", 20.0) > 0.0);
    }

    #[test]
    fn outline() {
        let mut path = Path::new();
        append_text(&mut path, "O", 0.0, 20.0, 20.0);
        let mask = path.rasterize((0, 0, 40, 40)).unwrap();
        // The hole of the "O" is empty, the ring is filled
        let at = |x: i32, y: i32| mask.coverage[(y - mask.top) as usize * mask.width + (x - mask.left) as usize];
        assert!(at(8, 13) < 0.01, "{}", at(8, 13));
        assert!(at(2, 13) > 0.9, "{}", at(2, 13));
    }
}
