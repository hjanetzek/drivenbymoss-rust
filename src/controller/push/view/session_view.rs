// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The session view of the Push (former `SessionView`).
//!
//! Former `((SelectSessionViewCommand) ...).setTemporary ()`: the view raises a flag which the
//! session button command takes (see [`SessionView::take_temporary_request`]), since the view
//! cannot access the command.

use crate::controller::push::colors::*;
use crate::controller::push::surface;
use crate::daw::bank::BankItem;
use crate::framework::color_ids::abstract_session_view::{COLOR_SCENE, COLOR_SCENE_OFF, COLOR_SELECTED_SCENE};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::grid::LightInfo;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::{is_scene_button, scene_index};
use crate::framework::view::session_view::{SessionColors, SessionHooks, SessionViewBase};
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The name of the session view.
pub const NAME_SESSION: &str = "Session";

/// The default hooks of the session view.
struct PushSessionHooks;

impl SessionHooks for PushSessionHooks {}

/// The session colors of the Push (also used by the XoX drum view for its clips).
pub fn push_session_colors(ctx: &Ctx) -> (SessionColors, LightInfo, LightInfo) {
    let is_push2 = surface::is_modern(ctx);
    let pick = |p2: i32, p1: i32| if is_push2 { p2 } else { p1 };
    let red_lo = pick(PUSH2_COLOR2_RED_LO, PUSH1_COLOR2_RED_LO);
    let red_hi = pick(PUSH2_COLOR2_RED_HI, PUSH1_COLOR2_RED_HI);
    let black = pick(PUSH2_COLOR2_BLACK, PUSH1_COLOR2_BLACK);
    let white = pick(PUSH2_COLOR2_WHITE, PUSH1_COLOR2_WHITE);
    let green = pick(PUSH2_COLOR2_GREEN, PUSH1_COLOR2_GREEN);
    let amber = pick(PUSH2_COLOR2_AMBER, PUSH1_COLOR2_AMBER);
    let grey = pick(PUSH2_COLOR2_GREY_LO, PUSH1_COLOR2_GREY_LO);
    let is_playing = LightInfo::new(green, green, false);
    let colors = SessionColors {
        is_recording: LightInfo::new(red_hi, red_hi, false),
        is_recording_queued: LightInfo::new(red_hi, black, true),
        is_playing,
        is_playing_queued: LightInfo::new(green, green, true),
        is_stop_queued: LightInfo::new(green, green, true),
        has_content: LightInfo::new(amber, white, false),
        has_no_content: LightInfo::new(black, -1, false),
        is_rec_armed: LightInfo::new(red_lo, -1, false),
        is_muted: LightInfo::new(grey, -1, false),
    };
    (colors, LightInfo::new(amber, -1, false), is_playing)
}

/// The session view.
pub struct SessionView {
    pub session: SessionViewBase,
    temporary_request: bool,
}

impl SessionView {
    pub fn new(ctx: &Ctx) -> Self {
        let mut session = SessionViewBase::new(8, 8, true);
        let (colors, bird_has_content, bird_selected) = push_session_colors(ctx);
        session.clip_colors = colors;
        session.bird_color_has_content = bird_has_content;
        session.bird_color_selected = bird_selected;
        SessionView { session, temporary_request: false }
    }

    /// Former `SelectSessionViewCommand.setTemporary`, taken by the session command.
    pub fn take_temporary_request(&mut self) -> bool {
        std::mem::take(&mut self.temporary_request)
    }

    /// Former `isBirdsEyeActive`: Shift + Select.
    pub fn is_birds_eye(ctx: &Ctx) -> bool {
        ctx.surface.is_shift_pressed() && ctx.surface.is_select_pressed()
    }
}

impl View for SessionView {
    fn name(&self) -> &str {
        NAME_SESSION
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if Self::is_birds_eye(ctx) {
            self.session.draw_birds_eye_grid(ctx);
        } else {
            self.session.draw_session_grid(ctx, &PushSessionHooks, false);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            self.temporary_request = true;
        }

        // Birds-eye-view navigation
        if Self::is_birds_eye(ctx) {
            if velocity == 0 {
                return;
            }
            let index = note - 36;
            let x = index % self.session.columns;
            let y = self.session.rows - 1 - index / self.session.columns;
            self.session.on_grid_note_birds_eye_view(ctx, x, y, 0);
            return;
        }

        // Former override of handleButtonCombinations: Select + pad selects the slot
        if velocity != 0 && ctx.surface.is_pressed(ButtonId::Select) {
            if PushSessionHooks.is_alternate_function(ctx) {
                self.session.set_alternate_interaction_used(true);
            }
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            let (track_index, slot_index) = PushSessionHooks.pad(ctx, &self.session, note);
            let slot = ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index);
            if slot.does_exist() {
                slot.select();
            }
            ctx.surface.consume_pads();
            return;
        }

        self.session.on_grid_note(ctx, &PushSessionHooks, note, velocity);
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let Some(scene) = scene_index(button) else {
            return BUTTON_COLOR_OFF;
        };
        let s = ctx.model.scene_bank().item(scene);
        if s.does_exist() {
            if s.is_selected() {
                COLOR_SELECTED_SCENE
            } else {
                COLOR_SCENE
            }
        } else {
            COLOR_SCENE_OFF
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) {
            return;
        }
        SessionViewBase::on_scene_button(ctx, &PushSessionHooks, button, event);
        if event == ButtonEvent::Up {
            self.temporary_request = true;
        }
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let table = ctx.model.scales().translate_matrix_to_grid(&crate::scale::Scales::empty_matrix());
        ctx.surface.set_key_translation_table(table.to_vec());
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for SessionView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.current_track_bank_mut().scene_bank_mut().select_next_page();
        }
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.current_track_bank_mut().scene_bank_mut().select_previous_page();
        }
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        ctx.model.current_track_bank().scene_bank().can_scroll_page_forwards()
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        ctx.model.current_track_bank().scene_bank().can_scroll_page_backwards()
    }

    fn reset_octave(&mut self, _ctx: &mut Ctx) {
        // Currently, not used
    }
}
