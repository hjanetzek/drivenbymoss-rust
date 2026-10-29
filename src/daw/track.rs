// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A track (former `ITrack` / `TrackImpl`) and the master track (former `IMasterTrack` /
//! `MasterTrackImpl`). A track derefs (mutably) to its [`Channel`] for all mixer functions
//! (volume, pan, mute, solo, sends, color, ...).
//!
//! Functions which need other tracks are on the track bank: `TrackBank::is_muted_by_solo`,
//! `TrackBank::enter` (enter a group), `TrackBank::has_parent`.

use std::ops::{Deref, DerefMut};

use crate::backend::Processor;

use super::bank::BankItem;
use super::channel::{Channel, ChannelKind};
use super::constants::{ChannelType, RecordQuantization};
use super::context::{DawContext, MASTER_POSITION};
use super::events::ParameterBankId;
use super::parameter::{Parameter, ParameterKind};
use super::parameter_bank::ParameterBank;
use super::slot::SlotBank;

/// A track or the master track.
pub struct Track {
    channel: Channel,
    is_group_expanded: bool,
    /// True if no clip of the track plays (reported by hosts with a clip launcher).
    is_stopped: bool,
    is_rec_arm: bool,
    monitor: bool,
    auto_monitor: bool,
    slot_bank: SlotBank,
    depth: i32,
    record_quantization_note_length: bool,
    record_quantization: RecordQuantization,
    is_overdub: bool,
    parameter_bank: Option<ParameterBank>,
    crossfader: Option<Parameter>,
}

impl Track {
    /// Create a track at the given position.
    pub fn new(ctx: DawContext, index: i32, position: i32, num_sends: usize, num_scenes: usize, num_params: usize) -> Self {
        let channel = Channel::new(ctx.clone(), ChannelKind::Track, index, position, num_sends);
        Self::with_channel(ctx, channel, index, num_scenes, num_params)
    }

    fn with_channel(ctx: DawContext, channel: Channel, index: i32, num_scenes: usize, num_params: usize) -> Self {
        Track {
            channel,
            is_group_expanded: true,
            is_stopped: true,
            is_rec_arm: false,
            monitor: false,
            auto_monitor: false,
            slot_bank: SlotBank::new(ctx.clone(), num_scenes, index),
            depth: 0,
            record_quantization_note_length: false,
            record_quantization: RecordQuantization::ResOff,
            is_overdub: false,
            parameter_bank: if num_params > 0 {
                Some(ParameterBank::new(ctx, ParameterBankId::CursorTrack, Processor::Track, num_params, false))
            } else {
                None
            },
            crossfader: None,
        }
    }

    /// Create the master track.
    pub fn new_master(ctx: DawContext, num_sends: usize, num_params: usize) -> Self {
        let channel = Channel::new(ctx.clone(), ChannelKind::Master, 0, 0, num_sends);
        let mut track = Self::with_channel(ctx.clone(), channel, 0, 0, num_params);
        track.crossfader = Some(Parameter::new(ctx, ParameterKind::MasterCrossfader, 0, 0.0));
        track.channel.set_exists(true);
        track.channel.set_internal_name("Master");
        track.channel.set_type(ChannelType::Master);
        track
    }

    /// Create a placeholder for a non-existing track.
    pub fn empty(ctx: &DawContext, num_sends: usize) -> Self {
        let channel = Channel::empty(ctx, num_sends);
        Self::with_channel(ctx.muted(), channel, -1, 0, 0)
    }

    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    pub fn channel_mut(&mut self) -> &mut Channel {
        &mut self.channel
    }

    /// True if this is the master track.
    pub fn is_master(&self) -> bool {
        self.channel.kind == ChannelKind::Master
    }

    fn id(&self) -> i32 {
        if self.is_master() {
            MASTER_POSITION
        } else {
            self.channel.position
        }
    }

    /// Is the track selected? If the cursor track is pinned, only the pinned track is selected.
    pub fn is_selected(&self) -> bool {
        if let Some(pinned) = self.channel.ctx.pinned_track() {
            return self.channel.kind != ChannelKind::Empty && pinned == self.id();
        }
        self.channel.is_selected()
    }

    /// Select the track.
    pub fn select(&mut self) {
        self.channel.select_channel();
    }

    /// Select the track. If it is already selected and it is a group, toggle the expanded
    /// state.
    pub fn select_or_expand_group(&mut self) {
        if !self.does_exist() {
            return;
        }
        if !self.is_selected() {
            self.select();
        } else if self.is_group() {
            self.toggle_group_expanded();
        }
    }

    /// Add the track to the multi-selection or remove it.
    pub fn toggle_multi_select(&mut self) {
        if self.does_exist() && !self.is_master() {
            self.channel.send_int("toggleMultiSelect", 1);
        }
    }

    /// Rename the track.
    pub fn set_name(&mut self, name: &str) {
        self.channel.send_str("name", name);
    }

    /// The crossfader parameter of the track (not supported in Reaper, does not exist).
    pub fn crossfade_parameter(&self) -> Option<&Parameter> {
        None
    }

    /// The crossfader of the master track (user parameter 0 of the master track).
    pub fn crossfader_parameter(&self) -> Option<&Parameter> {
        self.crossfader.as_ref()
    }

    pub fn crossfader_parameter_mut(&mut self) -> Option<&mut Parameter> {
        self.crossfader.as_mut()
    }

    pub fn is_group(&self) -> bool {
        self.channel_type() == ChannelType::Group
    }

    pub fn is_group_expanded(&self) -> bool {
        self.is_group_expanded
    }

    pub fn set_group_expanded(&mut self, is_expanded: bool) {
        self.is_group_expanded = is_expanded;
        self.channel.send_bool("isGroupExpanded", is_expanded);
    }

    pub fn toggle_group_expanded(&mut self) {
        let expanded = self.is_group_expanded;
        self.set_group_expanded(!expanded);
    }

    pub fn is_rec_arm(&self) -> bool {
        self.does_exist() && self.is_rec_arm
    }

    pub fn set_rec_arm(&mut self, value: bool) {
        if self.is_master() {
            return;
        }
        self.set_rec_arm_state(value);
        self.channel.send_bool("recarm", value);
    }

    pub fn toggle_rec_arm(&mut self) {
        let v = !self.is_rec_arm();
        self.set_rec_arm(v);
    }

    pub fn is_monitor(&self) -> bool {
        self.does_exist() && self.monitor
    }

    pub fn set_monitor(&mut self, value: bool) {
        if self.is_master() {
            return;
        }
        self.monitor = value;
        self.channel.send_int("monitor", i32::from(value));
    }

    pub fn toggle_monitor(&mut self) {
        let v = !self.monitor;
        self.set_monitor(v);
    }

    pub fn is_auto_monitor(&self) -> bool {
        self.does_exist() && self.auto_monitor
    }

    pub fn set_auto_monitor(&mut self, value: bool) {
        if self.is_master() {
            return;
        }
        self.auto_monitor = value;
        self.channel.send_bool("autoMonitor", value);
    }

    pub fn toggle_auto_monitor(&mut self) {
        let v = !self.auto_monitor;
        self.set_auto_monitor(v);
    }

    /// All Reaper tracks can hold notes (false for empty tracks).
    pub fn can_hold_notes(&self) -> bool {
        self.channel.kind != ChannelKind::Empty
    }

    /// All Reaper tracks can hold audio (false for empty tracks).
    pub fn can_hold_audio_data(&self) -> bool {
        self.channel.kind != ChannelKind::Empty
    }

    /// Not supported.
    /// True if a clip of the track plays (only reported by hosts with a clip launcher).
    pub fn is_playing(&self) -> bool {
        !self.is_stopped
    }

    /// Stop the playing clip of the track (not supported by Reaper).
    pub fn stop(&mut self, is_alternative: bool) {
        self.channel.send_cmd(if is_alternative { "stopAlt" } else { "stop" });
    }

    /// Switch the track back to the arranger (not supported by Reaper).
    pub fn return_to_arrangement(&mut self) {
        self.channel.send_cmd("returnToArrangement");
    }

    pub fn set_stopped_state(&mut self, is_stopped: bool) {
        self.is_stopped = is_stopped;
    }

    pub fn is_record_quantization_note_length(&self) -> bool {
        self.record_quantization_note_length
    }

    /// Not supported.
    pub fn toggle_record_quantization_note_length(&mut self) {}

    pub fn record_quantization_grid(&self) -> RecordQuantization {
        self.record_quantization
    }

    pub fn set_record_quantization_grid(&mut self, rq: RecordQuantization) {
        self.record_quantization = rq;
        self.channel.send_double("inQuantResolution", rq.beats());
    }

    pub fn previous_record_quantization(&mut self) {
        let all = RecordQuantization::ALL;
        let pos = all.iter().position(|r| *r == self.record_quantization).unwrap_or(0);
        let pos = if pos == 0 { all.len() - 1 } else { pos - 1 };
        self.set_record_quantization_grid(all[pos]);
    }

    pub fn next_record_quantization(&mut self) {
        let all = RecordQuantization::ALL;
        let pos = all.iter().position(|r| *r == self.record_quantization).unwrap_or(0) + 1;
        self.set_record_quantization_grid(all[if pos >= all.len() { 0 } else { pos }]);
    }

    pub fn slot_bank(&self) -> &SlotBank {
        &self.slot_bank
    }

    pub fn slot_bank_mut(&mut self) -> &mut SlotBank {
        &mut self.slot_bank
    }

    /// Create a new clip with the given length (the slot index is ignored in Reaper).
    /// Create a new clip in the slot with the given absolute position (Reaper ignores the slot and
    /// creates the clip at the edit cursor).
    pub fn create_clip(&mut self, slot_position: i32, length_in_beats: i32) {
        self.channel.send_int(&format!("createClip/{slot_position}"), length_in_beats);
    }

    /// Record a new clip.
    pub fn record_clip(&mut self) {
        self.channel.send_cmd("recordClip");
    }

    /// Not supported.
    pub fn has_drum_device(&self) -> bool {
        false
    }

    /// The FX parameter bank of the track (None if no parameters are configured).
    pub fn parameter_bank(&self) -> Option<&ParameterBank> {
        self.parameter_bank.as_ref()
    }

    pub fn parameter_bank_mut(&mut self) -> Option<&mut ParameterBank> {
        self.parameter_bank.as_mut()
    }

    pub fn depth(&self) -> i32 {
        self.depth
    }

    pub fn is_overdub(&self) -> bool {
        self.is_overdub
    }

    /// Move the track one position up.
    pub fn swap_with_previous(&mut self) {
        self.channel.send_cmd("movePrev");
    }

    /// Move the track one position down.
    pub fn swap_with_next(&mut self) {
        self.channel.send_cmd("moveNext");
    }

    pub fn enable_observers(&self, enable: bool) {
        if self.is_master() {
            self.channel.ctx.enable_updates(Processor::Master, enable);
        }
    }

    // Internal setters

    pub fn set_is_group_expanded(&mut self, expanded: bool) {
        self.is_group_expanded = expanded;
    }

    pub fn set_rec_arm_state(&mut self, is_rec_arm: bool) {
        self.is_rec_arm = is_rec_arm;
        self.slot_bank.set_track_info(self.channel.index, self.is_rec_arm);
    }

    pub fn set_monitor_state(&mut self, monitor: bool) {
        self.monitor = monitor;
    }

    pub fn set_auto_monitor_state(&mut self, auto_monitor: bool) {
        self.auto_monitor = auto_monitor;
    }

    pub fn set_depth(&mut self, depth: i32) {
        self.depth = depth;
    }

    pub fn set_overdub(&mut self, overdub: bool) {
        self.is_overdub = overdub;
    }

    pub fn set_record_quantization_note_length_state(&mut self, enabled: bool) {
        self.record_quantization_note_length = enabled;
    }

    /// Set the record quantization from the resolution in beats received from Reaper.
    pub fn set_record_quantization_grid_value(&mut self, resolution: f64) {
        let mut diff = 1.0;
        let mut result = RecordQuantization::ResOff;
        for rq in RecordQuantization::ALL {
            let new_diff = (rq.beats() - resolution).abs();
            if new_diff < diff {
                result = *rq;
                diff = new_diff;
            }
        }
        self.record_quantization = result;
    }

    pub(crate) fn set_index(&mut self, index: i32) {
        self.channel.set_index(index);
        self.slot_bank.set_track_info(index, self.is_rec_arm);
    }

    pub(crate) fn set_master_position(&mut self, position: i32) {
        self.channel.position = position;
    }
}

impl Deref for Track {
    type Target = Channel;
    fn deref(&self) -> &Channel {
        &self.channel
    }
}

impl DerefMut for Track {
    fn deref_mut(&mut self) -> &mut Channel {
        &mut self.channel
    }
}

impl BankItem for Track {
    fn does_exist(&self) -> bool {
        self.channel.exists
    }

    fn is_selected(&self) -> bool {
        Track::is_selected(self)
    }

    fn index(&self) -> i32 {
        self.channel.index
    }

    fn position(&self) -> i32 {
        self.channel.position
    }

    fn select(&mut self) {
        Track::select(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendCall;
    use crate::daw::bank::tests::test_ctx;

    #[test]
    fn commands() {
        let (ctx, backend) = test_ctx();
        let mut track = Track::new(ctx.clone(), 2, 10, 4, 8, 8);
        track.select();
        assert!(backend.take_calls().is_empty());
        track.set_exists(true);
        track.select();
        track.toggle_mute();
        track.set_rec_arm(true);
        track.set_color(crate::daw::color::ColorEx::RED);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::IntArg("track".into(), "10/select".into(), 1),
                BackendCall::IntArg("track".into(), "10/mute".into(), 1),
                BackendCall::IntArg("track".into(), "10/recarm".into(), 1),
                BackendCall::StringArg("track".into(), "10/color".into(), "RGB(255,0,0)".into()),
            ]
        );
        assert!(track.is_mute());
        let mut master = Track::new_master(ctx.clone(), 0, 8);
        master.set_volume(127);
        master.set_rec_arm(true);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("master".into(), "volume".into(), 1.0)]);
        let mut empty = Track::empty(&ctx, 8);
        empty.toggle_mute();
        empty.set_volume(3);
        assert!(backend.take_calls().is_empty());
        track.set_record_quantization_grid_value(0.26);
        assert_eq!(track.record_quantization_grid(), RecordQuantization::Res1_16);
    }
}
