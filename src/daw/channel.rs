// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A mixer channel (former `IChannel` / `ChannelImpl`), which is the base of tracks, the master
//! track and drum pads, and the bank of its sends (former `ISendBank` / `SendBankImpl`).

use std::cell::Cell;
use std::ops::{Deref, DerefMut};

use crate::backend::Processor;

use super::bank::{BankItem, PagedBank};
use super::color::ColorEx;
use super::constants::ChannelType;
use super::context::DawContext;
use super::events::ModelEvent;
use super::parameter::{Parameter, ParameterKind, Send};
use super::util;

const GRAY: ColorEx = ColorEx::new(0.2, 0.2, 0.2);

/// The kind of a channel which defines how commands are addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    /// A track (`track` processor, commands are prefixed with the position).
    Track,
    /// The master track (`master` processor, no prefix).
    Master,
    /// A (virtual) drum pad.
    DrumPad,
    /// A placeholder for a non-existing channel.
    Empty,
}

/// The sends of a channel. Derefs (mutably) to [`PagedBank<Send>`].
pub struct SendBank {
    bank: PagedBank<Send>,
}

impl SendBank {
    pub fn new(ctx: DawContext, num_sends: usize, channel_position: i32) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let mut bank = PagedBank::new(
            ctx,
            num_sends,
            move |position| {
                let index = if num_sends == 0 { 0 } else { position % num_sends };
                Send::new(item_ctx.clone(), channel_position, index as i32, position as i32)
            },
            move || Send::empty(&empty_ctx),
        );
        bank.set_page_event(ModelEvent::SendBankPageChanged { track_position: channel_position });
        SendBank { bank }
    }

    pub(crate) fn set_channel_position(&mut self, channel_position: i32) {
        for send in self.bank.all_items_mut() {
            send.set_channel_position(channel_position);
        }
    }
}

impl Deref for SendBank {
    type Target = PagedBank<Send>;
    fn deref(&self) -> &PagedBank<Send> {
        &self.bank
    }
}

impl DerefMut for SendBank {
    fn deref_mut(&mut self) -> &mut PagedBank<Send> {
        &mut self.bank
    }
}

/// A mixer channel.
pub struct Channel {
    pub(crate) ctx: DawContext,
    pub(crate) kind: ChannelKind,
    pub(crate) index: i32,
    pub(crate) position: i32,
    pub(crate) exists: bool,
    pub(crate) selected: bool,
    name: String,
    channel_type: Option<ChannelType>,
    vu: f64,
    vu_left: f64,
    vu_right: f64,
    vu_hold_db_left: f64,
    vu_hold_db_right: f64,
    vu_peak_left: Cell<i32>,
    vu_peak_right: Cell<i32>,
    vu_peak_last_volume: Cell<i32>,
    is_mute: bool,
    is_solo: bool,
    is_activated: bool,
    color: Option<ColorEx>,
    color_override: Option<ColorEx>,
    volume: Parameter,
    pan: Parameter,
    send_bank: SendBank,
}

impl Channel {
    /// Create a channel. `index` is the index in the page, `position` the absolute position.
    pub fn new(ctx: DawContext, kind: ChannelKind, index: i32, position: i32, num_sends: usize) -> Self {
        let (vol_kind, pan_kind) = if kind == ChannelKind::Master {
            (ParameterKind::MasterVolume, ParameterKind::MasterPan)
        } else {
            (ParameterKind::TrackVolume, ParameterKind::TrackPan)
        };
        // Volume and panorama use the position as index (it is the index at creation in Java)
        let param_index = if kind == ChannelKind::Master { 0 } else { position };
        let volume = Parameter::new(ctx.clone(), vol_kind, param_index, 0.716);
        let pan = Parameter::new(ctx.clone(), pan_kind, param_index, 0.5);
        Channel {
            send_bank: SendBank::new(ctx.clone(), num_sends, position),
            ctx,
            kind,
            index,
            position,
            exists: false,
            selected: false,
            name: "Track".into(),
            channel_type: None,
            vu: 0.0,
            vu_left: 0.0,
            vu_right: 0.0,
            vu_hold_db_left: 0.0,
            vu_hold_db_right: 0.0,
            vu_peak_left: Cell::new(0),
            vu_peak_right: Cell::new(0),
            vu_peak_last_volume: Cell::new(0),
            is_mute: false,
            is_solo: false,
            is_activated: true,
            color: None,
            color_override: None,
            volume,
            pan,
        }
    }

    /// Create a placeholder for a non-existing channel.
    pub fn empty(ctx: &DawContext, num_sends: usize) -> Self {
        let mut channel = Channel::new(ctx.muted(), ChannelKind::Empty, -1, -1, num_sends);
        channel.name = String::new();
        channel.volume = Parameter::empty(ctx);
        channel.pan = Parameter::empty(ctx);
        channel.color = Some(ColorEx::BLACK);
        channel
    }

    pub fn kind(&self) -> ChannelKind {
        self.kind
    }

    pub fn does_exist(&self) -> bool {
        self.exists
    }

    /// The index of the channel in the page of its bank (-1 if empty).
    pub fn index(&self) -> i32 {
        self.index
    }

    /// The absolute position of the channel (-1 if empty).
    pub fn position(&self) -> i32 {
        self.position
    }

    /// The raw selection state (without considering pinning).
    pub fn is_selected(&self) -> bool {
        self.exists && self.selected
    }

    pub fn name(&self) -> &str {
        if self.exists {
            &self.name
        } else {
            ""
        }
    }

    /// The name shortened to the given length.
    pub fn name_limited(&self, limit: usize) -> String {
        util::optimize_name(self.name(), limit)
    }

    pub fn channel_type(&self) -> ChannelType {
        if self.kind == ChannelKind::Empty {
            return ChannelType::Unknown;
        }
        self.channel_type.unwrap_or_default()
    }

    pub fn is_activated(&self) -> bool {
        self.exists && self.is_activated
    }

    // ----------------------------------------------------------------------------------------
    // Volume & panorama

    pub fn volume_parameter(&self) -> &Parameter {
        &self.volume
    }

    pub fn volume_parameter_mut(&mut self) -> &mut Parameter {
        &mut self.volume
    }

    pub fn volume_str(&self) -> String {
        if self.exists {
            self.volume.displayed_value()
        } else {
            String::new()
        }
    }

    pub fn volume_str_limited(&self, limit: usize) -> String {
        util::limit(&self.volume_str(), limit)
    }

    pub fn volume(&self) -> i32 {
        self.volume.value()
    }

    pub fn modulated_volume(&self) -> i32 {
        self.volume()
    }

    pub fn change_volume(&mut self, control: i32) {
        self.volume.change_value(control);
    }

    pub fn set_volume(&mut self, value: i32) {
        self.volume.set_value(value);
    }

    pub fn reset_volume(&mut self) {
        let value = (0.716 * self.ctx.value_changer().upper_bound() as f64) as i32;
        self.set_volume(value);
    }

    pub fn touch_volume(&mut self, is_being_touched: bool) {
        self.volume.touch_value(is_being_touched);
    }

    pub fn set_volume_indication(&mut self, _indicate: bool) {}

    pub fn pan_parameter(&self) -> &Parameter {
        &self.pan
    }

    pub fn pan_parameter_mut(&mut self) -> &mut Parameter {
        &mut self.pan
    }

    pub fn pan_str(&self) -> String {
        if self.exists {
            self.pan.displayed_value()
        } else {
            String::new()
        }
    }

    pub fn pan_str_limited(&self, limit: usize) -> String {
        util::limit(&self.pan_str(), limit)
    }

    pub fn pan(&self) -> i32 {
        self.pan.value()
    }

    pub fn modulated_pan(&self) -> i32 {
        self.pan()
    }

    pub fn change_pan(&mut self, control: i32) {
        self.pan.change_value(control);
    }

    pub fn set_pan(&mut self, value: i32) {
        self.pan.set_value(value);
    }

    pub fn reset_pan(&mut self) {
        let value = self.ctx.value_changer().upper_bound() / 2;
        self.set_pan(value);
    }

    pub fn touch_pan(&mut self, is_being_touched: bool) {
        self.pan.touch_value(is_being_touched);
    }

    pub fn set_pan_indication(&mut self, _indicate: bool) {}

    // ----------------------------------------------------------------------------------------
    // Color

    /// The color of the channel (for drum pads the color of the selected track).
    pub fn color(&self) -> ColorEx {
        if let Some(c) = self.color_override {
            return c;
        }
        self.color.unwrap_or(GRAY)
    }

    /// Set the color of the channel in the DAW.
    pub fn set_color(&mut self, color: ColorEx) {
        let [r, g, b] = color.to_int_rgb255();
        self.send_str("color", &format!("RGB({r},{g},{b})"));
    }

    // ----------------------------------------------------------------------------------------
    // Mute / Solo

    pub fn is_mute(&self) -> bool {
        self.exists && self.is_mute
    }

    pub fn set_mute(&mut self, value: bool) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.delay_updates(Processor::Track);
        }
        self.set_mute_state(value);
        self.send_bool("mute", value);
    }

    pub fn toggle_mute(&mut self) {
        self.set_mute(!self.is_mute());
    }

    pub fn is_solo(&self) -> bool {
        self.exists && self.is_solo
    }

    pub fn set_solo(&mut self, value: bool) {
        self.set_solo_state(value);
        self.send_bool("solo", value);
    }

    pub fn toggle_solo(&mut self) {
        self.set_solo(!self.is_solo());
    }

    // ----------------------------------------------------------------------------------------
    // VU meters

    pub fn vu(&self) -> i32 {
        self.ctx.value_changer().from_normalized_value(self.vu)
    }

    pub fn vu_left(&self) -> i32 {
        self.ctx.value_changer().from_normalized_value(self.vu_left)
    }

    pub fn vu_right(&self) -> i32 {
        self.ctx.value_changer().from_normalized_value(self.vu_right)
    }

    pub fn vu_peak_left(&self) -> i32 {
        self.check_peak_volume();
        let v = self.vu_left();
        if v > self.vu_peak_left.get() {
            self.vu_peak_left.set(v);
        }
        self.vu_peak_left.get()
    }

    pub fn vu_peak_right(&self) -> i32 {
        self.check_peak_volume();
        let v = self.vu_right();
        if v > self.vu_peak_right.get() {
            self.vu_peak_right.set(v);
        }
        self.vu_peak_right.get()
    }

    fn check_peak_volume(&self) {
        let volume = self.volume.value();
        if self.vu_peak_last_volume.get() == volume {
            return;
        }
        self.vu_peak_last_volume.set(volume);
        self.vu_peak_left.set(0);
        self.vu_peak_right.set(0);
    }

    pub fn vu_clip_state(&self) -> bool {
        self.vu_left_clip_state() || self.vu_right_clip_state()
    }

    pub fn vu_left_clip_state(&self) -> bool {
        self.vu_hold_db_left > 0.0
    }

    pub fn vu_right_clip_state(&self) -> bool {
        self.vu_hold_db_right > 0.0
    }

    // ----------------------------------------------------------------------------------------
    // Other commands

    pub fn set_is_activated(&mut self, enable: bool) {
        if self.kind == ChannelKind::Master {
            return;
        }
        self.is_activated = enable;
        self.send_bool("active", enable);
    }

    pub fn toggle_is_activated(&mut self) {
        let activated = self.is_activated;
        self.set_is_activated(!activated);
    }

    /// Remove the channel.
    pub fn remove(&mut self) {
        self.send_cmd("remove");
    }

    /// Duplicate the channel.
    pub fn duplicate(&mut self) {
        self.send_cmd("duplicate");
    }

    /// Add the device with the given creation name to the channel.
    pub fn add_device(&mut self, creation_name: &str) {
        self.ctx.send_str(Processor::Device, "add/0", creation_name);
    }

    pub fn add_equalizer_device(&mut self) {
        self.ctx.send(Processor::Eq, "add");
    }

    pub fn send_bank(&self) -> &SendBank {
        &self.send_bank
    }

    pub fn send_bank_mut(&mut self) -> &mut SendBank {
        &mut self.send_bank
    }

    pub(crate) fn select_channel(&mut self) {
        if !self.exists {
            return;
        }
        match self.kind {
            ChannelKind::Track | ChannelKind::Master => self.send_int("select", 1),
            _ => {}
        }
    }

    // ----------------------------------------------------------------------------------------
    // Sending

    fn processor(&self) -> Processor {
        if self.kind == ChannelKind::Master {
            Processor::Master
        } else {
            Processor::Track
        }
    }

    /// Create the command with the position prefix.
    pub(crate) fn create_command(&self, command: &str) -> String {
        if self.kind == ChannelKind::Master {
            command.to_string()
        } else {
            format!("{}/{}", self.position, command)
        }
    }

    pub(crate) fn send_cmd(&self, command: &str) {
        self.ctx.send(self.processor(), &self.create_command(command));
    }

    pub(crate) fn send_int(&self, command: &str, value: i32) {
        self.ctx.send_int(self.processor(), &self.create_command(command), value);
    }

    pub(crate) fn send_bool(&self, command: &str, value: bool) {
        self.ctx.send_bool(self.processor(), &self.create_command(command), value);
    }

    pub(crate) fn send_double(&self, command: &str, value: f64) {
        self.ctx.send_double(self.processor(), &self.create_command(command), value);
    }

    pub(crate) fn send_str(&self, command: &str, value: &str) {
        self.ctx.send_str(self.processor(), &self.create_command(command), value);
    }

    // ----------------------------------------------------------------------------------------
    // Internal setters used by the parser

    pub fn set_exists(&mut self, exists: bool) {
        self.exists = exists;
        self.volume.set_exists(exists);
        self.pan.set_exists(exists);
    }

    pub fn set_position(&mut self, position: i32) {
        if position >= 0 {
            self.position = position;
        }
    }

    pub(crate) fn set_index(&mut self, index: i32) {
        self.index = index;
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_type(&mut self, channel_type: ChannelType) {
        self.channel_type = Some(channel_type);
    }

    pub fn set_internal_volume(&mut self, volume: f64) {
        self.volume.set_internal_value(volume);
    }

    pub fn set_volume_str(&mut self, volume_str: &str) {
        self.volume.set_value_str(volume_str);
    }

    pub fn set_internal_pan(&mut self, pan: f64) {
        self.pan.set_internal_value(pan);
    }

    pub fn set_pan_str(&mut self, pan_str: &str) {
        self.pan.set_value_str(pan_str);
    }

    pub fn set_vu(&mut self, vu: f64) {
        self.vu = vu;
    }

    pub fn set_vu_left(&mut self, vu: f64) {
        self.vu_left = vu;
    }

    pub fn set_vu_right(&mut self, vu: f64) {
        self.vu_right = vu;
    }

    pub fn set_vu_hold_left(&mut self, vu: f64) {
        self.vu_hold_db_left = vu;
    }

    pub fn set_vu_hold_right(&mut self, vu: f64) {
        self.vu_hold_db_right = vu;
    }

    pub fn set_mute_state(&mut self, is_mute: bool) {
        self.is_mute = is_mute;
    }

    pub fn set_solo_state(&mut self, is_solo: bool) {
        self.is_solo = is_solo;
    }

    pub fn set_internal_is_activated(&mut self, is_activated: bool) {
        self.is_activated = is_activated;
    }

    pub fn set_color_state(&mut self, color: [f64; 3]) {
        self.color = Some(ColorEx::from_array(color));
    }

    /// Overrides the color (used for drum pads which show the color of the selected track).
    pub(crate) fn set_color_override(&mut self, color: Option<ColorEx>) {
        self.color_override = color;
    }
}

impl BankItem for Channel {
    fn does_exist(&self) -> bool {
        self.exists
    }

    fn is_selected(&self) -> bool {
        Channel::is_selected(self)
    }

    fn index(&self) -> i32 {
        self.index
    }

    fn position(&self) -> i32 {
        self.position
    }
}
