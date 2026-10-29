// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Maps color keys to controller specific color indices (former `ColorManager`).

use std::collections::HashMap;

pub const BUTTON_STATE_OFF: &str = "BUTTON_STATE_OFF";
pub const BUTTON_STATE_ON: &str = "BUTTON_STATE_ON";
pub const BUTTON_STATE_HI: &str = "BUTTON_STATE_HI";

/// Maps color IDs (e.g. "BUTTON_STATE_ON" or DAW color names) to controller color indices and
/// color indices to RGB values (only used for display purposes).
#[derive(Debug, Default, Clone)]
pub struct ColorManager {
    color_index_by_key: HashMap<String, i32>,
    color_by_index: HashMap<i32, (f64, f64, f64)>,
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
