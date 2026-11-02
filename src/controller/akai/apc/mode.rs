// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the APC40 for the 8 track knobs (former `BaseMode`, `PanMode`, `SendMode`,
//! `UserMode`, `NoteMode` and `BrowserMode`).
//!
//! The track knobs are bound to the parameters of the pan, send and user modes (the knob values
//! go directly to the parameters); the note and browser modes have no parameters, they get the
//! knob values with [`Mode::on_knob_value`] (former `setValue`).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use super::surface::{self as apc, APC_KNOB_TRACK_KNOB_1, APC_KNOB_TRACK_KNOB_LED_MODE_1, LED_MODE_PAN, LED_MODE_VOLUME};
use crate::daw::clip::NotePosition;
use crate::daw::events::ParameterBankId;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::mode::note_editor::{ClipKey, NoteEditor};
use crate::framework::mode::note_parameter::{note_value, set_note_value, NoteAttribute};
use crate::framework::mode::parameter_mode::{default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::device::BankParameterProvider;
use crate::framework::parameterprovider::provider;
use crate::framework::parameterprovider::track::{PanParameterProvider, SendParameterProvider};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The browser columns of the knobs 1-7 (former `BrowserMode.COLUMN_ORDER`).
const COLUMN_ORDER: [usize; 7] = [0, 6, 1, 2, 3, 4, 5];

/// The note attributes of the 8 knobs in the note mode.
const NOTE_ATTRIBUTES: [NoteAttribute; 8] = [
    NoteAttribute::Duration,
    NoteAttribute::Velocity,
    NoteAttribute::VelocitySpread,
    NoteAttribute::Gain,
    NoteAttribute::Panning,
    NoteAttribute::Chance,
    NoteAttribute::Timbre,
    NoteAttribute::Pressure,
];

/// What a mode does with the knobs.
enum Kind {
    Pan,
    Send,
    /// Former `UserMode` (project or track parameters).
    User { is_project_mode: bool },
    /// Former `NoteMode`.
    Note { note_editor: NoteEditor },
    /// Former `BrowserMode`.
    Browser { last_value: i32 },
}

/// A mode of the APC40 (former `BaseMode` and its sub classes).
pub struct ApcMode {
    base: ParameterModeBase,
    led_mode: i32,
    kind: Kind,
    is_knob_moving: Rc<Cell<bool>>,
    move_start_time: Rc<Cell<Option<Instant>>>,
}

impl ApcMode {
    fn new(name: &str, led_mode: i32, bank: ModeBank, kind: Kind) -> Self {
        ApcMode {
            base: ParameterModeBase::new(name, false, bank, default_knob_ids()),
            led_mode,
            kind,
            is_knob_moving: Rc::new(Cell::new(false)),
            move_start_time: Rc::new(Cell::new(None)),
        }
    }

    /// Former `PanMode`.
    pub fn pan(ctx: &Ctx) -> Self {
        let mut mode = Self::new("Panning", LED_MODE_PAN, ModeBank::TrackBank, Kind::Pan);
        mode.base.set_parameter_provider(ctx, provider(PanParameterProvider::new(ctx)));
        mode
    }

    /// Former `SendMode`.
    pub fn send(ctx: &Ctx, send_index: usize) -> Self {
        let mut mode = Self::new("Send", LED_MODE_VOLUME, ModeBank::TrackBank, Kind::Send);
        mode.base.set_parameter_provider(ctx, provider(SendParameterProvider::new(ctx, send_index as i32, 0)));
        mode
    }

    /// Former `UserMode`.
    pub fn user(ctx: &Ctx) -> Self {
        let mut mode = Self::new("User", LED_MODE_VOLUME, ModeBank::ParameterBank(ParameterBankId::Project), Kind::User { is_project_mode: true });
        mode.base.set_parameter_provider(ctx, provider(BankParameterProvider::new(ctx, ParameterBankId::Project)));
        mode
    }

    /// Former `NoteMode`.
    pub fn note() -> Self {
        Self::new("Note Edit", LED_MODE_VOLUME, ModeBank::None, Kind::Note { note_editor: NoteEditor::new() })
    }

    /// Former `BrowserMode`.
    pub fn browser() -> Self {
        Self::new("Browser", LED_MODE_PAN, ModeBank::None, Kind::Browser { last_value: 0 })
    }

    /// Former `NoteMode.setValues`.
    pub fn set_values(&mut self, clip: ClipKey, note_position: &NotePosition) {
        if let Kind::Note { note_editor } = &mut self.kind {
            note_editor.set_note(clip, note_position);
        }
    }

    /// Former `UserMode.isPageSelected`.
    pub fn is_page_selected(&self, ctx: &Ctx, page_index: usize) -> bool {
        let Kind::User { is_project_mode } = self.kind else {
            return false;
        };
        let bank = if is_project_mode { ctx.model.project().parameter_bank() } else { ctx.model.cursor_track().parameter_bank() };
        bank.is_some_and(|bank| bank.page_bank().selected_item_index() == page_index)
    }

    /// Former `UserMode.displayPageName`.
    pub fn display_page_name(&self, ctx: &mut Ctx) {
        if let Kind::User { is_project_mode } = self.kind {
            if is_project_mode {
                mv_helper::notify_selected_project_parameter_page(ctx);
            } else {
                mv_helper::notify_selected_track_parameter_page(ctx);
            }
        }
    }

    /// Former `UserMode.toggleMode`.
    pub fn toggle_mode(&mut self, ctx: &mut Ctx) {
        let Kind::User { is_project_mode } = &mut self.kind else {
            return;
        };
        *is_project_mode = !*is_project_mode;
        let bank = if *is_project_mode { ParameterBankId::Project } else { ParameterBankId::CursorTrack };
        self.base.switch_banks(ModeBank::ParameterBank(bank));
        self.base.set_parameter_provider(ctx, provider(BankParameterProvider::new(ctx, bank)));
        self.base.bind_controls(ctx);
        self.display_page_name(ctx);
    }

    /// Former `BaseMode.setValue`: only the modes without parameters get the knob values.
    fn set_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        match &mut self.kind {
            Kind::Note { note_editor } => {
                // Workaround to still intercept changes
                let editor = note_editor.clone();
                set_note_value(ctx, &editor, NOTE_ATTRIBUTES[index.min(7)], value);
                // Note was modified, prevent deletion of note on button up
                set_active_sequencer_note_edited(ctx);
            }
            Kind::Browser { last_value } => {
                let Some(browser) = ctx.model.browser_mut() else {
                    return;
                };
                if !browser.is_active() {
                    return;
                }
                let diff = value - *last_value;
                let is_left = value == 0 || diff < 0;
                *last_value = value;
                match index {
                    0..=6 => {
                        if is_left {
                            browser.select_previous_filter_item(COLUMN_ORDER[index]);
                        } else {
                            browser.select_next_filter_item(COLUMN_ORDER[index]);
                        }
                    }
                    7 => {
                        if is_left {
                            browser.select_previous_result();
                        } else {
                            browser.select_next_result();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Former `checkKnobMovement`: the knob is not moving anymore if it was not moved for 200ms.
    fn check_knob_movement(ctx: &mut Ctx, is_knob_moving: Rc<Cell<bool>>, move_start_time: Rc<Cell<Option<Instant>>>) {
        if !is_knob_moving.get() {
            return;
        }
        let elapsed = move_start_time.get().map_or(u128::MAX, |start| ctx.host.now().saturating_duration_since(start).as_millis());
        if elapsed > 200 {
            is_knob_moving.set(false);
        } else {
            ctx.schedule(100, move |ctx| Self::check_knob_movement(ctx, is_knob_moving, move_start_time));
        }
    }
}

/// Mark the note of the active sequencer view as edited (former
/// `if (activeView instanceof AbstractSequencerView) sequencerView.setNoteEdited ()`). The
/// views are borrowed while they call the mode, then the flag is set after the event.
fn set_active_sequencer_note_edited(ctx: &mut Ctx) {
    let Some(view) = ctx.active_view() else {
        return;
    };
    let is_borrowed = match view.try_borrow_mut() {
        Ok(mut view) => {
            super::view::set_note_edited(&mut *view);
            false
        }
        Err(_) => true,
    };
    if is_borrowed {
        ctx.schedule(0, set_active_sequencer_note_edited);
    }
}

impl Mode for ApcMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    // AbstractParameterMode
    crate::forward_mode!(base => on_activate, on_deactivate, is_any_knob_touched, select_item);

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        if !select_item_page_override(self, ctx, false) {
            self.base.select_previous_item_page(ctx);
        }
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        if !select_item_page_override(self, ctx, true) {
            self.base.select_next_item_page(ctx);
        }
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        match self.kind {
            Kind::Browser { .. } => self.has_previous_item(ctx),
            _ => self.base.has_previous_item_page(ctx),
        }
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        match self.kind {
            Kind::Browser { .. } => self.has_next_item(ctx),
            _ => self.base.has_next_item_page(ctx),
        }
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        self.set_value(ctx, index, value);
        self.move_start_time.set(Some(ctx.host.now()));
        if self.is_knob_moving.get() {
            return;
        }
        self.is_knob_moving.set(true);
        let (is_knob_moving, move_start_time) = (self.is_knob_moving.clone(), self.move_start_time.clone());
        ctx.schedule(100, move |ctx| Self::check_knob_movement(ctx, is_knob_moving, move_start_time));
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        match &self.kind {
            Kind::Note { note_editor } => note_value(ctx, note_editor, NOTE_ATTRIBUTES[index.min(7)]),
            _ => self.base.knob_value(ctx, index),
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        if self.is_knob_moving.get() {
            return;
        }
        for i in 0..8 {
            let value = self.knob_value(ctx, i);
            let is_off = value < 0;
            apc::set_led(ctx, APC_KNOB_TRACK_KNOB_LED_MODE_1 + i as i32, if is_off { LED_MODE_VOLUME } else { self.led_mode });
            apc::set_led(ctx, APC_KNOB_TRACK_KNOB_1 + i as i32, if is_off { 0 } else { value });
        }
    }

    fn on_button(&mut self, _ctx: &mut Ctx, _row: usize, _index: usize, _event: ButtonEvent) {
        // Intentionally empty
    }

    fn select_item_page(&mut self, ctx: &mut Ctx, page: usize) {
        self.base.select_item_page(ctx, page);
        self.display_page_name(ctx);
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        match self.kind {
            Kind::User { .. } => ctx.model.current_track_bank_mut().select_previous_item(),
            _ => self.base.select_previous_item(ctx),
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        match self.kind {
            Kind::User { .. } => ctx.model.current_track_bank_mut().select_next_item(),
            _ => self.base.select_next_item(ctx),
        }
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        match self.kind {
            Kind::Browser { .. } => ctx.model.browser().is_some_and(|b| b.has_previous_content_type()),
            _ => self.base.has_previous_item(ctx),
        }
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        match self.kind {
            Kind::Browser { .. } => ctx.model.browser().is_some_and(|b| b.has_next_content_type()),
            _ => self.base.has_next_item(ctx),
        }
    }

    fn note_editor(&self) -> Option<&NoteEditor> {
        match &self.kind {
            Kind::Note { note_editor } => Some(note_editor),
            _ => None,
        }
    }

    fn note_editor_mut(&mut self) -> Option<&mut NoteEditor> {
        match &mut self.kind {
            Kind::Note { note_editor } => Some(note_editor),
            _ => None,
        }
    }

    impl_as_any!();
}

/// The mode specific overrides of the item page functions: the user mode scrolls the track
/// bank, the browser mode the content types. Returns false for the other modes.
fn select_item_page_override(mode: &mut ApcMode, ctx: &mut Ctx, is_next: bool) -> bool {
    match mode.kind {
        Kind::User { .. } => {
            if is_next {
                ctx.model.current_track_bank_mut().select_next_page();
            } else {
                ctx.model.current_track_bank_mut().select_previous_page();
            }
            true
        }
        Kind::Browser { .. } => {
            if let Some(browser) = ctx.model.browser_mut() {
                if is_next {
                    browser.next_content_type();
                } else {
                    browser.previous_content_type();
                }
            }
            true
        }
        _ => false,
    }
}
