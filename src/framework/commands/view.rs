// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! View commands (former `framework.command.trigger.view`).

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::config::setting::SettingId;
use crate::daw::bank::BankItem;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Views};
use crate::midi::ButtonEvent;

/// Forwards the button event to the active view (former `ViewButtonCommand`).
pub struct ViewButtonCommand {
    button: ButtonId,
}

impl ViewButtonCommand {
    pub fn new(button: ButtonId) -> Self {
        ViewButtonCommand { button }
    }
}

impl TriggerCommand for ViewButtonCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if let Some(view) = ctx.active_view() {
            if let Ok(mut view) = view.try_borrow_mut() {
                view.on_button(ctx, self.button, event, velocity);
            }
        }
    }
}

/// Selects the next view of a list of views (former `ViewMultiSelectCommand`).
pub struct ViewMultiSelectCommand {
    pub view_ids: Vec<Views>,
    display_name: bool,
    trigger_event: ButtonEvent,
    store_preferred: bool,
}

impl ViewMultiSelectCommand {
    pub fn new(view_ids: &[Views]) -> Self {
        Self::with_options(view_ids, true, ButtonEvent::Down, false)
    }

    pub fn with_store_preferred(view_ids: &[Views], store_preferred: bool) -> Self {
        Self::with_options(view_ids, true, ButtonEvent::Down, store_preferred)
    }

    pub fn with_options(view_ids: &[Views], display_name: bool, trigger_event: ButtonEvent, store_preferred: bool) -> Self {
        ViewMultiSelectCommand { view_ids: view_ids.to_vec(), display_name, trigger_event, store_preferred }
    }

    /// Activate the next view of the list (former `executeNormal`).
    pub fn select_next(&self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != self.trigger_event || self.view_ids.is_empty() {
            return;
        }
        let active = ctx.views.active_id();
        let index = active.and_then(|id| self.view_ids.iter().position(|v| *v == id)).map(|i| i + 1).unwrap_or(0);
        let index = if index >= self.view_ids.len() { 0 } else { index };
        let view_id = self.view_ids[index];
        if ctx.views.is_active(view_id) {
            return;
        }
        if self.store_preferred {
            ctx.activate_preferred_view(view_id);
        } else {
            ctx.set_active_view(view_id);
        }
        if self.display_name {
            if let Some(view) = ctx.views.get(view_id) {
                let name = view.borrow().name().to_string();
                ctx.notify(&name);
            }
        }
    }
}

impl TriggerCommand for ViewMultiSelectCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.select_next(ctx, event);
    }
}

/// Selects the next play view of a list. Coming from a view which is not one of all the given
/// views, the preferred view of the track is restored (former `SelectPlayViewCommand`).
pub struct SelectPlayViewCommand {
    multi_select: ViewMultiSelectCommand,
    all_view_ids: HashSet<Views>,
}

impl SelectPlayViewCommand {
    pub fn new(view_ids: &[Views], all_view_ids: &[Views]) -> Self {
        SelectPlayViewCommand { multi_select: ViewMultiSelectCommand::new(view_ids), all_view_ids: all_view_ids.iter().copied().collect() }
    }
}

impl TriggerCommand for SelectPlayViewCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }

        // Restore the previous play view if coming from one not on the list
        if !ctx.views.active_id().is_some_and(|id| self.all_view_ids.contains(&id)) {
            ctx.recall_preferred_view();
            return;
        }

        self.multi_select.select_next(ctx, event);

        // Store the newly selected view for the current track
        let cursor_track = ctx.model.cursor_track();
        if cursor_track.does_exist() {
            let position = cursor_track.position();
            if let Some(active) = ctx.views.active_id() {
                ctx.views.set_preferred(position, active);
            }
        }
    }
}

/// The longest press of Shift which keeps the Shift view open (a tap), see [`install_shift_tap`].
pub const SHIFT_TAP_TIME: Duration = Duration::from_millis(250);

/// Toggles the Shift view (former `ToggleShiftViewCommand`).
#[derive(Default)]
pub struct ToggleShiftViewCommand {
    /// The On/Off setting whether the Shift view is shown while Shift is held (not in the Java
    /// version, which always shows it); none to always show it.
    hold_setting: Option<SettingId>,
}

impl ToggleShiftViewCommand {
    pub fn new() -> Self {
        ToggleShiftViewCommand { hold_setting: None }
    }

    /// The command with the setting whether the Shift view is shown while Shift is held.
    pub fn with_hold_setting(hold_setting: SettingId) -> Self {
        ToggleShiftViewCommand { hold_setting: Some(hold_setting) }
    }

    fn handle_on_down(&self, ctx: &mut Ctx) {
        if ctx.views.is_active(Views::Shift) {
            ctx.restore_view();
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
            ctx.surface.set_knob_sensitivity_is_slow(false);
            return;
        }

        // Do not trigger SHIFT view on button down if the active view is a session view since we
        // need this for the alternate clip/scene commands! The setting decides if there is one.
        if shows_shift_view_while_held(ctx, self.hold_setting) {
            ctx.set_temporary_view(Views::Shift);
        }
    }

    fn handle_up(ctx: &mut Ctx) {
        // If shift mode was already used, close it down otherwise leave it open for further
        // interaction
        if ctx.views.is_active(Views::Shift) && was_shift_view_used(ctx) {
            ctx.restore_view();
            return;
        }

        if is_session_alternative(ctx) && !ctx.views.is_active(Views::Shift) {
            if was_alternate_interaction_used(ctx) {
                set_alternate_interaction_used(ctx, false);
            } else {
                ctx.set_temporary_view(Views::Shift);
            }
        }
    }
}

impl TriggerCommand for ToggleShiftViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.surface.is_pressed(ButtonId::Alt) {
            return;
        }
        match event {
            ButtonEvent::Down => {
                ctx.surface.set_knob_sensitivity_is_slow(true);
                self.handle_on_down(ctx);
            }
            ButtonEvent::Long => {}
            ButtonEvent::Up => {
                ctx.surface.set_knob_sensitivity_is_slow(false);
                Self::handle_up(ctx);
            }
        }
    }
}

/// Is the Shift view shown while Shift is held? The On/Off setting if there is one, otherwise
/// not for a session alternative (former behaviour).
fn shows_shift_view_while_held(ctx: &Ctx, hold_setting: Option<SettingId>) -> bool {
    match hold_setting {
        Some(id) if ctx.config.is_setting_active(id) => ctx.config.custom_bool(id),
        _ => !is_session_alternative(ctx),
    }
}

/// The Shift view stays open only after a tap of Shift (not in the Java version, which also
/// keeps it open when Shift was held long or combined with a button): shorter than
/// [`SHIFT_TAP_TIME`] and no other button pressed in between. Otherwise the release closes it,
/// also when a combination consumed Shift (the handlers get the release of a consumed button).
/// If the Shift view is not shown while Shift is held (the setting of
/// [`ToggleShiftViewCommand::with_hold_setting`]), a tap opens it on release. Call after the
/// Shift button with [`ToggleShiftViewCommand`] was added.
pub fn install_shift_tap(ctx: &mut Ctx) {
    // When Shift was pressed, the button press count at that time and whether the press closed
    // the Shift view
    let shift_down: Rc<Cell<Option<(Instant, u64, bool)>>> = Rc::new(Cell::new(None));
    let down = shift_down.clone();
    ctx.surface.add_button_event_handler(
        ButtonId::Shift,
        ButtonEvent::Down,
        Rc::new(move |ctx: &mut Ctx, _| {
            let has_closed = ctx.surface.is_trigger_consumed(ButtonId::Shift);
            down.set(Some((ctx.host.now(), ctx.surface.button_press_count(), has_closed)));
        }),
    );
    ctx.surface.add_button_event_handler(
        ButtonId::Shift,
        ButtonEvent::Up,
        Rc::new(move |ctx: &mut Ctx, _| {
            let Some((time, count, has_closed)) = shift_down.take() else {
                return;
            };
            if has_closed {
                return;
            }
            let is_tap = ctx.host.now().duration_since(time) < SHIFT_TAP_TIME && ctx.surface.button_press_count() == count;
            let is_active = ctx.views.is_active(Views::Shift);
            if !is_tap {
                if is_active {
                    ctx.restore_view();
                }
            } else if !is_active && !is_session_alternative(ctx) {
                // Not shown while held (a session alternative opens it itself on release)
                ctx.set_temporary_view(Views::Shift);
            }
        }),
    );
}

/// Is the active view a session alternative (former `instanceof ISessionAlternative`)?
pub fn is_session_alternative(ctx: &Ctx) -> bool {
    ctx.active_view().is_some_and(|view| view.try_borrow().is_ok_and(|v| v.session_alternative().is_some()))
}

fn was_alternate_interaction_used(ctx: &Ctx) -> bool {
    ctx.active_view()
        .is_some_and(|view| view.try_borrow().is_ok_and(|v| v.session_alternative().is_some_and(|s| s.was_alternate_interaction_used())))
}

fn set_alternate_interaction_used(ctx: &Ctx, was_used: bool) {
    if let Some(view) = ctx.active_view() {
        if let Ok(mut view) = view.try_borrow_mut() {
            if let Some(alternative) = view.session_alternative_mut() {
                alternative.set_alternate_interaction_used(was_used);
            }
        }
    }
}

fn was_shift_view_used(ctx: &Ctx) -> bool {
    ctx.views.get(Views::Shift).is_some_and(|view| view.try_borrow().is_ok_and(|v| v.was_shift_view_used()))
}
