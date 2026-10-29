// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Abstract implementation for a clip launcher grid (former `AbstractSessionView`).
//!
//! The overridable functions of the Java class are collected in the [`SessionHooks`] trait,
//! which the concrete view implements (usually on a small helper type, since the hooks are
//! called while the view state is borrowed).

use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::daw::slot::Slot;
use crate::framework::ctx::Ctx;
use crate::framework::grid::LightInfo;
use crate::framework::ids::ButtonId;
use crate::framework::view::base::scene_index;
use crate::midi::ButtonEvent;

/// The overridable parts of the session view.
pub trait SessionHooks {
    /// Returns true if the button is pressed (and marks it as consumed).
    fn is_button_combination(&self, ctx: &mut Ctx, button: ButtonId) -> bool {
        ctx.is_button_combination(button)
    }

    /// Is the alternate function (e.g. alternative launch) active?
    fn is_alternate_function(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_shift_pressed()
    }

    fn is_scene_launch_alternate_action(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_shift_pressed()
    }

    /// Should the scene only be selected?
    fn is_scene_select_action(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_select_pressed()
    }

    fn do_select_clip_on_launch(&self, ctx: &Ctx) -> bool {
        ctx.config.is_select_clip_on_launch()
    }

    /// Get the track (x) and slot (y) index of a pad (former `getPad`).
    fn pad(&self, ctx: &Ctx, session: &SessionViewBase, note: i32) -> (usize, usize) {
        let index = note - ctx.surface.grid_ref().start_note();
        let t = index % session.columns;
        let s = session.rows - 1 - index / session.columns;
        let (x, y) = if ctx.config.is_flip_session() { (s, t) } else { (t, s) };
        (x.max(0) as usize, y.max(0) as usize)
    }

    /// Called after the button combinations of a pressed pad were checked (for subclasses
    /// which extend the Java `handleButtonCombinations`).
    fn after_button_combinations(&self, _ctx: &mut Ctx, _track_index: usize, _slot_index: usize) {}

    /// The offset of the session grid from the top (former `getYOffset`).
    fn y_offset(&self) -> i32 {
        0
    }
}

/// The colors of the clip states.
#[derive(Debug, Clone, Copy)]
pub struct SessionColors {
    pub is_recording: LightInfo,
    pub is_recording_queued: LightInfo,
    pub is_playing: LightInfo,
    pub is_playing_queued: LightInfo,
    pub is_stop_queued: LightInfo,
    pub has_content: LightInfo,
    pub has_no_content: LightInfo,
    pub is_rec_armed: LightInfo,
    pub is_muted: LightInfo,
}

impl Default for SessionColors {
    /// Needs to be overwritten with device specific colors.
    fn default() -> Self {
        SessionColors {
            is_recording: LightInfo::new(0, -1, false),
            is_recording_queued: LightInfo::new(1, -1, false),
            is_playing: LightInfo::new(2, -1, false),
            is_playing_queued: LightInfo::new(3, -1, false),
            is_stop_queued: LightInfo::new(3, -1, false),
            has_content: LightInfo::new(4, -1, false),
            has_no_content: LightInfo::new(5, -1, false),
            is_rec_armed: LightInfo::new(6, -1, false),
            is_muted: LightInfo::new(7, -1, false),
        }
    }
}

/// The state of a session view.
pub struct SessionViewBase {
    pub clip_colors: SessionColors,
    pub bird_color_has_content: LightInfo,
    pub bird_color_selected: LightInfo,
    pub rows: i32,
    pub columns: i32,
    pub use_clip_color: bool,
    pub ignore_clip_color_for_play_and_record: bool,
    /// The slot to paste (track index and slot index in the page).
    pub source_slot: Option<(usize, usize)>,
    pub is_birds_eye_active: bool,
    was_alternate_interaction_used: bool,
}

impl SessionViewBase {
    pub fn new(rows: i32, columns: i32, use_clip_color: bool) -> Self {
        SessionViewBase {
            clip_colors: SessionColors::default(),
            bird_color_has_content: LightInfo::new(4, -1, false),
            bird_color_selected: LightInfo::new(2, -1, false),
            rows,
            columns,
            use_clip_color,
            ignore_clip_color_for_play_and_record: false,
            source_slot: None,
            is_birds_eye_active: false,
            was_alternate_interaction_used: false,
        }
    }

    pub fn toggle_birds_eye_view(&mut self) {
        self.is_birds_eye_active = !self.is_birds_eye_active;
    }

    pub fn was_alternate_interaction_used(&self) -> bool {
        self.was_alternate_interaction_used
    }

    pub fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.was_alternate_interaction_used = was_used;
    }

    /// Handle a scene button (former `AbstractView.onSceneButton` with the hooks of the session
    /// view).
    pub fn on_scene_button(ctx: &mut Ctx, hooks: &dyn SessionHooks, button: ButtonId, event: ButtonEvent) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event == ButtonEvent::Long {
            return;
        }
        if !ctx.model.scene_bank().item(index).does_exist() {
            return;
        }
        let is_pressed = event == ButtonEvent::Down;
        if is_pressed {
            // Former handleSceneButtonCombinations
            if hooks.is_button_combination(ctx, ButtonId::Delete) {
                ctx.model.scene_bank_mut().item_mut(index).remove();
                return;
            }
            if hooks.is_button_combination(ctx, ButtonId::Duplicate) {
                ctx.model.scene_bank_mut().item_mut(index).duplicate();
                return;
            }
            ctx.model.scene_bank_mut().item_mut(index).select();
            let name = ctx.model.scene_bank().item(index).name();
            ctx.notify(&name);
            // Only select the scene
            if hooks.is_scene_select_action(ctx) {
                return;
            }
        }
        let is_alternative = hooks.is_scene_launch_alternate_action(ctx);
        ctx.model.scene_bank_mut().item_mut(index).launch(is_pressed, is_alternative);
    }

    /// A pad was pressed: launch or record the clip.
    pub fn on_grid_note(&mut self, ctx: &mut Ctx, hooks: &dyn SessionHooks, note: i32, velocity: i32) {
        let is_alternate_function = hooks.is_alternate_function(ctx);
        if is_alternate_function {
            self.was_alternate_interaction_used = true;
        }

        let (track_index, slot_index) = hooks.pad(ctx, self, note);
        let is_pressed = velocity != 0;

        if is_pressed {
            let handled = self.handle_button_combinations(ctx, hooks, track_index, slot_index);
            hooks.after_button_combinations(ctx, track_index, slot_index);
            if handled {
                ctx.surface.consume_pads();
                return;
            }
            if hooks.do_select_clip_on_launch(ctx) {
                ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).select();
            }
        }

        let track = ctx.model.current_track_bank().item(track_index);
        if !track.is_rec_arm() || track.slot_bank().item(slot_index).has_content() {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).launch(is_pressed, is_alternate_function);
            return;
        }

        Self::handle_recording(ctx, track_index, slot_index);
    }

    /// Record or create a clip on a rec-armed track (former `handleRecording`).
    pub fn handle_recording(ctx: &mut Ctx, track_index: usize, slot_index: usize) {
        let Some(track) = ctx.model.current_track_bank().ref_at(track_index) else {
            return;
        };
        match ctx.config.action_for_rec_armed_pad() {
            0 => ctx.model.record_note_clip(track),
            1 => {
                let length_in_beats = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
                ctx.model.create_note_clip(track, slot_index, length_in_beats, true);
            }
            // Do nothing
            _ => {}
        }
    }

    /// Handle button combinations with a pad (delete, duplicate, stop and browse). Returns true
    /// if a combination was handled.
    pub fn handle_button_combinations(&mut self, ctx: &mut Ctx, hooks: &dyn SessionHooks, track_index: usize, slot_index: usize) -> bool {
        if !ctx.model.current_track_bank().item(track_index).does_exist() {
            return true;
        }

        // Delete selected clip
        if hooks.is_button_combination(ctx, ButtonId::Delete) {
            let slot = ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index);
            if slot.does_exist() {
                slot.remove();
            }
            return true;
        }

        // Duplicate a clip
        if hooks.is_button_combination(ctx, ButtonId::Duplicate) {
            let slot = ctx.model.current_track_bank().item(track_index).slot_bank().item(slot_index);
            if slot.does_exist() && slot.has_content() {
                self.source_slot = Some((track_index, slot_index));
            } else if let Some((source_track, source_slot)) = self.source_slot {
                let source = ctx.model.current_track_bank().item(source_track).slot_bank().item(source_slot);
                let (source_track_index, source_position) = (source.track_index(), source.position());
                ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).paste(source_track_index, source_position);
            }
            return true;
        }

        // Stop clip
        if hooks.is_button_combination(ctx, ButtonId::StopClip) {
            let is_alternative = hooks.is_alternate_function(ctx);
            ctx.model.current_track_bank_mut().item_mut(track_index).stop(is_alternative);
            return true;
        }

        // Browse for clips (only with a host browser, Reaper has none for slots)
        if hooks.is_button_combination(ctx, ButtonId::Browse) {
            let slot = ctx.model.current_track_bank().item(track_index).slot_bank().item(slot_index);
            let (name, track, position) = (slot.name().to_string(), slot.track_index(), slot.position());
            if let Some(browser) = ctx.model.browser_mut() {
                browser.replace_slot(&name, track, position);
            }
            return true;
        }

        false
    }

    /// Scroll the track and scene banks to the page of the pad in the birds eye view.
    pub fn on_grid_note_birds_eye_view(&self, ctx: &mut Ctx, x: i32, y: i32, y_offset: i32) {
        let flip = ctx.config.is_flip_session();

        // Calculate page offsets
        let tb = ctx.model.current_track_bank();
        let num_tracks = tb.page_size().max(1) as i32;
        let num_scenes = tb.scene_bank().page_size().max(1) as i32;
        let track_position = tb.item(0).position() / num_tracks;
        let scene_position = tb.scene_bank().scroll_position() / num_scenes;
        let sel_x = if flip { scene_position } else { track_position };
        let sel_y = if flip { track_position } else { scene_position };
        let pads_x = if flip { self.rows } else { self.columns };
        let pads_y = if flip { self.columns } else { self.rows + y_offset };
        let offset_x = sel_x / pads_x * pads_x;
        let offset_y = sel_y / pads_y * pads_y;
        let track_target = offset_x * num_tracks + (if flip { y } else { x }) * pads_x;
        let scene_target = offset_y * num_scenes + (if flip { x } else { y }) * pads_y;
        ctx.model.current_track_bank_mut().scroll_to(track_target.max(0) as usize, true);
        ctx.model.scene_bank_mut().scroll_to(scene_target.max(0) as usize, true);
    }

    /// Draw the session or the birds eye grid.
    pub fn draw_grid(&self, ctx: &mut Ctx, hooks: &dyn SessionHooks, ignore_flip_check: bool) {
        if self.is_birds_eye_active {
            self.draw_birds_eye_grid(ctx);
        } else {
            self.draw_session_grid(ctx, hooks, ignore_flip_check);
        }
    }

    /// Draw the clips of the session.
    pub fn draw_session_grid(&self, ctx: &mut Ctx, hooks: &dyn SessionHooks, ignore_flip_check: bool) {
        let flip_session = ctx.config.is_flip_session();
        if flip_session && self.columns != self.rows && !ignore_flip_check {
            ctx.host.error("Session flip is only supported for same size of rows and columns!");
            return;
        }

        for x in 0..self.columns {
            for y in 0..self.rows {
                let track = ctx.model.current_track_bank().item(x as usize);
                let is_armed = track.is_rec_arm();
                let color = self.pad_light(ctx, track.slot_bank().item(y as usize), is_armed);
                let (px, py) = if flip_session { (y, x) } else { (x, y) };
                ctx.surface.grid().light_ex_blink(px, py + hooks.y_offset(), color.color, color.blink_color, color.fast);
            }
        }
    }

    /// Draw the birds eye grid (each pad represents a page of tracks and scenes).
    pub fn draw_birds_eye_grid(&self, ctx: &mut Ctx) {
        let tb = ctx.model.current_track_bank();
        let scene_bank = ctx.model.scene_bank();
        let num_tracks = tb.page_size().max(1) as i32;
        let num_scenes = scene_bank.page_size().max(1) as i32;
        let scene_count = scene_bank.item_count() as i32;
        let track_count = tb.item_count() as i32;
        let max_scene_pads = scene_count / num_scenes + i32::from(scene_count % num_scenes > 0);
        let max_track_pads = track_count / num_tracks + i32::from(track_count % num_tracks > 0);
        let scene_position = scene_bank.scroll_position();
        let track_position = tb.item(0).position();
        let scene_selection = scene_position / num_scenes + i32::from(scene_position % num_scenes > 0);
        let track_selection = track_position / num_tracks + i32::from(track_position % num_tracks > 0);
        let flip_session = ctx.config.is_flip_session();
        let mut sel_x = if flip_session { scene_selection } else { track_selection };
        let mut sel_y = if flip_session { track_selection } else { scene_selection };
        let pads_x = if flip_session { self.rows } else { self.columns };
        let pads_y = if flip_session { self.columns } else { self.rows };
        let offset_x = sel_x / pads_x * pads_x;
        let offset_y = sel_y / pads_y * pads_y;
        let max_x = (if flip_session { max_scene_pads } else { max_track_pads }) - offset_x;
        let max_y = (if flip_session { max_track_pads } else { max_scene_pads }) - offset_y;
        sel_x -= offset_x;
        sel_y -= offset_y;

        let grid = ctx.surface.grid();
        for x in 0..self.columns {
            let row_color = if x < max_x { self.bird_color_has_content } else { self.clip_colors.has_no_content };
            for y in 0..self.rows {
                let mut color = if y < max_y { row_color } else { self.clip_colors.has_no_content };
                if sel_x == x && sel_y == y {
                    color = self.bird_color_selected;
                }
                grid.light_ex_blink(x, y, color.color, color.blink_color, color.fast);
            }
        }
    }

    /// Get the light of a pad for a slot (former `getPadColor`).
    pub fn pad_light(&self, ctx: &Ctx, slot: &Slot, is_armed: bool) -> LightInfo {
        let color_id = DawColor::color_id(slot.color());
        let colors = &self.clip_colors;
        if slot.is_recording_queued() {
            return colors.is_recording_queued;
        }
        if slot.is_recording() {
            return self.insert_clip_color(ctx, color_id, colors.is_recording);
        }
        if slot.is_playing_queued() {
            return self.insert_clip_color(ctx, color_id, colors.is_playing_queued);
        }
        if slot.is_stop_queued() {
            return self.insert_clip_color(ctx, color_id, colors.is_stop_queued);
        }
        if slot.is_playing() {
            return self.insert_clip_color(ctx, color_id, colors.is_playing);
        }

        if slot.has_content() {
            if slot.is_muted() {
                return LightInfo::new(colors.is_muted.color, -1, false);
            }
            let blink_color = colors.has_content.blink_color;
            let color = if self.use_clip_color { ctx.colors.color_index(color_id) } else { colors.has_content.color };
            return LightInfo::new(color.clamp(0, 127), if slot.is_selected() { blink_color } else { -1 }, colors.has_content.fast);
        }

        if slot.does_exist() && is_armed && ctx.config.is_draw_record_stripe() {
            colors.is_rec_armed
        } else {
            colors.has_no_content
        }
    }

    fn insert_clip_color(&self, ctx: &Ctx, color_id: &str, light_info: LightInfo) -> LightInfo {
        if self.use_clip_color && !self.ignore_clip_color_for_play_and_record {
            let blink_color = light_info.blink_color;
            if blink_color > 0 {
                return LightInfo::new(ctx.colors.color_index(color_id).clamp(0, 127), blink_color, light_info.fast);
            }
        }
        light_info
    }
}
