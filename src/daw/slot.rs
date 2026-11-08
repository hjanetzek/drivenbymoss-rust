// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Clip launcher slots of a track (former `ISlot`/`SlotImpl` and `ISlotBank`/`SlotBankImpl`).
//!
//! In Reaper the "clips" of a track are its media items. The page of a slot bank follows the page
//! of the scene bank; scroll the scene bank to change it (see `TrackBank::scene_bank_mut`).
//!
//! Hosts with a clip launcher (Bitwig) report the states of the slots (playing, recording,
//! queued; protocol v2). Without them, the states are derived from the transport and the scene
//! (region) ranges, as for Reaper.

use std::ops::Deref;

use crate::protocol::{Cmd, Rgb, SlotAction, SlotRef};

use super::bank::{BankItem, PagedBank};
use super::color::ColorEx;
use super::context::DawContext;
use super::util;

const DEFAULT_COLOR: ColorEx = ColorEx::new(0.2, 0.2, 0.2);

/// A clip slot.
pub struct Slot {
    ctx: DawContext,
    index: i32,
    position: i32,
    exists: bool,
    selected: bool,
    name: String,
    has_content: bool,
    is_muted: bool,
    color: ColorEx,
    /// The position of the track which contains this slot (SlotImpl acts on the slot of its own
    /// track; the track index of the page addressed the wrong track once the bank was scrolled).
    track_position: i32,
    track_rec_arm: bool,
    /// The states reported by the host, `None` if it does not report them (Reaper).
    states: Option<SlotStates>,
}

/// The launcher states of a slot as reported by the host.
#[derive(Clone, Copy, Default)]
struct SlotStates {
    playing: bool,
    recording: bool,
    play_queued: bool,
    rec_queued: bool,
    stop_queued: bool,
}

impl Slot {
    pub fn new(ctx: DawContext, index: i32, position: i32, track_position: i32) -> Self {
        Slot {
            ctx,
            index,
            position,
            exists: false,
            selected: false,
            name: String::new(),
            has_content: false,
            is_muted: false,
            color: DEFAULT_COLOR,
            track_position,
            track_rec_arm: false,
            states: None,
        }
    }

    /// Create an empty (non-existing) slot.
    pub fn empty(ctx: &DawContext) -> Self {
        let mut slot = Slot::new(ctx.muted(), -1, -1, -1);
        slot.color = ColorEx::BLACK;
        slot
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

    pub fn has_content(&self) -> bool {
        self.has_content
    }

    pub fn is_muted(&self) -> bool {
        self.is_muted
    }

    pub fn color(&self) -> ColorEx {
        self.color
    }

    /// Recording is not queued in Reaper.
    pub fn is_recording_queued(&self) -> bool {
        self.states.is_some_and(|s| s.rec_queued)
    }

    /// The reported state, or: true if the transport records, the track is armed and the play
    /// position is in the range of the scene of this slot.
    pub fn is_recording(&self) -> bool {
        if let Some(states) = self.states {
            return states.recording;
        }
        if !self.exists {
            return false;
        }
        let transport = self.ctx.transport_snapshot();
        if !transport.is_recording || !self.track_rec_arm {
            return false;
        }
        self.is_in_scene_range(transport.position)
    }

    /// Playback or recording is queued (as `SlotImpl.isPlayingQueued` of the Java version).
    pub fn is_playing_queued(&self) -> bool {
        self.states.is_some_and(|s| s.play_queued || s.rec_queued)
    }

    /// The reported state, or: true if the transport plays and the play position is in the range
    /// of the scene of this slot.
    pub fn is_playing(&self) -> bool {
        if let Some(states) = self.states {
            return states.playing;
        }
        if !(self.exists && self.has_content) {
            return false;
        }
        let transport = self.ctx.transport_snapshot();
        transport.is_playing && self.is_in_scene_range(transport.position)
    }

    fn is_in_scene_range(&self, position: f64) -> bool {
        match self.ctx.scene_range(self.position) {
            Some((true, begin, end)) => position >= begin && position <= end,
            _ => false,
        }
    }

    pub fn is_stop_queued(&self) -> bool {
        self.states.is_some_and(|s| s.stop_queued)
    }

    /// The position of the track which contains this slot.
    pub fn track_position(&self) -> i32 {
        self.track_position
    }

    fn send_track_clip(&self, action: SlotAction) {
        if self.index >= 0 {
            self.ctx.send(Cmd::SlotAction { track: self.track_position, slot: self.position, action });
        }
    }

    /// Set the color (Reaper keeps it only locally).
    pub fn set_color(&mut self, color: ColorEx) {
        self.color = color;
        if self.index >= 0 {
            self.ctx.send(Cmd::SlotColor { track: self.track_position, slot: self.position, value: Rgb(color.to_int_rgb255()) });
        }
    }

    /// Launch the clip on press and release, normally or alternatively (as `SlotImpl.launch` of
    /// the Java version). Reaper only uses the press.
    pub fn launch(&mut self, is_pressed: bool, is_alternative: bool) {
        self.send_track_clip(match (is_pressed, is_alternative) {
            (true, false) => SlotAction::Launch,
            (true, true) => SlotAction::LaunchAlt,
            (false, false) => SlotAction::LaunchRelease,
            (false, true) => SlotAction::LaunchReleaseAlt,
        });
    }

    pub fn start_recording(&mut self) {
        self.send_track_clip(SlotAction::Record);
    }

    pub fn remove(&mut self) {
        self.send_track_clip(SlotAction::Remove);
    }

    pub fn duplicate(&mut self) {
        self.send_track_clip(SlotAction::Duplicate);
    }

    /// Paste the given slot into this slot (Reaper duplicates the source slot).
    pub fn paste(&mut self, source_track_position: i32, source_position: i32) {
        if self.index >= 0 {
            self.ctx.send(Cmd::SlotPaste { track: self.track_position, slot: self.position, source: SlotRef { track: source_track_position, slot: source_position } });
        }
    }

    pub fn insert_file(&mut self, path: &str) {
        if self.index >= 0 {
            self.ctx.send(Cmd::SlotInsertFile { track: self.track_position, slot: self.position, path });
        }
    }

    /// Not supported.
    pub fn set_name(&mut self, _name: &str) {}

    // Internal setters

    pub fn set_exists(&mut self, exists: bool) {
        self.exists = exists;
    }

    pub fn set_position(&mut self, position: i32) {
        if position >= 0 {
            self.position = position;
        }
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn set_has_content(&mut self, has_content: bool) {
        self.has_content = has_content;
    }

    pub fn set_muted(&mut self, is_muted: bool) {
        self.is_muted = is_muted;
    }

    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_color_state(&mut self, color: ColorEx) {
        self.color = color;
    }

    pub fn set_playing_state(&mut self, value: bool) {
        self.states.get_or_insert_with(SlotStates::default).playing = value;
    }

    pub fn set_recording_state(&mut self, value: bool) {
        self.states.get_or_insert_with(SlotStates::default).recording = value;
    }

    pub fn set_play_queued_state(&mut self, value: bool) {
        self.states.get_or_insert_with(SlotStates::default).play_queued = value;
    }

    pub fn set_rec_queued_state(&mut self, value: bool) {
        self.states.get_or_insert_with(SlotStates::default).rec_queued = value;
    }

    pub fn set_stop_queued_state(&mut self, value: bool) {
        self.states.get_or_insert_with(SlotStates::default).stop_queued = value;
    }

    pub(crate) fn set_track_info(&mut self, track_position: i32, rec_arm: bool) {
        self.track_position = track_position;
        self.track_rec_arm = rec_arm;
    }
}

impl BankItem for Slot {
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
        self.send_track_clip(SlotAction::Select);
    }
}

/// The clip slots of a track. Derefs to [`PagedBank<Slot>`].
pub struct SlotBank {
    bank: PagedBank<Slot>,
    track_position: i32,
    track_rec_arm: bool,
}

impl SlotBank {
    pub fn new(ctx: DawContext, num_slots: usize, track_position: i32) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let bank = PagedBank::new(
            ctx,
            num_slots,
            move |position| {
                let index = if num_slots == 0 { 0 } else { position % num_slots };
                Slot::new(item_ctx.clone(), index as i32, position as i32, track_position)
            },
            move || Slot::empty(&empty_ctx),
        );
        SlotBank { bank, track_position, track_rec_arm: false }
    }

    /// Get a slot of the current page for modification.
    pub fn item_mut(&mut self, index: usize) -> &mut Slot {
        let (ti, ra) = (self.track_position, self.track_rec_arm);
        let slot = self.bank.item_mut(index);
        slot.set_track_info(ti, ra);
        slot
    }

    /// Get a slot by its absolute position for modification (creates it if necessary).
    pub fn unpaged_item_mut(&mut self, position: usize) -> &mut Slot {
        let (ti, ra) = (self.track_position, self.track_rec_arm);
        let slot = self.bank.unpaged_item_mut(position);
        slot.set_track_info(ti, ra);
        slot
    }

    pub fn set_item_count(&mut self, count: usize) {
        self.bank.set_item_count(count);
        self.update_slots();
    }

    /// Set the item count of all slot banks to the maximum number of clips of all tracks.
    pub fn set_max_slot_count(&mut self, max_slot_count: usize) {
        let count = self.bank.item_count;
        self.bank.items.truncate(count);
        self.bank.item_count = max_slot_count;
        self.bank.items.reserve(max_slot_count.saturating_sub(count));
        for position in count..max_slot_count {
            let slot = self.bank.unpaged_item_mut(position);
            slot.set_exists(true);
            slot.set_has_content(false);
        }
        self.update_slots();
    }

    /// Set the offset of the page (follows the scene bank).
    pub fn set_bank_offset(&mut self, offset: usize) {
        self.bank.set_bank_offset(offset as i64);
    }

    /// The selected slot of the page.
    pub fn selected_item(&self) -> Option<&Slot> {
        if self.bank.items.is_empty() {
            return None;
        }
        self.bank.selected_item()
    }

    /// Only one slot can be selected in Reaper.
    pub fn selected_items(&self) -> Vec<&Slot> {
        self.selected_item().into_iter().collect()
    }

    /// Get a new (non-existing) slot with the given index (e.g. to create a new clip).
    pub fn empty_slot(&self, start_from: i32) -> Slot {
        let mut slot = Slot::new(self.bank.ctx.clone(), start_from, start_from, self.track_position);
        slot.set_track_info(self.track_position, self.track_rec_arm);
        slot
    }

    pub(crate) fn set_track_info(&mut self, track_position: i32, rec_arm: bool) {
        if self.track_position == track_position && self.track_rec_arm == rec_arm {
            return;
        }
        self.track_position = track_position;
        self.track_rec_arm = rec_arm;
        self.update_slots();
    }

    fn update_slots(&mut self) {
        let (ti, ra) = (self.track_position, self.track_rec_arm);
        for slot in self.bank.all_items_mut() {
            slot.set_track_info(ti, ra);
        }
    }
}

impl Deref for SlotBank {
    type Target = PagedBank<Slot>;
    fn deref(&self) -> &PagedBank<Slot> {
        &self.bank
    }
}
