// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Generic parts of the former `AbstractControllerSetup` which several controller setups share:
//! the scale observers, the browser observers, the track change handling and the update of the
//! scale properties of the configuration.

use crate::config::SettingId;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{Modes, Views};

/// React on a changed scale setting (former `createScaleObservers`). Returns true if the
/// setting was one of the scale settings.
pub fn handle_scale_setting(ctx: &mut Ctx, id: SettingId) -> bool {
    match id {
        SettingId::SCALES_SCALE => {
            let name = ctx.config.scale().to_string();
            ctx.model.scales_mut().set_scale_by_name(&name);
        }
        SettingId::SCALES_BASE => {
            let name = ctx.config.scale_base().to_string();
            ctx.model.scales_mut().set_scale_offset_by_name(&name);
        }
        SettingId::SCALES_IN_KEY => {
            let chromatic = !ctx.config.is_scale_in_key();
            ctx.model.scales_mut().set_chromatic(chromatic);
        }
        SettingId::SCALES_LAYOUT => {
            let name = ctx.config.scale_layout().to_string();
            ctx.model.scales_mut().set_scale_layout_by_name(&name);
        }
        _ => return false,
    }
    ctx.update_view_note_mapping();
    true
}

/// React on the deactivated items setting (former `registerDeactivatedItemsHandler`). Returns
/// true if it was the setting.
pub fn handle_deactivated_items_setting(ctx: &mut Ctx, id: SettingId) -> bool {
    if id != SettingId::EXCLUDE_DEACTIVATED_ITEMS {
        return false;
    }
    let exclude = ctx.config.are_deactivated_items_excluded();
    ctx.model.track_bank_mut().set_skip_disabled_items(exclude);
    true
}

/// Write the scale settings of the model to the configuration (former
/// `Scales.updateScaleProperties (Configuration)`).
pub fn update_scale_properties(ctx: &mut Ctx) {
    let properties = ctx.model.scales().scale_properties();
    if ctx.config.scale() != properties.scale {
        ctx.config.set_scale(properties.scale);
    }
    if ctx.config.scale_base() != properties.scale_base {
        ctx.config.set_scale_base(properties.scale_base);
    }
    if ctx.config.is_scale_in_key() != properties.scale_in_key {
        ctx.config.set_scale_in_key(properties.scale_in_key);
    }
    if ctx.config.scale_layout() != properties.scale_layout {
        ctx.config.set_scale_layout(properties.scale_layout);
    }
}

/// Replaces the active observer of the browser (former `activateBrowserObserver`): call
/// [`BrowserObserver::check_view`] or [`BrowserObserver::check_mode`] in `handle_changes`.
#[derive(Debug, Default, Clone, Copy)]
pub struct BrowserObserver {
    is_active: bool,
}

impl BrowserObserver {
    /// Returns the new state if the active state of the browser has changed.
    fn changed(&mut self, ctx: &Ctx) -> Option<bool> {
        let is_active = ctx.model.browser().is_some_and(|browser| browser.is_active());
        if is_active == self.is_active {
            return None;
        }
        self.is_active = is_active;
        Some(is_active)
    }

    /// De-/Activate the browser view depending on the browsers' active state (former
    /// `activateBrowserObserver (Views)`).
    pub fn check_view(&mut self, ctx: &mut Ctx, browser_view: Views) {
        let Some(is_active) = self.changed(ctx) else {
            return;
        };
        if is_active {
            let previous_view_id = ctx.views.previous_id();
            ctx.set_temporary_view(browser_view);
            if ctx.views.previous_id() == Some(Views::Shift) {
                ctx.views.set_previous_id(previous_view_id);
            }
        } else if ctx.views.is_active(browser_view) {
            ctx.restore_view();
        }
    }

    /// De-/Activate the browser mode depending on the browsers' active state (former
    /// `activateBrowserObserver (Modes)`).
    pub fn check_mode(&mut self, ctx: &mut Ctx, browser_mode: Modes) {
        let Some(is_active) = self.changed(ctx) else {
            return;
        };
        if is_active {
            ctx.set_temporary_mode(browser_mode);
        } else if ctx.modes.is_active(browser_mode) {
            ctx.restore_mode();
        }
    }
}

/// The track modes (former `Modes.isTrackMode`).
pub const TRACK_MODES: [Modes; 15] = [
    Modes::Track,
    Modes::Volume,
    Modes::Pan,
    Modes::Crossfader,
    Modes::Send,
    Modes::Send1,
    Modes::Send2,
    Modes::Send3,
    Modes::Send4,
    Modes::Send5,
    Modes::Send6,
    Modes::Send7,
    Modes::Send8,
    Modes::TrackDetails,
    Modes::Master,
];

/// Recall last used view if not in session, mix or project mode (former `recallLastView`).
pub fn recall_last_view(ctx: &mut Ctx) {
    if !ctx.views.is_any_active(&[Views::Session, Views::Mix, Views::Project]) {
        ctx.recall_preferred_view();
    }
}

/// Reset drum octave because the drum pad bank is also reset (former `resetDrumOctave`).
pub fn reset_drum_octave(ctx: &mut Ctx) {
    let Some(view) = ctx.active_view() else {
        return;
    };
    let Ok(mut view) = view.try_borrow_mut() else {
        return;
    };
    if ctx.views.is_any_active(&[Views::Drum, Views::Play]) {
        view.update_note_mapping(ctx);
    } else if ctx.views.is_active(Views::Drum64) {
        // Former instanceof AbstractDrum64View (all drum 64 views are registered as DRUM64)
        if let Some(transpose) = view.as_transpose_view_mut() {
            transpose.reset_octave(ctx);
        }
    }
}

/// Update the used mode (former `updateMode`).
pub fn update_mode(ctx: &mut Ctx) {
    if ctx.modes.is_active(Modes::Master) && !ctx.model.master_track().is_selected() {
        if ctx.modes.previous_id().is_some_and(|id| TRACK_MODES.contains(&id)) {
            ctx.restore_mode();
        } else {
            ctx.set_active_mode(Modes::Track);
        }
    }
}

/// Handle a track selection change (former `AbstractControllerSetup.handleTrackChange`).
pub fn handle_track_change(ctx: &mut Ctx, is_selected: bool) {
    if !is_selected {
        return;
    }
    // updateView
    recall_last_view(ctx);
    reset_drum_octave(ctx);
    update_mode(ctx);
}
