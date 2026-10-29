// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Parameters (former `IParameter` with `ParameterImpl` and its Reaper sub-classes for volume,
//! panorama, master, sends, groove and the master crossfader).
//!
//! Values follow the value changer convention: integer values are in the range of
//! `[0..upper_bound-1]`, internally the normalized value `[0..1]` is stored.

use crate::backend::Processor;
use crate::framework::value_changer::ValueChanger;

use super::bank::BankItem;
use super::color::ColorEx;
use super::constants::GrooveParameterId;
use super::context::{DawContext, MASTER_POSITION};
use super::util;

/// Defines how a parameter sends its value to the DAW.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterKind {
    /// A device parameter, sent as `param/<position>/value` to the given processor. For the
    /// track processor the position of the cursor track is prepended (track FX parameters).
    Device(Processor),
    /// The volume of the track (`track` `<track>/volume`).
    TrackVolume,
    /// The panorama of the track (`track` `<track>/pan`).
    TrackPan,
    /// The volume of the master track (`master` `volume`).
    MasterVolume,
    /// The panorama of the master track (`master` `pan`).
    MasterPan,
    /// The volume of a drum pad (`drum` `pad/<pad>/volume`, protocol v2).
    DrumPadVolume,
    /// The panorama of a drum pad (`drum` `pad/<pad>/pan`, protocol v2).
    DrumPadPan,
    /// A send of the track with the given position (`track` `<track>/send/<pos>/volume`).
    Send { channel_position: i32 },
    /// The crossfader on the master track (`master` `user/param/0/value`).
    MasterCrossfader,
    /// A parameter of the groove (`groove` `active` / `amount`).
    Groove(GrooveParameterId),
    /// A parameter which does not exist (former `EmptyParameter`).
    Empty,
}

/// A parameter of a device, track or other part of the DAW.
pub struct Parameter {
    ctx: DawContext,
    kind: ParameterKind,
    index: i32,
    position: i32,
    exists: bool,
    selected: bool,
    name: String,
    value_str: String,
    is_being_touched: bool,
    value: f64,
    last_received_value: f64,
    number_of_steps: i32,
    default_value: i32,
    /// The modulated value reported by the host (Bitwig), `None` if not reported (Reaper).
    modulated: Option<f64>,
}

impl Parameter {
    /// Create a parameter. `index` is the index in the page of the bank, `default_value` the
    /// normalized default value.
    pub fn new(ctx: DawContext, kind: ParameterKind, index: i32, default_value: f64) -> Self {
        let default_value = ctx.value_changer().from_normalized_value(default_value);
        let mut p = Parameter {
            ctx,
            kind,
            index,
            position: 0,
            exists: false,
            selected: false,
            name: String::new(),
            value_str: String::new(),
            is_being_touched: false,
            value: 0.0,
            last_received_value: -1.0,
            number_of_steps: 0,
            default_value,
            modulated: None,
        };
        match kind {
            ParameterKind::TrackVolume | ParameterKind::DrumPadVolume => p.name = "Volume".into(),
            ParameterKind::TrackPan | ParameterKind::DrumPadPan => p.name = "Pan".into(),
            ParameterKind::MasterVolume => p.name = "Master Volume".into(),
            ParameterKind::MasterPan => p.name = "Master Pan".into(),
            ParameterKind::Groove(id) => {
                p.exists = true;
                p.name = match id {
                    GrooveParameterId::Enabled => "Enabled".into(),
                    GrooveParameterId::ShuffleAmount => "Shuffle Amount".into(),
                    _ => String::new(),
                };
            }
            ParameterKind::Empty => p.index = -1,
            _ => {}
        }
        p
    }

    /// Create a parameter which does not exist.
    pub fn empty(ctx: &DawContext) -> Self {
        let mut p = Parameter::new(ctx.muted(), ParameterKind::Empty, -1, 0.0);
        p.position = -1;
        p
    }

    pub fn kind(&self) -> ParameterKind {
        self.kind
    }

    fn vc(&self) -> &ValueChanger {
        self.ctx.value_changer()
    }

    pub fn does_exist(&self) -> bool {
        self.exists
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn position(&self) -> i32 {
        self.position
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

    pub fn is_selected(&self) -> bool {
        self.exists && self.selected
    }

    /// The value formatted by the DAW.
    pub fn displayed_value(&self) -> String {
        match self.kind {
            ParameterKind::Groove(GrooveParameterId::Enabled) => (self.value as i32).to_string(),
            ParameterKind::Groove(_) => format!("{:.1} %", (self.value * 2.0 - 1.0) * 100.0),
            _ => self.value_str.clone(),
        }
    }

    /// The displayed value (not shortened, as in the Java implementation).
    pub fn displayed_value_limited(&self, _limit: usize) -> String {
        self.displayed_value()
    }

    /// The value in the range of the value changer.
    pub fn value(&self) -> i32 {
        self.vc().from_normalized_value(self.value.max(0.0))
    }

    /// The modulated value as reported by the host (Bitwig), otherwise the value (Reaper).
    pub fn modulated_value(&self) -> i32 {
        match self.modulated {
            Some(modulated) => self.vc().from_normalized_value(modulated.max(0.0)),
            None => self.value(),
        }
    }

    pub fn set_modulated_value(&mut self, value: f64) {
        self.modulated = Some(value);
    }

    /// The normalized value [0..1].
    pub fn internal_value(&self) -> f64 {
        self.value.max(0.0)
    }

    /// The number of steps of the parameter (0 if continuous).
    pub fn number_of_steps(&self) -> i32 {
        self.number_of_steps
    }

    /// Increase/decrease the value by the given amount.
    pub fn inc(&mut self, increment: f64) {
        let upper = self.vc().upper_bound() as f64;
        match self.kind {
            ParameterKind::Send { .. } => self.set_value((self.value() as f64 + increment) as i32),
            _ => self.set_value((self.value() as f64 + increment).clamp(0.0, upper - 1.0) as i32),
        }
    }

    /// Change the value with a relative knob value.
    pub fn change_value(&mut self, control: i32) {
        let vc = self.vc().clone();
        self.change_value_with(&vc, control);
    }

    /// Change the value with a relative knob value and the given value changer.
    pub fn change_value_with(&mut self, value_changer: &ValueChanger, control: i32) {
        let mut offset = value_changer.calc_knob_change(control);
        let is_negative = offset < 0.0;
        offset = value_changer.to_normalized_value(offset.abs());
        if is_negative {
            offset = -offset;
        }
        self.set_normalized_value((self.internal_value() + offset).clamp(0.0, 1.0));
    }

    /// Set the value (in the range of the value changer).
    pub fn set_value(&mut self, value: i32) {
        let normalized = self.vc().to_normalized_value(value as f64);
        if let ParameterKind::Send { .. } = self.kind {
            if !self.exists {
                return;
            }
            if self.ctx.is_automation_rec_active() {
                self.ctx.delay_updates(Processor::Track);
            }
            self.value = normalized;
            self.send_value();
            return;
        }
        self.set_normalized_value(normalized);
    }

    /// Set the value with the given value changer.
    pub fn set_value_with(&mut self, value_changer: &ValueChanger, value: i32) {
        self.set_normalized_value(value_changer.to_normalized_value(value as f64));
    }

    pub fn set_value_immediatly(&mut self, value: i32) {
        self.set_value(value);
    }

    /// Set the normalized value [0..1].
    pub fn set_normalized_value(&mut self, value: f64) {
        if !self.exists {
            return;
        }
        self.value = value;
        self.send_value();
    }

    /// Reset the value to its default.
    pub fn reset_value(&mut self) {
        match self.kind {
            ParameterKind::Groove(id) if id != GrooveParameterId::Enabled => self.set_normalized_value(0.5),
            _ => self.set_value(self.default_value),
        }
    }

    /// Signal that the parameter is touched (e.g. a touch sensitive knob). While touched, values
    /// received from the DAW are deferred.
    pub fn touch_value(&mut self, is_being_touched: bool) {
        let touch = if is_being_touched { 1 } else { 0 };
        match self.kind {
            ParameterKind::TrackVolume => self.ctx.send_int(Processor::Track, &format!("{}/volume/touch", self.index), touch),
            ParameterKind::TrackPan => self.ctx.send_int(Processor::Track, &format!("{}/pan/touch", self.index), touch),
            ParameterKind::MasterVolume => self.ctx.send_int(Processor::Master, "volume/touch", touch),
            ParameterKind::MasterPan => self.ctx.send_int(Processor::Master, "pan/touch", touch),
            ParameterKind::Device(processor) => self.ctx.send_int(processor, &format!("param/{}/touch", self.position), touch),
            _ => {}
        }
        self.is_being_touched = is_being_touched;
        if self.is_being_touched || self.last_received_value < 0.0 {
            return;
        }
        self.value = self.last_received_value;
        self.last_received_value = -1.0;
    }

    /// Indicate the parameter on the DAW UI (device parameters, not supported by Reaper).
    pub fn set_indication(&mut self, enable: bool) {
        if let ParameterKind::Device(processor) = self.kind {
            if self.position >= 0 {
                self.ctx.send_int(processor, &format!("param/{}/indicate", self.position), if enable { 1 } else { 0 });
            }
        }
    }

    fn send_value(&self) {
        match self.kind {
            ParameterKind::Device(processor) => {
                let mut cmd = format!("param/{}/value", self.position);
                if processor == Processor::Track {
                    let pos = self.ctx.cursor_track_position();
                    if pos == MASTER_POSITION {
                        // The master track uses no position prefix
                    } else if pos >= 0 {
                        cmd = format!("{pos}/{cmd}");
                    }
                }
                self.ctx.send_double(processor, &cmd, self.value);
            }
            ParameterKind::TrackVolume | ParameterKind::TrackPan => {
                if self.ctx.is_automation_rec_active() {
                    self.ctx.delay_updates(Processor::Track);
                }
                let name = if self.kind == ParameterKind::TrackVolume { "volume" } else { "pan" };
                self.ctx.send_double(Processor::Track, &format!("{}/{}", self.index, name), self.value);
            }
            ParameterKind::DrumPadVolume | ParameterKind::DrumPadPan => {
                let name = if self.kind == ParameterKind::DrumPadVolume { "volume" } else { "pan" };
                self.ctx.send_double(Processor::Drum, &format!("pad/{}/{}", self.index, name), self.value);
            }
            ParameterKind::MasterVolume | ParameterKind::MasterPan => {
                if self.ctx.is_automation_rec_active() {
                    self.ctx.delay_updates(Processor::Master);
                }
                let name = if self.kind == ParameterKind::MasterVolume { "volume" } else { "pan" };
                self.ctx.send_double(Processor::Master, name, self.value);
            }
            ParameterKind::Send { channel_position } => {
                self.ctx.send_double(Processor::Track, &format!("{}/send/{}/volume", channel_position, self.position), self.value);
            }
            ParameterKind::MasterCrossfader => self.ctx.send_double(Processor::Master, "user/param/0/value", self.value),
            ParameterKind::Groove(GrooveParameterId::Enabled) => self.ctx.send_double(Processor::Groove, "active", self.value),
            ParameterKind::Groove(_) => self.ctx.send_double(Processor::Groove, "amount", self.value * 2.0 - 1.0),
            ParameterKind::Empty => {}
        }
    }

    // ----------------------------------------------------------------------------------------
    // Internal setters used by the parser

    /// Set the value received from the DAW (deferred while the parameter is touched).
    pub fn set_internal_value(&mut self, value: f64) {
        let value = match self.kind {
            ParameterKind::Groove(GrooveParameterId::Enabled) => {
                if value > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            ParameterKind::Groove(_) => (value + 1.0) / 2.0,
            _ => value,
        };
        if self.is_being_touched {
            self.last_received_value = value;
        } else {
            self.value = value;
        }
    }

    pub fn set_value_str(&mut self, value_str: &str) {
        self.value_str = value_str.to_string();
    }

    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_position(&mut self, position: i32) {
        if position >= 0 {
            self.position = position;
        }
    }

    pub fn set_exists(&mut self, exists: bool) {
        self.exists = exists;
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn set_internal_number_of_steps(&mut self, number_of_steps: i32) {
        self.number_of_steps = number_of_steps;
    }

    pub(crate) fn set_kind(&mut self, kind: ParameterKind) {
        self.kind = kind;
    }

    /// Copy name, existence, value and displayed value to the given parameter.
    pub fn copy_values(&self, dest: &mut Parameter) {
        dest.set_internal_name(&self.name);
        dest.set_exists(self.exists);
        dest.set_internal_value(self.value);
        dest.set_value_str(&self.value_str);
    }
}

impl BankItem for Parameter {
    fn does_exist(&self) -> bool {
        self.exists
    }

    fn is_selected(&self) -> bool {
        Parameter::is_selected(self)
    }

    fn index(&self) -> i32 {
        self.index
    }

    fn position(&self) -> i32 {
        self.position
    }
}

/// A send of a channel (former `ISend` / `SendImpl`). Derefs to its [`Parameter`].
pub struct Send {
    param: Parameter,
    color: Option<ColorEx>,
    is_enabled: bool,
}

impl Send {
    pub fn new(ctx: DawContext, channel_position: i32, index: i32, position: i32) -> Self {
        let mut param = Parameter::new(ctx, ParameterKind::Send { channel_position }, index, 0.0);
        param.set_position(position);
        Send { param, color: None, is_enabled: false }
    }

    pub fn empty(ctx: &DawContext) -> Self {
        Send { param: Parameter::empty(ctx), color: None, is_enabled: false }
    }

    /// The color of the send (the color of the send/return channel).
    pub fn color(&self) -> ColorEx {
        self.color.unwrap_or(ColorEx::BLACK)
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Toggle the enabled state of the send.
    pub fn toggle_enabled(&mut self) {
        if let ParameterKind::Send { channel_position } = self.param.kind {
            let cmd = format!("{}/send/{}/active", channel_position, self.param.position);
            self.param.ctx.send_double(Processor::Track, &cmd, self.param.value);
        }
    }

    pub fn set_color_state(&mut self, color: [f64; 3]) {
        self.color = Some(ColorEx::from_array(color));
    }

    pub fn set_internal_enabled(&mut self, is_enabled: bool) {
        self.is_enabled = is_enabled;
    }

    pub(crate) fn set_channel_position(&mut self, channel_position: i32) {
        if let ParameterKind::Send { .. } = self.param.kind {
            self.param.set_kind(ParameterKind::Send { channel_position });
        }
    }
}

impl std::ops::Deref for Send {
    type Target = Parameter;
    fn deref(&self) -> &Parameter {
        &self.param
    }
}

impl std::ops::DerefMut for Send {
    fn deref_mut(&mut self) -> &mut Parameter {
        &mut self.param
    }
}

impl BankItem for Send {
    fn does_exist(&self) -> bool {
        self.param.exists
    }
    fn is_selected(&self) -> bool {
        self.param.is_selected()
    }
    fn index(&self) -> i32 {
        self.param.index
    }
    fn position(&self) -> i32 {
        self.param.position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendCall;
    use crate::daw::bank::tests::test_ctx;

    #[test]
    fn volume_and_touch() {
        let (ctx, backend) = test_ctx();
        let mut p = Parameter::new(ctx, ParameterKind::TrackVolume, 3, 0.716);
        p.set_value(64);
        assert!(backend.take_calls().is_empty(), "Must not send if it does not exist");
        p.set_exists(true);
        p.set_value(127);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("track".into(), "3/volume".into(), 1.0)]);
        p.touch_value(true);
        p.set_internal_value(0.5);
        assert_eq!(p.value(), 127);
        p.touch_value(false);
        assert_eq!(p.value(), 64);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::IntArg("track".into(), "3/volume/touch".into(), 1),
                BackendCall::IntArg("track".into(), "3/volume/touch".into(), 0)
            ]
        );
        p.reset_value();
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("track".into(), "3/volume".into(), 91.0 / 127.0)]);
    }

    #[test]
    fn groove_and_device() {
        let (ctx, backend) = test_ctx();
        let mut p = Parameter::new(ctx.clone(), ParameterKind::Groove(GrooveParameterId::ShuffleAmount), 1, 0.0);
        p.set_internal_value(0.0);
        assert_eq!(p.displayed_value(), "0.0 %");
        p.set_normalized_value(1.0);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("groove".into(), "amount".into(), 1.0)]);
        let mut d = Parameter::new(ctx.clone(), ParameterKind::Device(Processor::Track), 0, 0.0);
        d.set_exists(true);
        d.set_position(5);
        ctx.set_cursor_track_position(2);
        d.set_normalized_value(0.25);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("track".into(), "2/param/5/value".into(), 0.25)]);
    }
}
