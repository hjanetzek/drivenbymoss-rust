// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Maps color keys to controller specific color indices (former `ColorManager`).
//!
//! **RGB colors** (of tracks, clips, scenes, drum pads, and the slots of the surface link) take
//! one of two paths to a palette index ([`ColorManager::color_id`],
//! [`ColorManager::index_for_color`]):
//!
//! - [`ColorSource::Bitwig`]: the closest of Bitwig's track colors ([`DawColor::closest`]), then
//!   the controller's table of the DAW colors (adopted from the Java version). Bitwig's colors
//!   are exactly these entries, so this is exact for Bitwig and the behaviour of the Java version
//!   (conformance).
//! - [`ColorSource::Direct`]: the perceptually nearest entry of the controller's own palette
//!   ([`ColorManager::set_palette_rgb`], CIEDE2000), for all other hosts and for the surface
//!   link. A controller without a palette table keeps the Bitwig path.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::daw::color::{ColorEx, DawColor};

pub const BUTTON_STATE_OFF: &str = "BUTTON_STATE_OFF";
pub const BUTTON_STATE_ON: &str = "BUTTON_STATE_ON";
pub const BUTTON_STATE_HI: &str = "BUTTON_STATE_HI";

/// Maps color IDs (e.g. "BUTTON_STATE_ON" or DAW color names) to controller color indices and
/// color indices to RGB values (only used for display purposes).
#[derive(Debug, Default, Clone)]
pub struct ColorManager {
    color_index_by_key: HashMap<String, i32>,
    color_by_index: HashMap<i32, (f64, f64, f64)>,
    source: ColorSource,
    /// The palette entries for the direct path: the index and its color in CIE L*a*b*.
    palette: Vec<(i32, [f64; 3])>,
    /// The nearest palette index of the RGB colors looked up so far (encoded RGB).
    nearest_cache: RefCell<HashMap<i32, i32>>,
}

/// Where the RGB colors come from, which decides their path to the palette (see the module
/// documentation).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ColorSource {
    /// The host is Bitwig (its colors are Bitwig's track colors): over the DAW color table, as
    /// the Java version.
    #[default]
    Bitwig,
    /// Any other host: the nearest palette entry.
    Direct,
}

impl ColorSource {
    /// The source for a host by the application name and version which it gives at the creation
    /// (`dbm_create_controller`'s `app_version`): Bitwig if it starts with "Bitwig" (the Bitwig
    /// extension gives `getHostProduct () + " " + getHostVersion ()`, "Bitwig Studio 5.3.13";
    /// the conformance runs and their replays, which compare with the Java version on Bitwig,
    /// give the same), direct for all others (the reference host, zynthian, Reaper).
    pub fn for_host(app_version: &str) -> ColorSource {
        if app_version.trim_start().to_ascii_lowercase().starts_with("bitwig") {
            ColorSource::Bitwig
        } else {
            ColorSource::Direct
        }
    }
}

/// The entries of a palette in RGB (0..255) for [`ColorManager::set_palette_rgb`], index i at i.
pub fn palette_from_rgb255(rgb: &[[i32; 3]]) -> Vec<(i32, ColorEx)> {
    rgb.iter().enumerate().map(|(i, [r, g, b])| (i as i32, ColorEx::from_rgb(*r, *g, *b))).collect()
}

/// The keys of the palette indices for the direct path ([`ColorManager::color_id`]).
const PALETTE_KEYS: [&str; 128] = palette_keys();

const fn palette_keys() -> [&'static str; 128] {
    macro_rules! keys {
        ($($i:literal)*) => { [$(concat!("PALETTE_", $i)),*] };
    }
    keys!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
        32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63
        64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80 81 82 83 84 85 86 87 88 89 90 91 92 93 94 95
        96 97 98 99 100 101 102 103 104 105 106 107 108 109 110 111 112 113 114 115 116 117 118 119 120 121 122 123 124 125 126 127)
}

impl ColorManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a color index for a key. Panics if the key is already registered (programming
    /// error, same as the Java version).
    pub fn register_color_index(&mut self, key: &str, color_index: i32) {
        assert!(!self.color_index_by_key.contains_key(key), "Color for key {key} is already registered!");
        self.update_color_index(key, color_index);
    }

    pub fn update_color_index(&mut self, key: &str, color_index: i32) {
        self.color_index_by_key.insert(key.to_string(), color_index);
    }

    /// Get the color index for a key. Unknown keys are logged in Java with an exception; here
    /// they return 0 (off) to keep the surface working.
    pub fn color_index(&self, key: &str) -> i32 {
        match self.color_index_by_key.get(key) {
            Some(index) => *index,
            None => {
                debug_assert!(false, "Color for key {key} is not registered!");
                0
            }
        }
    }

    /// Set where the RGB colors come from (by the host, see [`ColorSource::for_host`]).
    pub fn set_color_source(&mut self, source: ColorSource) {
        self.source = source;
    }

    pub fn color_source(&self) -> ColorSource {
        self.source
    }

    /// The RGB colors of the palette entries which the direct path may choose (the controller's
    /// own palette, as the device shows them). Black entries (off) are left out: an RGB color is
    /// never mapped to off (as on the Bitwig path, where black becomes a grey). Also registers
    /// the keys of [`Self::color_id`] for these indices.
    pub fn set_palette_rgb(&mut self, entries: &[(i32, ColorEx)]) {
        self.palette.clear();
        self.nearest_cache.borrow_mut().clear();
        for (index, color) in entries {
            assert!((0..=127).contains(index), "Color index must be in the range of 0..127!");
            if color.to_int_rgb255() != [0, 0, 0] {
                self.palette.push((*index, color.to_lab()));
            }
            self.update_color_index(PALETTE_KEYS[*index as usize], *index);
        }
    }

    /// True if the controller has a palette table for the direct path.
    pub fn has_palette_rgb(&self) -> bool {
        !self.palette.is_empty()
    }

    /// True if the RGB colors take the direct path (not Bitwig, and a palette table).
    pub fn is_direct(&self) -> bool {
        self.source == ColorSource::Direct && self.has_palette_rgb()
    }

    /// The color ID of an RGB color: on the Bitwig path the ID of the closest DAW color
    /// ([`DawColor::color_id`]); on the direct path the key of the nearest palette entry
    /// ([`Self::nearest_palette_index`]). [`Self::color_index`] resolves both.
    pub fn color_id(&self, color: ColorEx) -> &'static str {
        if self.is_direct() {
            self.direct_color_id(color)
        } else {
            DawColor::color_id(color)
        }
    }

    /// The palette index of an RGB color (the index of [`Self::color_id`]).
    pub fn index_for_color(&self, color: ColorEx) -> i32 {
        self.color_index(self.color_id(color))
    }

    /// The color ID of an RGB color which never comes from Bitwig (the surface link): the
    /// nearest palette entry if the controller has a palette table, otherwise the closest DAW
    /// color.
    pub fn direct_color_id(&self, color: ColorEx) -> &'static str {
        match self.nearest_palette_index(color) {
            Some(index) => PALETTE_KEYS[index as usize],
            None => DawColor::color_id(color),
        }
    }

    /// The index of the palette entry which is perceptually nearest to the color (CIEDE2000 in
    /// CIE L*a*b*), None without a palette table.
    pub fn nearest_palette_index(&self, color: ColorEx) -> Option<i32> {
        if self.palette.is_empty() {
            return None;
        }
        let key = color.encode();
        if let Some(index) = self.nearest_cache.borrow().get(&key) {
            return Some(*index);
        }
        let lab = color.to_lab();
        let mut best = (f64::MAX, 0);
        for (index, entry) in &self.palette {
            let distance = ColorEx::perceptual_distance(lab, *entry);
            if distance < best.0 {
                best = (distance, *index);
            }
        }
        let mut cache = self.nearest_cache.borrow_mut();
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(key, best.1);
        Some(best.1)
    }

    pub fn has_color_index(&self, key: &str) -> bool {
        self.color_index_by_key.contains_key(key)
    }

    pub fn register_color(&mut self, color_index: i32, rgb: (f64, f64, f64)) {
        assert!((0..=127).contains(&color_index), "Color index must be in the range of 0..127!");
        self.color_by_index.insert(color_index, rgb);
    }

    /// Get the RGB color for a color index; black for negative or unknown indices.
    pub fn color(&self, color_index: i32) -> (f64, f64, f64) {
        if color_index < 0 {
            return (0.0, 0.0, 0.0);
        }
        self.color_by_index.get(&color_index).copied().unwrap_or((0.0, 0.0, 0.0))
    }
}
