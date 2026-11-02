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
use super::context::{DawContext, HostFeature, MASTER_POSITION};
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
    /// The volume of a channel of a device, a drum pad or layer (`<processor>`
    /// `<prefix>/<position>/volume`, e.g. `drum` `pad/36/volume`, protocol v2).
    DeviceChannelVolume(Processor, &'static str),
    /// The panorama of a channel of a device (`<processor>` `<prefix>/<position>/pan`).
    DeviceChannelPan(Processor, &'static str),
    /// A send of the track with the given position (`track` `<track>/send/<pos>/volume`).
    Send { channel_position: i32 },
    /// The crossfader on the master track (`master` `user/param/0/value`).
    MasterCrossfader,
    /// The crossfader of the host's transport (Bitwig: `transport` `crossfade`, the
    /// `ParameterImpl` on `transport.crossfade ()` of TransportImpl in the Java version). The
    /// master crossfader becomes this one when the host reports `/crossfade`.
    HostCrossfade,
    /// A parameter of the groove (`groove` `active` / `amount`).
    Groove(GrooveParameterId),
    /// The cue volume or cue mix of the project (Bitwig: `project` `cueVolume` / `cueMix`,
    /// ProjectImpl of the Java version).
    ProjectCue(&'static str),
    /// A parameter of the groove of the host (Bitwig: `groove` `<name>`, GrooveImpl of the Java
    /// version: a `ParameterImpl` on `groove.getEnabled ()` etc.).
    HostGroove(&'static str),
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
    /// The displayed value shortened by the host for some lengths (Bitwig: `getLimited`, which
    /// shortens in its own way, e.g. drops the unit), empty if not reported (Reaper).
    limited_value_strs: Vec<(usize, String)>,
    /// The value is sent to be applied immediately, without the takeover mode of the host
    /// (see [`Parameter::set_value_immediatly`]).
    immediately: bool,
    /// The processor of the track volume, panorama and sends: `track`, or `fxtrack` for the
    /// effect tracks.
    track_processor: Processor,
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
            immediately: false,
            track_processor: Processor::Track,
            value: 0.0,
            last_received_value: -1.0,
            number_of_steps: 0,
            default_value,
            modulated: None,
            limited_value_strs: Vec::new(),
        };
        match kind {
            ParameterKind::TrackVolume | ParameterKind::DeviceChannelVolume(..) => p.name = "Volume".into(),
            ParameterKind::TrackPan | ParameterKind::DeviceChannelPan(..) => p.name = "Pan".into(),
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

    /// The displayed value shortened by the host for the length (Bitwig), otherwise not
    /// shortened (as the Java version for Reaper).
    pub fn displayed_value_limited(&self, limit: usize) -> String {
        self.host_limited_value_str(limit).map_or_else(|| self.displayed_value(), str::to_string)
    }

    /// The displayed value shortened by the host for the length, if reported (Bitwig).
    pub fn host_limited_value_str(&self, limit: usize) -> Option<&str> {
        self.limited_value_strs.iter().find(|(l, _)| *l == limit).map(|(_, s)| s.as_str())
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

    /// True if the host understands the command form of the feature for this parameter's value
    /// command (not the Reaper groove, which has no such suffixes).
    fn host_value_command(&self, feature: HostFeature) -> bool {
        self.ctx.host_supports(feature) && !matches!(self.kind, ParameterKind::Groove(_) | ParameterKind::Empty)
    }

    /// True if the host changes the value relatively (Bitwig: `…/inc`, as `RangedValueImpl.inc`
    /// of the Java version for Bitwig). Reaper gets absolute values (its backend has no `/inc`).
    fn host_increments(&self) -> bool {
        self.host_value_command(HostFeature::ValueInc)
    }

    /// Increase/decrease the value by the given amount (in the range of the value changer).
    /// Bitwig: RangedValueImpl.inc, `rangedValue.inc (increment, upperBound)`: the host changes
    /// its value (also through its takeover mode, keeping fractions), the core gets the result.
    pub fn inc(&mut self, increment: f64) {
        if self.host_increments() {
            let upper = self.vc().upper_bound() as f64;
            self.send_increment(increment / upper);
            return;
        }
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
        if self.host_increments() {
            // RangedValueImpl.changeValue: this.inc (valueChanger.calcKnobChange (value)), the
            // increment relative to the upper bound of the parameter's own value changer
            self.inc(value_changer.calc_knob_change(control));
            return;
        }
        let mut offset = value_changer.calc_knob_change(control);
        let is_negative = offset < 0.0;
        offset = value_changer.to_normalized_value(offset.abs());
        if is_negative {
            offset = -offset;
        }
        self.set_normalized_value((self.internal_value() + offset).clamp(0.0, 1.0));
    }

    /// The processor of a track parameter (volume, panorama, send), see `track_processor`.
    pub(crate) fn set_track_processor(&mut self, processor: Processor) {
        self.track_processor = processor;
    }

    /// Set the value (in the range of the value changer).
    pub fn set_value(&mut self, value: i32) {
        let normalized = self.vc().to_normalized_value(value as f64);
        if let ParameterKind::Send { .. } = self.kind {
            if !self.exists {
                return;
            }
            if self.ctx.is_automation_rec_active() {
                self.ctx.delay_updates(self.track_processor);
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

    /// Set the value without the takeover mode of the host (Bitwig: catch, immediate or
    /// relative scaling, `SettableRangedValue.setImmediately`), for controls without a physical
    /// position such as pads. Reaper has no takeover mode, there it is the same as `set_value`.
    pub fn set_value_immediatly(&mut self, value: i32) {
        self.immediately = true;
        self.set_value(value);
        self.immediately = false;
    }

    /// Set the normalized value [0..1].
    pub fn set_normalized_value(&mut self, value: f64) {
        if !self.exists {
            return;
        }
        self.value = value;
        self.send_value();
    }

    /// Reset the value to its default. With [`HostFeature::ValueReset`] (Bitwig) as
    /// ParameterImpl.resetValue: `parameter.reset ()`, Bitwig's own default (`<value
    /// command>/reset`). Reaper: the default of the core (the Reaper backend has no reset).
    pub fn reset_value(&mut self) {
        if self.host_value_command(HostFeature::ValueReset) {
            if let Some((processor, cmd)) = self.value_address() {
                self.ctx.send(processor, &format!("{cmd}/reset"));
            }
            return;
        }
        match self.kind {
            ParameterKind::Groove(id) if id != GrooveParameterId::Enabled => self.set_normalized_value(0.5),
            _ => self.set_value(self.default_value),
        }
    }

    /// Signal that the parameter is touched (e.g. a touch sensitive knob). While touched, values
    /// received from the DAW are deferred (Reaper; Bitwig: only `parameter.touch (b)`).
    pub fn touch_value(&mut self, is_being_touched: bool) {
        let touch = if is_being_touched { 1 } else { 0 };
        match self.kind {
            ParameterKind::TrackVolume => self.ctx.send_int(self.track_processor, &format!("{}/volume/touch", self.index), touch),
            ParameterKind::TrackPan => self.ctx.send_int(self.track_processor, &format!("{}/pan/touch", self.index), touch),
            ParameterKind::MasterVolume => self.ctx.send_int(Processor::Master, "volume/touch", touch),
            ParameterKind::MasterPan => self.ctx.send_int(Processor::Master, "pan/touch", touch),
            ParameterKind::Device(processor) => self.ctx.send_int(processor, &format!("param/{}/touch", self.position), touch),
            ParameterKind::ProjectCue(name) => self.ctx.send_int(Processor::Project, &format!("{name}/touch"), touch),
            ParameterKind::HostGroove(name) => self.ctx.send_int(Processor::Groove, &format!("{name}/touch"), touch),
            ParameterKind::HostCrossfade => self.ctx.send_int(Processor::Transport, "crossfade/touch", touch),
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
            return;
        }
        // ParameterImpl.setIndication of the Java version for Bitwig: also the volume, panorama
        // and sends (`<value command>/indicate`)
        if !self.host_value_command(HostFeature::Indication) {
            return;
        }
        if let Some((processor, cmd)) = self.value_address() {
            self.ctx.send_int(processor, &format!("{cmd}/indicate"), if enable { 1 } else { 0 });
        }
    }

    /// The processor and the command of the value (without a suffix), None if the parameter
    /// has no value command.
    fn value_address(&self) -> Option<(Processor, String)> {
        Some(match self.kind {
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
                (processor, cmd)
            }
            ParameterKind::TrackVolume => (self.track_processor, format!("{}/volume", self.index)),
            ParameterKind::TrackPan => (self.track_processor, format!("{}/pan", self.index)),
            ParameterKind::DeviceChannelVolume(processor, prefix) => (processor, format!("{prefix}/{}/volume", self.index)),
            ParameterKind::DeviceChannelPan(processor, prefix) => (processor, format!("{prefix}/{}/pan", self.index)),
            ParameterKind::MasterVolume => (Processor::Master, "volume".into()),
            ParameterKind::MasterPan => (Processor::Master, "pan".into()),
            ParameterKind::Send { channel_position } => (self.track_processor, format!("{}/send/{}/volume", channel_position, self.position)),
            ParameterKind::MasterCrossfader => (Processor::Master, "user/param/0/value".into()),
            ParameterKind::HostCrossfade => (Processor::Transport, "crossfade".into()),
            ParameterKind::ProjectCue(name) => (Processor::Project, name.into()),
            ParameterKind::HostGroove(name) => (Processor::Groove, name.into()),
            ParameterKind::Groove(GrooveParameterId::Enabled) => (Processor::Groove, "active".into()),
            ParameterKind::Groove(_) => (Processor::Groove, "amount".into()),
            ParameterKind::Empty => return None,
        })
    }

    /// Send a relative change of the normalized value (`<value command>/inc <delta>`, Bitwig
    /// `SettableRangedValue.inc`).
    fn send_increment(&self, delta: f64) {
        if let Some((processor, cmd)) = self.value_address() {
            self.ctx.send_double(processor, &format!("{cmd}/inc"), delta);
        }
    }

    fn send_value(&self) {
        let Some((processor, cmd)) = self.value_address() else {
            return;
        };
        match self.kind {
            ParameterKind::TrackVolume | ParameterKind::TrackPan if self.ctx.is_automation_rec_active() => self.ctx.delay_updates(self.track_processor),
            ParameterKind::MasterVolume | ParameterKind::MasterPan if self.ctx.is_automation_rec_active() => self.ctx.delay_updates(Processor::Master),
            _ => {}
        }
        let value = match self.kind {
            ParameterKind::Groove(GrooveParameterId::Enabled) => self.value,
            ParameterKind::Groove(_) => self.value * 2.0 - 1.0,
            _ => self.value,
        };
        // The host knows the commands to set a value immediately (Bitwig)
        let suffix = if self.immediately && self.ctx.host_supports(HostFeature::Immediately) { "/immediately" } else { "" };
        self.ctx.send_double(processor, &format!("{cmd}{suffix}"), value);
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
        // ParameterImpl.touchValue of the Java version for Bitwig only calls parameter.touch (b):
        // the values of the host are shown while touched (Reaper: held back until released). A
        // host which changes the values relatively has to: the core does not change them itself
        if self.is_being_touched && !self.ctx.host_supports(HostFeature::ValueInc) {
            self.last_received_value = value;
        } else {
            self.value = value;
        }
    }

    pub fn set_value_str(&mut self, value_str: &str) {
        self.value_str = value_str.to_string();
    }

    /// The host reported the displayed value shortened for the length (Bitwig).
    pub fn set_limited_value_str(&mut self, limit: usize, value_str: &str) {
        match self.limited_value_strs.iter_mut().find(|(l, _)| *l == limit) {
            Some(entry) => entry.1 = value_str.to_string(),
            None => self.limited_value_strs.push((limit, value_str.to_string())),
        }
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
    /// The processor of the track of the send (`track` or `fxtrack`).
    pub(crate) fn set_track_processor(&mut self, processor: Processor) {
        self.param.set_track_processor(processor);
    }

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
            self.param.ctx.send_double(self.param.track_processor, &cmd, self.param.value);
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
    fn set_value_immediately_only_on_bitwig() {
        let (ctx, backend) = test_ctx();
        let mut p = Parameter::new(ctx.clone(), ParameterKind::Send { channel_position: 2 }, 0, 0.0);
        p.set_exists(true);
        p.set_value_immediatly(127);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("track".into(), "2/send/0/volume".into(), 1.0)]);
        ctx.set_host_paging();
        p.set_value_immediatly(0);
        p.set_value(127);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::DoubleArg("track".into(), "2/send/0/volume/immediately".into(), 0.0),
                BackendCall::DoubleArg("track".into(), "2/send/0/volume".into(), 1.0)
            ]
        );
    }

    /// RangedValueImpl.inc / changeValue: Bitwig changes the value relatively (`…/inc` with the
    /// normalized delta, fractions kept); Reaper gets the absolute value.
    #[test]
    fn relative_changes_on_bitwig() {
        let (ctx, backend) = test_ctx();
        let mut volume = Parameter::new(ctx.clone(), ParameterKind::TrackVolume, 3, 0.716);
        volume.set_exists(true);
        volume.set_internal_value(0.5);
        volume.inc(1.0);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("track".into(), "3/volume".into(), 65.0 / 127.0)]);
        ctx.set_host_paging();
        volume.inc(1.5);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("track".into(), "3/volume/inc".into(), 1.5 / 128.0)]);
        let mut param = Parameter::new(ctx.clone(), ParameterKind::Device(Processor::Device), 0, 0.0);
        param.set_exists(true);
        param.set_position(9);
        let control = ctx.value_changer().clone();
        let delta = control.calc_knob_change(127);
        param.change_value(127);
        let mut send = Parameter::new(ctx.clone(), ParameterKind::Send { channel_position: 2 }, 0, 0.0);
        send.set_position(1);
        send.inc(-2.0);
        let mut pad = Parameter::new(ctx, ParameterKind::DeviceChannelPan(Processor::Drum, "pad"), 36, 0.5);
        pad.inc(4.0);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::DoubleArg("device".into(), "param/9/value/inc".into(), delta / 128.0),
                BackendCall::DoubleArg("track".into(), "2/send/1/volume/inc".into(), -2.0 / 128.0),
                BackendCall::DoubleArg("drum".into(), "pad/36/pan/inc".into(), 4.0 / 128.0),
            ]
        );
        // Not changed locally (the value of the Reaper change): Bitwig reports the result
        assert_eq!(volume.value(), 65);
    }

    /// ParameterImpl.resetValue: `parameter.reset ()` on Bitwig.
    #[test]
    fn reset_on_bitwig() {
        let (ctx, backend) = test_ctx();
        ctx.set_host_paging();
        let mut volume = Parameter::new(ctx.clone(), ParameterKind::TrackVolume, 3, 0.716);
        volume.reset_value();
        let mut param = Parameter::new(ctx.clone(), ParameterKind::Device(Processor::Eq), 0, 0.0);
        param.set_position(4);
        param.reset_value();
        let mut send = Parameter::new(ctx.clone(), ParameterKind::Send { channel_position: 2 }, 0, 0.0);
        send.set_position(1);
        send.reset_value();
        let mut master = Parameter::new(ctx, ParameterKind::MasterPan, 0, 0.5);
        master.reset_value();
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("track".into(), "3/volume/reset".into()),
                BackendCall::NoArg("eq".into(), "param/4/value/reset".into()),
                BackendCall::NoArg("track".into(), "2/send/1/volume/reset".into()),
                BackendCall::NoArg("master".into(), "pan/reset".into()),
            ]
        );
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
