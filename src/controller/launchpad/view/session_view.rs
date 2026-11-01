// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The session view of the Launchpad (former `SessionView`).

use crate::controller::launchpad::colors::*;
use crate::controller::launchpad::command::view_select::set_session_view_temporary;
use crate::controller::launchpad::surface as lp;
use crate::daw::bank::BankItem;
use crate::daw::clip::SessionAlternative;
use crate::daw::color::DawColor;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View, BUTTON_COLOR_OFF};
use crate::framework::grid::LightInfo;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::view::base::{is_scene_button, scene_index, ViewBase};
use crate::framework::view::session_view::{SessionColors, SessionHooks, SessionViewBase};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The name of the session view.
pub const NAME_SESSION: &str = "Session";

/// The overridden functions of the Launchpad session view.
struct LaunchpadSessionHooks;

impl SessionHooks for LaunchpadSessionHooks {
    fn is_button_combination(&self, ctx: &mut Ctx, button: ButtonId) -> bool {
        is_button_combination(ctx, button)
    }

    // Trigger alternate scene launch on shift button: the default

    fn is_scene_select_action(&self, _ctx: &Ctx) -> bool {
        // Sadly, no additional button available
        false
    }

    /// Former `handleButtonCombinations` override: the delete/duplicate modes are switched off
    /// after they were used.
    fn after_button_combinations(&self, ctx: &mut Ctx, track_index: usize, slot_index: usize) {
        if is_button_combination(ctx, ButtonId::Delete) && ctx.config.is_delete_mode_active() {
            ctx.config.toggle_delete_mode_active();
        } else if is_button_combination(ctx, ButtonId::Duplicate) && ctx.config.is_duplicate_mode_active() {
            let slot = ctx.model.current_track_bank().item(track_index).slot_bank().item(slot_index);
            if !slot.does_exist() || !slot.has_content() {
                ctx.config.toggle_duplicate_mode_active();
            }
        }
    }

    fn pad(&self, ctx: &Ctx, _session: &SessionViewBase, note: i32) -> (usize, usize) {
        let index = note - 36;
        let x = index % 8;
        let y = 7 - index / 8;
        let (x, y) = if ctx.config.is_flip_session() { (y, x) } else { (x, y) };
        (x.max(0) as usize, y.max(0) as usize)
    }
}

/// The Launchpad version of `isButtonCombination`: the delete and duplicate modes act like
/// pressed buttons.
fn is_button_combination(ctx: &mut Ctx, button: ButtonId) -> bool {
    if ctx.is_button_combination(button) || button == ButtonId::Delete && ctx.config.is_delete_mode_active() {
        return true;
    }
    button == ButtonId::Duplicate && ctx.config.is_duplicate_mode_active()
}

/// The scroll states of the session view and the views based on it.
pub fn session_scroll_states(ctx: &Ctx) -> ScrollStates {
    let tb = ctx.model.current_track_bank();
    let sel_index = tb.selected_item().map(|t| t.index()).unwrap_or(-1);
    let scene_bank = tb.scene_bank();
    ScrollStates {
        can_scroll_left: sel_index > 0 || tb.can_scroll_page_backwards(),
        can_scroll_right: (0..7).contains(&sel_index) && tb.item(sel_index as usize + 1).does_exist() || tb.can_scroll_page_forwards(),
        can_scroll_up: scene_bank.can_scroll_page_backwards(),
        can_scroll_down: scene_bank.can_scroll_page_forwards(),
    }
}

/// True if the view is the session view or one of the views which are based on it in Java
/// (former `instanceof SessionView`, used by the indication update of the setup).
pub fn is_session_view(view: &dyn View) -> bool {
    view.as_any().is::<SessionView>() || view.as_virtual_fader_view().is_some()
}

/// The session view.
pub struct SessionView {
    base: ViewBase,
    pub session: SessionViewBase,
    is_show_temporarily: bool,
}

impl SessionView {
    pub fn new(name: &str, _ctx: &Ctx) -> Self {
        let mut session = SessionViewBase::new(8, 8, true);
        session.ignore_clip_color_for_play_and_record = true;

        let has_content = LightInfo::new(LAUNCHPAD_COLOR_AMBER, -1, false);
        session.clip_colors = SessionColors {
            is_recording: LightInfo::new(LAUNCHPAD_COLOR_RED_HI, LAUNCHPAD_COLOR_RED_HI, false),
            is_recording_queued: LightInfo::new(LAUNCHPAD_COLOR_RED_HI, LAUNCHPAD_COLOR_RED_LO, true),
            is_playing: LightInfo::new(LAUNCHPAD_COLOR_GREEN, LAUNCHPAD_COLOR_GREEN, false),
            is_playing_queued: LightInfo::new(LAUNCHPAD_COLOR_GREEN, LAUNCHPAD_COLOR_GREEN_LO, true),
            is_stop_queued: LightInfo::new(LAUNCHPAD_COLOR_GREEN, LAUNCHPAD_COLOR_GREEN_LO, true),
            has_content,
            has_no_content: LightInfo::new(LAUNCHPAD_COLOR_BLACK, -1, false),
            is_rec_armed: LightInfo::new(LAUNCHPAD_COLOR_RED_LO, -1, false),
            is_muted: LightInfo::new(LAUNCHPAD_COLOR_GREY_LO, -1, false),
        };
        session.bird_color_has_content = has_content;
        session.bird_color_selected = LightInfo::new(LAUNCHPAD_COLOR_GREEN, -1, false);

        SessionView { base: ViewBase::new(name), session, is_show_temporarily: false }
    }

    pub fn is_birds_eye_active(&self) -> bool {
        self.session.is_birds_eye_active
    }

    pub fn set_birds_eye_active(&mut self, is_birds_eye_active: bool) {
        self.session.is_birds_eye_active = is_birds_eye_active;
    }

    /// Former `onSession`: show the session view temporarily while the Session button is long
    /// pressed.
    pub fn on_session(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        match event {
            ButtonEvent::Long => self.is_show_temporarily = true,
            ButtonEvent::Up => {
                if !self.is_show_temporarily {
                    return;
                }
                self.is_show_temporarily = false;
                ctx.schedule(0, |ctx| ctx.recall_preferred_view());
            }
            ButtonEvent::Down => {}
        }
    }

    fn is_control_mode_off(&self, ctx: &Ctx) -> bool {
        lp::has_track_selection_buttons(ctx) || self.session.is_birds_eye_active || ctx.modes.active_id() == Some(Modes::Dummy)
    }

    /// Former `updateRowsCols`, which Java calls on all changes of the values it depends on. It
    /// is called here before the values are used.
    fn update_rows_cols(&mut self, ctx: &Ctx) {
        self.session.rows = if self.is_control_mode_off(ctx) { 8 } else { 7 };
        self.session.columns = 8;
    }

    fn control_mode_index(&self, ctx: &Ctx, note: i32) -> Option<usize> {
        if self.is_control_mode_off(ctx) {
            return None;
        }
        if ctx.config.is_flip_session() {
            let n = note + 1 - 36;
            if n % 8 == 0 {
                return Some((8 - n / 8) as usize);
            }
            return None;
        }
        if note < 44 {
            return Some((note - 36) as usize);
        }
        None
    }

    fn handle_first_row_modes(&self, ctx: &mut Ctx, index: usize) {
        // First row mode handling
        if is_button_combination(ctx, ButtonId::Delete) {
            ctx.model.current_track_bank_mut().item_mut(index).remove();
            return;
        }
        if is_button_combination(ctx, ButtonId::Duplicate) {
            ctx.model.current_track_bank_mut().item_mut(index).duplicate();
            return;
        }

        if ctx.modes.is_active(Modes::RecArm) {
            ctx.model.current_track_bank_mut().item_mut(index).toggle_rec_arm();
        } else if ctx.modes.is_active(Modes::TrackSelect) {
            ctx.model.current_track_bank_mut().item_mut(index).select_or_expand_group();
            mv_helper::notify_selected_track(ctx);
        } else if ctx.modes.is_active(Modes::Mute) {
            ctx.model.current_track_bank_mut().item_mut(index).toggle_mute();
        } else if ctx.modes.is_active(Modes::Solo) {
            ctx.model.current_track_bank_mut().item_mut(index).toggle_solo();
        } else if ctx.modes.is_active(Modes::StopClip) {
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
            let is_shift = ctx.surface.is_shift_pressed();
            ctx.model.current_track_bank_mut().item_mut(index).stop(is_shift);
        }
    }

    fn mode_color(ctx: &Ctx, index: usize) -> i32 {
        let track = ctx.model.current_track_bank().item(index);
        if track.does_exist() {
            let modes = &ctx.modes;
            if modes.is_active(Modes::RecArm) {
                return if track.is_rec_arm() { LAUNCHPAD_COLOR_RED_HI } else { LAUNCHPAD_COLOR_RED_LO };
            }
            if modes.is_active(Modes::TrackSelect) {
                return if track.is_selected() { LAUNCHPAD_COLOR_GREEN_HI } else { LAUNCHPAD_COLOR_GREEN_LO };
            }
            if modes.is_active(Modes::Mute) {
                return if track.is_mute() { LAUNCHPAD_COLOR_YELLOW_HI } else { LAUNCHPAD_COLOR_YELLOW_LO };
            }
            if modes.is_active(Modes::Solo) {
                return if track.is_solo() { LAUNCHPAD_COLOR_BLUE_HI } else { LAUNCHPAD_COLOR_BLUE_LO };
            }
            if modes.is_active(Modes::StopClip) {
                return if ctx.surface.is_pressed(ButtonId::get(ButtonId::Pad1, index)) { LAUNCHPAD_COLOR_RED } else { LAUNCHPAD_COLOR_ROSE };
            }
        }
        LAUNCHPAD_COLOR_BLACK
    }

    fn on_grid_note_bank_selection(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let index = note - 36;
        let columns = self.session.columns;
        let rows = self.session.rows;
        self.session.on_grid_note_birds_eye_view(ctx, index % columns, rows - 1 - index / columns, 0);
        if ctx.surface.is_pressed(ButtonId::Session) {
            self.set_birds_eye_active(false);
        }
    }

}

impl View for SessionView {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
        self.update_rows_cols(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if ctx.surface.is_pressed(button) {
            return LAUNCHPAD_COLOR_WHITE;
        }
        ctx.colors.color_index(self.button_color_id(ctx, button))
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = scene_index(button) {
            let scene = ctx.model.current_track_bank().scene_bank().item(index);
            if scene.does_exist() {
                return DawColor::color_id(scene.color());
            }
        }
        BUTTON_COLOR_OFF
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.update_rows_cols(ctx);
        // The Launchpad ignores the flip check (former drawSessionGrid override)
        self.session.draw_grid(ctx, &LaunchpadSessionHooks, true);
        if self.is_control_mode_off(ctx) {
            return;
        }
        let flip_session = ctx.config.is_flip_session();
        for x in 0..8 {
            let (pad_x, pad_y) = if flip_session { (7, x) } else { (x, 7) };
            let color = Self::mode_color(ctx, x as usize);
            ctx.surface.grid().light_ex(pad_x, pad_y, color);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.update_rows_cols(ctx);
        if velocity == 0 {
            set_session_view_temporary(ctx);
        }

        if let Some(mode_index) = self.control_mode_index(ctx, note) {
            if velocity != 0 {
                self.handle_first_row_modes(ctx, mode_index);
            }
            return;
        }

        if self.session.is_birds_eye_active {
            self.on_grid_note_bank_selection(ctx, note, velocity);
            return;
        }

        self.session.on_grid_note(ctx, &LaunchpadSessionHooks, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if is_scene_button(button) {
            SessionViewBase::on_scene_button(ctx, &LaunchpadSessionHooks, button, event);
        }
        if is_scene_button(button) && event == ButtonEvent::Up {
            set_session_view_temporary(ctx);
            if ctx.surface.is_shift_pressed() {
                self.session.set_alternate_interaction_used(true);
            }
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        Some(session_scroll_states(ctx))
    }

    fn is_birds_eye_active(&self) -> bool {
        self.session.is_birds_eye_active
    }

    fn set_birds_eye_active(&mut self, is_active: bool) {
        SessionView::set_birds_eye_active(self, is_active);
    }

    crate::impl_session_alternative!();

    impl_as_any!();
}

impl SessionAlternative for SessionView {
    fn was_alternate_interaction_used(&self) -> bool {
        self.session.was_alternate_interaction_used()
    }

    fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.session.set_alternate_interaction_used(was_used);
    }
}
