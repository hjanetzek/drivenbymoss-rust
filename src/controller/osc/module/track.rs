// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The tracks of the current track bank, the master track, the cursor track and its remote
//! controls (former `TrackModule`).
//!
//! Differences to Java: the model has no crossfade mode for tracks (the A/B/AB parameter, see
//! `framework::commands::channel::CrossfadeModeCommand`): `crossfadeMode/{A,B,AB}` reports AB
//! and setting it does nothing.

use super::super::address::{AddTrack, Dir, FaderOp, Indicate, ParamBankKeys, SlotCmd, TrackFlag, TrackIn, TrackKeys, TrackRef};
use super::project::flush_parameter_bank;
use super::{flush_send_data, is_trigger, match_color, text_or_throw, to_integer, to_string, toggle_or_set, OscError, OscResult, OscShared, Value};
use crate::daw::bank::BankItem;
use crate::daw::constants::{ChannelType, RecordQuantization};
use crate::daw::track::Track;
use crate::daw::track_bank::TrackRef as ModelTrackRef;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::{Key, KeyGroup, KeyList, OscWriter};

fn track_mut(ctx: &mut Ctx, target: TrackRef) -> &mut Track {
    match target {
        TrackRef::Bank(index) => ctx.model.current_track_bank_mut().item_mut(index),
        TrackRef::Master => ctx.model.master_track_mut(),
        TrackRef::Cursor => ctx.model.cursor_track_mut(),
    }
}

fn track(ctx: &Ctx, target: TrackRef) -> &Track {
    match target {
        TrackRef::Bank(index) => ctx.model.current_track_bank().item(index),
        TrackRef::Master => ctx.model.master_track(),
        TrackRef::Cursor => ctx.model.cursor_track(),
    }
}

/// The reference of the track for the clip functions of the model (only for the instrument and
/// audio track bank, the effect tracks have no clips).
fn track_ref(ctx: &Ctx, target: TrackRef) -> Option<ModelTrackRef> {
    match target {
        TrackRef::Bank(index) if !ctx.model.is_effect_track_bank_active() => ctx.model.current_track_bank().ref_at(index),
        TrackRef::Bank(_) => None,
        TrackRef::Master => Some(ModelTrackRef::Master),
        TrackRef::Cursor => ctx.model.cursor_track_ref(),
    }
}

/// Former `TrackModule`.
pub struct TrackModule {
    /// The size of a page of the parameter banks, for the empty pages if there is no bank.
    page_size: usize,
    tracks: KeyList<TrackKeys>,
    master: TrackKeys,
    cursor: TrackKeys,
    toggle_bank: Key<i32>,
    has_parent: Key<bool>,
    params: ParamBankKeys,
}

impl TrackModule {
    pub fn new(w: &mut OscWriter, page_size: usize) -> Self {
        TrackModule {
            page_size,
            tracks: KeyList::new("/track/".into()),
            master: TrackKeys::new(w, "/master/"),
            cursor: TrackKeys::new(w, "/track/selected/"),
            toggle_bank: w.key("/track/toggleBank"),
            has_parent: w.key("/track/hasParent"),
            params: ParamBankKeys::new(w, "/track/"),
        }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: TrackIn, value: Value) -> OscResult {
        match command {
            TrackIn::Name { track } => {
                if let Some(name) = value.text() {
                    track_mut(ctx, track).set_name(&name);
                }
            }
            TrackIn::Activated { track } => track_mut(ctx, track).set_is_activated(is_trigger(value)),
            // A: 0, B: 1, otherwise AB: 0.5. The model has no crossfade mode parameter for
            // tracks (see the module documentation): nothing to set
            TrackIn::CrossfadeMode { .. } => {}
            TrackIn::RecordQuantization { track } => {
                let text = text_or_throw(value)?;
                track_mut(ctx, track).set_record_quantization_grid(RecordQuantization::lookup(&text));
            }
            TrackIn::Select { track } => {
                if is_trigger(value) {
                    track_mut(ctx, track).select_or_expand_group();
                }
            }
            TrackIn::Duplicate { track } => track_mut(ctx, track).duplicate(),
            TrackIn::Remove { track } => track_mut(ctx, track).remove(),
            TrackIn::Volume { track, op } => {
                let track = track_mut(ctx, track);
                match op {
                    FaderOp::Set => {
                        let volume = to_integer(value)?;
                        track.set_volume(volume);
                    }
                    FaderOp::Indicate => track.set_volume_indication(is_trigger(value)),
                    FaderOp::Reset => track.reset_volume(),
                    FaderOp::Touched => track.touch_volume(is_trigger(value)),
                    FaderOp::Ignore => {}
                }
            }
            TrackIn::Pan { track, op } => {
                let track = track_mut(ctx, track);
                match op {
                    FaderOp::Set => {
                        let pan = to_integer(value)?;
                        track.set_pan(pan);
                    }
                    FaderOp::Indicate => track.set_pan_indication(is_trigger(value)),
                    FaderOp::Reset => track.reset_pan(),
                    FaderOp::Touched => track.touch_pan(is_trigger(value)),
                    FaderOp::Ignore => {}
                }
            }
            TrackIn::Flag { track, flag } => {
                let track = track_mut(ctx, track);
                match flag {
                    TrackFlag::Mute => toggle_or_set(track, value, |t| t.toggle_mute(), |t, on| t.set_mute(on)),
                    TrackFlag::Solo => toggle_or_set(track, value, |t| t.toggle_solo(), |t, on| t.set_solo(on)),
                    TrackFlag::RecArm => toggle_or_set(track, value, |t| t.toggle_rec_arm(), |t, on| t.set_rec_arm(on)),
                    TrackFlag::Monitor => toggle_or_set(track, value, |t| t.toggle_monitor(), |t, on| t.set_monitor(on)),
                    TrackFlag::AutoMonitor => toggle_or_set(track, value, |t| t.toggle_auto_monitor(), |t, on| t.set_auto_monitor(on)),
                }
            }
            TrackIn::SendActivated { track, send } => {
                if is_trigger(value) {
                    track_mut(ctx, track).send_bank_mut().item_mut(send).toggle_enabled();
                }
            }
            TrackIn::SendVolume { track, send, op } => {
                let send = track_mut(ctx, track).send_bank_mut().item_mut(send);
                match op {
                    FaderOp::Set => send.set_value(to_integer(value)?),
                    FaderOp::Indicate => send.set_indication(is_trigger(value)),
                    FaderOp::Reset => send.reset_value(),
                    FaderOp::Touched => send.touch_value(is_trigger(value)),
                    FaderOp::Ignore => {}
                }
            }
            TrackIn::Slot { track, slot, cmd } => execute_slot(ctx, track, slot, cmd, value)?,
            TrackIn::StopClip { track } => track_mut(ctx, track).stop(false),
            TrackIn::ReturnToArrangement { track } => track_mut(ctx, track).return_to_arrangement(),
            TrackIn::Enter { track: target } => {
                if is_trigger(value) && track(ctx, target).is_group() {
                    if ctx.config.is_track_navigation_flat() {
                        track_mut(ctx, target).toggle_group_expanded();
                    } else {
                        track_mut(ctx, target).set_group_expanded(true);
                        let index = match target {
                            TrackRef::Bank(index) => Some(index),
                            TrackRef::Cursor => ctx.model.current_track_bank().selected_index(),
                            TrackRef::Master => None,
                        };
                        if let Some(index) = index {
                            ctx.model.current_track_bank_mut().enter(index);
                        }
                    }
                }
            }
            TrackIn::Color { track } => {
                if let Some(color) = match_color(&to_string(value)?) {
                    track_mut(ctx, track).set_color(color);
                }
            }
            TrackIn::Pinned { track } => {
                if track == TrackRef::Cursor {
                    if value.is_null() {
                        ctx.model.toggle_cursor_track_pinned();
                    } else {
                        ctx.model.set_cursor_track_pinned(is_trigger(value));
                    }
                }
            }
            TrackIn::Indicate { what } => {
                let is_true = is_trigger(value);
                let tb = ctx.model.current_track_bank_mut();
                let page_size = tb.page_size();
                for i in 0..page_size {
                    let track = tb.item_mut(i);
                    match what {
                        Indicate::Volume => track.set_volume_indication(is_true),
                        Indicate::Pan => track.set_pan_indication(is_true),
                        Indicate::Send(send) => track.send_bank_mut().item_mut(send).set_indication(is_true),
                    }
                }
            }
            TrackIn::BankPage { dir } => {
                let tb = ctx.model.current_track_bank_mut();
                match dir {
                    Dir::Next => tb.select_next_page(),
                    Dir::Previous => tb.select_previous_page(),
                }
            }
            TrackIn::BankScroll { dir: Dir::Next } => ctx.model.current_track_bank_mut().scroll_forwards(),
            TrackIn::BankScroll { dir: Dir::Previous } => ctx.model.current_track_bank_mut().scroll_backwards(),
            TrackIn::SelectAdjacent { dir: Dir::Next } => {
                let tb = ctx.model.current_track_bank_mut();
                let index = tb.selected_item().map_or(0, |t| t.index() + 1);
                if index == tb.page_size() as i32 {
                    tb.select_next_page();
                    return Ok(());
                }
                tb.item_mut(usize::try_from(index).unwrap_or(0)).select();
            }
            TrackIn::SelectAdjacent { dir: Dir::Previous } => {
                let tb = ctx.model.current_track_bank_mut();
                let index = tb.selected_item().map_or(0, |t| t.index() - 1);
                if index == -1 {
                    tb.select_previous_page();
                    return Ok(());
                }
                match usize::try_from(index) {
                    Ok(index) => tb.item_mut(index).select(),
                    Err(_) => return Err(OscError::Runtime(format!("Index {index} out of bounds"))),
                }
            }
            TrackIn::Add { kind } => match kind {
                AddTrack::Audio => ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]),
                AddTrack::Effect => ctx.model.application_mut().add_effect_track(),
                AddTrack::Instrument => ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]),
            },
            TrackIn::Stop { alt } => ctx.model.current_track_bank_mut().stop(alt),
            TrackIn::Vu {} => ctx.config.set_vu_meters_enabled(is_trigger(value)),
            TrackIn::ToggleBank {} => {
                if ctx.model.effect_track_bank().is_none() {
                    return Ok(());
                }
                ctx.model.toggle_current_track_bank();
                // Make sure a track is selected
                if ctx.model.current_track_bank().selected_item().is_none() {
                    ctx.model.current_track_bank_mut().item_mut(0).select();
                }
                // Move the indication to the other bank
                let effect_active = ctx.model.is_effect_track_bank_active();
                let page_size = ctx.model.current_track_bank().page_size();
                for i in 0..page_size {
                    let other = if effect_active { Some(ctx.model.track_bank_mut()) } else { ctx.model.effect_track_bank_mut() };
                    if let Some(other) = other {
                        let other_track = other.item_mut(i);
                        other_track.set_volume_indication(false);
                        other_track.set_pan_indication(false);
                    }
                    let track = ctx.model.current_track_bank_mut().item_mut(i);
                    track.set_volume_indication(true);
                    track.set_pan_indication(true);
                }
            }
            TrackIn::Parent {} => {
                if !ctx.config.is_track_navigation_flat() {
                    ctx.model.current_track_bank_mut().select_parent();
                }
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let track_bank = ctx.model.current_track_bank();
        for i in 0..track_bank.page_size() {
            let keys = self.tracks.at(w, i);
            flush_track(ctx, w, keys, TrackRef::Bank(i), dump);
        }
        flush_track(ctx, w, &mut self.master, TrackRef::Master, dump);
        flush_track(ctx, w, &mut self.cursor, TrackRef::Cursor, dump);

        w.put(self.toggle_bank, i32::from(ctx.model.is_effect_track_bank_active()), dump);
        w.put(self.has_parent, track_bank.has_parent(), dump);

        // Flush track parameters
        flush_parameter_bank(w, &mut self.params, ctx.model.cursor_track_parameter_bank(), self.page_size, &ctx.model, dump);
    }
}

/// Send the values of a track (former `flushTrack`).
fn flush_track(ctx: &Ctx, w: &mut OscWriter, k: &mut TrackKeys, target: TrackRef, dump: bool) {
    let track = track(ctx, target);
    w.put(k.exists, track.does_exist(), dump);
    w.put(k.kind, &track.channel_type().name().to_lowercase(), dump);
    w.put(k.activated, track.is_activated(), dump);
    w.put(k.selected, track.is_selected(), dump);
    w.put(k.is_group, track.is_group(), dump);
    w.put(k.name, track.name(), dump);
    w.put(k.volume_str, &track.volume_str(), dump);
    w.put(k.volume, track.volume(), dump);
    w.put(k.pan_str, &track.pan_str(), dump);
    w.put(k.pan, track.pan(), dump);
    w.put(k.mute, track.is_mute(), dump);
    w.put(k.solo, track.is_solo(), dump);
    w.put(k.recarm, track.is_rec_arm(), dump);
    w.put(k.monitor, track.is_monitor(), dump);
    w.put(k.auto_monitor, track.is_auto_monitor(), dump);
    w.put(k.can_hold_notes, track.can_hold_notes(), dump);
    w.put(k.can_hold_audio_data, track.can_hold_audio_data(), dump);
    w.put(k.position, track.position(), dump);
    if target == TrackRef::Cursor {
        w.put(k.pinned, ctx.model.is_cursor_track_pinned(), dump);
    }

    // The master track of Bitwig has no sends and no clip slots
    if target != TrackRef::Master {
        let send_bank = track.send_bank();
        for i in 0..send_bank.page_size() {
            let keys = k.sends.at(w, i);
            flush_send_data(w, keys, send_bank.item(i), dump);
        }

        let slot_bank = track.slot_bank();
        for i in 0..slot_bank.page_size() {
            let slot = slot_bank.item(i);
            let c = k.clips.at(w, i);
            w.put(c.name, slot.name(), dump);
            w.put(c.is_selected, slot.is_selected(), dump);
            w.put(c.has_content, slot.has_content(), dump);
            w.put(c.is_playing, slot.is_playing(), dump);
            w.put(c.is_recording, slot.is_recording(), dump);
            w.put(c.is_playing_queued, slot.is_playing_queued(), dump);
            w.put(c.is_recording_queued, slot.is_recording_queued(), dump);
            w.put(c.is_stop_queued, slot.is_stop_queued(), dump);
            w.put(c.color, slot.color(), dump);
        }
    }

    w.put(k.color, track.color(), dump);

    let crossfade_mode = track.crossfade_parameter().map(|p| p.displayed_value());
    let crossfade_mode = crossfade_mode.as_deref().unwrap_or("AB");
    w.put(k.crossfade_a, crossfade_mode == "A", dump);
    w.put(k.crossfade_b, crossfade_mode == "B", dump);
    w.put(k.crossfade_ab, crossfade_mode == "AB", dump);
    w.put(k.record_quantization, track.record_quantization_grid().value(), dump);
    w.put(k.vu, if ctx.config.is_enable_vu_meters() { track.vu() } else { 0 }, dump);
}

/// The commands of a clip slot of a track (former `parseClipValue`).
fn execute_slot(ctx: &mut Ctx, target: TrackRef, slot_index: usize, cmd: SlotCmd, value: Value) -> OscResult {
    match cmd {
        SlotCmd::Select => track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).select(),
        SlotCmd::Launch { alt } => {
            let pressed = to_integer(value)? > 0;
            track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).launch(pressed, alt);
        }
        SlotCmd::Record => {
            if let Some(track) = track_ref(ctx, target) {
                ctx.model.record_note_clip(track, slot_index);
            }
        }
        SlotCmd::Create => {
            let length = to_integer(value)?;
            if let Some(track) = track_ref(ctx, target) {
                ctx.model.create_note_clip(track, slot_index, length, true);
            }
        }
        SlotCmd::InsertFile => {
            let file = to_string(value)?;
            track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).insert_file(&file);
        }
        SlotCmd::Duplicate => track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).duplicate(),
        SlotCmd::Remove => track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).remove(),
        SlotCmd::Color => {
            if let Some(color) = match_color(&to_string(value)?) {
                track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).set_color(color);
            }
        }
    }
    Ok(())
}
