// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The snapshots which the host keeps (`/snapshot/…`, doc/HOST_BRIDGE_GUIDE.md §4.14; not part
//! of Java; zynthian's ZS3): a bank window of [`BANK_SIZE`] slots, filled by the parser. The host
//! owns the numbering, the names, the colours and the persistence; the core only sends
//! `SnapshotLoad|Store|Remove` and shows what the host reports.
//!
//! The slots may have gaps (a sparse host: slot i is the snapshot of program change i) or be a
//! dense list (`appendOnly`: the snapshots 0..count, a new one is stored at index = count).
//! Without appendOnly the count is informational only (zynthian reports 128, all its program
//! slots, of which the window shows the first 64): the store targets and the colours come from
//! `exists` per slot, nothing is indexed by the count.
//!
//! The reported index is the slot in the window; the window starts at [`SnapshotBank::offset`]
//! (always 0 for now, a later bank offset moves it as the scene bank's).

/// The number of slots in the window (8 x 8 pads).
pub const BANK_SIZE: usize = 64;

/// A slot of the bank.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub exists: bool,
    pub name: String,
    /// RGB, 0..1; None: no colour of its own.
    pub color: Option<[f64; 3]>,
}

/// The snapshots of the host.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotBank {
    /// The total number of slots (may exceed the window).
    count: i32,
    /// The active snapshot (the absolute index), -1: none.
    active: i32,
    append_only: bool,
    /// The absolute index of the first slot of the window.
    offset: usize,
    items: Vec<Snapshot>,
    /// Counts the changes (every mutable access), so that a view can detect one.
    generation: u64,
}

impl Default for SnapshotBank {
    fn default() -> Self {
        SnapshotBank { count: 0, active: -1, append_only: false, offset: 0, items: vec![Snapshot::default(); BANK_SIZE], generation: 0 }
    }
}

impl SnapshotBank {
    /// The number of changes so far.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Mark a change (the parser calls it for every report).
    pub fn touch(&mut self) -> &mut Self {
        self.generation += 1;
        self
    }

    /// The total number of slots (appendOnly: of the stored snapshots).
    pub fn count(&self) -> i32 {
        self.count
    }

    /// Set the reported count; the slots of the window at and beyond it are cleared.
    pub fn set_count(&mut self, count: i32) {
        self.count = count.max(0);
        let first_gone = (self.count as usize).saturating_sub(self.offset);
        for item in self.items.iter_mut().skip(first_gone) {
            *item = Snapshot::default();
        }
    }

    /// The active snapshot (absolute index), -1: none.
    pub fn active(&self) -> i32 {
        self.active
    }

    pub fn set_active(&mut self, active: i32) {
        self.active = active.max(-1);
    }

    /// True if the host keeps a dense list: a new snapshot goes to index = count.
    pub fn is_append_only(&self) -> bool {
        self.append_only
    }

    pub fn set_append_only(&mut self, append_only: bool) {
        self.append_only = append_only;
    }

    /// The absolute index of the first slot of the window.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The slot of the window (an empty one beyond it).
    pub fn item(&self, slot: usize) -> &Snapshot {
        static EMPTY: Snapshot = Snapshot { exists: false, name: String::new(), color: None };
        self.items.get(slot).unwrap_or(&EMPTY)
    }

    /// The slot of the window to change (None beyond it).
    pub fn item_mut(&mut self, slot: usize) -> Option<&mut Snapshot> {
        self.items.get_mut(slot)
    }

    /// True if the slot of the window holds a snapshot.
    pub fn exists(&self, slot: usize) -> bool {
        self.item(slot).exists
    }

    /// True if the slot of the window is the active snapshot.
    pub fn is_active(&self, slot: usize) -> bool {
        self.active >= 0 && self.active as usize == self.offset + slot
    }

    /// True if any slot of the window holds a snapshot.
    pub fn has_any(&self) -> bool {
        self.items.iter().any(|item| item.exists)
    }

    /// The name of a slot of the window to show: its name, else "Snapshot <n>" (n from 1).
    pub fn display_name(&self, slot: usize) -> String {
        let name = &self.item(slot).name;
        if name.is_empty() {
            format!("Snapshot {}", self.offset + slot + 1)
        } else {
            name.clone()
        }
    }

    /// The index which `SnapshotStore` sends for the slot of the window: an occupied slot
    /// itself (overwrite), an empty one itself, with appendOnly the next new one (count).
    pub fn store_index(&self, slot: usize) -> usize {
        if self.exists(slot) || !self.append_only {
            slot
        } else {
            (self.count.max(0) as usize).saturating_sub(self.offset)
        }
    }

    /// True if the slot of the window is an empty store target: with appendOnly only the slot
    /// at count, otherwise every empty slot.
    pub fn is_store_target(&self, slot: usize) -> bool {
        if self.exists(slot) || slot >= BANK_SIZE {
            return false;
        }
        !self.append_only || self.store_index(slot) == slot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank(append_only: bool, stored: &[usize], count: i32) -> SnapshotBank {
        let mut bank = SnapshotBank::default();
        bank.set_append_only(append_only);
        bank.set_count(count);
        for &slot in stored {
            bank.item_mut(slot).unwrap().exists = true;
        }
        bank
    }

    #[test]
    fn append_only_stores_at_count() {
        let bank = bank(true, &[0, 1, 2], 3);
        assert_eq!(bank.store_index(1), 1);
        assert_eq!(bank.store_index(10), 3);
        assert!(bank.is_store_target(3));
        assert!(!bank.is_store_target(4));
        assert!(!bank.is_store_target(1));
    }

    #[test]
    fn sparse_stores_in_the_gap() {
        let bank = bank(false, &[0, 5, 40], 64);
        assert_eq!(bank.store_index(7), 7);
        assert!(bank.is_store_target(1));
        assert!(bank.is_store_target(63));
        assert!(!bank.is_store_target(40));
    }

    #[test]
    fn a_sparse_count_beyond_the_window_is_informational() {
        let bank = bank(false, &[0, 5, 40], 128);
        assert_eq!(bank.count(), 128);
        assert!(bank.exists(40) && bank.is_store_target(63) && !bank.is_store_target(64));
        assert_eq!(bank.store_index(63), 63);
    }

    #[test]
    fn count_clears_the_slots_beyond() {
        let mut bank = bank(true, &[0, 1, 2], 3);
        bank.set_count(1);
        assert!(bank.exists(0));
        assert!(!bank.exists(1));
        assert_eq!(bank.display_name(1), "Snapshot 2");
    }
}
