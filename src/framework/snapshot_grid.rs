// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The host's snapshots on a grid of pads (doc/HOST_BRIDGE_GUIDE.md §4.14; not part of Java):
//! the logic of a snapshot view which a controller wraps with its own pads and colours (the
//! Maschine JAM's 8 x 8 pads; a Launchpad or Push can do the same).
//!
//! - A pad loads its snapshot (`SnapshotLoad`), with the store modifier (Shift) it stores the
//!   current state into the slot (`SnapshotStore`, an occupied slot is overwritten; with
//!   appendOnly an empty pad stores a new one at index = count), with the remove modifier (Clear)
//!   it removes an occupied slot (`SnapshotRemove`, no confirmation).
//! - [`SnapshotGrid::pad`] tells the state of a pad, the controller maps it to its colours.
//! - Without [`HostFeature::Snapshots`] all pads are off and a press only notifies
//!   [`NO_SNAPSHOTS`], nothing is sent.

use super::ctx::Ctx;
use crate::daw::context::HostFeature;
use crate::daw::snapshot::BANK_SIZE;
use crate::protocol::Cmd;

/// The notification of a host without snapshots.
pub const NO_SNAPSHOTS: &str = "No snapshots";

/// The modifier which is held while a pad is pressed or drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotModifier {
    None,
    /// Store (Shift).
    Store,
    /// Remove (Clear).
    Remove,
}

/// The state of a pad, in the order of precedence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SnapshotPad {
    /// Empty (or no snapshots at all).
    Off,
    /// The pad is held.
    Pressed,
    /// Store held: the empty slot where a press stores (appendOnly: only the one at count).
    StoreTarget,
    /// Remove held: an occupied slot.
    RemoveTarget,
    /// The active snapshot.
    Active,
    /// A stored snapshot with its colour (RGB 0..1; None: none of its own).
    Stored(Option<[f64; 3]>),
}

/// The help text of a pad (load), for the help tables of the controllers.
pub fn help_load(ctx: &Ctx) -> &'static str {
    if has_snapshots(ctx) {
        "Load the snapshot (green: active)"
    } else {
        "The host has no snapshots"
    }
}

/// The help text of the store modifier + a pad.
pub fn help_store(ctx: &Ctx) -> &'static str {
    if ctx.model.snapshots().is_append_only() {
        "Store the current state (red: a new snapshot; a stored one: overwrite)"
    } else {
        "Store the current state into the slot (red: empty; a stored one: overwrite)"
    }
}

/// The help text of the remove modifier + a pad.
pub const HELP_REMOVE: &str = "Remove the snapshot (red: the stored ones)";

/// True if the host keeps snapshots.
pub fn has_snapshots(ctx: &Ctx) -> bool {
    ctx.model.ctx().host_supports(HostFeature::Snapshots)
}

/// The held pads of a snapshot grid.
#[derive(Debug, Default)]
pub struct SnapshotGrid {
    /// Bit i: slot i is held.
    pressed: u64,
}

impl SnapshotGrid {
    pub fn new() -> Self {
        SnapshotGrid::default()
    }

    /// Notify on entering the view: "Snapshots", or [`NO_SNAPSHOTS`] without the feature.
    pub fn notify_entered(ctx: &mut Ctx) {
        ctx.notify(if has_snapshots(ctx) { "Snapshots" } else { NO_SNAPSHOTS });
    }

    /// The state of the pad of a slot of the window.
    pub fn pad(&self, ctx: &Ctx, slot: usize, modifier: SnapshotModifier) -> SnapshotPad {
        if slot >= BANK_SIZE || !has_snapshots(ctx) {
            return SnapshotPad::Off;
        }
        if self.pressed & (1 << slot) != 0 {
            return SnapshotPad::Pressed;
        }
        let bank = ctx.model.snapshots();
        let item = bank.item(slot);
        match modifier {
            SnapshotModifier::Store if bank.is_store_target(slot) => return SnapshotPad::StoreTarget,
            SnapshotModifier::Remove if item.exists => return SnapshotPad::RemoveTarget,
            _ => {}
        }
        if !item.exists {
            SnapshotPad::Off
        } else if bank.is_active(slot) {
            SnapshotPad::Active
        } else {
            SnapshotPad::Stored(item.color)
        }
    }

    /// A pad of a slot of the window is pressed (true) or released (false). Returns true if a
    /// command was sent (the controller may consume its modifier then).
    pub fn on_pad(&mut self, ctx: &mut Ctx, slot: usize, modifier: SnapshotModifier, pressed: bool) -> bool {
        if slot >= BANK_SIZE {
            return false;
        }
        if !pressed {
            self.pressed &= !(1 << slot);
            return false;
        }
        if !has_snapshots(ctx) {
            ctx.notify(NO_SNAPSHOTS);
            return false;
        }
        self.pressed |= 1 << slot;
        let bank = ctx.model.snapshots();
        let exists = bank.exists(slot);
        let name = bank.display_name(slot);
        let offset = bank.offset();
        match modifier {
            SnapshotModifier::None => {
                if !exists {
                    return false;
                }
                ctx.model.ctx().send(Cmd::SnapshotLoad { index: offset + slot });
                ctx.notify(&name);
            }
            SnapshotModifier::Store => {
                let index = bank.store_index(slot);
                ctx.model.ctx().send(Cmd::SnapshotStore { index: offset + index });
                ctx.notify(&if exists { format!("Stored: {name}") } else { "Stored".to_string() });
            }
            SnapshotModifier::Remove => {
                if !exists {
                    return false;
                }
                ctx.model.ctx().send(Cmd::SnapshotRemove { index: offset + slot });
                ctx.notify(&format!("Removed: {name}"));
            }
        }
        true
    }

    /// Forget the held pads (e.g. when the view is left).
    pub fn release_all(&mut self) {
        self.pressed = 0;
    }
}
