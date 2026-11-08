// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The browser (former `IBrowser` / `BrowserImpl`). On Reaper only browsing the presets of the
//! cursor device is supported; device browsing (which requires the plugin database and the
//! filter columns of the Java version) and the browser window are not ported.
//!
//! A host with its own browser (Bitwig, protocol v2) reports `/browser/exists` and the content
//! types, filter columns and results; then all browser functions are commands to the host.

use crate::protocol::{Cmd, Dir};
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

/// An item of a filter column or of the results (former `IBrowserColumnItem`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BrowserColumnItem {
    pub exists: bool,
    pub name: String,
    pub is_selected: bool,
    pub hit_count: i32,
}

/// A filter column of a host browser (former `IBrowserColumn`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BrowserColumn {
    pub index: usize,
    pub exists: bool,
    pub name: String,
    /// The name of the item which does not filter (e.g. "Any Category").
    pub wildcard: String,
    pub cursor_exists: bool,
    pub cursor_name: String,
    /// The index of the cursor item on the page of `items`, -1 if it is not on the page.
    pub cursor_index: i32,
    pub items: Vec<BrowserColumnItem>,
}

/// The device chain to which a device is added (former `addDevice (IChannel)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceChainTarget {
    /// The track at the absolute position.
    Track(i32),
    Master,
}

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
    /// The host has its own browser (protocol v2): the fields below are reported by it.
    host_reported: bool,
    host_active: bool,
    content_type_names: Vec<String>,
    selected_content_type_index: usize,
    selected_content_type_name: String,
    preview: bool,
    columns: Vec<BrowserColumn>,
    results: Vec<BrowserColumnItem>,
    selected_result_name: String,
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
            host_reported: false,
            host_active: false,
            content_type_names: Vec::new(),
            selected_content_type_index: 0,
            selected_content_type_name: String::new(),
            preview: false,
            columns: Vec::new(),
            results: vec![BrowserColumnItem::default(); num_results],
            selected_result_name: String::new(),
        }
    }

    /// True if the host has its own browser (protocol v2).
    pub fn is_host_reported(&self) -> bool {
        self.host_reported
    }


    pub fn enable_observers(&self, enable: bool) {
        self.ctx.send(Cmd::enable_updates(Processor::Browser, enable));
    }

    pub fn is_active(&self) -> bool {
        if self.host_reported {
            self.host_active
        } else {
            self.is_active
        }
    }

    /// A text describing the current browsing action (e.g. "Replace: ReaEQ").
    pub fn info_text(&self) -> &str {
        &self.info_text
    }

    pub fn content_type(&self) -> BrowserContentType {
        self.content_type
    }

    /// The host browser: the content type with index 1 is the presets (as in the Java version).
    pub fn is_preset_content_type(&self) -> bool {
        if self.host_reported {
            self.selected_content_type_index == 1
        } else {
            self.content_type == BrowserContentType::Preset
        }
    }

    pub fn selected_content_type_index(&self) -> usize {
        if self.host_reported {
            self.selected_content_type_index
        } else {
            self.content_type as usize
        }
    }

    pub fn selected_content_type(&self) -> &str {
        if self.host_reported {
            &self.selected_content_type_name
        } else {
            self.content_type.name()
        }
    }

    pub fn content_type_names(&self) -> Vec<&str> {
        if self.host_reported {
            self.content_type_names.iter().map(String::as_str).collect()
        } else {
            vec![self.selected_content_type()]
        }
    }

    /// Without a host browser there is no content type switching.
    pub fn has_previous_content_type(&self) -> bool {
        self.host_reported && self.selected_content_type_index > 0
    }

    /// Without a host browser there is no content type switching.
    pub fn has_next_content_type(&self) -> bool {
        self.host_reported && self.selected_content_type_index + 1 < self.content_type_names.len()
    }

    pub fn previous_content_type(&mut self) {
        if self.host_reported {
            self.ctx.send(Cmd::BrowserContentType { dir: Dir::Prev });
        }
    }

    pub fn next_content_type(&mut self) {
        if self.host_reported {
            self.ctx.send(Cmd::BrowserContentType { dir: Dir::Next });
        }
    }

    pub fn is_preview_enabled(&self) -> bool {
        self.preview
    }

    pub fn toggle_preview_enabled(&mut self) {
        let enabled = !self.preview;
        self.set_preview_enabled(enabled);
    }

    pub fn set_preview_enabled(&mut self, enabled: bool) {
        if self.host_reported {
            self.ctx.send(Cmd::BrowserPreview { value: enabled });
        }
    }

    pub fn num_filter_column_entries(&self) -> usize {
        self.num_filter_column_entries
    }

    /// The number of filter columns; 0 without a host browser.
    pub fn filter_column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn filter_column(&self, column: usize) -> Option<&BrowserColumn> {
        self.columns.get(column)
    }

    pub fn filter_column_names(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    fn send_column(&self, column: usize, cmd: Cmd) {
        if self.host_reported && column < self.columns.len() {
            self.ctx.send(cmd);
        }
    }

    pub fn select_previous_filter_item(&mut self, column: usize) {
        self.send_column(column, Cmd::BrowserColumnStep { column, dir: Dir::Prev });
    }

    pub fn select_next_filter_item(&mut self, column: usize) {
        self.send_column(column, Cmd::BrowserColumnStep { column, dir: Dir::Next });
    }

    pub fn previous_filter_item_page(&mut self, column: usize) {
        self.send_column(column, Cmd::BrowserColumnPage { column, dir: Dir::Prev });
    }

    pub fn next_filter_item_page(&mut self, column: usize) {
        self.send_column(column, Cmd::BrowserColumnPage { column, dir: Dir::Next });
    }

    /// Select the wildcard item of the column (former `resetFilterColumn`).
    pub fn reset_filter_column(&mut self, column: usize) {
        self.send_column(column, Cmd::BrowserColumnReset { column });
    }

    /// The index of the cursor item on the page of the column, -1 if not on the page.
    pub fn selected_filter_item_index(&self, column: usize) -> i32 {
        self.columns.get(column).map_or(-1, |c| c.cursor_index)
    }

    /// Browse the presets of the cursor device (former `replace(cursorDevice)`).
    /// `device_name` and `device_position` are the ones of the cursor device.
    pub fn replace_device(&mut self, device_name: &str, device_position: i32) {
        self.info_text = format!("Replace: {}", if device_name.is_empty() { "Empty" } else { device_name });
        if self.host_reported {
            self.ctx.send(Cmd::BrowserReplaceDevice {});
            return;
        }
        self.browse(BrowserContentType::Preset, device_position);
    }

    /// Browse for a clip to load into a slot (former `replace(slot)`); only with a host browser.
    /// `track` and `slot` are absolute positions.
    pub fn replace_slot(&mut self, slot_name: &str, track: i32, slot: i32) {
        if self.host_reported {
            self.info_text = format!("Replace: {}", if slot_name.is_empty() { "Empty" } else { slot_name });
            self.ctx.send(Cmd::BrowserReplaceSlot { track, slot });
        }
    }

    /// Browse for the device of a drum pad (former `replace(drumPad)`); only with a host
    /// browser. `owner` is the processor of the pad's device (drum, primary), `pad` the pad's
    /// position (the note).
    pub fn replace_drum_pad(&mut self, owner: Processor, pad_name: &str, pad: i32) {
        if self.host_reported {
            self.info_text = format!("Replace: {}", if pad_name.is_empty() { "Empty" } else { pad_name });
            self.ctx.send(Cmd::BrowserReplacePad { owner, pad });
        }
    }

    /// Browse for a device to add to the start of a channel's device chain.
    pub fn add_device(&mut self, channel_name: &str, target: DeviceChainTarget) {
        self.info_text = format!("Add device to: {channel_name}");
        if self.host_reported {
            self.ctx.send(match target {
                DeviceChainTarget::Track(track) => Cmd::BrowserAddToTrack { track },
                DeviceChainTarget::Master => Cmd::BrowserAddToMaster {},
            });
            return;
        }
        self.browse(BrowserContentType::Device, 0);
    }

    /// Browse for a device to insert before the cursor device.
    pub fn insert_before_cursor_device(&mut self, device_name: &str, device_position: i32) {
        self.info_text = format!("{INSERT_DEVICE_BEFORE}{device_name}");
        if self.host_reported {
            self.ctx.send(Cmd::BrowserInsertBefore {});
            return;
        }
        self.browse(BrowserContentType::Device, device_position);
    }

    /// Browse for a device to insert after the cursor device.
    pub fn insert_after_cursor_device(&mut self, device_name: &str, device_position: i32) {
        self.info_text = format!("{INSERT_DEVICE_AFTER}{device_name}");
        if self.host_reported {
            self.ctx.send(Cmd::BrowserInsertAfter {});
            return;
        }
        self.browse(BrowserContentType::Device, device_position + 1);
    }

    /// Switch between inserting before and after the cursor device (former
    /// `toggleInsertionPoint`).
    pub fn toggle_insertion_point(&mut self, device_name: &str, device_position: i32) {
        if self.info_text.starts_with(INSERT_DEVICE_BEFORE) {
            self.insert_after_cursor_device(device_name, device_position);
        } else {
            self.insert_before_cursor_device(device_name, device_position);
        }
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
        if self.host_reported {
            // As the Java version: commit or cancel, even if not active
            self.ctx.send(Cmd::BrowserStop { commit: commit_selection });
            return;
        }
        if !self.is_active {
            return;
        }
        if commit_selection && self.insert_position >= 0 && self.content_type == BrowserContentType::Preset {
            let index = self.selected_result_index();
            if index != -1 {
                self.ctx.send(Cmd::DevicePreset { value: index });
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

    /// The index of the selected result (-1 if there are no results). With a host browser the
    /// index on the result page.
    pub fn selected_result_index(&self) -> i32 {
        if self.host_reported {
            return self.results.iter().position(|r| r.is_selected).map_or(-1, |i| i as i32);
        }
        if self.result_count() == 0 {
            -1
        } else {
            self.selected_index
        }
    }

    /// The name of the selected result.
    pub fn selected_result(&self) -> Option<&str> {
        if self.host_reported {
            return Some(&self.selected_result_name);
        }
        if !self.is_preset_content_type() {
            return None;
        }
        self.presets.get(self.selected_index.max(0) as usize).map(|s| s.as_str())
    }

    /// The names of the results of the current result page.
    pub fn result_page(&self) -> Vec<String> {
        if self.host_reported {
            return self.results.iter().map(|r| r.name.clone()).collect();
        }
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

    /// The items of the result page (former `getResultColumnItems`); empty without a host
    /// browser.
    pub fn result_items(&self) -> &[BrowserColumnItem] {
        if self.host_reported {
            &self.results
        } else {
            &[]
        }
    }

    pub fn select_previous_result(&mut self) {
        if self.host_reported {
            self.ctx.send(Cmd::BrowserResultStep { dir: Dir::Prev });
            return;
        }
        self.set_selected_result(self.selected_index - 1);
    }

    pub fn select_next_result(&mut self) {
        if self.host_reported {
            self.ctx.send(Cmd::BrowserResultStep { dir: Dir::Next });
            return;
        }
        self.set_selected_result(self.selected_index + 1);
    }

    pub fn previous_result_page(&mut self) {
        if self.host_reported {
            self.ctx.send(Cmd::BrowserResultPage { dir: Dir::Prev });
            return;
        }
        self.set_selected_result(self.selected_index - self.num_results as i32);
    }

    pub fn next_result_page(&mut self) {
        if self.host_reported {
            self.ctx.send(Cmd::BrowserResultPage { dir: Dir::Next });
            return;
        }
        self.set_selected_result(self.selected_index + self.num_results as i32);
    }

    /// The presets of the cursor device.
    pub fn presets(&self) -> &[String] {
        &self.presets
    }

    // Internal setters

    /// The host browser is open or closed (protocol v2).
    pub fn set_host_active(&mut self, active: bool) {
        self.host_reported = true;
        self.host_active = active;
    }

    pub fn set_content_type_count(&mut self, count: usize) {
        self.content_type_names.resize(count, String::new());
    }

    pub fn set_content_type_name(&mut self, index: usize, name: &str) {
        if index >= self.content_type_names.len() {
            self.content_type_names.resize(index + 1, String::new());
        }
        self.content_type_names[index] = name.to_string();
    }

    pub fn set_selected_content_type(&mut self, index: usize) {
        self.selected_content_type_index = index;
    }

    pub fn set_selected_content_type_name(&mut self, name: &str) {
        self.selected_content_type_name = name.to_string();
    }

    pub fn set_preview(&mut self, preview: bool) {
        self.preview = preview;
    }

    pub fn set_filter_column_count(&mut self, count: usize) {
        let entries = self.num_filter_column_entries;
        self.columns.resize_with(count, Default::default);
        for (index, column) in self.columns.iter_mut().enumerate() {
            column.index = index;
            column.items.resize_with(entries, Default::default);
        }
    }

    /// The filter column, created if the count has not been reported yet.
    pub fn filter_column_mut(&mut self, column: usize) -> &mut BrowserColumn {
        if column >= self.columns.len() {
            self.set_filter_column_count(column + 1);
        }
        &mut self.columns[column]
    }

    /// An item of the result page; None if outside of the page.
    pub fn result_item_mut(&mut self, index: usize) -> Option<&mut BrowserColumnItem> {
        self.results.get_mut(index)
    }

    pub fn set_selected_result_name(&mut self, name: &str) {
        self.selected_result_name = name.to_string();
    }

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
