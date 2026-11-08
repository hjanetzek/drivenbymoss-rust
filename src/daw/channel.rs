// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A mixer channel (former `IChannel` / `ChannelImpl`), which is the base of tracks, the master
//! track and drum pads, and the bank of its sends (former `ISendBank` / `SendBankImpl`).

use std::cell::Cell;
use std::rc::Rc;
use std::ops::{Deref, DerefMut};

use crate::backend::Processor;
use crate::protocol::{Chan, Cmd, Rgb};

use super::bank::{BankItem, PagedBank};
use super::color::ColorEx;
use super::constants::ChannelType;
use super::context::{DawContext, HostFeature};
use super::events::ModelEvent;
use super::parameter::{Parameter, ParameterKind, Send};
use super::util;

const GRAY: ColorEx = ColorEx::new(0.2, 0.2, 0.2);

/// The clip border of the VU meters of Bitwig (ChannelImpl.CLIP_BORDER = 16241 of 16384).
const VU_CLIP_BORDER: f64 = 16241.0 / 16384.0;

/// The kind of a channel which defines how commands are addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    /// A track (`track` processor, commands are prefixed with the position).
    Track,
    /// The master track (`master` processor, no prefix).
    Master,
    /// A (virtual) drum pad (commands go to the processor of its device: `pad/<position>/...`).
    DrumPad,
    /// A layer of a device (commands go to the processor of its device: `layer/<position>/...`).
    Layer,
    /// A placeholder for a non-existing channel.
    Empty,
}

/// The sends of a channel. Derefs (mutably) to [`PagedBank<Send>`].
pub struct SendBank {
    bank: PagedBank<Send>,
    /// The processor of the track (`track` or `fxtrack`), also for the sends created later.
    processor: Rc<Cell<Processor>>,
}

impl SendBank {
    pub fn new(ctx: DawContext, num_sends: usize, channel_position: i32) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let processor = Rc::new(Cell::new(Processor::Track));
        let item_processor = processor.clone();
        let mut bank = PagedBank::new(
            ctx,
            num_sends,
            move |position| {
                let index = if num_sends == 0 { 0 } else { position % num_sends };
                let mut send = Send::new(item_ctx.clone(), channel_position, index as i32, position as i32);
                send.set_track_processor(item_processor.get());
                send
            },
            move || Send::empty(&empty_ctx),
        );
        bank.set_page_event(ModelEvent::SendBankPageChanged { track_position: channel_position });
        SendBank { bank, processor }
    }

    /// The processor of the track of the sends (`track` or `fxtrack`).
    pub(crate) fn set_processor(&mut self, processor: Processor) {
        self.processor.set(processor);
        for send in self.bank.all_items_mut() {
            send.set_track_processor(processor);
        }
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
    /// The host reported the activation state (Bitwig, also for the master).
    activation_reported: bool,
    color: Option<ColorEx>,
    color_override: Option<ColorEx>,
    volume: Parameter,
    pan: Parameter,
    send_bank: SendBank,
    /// The processor of the device which contains a drum pad or layer.
    owner: Processor,
}

impl Channel {
    /// Create a channel. `index` is the index in the page, `position` the absolute position.
    pub fn new(ctx: DawContext, kind: ChannelKind, index: i32, position: i32, num_sends: usize) -> Self {
        let (vol_kind, pan_kind) = match kind {
            ChannelKind::Master => (ParameterKind::MasterVolume, ParameterKind::MasterPan),
            ChannelKind::DrumPad => (ParameterKind::DeviceChannelVolume(Processor::Drum, "pad"), ParameterKind::DeviceChannelPan(Processor::Drum, "pad")),
            ChannelKind::Layer => (ParameterKind::DeviceChannelVolume(Processor::Primary, "layer"), ParameterKind::DeviceChannelPan(Processor::Primary, "layer")),
            _ => (ParameterKind::TrackVolume, ParameterKind::TrackPan),
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
            activation_reported: false,
            color: None,
            color_override: None,
            volume,
            pan,
            owner: match kind {
                ChannelKind::Layer => Processor::Primary,
                ChannelKind::Track => Processor::Track,
                _ => Processor::Drum,
            },
        }
    }

    /// Set the processor of the device which contains this drum pad or layer.
    pub(crate) fn with_owner(mut self, owner: Processor) -> Self {
        let prefix = if self.kind == ChannelKind::Layer { "layer" } else { "pad" };
        self.owner = owner;
        self.volume.set_kind(ParameterKind::DeviceChannelVolume(owner, prefix));
        self.pan.set_kind(ParameterKind::DeviceChannelPan(owner, prefix));
        self
    }

    /// Set the processor of a track: `track`, or `fxtrack` for an effect track (its commands, volume
    /// and panorama go there).
    pub(crate) fn set_track_owner(&mut self, owner: Processor) {
        self.owner = owner;
        self.volume.set_track_processor(owner);
        self.pan.set_track_processor(owner);
        self.send_bank.set_processor(owner);
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

    /// Bitwig's shortened text if reported, otherwise the text cut to the length (Reaper).
    pub fn volume_str_limited(&self, limit: usize) -> String {
        match self.volume.host_limited_value_str(limit) {
            Some(text) if self.exists => text.to_string(),
            _ => util::limit(&self.volume_str(), limit),
        }
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

    /// ChannelImpl.resetVolume: the volume parameter's reset (Bitwig: `parameter.reset ()`).
    pub fn reset_volume(&mut self) {
        if self.ctx.host_supports(HostFeature::ValueReset) {
            self.volume.reset_value();
            return;
        }
        let value = (0.716 * self.ctx.value_changer().upper_bound() as f64) as i32;
        self.set_volume(value);
    }

    pub fn touch_volume(&mut self, is_being_touched: bool) {
        self.volume.touch_value(is_being_touched);
    }

    /// ChannelImpl.setVolumeIndication: the volume parameter's `setIndication` (Bitwig).
    pub fn set_volume_indication(&mut self, indicate: bool) {
        self.volume.set_indication(indicate);
    }

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

    /// Bitwig's shortened text if reported, otherwise the text cut to the length (Reaper).
    pub fn pan_str_limited(&self, limit: usize) -> String {
        match self.pan.host_limited_value_str(limit) {
            Some(text) if self.exists => text.to_string(),
            _ => util::limit(&self.pan_str(), limit),
        }
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

    /// ChannelImpl.resetPan: the panorama parameter's reset (Bitwig: `parameter.reset ()`).
    pub fn reset_pan(&mut self) {
        if self.ctx.host_supports(HostFeature::ValueReset) {
            self.pan.reset_value();
            return;
        }
        let value = self.ctx.value_changer().upper_bound() / 2;
        self.set_pan(value);
    }

    pub fn touch_pan(&mut self, is_being_touched: bool) {
        self.pan.touch_value(is_being_touched);
    }

    /// ChannelImpl.setPanIndication: the panorama parameter's `setIndication` (Bitwig).
    pub fn set_pan_indication(&mut self, indicate: bool) {
        self.pan.set_indication(indicate);
    }

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
        self.ctx.send(Cmd::ChannelColor { ch: self.address(), value: Rgb(color.to_int_rgb255()) });
    }

    // ----------------------------------------------------------------------------------------
    // Mute / Solo

    pub fn is_mute(&self) -> bool {
        self.exists && self.is_mute
    }

    pub fn set_mute(&mut self, value: bool) {
        if self.ctx.is_automation_rec_active() {
            self.ctx.send(Cmd::DelayUpdates { area: if self.kind == ChannelKind::Track { self.owner } else { Processor::Track } });
        }
        self.set_mute_state(value);
        self.ctx.send(Cmd::ChannelMute { ch: self.address(), value });
    }

    pub fn toggle_mute(&mut self) {
        self.set_mute(!self.is_mute());
    }

    pub fn is_solo(&self) -> bool {
        self.exists && self.is_solo
    }

    pub fn set_solo(&mut self, value: bool) {
        self.set_solo_state(value);
        self.ctx.send(Cmd::ChannelSolo { ch: self.address(), value });
    }

    /// ChannelImpl.toggleSolo: `solo ().toggleUsingPreferences (false)` on Bitwig (exclusive
    /// solo as set in Bitwig's preferences): `solo/toggle`.
    pub fn toggle_solo(&mut self) {
        if self.ctx.host_supports(HostFeature::SoloToggle) {
            self.ctx.send(Cmd::ChannelSoloToggle { ch: self.address() });
            return;
        }
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

    /// ChannelImpl.getVuLeftClipState: `vuLeft >= CLIP_BORDER` (16241 of 16384) on Bitwig;
    /// Reaper (a host which reports the VU hold values): the hold value above 0 dB.
    pub fn vu_left_clip_state(&self) -> bool {
        if !self.ctx.host_reports_vu_hold() {
            return self.vu_left >= VU_CLIP_BORDER;
        }
        self.vu_hold_db_left > 0.0
    }

    pub fn vu_right_clip_state(&self) -> bool {
        if !self.ctx.host_reports_vu_hold() {
            return self.vu_right >= VU_CLIP_BORDER;
        }
        self.vu_hold_db_right > 0.0
    }

    // ----------------------------------------------------------------------------------------
    // Other commands

    /// ChannelImpl.setIsActivated: `isActivated ().set (value)`, also for the master on a host
    /// which reports its activation state (Bitwig; Reaper can not deactivate its master).
    pub fn set_is_activated(&mut self, enable: bool) {
        if self.kind == ChannelKind::Master && !self.activation_reported {
            return;
        }
        self.is_activated = enable;
        self.ctx.send(Cmd::ChannelActive { ch: self.address(), value: enable });
    }

    pub fn toggle_is_activated(&mut self) {
        let activated = self.is_activated;
        self.set_is_activated(!activated);
    }

    /// Remove the channel.
    pub fn remove(&mut self) {
        self.ctx.send(Cmd::ChannelRemove { ch: self.address() });
    }

    /// Duplicate the channel.
    pub fn duplicate(&mut self) {
        self.ctx.send(Cmd::ChannelDuplicate { ch: self.address() });
    }

    /// Add the device with the given creation name to the channel.
    pub fn add_device(&mut self, creation_name: &str) {
        self.ctx.send(Cmd::DeviceAdd { name: creation_name });
    }

    pub fn add_equalizer_device(&mut self) {
        self.ctx.send(Cmd::EqAdd {});
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
            ChannelKind::Track | ChannelKind::Master => self.ctx.send(Cmd::ChannelSelect { ch: self.address(), value: true }),
            _ => {}
        }
    }

    // ----------------------------------------------------------------------------------------
    // Sending

    /// The address of the channel in the commands (processor and prefix): tracks `<owner>
    /// <position>/…` (`track` or `fxtrack`), the master `master …`, drum pads and layers `<owner>
    /// pad|layer/<position>/…` (protocol v2; Reaper ignores them), others `track <position>/…`.
    pub(crate) fn address(&self) -> Chan {
        match self.kind {
            ChannelKind::Master => Chan::Master,
            ChannelKind::DrumPad => Chan::Pad { owner: self.owner, i: self.position },
            ChannelKind::Layer => Chan::Layer { owner: self.owner, i: self.position },
            ChannelKind::Track => Chan::Track { bank: self.owner, p: self.position },
            ChannelKind::Empty => Chan::Track { bank: Processor::Track, p: self.position },
        }
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
        self.ctx.set_host_reports_vu_hold();
        self.vu_hold_db_left = vu;
    }

    pub fn set_vu_hold_right(&mut self, vu: f64) {
        self.ctx.set_host_reports_vu_hold();
        self.vu_hold_db_right = vu;
    }

    pub fn set_mute_state(&mut self, is_mute: bool) {
        self.is_mute = is_mute;
    }

    pub fn set_solo_state(&mut self, is_solo: bool) {
        self.is_solo = is_solo;
    }

    pub fn set_internal_is_activated(&mut self, is_activated: bool) {
        self.activation_reported = true;
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

    /// Select a layer of a device (former `LayerImpl.select`); the host reports the selection
    /// back. Tracks, the master and drum pads have their own selection.
    fn select(&mut self) {
        if self.kind == ChannelKind::Layer {
            self.ctx.send(Cmd::ChannelSelectInEditor { ch: self.address() });
        }
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
