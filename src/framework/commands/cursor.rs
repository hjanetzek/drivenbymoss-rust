// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Cursor commands (former `Direction`, `ModeCursorCommand` and `CursorCommand`).
//!
//! The Java `ModeCursorCommand` (which scrolls the items of the active mode) and its subclass
//! `CursorCommand` (which scrolls the scenes up/down and the items of the active mode left/right)
//! are merged into [`CursorCommand`]; [`CursorKind`] selects the behaviour. Controllers which
//! override the scroll functions (e.g. the Launchpad) wrap the command and call the `scroll_*`
//! functions as their "super" implementation.

use super::mv_helper;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
pub use crate::framework::feature::ScrollStates;
use crate::midi::ButtonEvent;

/// The direction of a cursor key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Get the scroll state for a direction (former `ScrollStates.canScroll`).
pub fn can_scroll(states: &ScrollStates, direction: Direction) -> bool {
    match direction {
        Direction::Left => states.can_scroll_left,
        Direction::Right => states.can_scroll_right,
        Direction::Up => states.can_scroll_up,
        Direction::Down => states.can_scroll_down,
    }
}

/// Which variant of the Java cursor commands is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    /// Former `ModeCursorCommand`: left/right selects the previous/next item of the active mode,
    /// up/down the next/previous page.
    Mode,
    /// Former `CursorCommand`: up/down scrolls the scenes, left/right selects the items of the
    /// active mode. Shift switches to pages.
    Scenes,
}

/// Command for the cursor keys.
pub struct CursorCommand {
    pub direction: Direction,
    pub kind: CursorKind,
    pub notify_selection: bool,
    pub is_flipped: bool,
    pub trigger_event: ButtonEvent,
}

impl CursorCommand {
    /// Former `CursorCommand (direction, model, surface, notifySelection)`.
    pub fn new(direction: Direction, notify_selection: bool) -> Self {
        Self::with_flipped(direction, notify_selection, false)
    }

    pub fn with_flipped(direction: Direction, notify_selection: bool, is_flipped: bool) -> Self {
        CursorCommand { direction, kind: CursorKind::Scenes, notify_selection, is_flipped, trigger_event: ButtonEvent::Down }
    }

    /// Former `ModeCursorCommand`.
    pub fn new_mode_cursor(direction: Direction, notify_selection: bool) -> Self {
        CursorCommand { direction, kind: CursorKind::Mode, notify_selection, is_flipped: false, trigger_event: ButtonEvent::Down }
    }

    /// Execute the scroll function of the direction.
    pub fn scroll(&self, ctx: &mut Ctx) {
        match self.direction {
            Direction::Left => self.scroll_left(ctx),
            Direction::Right => self.scroll_right(ctx),
            Direction::Up => self.scroll_up(ctx),
            Direction::Down => self.scroll_down(ctx),
        }
    }

    /// Notify the selected item of the active mode if enabled.
    pub fn notify(&self, ctx: &mut Ctx) {
        if self.notify_selection && ctx.active_mode().is_some() {
            mv_helper::notify_selected_item_of_active_mode(ctx);
        }
    }

    /// Can the cursor be moved in the direction of the command?
    pub fn can_scroll(&self, ctx: &Ctx) -> bool {
        can_scroll(&self.arrow_states(ctx), self.direction)
    }

    /// Former `updateArrowStates`.
    pub fn arrow_states(&self, ctx: &Ctx) -> ScrollStates {
        let is_shift = ctx.surface.is_shift_pressed();
        match self.kind {
            CursorKind::Mode => ScrollStates {
                can_scroll_left: self.can_select_item_or_page(ctx, false, false),
                can_scroll_right: self.can_select_item_or_page(ctx, false, true),
                can_scroll_down: self.can_select_item_or_page(ctx, true, false),
                can_scroll_up: self.can_select_item_or_page(ctx, true, true),
            },
            CursorKind::Scenes => ScrollStates {
                can_scroll_up: self.can_select_scene_item_or_page(ctx, is_shift, false),
                can_scroll_down: self.can_select_scene_item_or_page(ctx, is_shift, true),
                can_scroll_left: self.can_select_item_or_page(ctx, is_shift, false),
                can_scroll_right: self.can_select_item_or_page(ctx, is_shift, true),
            },
        }
    }

    pub fn scroll_left(&self, ctx: &mut Ctx) {
        let is_page = self.kind == CursorKind::Scenes && ctx.surface.is_shift_pressed();
        self.select_item_or_page(ctx, is_page, false);
    }

    pub fn scroll_right(&self, ctx: &mut Ctx) {
        let is_page = self.kind == CursorKind::Scenes && ctx.surface.is_shift_pressed();
        self.select_item_or_page(ctx, is_page, true);
    }

    pub fn scroll_up(&self, ctx: &mut Ctx) {
        match self.kind {
            CursorKind::Mode => self.select_item_or_page(ctx, true, true),
            CursorKind::Scenes => {
                let is_page = ctx.surface.is_shift_pressed();
                self.select_scene_item_or_page(ctx, is_page, false);
            }
        }
    }

    pub fn scroll_down(&self, ctx: &mut Ctx) {
        match self.kind {
            CursorKind::Mode => self.select_item_or_page(ctx, true, false),
            CursorKind::Scenes => {
                let is_page = ctx.surface.is_shift_pressed();
                self.select_scene_item_or_page(ctx, is_page, true);
            }
        }
    }

    /// Select the previous/next item or page of the active mode.
    pub fn select_item_or_page(&self, ctx: &mut Ctx, is_page: bool, is_next: bool) {
        let Some(mode) = ctx.active_mode() else {
            return;
        };
        let Ok(mut mode) = mode.try_borrow_mut() else {
            return;
        };
        match (is_page ^ self.is_flipped, is_next) {
            (true, true) => mode.select_next_item_page(ctx),
            (true, false) => mode.select_previous_item_page(ctx),
            (false, true) => mode.select_next_item(ctx),
            (false, false) => mode.select_previous_item(ctx),
        }
    }

    pub fn can_select_item_or_page(&self, ctx: &Ctx, is_page: bool, is_next: bool) -> bool {
        let Some(mode) = ctx.active_mode() else {
            return false;
        };
        let Ok(mode) = mode.try_borrow() else {
            return false;
        };
        match (is_page ^ self.is_flipped, is_next) {
            (true, true) => mode.has_next_item_page(ctx),
            (true, false) => mode.has_previous_item_page(ctx),
            (false, true) => mode.has_next_item(ctx),
            (false, false) => mode.has_previous_item(ctx),
        }
    }

    /// Scroll the scene bank of the current track bank by one scene or a page.
    pub fn select_scene_item_or_page(&self, ctx: &mut Ctx, is_page: bool, is_next: bool) {
        let mut scene_bank = ctx.model.current_track_bank_mut().scene_bank_mut();
        match (is_page ^ self.is_flipped, is_next) {
            (true, true) => scene_bank.select_next_page(),
            (true, false) => scene_bank.select_previous_page(),
            (false, true) => scene_bank.scroll_forwards(),
            (false, false) => scene_bank.scroll_backwards(),
        }
    }

    pub fn can_select_scene_item_or_page(&self, ctx: &Ctx, is_page: bool, is_next: bool) -> bool {
        let scene_bank = ctx.model.current_track_bank().scene_bank();
        match (is_page ^ self.is_flipped, is_next) {
            (true, true) => scene_bank.can_scroll_page_forwards(),
            (true, false) => scene_bank.can_scroll_page_backwards(),
            (false, true) => scene_bank.can_scroll_forwards(),
            (false, false) => scene_bank.can_scroll_backwards(),
        }
    }
}

impl TriggerCommand for CursorCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != self.trigger_event {
            return;
        }
        self.scroll(ctx);
        self.notify(ctx);
    }
}
