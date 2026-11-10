// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The tracks of the current track bank, the master track, the cursor track and its remote
//! controls (former `TrackModule`).
//!
//! Differences to Java: the model has no crossfade mode for tracks (the A/B/AB parameter, see
//! `framework::commands::channel::CrossfadeModeCommand`): `crossfadeMode/{A,B,AB}` reports AB
//! and setting it does nothing.

use super::project::{flush_parameter_bank, parse_page_value, parse_param_value};
use super::{
    first, flush_send_data, index_of, is_trigger, match_color, parse_int, parse_int_or_throw, send_color, sub_command, to_integer, to_string, OscError, OscModule,
    OscResult, OscShared, Path, Value,
};
use super::{
    TAG_ACTIVATED, TAG_COLOR, TAG_DUPLICATE, TAG_EXISTS, TAG_INDICATE, TAG_INSERT_FILE, TAG_NAME, TAG_PAGE, TAG_REMOVE, TAG_RESET, TAG_SELECT, TAG_SELECTED,
    TAG_TOUCHED, TAG_VOLUME,
};
use crate::daw::bank::BankItem;
use crate::daw::constants::{ChannelType, RecordQuantization};
use crate::daw::events::ParameterBankId;
use crate::daw::track::Track;
use crate::daw::track_bank::TrackRef;
use crate::framework::ctx::Ctx;
use crate::framework::osc::OscWriter;

/// The track which a command addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// The track at the index of the page of the current track bank.
    Bank(usize),
    /// The master track.
    Master,
    /// The cursor (selected or pinned) track.
    Cursor,
}

fn track_mut(ctx: &mut Ctx, target: Target) -> &mut Track {
    match target {
        Target::Bank(index) => ctx.model.current_track_bank_mut().item_mut(index),
        Target::Master => ctx.model.master_track_mut(),
        Target::Cursor => ctx.model.cursor_track_mut(),
    }
}

fn track(ctx: &Ctx, target: Target) -> &Track {
    match target {
        Target::Bank(index) => ctx.model.current_track_bank().item(index),
        Target::Master => ctx.model.master_track(),
        Target::Cursor => ctx.model.cursor_track(),
    }
}

/// The reference of the track for the clip functions of the model (only for the instrument and
/// audio track bank, the effect tracks have no clips).
fn track_ref(ctx: &Ctx, target: Target) -> Option<TrackRef> {
    match target {
        Target::Bank(index) if !ctx.model.is_effect_track_bank_active() => ctx.model.current_track_bank().ref_at(index),
        Target::Bank(_) => None,
        Target::Master => Some(TrackRef::Master),
        Target::Cursor => ctx.model.cursor_track_ref(),
    }
}

/// Former `TrackModule`.
pub struct TrackModule {
    /// The size of a page of the parameter banks, for the empty pages if there is no bank.
    page_size: usize,
}

impl TrackModule {
    pub fn new(page_size: usize) -> Self {
        TrackModule { page_size }
    }
}

impl OscModule for TrackModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["track", "master"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        match command {
            "track" => {
                let sub = sub_command(path)?;
                match parse_int(&sub) {
                    Some(number) => parse_track_value(ctx, Target::Bank(index_of(number)?), path, value),
                    None => parse_track_commands(ctx, &sub, path, value),
                }
            }
            "master" => parse_track_value(ctx, Target::Master, path, value),
            _ => Err(OscError::UnknownCommand(command.to_string())),
        }
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let track_bank = ctx.model.current_track_bank();
        for i in 0..track_bank.page_size() {
            flush_track(ctx, w, &format!("/track/{}/", i + 1), Target::Bank(i), dump);
        }
        flush_track(ctx, w, "/master/", Target::Master, dump);
        flush_track(ctx, w, "/track/selected/", Target::Cursor, dump);

        w.send_int("/track/toggleBank", i32::from(ctx.model.is_effect_track_bank_active()), dump);
        w.send_bool("/track/hasParent", track_bank.has_parent(), dump);

        // Flush track parameters
        flush_parameter_bank(w, "/track/", ctx.model.cursor_track_parameter_bank(), self.page_size, &ctx.model, dump);
    }
}

/// Send the values of a track (former `flushTrack`).
fn flush_track(ctx: &Ctx, w: &mut OscWriter, address: &str, target: Target, dump: bool) {
    let track = track(ctx, target);
    let a = |tag: &str| format!("{address}{tag}");
    w.send_bool(&a(TAG_EXISTS), track.does_exist(), dump);
    w.send_str(&a("type"), &track.channel_type().name().to_lowercase(), dump);
    w.send_bool(&a(TAG_ACTIVATED), track.is_activated(), dump);
    w.send_bool(&a(TAG_SELECTED), track.is_selected(), dump);
    w.send_bool(&a("isGroup"), track.is_group(), dump);
    w.send_str(&a(TAG_NAME), track.name(), dump);
    w.send_str(&a("volumeStr"), &track.volume_str(), dump);
    w.send_int(&a(TAG_VOLUME), track.volume(), dump);
    w.send_str(&a("panStr"), &track.pan_str(), dump);
    w.send_int(&a("pan"), track.pan(), dump);
    w.send_bool(&a("mute"), track.is_mute(), dump);
    w.send_bool(&a("solo"), track.is_solo(), dump);
    w.send_bool(&a("recarm"), track.is_rec_arm(), dump);
    w.send_bool(&a("monitor"), track.is_monitor(), dump);
    w.send_bool(&a("autoMonitor"), track.is_auto_monitor(), dump);
    w.send_bool(&a("canHoldNotes"), track.can_hold_notes(), dump);
    w.send_bool(&a("canHoldAudioData"), track.can_hold_audio_data(), dump);
    w.send_int(&a("position"), track.position(), dump);
    if target == Target::Cursor {
        w.send_bool(&a("pinned"), ctx.model.is_cursor_track_pinned(), dump);
    }

    // The master track of Bitwig has no sends and no clip slots
    if target != Target::Master {
        let send_bank = track.send_bank();
        for i in 0..send_bank.page_size() {
            flush_send_data(w, &format!("{address}send/{}/", i + 1), send_bank.item(i), dump);
        }

        let slot_bank = track.slot_bank();
        for i in 0..slot_bank.page_size() {
            let slot = slot_bank.item(i);
            let clip = format!("{address}clip/{}/", i + 1);
            let c = |tag: &str| format!("{clip}{tag}");
            w.send_str(&c(TAG_NAME), slot.name(), dump);
            w.send_bool(&c("isSelected"), slot.is_selected(), dump);
            w.send_bool(&c("hasContent"), slot.has_content(), dump);
            w.send_bool(&c("isPlaying"), slot.is_playing(), dump);
            w.send_bool(&c("isRecording"), slot.is_recording(), dump);
            w.send_bool(&c("isPlayingQueued"), slot.is_playing_queued(), dump);
            w.send_bool(&c("isRecordingQueued"), slot.is_recording_queued(), dump);
            w.send_bool(&c("isStopQueued"), slot.is_stop_queued(), dump);
            send_color(w, &c(TAG_COLOR), slot.color(), dump);
        }
    }

    send_color(w, &a(TAG_COLOR), track.color(), dump);

    let crossfade_mode = track.crossfade_parameter().map_or_else(|| "AB".to_string(), |p| p.displayed_value());
    w.send_bool(&a("crossfadeMode/A"), crossfade_mode == "A", dump);
    w.send_bool(&a("crossfadeMode/B"), crossfade_mode == "B", dump);
    w.send_bool(&a("crossfadeMode/AB"), crossfade_mode == "AB", dump);
    w.send_str(&a("recordQuantization"), track.record_quantization_grid().value(), dump);
    w.send_int(&a("vu"), if ctx.config.is_enable_vu_meters() { track.vu() } else { 0 }, dump);
}

/// The commands of `/track/<command>` which do not address a track by its number (former
/// `parseTrackCommands`).
fn parse_track_commands(ctx: &mut Ctx, command: &str, path: &mut Path, value: Value) -> OscResult {
    match command {
        TAG_INDICATE => {
            let indicate_command = sub_command(path)?;
            let is_true = is_trigger(value);
            let tb = ctx.model.current_track_bank_mut();
            let page_size = tb.page_size();
            match indicate_command.as_str() {
                TAG_VOLUME => (0..page_size).for_each(|i| tb.item_mut(i).set_volume_indication(is_true)),
                "pan" => (0..page_size).for_each(|i| tb.item_mut(i).set_pan_indication(is_true)),
                "send" => {
                    let send_index = index_of(parse_int_or_throw(first(path)?)?)?;
                    for i in 0..page_size {
                        tb.item_mut(i).send_bank_mut().item_mut(send_index).set_indication(is_true);
                    }
                }
                _ => return Err(OscError::UnknownCommand(indicate_command)),
            }
        }
        "bank" => {
            let bank_command = sub_command(path)?;
            match bank_command.as_str() {
                TAG_PAGE => {
                    let direction = sub_command(path)?;
                    let tb = ctx.model.current_track_bank_mut();
                    if direction == "+" {
                        tb.select_next_page();
                    } else {
                        tb.select_previous_page();
                    }
                }
                "+" => ctx.model.current_track_bank_mut().scroll_forwards(),
                "-" => ctx.model.current_track_bank_mut().scroll_backwards(),
                _ => return Err(OscError::UnknownCommand(bank_command)),
            }
        }
        "+" => {
            let tb = ctx.model.current_track_bank_mut();
            let index = tb.selected_item().map_or(0, |t| t.index() + 1);
            if index == tb.page_size() as i32 {
                tb.select_next_page();
                return Ok(());
            }
            tb.item_mut(usize::try_from(index).unwrap_or(0)).select();
        }
        "-" => {
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
        "add" => {
            let sub = sub_command(path)?;
            match sub.as_str() {
                "audio" => ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]),
                "effect" => ctx.model.application_mut().add_effect_track(),
                "instrument" => ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]),
                _ => return Err(OscError::UnknownCommand(sub)),
            }
        }
        "stop" | "stopAlt" => ctx.model.current_track_bank_mut().stop(command == "stopAlt"),
        "vu" => ctx.config.set_vu_meters_enabled(is_trigger(value)),
        "toggleBank" => {
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
        "parent" => {
            if !ctx.config.is_track_navigation_flat() {
                ctx.model.current_track_bank_mut().select_parent();
            }
        }
        TAG_SELECT | TAG_SELECTED => {
            if ctx.model.cursor_track().does_exist() {
                parse_track_value(ctx, Target::Cursor, path, value)?;
            }
        }
        "param" => parse_param_value(ctx, ParameterBankId::CursorTrack, path, value)?,
        "page" => parse_page_value(ctx, ParameterBankId::CursorTrack, path, value)?,
        _ => return Err(OscError::UnknownCommand(command.to_string())),
    }
    Ok(())
}

/// The commands of a track (former `parseTrackValue`).
fn parse_track_value(ctx: &mut Ctx, target: Target, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    match command.as_str() {
        TAG_NAME => {
            if let Some(name) = value.text() {
                track_mut(ctx, target).set_name(&name);
            }
        }
        TAG_ACTIVATED => track_mut(ctx, target).set_is_activated(is_trigger(value)),
        "crossfadeMode" => {
            // A: 0, B: 1, otherwise AB: 0.5. The model has no crossfade mode parameter for
            // tracks (see the module documentation): nothing to set
            sub_command(path)?;
        }
        "recordQuantization" => {
            let text = value.text().ok_or_else(|| OscError::Runtime("Cannot invoke \"Object.toString()\" because \"value\" is null".into()))?;
            track_mut(ctx, target).set_record_quantization_grid(RecordQuantization::lookup(&text));
        }
        TAG_SELECT | TAG_SELECTED => {
            if is_trigger(value) {
                track_mut(ctx, target).select_or_expand_group();
            }
        }
        TAG_DUPLICATE => track_mut(ctx, target).duplicate(),
        TAG_REMOVE => track_mut(ctx, target).remove(),
        TAG_VOLUME => {
            if path.is_empty() {
                let volume = to_integer(value)?;
                track_mut(ctx, target).set_volume(volume);
            } else {
                match first(path)? {
                    TAG_INDICATE => track_mut(ctx, target).set_volume_indication(is_trigger(value)),
                    TAG_RESET => track_mut(ctx, target).reset_volume(),
                    TAG_TOUCHED => track_mut(ctx, target).touch_volume(is_trigger(value)),
                    _ => {}
                }
            }
        }
        "pan" => {
            if path.is_empty() {
                let pan = to_integer(value)?;
                track_mut(ctx, target).set_pan(pan);
            } else {
                match first(path)? {
                    TAG_INDICATE => track_mut(ctx, target).set_pan_indication(is_trigger(value)),
                    TAG_RESET => track_mut(ctx, target).reset_pan(),
                    TAG_TOUCHED => track_mut(ctx, target).touch_pan(is_trigger(value)),
                    _ => {}
                }
            }
        }
        "mute" => {
            let track = track_mut(ctx, target);
            if value.is_null() {
                track.toggle_mute();
            } else {
                track.set_mute(is_trigger(value));
            }
        }
        "solo" => {
            let track = track_mut(ctx, target);
            if value.is_null() {
                track.toggle_solo();
            } else {
                track.set_solo(is_trigger(value));
            }
        }
        "recarm" => {
            let track = track_mut(ctx, target);
            if value.is_null() {
                track.toggle_rec_arm();
            } else {
                track.set_rec_arm(is_trigger(value));
            }
        }
        "monitor" => {
            let track = track_mut(ctx, target);
            if value.is_null() {
                track.toggle_monitor();
            } else {
                track.set_monitor(is_trigger(value));
            }
        }
        "autoMonitor" => {
            let track = track_mut(ctx, target);
            if value.is_null() {
                track.toggle_auto_monitor();
            } else {
                track.set_auto_monitor(is_trigger(value));
            }
        }
        "send" => {
            // Java: Integer.parseInt (path.removeFirst ()), both throw
            let number = path.pop_front().ok_or_else(|| OscError::Runtime("NoSuchElementException".into()))?;
            let send_index = index_of(parse_int_or_throw(&number)?)?;
            parse_send_value(ctx, target, send_index, path, value)?;
        }
        "clip" => parse_clip_value(ctx, target, path, value)?,
        "enter" => {
            if is_trigger(value) && track(ctx, target).is_group() {
                if ctx.config.is_track_navigation_flat() {
                    track_mut(ctx, target).toggle_group_expanded();
                } else {
                    track_mut(ctx, target).set_group_expanded(true);
                    let index = match target {
                        Target::Bank(index) => Some(index),
                        Target::Cursor => ctx.model.current_track_bank().selected_index(),
                        Target::Master => None,
                    };
                    if let Some(index) = index {
                        ctx.model.current_track_bank_mut().enter(index);
                    }
                }
            }
        }
        TAG_COLOR => {
            if let Some(color) = match_color(&to_string(value)?) {
                track_mut(ctx, target).set_color(color);
            }
        }
        "pinned" => {
            if target == Target::Cursor {
                if value.is_null() {
                    ctx.model.toggle_cursor_track_pinned();
                } else {
                    ctx.model.set_cursor_track_pinned(is_trigger(value));
                }
            }
        }
        _ => return Err(OscError::UnknownCommand(command)),
    }
    Ok(())
}

/// The commands of a clip slot of a track (former `parseClipValue`).
fn parse_clip_value(ctx: &mut Ctx, target: Target, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    let Some(number) = parse_int(&command) else {
        match command.as_str() {
            "stop" => track_mut(ctx, target).stop(false),
            "returntoarrangement" => track_mut(ctx, target).return_to_arrangement(),
            _ => return Err(OscError::UnknownCommand(command)),
        }
        return Ok(());
    };
    let slot_index = index_of(number)?;
    let clip_command = sub_command(path)?;
    match clip_command.as_str() {
        TAG_SELECT | TAG_SELECTED => track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).select(),
        "launch" | "launchAlt" => {
            let pressed = to_integer(value)? > 0;
            track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).launch(pressed, clip_command == "launchAlt");
        }
        "record" => {
            if let Some(track) = track_ref(ctx, target) {
                ctx.model.record_note_clip(track, slot_index);
            }
        }
        "create" => {
            let length = to_integer(value)?;
            if let Some(track) = track_ref(ctx, target) {
                ctx.model.create_note_clip(track, slot_index, length, true);
            }
        }
        TAG_INSERT_FILE => {
            let file = to_string(value)?;
            track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).insert_file(&file);
        }
        TAG_DUPLICATE => track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).duplicate(),
        TAG_REMOVE => track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).remove(),
        TAG_COLOR => {
            if let Some(color) = match_color(&to_string(value)?) {
                track_mut(ctx, target).slot_bank_mut().item_mut(slot_index).set_color(color);
            }
        }
        _ => return Err(OscError::UnknownCommand(clip_command)),
    }
    Ok(())
}

/// The commands of a send of a track (former `parseSendValue`).
fn parse_send_value(ctx: &mut Ctx, target: Target, send_index: usize, path: &mut Path, value: Value) -> OscResult {
    let command = sub_command(path)?;
    let send = track_mut(ctx, target).send_bank_mut().item_mut(send_index);
    match command.as_str() {
        TAG_ACTIVATED => {
            if is_trigger(value) {
                send.toggle_enabled();
            }
        }
        TAG_VOLUME => {
            if path.is_empty() {
                send.set_value(to_integer(value)?);
            } else {
                match first(path)? {
                    TAG_INDICATE => send.set_indication(is_trigger(value)),
                    TAG_RESET => send.reset_value(),
                    TAG_TOUCHED => send.touch_value(is_trigger(value)),
                    _ => {}
                }
            }
        }
        _ => return Err(OscError::UnknownCommand(command)),
    }
    Ok(())
}
