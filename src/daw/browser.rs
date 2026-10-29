// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A minimal browser (former `IBrowser` / `BrowserImpl`). Only browsing the presets of the
//! cursor device is supported; device browsing (which requires the plugin database and the
//! filter columns of the Java version) and the browser window are not ported.

use crate::backend::Processor;

use super::context::DawContext;

/// The content types of the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserContentType {
    Device,
    Preset,
}

impl BrowserContentType {
    pub fn name(self) -> &'static str {
        match self {
            BrowserContentType::Device => "Devices",
            BrowserContentType::Preset => "Presets",
        }
    }
}

const INSERT_DEVICE_BEFORE: &str = "Insert device before: ";
const INSERT_DEVICE_AFTER: &str = "Insert device after: ";

/// The browser.
pub struct Browser {
    ctx: DawContext,
    content_type: BrowserContentType,
    is_active: bool,
    presets: Vec<String>,
    selected_index: i32,
    num_results: usize,
    num_filter_column_entries: usize,
    insert_position: i32,
    info_text: String,
}

impl Browser {
    pub fn new(ctx: DawContext, num_filter_column_entries: usize, num_results: usize) -> Self {
        Browser {
            ctx,
            content_type: BrowserContentType::Preset,
            is_active: false,
            presets: Vec::new(),
            selected_index: 0,
            num_results,
            num_filter_column_entries,
            insert_position: -1,
            info_text: String::new(),
        }
    }

    pub fn enable_observers(&self, enable: bool) {
        self.ctx.enable_updates(Processor::Browser, enable);
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// A text describing the current browsing action (e.g. "Replace: ReaEQ").
    pub fn info_text(&self) -> &str {
        &self.info_text
    }

    pub fn content_type(&self) -> BrowserContentType {
        self.content_type
    }

    pub fn is_preset_content_type(&self) -> bool {
        self.content_type == BrowserContentType::Preset
    }

    pub fn selected_content_type_index(&self) -> usize {
        self.content_type as usize
    }

    pub fn selected_content_type(&self) -> &'static str {
        self.content_type.name()
    }

    pub fn content_type_names(&self) -> Vec<&'static str> {
        vec![self.selected_content_type()]
    }

    /// No content type switching.
    pub fn has_previous_content_type(&self) -> bool {
        false
    }

    /// No content type switching.
    pub fn has_next_content_type(&self) -> bool {
        false
    }

    pub fn previous_content_type(&mut self) {}

    pub fn next_content_type(&mut self) {}

    pub fn is_preview_enabled(&self) -> bool {
        false
    }

    pub fn toggle_preview_enabled(&mut self) {}

    pub fn set_preview_enabled(&mut self, _enabled: bool) {}

    pub fn num_filter_column_entries(&self) -> usize {
        self.num_filter_column_entries
    }

    /// No filter columns are supported.
    pub fn select_previous_filter_item(&mut self, _column: usize) {}

    /// No filter columns are supported.
    pub fn select_next_filter_item(&mut self, _column: usize) {}

    /// Browse the presets of the cursor device (former `replace(cursorDevice)`).
    /// `device_name` and `device_position` are the ones of the cursor device.
    pub fn replace_device(&mut self, device_name: &str, device_position: i32) {
        self.info_text = format!("Replace: {}", if device_name.is_empty() { "Empty" } else { device_name });
        self.browse(BrowserContentType::Preset, device_position);
    }

    /// Browse for a device to add to a channel (device browsing is not supported).
    pub fn add_device(&mut self, channel_name: &str) {
        self.info_text = format!("Add device to: {channel_name}");
        self.browse(BrowserContentType::Device, 0);
    }

    /// Browse for a device to insert before the cursor device.
    pub fn insert_before_cursor_device(&mut self, device_name: &str, device_position: i32) {
        self.info_text = format!("{INSERT_DEVICE_BEFORE}{device_name}");
        self.browse(BrowserContentType::Device, device_position);
    }

    /// Browse for a device to insert after the cursor device.
    pub fn insert_after_cursor_device(&mut self, device_name: &str, device_position: i32) {
        self.info_text = format!("{INSERT_DEVICE_AFTER}{device_name}");
        self.browse(BrowserContentType::Device, device_position + 1);
    }

    fn browse(&mut self, content_type: BrowserContentType, insert_position: i32) {
        self.stop_browsing(false);
        self.enable_observers(true);
        self.insert_position = insert_position;
        self.content_type = content_type;
        self.selected_index = 0;
        self.is_active = true;
    }

    /// Stop browsing. If `commit_selection` is true the selected preset is loaded.
    pub fn stop_browsing(&mut self, commit_selection: bool) {
        if !self.is_active {
            return;
        }
        if commit_selection && self.insert_position >= 0 && self.content_type == BrowserContentType::Preset {
            let index = self.selected_result_index();
            if index != -1 {
                self.ctx.send_int(Processor::Device, "preset", index);
            }
        }
        self.enable_observers(false);
        self.insert_position = -1;
        self.is_active = false;
    }

    fn result_count(&self) -> usize {
        if self.is_preset_content_type() {
            self.presets.len()
        } else {
            0
        }
    }

    /// The index of the selected result (-1 if there are no results).
    pub fn selected_result_index(&self) -> i32 {
        if self.result_count() == 0 {
            -1
        } else {
            self.selected_index
        }
    }

    /// The name of the selected result.
    pub fn selected_result(&self) -> Option<&str> {
        if !self.is_preset_content_type() {
            return None;
        }
        self.presets.get(self.selected_index.max(0) as usize).map(|s| s.as_str())
    }

    /// The names of the results of the current result page.
    pub fn result_page(&self) -> Vec<String> {
        if !self.is_preset_content_type() || self.num_results == 0 {
            return Vec::new();
        }
        let start = (self.selected_index.max(0) as usize) / self.num_results * self.num_results;
        (start..start + self.num_results).map(|i| self.presets.get(i).cloned().unwrap_or_default()).collect()
    }

    pub fn num_results(&self) -> usize {
        self.num_results
    }

    pub fn set_selected_result(&mut self, index: i32) {
        let length = self.result_count() as i32;
        self.selected_index = index.clamp(0, (length - 1).max(0));
    }

    pub fn select_previous_result(&mut self) {
        self.set_selected_result(self.selected_index - 1);
    }

    pub fn select_next_result(&mut self) {
        self.set_selected_result(self.selected_index + 1);
    }

    pub fn previous_result_page(&mut self) {
        self.set_selected_result(self.selected_index - self.num_results as i32);
    }

    pub fn next_result_page(&mut self) {
        self.set_selected_result(self.selected_index + self.num_results as i32);
    }

    /// The presets of the cursor device.
    pub fn presets(&self) -> &[String] {
        &self.presets
    }

    // Internal setters

    pub fn set_preset_selected(&mut self, index: i32) {
        self.selected_index = index;
    }

    /// Load the preset names from the given Reaper preset INI file.
    pub fn set_presets_file(&mut self, filename: &str) {
        let Ok(content) = std::fs::read_to_string(filename) else {
            return;
        };
        self.set_presets(parse_preset_file(&content));
    }

    pub fn set_presets(&mut self, presets: Vec<String>) {
        self.presets = presets;
        self.selected_index = 0;
    }
}

/// Parse the preset names (`[PresetN]` `Name=...`) of a Reaper preset INI file.
pub fn parse_preset_file(content: &str) -> Vec<String> {
    let mut section = String::new();
    let mut count = 0usize;
    let mut names: Vec<(usize, String)> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if section == "General" && key.trim() == "NbPresets" {
            count = value.trim().parse().unwrap_or(0);
        } else if let Some(n) = section.strip_prefix("Preset") {
            if key.trim() == "Name" {
                if let Ok(n) = n.parse::<usize>() {
                    names.push((n, value.to_string()));
                }
            }
        }
    }
    let mut presets = vec![String::new(); count];
    for (n, name) in names {
        if n < count {
            presets[n] = name;
        }
    }
    presets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets() {
        let p = parse_preset_file("[General]\nNbPresets=2\n[Preset0]\nName=A\n[Preset1]\nData=x\nName=B\n");
        assert_eq!(p, vec!["A".to_string(), "B".to_string()]);
    }
}
