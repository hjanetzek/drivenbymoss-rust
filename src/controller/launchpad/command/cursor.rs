// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cursor keys of the Launchpad (former `LaunchpadCursorCommand`).
//!
//! Differences to Java:
//! - The edit page of a sequencer is not notified after scrolling (`mvHelper.notifyEditPage`);
//!   the clip belongs to the view, which can notify it itself in `on_sequencer_left/right`.
//! - The User view functions (previous/next page, set mode) are reached via downcasting to the
//!   Launchpad `UserView`.

use crate::daw::constants::GrooveParameterId;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::cursor::{can_scroll, CursorCommand, Direction, ScrollStates};
use crate::framework::commands::mv_helper;
use crate::controller::launchpad::view::user_view::UserView;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::midi::ButtonEvent;

/// The interval in which the tempo, shuffle and zoom is changed while the button is held.
const REPEAT_SPEED: u64 = 300;

/// The cursor command of the Launchpad, which handles the cursor keys depending on the active
/// view.
pub struct LaunchpadCursorCommand {
    cursor: CursorCommand,
}

impl LaunchpadCursorCommand {
    pub fn new(direction: Direction) -> Self {
        LaunchpadCursorCommand { cursor: CursorCommand::new(direction, false) }
    }

    pub fn direction(&self) -> Direction {
        self.cursor.direction
    }

    /// Can the active view be scrolled into the direction of the command (former `canScroll`)?
    pub fn can_scroll(ctx: &Ctx, direction: Direction) -> bool {
        can_scroll(&Self::arrow_states(ctx), direction)
    }

    /// The scroll states of the active view, all false if it is not scrollable (former
    /// `updateArrowStates`).
    pub fn arrow_states(ctx: &Ctx) -> ScrollStates {
        ctx.active_view().and_then(|view| view.try_borrow().ok().and_then(|v| v.scroll_states(ctx))).unwrap_or_default()
    }

    /// The button event on which the command is triggered.
    fn trigger_event(ctx: &Ctx) -> ButtonEvent {
        if ctx.views.is_any_active(&[Views::Drum, Views::Sequencer, Views::PolySequencer, Views::Drum4, Views::Drum8]) {
            ButtonEvent::Up
        } else {
            ButtonEvent::Down
        }
    }

    fn scroll_left(&self, ctx: &mut Ctx) {
        let Some(active) = ctx.views.active_id() else {
            return;
        };
        match active {
            Views::User => {
                with_user_view(ctx, |view, ctx| view.select_previous_page(ctx));
            }
            Views::Shift => ctx.set_temporary_view(Views::Shuffle),
            Views::Play | Views::Chords => change_scale(ctx, false),
            Views::Sequencer | Views::Raindrops | Views::PolySequencer | Views::Drum | Views::Drum4 | Views::Drum8 => {
                with_active_view(ctx, |view, ctx| view.on_sequencer_left(ctx, ButtonEvent::Down));
            }
            Views::Device => {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    bank.scroll_backwards();
                }
                mv_helper::notify_selected_parameter_page(ctx);
            }
            Views::Browser => {
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.previous_content_type();
                }
            }
            Views::Session => {
                if !scroll_volume_mode(ctx, false) {
                    return;
                }
                ctx.surface.set_trigger_consumed(ButtonId::Shift);
                mv_helper::notify_selected_track(ctx);
            }
            Views::TrackVolume | Views::TrackPan | Views::TrackSends | Views::Mix => {
                with_mode(ctx, Modes::Volume, |mode, ctx| mode.select_previous_item(ctx));
            }
            Views::Shuffle => {
                if !ctx.surface.is_pressed(ButtonId::Right) {
                    trigger_change_shuffle(ctx, -10, ButtonId::Left);
                }
            }
            Views::Tempo => {
                if !ctx.surface.is_pressed(ButtonId::Right) {
                    trigger_change_tempo(ctx, -10, ButtonId::Left);
                }
            }
            Views::Project => trigger_change_zoom(ctx, false),
            // Not used: Control, Piano, Drum64, ClipLength, NoteEditView, ScenePlay
            _ => {}
        }
    }

    fn scroll_right(&self, ctx: &mut Ctx) {
        let Some(active) = ctx.views.active_id() else {
            return;
        };
        match active {
            Views::User => {
                with_user_view(ctx, |view, ctx| view.select_next_page(ctx));
            }
            Views::Shift => ctx.set_temporary_view(Views::Shuffle),
            Views::Play | Views::Chords => change_scale(ctx, true),
            Views::Sequencer | Views::Raindrops | Views::PolySequencer | Views::Drum | Views::Drum4 | Views::Drum8 => {
                with_active_view(ctx, |view, ctx| view.on_sequencer_right(ctx, ButtonEvent::Down));
            }
            Views::Device => {
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    bank.scroll_forwards();
                }
                mv_helper::notify_selected_parameter_page(ctx);
            }
            Views::Browser => {
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.next_content_type();
                }
            }
            Views::Session => {
                if !scroll_volume_mode(ctx, true) {
                    return;
                }
                ctx.surface.set_trigger_consumed(ButtonId::Shift);
                mv_helper::notify_selected_track(ctx);
            }
            Views::TrackVolume | Views::TrackPan | Views::TrackSends | Views::Mix => {
                with_mode(ctx, Modes::Volume, |mode, ctx| mode.select_next_item(ctx));
            }
            Views::Shuffle => {
                if !ctx.surface.is_pressed(ButtonId::Left) {
                    trigger_change_shuffle(ctx, 10, ButtonId::Right);
                }
            }
            Views::Tempo => {
                if !ctx.surface.is_pressed(ButtonId::Left) {
                    trigger_change_tempo(ctx, 10, ButtonId::Right);
                }
            }
            Views::Project => trigger_change_zoom(ctx, true),
            // Not used: Control, Piano, Drum64, ClipLength, NoteEditView, ScenePlay
            _ => {}
        }
    }

    fn scroll_up(&self, ctx: &mut Ctx) {
        let Some(active) = ctx.views.active_id() else {
            return;
        };
        match active {
            Views::User => {
                with_user_view(ctx, |view, ctx| view.set_mode(ctx, true));
            }
            Views::Shift => ctx.set_temporary_view(Views::Tempo),
            Views::Play
            | Views::Chords
            | Views::Piano
            | Views::Drum64
            | Views::Drum
            | Views::Sequencer
            | Views::Raindrops
            | Views::PolySequencer
            | Views::Drum4
            | Views::Drum8 => with_active_view(ctx, |view, ctx| {
                if let Some(transpose) = view.as_transpose_view_mut() {
                    transpose.on_octave_up(ctx, ButtonEvent::Down);
                }
            }),
            Views::Device => {
                ctx.model.cursor_device_mut().select_previous();
                mv_helper::notify_selected_device(ctx);
            }
            Views::Browser => insert_device(ctx, true),
            Views::Session => {
                if is_birds_eye_active(ctx) {
                    ctx.model.current_track_bank_mut().scene_bank_mut().select_previous_page();
                } else {
                    self.cursor.scroll_up(ctx);
                }
                ctx.surface.set_trigger_consumed(ButtonId::Shift);
            }
            Views::TrackVolume | Views::TrackPan | Views::TrackSends | Views::Mix => self.cursor.scroll_up(ctx),
            Views::Shuffle => {
                if !ctx.surface.is_pressed(ButtonId::Down) {
                    trigger_change_shuffle(ctx, 1, ButtonId::Up);
                }
            }
            Views::Tempo => {
                if !ctx.surface.is_pressed(ButtonId::Down) {
                    trigger_change_tempo(ctx, 1, ButtonId::Up);
                }
            }
            Views::Project => ctx.model.application_mut().inc_track_height(),
            // Not used: Control, ClipLength, NoteEditView, ScenePlay
            _ => {}
        }
    }

    fn scroll_down(&self, ctx: &mut Ctx) {
        let Some(active) = ctx.views.active_id() else {
            return;
        };
        match active {
            Views::User => {
                with_user_view(ctx, |view, ctx| view.set_mode(ctx, false));
            }
            Views::Shift => ctx.set_temporary_view(Views::Tempo),
            Views::Play
            | Views::Chords
            | Views::Piano
            | Views::Drum64
            | Views::Drum
            | Views::Sequencer
            | Views::Raindrops
            | Views::PolySequencer
            | Views::Drum4
            | Views::Drum8 => with_active_view(ctx, |view, ctx| {
                if let Some(transpose) = view.as_transpose_view_mut() {
                    transpose.on_octave_down(ctx, ButtonEvent::Down);
                }
            }),
            Views::Device => {
                ctx.model.cursor_device_mut().select_next();
                mv_helper::notify_selected_device(ctx);
            }
            Views::Browser => insert_device(ctx, false),
            Views::Session => {
                if is_birds_eye_active(ctx) {
                    ctx.model.current_track_bank_mut().scene_bank_mut().select_next_page();
                } else {
                    self.cursor.scroll_down(ctx);
                }
                ctx.surface.set_trigger_consumed(ButtonId::Shift);
            }
            Views::TrackVolume | Views::TrackPan | Views::TrackSends | Views::Mix => self.cursor.scroll_down(ctx),
            Views::Shuffle => {
                if !ctx.surface.is_pressed(ButtonId::Up) {
                    trigger_change_shuffle(ctx, -1, ButtonId::Down);
                }
            }
            Views::Tempo => {
                if !ctx.surface.is_pressed(ButtonId::Up) {
                    trigger_change_tempo(ctx, -1, ButtonId::Down);
                }
            }
            Views::Project => ctx.model.application_mut().dec_track_height(),
            // Not used: Control, ClipLength, NoteEditView, ScenePlay
            _ => {}
        }
    }
}

impl TriggerCommand for LaunchpadCursorCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != Self::trigger_event(ctx) {
            return;
        }
        match self.cursor.direction {
            Direction::Left => self.scroll_left(ctx),
            Direction::Right => self.scroll_right(ctx),
            Direction::Up => self.scroll_up(ctx),
            Direction::Down => self.scroll_down(ctx),
        }
        // notifySelection is false for the Launchpad
    }
}

/// Call the function with the active view (if it is not borrowed).
fn with_active_view(ctx: &mut Ctx, f: impl FnOnce(&mut dyn crate::framework::feature::View, &mut Ctx)) {
    if let Some(view) = ctx.active_view() {
        if let Ok(mut view) = view.try_borrow_mut() {
            f(&mut *view, ctx);
        }
    }
}

/// Call the function with the Launchpad user view (if it is not borrowed).
fn with_user_view(ctx: &mut Ctx, f: impl FnOnce(&mut UserView, &mut Ctx)) {
    let Some(view) = ctx.views.get(Views::User) else {
        return;
    };
    let Ok(mut view) = view.try_borrow_mut() else {
        return;
    };
    if let Some(user_view) = view.as_any_mut().downcast_mut::<UserView>() {
        f(user_view, ctx);
    }
}

/// Call the function with a registered mode (if it is not borrowed).
fn with_mode(ctx: &mut Ctx, id: Modes, f: impl FnOnce(&mut dyn crate::framework::feature::Mode, &mut Ctx)) -> bool {
    let Some(mode) = ctx.modes.get(id) else {
        return false;
    };
    let Ok(mut mode) = mode.try_borrow_mut() else {
        return false;
    };
    f(&mut *mode, ctx);
    true
}

fn is_birds_eye_active(ctx: &Ctx) -> bool {
    ctx.views.get(Views::Session).is_some_and(|view| view.try_borrow().is_ok_and(|v| v.is_birds_eye_active()))
}

/// Select the previous/next track (or page in birds-eye view) with the volume mode. Returns
/// false if there is no volume mode.
fn scroll_volume_mode(ctx: &mut Ctx, is_next: bool) -> bool {
    let birds_eye = is_birds_eye_active(ctx);
    with_mode(ctx, Modes::Volume, |mode, ctx| match (birds_eye, is_next) {
        (true, true) => mode.select_next_item_page(ctx),
        (true, false) => mode.select_previous_item_page(ctx),
        (false, true) => mode.select_next_item(ctx),
        (false, false) => mode.select_previous_item(ctx),
    })
}

/// Select the previous/next scale and store it in the configuration.
fn change_scale(ctx: &mut Ctx, is_next: bool) {
    let scales = ctx.model.scales_mut();
    if is_next {
        scales.next_scale();
    } else {
        scales.prev_scale();
    }
    let name = ctx.model.scales().scale().name();
    ctx.config.set_scale(name);
    ctx.notify(name);
}

/// Browse for a device to insert before/after the cursor device.
fn insert_device(ctx: &mut Ctx, before: bool) {
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return;
    }
    let (name, position) = (device.name().to_string(), device.position());
    if let Some(browser) = ctx.model.browser_mut() {
        if before {
            browser.insert_before_cursor_device(&name, position);
        } else {
            browser.insert_after_cursor_device(&name, position);
        }
    }
}

/// Change the tempo as long as the button is pressed.
fn trigger_change_tempo(ctx: &mut Ctx, amount: i32, button: ButtonId) {
    if !ctx.surface.is_pressed(button) {
        return;
    }
    let transport = ctx.model.transport_mut();
    let tempo = transport.tempo();
    transport.set_tempo(tempo + amount as f64);
    ctx.schedule(REPEAT_SPEED, move |ctx| trigger_change_tempo(ctx, amount, button));
}

/// Change the shuffle amount as long as the button is pressed.
fn trigger_change_shuffle(ctx: &mut Ctx, amount: i32, button: ButtonId) {
    if !ctx.surface.is_pressed(button) {
        return;
    }
    let max = ctx.model.value_changer().upper_bound() - 1;
    let a = (amount as f64 * max as f64 / 100.0).round() as i32;
    let parameter = ctx.model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount);
    let value = parameter.value();
    parameter.set_value(max.min(value + a));
    ctx.schedule(REPEAT_SPEED, move |ctx| trigger_change_shuffle(ctx, amount, button));
}

/// Zoom in/out as long as the button is pressed (former `triggerChangeZoom1`).
fn trigger_change_zoom(ctx: &mut Ctx, zoom_in: bool) {
    if !ctx.surface.is_pressed(if zoom_in { ButtonId::Right } else { ButtonId::Left }) {
        return;
    }
    let application = ctx.model.application_mut();
    if zoom_in {
        application.zoom_in();
    } else {
        application.zoom_out();
    }
    ctx.schedule(REPEAT_SPEED, move |ctx| trigger_change_zoom(ctx, zoom_in));
}
