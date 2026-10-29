// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The session view of the Maschine JAM (former `SessionView`).

use crate::controller::maschine::core::colors::*;
use crate::daw::bank::BankItem;
use crate::daw::clip::SessionAlternative;
use crate::framework::color_ids::abstract_session_view::{COLOR_SCENE, COLOR_SCENE_OFF, COLOR_SELECTED_SCENE};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View, BUTTON_COLOR_OFF};
use crate::framework::grid::LightInfo;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::{scene_index, ViewBase};
use crate::framework::view::session_view::{SessionColors, SessionHooks, SessionViewBase};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

struct JamSessionHooks;

impl SessionHooks for JamSessionHooks {}

/// The session view.
pub struct SessionView {
    base: ViewBase,
    session: SessionViewBase,
}

impl SessionView {
    pub fn new(_ctx: &Ctx) -> Self {
        let mut session = SessionViewBase::new(8, 8, true);
        let is_playing = LightInfo::new(COLOR_GREEN, -1, false);
        let has_content = LightInfo::new(COLOR_AMBER, COLOR_WHITE, false);
        session.clip_colors = SessionColors {
            is_recording: LightInfo::new(COLOR_RED, COLOR_RED, false),
            is_recording_queued: LightInfo::new(COLOR_RED, COLOR_BLACK, true),
            is_playing,
            is_playing_queued: LightInfo::new(COLOR_GREEN, COLOR_GREEN, true),
            is_stop_queued: LightInfo::new(COLOR_GREEN, COLOR_GREEN, true),
            has_content,
            has_no_content: LightInfo::new(COLOR_BLACK, -1, false),
            is_rec_armed: LightInfo::new(COLOR_RED_LO, -1, false),
            is_muted: LightInfo::new(COLOR_DARK_GREY, -1, false),
        };
        session.bird_color_has_content = has_content;
        session.bird_color_selected = is_playing;
        SessionView { base: ViewBase::new("Session"), session }
    }

    fn scroll_track_bank(ctx: &mut Ctx, is_forwards: bool) {
        let track_bank = ctx.model.current_track_bank_mut();
        if is_forwards {
            track_bank.scroll_forwards();
        } else {
            track_bank.scroll_backwards();
        }
    }

    fn scroll_scene_bank(ctx: &mut Ctx, is_forwards: bool) {
        let mut scene_bank = ctx.model.current_track_bank_mut().scene_bank_mut();
        if is_forwards {
            scene_bank.scroll_forwards();
        } else {
            scene_bank.scroll_backwards();
        }
    }

    /// Former `handleButtonCombinations` override (the parts before calling the super method).
    /// Returns true if handled.
    fn handle_jam_button_combinations(ctx: &mut Ctx, track_index: usize, slot_index: usize) -> bool {
        if !ctx.model.current_track_bank().item(track_index).does_exist() {
            return true;
        }
        if ctx.is_button_combination(ButtonId::Mute) {
            let is_select = ctx.surface.is_select_pressed();
            ctx.model.current_track_bank_mut().item_mut(track_index).stop(is_select);
            return true;
        }
        if ctx.is_button_combination(ButtonId::Select) {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).select();
            return true;
        }
        if ctx.is_button_combination(ButtonId::Groove) {
            ctx.model.current_track_bank_mut().item_mut(track_index).return_to_arrangement();
            return true;
        }
        false
    }
}

impl View for SessionView {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
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

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.session.draw_grid(ctx, &JamSessionHooks, false);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if self.session.is_birds_eye_active {
            if velocity == 0 {
                return;
            }
            let index = note - 36;
            let x = index % self.session.columns;
            let y = self.session.rows - 1 - index / self.session.columns;
            self.session.on_grid_note_birds_eye_view(ctx, x, y, 0);
            return;
        }

        if velocity != 0 {
            let (track_index, slot_index) = JamSessionHooks.pad(ctx, &self.session, note);
            if Self::handle_jam_button_combinations(ctx, track_index, slot_index) {
                ctx.surface.consume_pads();
                return;
            }
        }
        self.session.on_grid_note(ctx, &JamSessionHooks, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let flip_session = ctx.config.is_flip_session();
        let is_down = event == ButtonEvent::Down;
        match button {
            ButtonId::ArrowLeft | ButtonId::ArrowRight => {
                if is_down {
                    if flip_session {
                        Self::scroll_scene_bank(ctx, button == ButtonId::ArrowRight);
                    } else {
                        Self::scroll_track_bank(ctx, button == ButtonId::ArrowRight);
                    }
                }
            }
            ButtonId::ArrowUp | ButtonId::ArrowDown => {
                if is_down {
                    if flip_session {
                        Self::scroll_track_bank(ctx, button == ButtonId::ArrowDown);
                    } else {
                        Self::scroll_scene_bank(ctx, button == ButtonId::ArrowDown);
                    }
                }
            }
            _ => SessionViewBase::on_scene_button(ctx, &JamSessionHooks, button, event),
        }
    }

    fn scroll_states(&self, ctx: &Ctx) -> Option<ScrollStates> {
        let flip_session = ctx.config.is_flip_session();
        let track_bank = ctx.model.current_track_bank();
        let scene_bank = track_bank.scene_bank();
        let (tracks_back, tracks_fwd) = (track_bank.can_scroll_page_backwards(), track_bank.can_scroll_page_forwards());
        let (scenes_back, scenes_fwd) = (scene_bank.can_scroll_page_backwards(), scene_bank.can_scroll_page_forwards());
        Some(if flip_session {
            ScrollStates { can_scroll_left: scenes_back, can_scroll_right: scenes_fwd, can_scroll_up: tracks_back, can_scroll_down: tracks_fwd }
        } else {
            ScrollStates { can_scroll_left: tracks_back, can_scroll_right: tracks_fwd, can_scroll_up: scenes_back, can_scroll_down: scenes_fwd }
        })
    }

    fn is_birds_eye_active(&self) -> bool {
        self.session.is_birds_eye_active
    }

    fn set_birds_eye_active(&mut self, is_active: bool) {
        self.session.is_birds_eye_active = is_active;
    }

    fn session_alternative(&self) -> Option<&dyn SessionAlternative> {
        Some(self)
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn SessionAlternative> {
        Some(self)
    }

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
