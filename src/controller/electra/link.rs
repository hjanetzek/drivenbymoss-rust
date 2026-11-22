// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Electra One as the provider of the surface link (doc/SURFACE_LINK.md; not part of Java):
//! the page of the active mode (its page cache and the controls of the preset) is the page
//! model, and the input of the surfaces is executed as if it came from the Electra's own knobs,
//! pads and touches (the MIDI of Port 1 and the pot touch of CTRL), so that the Electra shows
//! the change too.
//!
//! The active row and the selected slot are state of this provider; the device learns about
//! them through [`super::surface_ext`] (optional: a real Electra only switches the control set).
//! Only the normal model provides; the Mini has another grid.

use super::definition::{ctrl_cc, ElectraVariant, ELECTRA_MIDI_CHANNEL, ELECTRA_ROW_1, SUBSCRIBED_EVENTS};
use super::mode::with_normal_mode;
use super::preset_layout::{PAGES, PAGE_NAMES};
use super::surface::{self as electra, state, state_mut};
use super::surface_ext::{self as ext, Capability, Origin, Pending, Report};
use crate::framework::ctx::Ctx;
use crate::framework::parameterprovider::ParamRef;
use crate::framework::surface_link::model::GRID_SIZE;
use crate::framework::surface_link::provider::LinkProvider;
use crate::framework::surface_link::{Input, PageModel, ProviderEvent, Rgb, Slot, SlotKind, DEFAULT_LINK, NUM_SLOTS};
use crate::json::JsonValue;

/// The name of the provider, shown by the surfaces.
pub const PROVIDER_NAME: &str = "Electra One";
/// The page model while the DrivenByMoss preset is not selected.
pub const OFFLINE_PAGE: &str = "Offline";

/// The provider state of an Electra instance.
pub struct ElectraLink {
    pub provider: LinkProvider,
    /// The row which the encoders of the surfaces drive (0..5).
    pub active_row: usize,
    /// The selected slot (0..35).
    pub selected: Option<usize>,
    /// What the device can do beyond the standard API.
    pub capability: Capability,
    pending: Pending,
}

impl ElectraLink {
    /// Register as the provider (the normal model only). If another instance provides already,
    /// this one works without.
    pub fn new(variant: ElectraVariant, host: &crate::host::Host) -> Self {
        let provider = match variant {
            ElectraVariant::Normal => match LinkProvider::new(DEFAULT_LINK, PROVIDER_NAME) {
                Ok(provider) => provider,
                Err(_) => {
                    host.println("Surface link: another controller provides the pages already.");
                    LinkProvider::inactive()
                }
            },
            ElectraVariant::Mini => LinkProvider::inactive(),
        };
        ElectraLink { provider, active_row: 0, selected: None, capability: Capability::default(), pending: Pending::default() }
    }
}

fn link(ctx: &Ctx) -> &ElectraLink {
    &state(ctx).link
}

fn link_mut(ctx: &mut Ctx) -> &mut ElectraLink {
    &mut state_mut(ctx).link
}

/// The page of the active mode, `None` while offline (Dummy mode).
fn active_page(ctx: &Ctx) -> Option<usize> {
    if !electra::is_online(ctx) {
        return None;
    }
    let mode = ctx.modes.active_id()?;
    state(ctx).variant.modes().iter().position(|m| *m == mode)
}

/// The control ID on the device of a slot of the current page.
fn control_id(page: usize, slot: usize) -> i32 {
    (page * NUM_SLOTS + slot + 1) as i32
}

// ---------------------------------------------------------------------------------------------
// The tick

/// Drain the input of the surfaces and execute it (in `handle_changes`).
pub fn poll(ctx: &mut Ctx) {
    if !link(ctx).provider.is_providing() {
        return;
    }
    let events = link_mut(ctx).provider.poll();
    for event in events {
        match event {
            ProviderEvent::Input { input, .. } => apply_input(ctx, input),
            ProviderEvent::Attached { .. } => ctx.host.println("Surface link: a surface attached."),
            ProviderEvent::Detached { .. } => ctx.host.println("Surface link: a surface detached."),
            ProviderEvent::Refused { .. } => ctx.host.println("Surface link: refused a surface with another protocol version."),
        }
    }
}

/// Update the page model from the active mode and send the changes (after the flush of the
/// modes).
pub fn publish(ctx: &mut Ctx) {
    if !link(ctx).provider.is_providing() {
        return;
    }
    let model = build_model(ctx);
    let provider = &mut link_mut(ctx).provider;
    *provider.model_mut() = model;
    provider.publish();
}

/// The page model of the active page.
pub fn build_model(ctx: &Ctx) -> PageModel {
    let link = link(ctx);
    let Some(page) = active_page(ctx) else {
        return PageModel { pages: vec![OFFLINE_PAGE.to_string()], ..PageModel::default() };
    };
    let mut model = PageModel {
        page,
        pages: PAGE_NAMES.iter().map(|n| n.to_string()).collect(),
        active_row: link.active_row,
        selected_slot: link.selected,
        ..PageModel::default()
    };
    let Some(mode) = ctx.active_mode() else {
        return model;
    };
    let Ok(mode) = mode.try_borrow() else {
        return model;
    };
    with_normal_mode(&*mode, |base| {
        let cache = &base.page_cache;
        for (id, preset) in PAGES[page].iter().enumerate() {
            let control = cache.control(id / GRID_SIZE, id % GRID_SIZE);
            let text = |t: Option<&str>| t.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
            let enabled = control.exists.unwrap_or(true);
            let has_value = matches!(preset.kind, SlotKind::Fader | SlotKind::List);
            model.slots[id] = Slot {
                kind: preset.kind,
                name: text(control.label).unwrap_or_else(|| if enabled { preset.name.to_string() } else { String::new() }),
                color: control.color.map(Rgb::from).unwrap_or_else(|| Rgb::from_hex(preset.color)),
                value: if has_value { control.value.clamp(0, 127) as f32 / 127.0 } else { 0.0 },
                value_text: text(control.value_label).unwrap_or_default(),
                steps: preset.steps,
                on: false,
                enabled,
            };
        }
    });
    model
}

// ---------------------------------------------------------------------------------------------
// Input

/// Execute the input of a surface as if it came from the Electra.
pub fn apply_input(ctx: &mut Ctx, input: Input) {
    let Some(page) = active_page(ctx) else {
        return;
    };
    let kind = |id: usize| PAGES[page].get(id).map_or(SlotKind::Empty, |c| c.kind);
    match input {
        Input::SlotTurn { id, delta, fine } => turn(ctx, page, id, delta, fine),
        Input::SlotPress { id } => match kind(id) {
            SlotKind::Pad => press_pad(ctx, id, true),
            SlotKind::Empty => {}
            slot_kind => {
                select(ctx, page, Some(id));
                if slot_kind == SlotKind::List {
                    turn(ctx, page, id, 1, false);
                }
            }
        },
        Input::SlotRelease { id } => {
            if kind(id) == SlotKind::Pad {
                press_pad(ctx, id, false);
            }
        }
        Input::SlotTouch { id, on } => {
            if kind(id).is_turnable() {
                electra::set_editing(ctx, control_id(page, id), on);
            }
        }
        Input::RowSelect { row } => set_row(ctx, row),
        Input::PageSelect { page: new_page } => select_page(ctx, new_page),
        Input::PageNext => select_page(ctx, page + 1),
        Input::PagePrev => {
            if page > 0 {
                select_page(ctx, page - 1);
            }
        }
        Input::SelectedTurn { delta, fine } => {
            if let Some(id) = link(ctx).selected {
                turn(ctx, page, id, delta, fine);
            }
        }
        Input::SelectedReset => {
            if let Some(id) = link(ctx).selected.filter(|id| kind(*id).is_turnable()) {
                reset(ctx, id);
            }
        }
        // The Electra pages have no extras
        Input::ExtraTurn { .. } => {}
    }
}

/// Turn the knob of a slot: the relative CC of the Electra's knob (two's complement), with
/// `fine` in single steps and the slow sensitivity, as the Electra's emulated Shift.
fn turn(ctx: &mut Ctx, page: usize, id: usize, delta: i32, fine: bool) {
    if delta == 0 || !PAGES[page].get(id).is_some_and(|c| c.kind.is_turnable()) {
        return;
    }
    let delta = if fine { delta.signum() } else { delta.clamp(-63, 63) };
    let value = if delta < 0 { 128 + delta } else { delta };
    let was_slow = ctx.surface.is_knob_sensitivity_slow();
    if fine {
        ctx.surface.set_knob_sensitivity_is_slow(true);
    }
    ctx.handle_midi(0xB0 | ELECTRA_MIDI_CHANNEL, ctrl_cc(id / GRID_SIZE, id % GRID_SIZE), value);
    ctx.surface.set_knob_sensitivity_is_slow(was_slow);
}

/// A pad of a slot: its CC with 127 (pressed) or 0 (released).
fn press_pad(ctx: &mut Ctx, id: usize, pressed: bool) {
    let cc = ELECTRA_ROW_1 + 10 * (id / GRID_SIZE) as i32 + (id % GRID_SIZE) as i32;
    ctx.handle_midi(0xB0 | ELECTRA_MIDI_CHANNEL, cc, if pressed { 127 } else { 0 });
}

/// Reset the parameter of a slot (the Electra itself has no gesture for it).
fn reset(ctx: &mut Ctx, id: usize) {
    let Some(mode) = ctx.active_mode() else {
        return;
    };
    let parameter: Option<ParamRef> = mode.try_borrow().ok().and_then(|mode| with_normal_mode(&*mode, |base| base.base.parameter(ctx, id)));
    if let Some(parameter) = parameter {
        parameter.reset_value(ctx);
    }
}

/// Switch to a page: the mode at once, the device with `09 0A`. The device resets the row and
/// the selection, so does the provider.
fn select_page(ctx: &mut Ctx, page: usize) {
    let modes = state(ctx).variant.modes();
    let Some(&mode) = modes.get(page) else {
        return;
    };
    ctx.set_active_mode(mode);
    electra::select_page(ctx, page as i32);
    reset_row_and_selection(ctx);
}

/// The row and the selection after a page switch.
fn reset_row_and_selection(ctx: &mut Ctx) {
    let link = link_mut(ctx);
    link.active_row = 0;
    link.selected = None;
    link.pending = Pending::default();
}

/// Make a row the active one and tell the device.
pub fn set_row(ctx: &mut Ctx, row: usize) {
    if row >= GRID_SIZE {
        return;
    }
    let link = link_mut(ctx);
    let old = link.active_row;
    if old == row {
        return;
    }
    link.active_row = row;
    let messages = ext::set_row_messages(link.capability, old, row);
    if link.capability.rows {
        link.pending.row.get_or_insert(old);
    }
    for message in messages {
        state(ctx).ctrl_output.send_sysex(&message);
    }
}

/// Select a slot (and its row) and tell the device.
fn select(ctx: &mut Ctx, page: usize, slot: Option<usize>) {
    if let Some(id) = slot {
        set_row(ctx, id / GRID_SIZE);
    }
    let link = link_mut(ctx);
    let old = link.selected;
    if old == slot {
        return;
    }
    link.selected = slot;
    if let Some(message) = ext::set_selection_message(link.capability, slot.map_or(0, |id| control_id(page, id))) {
        link.pending.selection.get_or_insert(old);
        state(ctx).ctrl_output.send_sysex(&message);
    }
}

// ---------------------------------------------------------------------------------------------
// The device

/// The device information arrived: read the capability. Returns the events to subscribe to.
pub fn on_device_info(ctx: &mut Ctx, info: &JsonValue) -> u8 {
    let capability = if state(ctx).variant == ElectraVariant::Normal { Capability::from_device_info(info) } else { Capability::default() };
    link_mut(ctx).capability = capability;
    if capability.any() {
        ctx.host.println("Surface link: the Electra shows the active row and the selection.");
        // The row and selection events come with the control set events
        SUBSCRIBED_EVENTS | ext::SUBSCRIBE_CONTROL_SET
    } else {
        SUBSCRIBED_EVENTS
    }
}

/// Online: read the row and the selection of the device (the hook for the initial state).
pub fn request_initial_state(ctx: &mut Ctx) {
    for request in ext::initial_state_requests(link(ctx).capability) {
        state(ctx).ctrl_output.send_sysex(&request);
    }
}

/// The device switched the page (`7E 06`): it resets the row and the selection.
pub fn on_page_switched(ctx: &mut Ctx) {
    reset_row_and_selection(ctx);
}

/// A message of the device for the link; returns false if it is none.
pub fn handle_device_sysex(ctx: &mut Ctx, data: &[u8]) -> bool {
    let Some(report) = ext::parse(data) else {
        return false;
    };
    let page = active_page(ctx);
    let slot_of = |control_id: i32| -> Option<Option<usize>> {
        if control_id == 0 {
            return Some(None);
        }
        let page = page?;
        let index = control_id - 1 - (page * NUM_SLOTS) as i32;
        (0..NUM_SLOTS as i32).contains(&index).then_some(Some(index as usize))
    };
    let link = link_mut(ctx);
    match report {
        // Nearly every command is acknowledged; the own changes are confirmed by their events
        Report::Ack => {}
        Report::Nack => {
            if let Some(row) = link.pending.row.take() {
                // The device does not have the rows after all: back to the control sets
                link.active_row = row;
                link.capability.rows = false;
            }
            if let Some(selection) = link.pending.selection.take() {
                // The selection is unchanged
                link.selected = selection;
            }
        }
        Report::ControlSet(set) => {
            if set < GRID_SIZE / 2 && link.active_row / 2 != set {
                link.active_row = set * 2;
            }
        }
        // The own changes come back with origin 1: the provider has them already, they are
        // confirmed (no NACK any more)
        Report::Row { origin: Origin::Sysex, .. } => link.pending.row = None,
        Report::Selection { origin: Origin::Sysex, .. } => link.pending.selection = None,
        Report::Row { row, .. } | Report::RowState(row) => {
            link.active_row = row;
            link.pending.row = None;
        }
        Report::Selection { control_id, .. } | Report::SelectionState(control_id) => {
            if let Some(selection) = slot_of(control_id) {
                let link = link_mut(ctx);
                link.selected = selection;
                link.pending.selection = None;
            }
        }
    }
    true
}
