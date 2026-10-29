// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Devices: the cursor device, the first instrument, the drum device and the equalizer (former
//! `IDevice`, `ISpecificDevice`, `ICursorDevice`, `IDrumDevice`, `IEqualizerDevice` and their
//! Reaper implementations), the device bank (siblings of the cursor device) and the drum pads.
//!
//! All devices are represented by [`SpecificDevice`]; the [`DeviceKind`] tells which one it is.
//! Reaper has no device layers, therefore layer banks are not modeled.

use std::ops::{Deref, DerefMut};

use crate::backend::Processor;

use super::bank::{BankItem, PagedBank};
use super::channel::{Channel, ChannelKind};
use super::color::{ColorEx, DawColor};
use super::context::DawContext;
use super::events::ParameterBankId;
use super::parameter_bank::ParameterBank;
use super::util;

// ---------------------------------------------------------------------------------------------
// Device (sibling)

/// A device in the device chain of the selected track (former `IDevice` / `DeviceImpl`).
pub struct Device {
    ctx: DawContext,
    index: i32,
    position: i32,
    exists: bool,
    selected: bool,
    name: String,
    is_enabled: bool,
}

impl Device {
    pub fn new(ctx: DawContext, index: i32) -> Self {
        Device { ctx, index, position: 0, exists: false, selected: false, name: String::new(), is_enabled: false }
    }

    pub fn name(&self) -> &str {
        if self.exists {
            &self.name
        } else {
            ""
        }
    }

    pub fn name_limited(&self, limit: usize) -> String {
        util::optimize_name(self.name(), limit)
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Toggle the bypass state of the device.
    pub fn toggle_enabled_state(&mut self) {
        self.ctx.send_bool(Processor::Device, &format!("{}/bypass", self.index + 1), self.is_enabled);
    }

    pub fn remove(&mut self) {
        self.ctx.send(Processor::Device, &format!("{}/remove", self.index + 1));
    }

    pub fn duplicate(&mut self) {
        self.ctx.send(Processor::Device, &format!("{}/duplicate", self.index + 1));
    }

    // Internal setters

    /// Set the name (removes the plugin type and manufacturer).
    pub fn set_name(&mut self, name: &str) {
        self.name = util::remove_type_and_manufacturer(name);
    }

    pub fn set_exists(&mut self, exists: bool) {
        self.exists = exists;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
    }

    pub fn set_position(&mut self, position: i32) {
        if position >= 0 {
            self.position = position;
        }
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl BankItem for Device {
    fn does_exist(&self) -> bool {
        self.exists
    }

    fn is_selected(&self) -> bool {
        self.exists && self.selected
    }

    fn index(&self) -> i32 {
        self.index
    }

    fn position(&self) -> i32 {
        self.position
    }

    fn select(&mut self) {
        self.ctx.send_int(Processor::Device, "selected", self.index + 1);
    }
}

/// The page of devices around the cursor device (former `IDeviceBank` / `DeviceBankImpl`). The
/// page is controlled by Reaper, therefore it is not a paged bank.
pub struct DeviceBank {
    ctx: DawContext,
    items: Vec<Device>,
    item_count: usize,
}

impl DeviceBank {
    pub fn new(ctx: DawContext, num_devices: usize) -> Self {
        let items = (0..num_devices).map(|i| Device::new(ctx.clone(), i as i32)).collect();
        DeviceBank { ctx, items, item_count: 0 }
    }

    pub fn page_size(&self) -> usize {
        self.items.len()
    }

    pub fn item_count(&self) -> usize {
        self.item_count
    }

    pub fn set_item_count(&mut self, count: usize) {
        self.item_count = count;
    }

    /// The device at the given index of the page. Panics if the index is not smaller than the
    /// page size.
    pub fn item(&self, index: usize) -> &Device {
        &self.items[index]
    }

    pub fn item_mut(&mut self, index: usize) -> &mut Device {
        &mut self.items[index]
    }

    pub fn has_existing_items(&self) -> bool {
        self.items.iter().any(|d| d.does_exist())
    }

    pub fn selected_item(&self) -> Option<&Device> {
        self.items.iter().find(|d| d.is_selected())
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.items.iter().position(|d| d.is_selected())
    }

    pub fn scroll_position(&self) -> i32 {
        self.items.first().map(|d| d.position()).unwrap_or(0)
    }

    pub fn can_scroll_page_backwards(&self) -> bool {
        self.items.first().map(|d| d.position() > 0).unwrap_or(false)
    }

    pub fn can_scroll_page_forwards(&self) -> bool {
        self.selected_item().map(|d| (d.position() as i64) < self.item_count as i64 - 1).unwrap_or(false)
    }

    pub fn can_scroll_backwards(&self) -> bool {
        self.selected_item().map(|d| d.index()).unwrap_or(-1) > 0 || self.can_scroll_page_backwards()
    }

    pub fn can_scroll_forwards(&self) -> bool {
        let sel = self.selected_item().map(|d| d.index()).unwrap_or(-1);
        (sel >= 0 && (sel as usize) + 1 < self.items.len() && self.items[sel as usize + 1].does_exist()) || self.can_scroll_page_forwards()
    }

    pub fn select_previous_page(&mut self) {
        self.ctx.send(Processor::Device, "page/-");
    }

    pub fn select_next_page(&mut self) {
        self.ctx.send(Processor::Device, "page/+");
    }
}

// ---------------------------------------------------------------------------------------------
// Drum pads

/// A drum pad (former `IDrumPad` / `DrumPadImpl`). Reaper has no real drum pads; they are
/// virtual channels for the 128 notes. Hosts with drum machines (Bitwig) report the pads
/// (protocol v2). Derefs to [`Channel`].
pub struct DrumPad {
    channel: Channel,
}

impl DrumPad {
    /// Not supported.
    pub fn has_devices(&self) -> bool {
        false
    }
}

impl Deref for DrumPad {
    type Target = Channel;
    fn deref(&self) -> &Channel {
        &self.channel
    }
}

impl DerefMut for DrumPad {
    fn deref_mut(&mut self) -> &mut Channel {
        &mut self.channel
    }
}

impl BankItem for DrumPad {
    fn does_exist(&self) -> bool {
        self.channel.exists
    }

    fn is_selected(&self) -> bool {
        self.channel.is_selected()
    }

    fn index(&self) -> i32 {
        self.channel.index
    }

    fn position(&self) -> i32 {
        self.channel.position
    }
}

/// The 128 drum pads of a drum device (former `IDrumPadBank` / `DrumPadBankImpl`). Derefs to
/// [`PagedBank<DrumPad>`]. Use [`DrumPadBank::select_pad`] to select a pad.
pub struct DrumPadBank {
    bank: PagedBank<DrumPad>,
    /// The processor of the device which contains the pads.
    owner: Processor,
    /// True if the host reports the pads (protocol v2): their colors are not overridden with the
    /// track color and the mute/solo summaries come from the host.
    host_reported: bool,
    has_muted_pads: bool,
    has_soloed_pads: bool,
}

impl DrumPadBank {
    /// Create the pad bank of a device; `owner` is the processor of the device (drum, primary).
    pub fn new(ctx: DawContext, num_layers: usize, num_sends: usize, owner: Processor) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let mut bank = PagedBank::new(
            ctx,
            num_layers,
            move |position| {
                let index = if num_layers > 0 { position % num_layers } else { position };
                let mut channel = Channel::new(item_ctx.clone(), ChannelKind::DrumPad, index as i32, position as i32, num_sends).with_owner(owner);
                channel.set_exists(true);
                channel.set_internal_name(&format!("Drum {position}"));
                DrumPad { channel }
            },
            move || DrumPad { channel: Channel::empty(&empty_ctx, num_sends) },
        );
        if num_layers > 0 {
            bank.set_item_count(128);
        }
        let mut pad_bank = DrumPadBank { bank, owner, host_reported: false, has_muted_pads: false, has_soloed_pads: false };
        if num_layers > 0 {
            pad_bank.set_selected_drum_pad(36);
            pad_bank.scroll_to(36, true);
        }
        pad_bank
    }

    /// Get the drum pad at the index of the page for modification.
    pub fn item_mut(&mut self, index: usize) -> &mut DrumPad {
        self.bank.item_mut(index)
    }

    /// Scroll to the given pad position. As the Java versions: Bitwig (pads reported by the
    /// host) scrolls exactly to the position, Reaper to the page which contains it.
    pub fn scroll_to(&mut self, position: usize, _adjust_page: bool) {
        if self.host_reported {
            self.bank.set_bank_offset(position as i64);
        } else {
            let ps = self.bank.page_size().max(1);
            self.bank.set_bank_offset((position / ps * ps) as i64);
        }
    }

    pub fn scroll_page_backwards(&mut self) {
        self.bank.scroll_page_backwards();
    }

    pub fn scroll_page_forwards(&mut self) {
        self.bank.scroll_page_forwards();
    }

    pub fn scroll_backwards(&mut self) {
        self.bank.scroll_backwards();
    }

    pub fn scroll_forwards(&mut self) {
        self.bank.scroll_forwards();
    }

    /// Select the drum pad at the index of the current page.
    pub fn select_pad(&mut self, index: usize) {
        let position = self.bank.item(index).position();
        if position >= 0 {
            self.set_selected_drum_pad(position as usize);
        }
    }

    /// Select the drum pad at the given absolute position (note). The host selects the pad as
    /// well (protocol v2, Reaper ignores it).
    pub fn set_selected_drum_pad(&mut self, position: usize) {
        let count = self.bank.item_count();
        for i in 0..count {
            self.bank.unpaged_item_mut(i).set_selected(i == position);
        }
        if self.host_reported {
            self.bank.ctx.send(self.owner, &format!("pad/{position}/select"));
        }
    }

    /// Get the drum pad at the absolute position (note) for modification.
    pub fn unpaged_item_mut(&mut self, position: usize) -> &mut DrumPad {
        self.bank.unpaged_item_mut(position)
    }

    /// The host reports the pads (protocol v2): from now on their colors are not overridden with
    /// the track color.
    pub(crate) fn set_host_reported(&mut self) {
        if !self.host_reported {
            self.host_reported = true;
            self.set_pad_color(None);
        }
    }

    pub(crate) fn set_has_muted_pads(&mut self, value: bool) {
        self.has_muted_pads = value;
    }

    pub(crate) fn set_has_soloed_pads(&mut self, value: bool) {
        self.has_soloed_pads = value;
    }

    /// The color ID of the selected pad.
    pub fn selected_channel_color_entry(&self) -> &'static str {
        match self.bank.selected_item() {
            Some(pad) => DawColor::color_id(pad.color()),
            None => DawColor::ColorOff.id(),
        }
    }

    /// Set the color of all pads (the color of the selected track). Not if the host reports the
    /// colors of the pads.
    pub(crate) fn set_pad_color(&mut self, color: Option<ColorEx>) {
        let color = if self.host_reported { None } else { color };
        for pad in self.bank.all_items_mut() {
            pad.set_color_override(color);
        }
    }

    /// Unmute all pads (not supported by Reaper).
    pub fn clear_mute(&mut self) {
        self.bank.ctx.send(self.owner, "clearMute");
    }

    /// Unsolo all pads (not supported by Reaper).
    pub fn clear_solo(&mut self) {
        self.bank.ctx.send(self.owner, "clearSolo");
    }

    pub fn has_muted_pads(&self) -> bool {
        self.has_muted_pads
    }

    pub fn has_soloed_pads(&self) -> bool {
        self.has_soloed_pads
    }

    pub fn can_edit_send(&self, _send_index: usize) -> bool {
        false
    }

    pub fn edit_send_name(&self, _send_index: usize) -> String {
        String::new()
    }

    /// Indicate the pads on the DAW user interface (not supported by Reaper).
    pub fn set_indication(&mut self, indicate: bool) {
        self.bank.ctx.send_int(self.owner, "indicate", if indicate { 1 } else { 0 });
    }
}

impl Deref for DrumPadBank {
    type Target = PagedBank<DrumPad>;
    fn deref(&self) -> &PagedBank<DrumPad> {
        &self.bank
    }
}

// ---------------------------------------------------------------------------------------------
// Specific devices

// ---------------------------------------------------------------------------------------------
// Layers

/// The layers of a device (former `ILayerBank` / `LayerBankImpl`), e.g. of Bitwig's Instrument
/// Layer. Reaper has no layers; hosts report them (protocol v2). Derefs to
/// [`PagedBank<Channel>`].
pub struct LayerBank {
    bank: PagedBank<Channel>,
}

impl LayerBank {
    /// Create the layer bank of a device; `owner` is the processor of the device.
    pub fn new(ctx: DawContext, num_layers: usize, num_sends: usize, owner: Processor) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let bank = PagedBank::new(
            ctx,
            num_layers,
            move |position| {
                let index = if num_layers > 0 { position % num_layers } else { position };
                Channel::new(item_ctx.clone(), ChannelKind::Layer, index as i32, position as i32, num_sends).with_owner(owner)
            },
            move || Channel::empty(&empty_ctx, num_sends),
        );
        LayerBank { bank }
    }

    /// Get the layer at the index of the page for modification.
    pub fn item_mut(&mut self, index: usize) -> &mut Channel {
        self.bank.item_mut(index)
    }

    /// Get the layer at the absolute position for modification.
    pub fn unpaged_item_mut(&mut self, position: usize) -> &mut Channel {
        self.bank.unpaged_item_mut(position)
    }

    pub fn set_item_count(&mut self, count: usize) {
        self.bank.set_item_count(count);
    }

    pub fn scroll_page_backwards(&mut self) {
        self.bank.scroll_page_backwards();
    }

    pub fn scroll_page_forwards(&mut self) {
        self.bank.scroll_page_forwards();
    }
}

impl Deref for LayerBank {
    type Target = PagedBank<Channel>;
    fn deref(&self) -> &PagedBank<Channel> {
        &self.bank
    }
}

impl DerefMut for LayerBank {
    fn deref_mut(&mut self) -> &mut PagedBank<Channel> {
        &mut self.bank
    }
}

/// Which device a [`SpecificDevice`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    /// The selected device (former `ICursorDevice`).
    Cursor,
    /// The first instrument of the selected track.
    FirstInstrument,
    /// The drum device (Reaper has none, its drum pads are virtual).
    Drum,
    /// The ReaEQ equalizer of the selected track.
    Eq,
}

/// The types of an equalizer band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EqualizerBandType {
    #[default]
    Off,
    Lowcut,
    Lowshelf,
    Bell,
    Highcut,
    Highshelf,
    Notch,
}

impl EqualizerBandType {
    pub const ALL: &'static [EqualizerBandType] = &[
        EqualizerBandType::Off,
        EqualizerBandType::Lowcut,
        EqualizerBandType::Lowshelf,
        EqualizerBandType::Bell,
        EqualizerBandType::Highcut,
        EqualizerBandType::Highshelf,
        EqualizerBandType::Notch,
    ];

    /// The ReaEQ type index.
    fn reaper_index(self) -> i32 {
        match self {
            EqualizerBandType::Off => -1,
            EqualizerBandType::Lowcut => 4,
            EqualizerBandType::Lowshelf => 0,
            EqualizerBandType::Bell => 8,
            EqualizerBandType::Highcut => 3,
            EqualizerBandType::Highshelf => 1,
            EqualizerBandType::Notch => 6,
        }
    }

    fn from_reaper_index(index: i32) -> EqualizerBandType {
        match index {
            4 => EqualizerBandType::Lowcut,
            0 => EqualizerBandType::Lowshelf,
            8 | 9 | 2 | 5 | 7 => EqualizerBandType::Bell,
            3 => EqualizerBandType::Highcut,
            1 => EqualizerBandType::Highshelf,
            6 => EqualizerBandType::Notch,
            _ => EqualizerBandType::Off,
        }
    }
}

/// The number of bands of the equalizer.
pub const EQ_NUMBER_OF_BANDS: usize = 8;

/// A device (cursor device, first instrument, drum device or equalizer).
pub struct SpecificDevice {
    ctx: DawContext,
    kind: DeviceKind,
    position: i32,
    exists: bool,
    name: String,
    is_enabled: bool,
    is_window_open: bool,
    is_expanded: bool,
    parameter_bank: Option<ParameterBank>,
    drum_pad_bank: DrumPadBank,
    layer_bank: LayerBank,
    /// Reported by the host (protocol v2); Reaper always has (virtual) drum pads.
    has_drum_pads: bool,
    has_layers: bool,
    device_bank: DeviceBank,
    device_count: usize,
    last_touched_parameter_index: i32,
    eq_band_types: [EqualizerBandType; EQ_NUMBER_OF_BANDS],
}

impl SpecificDevice {
    /// Create a device. Negative numbers use the defaults (8 parameters, 8 devices, 16 pads).
    pub fn new(ctx: DawContext, kind: DeviceKind, num_sends: usize, num_params: i32, num_devices_in_bank: i32, num_drum_pad_layers: i32) -> Self {
        let num_params = if num_params >= 0 { num_params as usize } else { 8 };
        let num_pads = if num_drum_pad_layers >= 0 { num_drum_pad_layers as usize } else { 16 };
        let num_devices = if num_devices_in_bank >= 0 { num_devices_in_bank as usize } else { 8 };
        // The processor which gets the commands of the pads and layers of the device
        let channel_owner = match kind {
            DeviceKind::FirstInstrument => Processor::Primary,
            DeviceKind::Cursor => Processor::Device,
            DeviceKind::Eq => Processor::Eq,
            DeviceKind::Drum => Processor::Drum,
        };
        let (processor, bank_id) = match kind {
            DeviceKind::Cursor => (Processor::Device, ParameterBankId::CursorDevice),
            // The parameters of the first instrument go to its own processor (protocol v2)
            DeviceKind::FirstInstrument => (Processor::Primary, ParameterBankId::FirstInstrument),
            DeviceKind::Drum => (Processor::Device, ParameterBankId::DrumDevice),
            DeviceKind::Eq => (Processor::Eq, ParameterBankId::Eq),
        };
        SpecificDevice {
            parameter_bank: if num_params > 0 { Some(ParameterBank::new(ctx.clone(), bank_id, processor, num_params, true)) } else { None },
            drum_pad_bank: DrumPadBank::new(ctx.clone(), num_pads, num_sends, channel_owner),
            layer_bank: LayerBank::new(ctx.clone(), num_pads, num_sends, channel_owner),
            has_drum_pads: true,
            has_layers: false,
            device_bank: DeviceBank::new(ctx.clone(), if kind == DeviceKind::Cursor { num_devices } else { 0 }),
            ctx,
            kind,
            position: 0,
            exists: false,
            name: String::new(),
            is_enabled: false,
            is_window_open: false,
            is_expanded: false,
            device_count: 0,
            last_touched_parameter_index: -1,
            eq_band_types: [EqualizerBandType::Off; EQ_NUMBER_OF_BANDS],
        }
    }

    pub fn kind(&self) -> DeviceKind {
        self.kind
    }

    fn processor(&self) -> Processor {
        if self.kind == DeviceKind::Eq {
            Processor::Eq
        } else {
            Processor::Device
        }
    }

    pub fn enable_observers(&self, enable: bool) {
        self.ctx.enable_updates(Processor::Device, enable);
    }

    pub fn does_exist(&self) -> bool {
        self.exists
    }

    pub fn position(&self) -> i32 {
        self.position
    }

    /// The index of the device in the page of the device bank (-1 if there is no device bank).
    pub fn index(&self) -> i32 {
        let ps = self.device_bank.page_size();
        if ps == 0 {
            -1
        } else {
            self.position % ps as i32
        }
    }

    pub fn name(&self) -> &str {
        if self.exists {
            &self.name
        } else {
            ""
        }
    }

    /// The name limited to the given length. Removes a "type: " prefix if it is too long.
    pub fn name_limited(&self, limit: usize) -> String {
        let mut n = self.name().to_string();
        if n.chars().count() < limit {
            return n;
        }
        let split: Vec<&str> = n.split(": ").collect();
        if split.len() == 2 {
            n = split[1].to_string();
        }
        if n.chars().count() < limit {
            return n;
        }
        util::limit(&n, limit)
    }

    /// The ID of the device (the name of the first parameter).
    pub fn id(&self) -> String {
        self.unpaged_parameter_name(0)
    }

    /// The name of the parameter at the given absolute position.
    pub fn unpaged_parameter_name(&self, position: usize) -> String {
        match &self.parameter_bank {
            Some(bank) if bank.item_count() > position => bank.unpaged_item(position).name().to_string(),
            _ => String::new(),
        }
    }

    pub fn preset_name(&self) -> &str {
        "none"
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    pub fn is_plugin(&self) -> bool {
        true
    }

    pub fn is_expanded(&self) -> bool {
        self.is_expanded
    }

    pub fn is_parameter_page_section_visible(&self) -> bool {
        true
    }

    pub fn is_window_open(&self) -> bool {
        self.is_window_open
    }

    pub fn is_nested(&self) -> bool {
        false
    }

    /// Reaper always has (virtual) drum pads.
    /// True if the device has drum pads: always for Reaper (virtual pads), reported by other
    /// hosts.
    pub fn has_drum_pads(&self) -> bool {
        self.has_drum_pads
    }

    /// True if the device has layers (reported by the host, Reaper has none).
    pub fn has_layers(&self) -> bool {
        self.has_layers
    }

    pub(crate) fn set_has_drum_pads(&mut self, value: bool) {
        self.has_drum_pads = value;
    }

    pub(crate) fn set_has_layers(&mut self, value: bool) {
        self.has_layers = value;
    }

    pub fn layer_bank(&self) -> &LayerBank {
        &self.layer_bank
    }

    pub fn layer_bank_mut(&mut self) -> &mut LayerBank {
        &mut self.layer_bank
    }

    pub fn has_slots(&self) -> bool {
        false
    }

    pub fn slot_chains(&self) -> Vec<String> {
        Vec::new()
    }

    pub fn select_slot_chain(&mut self, _name: &str) {}

    pub fn toggle_window_open(&mut self) {
        self.ctx.send_bool(Processor::Device, "window", !self.is_window_open);
    }

    pub fn toggle_expanded(&mut self) {
        self.is_expanded = !self.is_expanded;
        self.ctx.send_bool(Processor::Device, "expand", self.is_expanded);
    }

    pub fn toggle_parameter_page_section_visible(&mut self) {}

    /// Toggle the bypass state.
    pub fn toggle_enabled_state(&mut self) {
        self.ctx.send_bool(self.processor(), &format!("{}/bypass", self.index() + 1), self.is_enabled);
    }

    /// Select this device in the DAW.
    pub fn select(&mut self) {
        self.ctx.send_int(self.processor(), "selected", self.index() + 1);
    }

    pub fn remove(&mut self) {
        self.ctx.send(self.processor(), &format!("{}/remove", self.index() + 1));
    }

    pub fn duplicate(&mut self) {
        self.ctx.send(self.processor(), &format!("{}/duplicate", self.index() + 1));
    }

    pub fn parameter_bank(&self) -> Option<&ParameterBank> {
        self.parameter_bank.as_ref()
    }

    pub fn parameter_bank_mut(&mut self) -> Option<&mut ParameterBank> {
        self.parameter_bank.as_mut()
    }

    pub fn drum_pad_bank(&self) -> &DrumPadBank {
        &self.drum_pad_bank
    }

    pub fn drum_pad_bank_mut(&mut self) -> &mut DrumPadBank {
        &mut self.drum_pad_bank
    }

    pub fn last_touched_parameter_index(&self) -> i32 {
        self.last_touched_parameter_index
    }

    // ----------------------------------------------------------------------------------------
    // Cursor device functions

    /// The devices around the cursor device (empty page for other devices).
    pub fn device_bank(&self) -> &DeviceBank {
        &self.device_bank
    }

    pub fn device_bank_mut(&mut self) -> &mut DeviceBank {
        &mut self.device_bank
    }

    pub fn can_select_previous(&self) -> bool {
        self.position > 0
    }

    pub fn can_select_next(&self) -> bool {
        (self.position as i64) < self.device_count as i64 - 1
    }

    /// The cursor device is always pinned in Reaper.
    pub fn is_pinned(&self) -> bool {
        true
    }

    pub fn toggle_pinned(&mut self) {}

    pub fn set_pinned(&mut self, _pinned: bool) {}

    /// Select the previous device of the track.
    pub fn select_previous(&mut self) {
        if !self.can_select_previous() {
            return;
        }
        let index = self.index();
        if index > 0 && (index as usize) <= self.device_bank.page_size() {
            let name = self.device_bank.item(index as usize - 1).name().to_string();
            self.set_name(&name);
        }
        self.ctx.send(Processor::Device, "-");
    }

    /// Select the next device of the track.
    pub fn select_next(&mut self) {
        if !self.can_select_next() {
            return;
        }
        let index = self.index();
        if index >= 0 && (index as usize) + 1 < self.device_bank.page_size() {
            let name = self.device_bank.item(index as usize + 1).name().to_string();
            self.set_name(&name);
        }
        self.ctx.send(Processor::Device, "+");
    }

    pub fn select_parent(&mut self) {}

    pub fn select_channel(&mut self) {}

    /// Move the device one position to the left.
    pub fn swap_with_previous(&mut self) {
        self.ctx.send(Processor::Device, &format!("{}/movePrev", self.index() + 1));
        let ps = self.device_bank.page_size();
        if ps > 0 {
            let i = (self.index() - 1).max(0) as usize;
            self.device_bank.item_mut(i.min(ps - 1)).select();
        }
    }

    /// Move the device one position to the right.
    pub fn swap_with_next(&mut self) {
        self.ctx.send(Processor::Device, &format!("{}/moveNext", self.index() + 1));
        let ps = self.device_bank.page_size();
        if ps > 0 {
            let i = ((self.index() + 1).max(0) as usize).min(ps - 1);
            self.device_bank.item_mut(i).select();
        }
    }

    /// Not needed in Reaper (no parameter maps).
    pub fn refresh_parameter_mapping(&mut self) {}

    // ----------------------------------------------------------------------------------------
    // Equalizer functions

    pub fn band_count(&self) -> usize {
        EQ_NUMBER_OF_BANDS
    }

    /// The type of the band with the given index.
    pub fn type_id(&self, index: usize) -> EqualizerBandType {
        if self.exists {
            self.eq_band_types.get(index).copied().unwrap_or_default()
        } else {
            EqualizerBandType::Off
        }
    }

    /// Set the type of the band with the given index.
    pub fn set_type(&mut self, index: usize, band_type: EqualizerBandType) {
        self.ctx.send_str(Processor::Eq, &format!("band/{index}"), &band_type.reaper_index().to_string());
    }

    /// Change the type of the band with the given index by the increment.
    pub fn change_type(&mut self, index: usize, increment: i32) {
        let current = self.type_id(index) as i32;
        let id = (current + increment).clamp(0, 6) as usize;
        self.set_type(index, EqualizerBandType::ALL[id]);
    }

    /// The position of the frequency parameter of the band in the parameter bank.
    pub fn frequency_parameter_index(index: usize) -> usize {
        3 * index
    }

    /// The position of the gain parameter of the band in the parameter bank.
    pub fn gain_parameter_index(index: usize) -> usize {
        3 * index + 1
    }

    /// The position of the Q parameter of the band in the parameter bank.
    pub fn q_parameter_index(index: usize) -> usize {
        3 * index + 2
    }

    // ----------------------------------------------------------------------------------------
    // Internal setters

    pub fn set_exists(&mut self, exists: bool) {
        self.exists = exists;
    }

    pub fn set_position(&mut self, position: i32) {
        if position >= 0 {
            self.position = position;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
    }

    /// Set the name (removes plugin type and manufacturer). Restores the last selected parameter
    /// page of the device.
    pub fn set_name(&mut self, name: &str) {
        let name = util::remove_type_and_manufacturer(name);
        if self.name == name {
            return;
        }
        self.name = name;
        if let Some(bank) = &mut self.parameter_bank {
            bank.device_name_changed(&self.name);
        }
    }

    pub fn set_window_open(&mut self, open: bool) {
        self.is_window_open = open;
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.is_expanded = expanded;
    }

    pub fn set_parameter_count(&mut self, count: usize) {
        if let Some(bank) = &mut self.parameter_bank {
            bank.set_item_count(count);
        }
    }

    pub fn set_last_touched_parameter_index(&mut self, index: i32) {
        self.last_touched_parameter_index = index;
    }

    pub fn set_device_count(&mut self, count: usize) {
        self.device_count = count;
        self.device_bank.set_item_count(count);
    }

    pub fn set_type_internal(&mut self, index: usize, type_index: i32) {
        if let Some(t) = self.eq_band_types.get_mut(index) {
            *t = EqualizerBandType::from_reaper_index(type_index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendCall;
    use crate::daw::bank::tests::test_ctx;

    #[test]
    fn cursor_device() {
        let (ctx, backend) = test_ctx();
        let mut device = SpecificDevice::new(ctx, DeviceKind::Cursor, 0, 8, 8, 16);
        device.set_exists(true);
        device.set_device_count(3);
        device.set_position(1);
        device.device_bank_mut().item_mut(2).set_name("VST: ReaComp (Cockos)");
        device.device_bank_mut().item_mut(2).set_exists(true);
        assert!(device.can_select_next());
        device.select_next();
        assert_eq!(device.name(), "ReaComp");
        device.toggle_enabled_state();
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::NoArg("device".into(), "+".into()), BackendCall::IntArg("device".into(), "2/bypass".into(), 0)]
        );
        let pads = device.drum_pad_bank();
        assert_eq!(pads.scroll_position(), 32);
        assert!(pads.item(4).is_selected());
        assert_eq!(pads.item(0).name(), "Drum 32");
    }
}
