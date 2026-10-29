// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The configuration of the size of the model (former `ModelSetup`).

use super::constants::DeviceId;

/// The configuration of the model: number of tracks, scenes, sends, parameters, etc.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSetup {
    pub has_flat_track_list: bool,
    pub has_full_flat_track_list: bool,
    pub num_tracks: usize,
    pub num_fx_tracks: i32,
    pub num_scenes: usize,
    pub num_sends: usize,
    pub wants_clip_launcher_navigator: bool,
    pub wants_drum_device: bool,
    pub additional_drum_devices: Vec<usize>,
    pub num_devices_in_bank: i32,
    pub num_device_layers: i32,
    pub num_param_pages: usize,
    pub num_params: i32,
    pub num_drum_pad_layers: i32,
    pub num_list_params: usize,
    pub wants_cursor_layer: bool,
    pub wants_focused_parameter: bool,
    pub num_filter_column_entries: usize,
    pub num_results: usize,
    pub num_markers: usize,
    pub device_ids: Vec<DeviceId>,
}

impl Default for ModelSetup {
    fn default() -> Self {
        ModelSetup {
            has_flat_track_list: true,
            has_full_flat_track_list: false,
            num_tracks: 8,
            num_fx_tracks: -1,
            num_scenes: 8,
            num_sends: 8,
            wants_clip_launcher_navigator: false,
            wants_drum_device: true,
            additional_drum_devices: Vec::new(),
            num_devices_in_bank: 8,
            num_device_layers: 8,
            num_param_pages: 8,
            num_params: 8,
            num_drum_pad_layers: 16,
            num_list_params: 0,
            wants_cursor_layer: false,
            wants_focused_parameter: false,
            num_filter_column_entries: 16,
            num_results: 16,
            num_markers: 0,
            device_ids: Vec::new(),
        }
    }
}

impl ModelSetup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enable_main_drum_device(&mut self, wants_drum_device: bool) {
        self.wants_drum_device = wants_drum_device;
    }

    /// Adds a drum device with 64 pads (in addition to the main one).
    pub fn enable_drum64_device(&mut self) {
        self.additional_drum_devices = vec![64];
    }

    pub fn set_additional_drum_devices(&mut self, page_sizes: Vec<usize>) {
        self.additional_drum_devices = page_sizes;
    }

    pub fn enable_device(&mut self, device_id: DeviceId) {
        self.device_ids.push(device_id);
    }

    pub fn set_num_tracks(&mut self, n: usize) {
        self.num_tracks = n;
    }

    pub fn set_num_scenes(&mut self, n: usize) {
        self.num_scenes = n;
    }

    pub fn set_num_sends(&mut self, n: usize) {
        self.num_sends = n;
    }

    pub fn set_num_params(&mut self, n: i32) {
        self.num_params = n;
    }

    pub fn set_num_devices_in_bank(&mut self, n: i32) {
        self.num_devices_in_bank = n;
    }

    pub fn set_num_drum_pad_layers(&mut self, n: i32) {
        self.num_drum_pad_layers = n;
    }

    pub fn set_num_markers(&mut self, n: usize) {
        self.num_markers = n;
    }

    pub fn set_num_results(&mut self, n: usize) {
        self.num_results = n;
    }

    pub fn set_has_flat_track_list(&mut self, v: bool) {
        self.has_flat_track_list = v;
    }

    pub fn set_has_full_flat_track_list(&mut self, v: bool) {
        self.has_full_flat_track_list = v;
    }
}
