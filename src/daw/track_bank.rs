// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The bank of tracks (former `ITrackBank`, `AbstractTrackBankImpl`, `TrackBankImpl`).
//!
//! All tracks are stored by their position. The bank shows either a flat list of all tracks
//! (optionally including the master track at the end, see `ModelSetup::has_full_flat_track_list`)
//! or the children of the current folder (group track) if the flat track list is disabled.
//! The track bank also owns the master track and the scene bank (the pages of the slot banks of
//! all tracks follow the page of the scene bank).

use std::ops::{Deref, DerefMut};

use crate::backend::Processor;

use super::bank::BankItem;
use super::clip::Note;
use super::color::{ColorEx, DawColor};
use super::constants::ChannelType;
use super::context::DawContext;
use super::events::ModelEvent;
use super::scene::SceneBank;
use super::track::Track;

const NOTE_OFF: u8 = 0;
const NOTE_ON: u8 = 1;
const NOTE_ON_NEW: u8 = 2;

/// An entry of the visible track list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Track(usize),
    Master,
}

/// Identifies a track: either a track at a position or the master track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackRef {
    /// The track at the given absolute position.
    Position(usize),
    /// The master track.
    Master,
}

/// The bank of tracks.
pub struct TrackBank {
    ctx: DawContext,
    tracks: Vec<Track>,
    empty: Track,
    master: Track,
    scene_bank: SceneBank,
    page_size: usize,
    raw_item_count: usize,
    bank_offset: usize,
    /// True if the host pages the tracks (it reported `/track/offset`, protocol v2): it only
    /// reports the tracks of its window and is told each change of the bank offset.
    host_paged: bool,
    num_sends: usize,
    num_scenes: usize,
    num_params: usize,
    has_flat_track_list: bool,
    has_full_flat_track_list: bool,
    skip_disabled_items: bool,
    is_dirty: bool,
    /// The currently visible tracks (flat list or children of the current folder).
    view: Vec<Entry>,
    /// Tree: children of each node; node 0 is the root, node p+1 is the track at position p.
    tree_children: Vec<Vec<usize>>,
    tree_parent: Vec<Option<usize>>,
    /// Tree: the node of the current folder.
    current_folder: usize,
    note_cache: [u8; 128],
}

impl TrackBank {
    /// Create a track bank with `num_tracks` tracks per page.
    pub fn new(
        ctx: DawContext,
        num_tracks: usize,
        num_scenes: usize,
        num_sends: usize,
        num_params: usize,
        has_flat_track_list: bool,
        has_full_flat_track_list: bool,
    ) -> Self {
        TrackBank {
            empty: Track::empty(&ctx, num_sends),
            master: Track::new_master(ctx.clone(), num_sends, num_params),
            scene_bank: SceneBank::new(ctx.clone(), num_scenes),
            ctx,
            tracks: Vec::new(),
            page_size: num_tracks,
            raw_item_count: 0,
            bank_offset: 0,
            host_paged: false,
            num_sends,
            num_scenes,
            num_params,
            has_flat_track_list,
            has_full_flat_track_list,
            skip_disabled_items: false,
            is_dirty: true,
            view: Vec::new(),
            tree_children: vec![Vec::new()],
            tree_parent: vec![None],
            current_folder: 0,
            note_cache: [NOTE_OFF; 128],
        }
    }

    /// Enable or disable the updates of the track data.
    pub fn enable_observers(&self, enable: bool) {
        self.ctx.enable_updates(Processor::Track, enable);
    }

    // ----------------------------------------------------------------------------------------
    // Access

    /// The number of tracks per page.
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The number of visible tracks (all tracks for a flat list, otherwise the number of tracks
    /// in the current folder).
    pub fn item_count(&self) -> usize {
        self.view.len()
    }

    /// The number of all tracks in the project (without the master track).
    pub fn track_count(&self) -> usize {
        self.raw_item_count
    }

    fn entry(&self, index: usize) -> Option<Entry> {
        self.view.get(self.bank_offset + index).copied()
    }

    /// Get the track at the index of the current page (an empty track if there is none).
    pub fn item(&self, index: usize) -> &Track {
        match self.entry(index) {
            Some(Entry::Track(p)) => &self.tracks[p],
            Some(Entry::Master) => &self.master,
            None => &self.empty,
        }
    }

    /// Get the track at the index of the current page for modification (a fresh empty track if
    /// there is none; its commands do nothing).
    pub fn item_mut(&mut self, index: usize) -> &mut Track {
        match self.entry(index) {
            Some(Entry::Track(p)) => &mut self.tracks[p],
            Some(Entry::Master) => &mut self.master,
            None => {
                self.empty = Track::empty(&self.ctx, self.num_sends);
                &mut self.empty
            }
        }
    }

    /// Get the track at the given absolute position (an empty track if it was not created).
    pub fn unpaged_item(&self, position: usize) -> &Track {
        self.tracks.get(position).unwrap_or(&self.empty)
    }

    /// Get the track at the given absolute position (it is created if necessary).
    pub fn unpaged_item_mut(&mut self, position: usize) -> &mut Track {
        self.ensure_tracks(position + 1);
        &mut self.tracks[position]
    }

    /// Get a track by reference.
    pub fn track(&self, track: TrackRef) -> &Track {
        match track {
            TrackRef::Position(p) => self.unpaged_item(p),
            TrackRef::Master => &self.master,
        }
    }

    /// Get a track by reference for modification.
    pub fn track_mut(&mut self, track: TrackRef) -> &mut Track {
        match track {
            TrackRef::Position(p) => self.unpaged_item_mut(p),
            TrackRef::Master => &mut self.master,
        }
    }

    /// All tracks which were created (by position).
    pub fn all_tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn master(&self) -> &Track {
        &self.master
    }

    pub fn master_mut(&mut self) -> &mut Track {
        &mut self.master
    }

    fn ensure_tracks(&mut self, count: usize) {
        let offset = self.scene_bank.scroll_position().max(0) as usize;
        while self.tracks.len() < count {
            let position = self.tracks.len();
            let ps = self.page_size.max(1);
            let mut track = Track::new(self.ctx.clone(), (position % ps) as i32, position as i32, self.num_sends, self.num_scenes, self.num_params);
            track.slot_bank_mut().set_bank_offset(offset);
            self.tracks.push(track);
        }
    }

    /// The position of the first track of the page.
    pub fn scroll_position(&self) -> i32 {
        self.bank_offset as i32
    }

    /// Iterate over the tracks of the current page (including empty placeholders).
    pub fn page_items(&self) -> impl Iterator<Item = &Track> {
        (0..self.page_size).map(move |i| self.item(i))
    }

    pub fn has_existing_items(&self) -> bool {
        self.page_items().any(|t| t.does_exist())
    }

    /// The selected track of the current page.
    pub fn selected_item(&self) -> Option<&Track> {
        self.page_items().find(|t| t.is_selected())
    }

    /// The index of the selected track in the current page.
    pub fn selected_index(&self) -> Option<usize> {
        (0..self.page_size).find(|i| self.item(*i).is_selected())
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut Track> {
        let index = self.selected_index()?;
        Some(self.item_mut(index))
    }

    /// A reference to the selected track of the current page.
    pub fn selected_ref(&self) -> Option<TrackRef> {
        let index = self.selected_index()?;
        self.ref_at(index)
    }

    /// The reference to the track at the index of the page.
    pub fn ref_at(&self, index: usize) -> Option<TrackRef> {
        match self.entry(index)? {
            Entry::Track(p) => Some(TrackRef::Position(p)),
            Entry::Master => Some(TrackRef::Master),
        }
    }

    pub fn selected_items(&self) -> Vec<&Track> {
        self.page_items().filter(|t| t.is_selected()).collect()
    }

    pub fn position_of_last_item(&self) -> i32 {
        for i in (0..self.page_size).rev() {
            let item = self.item(i);
            if item.does_exist() && item.position() >= 0 {
                return item.position();
            }
        }
        -1
    }

    // ----------------------------------------------------------------------------------------
    // Scrolling

    pub fn can_scroll_backwards(&self) -> bool {
        let sel_index = self.selected_item().map(|t| t.index()).unwrap_or(-1);
        sel_index > 0 || self.can_scroll_page_backwards()
    }

    pub fn can_scroll_forwards(&self) -> bool {
        let sel_index = self.selected_item().map(|t| t.index()).unwrap_or(-1);
        (sel_index >= 0 && (sel_index as usize) + 1 < self.page_size && self.item(sel_index as usize + 1).does_exist())
            || self.can_scroll_page_forwards()
    }

    pub fn can_scroll_page_backwards(&self) -> bool {
        // Java for Bitwig asks the bank of the host (canScrollBackwards: any position > 0), Java
        // for Reaper requires a full page
        if self.ctx.is_host_paging() { self.bank_offset > 0 } else { self.bank_offset >= self.page_size }
    }

    pub fn can_scroll_page_forwards(&self) -> bool {
        self.bank_offset + self.page_size < self.item_count()
    }

    pub fn scroll_backwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 - 1);
    }

    pub fn scroll_forwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 + 1);
    }

    /// Scroll the DAW to the given position. With host paging (Bitwig) as
    /// AbstractItemBankImpl.scrollTo: the window starts exactly at the position (`adjust_page` is
    /// ignored), nothing is selected. Reaper: sends `<pos>/scrollto`, the DAW then updates the
    /// selection.
    pub fn scroll_to(&mut self, position: usize, adjust_page: bool) {
        if self.host_paged {
            if position < self.item_count() {
                self.set_bank_offset(position as i64);
            }
            return;
        }
        if self.has_flat_track_list && self.has_full_flat_track_list && self.view.get(position) == Some(&Entry::Master) {
            self.master.select();
            return;
        }
        if position >= self.item_count() {
            return;
        }
        let ps = self.page_size.max(1);
        let pos = if adjust_page { position / ps * ps } else { position };
        self.ctx.send(Processor::Track, &format!("{pos}/scrollto"));
    }

    fn scroll_page_backwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 - self.page_size as i64);
        self.send_page_selection(self.page_size.saturating_sub(1));
    }

    fn scroll_page_forwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 + self.page_size as i64);
        self.send_page_selection(0);
    }

    /// Move the selection to the new page (former `AbstractTrackBankImpl.scrollPage*` of the
    /// Java version for Reaper). With host paging (Bitwig) scrolling only scrolls, as in the Java
    /// version for Bitwig.
    fn send_page_selection(&mut self, sel_index: usize) {
        if self.ctx.is_host_paging() {
            return;
        }
        if let Some(selected) = self.selected_item() {
            let pos = selected.position();
            self.ctx.send_int(Processor::Track, &format!("{pos}/select"), 0);
        }
        let sel_pos = self.item(sel_index).position();
        self.ctx.send_int(Processor::Track, &format!("{sel_pos}/select"), 1);
    }

    /// Set the position of the first track of the page (clamped).
    pub fn set_bank_offset(&mut self, bank_offset: i64) {
        let max = (self.item_count() as i64 - 1).max(0);
        let old_offset = self.bank_offset;
        self.bank_offset = bank_offset.clamp(0, max) as usize;
        if self.host_paged && self.bank_offset != old_offset {
            self.ctx.send_int(Processor::Bank, "track", self.bank_offset as i32);
        }
        self.ctx.post_event(ModelEvent::TrackBankPageChanged);
    }

    /// The host reported the offset of its track window: from now on the host pages the tracks.
    /// Tells the host the current offset.
    pub fn enable_host_paging(&mut self) {
        if !self.host_paged {
            self.host_paged = true;
            self.ctx.set_host_paging();
            self.ctx.send_int(Processor::Bank, "track", self.bank_offset as i32);
        }
    }

    /// Enable host paging of the scenes (and with them the slots).
    pub fn enable_host_scene_paging(&mut self) {
        self.scene_bank.enable_host_paging();
    }

    pub fn select_next_item(&mut self) {
        let index = self.selected_item().map(|t| t.index() + 1).unwrap_or(0);
        if index as usize == self.page_size {
            self.select_next_page();
        } else {
            self.item_mut(index.max(0) as usize).select();
        }
    }

    /// AbstractItemBankImpl.selectPreviousItem: at the first track, scroll to the previous page
    /// and select its last track (`select_previous_page` selects the first one on Bitwig).
    pub fn select_previous_item(&mut self) {
        let index = self.selected_item().map(|t| t.index() - 1).unwrap_or(0);
        if index == -1 {
            if !self.can_scroll_page_backwards() {
                return;
            }
            self.scroll_page_backwards();
            let index = self.page_size.saturating_sub(1);
            self.ctx.schedule(75, move |m| m.track_bank_mut().item_mut(index).select());
        } else {
            self.item_mut(index as usize).select();
        }
    }

    pub fn select_previous_page(&mut self) {
        if !self.can_scroll_page_backwards() {
            return;
        }
        self.scroll_page_backwards();
        // Java for Bitwig (AbstractItemBankImpl) selects the first track of the page, Java for
        // Reaper (AbstractPagedBankImpl) the last one
        let index = if self.ctx.is_host_paging() { 0 } else { self.page_size.saturating_sub(1) };
        self.ctx.schedule(75, move |m| m.track_bank_mut().item_mut(index).select());
    }

    pub fn select_next_page(&mut self) {
        if !self.can_scroll_page_forwards() {
            return;
        }
        self.scroll_page_forwards();
        self.ctx.schedule(75, |m| m.track_bank_mut().item_mut(0).select());
    }

    /// Scroll to the page of the track at the given position and select it (after a delay).
    pub fn select_item_at_position(&mut self, position: usize) {
        if position >= self.item_count() || self.page_size == 0 {
            return;
        }
        let ps = self.page_size;
        self.scroll_to(position / ps * ps, true);
        self.ctx.schedule(75, move |m| {
            m.track_bank_mut().item_mut(position % ps).select();
            m.ctx().post_event(ModelEvent::TrackBankPageChanged);
        });
    }

    /// Skip deactivated tracks.
    pub fn set_skip_disabled_items(&mut self, should_skip: bool) {
        self.skip_disabled_items = should_skip;
        self.mark_dirty();
        self.recalc_if_dirty();
    }

    // ----------------------------------------------------------------------------------------
    // Groups / folders

    /// True if the current folder has a parent (always false for a flat track list).
    pub fn has_parent(&self) -> bool {
        !self.has_flat_track_list && self.current_folder != 0
    }

    /// Leave the current folder.
    pub fn select_parent(&mut self) {
        if self.has_flat_track_list {
            return;
        }
        let previous = self.current_folder;
        self.current_folder = self.tree_parent.get(previous).copied().flatten().unwrap_or(0);
        self.update_tree_view();
        if previous > 0 {
            self.tracks[previous - 1].select();
        }
        self.ctx.post_event(ModelEvent::TrackBankPageChanged);
    }

    /// Enter the selected group track of the current folder.
    pub fn enter_current_folder(&mut self) {
        if self.has_flat_track_list {
            return;
        }
        let children = self.tree_children[self.current_folder].clone();
        for node in children {
            let track = &self.tracks[node - 1];
            if track.is_selected() && track.is_group() {
                self.current_folder = node;
                self.update_tree_view();
                if let Some(first) = self.tree_children[node].first().copied() {
                    self.tracks[first - 1].select();
                }
                self.ctx.post_event(ModelEvent::TrackBankPageChanged);
                break;
            }
        }
    }

    /// Enter the group track at the given index of the page (former `ITrack.enter`). Selects the
    /// track first if it is not selected.
    pub fn enter(&mut self, index: usize) {
        let track = self.item(index);
        if !track.is_group() || track.is_master() {
            return;
        }
        if track.is_selected() {
            self.enter_current_folder();
            return;
        }
        self.item_mut(index).select();
        self.ctx.schedule(100, |m| m.track_bank_mut().enter_current_folder());
    }

    // ----------------------------------------------------------------------------------------
    // Mixer functions

    /// Toggle the record arm state of all tracks (based on the state of the first track).
    pub fn toggle_rec_arm(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        let state = !self.tracks[0].is_rec_arm();
        for track in &mut self.tracks {
            track.set_rec_arm(state);
        }
    }

    /// True if another track than the given one is soloed.
    pub fn is_muted_by_solo(&self, position: usize) -> bool {
        (0..self.raw_item_count.min(self.tracks.len())).any(|p| p != position && self.tracks[p].is_solo())
    }

    pub fn has_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.is_solo())
    }

    pub fn has_mute(&self) -> bool {
        self.tracks.iter().any(|t| t.is_mute())
    }

    pub fn clear_solo(&mut self) {
        for track in &mut self.tracks {
            track.set_solo(false);
        }
    }

    pub fn clear_mute(&mut self) {
        for track in &mut self.tracks {
            track.set_mute(false);
        }
    }

    /// Not supported.
    pub fn is_clip_recording(&self) -> bool {
        false
    }

    /// Stop all clips (not supported by Reaper).
    pub fn stop(&mut self, is_alternative: bool) {
        self.ctx.send(Processor::Scene, if is_alternative { "stopAlt" } else { "stop" });
    }

    /// Not supported.
    pub fn set_indication(&mut self, _enable: bool) {}

    /// The color ID (see [`DawColor::id`]) of the selected track.
    pub fn selected_channel_color_entry(&self) -> &'static str {
        match self.selected_item() {
            Some(track) => DawColor::color_id(track.color()),
            None => DawColor::ColorOff.id(),
        }
    }

    /// True if the send with the given index exists on one of the tracks of the page.
    pub fn can_edit_send(&self, send_index: usize) -> bool {
        self.page_items().any(|t| t.send_bank().item_count() > 0 && t.send_bank().item(send_index).does_exist())
    }

    pub fn edit_send_name(&self, send_index: usize) -> String {
        if self.can_edit_send(send_index) {
            format!("Send {}", send_index + 1)
        } else {
            String::new()
        }
    }

    /// Add a new track (former `addChannel`). Without a color the next DAW color is used.
    pub fn add_channel(&mut self, channel_type: ChannelType, name: Option<&str>, color: Option<ColorEx>, devices: &[&str]) {
        let color = color.unwrap_or_else(|| DawColor::next_color().color());
        add_channel(&self.ctx, channel_type, name, Some(color), devices);
    }

    // ----------------------------------------------------------------------------------------
    // Scenes

    pub fn scene_bank(&self) -> &SceneBank {
        &self.scene_bank
    }

    /// Modify the scene bank. When the returned guard is dropped the pages of the slot banks of
    /// all tracks are updated to the page of the scene bank.
    pub fn scene_bank_mut(&mut self) -> SceneBankGuard<'_> {
        SceneBankGuard { bank: self }
    }

    /// The scene bank without updating the slot banks (for selections which do not page).
    pub(crate) fn scene_bank_unguarded_mut(&mut self) -> &mut SceneBank {
        &mut self.scene_bank
    }

    fn update_slot_banks(&mut self) {
        let offset = self.scene_bank.scroll_position().max(0) as usize;
        for track in &mut self.tracks {
            track.slot_bank_mut().set_bank_offset(offset);
        }
    }

    // ----------------------------------------------------------------------------------------
    // Internal (parser)

    /// Set the number of tracks (creates the tracks if necessary).
    pub fn set_item_count(&mut self, count: usize) {
        self.raw_item_count = count;
        self.ensure_tracks(count);
        self.mark_dirty();
        self.recalc_if_dirty();
        let ps = self.page_size;
        if let Some(pages) = count.checked_div(ps) {
            let offset = self.bank_offset.min(pages * ps);
            self.set_bank_offset(offset as i64);
        }
    }

    /// Mark the track list for recalculation.
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    /// Recalculate the visible track list if it was marked dirty.
    pub fn recalc_if_dirty(&mut self) {
        if !self.is_dirty {
            return;
        }
        self.is_dirty = false;
        if self.has_flat_track_list {
            self.calc_flat_tracks();
        } else {
            self.calc_tree_tracks();
        }
        self.ctx.post_event(ModelEvent::TrackBankPageChanged);
    }

    fn update_master_position(&mut self) {
        let position = if self.has_full_flat_track_list { self.item_count() as i32 - 1 } else { 0 };
        self.master.set_master_position(position.max(0));
        self.master.send_bank_mut().set_channel_position(position.max(0));
        let ps = self.page_size.max(1);
        let index = ((self.item_count() as i64 - 1).max(0) as usize % ps) as i32;
        self.master.set_index(index);
    }

    fn calc_flat_tracks(&mut self) {
        self.view.clear();
        for p in 0..self.raw_item_count.min(self.tracks.len()) {
            if self.skip_disabled_items && !self.tracks[p].is_activated() {
                continue;
            }
            self.view.push(Entry::Track(p));
        }
        // Bitwig lists its master (and the effect tracks) in the full flat track list itself
        // (createTrackBank of the Java version); Reaper does not, then the core adds it
        let host_lists_master = self.tracks[..self.raw_item_count.min(self.tracks.len())].iter().any(|t| t.channel_type() == ChannelType::Master);
        if self.has_full_flat_track_list && !host_lists_master {
            self.view.push(Entry::Master);
        }
        let ps = self.page_size.max(1);
        for (i, entry) in self.view.clone().into_iter().enumerate() {
            if let Entry::Track(p) = entry {
                self.tracks[p].set_index((i % ps) as i32);
            }
        }
        self.update_master_position();
    }

    fn calc_tree_tracks(&mut self) {
        let count = self.raw_item_count.min(self.tracks.len());
        self.tree_children = vec![Vec::new(); count + 1];
        self.tree_parent = vec![None; count + 1];
        let mut current_folder = None;
        let mut hierarchy: Vec<usize> = vec![0];
        let ps = self.page_size.max(1);
        for p in 0..count {
            if self.skip_disabled_items && !self.tracks[p].is_activated() {
                continue;
            }
            let depth = self.tracks[p].depth().max(0) as usize;
            if depth >= hierarchy.len() {
                continue;
            }
            let parent = hierarchy[depth];
            let node = p + 1;
            self.tree_children[parent].push(node);
            self.tree_parent[node] = Some(parent);
            let children_size = self.tree_children[parent].len();
            self.tracks[p].set_index(((children_size - 1) % ps) as i32);
            let index = depth + 1;
            if index < hierarchy.len() {
                hierarchy[index] = node;
            } else {
                hierarchy.push(node);
            }
            if self.tracks[p].is_selected() {
                current_folder = Some(parent);
                self.bank_offset = (children_size - 1) / ps * ps;
            }
        }
        self.current_folder = current_folder.unwrap_or(0);
        self.update_tree_view();
        self.update_master_position();
    }

    fn update_tree_view(&mut self) {
        self.view = self.tree_children.get(self.current_folder).map(|c| c.iter().map(|n| Entry::Track(n - 1)).collect()).unwrap_or_default();
    }

    fn find_selected_track(&mut self, node: usize) -> bool {
        let ps = self.page_size.max(1);
        let children = self.tree_children.get(node).cloned().unwrap_or_default();
        for (i, child) in children.into_iter().enumerate() {
            let track = &self.tracks[child - 1];
            if track.is_selected() {
                self.current_folder = node;
                self.update_tree_view();
                self.set_bank_offset((i / ps * ps) as i64);
                return true;
            }
            if track.is_group() && self.find_selected_track(child) {
                return true;
            }
        }
        false
    }

    fn is_on_selected_page(&self, position: usize) -> bool {
        (0..self.page_size).any(|i| self.entry(i) == Some(Entry::Track(position)))
    }

    /// Handle the selection change of a track: adjusts the page so that the selected track is
    /// visible and posts a [`ModelEvent::TrackSelectionChanged`].
    pub fn handle_bank_track_selection(&mut self, track: TrackRef, is_selected: bool) {
        self.recalc_if_dirty();
        if self.has_flat_track_list {
            if is_selected {
                let entry = match track {
                    TrackRef::Master => Entry::Master,
                    TrackRef::Position(p) => Entry::Track(p),
                };
                if let Some(view_pos) = self.view.iter().position(|e| *e == entry) {
                    let ps = self.page_size.max(1);
                    if view_pos < self.bank_offset || view_pos >= self.bank_offset + self.page_size {
                        self.set_bank_offset((view_pos / ps * ps) as i64);
                    }
                }
            }
        } else {
            if track == TrackRef::Master {
                return;
            }
            self.find_selected_track(0);
        }
        let index = match track {
            TrackRef::Master => -1,
            TrackRef::Position(p) => self.unpaged_item(p).index(),
        };
        self.ctx.post_event(ModelEvent::TrackSelectionChanged { index, is_selected });
    }

    /// Handle the currently playing notes of a track (posts [`ModelEvent::TrackNote`] events for
    /// tracks on the current page).
    pub fn handle_notes(&mut self, track_position: usize, notes: &[Note]) {
        for note in notes {
            let pitch = note.pitch();
            if !(0..128).contains(&pitch) {
                continue;
            }
            self.note_cache[pitch as usize] = NOTE_ON_NEW;
            self.notify_note_observers(track_position, pitch, note.velocity());
        }
        for i in 0..128 {
            if self.note_cache[i] == NOTE_ON_NEW {
                self.note_cache[i] = NOTE_ON;
            } else if self.note_cache[i] == NOTE_ON {
                self.note_cache[i] = NOTE_OFF;
                self.notify_note_observers(track_position, i as i32, 0);
            }
        }
    }

    fn notify_note_observers(&self, track_position: usize, note: i32, velocity: i32) {
        if !self.is_on_selected_page(track_position) {
            return;
        }
        let track_index = self.unpaged_item(track_position).index();
        self.ctx.post_event(ModelEvent::TrackNote { track_index, note, velocity });
    }

    pub fn has_flat_track_list(&self) -> bool {
        self.has_flat_track_list
    }

    pub fn has_full_flat_track_list(&self) -> bool {
        self.has_full_flat_track_list
    }
}

/// Sends the command to add a new track (former `IApplication.addChannel`).
pub(crate) fn add_channel(ctx: &DawContext, channel_type: ChannelType, name: Option<&str>, color: Option<ColorEx>, devices: &[&str]) {
    let type_str = match channel_type {
        ChannelType::Hybrid | ChannelType::Instrument => "INSTRUMENT",
        ChannelType::Effect => "EFFECT",
        _ => "AUDIO",
    };
    let color_txt = match color {
        Some(c) => {
            let [r, g, b] = c.to_int_rgb255();
            format!("RGB({r},{g},{b})")
        }
        None => String::new(),
    };
    let mut params: Vec<&str> = vec![type_str, name.unwrap_or(""), &color_txt];
    params.extend_from_slice(devices);
    ctx.send_strs(Processor::Track, "addTrack", &params);
}

/// Gives mutable access to the scene bank and synchronizes the slot banks when dropped.
pub struct SceneBankGuard<'a> {
    bank: &'a mut TrackBank,
}

impl Deref for SceneBankGuard<'_> {
    type Target = SceneBank;
    fn deref(&self) -> &SceneBank {
        &self.bank.scene_bank
    }
}

impl DerefMut for SceneBankGuard<'_> {
    fn deref_mut(&mut self) -> &mut SceneBank {
        &mut self.bank.scene_bank
    }
}

impl Drop for SceneBankGuard<'_> {
    fn drop(&mut self) {
        self.bank.update_slot_banks();
    }
}
