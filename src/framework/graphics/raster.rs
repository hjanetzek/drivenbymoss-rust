// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A small anti-aliasing path rasterizer (replaces Cairo, which draws the graphics in the Java
//! Bitwig version).
//!
//! A [`Path`] consists of closed polygons (lines, quadratic curves and arcs are flattened). It
//! is rasterized into a [`Mask`] with the exact area coverage of every pixel: the signed area
//! of every line segment is accumulated per pixel and the running sum of a row is the coverage
//! (the accumulation approach of font-rs / stb_truetype). The fill rule is "non-zero" for
//! non-overlapping contours (the coverage is clamped to 1, holes of opposite direction cancel
//! out), which suffices for glyphs, rounded rectangles, circles and triangles.

/// Closed polygons in pixel coordinates (y down).
#[derive(Debug, Clone, Default)]
pub struct Path {
    contours: Vec<Vec<(f64, f64)>>,
}

impl Path {
    pub fn new() -> Self {
        Path::default()
    }

    /// Start a new contour (former Cairo `moveTo` / `newSubPath`).
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.contours.push(vec![(x, y)]);
    }

    /// Add a line to the current contour; starts a contour if there is none.
    pub fn line_to(&mut self, x: f64, y: f64) {
        match self.contours.last_mut() {
            Some(contour) => contour.push((x, y)),
            None => self.move_to(x, y),
        }
    }

    fn current(&self) -> Option<(f64, f64)> {
        self.contours.last().and_then(|c| c.last().copied())
    }

    /// Add a quadratic Bézier curve from the current point.
    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        let Some((x0, y0)) = self.current() else {
            self.move_to(x, y);
            return;
        };
        let dx = x0 - 2.0 * cx + x;
        let dy = y0 - 2.0 * cy + y;
        let segments = ((dx * dx + dy * dy).sqrt().sqrt() * 1.5).ceil().clamp(1.0, 16.0) as usize;
        for i in 1..=segments {
            let t = i as f64 / segments as f64;
            let mt = 1.0 - t;
            self.line_to(mt * mt * x0 + 2.0 * mt * t * cx + t * t * x, mt * mt * y0 + 2.0 * mt * t * cy + t * t * y);
        }
    }

    /// Add a circular arc (angles in radians, clockwise on the screen like Cairo `arc`); a line
    /// connects the current point with the start of the arc.
    pub fn arc(&mut self, cx: f64, cy: f64, radius: f64, angle1: f64, angle2: f64) {
        let sweep = angle2 - angle1;
        let segments = ((sweep.abs() * radius.max(0.0)) / 0.5).ceil().clamp(1.0, 256.0) as usize;
        for i in 0..=segments {
            let a = angle1 + sweep * i as f64 / segments as f64;
            self.line_to(cx + radius * a.cos(), cy + radius * a.sin());
        }
    }

    /// Add a closed rectangle.
    pub fn rectangle(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.move_to(x, y);
        self.line_to(x + width, y);
        self.line_to(x + width, y + height);
        self.line_to(x, y + height);
    }

    /// Add a closed rectangle with rounded corners (the Bitwig `drawRoundedRectInternal`).
    pub fn rounded_rectangle(&mut self, left: f64, top: f64, width: f64, height: f64, radius: f64) {
        use std::f64::consts::PI;
        let degrees = PI / 180.0;
        self.contours.push(Vec::new());
        self.arc(left + width - radius, top + radius, radius, -90.0 * degrees, 0.0);
        self.arc(left + width - radius, top + height - radius, radius, 0.0, 90.0 * degrees);
        self.arc(left + radius, top + height - radius, radius, 90.0 * degrees, 180.0 * degrees);
        self.arc(left + radius, top + radius, radius, 180.0 * degrees, 270.0 * degrees);
    }

    /// Add a closed circle.
    pub fn circle(&mut self, x: f64, y: f64, radius: f64) {
        self.contours.push(Vec::new());
        self.arc(x, y, radius, 0.0, 2.0 * std::f64::consts::PI);
    }

    /// Add a line as a closed quadrilateral of the given width.
    pub fn stroke_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64) {
        let (dx, dy) = (x2 - x1, y2 - y1);
        let length = (dx * dx + dy * dy).sqrt();
        if length == 0.0 {
            return;
        }
        let (nx, ny) = (-dy / length * width / 2.0, dx / length * width / 2.0);
        self.move_to(x1 + nx, y1 + ny);
        self.line_to(x2 + nx, y2 + ny);
        self.line_to(x2 - nx, y2 - ny);
        self.line_to(x1 - nx, y1 - ny);
    }

    pub fn is_empty(&self) -> bool {
        self.contours.iter().all(|c| c.len() < 2)
    }

    /// The bounding box: left, top, right, bottom.
    fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut points = self.contours.iter().flatten();
        let first = points.next()?;
        let mut b = (first.0, first.1, first.0, first.1);
        for &(x, y) in points {
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
        Some(b)
    }

    /// Rasterize the path into a mask which covers the path and the given clipping area (left,
    /// top, right, bottom; exclusive); `None` if nothing is visible.
    pub fn rasterize(&self, clip: (i32, i32, i32, i32)) -> Option<Mask> {
        let (l, t, r, b) = self.bounds()?;
        if !(l.is_finite() && t.is_finite() && r.is_finite() && b.is_finite()) {
            return None;
        }
        let left = (l.floor() as i32).max(clip.0);
        let top = (t.floor() as i32).max(clip.1);
        let right = (r.ceil() as i32).min(clip.2);
        let bottom = (b.ceil() as i32).min(clip.3);
        if left >= right || top >= bottom {
            return None;
        }
        let mut acc = Accumulator::new(left, top, (right - left) as usize, (bottom - top) as usize);
        for contour in &self.contours {
            if contour.len() < 2 {
                continue;
            }
            for i in 0..contour.len() {
                acc.line(contour[i], contour[(i + 1) % contour.len()]);
            }
        }
        Some(acc.finish())
    }
}

/// The coverage (0..1) of the pixels of a rectangular area.
#[derive(Debug, Clone)]
pub struct Mask {
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<f32>,
}

struct Accumulator {
    left: i32,
    top: i32,
    width: usize,
    height: usize,
    stride: usize,
    acc: Vec<f32>,
}

impl Accumulator {
    fn new(left: i32, top: i32, width: usize, height: usize) -> Self {
        let stride = width + 2;
        Accumulator { left, top, width, height, stride, acc: vec![0.0; stride * height] }
    }

    fn line(&mut self, p0: (f64, f64), p1: (f64, f64)) {
        let (x0, y0) = (p0.0 - self.left as f64, p0.1 - self.top as f64);
        let (x1, y1) = (p1.0 - self.left as f64, p1.1 - self.top as f64);
        if y0 == y1 {
            return;
        }
        let (dir, (x0, y0), (x1, y1)) = if y0 < y1 { (1.0, (x0, y0), (x1, y1)) } else { (-1.0, (x1, y1), (x0, y0)) };
        let h = self.height as f64;
        if y1 <= 0.0 || y0 >= h {
            return;
        }
        let dxdy = (x1 - x0) / (y1 - y0);
        let ys = y0.max(0.0);
        let ye = y1.min(h);
        let mut x = x0 + (ys - y0) * dxdy;
        let max_x = self.width as f64;
        let row_start = ys.floor() as usize;
        let row_end = (ye.ceil() as usize).min(self.height);
        for row in row_start..row_end {
            let dy = ((row + 1) as f64).min(ye) - (row as f64).max(ys);
            let x_next = x + dxdy * dy;
            let d = (dy * dir) as f32;
            // Clamping to the area keeps the winding of the parts which are cut off
            let xa = x.clamp(0.0, max_x);
            let xb = x_next.clamp(0.0, max_x);
            self.accumulate(row * self.stride, xa, xb, d);
            x = x_next;
        }
    }

    fn accumulate(&mut self, line_start: usize, xa: f64, xb: f64, d: f32) {
        let acc = &mut self.acc[line_start..line_start + self.stride];
        let (x0, x1) = if xa < xb { (xa, xb) } else { (xb, xa) };
        let x0_floor = x0.floor();
        let x0i = x0_floor as usize;
        let x1_ceil = x1.ceil();
        let x1i = x1_ceil as usize;
        if x1i <= x0i + 1 {
            let xmf = (0.5 * (xa + xb) - x0_floor) as f32;
            acc[x0i] += d - d * xmf;
            acc[x0i + 1] += d * xmf;
        } else {
            let s = (1.0 / (x1 - x0)) as f32;
            let x0f = (x0 - x0_floor) as f32;
            let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
            let x1f = (x1 - x1_ceil + 1.0) as f32;
            let am = 0.5 * s * x1f * x1f;
            acc[x0i] += d * a0;
            if x1i == x0i + 2 {
                acc[x0i + 1] += d * (1.0 - a0 - am);
            } else {
                let a1 = s * (1.5 - x0f);
                acc[x0i + 1] += d * (a1 - a0);
                for value in &mut acc[x0i + 2..x1i - 1] {
                    *value += d * s;
                }
                let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                acc[x1i - 1] += d * (1.0 - a2 - am);
            }
            acc[x1i] += d * am;
        }
    }

    fn finish(self) -> Mask {
        let mut coverage = Vec::with_capacity(self.width * self.height);
        for row in 0..self.height {
            let mut sum = 0.0f32;
            for value in &self.acc[row * self.stride..row * self.stride + self.width] {
                sum += value;
                coverage.push(sum.abs().min(1.0));
            }
        }
        Mask { left: self.left, top: self.top, width: self.width, height: self.height, coverage }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cov(mask: &Mask, x: i32, y: i32) -> f32 {
        mask.coverage[(y - mask.top) as usize * mask.width + (x - mask.left) as usize]
    }

    #[test]
    fn rectangle_coverage() {
        let mut path = Path::new();
        path.rectangle(1.5, 1.0, 2.0, 2.0);
        let mask = path.rasterize((0, 0, 10, 10)).unwrap();
        assert!((cov(&mask, 1, 1) - 0.5).abs() < 1e-5);
        assert!((cov(&mask, 2, 1) - 1.0).abs() < 1e-5);
        assert!((cov(&mask, 3, 2) - 0.5).abs() < 1e-5);
        // Clipped
        let mask = path.rasterize((2, 0, 3, 10)).unwrap();
        assert_eq!((mask.left, mask.width), (2, 1));
        assert!((cov(&mask, 2, 2) - 1.0).abs() < 1e-5);
        assert!(path.rasterize((5, 5, 10, 10)).is_none());
    }

    #[test]
    fn circle_and_triangle() {
        let mut path = Path::new();
        path.circle(5.0, 5.0, 4.0);
        let mask = path.rasterize((0, 0, 10, 10)).unwrap();
        assert!(cov(&mask, 5, 5) > 0.99);
        assert!(cov(&mask, 1, 1) < 0.2);
        let total: f32 = mask.coverage.iter().sum();
        assert!((total as f64 - std::f64::consts::PI * 16.0).abs() < 0.5, "{total}");

        let mut path = Path::new();
        path.move_to(0.0, 0.0);
        path.line_to(8.0, 0.0);
        path.line_to(0.0, 8.0);
        let total: f32 = path.rasterize((0, 0, 10, 10)).unwrap().coverage.iter().sum();
        assert!((total - 32.0).abs() < 1e-3, "{total}");
    }
}
