// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Navigation in the clip launcher which keeps the selected slot when another track is selected
//! (former `IClipLauncherNavigator` and the Reaper `ClipLauncherNavigatorImpl`). A host with its
//! own navigator ([`HostFeature::ClipLauncherNavigator`], Bitwig: the Bitwig
//! `ClipLauncherNavigatorImpl`) gets `navigator …` commands instead.
//!
//! The navigator state is shared (`Rc<RefCell<ClipLauncherNavigator>>`) and owned by the
//! controller which uses it. Differences to Java: the slot selection thread (a
//! `LatestTaskExecutor` which sleeps 50 ms per loop) is a task which is scheduled every 50 ms.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::backend::Processor;
use crate::daw::bank::BankItem;
use crate::daw::context::HostFeature;
use crate::framework::ctx::Ctx;

/// The interval of the slot selection task.
const SELECT_SLOT_INTERVAL_MS: u64 = 50;

/// The state of the navigator.
#[derive(Debug, Default)]
pub struct ClipLauncherNavigator {
    last_edit: Option<Instant>,
    /// The slot to select on the newly selected track, -1 if none.
    target_slot: Option<i32>,
    /// True while the slot selection task is scheduled.
    is_task_running: bool,
}

/// A shared navigator.
pub type SharedNavigator = Rc<RefCell<ClipLauncherNavigator>>;

/// True if the host has its own navigator; then the command was sent to it.
fn send_to_host(ctx: &Ctx, command: &str, is_left: bool) -> bool {
    if !ctx.model.ctx().host_supports(HostFeature::ClipLauncherNavigator) {
        return false;
    }
    ctx.model.ctx().send(Processor::Navigator, &format!("{command}/{}", if is_left { "-" } else { "+" }));
    true
}

/// Select the previous or next scene (former `navigateScenes`).
pub fn navigate_scenes(ctx: &mut Ctx, is_left: bool) {
    if send_to_host(ctx, "scenes", is_left) {
        return;
    }
    let mut scene_bank = ctx.model.scene_bank_mut();
    if is_left {
        scene_bank.select_previous_item();
    } else {
        scene_bank.select_next_item();
    }
}

/// Select the previous or next clip of the cursor track (former `navigateClips`).
pub fn navigate_clips(ctx: &mut Ctx, is_left: bool) {
    if send_to_host(ctx, "clips", is_left) {
        return;
    }
    select_clip(ctx, is_left);
}

/// Select the previous or next slot of the cursor track, without the navigator of the host
/// (former `getCursorTrack ().getSlotBank ().selectPrevious/NextItem ()`).
pub fn select_clip(ctx: &mut Ctx, is_left: bool) {
    if !ctx.model.cursor_track_exists() {
        return;
    }
    // Former AbstractItemBankImpl.selectPrevious/NextItem. The slot bank has no paging functions
    // in Rust, so the selection stays in the current page.
    let slot_bank = ctx.model.cursor_track_mut().slot_bank_mut();
    let page_size = slot_bank.page_size() as i32;
    let index = match slot_bank.selected_item() {
        Some(slot) => slot.index() + if is_left { -1 } else { 1 },
        None => 0,
    };
    if (0..page_size).contains(&index) {
        slot_bank.item_mut(index as usize).select();
    }
}

/// The index of the selected slot of the cursor track, or the first slot if it exists.
fn current_target_slot(ctx: &Ctx) -> Option<i32> {
    let slot_bank = ctx.model.cursor_track().slot_bank();
    match slot_bank.selected_item() {
        Some(slot) => Some(slot.index()),
        None => (slot_bank.item_count() > 0 && slot_bank.item(0).does_exist()).then_some(0),
    }
}

/// Select the previous or next track and move the selected slot as well (former
/// `navigateTracks`).
pub fn navigate_tracks(navigator: &SharedNavigator, ctx: &mut Ctx, is_left: bool) {
    if send_to_host(ctx, "tracks", is_left) {
        return;
    }
    if !ctx.model.cursor_track_exists() {
        let track = ctx.model.track_bank_mut().item_mut(0);
        if !track.does_exist() {
            return;
        }
        track.select();
    }

    // Move the selected slot as well. Are we already moving?
    if navigator.borrow().target_slot.is_none() {
        let target = current_target_slot(ctx);
        navigator.borrow_mut().target_slot = target;
    }
    if is_left {
        ctx.model.cursor_track_select_previous();
    } else {
        ctx.model.cursor_track_select_next();
    }
    navigator.borrow_mut().last_edit = Some(ctx.host.now());
    start_select_slot(navigator, ctx);
}

/// Select the track with the given index in the page of the track bank and move the selected
/// slot as well (former `selectTrack`).
pub fn select_track(navigator: &SharedNavigator, ctx: &mut Ctx, index: usize) {
    // The Bitwig version: the single track bank of the navigator is scrolled to the track, then
    // the track is selected
    if ctx.model.ctx().host_supports(HostFeature::ClipLauncherNavigator) {
        let first = ctx.model.track_bank().item(0);
        if !first.does_exist() {
            return;
        }
        let position = first.position() + index as i32;
        ctx.model.ctx().send_int(Processor::Navigator, "selectTrack", position);
        ctx.model.track_bank_mut().item_mut(index).select();
        return;
    }
    let track = ctx.model.track_bank().item(index);
    if !track.does_exist() {
        return;
    }
    if track.is_selected() {
        if track.is_group() {
            ctx.model.track_bank_mut().item_mut(index).toggle_group_expanded();
        }
        return;
    }

    // Move the selected slot as well
    let does_exist = ctx.model.cursor_track_exists();
    if does_exist && navigator.borrow().target_slot.is_none() {
        let target = current_target_slot(ctx);
        navigator.borrow_mut().target_slot = target;
    }
    ctx.model.track_bank_mut().item_mut(index).select();
    if does_exist {
        navigator.borrow_mut().last_edit = Some(ctx.host.now());
        start_select_slot(navigator, ctx);
    }
}

fn start_select_slot(navigator: &SharedNavigator, ctx: &mut Ctx) {
    if navigator.borrow().is_task_running {
        return;
    }
    navigator.borrow_mut().is_task_running = true;
    let navigator = navigator.clone();
    ctx.schedule(SELECT_SLOT_INTERVAL_MS, move |ctx| select_slot(&navigator, ctx));
}

/// Select the target slot on the new track until no further track change happened for 2
/// seconds (former `selectSlot`).
fn select_slot(navigator: &SharedNavigator, ctx: &mut Ctx) {
    let (target, last_edit) = {
        let state = navigator.borrow();
        (state.target_slot, state.last_edit)
    };
    let Some(target) = target else {
        navigator.borrow_mut().is_task_running = false;
        return;
    };
    let diff = last_edit.map_or(u128::MAX, |last| ctx.host.now().saturating_duration_since(last).as_millis());

    // Finally done
    if diff > 2000 {
        let mut state = navigator.borrow_mut();
        state.target_slot = None;
        state.is_task_running = false;
        return;
    }

    // Update the selection but do not yet clear it!
    if diff > 200 && ctx.model.cursor_track_exists() {
        if let Ok(index) = usize::try_from(target) {
            let slot_bank = ctx.model.cursor_track_mut().slot_bank_mut();
            if index < slot_bank.page_size() {
                slot_bank.item_mut(index).select();
            }
        }
    }
    let navigator = navigator.clone();
    ctx.schedule(SELECT_SLOT_INTERVAL_MS, move |ctx| select_slot(&navigator, ctx));
}
