// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The USB frame format of the Push 2/3 display (former `PushUsbDisplay`).
//!
//! A frame is a 16 byte header (`FF CC AA 88` and 12 zero bytes) followed by 160 lines of 960
//! pixels in 16 bit BGR565 (red in the lowest 5 bits, green in the middle 6, blue in the
//! highest 5; little endian) and 128 filler bytes each: 160 * 2048 = 327,680 data bytes
//! (Java `DATA_SZ = 20 * 0x4000`). The data bytes (not the header) are XOR-ed with the pattern
//! `E7 F3 E7 FF` ("signal shaping"). This matches the Java code and Ableton's Push 2 display
//! interface description.
//!
//! Differences to Java:
//! - [`encode_frame`] produces header and data in one buffer ([`FRAME_SIZE`] bytes). Java sends
//!   them as two bulk transfers (16 bytes, then 327,680 bytes). Split at [`HEADER_SIZE`] to
//!   send them separately.
//! - Java sends the image at every flush (in a separate thread). [`UsbDisplay::frame`] returns
//!   a changed image at most every [`MIN_FRAME_INTERVAL`] and an unchanged one at least every
//!   [`KEEP_ALIVE_INTERVAL`], because the display turns black after about 2 seconds without a
//!   frame.

use std::time::{Duration, Instant};

use crate::framework::graphics::Bitmap;

/// The USB vendor ID of Ableton.
pub const VENDOR_ID: u16 = 0x2982;
/// The USB product ID of the Push 2.
pub const PUSH2_PRODUCT_ID: u16 = 0x1967;
/// The USB product ID of the Push 3.
pub const PUSH3_PRODUCT_ID: u16 = 0x1969;
/// The USB interface of the display.
pub const INTERFACE: u8 = 0;
/// The bulk out endpoint of the display.
pub const ENDPOINT: u8 = 0x01;

/// The width of the display in pixels.
pub const WIDTH: usize = 960;
/// The height of the display in pixels.
pub const HEIGHT: usize = 160;
/// The bytes of a line incl. the filler.
pub const LINE_SIZE: usize = 2048;
/// The size of the frame header.
pub const HEADER_SIZE: usize = 16;
/// The size of the pixel data (former `DATA_SZ`).
pub const DATA_SIZE: usize = HEIGHT * LINE_SIZE;
/// The size of a complete frame: header and data.
pub const FRAME_SIZE: usize = HEADER_SIZE + DATA_SIZE;

/// The minimum time between two frames (about 30 frames per second).
pub const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// The maximum time between two frames (the display blanks after about 2 seconds).
pub const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(1);

/// The frame header (former `DISPLAY_HEADER`).
pub const DISPLAY_HEADER: [u8; HEADER_SIZE] = [0xFF, 0xCC, 0xAA, 0x88, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// The XOR pattern of the data (former `signalShaping`).
const MASK: [u8; 4] = [0xE7, 0xF3, 0xE7, 0xFF];

/// Convert a color to the 16 bit pixel format of the display (former `sPixelFromRGB`).
pub fn pixel_from_rgb(red: u8, green: u8, blue: u8) -> u16 {
    let mut pixel = ((blue & 0xF8) >> 3) as u16;
    pixel <<= 6;
    pixel += ((green & 0xFC) >> 2) as u16;
    pixel <<= 5;
    pixel += ((red & 0xF8) >> 3) as u16;
    pixel
}

/// Encode a bitmap into the bytes of a frame (header and masked data, [`FRAME_SIZE`] bytes).
/// The bitmap should be 960x160; other sizes are cropped or filled with black.
pub fn encode_frame(bitmap: &Bitmap, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(FRAME_SIZE);
    out.extend_from_slice(&DISPLAY_HEADER);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b] = bitmap.pixel(x as i32, y as i32);
            out.extend_from_slice(&pixel_from_rgb(r, g, b).to_le_bytes());
        }
        out.resize(out.len() + LINE_SIZE - 2 * WIDTH, 0);
    }
    for (i, byte) in out[HEADER_SIZE..].iter_mut().enumerate() {
        *byte ^= MASK[i % 4];
    }
}

/// The sender state of the display: decides when a frame is sent and keeps the frame buffer.
#[derive(Debug, Default)]
pub struct UsbDisplay {
    buffer: Vec<u8>,
    last_sent: Option<Instant>,
    /// A changed image was not sent yet due to the rate limit.
    pending: bool,
}

impl UsbDisplay {
    pub fn new() -> Self {
        UsbDisplay::default()
    }

    /// Get the frame to send now, if any. `changed` is true if the bitmap changed since the
    /// last call (e.g. `GraphicDisplay::has_changed` after `render`). A changed image is sent
    /// at most every [`MIN_FRAME_INTERVAL`] (a rate-limited change is sent with a later call),
    /// an unchanged one is resent every [`KEEP_ALIVE_INTERVAL`]. The first call always returns
    /// a frame.
    pub fn frame(&mut self, bitmap: &Bitmap, changed: bool, now: Instant) -> Option<&[u8]> {
        self.pending |= changed;
        let send = match self.last_sent {
            None => true,
            Some(last) => {
                let elapsed = now.saturating_duration_since(last);
                (self.pending && elapsed >= MIN_FRAME_INTERVAL) || elapsed >= KEEP_ALIVE_INTERVAL
            }
        };
        if !send {
            return None;
        }
        // An unchanged image does not need to be encoded again
        if self.pending || self.buffer.len() != FRAME_SIZE {
            encode_frame(bitmap, &mut self.buffer);
        }
        self.pending = false;
        self.last_sent = Some(now);
        Some(&self.buffer)
    }

    /// Forget the last sent frame, e.g. after the device was reconnected: the next call of
    /// [`Self::frame`] sends at once.
    pub fn reset(&mut self) {
        self.last_sent = None;
        self.pending = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daw::color::ColorEx;

    fn filled(color: ColorEx) -> Bitmap {
        let mut bitmap = Bitmap::new(WIDTH as i32, HEIGHT as i32);
        bitmap.context().fill_rectangle(0.0, 0.0, WIDTH as f64, HEIGHT as f64, color);
        bitmap
    }

    fn unmask(frame: &[u8]) -> Vec<u8> {
        frame[HEADER_SIZE..].iter().enumerate().map(|(i, b)| b ^ MASK[i % 4]).collect()
    }

    #[test]
    fn header_length_and_mask() {
        let mut out = Vec::new();
        encode_frame(&filled(ColorEx::BLACK), &mut out);
        assert_eq!(FRAME_SIZE, 16 + 327_680);
        assert_eq!(out.len(), FRAME_SIZE);
        assert_eq!(out[..16], [0xFF, 0xCC, 0xAA, 0x88, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        // Black: only the mask
        assert_eq!(out[16..24], [0xE7, 0xF3, 0xE7, 0xFF, 0xE7, 0xF3, 0xE7, 0xFF]);
        assert!(out[16..].chunks(4).all(|c| c == MASK));

        // White: 0xFFFF pixels, filler 0
        encode_frame(&filled(ColorEx::WHITE), &mut out);
        assert_eq!(out[16..20], [0xFF ^ 0xE7, 0xFF ^ 0xF3, 0xFF ^ 0xE7, 0xFF ^ 0xFF]);
        let data = unmask(&out);
        for line in data.chunks(LINE_SIZE) {
            assert!(line[..2 * WIDTH].iter().all(|&b| b == 0xFF));
            assert!(line[2 * WIDTH..].iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn pixel_format() {
        // Red in the lowest 5 bits, green in the middle 6, blue in the highest 5
        assert_eq!(pixel_from_rgb(255, 0, 0), 0x001F);
        assert_eq!(pixel_from_rgb(0, 255, 0), 0x07E0);
        assert_eq!(pixel_from_rgb(0, 0, 255), 0xF800);
        assert_eq!(pixel_from_rgb(0x08, 0x04, 0x08), 0x0821);

        let mut out = Vec::new();
        encode_frame(&filled(ColorEx::RED), &mut out);
        // Little endian: low byte first
        assert_eq!(unmask(&out)[..4], [0x1F, 0x00, 0x1F, 0x00]);
        assert_eq!(out[16..20], [0x1F ^ 0xE7, 0xF3, 0x1F ^ 0xE7, 0xFF]);

        // A single blue pixel at (1, 2)
        let mut bitmap = Bitmap::new(WIDTH as i32, HEIGHT as i32);
        bitmap.set_pixel(1, 2, [0, 0, 255]);
        encode_frame(&bitmap, &mut out);
        let data = unmask(&out);
        let offset = 2 * LINE_SIZE + 2;
        assert_eq!(data[offset..offset + 2], [0x00, 0xF8]);
        assert_eq!(data.iter().filter(|&&b| b != 0).count(), 1);
    }

    #[test]
    fn rate_limit_and_keep_alive() {
        let bitmap = filled(ColorEx::BLACK);
        let mut display = UsbDisplay::new();
        let t0 = Instant::now();
        assert_eq!(display.frame(&bitmap, false, t0).map(|f| f.len()), Some(FRAME_SIZE));
        // Changed too early: delayed
        assert!(display.frame(&bitmap, true, t0 + Duration::from_millis(10)).is_none());
        // Unchanged afterwards, but the pending change is sent
        assert!(display.frame(&bitmap, false, t0 + Duration::from_millis(20)).is_none());
        assert!(display.frame(&bitmap, false, t0 + Duration::from_millis(40)).is_some());
        // Nothing changed: no frame until the keep alive
        let t1 = t0 + Duration::from_millis(40);
        assert!(display.frame(&bitmap, false, t1 + Duration::from_millis(500)).is_none());
        assert!(display.frame(&bitmap, false, t1 + Duration::from_millis(1000)).is_some());
        // Changed after the interval: at once
        let t2 = t1 + Duration::from_millis(1000);
        assert!(display.frame(&bitmap, true, t2 + Duration::from_millis(33)).is_some());
        // The frame contains the new image
        let red = filled(ColorEx::RED);
        let frame = display.frame(&red, true, t2 + Duration::from_millis(100)).unwrap();
        assert_eq!(unmask(frame)[0], 0x1F);
        display.reset();
        assert!(display.frame(&red, false, t2 + Duration::from_millis(101)).is_some());
    }
}
