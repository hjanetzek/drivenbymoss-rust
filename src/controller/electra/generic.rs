// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Electra One as the generic provider of the surface link (doc/SURFACE_LINK.md §6.2,
//! electra-one `docs/surface-link.md` §6c; not part of Java): while a preset other than
//! DrivenByMoss's is active on a device which has the page view and the knob input (the Mk2
//! emulator), the surfaces get that preset's pages, and their input turns its knobs. The
//! firmware does all the value math (ranges, steps, overlays, formatters); this side only maps
//! the cells to knobs ([`super::page_view`]) and the input to the messages
//! ([`super::surface_ext`]).
//!
//! - **Model:** fetched with `02 52` on entering, on `7E 62` (the page view changed: Lua, a
//!   handle, a control set, page or preset switch) and `7E 06`. One request at a time; a change
//!   while one is open asks again after the answer. The values follow `7E 63`.
//! - **Input:**
//!
//! | Input | Message |
//! | --- | --- |
//! | `SlotTurn` | `14 53` for the cell's knob, after `09 50` if the cell's control set is not the active one. One link step is half a panel step (4 fine steps; a JAM strip across its length is about the whole range, as on the DrivenByMoss pages); `fine` is one fine step (1/8 panel step) |
//! | `SlotTouch` | `14 54` (after `09 50` as above) |
//! | `SlotPress` / `SlotRelease` | a pad: `14 55` with its maximum (momentary: the minimum on the release; toggle: the other one); a list: selects the slot and steps to the next item (`14 55 step 1`, the first after the last); other knobs: select the slot (`09 50`, `14 51`); a covered cell: selects its control's knob slot |
//! | `SelectedTurn` | as `SlotTurn` for the selected slot |
//! | `SelectedReset` | `14 55 default` |
//! | `ValueNext` / `ValuePrev` | `14 56 next/prev` for the selected slot's control, if it has several values (envelopes, XY pads) |
//! | `RowSelect` | `09 50` |
//! | `PageSelect`, `PageNext`, `PagePrev` | `09 0A [page id - 1]` |
//!
//! The DAW side (the DrivenByMoss modes) is inactive meanwhile: the controller is offline in
//! the Dummy mode, as without the generic provider.

use std::time::{Duration, Instant};

use super::definition::{sysex, SUBSCRIBED_EVENTS, SYSEX_RUNTIME_SWITCH_PAGE};
use super::page_view::{knob_slot_model, slot_knob, Cell, PageView};
use super::surface::{state, state_mut};
use super::surface_ext::{self as ext, Origin, Report, SetValue};
use crate::framework::ctx::Ctx;
use crate::framework::surface_link::{Input, PageModel, SlotKind, GRID_SIZE, NUM_SLOTS};

/// An open `02 52` without an answer for this long is given up.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(1000);
/// The fine steps of `14 53` per link step (8 are one panel step).
const FINE_STEPS_PER_STEP: i32 = 4;
/// How often an unanswered request is sent again.
const MAX_RETRIES: u32 = 3;

/// The page name while no page view arrived yet.
pub const WAITING_PAGE: &str = "…";

/// The state of the generic provider.
#[derive(Debug, Default)]
pub struct GenericProvider {
    /// True while the generic provider is active.
    pub active: bool,
    pub view: Option<PageView>,
    cells: Vec<Cell>,
    /// The active row (0..5) and the selected slot, as far as known.
    pub active_row: usize,
    pub selected: Option<usize>,
    /// An open request of the page view, and whether another change came meanwhile.
    request: Option<Instant>,
    again: bool,
    /// The requests sent again without an answer.
    retries: u32,
}

impl GenericProvider {
    /// The control and the value of the knob of a slot, if it is one.
    fn knob(&self, slot: usize) -> Option<(&super::page_view::ViewControl, &str)> {
        match (self.view.as_ref(), self.cells.get(slot)) {
            (Some(view), Some(Cell::Knob { control, value_id })) => Some((&view.controls[*control], value_id.as_str())),
            _ => None,
        }
    }

    /// The kind of a slot (`Empty` if it is no knob).
    fn kind(&self, slot: usize) -> SlotKind {
        self.knob(slot).map_or(SlotKind::Empty, |(control, value_id)| knob_slot_model(control, value_id).kind)
    }

    /// The slot of a control: its knob slot (of its active value), the first one otherwise.
    fn slot_of_control(&self, control_id: i32) -> Option<usize> {
        let view = self.view.as_ref()?;
        let index = view.controls.iter().position(|c| c.id == control_id)?;
        let mut first = None;
        for (slot, cell) in self.cells.iter().enumerate() {
            if let Cell::Knob { control, value_id } = cell {
                if *control == index {
                    if *value_id == view.controls[index].active_value {
                        return Some(slot);
                    }
                    first = first.or(Some(slot));
                }
            }
        }
        first
    }

    /// The model of the surfaces.
    pub fn model(&self) -> PageModel {
        match &self.view {
            Some(view) => view.to_model(&self.cells, self.active_row, self.selected),
            None => PageModel { pages: vec![WAITING_PAGE.to_string()], ..PageModel::default() },
        }
    }

    fn set_view(&mut self, view: PageView) {
        self.cells = view.cells();
        self.active_row = view.row;
        self.view = Some(view);
        let selected = self.view.as_ref().map_or(0, |v| v.selected_control);
        // Keep the own selection of a slot of the same control (e.g. one input of a Custom
        // control)
        if selected == 0 {
            self.selected = None;
        } else if self.selected.and_then(|s| self.knob(s)).is_none_or(|(c, _)| c.id != selected) {
            self.selected = self.slot_of_control(selected);
        }
    }
}

fn generic(ctx: &Ctx) -> &GenericProvider {
    &state(ctx).link.generic
}

fn generic_mut(ctx: &mut Ctx) -> &mut GenericProvider {
    &mut state_mut(ctx).link.generic
}

fn send(ctx: &Ctx, message: &[u8]) {
    state(ctx).ctrl_output.send_sysex(message);
}

pub fn is_active(ctx: &Ctx) -> bool {
    generic(ctx).active
}

/// Another preset than DrivenByMoss's is active: become its provider, if the device can.
/// Returns true if the generic provider is active.
pub fn enter(ctx: &mut Ctx) -> bool {
    if !state(ctx).link.capability.generic() || !state(ctx).link.provider.is_providing() {
        return false;
    }
    if !is_active(ctx) {
        ctx.host.println("Surface link: providing the pages of the active preset.");
        *generic_mut(ctx) = GenericProvider { active: true, ..GenericProvider::default() };
        send(ctx, &ext::subscribe_message(SUBSCRIBED_EVENTS | ext::SUBSCRIBE_CONTROL_SET, Some(true)));
    }
    request_view(ctx);
    true
}

/// The DrivenByMoss preset is back: stop the value changes.
pub fn leave(ctx: &mut Ctx) {
    if !is_active(ctx) {
        return;
    }
    ctx.host.println("Surface link: providing the DrivenByMoss pages again.");
    *generic_mut(ctx) = GenericProvider::default();
    send(ctx, &ext::subscribe_message(SUBSCRIBED_EVENTS | ext::SUBSCRIBE_CONTROL_SET, Some(false)));
}

/// Ask for the page view; if a request is open, ask again after its answer.
fn request_view(ctx: &mut Ctx) {
    let now = ctx.host.now();
    let provider = generic_mut(ctx);
    if provider.request.is_some_and(|time| now.saturating_duration_since(time) < REQUEST_TIMEOUT) {
        provider.again = true;
        return;
    }
    provider.request = Some(now);
    provider.again = false;
    send(ctx, &ext::page_view_request());
}

/// The tick: a request which got no answer is sent again (a few times).
pub fn tick(ctx: &mut Ctx) {
    let now = ctx.host.now();
    let provider = generic_mut(ctx);
    if provider.active && provider.retries < MAX_RETRIES && provider.request.is_some_and(|time| now.saturating_duration_since(time) >= REQUEST_TIMEOUT) {
        provider.retries += 1;
        request_view(ctx);
    }
}

/// A message of the device for the generic provider (while it is active).
pub fn on_report(ctx: &mut Ctx, report: Report) {
    match report {
        Report::PageView(json) => {
            let again = {
                let provider = generic_mut(ctx);
                provider.request = None;
                provider.retries = 0;
                provider.again
            };
            match PageView::parse(&json) {
                Some(view) => generic_mut(ctx).set_view(view),
                None => ctx.host.error("Surface link: the page view could not be read."),
            }
            if again {
                request_view(ctx);
            }
        }
        Report::PageViewChanged => request_view(ctx),
        Report::ValueChanged(change) => {
            if let Some(view) = generic_mut(ctx).view.as_mut() {
                view.set_value(change.control_id, &change.value_id, change.value, change.text.as_deref());
            }
        }
        // The own changes come back with origin 1, the device's and Lua's are adopted
        Report::Row { origin: Origin::Sysex, .. } | Report::Selection { origin: Origin::Sysex, .. } => {}
        Report::Row { row, .. } | Report::RowState(row) => generic_mut(ctx).active_row = row,
        Report::Selection { control_id, .. } | Report::SelectionState(control_id) => {
            let provider = generic_mut(ctx);
            provider.selected = if control_id == 0 { None } else { provider.slot_of_control(control_id) };
        }
        Report::ControlSet(set) => {
            let provider = generic_mut(ctx);
            if set < GRID_SIZE / 2 && provider.active_row / 2 != set {
                provider.active_row = set * 2;
            }
        }
        Report::Ack | Report::Nack => {}
    }
}

/// The device switched the page (`7E 06`) or the preset (`7E 02`).
pub fn on_switched(ctx: &mut Ctx) {
    let provider = generic_mut(ctx);
    provider.selected = None;
    provider.active_row = 0;
    request_view(ctx);
}

// ---------------------------------------------------------------------------------------------
// Input

/// Execute the input of a surface.
pub fn apply_input(ctx: &mut Ctx, input: Input) {
    match input {
        Input::SlotTurn { id, delta, fine, .. } => turn(ctx, id, delta, fine),
        Input::SlotTouch { id, on } => {
            if let Some(pot) = knob_of_turnable(ctx, id) {
                activate_row(ctx, id);
                send(ctx, &ext::touch_knob_message(pot, on));
            }
        }
        Input::SlotPress { id } => press(ctx, id, true),
        Input::SlotRelease { id } => press(ctx, id, false),
        Input::RowSelect { row } => set_row(ctx, row),
        Input::PageSelect { page } => select_page(ctx, page),
        Input::PageNext | Input::PagePrev | Input::PagePrevLast => {
            let current = generic(ctx).model().page;
            let page = if input == Input::PageNext { current + 1 } else { current.wrapping_sub(1) };
            select_page(ctx, page);
        }
        Input::SelectedTurn { delta, fine, .. } => {
            if let Some(id) = generic(ctx).selected {
                turn(ctx, id, delta, fine);
            }
        }
        Input::SelectedReset => {
            let provider = generic(ctx);
            let message = provider
                .selected
                .filter(|id| matches!(provider.kind(*id), SlotKind::Fader | SlotKind::List))
                .and_then(|id| provider.knob(id))
                .map(|(control, value_id)| ext::set_value_message(control.id, Some(value_id), SetValue::Default));
            if let Some(message) = message {
                send(ctx, &message);
            }
        }
        Input::ValueNext | Input::ValuePrev => {
            let step = if input == Input::ValueNext { 1 } else { -1 };
            let control = generic(ctx).selected.and_then(|id| generic(ctx).knob(id)).filter(|(c, _)| c.has_handles()).map(|(c, _)| c.id);
            if let Some(control) = control {
                send(ctx, &ext::step_active_value_message(control, step));
            }
        }
        // The host's input (since 1.2): nothing for these pages
        Input::ExtraTurn { .. } | Input::Shown { .. } | Input::SetSelect { .. } | Input::SetStep { .. } | Input::SectionSelect { .. } | Input::SlotAssign { .. } => {}
    }
}

/// The knob (1..12) of a slot which can be turned.
fn knob_of_turnable(ctx: &Ctx, id: usize) -> Option<u8> {
    generic(ctx).kind(id).is_turnable().then(|| slot_knob(id).1 as u8)
}

/// Make the row of a slot the active one, if its control set is not.
fn activate_row(ctx: &mut Ctx, id: usize) {
    let row = id / GRID_SIZE;
    if generic(ctx).active_row / 2 != row / 2 {
        set_row(ctx, row);
    }
}

fn turn(ctx: &mut Ctx, id: usize, delta: i32, fine: bool) {
    if delta == 0 || id >= NUM_SLOTS {
        return;
    }
    let Some(pot) = knob_of_turnable(ctx, id) else {
        return;
    };
    activate_row(ctx, id);
    let delta = if fine { delta.signum() } else { delta.saturating_mul(FINE_STEPS_PER_STEP) };
    send(ctx, &ext::turn_knob_message(pot, delta, true));
}

fn press(ctx: &mut Ctx, id: usize, pressed: bool) {
    let provider = generic(ctx);
    if let Some(Cell::Covered { control }) = provider.cells.get(id) {
        // Select the knob slot of the covering control
        let control_id = provider.view.as_ref().map_or(0, |v| v.controls[*control].id);
        if let Some(slot) = provider.slot_of_control(control_id).filter(|_| pressed) {
            select(ctx, slot);
        }
        return;
    }
    let Some((control, value_id)) = provider.knob(id) else {
        return;
    };
    let (control_id, value_id) = (control.id, value_id.to_string());
    let Some(value) = control.value(&value_id).cloned() else {
        return;
    };
    let kind = provider.kind(id);
    let message = match kind {
        SlotKind::Pad => {
            let momentary = control.mode != "toggle";
            let on = value.value > value.min;
            match (momentary, pressed) {
                (true, _) => Some(SetValue::Value(if pressed { value.max } else { value.min } as i64)),
                (false, true) => Some(SetValue::Value(if on { value.min } else { value.max } as i64)),
                (false, false) => None,
            }
        }
        SlotKind::List if pressed => {
            select(ctx, id);
            let last = value.items.len().saturating_sub(1);
            match value.items.first() {
                Some((first, _)) if value.value as usize >= last => Some(SetValue::Value(*first)),
                _ => Some(SetValue::Step(1)),
            }
        }
        SlotKind::Fader | SlotKind::Relative if pressed => {
            select(ctx, id);
            None
        }
        _ => None,
    };
    if let Some(set) = message {
        send(ctx, &ext::set_value_message(control_id, Some(&value_id), set));
    }
}

/// Select a slot (and its row) on the device.
fn select(ctx: &mut Ctx, id: usize) {
    set_row(ctx, id / GRID_SIZE);
    let Some(control_id) = generic(ctx).knob(id).map(|(c, _)| c.id) else {
        return;
    };
    let provider = generic_mut(ctx);
    if provider.selected == Some(id) {
        return;
    }
    provider.selected = Some(id);
    if let Some(message) = ext::set_selection_message(state(ctx).link.capability, control_id) {
        send(ctx, &message);
    }
}

fn set_row(ctx: &mut Ctx, row: usize) {
    if row >= GRID_SIZE || generic(ctx).active_row == row {
        return;
    }
    let old = generic(ctx).active_row;
    generic_mut(ctx).active_row = row;
    for message in ext::set_row_messages(state(ctx).link.capability, old, row) {
        send(ctx, &message);
    }
}

/// A page of the surfaces' list (the shown pages).
fn select_page(ctx: &mut Ctx, page: usize) {
    let Some(id) = generic(ctx).view.as_ref().and_then(|v| v.shown_pages().get(page).map(|(id, _)| *id)) else {
        return;
    };
    send(ctx, &sysex(&SYSEX_RUNTIME_SWITCH_PAGE, &[(id.saturating_sub(1) & 0x7F) as u8]));
    // The device resets the row and the selection and announces the new page view
    let provider = generic_mut(ctx);
    provider.active_row = 0;
    provider.selected = None;
}
