// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The views of the Maschine JAM.
//!
//! The Java interface `IViewNavigation.canScroll` is mapped to [`View::scroll_states`], the
//! interface `IMaschineView` to [`MaschineView`] (see [`change_option`]).

pub mod accent_view;
pub mod drum_views;
pub mod note_repeat_view;
pub mod play_views;
pub mod sequencer_views;
pub mod session_view;
pub mod shift_view;
pub mod utility_views;

use crate::controller::maschine::core::encoder_mode_manager::EncoderMode;
use crate::framework::commands::cursor::Direction;
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::scale::Scales;

/// Former `IMaschineView`: a view whose options can be changed with the encoder while one of the
/// Perform, Notes, Lock or Tune buttons is pressed.
pub trait MaschineView {
    fn change_option(&mut self, ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32);
}

/// Get the view as a [`MaschineView`] if it is one (former `instanceof IMaschineView`).
pub fn as_maschine_view(view: &mut dyn View) -> Option<&mut dyn MaschineView> {
    let any = view.as_any_mut();
    if any.is::<play_views::PlayView>() {
        return any.downcast_mut::<play_views::PlayView>().map(|v| v as &mut dyn MaschineView);
    }
    if any.is::<play_views::ChordsView>() {
        return any.downcast_mut::<play_views::ChordsView>().map(|v| v as &mut dyn MaschineView);
    }
    if any.is::<drum_views::DrumView>() {
        return any.downcast_mut::<drum_views::DrumView>().map(|v| v as &mut dyn MaschineView);
    }
    if any.is::<drum_views::DrumLaneView>() {
        return any.downcast_mut::<drum_views::DrumLaneView>().map(|v| v as &mut dyn MaschineView);
    }
    if any.is::<sequencer_views::SequencerView>() {
        return any.downcast_mut::<sequencer_views::SequencerView>().map(|v| v as &mut dyn MaschineView);
    }
    if any.is::<accent_view::AccentView>() {
        return any.downcast_mut::<accent_view::AccentView>().map(|v| v as &mut dyn MaschineView);
    }
    None
}

/// Call `change_option` on the active view if it is a [`MaschineView`] (the handler of the
/// encoder mode manager).
pub fn change_option(ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, control: i32) {
    let Some(view) = ctx.active_view() else {
        return;
    };
    let Ok(mut view) = view.try_borrow_mut() else {
        return;
    };
    if let Some(maschine_view) = as_maschine_view(&mut *view) {
        maschine_view.change_option(ctx, temporary_encoder_mode, control);
    }
}

/// Former `IViewNavigation.canScroll` of the active view.
pub fn can_scroll(ctx: &Ctx, direction: Direction) -> bool {
    let Some(view) = ctx.active_view() else {
        return false;
    };
    let Ok(view) = view.try_borrow() else {
        return false;
    };
    view.scroll_states(ctx).is_some_and(|states| crate::framework::commands::cursor::can_scroll(&states, direction))
}

/// Former `Scales.updateScaleProperties (configuration)`: store the scale settings in the
/// configuration.
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

/// The scale changes of the play, chord and sequencer views for the temporary encoder modes
/// Perform, Notes and Lock (Tune is view specific). Returns false if the mode is not handled.
pub fn change_scale_option(ctx: &mut Ctx, temporary_encoder_mode: EncoderMode, increase: bool) -> bool {
    match temporary_encoder_mode {
        EncoderMode::TemporaryPerform => {
            if increase {
                ctx.model.scales_mut().next_scale();
            } else {
                ctx.model.scales_mut().prev_scale();
            }
            delay_display(ctx, |ctx| format!("Scale: {}", ctx.model.scales().scale().name()));
        }
        EncoderMode::TemporaryNotes => {
            if increase {
                ctx.model.scales_mut().next_scale_offset();
            } else {
                ctx.model.scales_mut().prev_scale_offset();
            }
            delay_display(ctx, |ctx| format!("Scale Offset: {}", Scales::BASES[ctx.model.scales().scale_offset_index()]));
        }
        EncoderMode::TemporaryLock => {
            ctx.model.scales_mut().toggle_chromatic();
            delay_display(ctx, |ctx| format!("Chromatic: {}", if ctx.model.scales().is_chromatic() { "On" } else { "Off" }));
        }
        _ => return false,
    }
    true
}

/// Scroll states for views which can only scroll up and down.
pub fn up_down_scroll_states(can_scroll_up: bool, can_scroll_down: bool) -> ScrollStates {
    ScrollStates { can_scroll_left: false, can_scroll_right: false, can_scroll_up, can_scroll_down }
}
