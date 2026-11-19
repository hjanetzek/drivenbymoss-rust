// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The session mode (former `SessionMode`): shows the clips (or scenes) on the display.
//!
//! Differences to Java:
//! - Java derives it from `AbstractTrackMode`; the parts it uses (the first row track
//!   selection, the second row button 7 and the track paging with the cursor keys options) are
//!   ported here, since the track modes are a separate module.
//! - The mode owns the state of its clip launcher navigator.
//! - Mode switches and the Device button trigger are executed right after the button handler.

use super::{forward_base, row_index, shorten_and_fix_ascii, Push3Encoder};
use crate::config::push as config;
use crate::config::values::{CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_1, CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE, CURSOR_KEYS_TRACK_OPTION_SWAP};
use crate::controller::push::colors::{PUSH2_COLOR2_WHITE, PUSH2_COLOR_BLACK, PUSH_BLACK, PUSH_ORANGE_HI, PUSH_RED_HI, PUSH_RED_LO, PUSH_YELLOW_LO};
use crate::controller::push::display1::{DEGREE, RIGHT_ARROW, SELECT_ARROW};
use crate::controller::push::mode::PushDisplayMode;
use crate::controller::push::surface;
use crate::daw::bank::BankItem;
use crate::daw::scene::SceneBank;
use crate::framework::clip_launcher_navigator::{self as navigator, SharedNavigator};
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::{GraphicDisplay, SceneInfo, SlotInfo};
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::parameter_mode::{default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, VolumeParameterProvider};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The size of the scene bank of the mode.
pub const NUM_SCENES: usize = 64;

/// Which rows are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowDisplayMode {
    All,
    Upper,
    Lower,
}

fn scene_bank(ctx: &Ctx) -> &SceneBank {
    ctx.model.scene_bank_sized(NUM_SCENES).unwrap_or_else(|| ctx.model.scene_bank())
}

fn with_scene_bank_mut(ctx: &mut Ctx, f: impl FnOnce(&mut SceneBank)) {
    if let Some(bank) = ctx.model.scene_bank_sized_mut(NUM_SCENES) {
        f(bank);
    } else {
        f(&mut ctx.model.scene_bank_mut());
    }
}

/// Shows the clips or scenes on the display, the knobs control the volumes.
pub struct SessionMode {
    base: ParameterModeBase,
    row_display_mode: RowDisplayMode,
    navigator: SharedNavigator,
}

impl SessionMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = ParameterModeBase::new("Session", false, ModeBank::TrackBank, default_knob_ids());
        base.set_parameter_provider(ctx, provider(VolumeParameterProvider::new(ctx)));
        let row_display_mode = if surface::is_modern(ctx) { RowDisplayMode::All } else { RowDisplayMode::Upper };
        SessionMode { base, row_display_mode, navigator: SharedNavigator::default() }
    }

    /// The rows which are shown.
    pub fn row_display_mode(&self) -> RowDisplayMode {
        self.row_display_mode
    }

    /// Former `AbstractTrackMode.onFirstRow`.
    fn on_first_row(ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            return;
        }
        if event == ButtonEvent::Up {
            for (button, action) in [(ButtonId::Duplicate, 0), (ButtonId::Delete, 1), (ButtonId::StopClip, 2), (ButtonId::Record, 3)] {
                if ctx.surface.is_pressed(button) {
                    ctx.surface.set_trigger_consumed(button);
                    let track = ctx.model.current_track_bank_mut().item_mut(index);
                    match action {
                        0 => track.duplicate(),
                        1 => track.remove(),
                        2 => track.stop(true),
                        _ => track.toggle_rec_arm(),
                    }
                    return;
                }
            }
            if ctx.surface.is_select_pressed() {
                ctx.surface.set_trigger_consumed(ButtonId::Select);
                ctx.model.current_track_bank_mut().item_mut(index).toggle_multi_select();
                return;
            }
            let track = ctx.model.current_track_bank().item(index);
            if !track.is_selected() {
                ctx.model.current_track_bank_mut().item_mut(index).select();
                return;
            }
            // If it is a group display child channels of group, otherwise jump into device mode
            if track.is_group() {
                if ctx.surface.is_shift_pressed() || ctx.config.is_track_navigation_flat() {
                    ctx.model.current_track_bank_mut().item_mut(index).toggle_group_expanded();
                } else {
                    let bank = ctx.model.current_track_bank_mut();
                    bank.item_mut(index).set_group_expanded(true);
                    bank.enter(index);
                }
            } else {
                ctx.schedule(0, |ctx| ctx.trigger_button(ButtonId::Device, ButtonEvent::Down, 127.0));
            }
            return;
        }
        // LONG press, go out of group
        ctx.model.track_bank_mut().select_parent();
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Row11, index));
    }

    /// Former `AbstractTrackMode.lastSendIsAccessible`.
    fn last_send_is_accessible(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_shift_pressed() || !ctx.model.current_track_bank().has_parent() || self.base.is_knob_touched(7)
    }

    /// Former `AbstractTrackMode.onSecondRow` for the button 7 (the only one which reaches it).
    fn on_second_row_track(&self, ctx: &mut Ctx, index: usize) {
        let state = |ctx: &Ctx, button: ButtonId| ctx.surface.is_long_pressed(button);
        if surface::is_mute_state(ctx, state(ctx, ButtonId::Mute)) {
            ctx.surface.set_trigger_consumed(ButtonId::Mute);
            ctx.model.current_track_bank_mut().item_mut(index).toggle_mute();
            return;
        }
        if surface::is_solo_state(ctx, state(ctx, ButtonId::Solo)) {
            ctx.surface.set_trigger_consumed(ButtonId::Solo);
            ctx.model.current_track_bank_mut().item_mut(index).toggle_solo();
            return;
        }
        if surface::is_clip_stop_state(ctx, state(ctx, ButtonId::StopClip)) {
            ctx.surface.set_trigger_consumed(ButtonId::StopClip);
            let is_shift = ctx.surface.is_shift_pressed();
            let track = ctx.model.current_track_bank_mut().item_mut(index);
            if is_shift {
                track.return_to_arrangement();
            } else {
                track.stop(is_shift);
            }
            return;
        }
        let mut new_mode = None;
        if self.last_send_is_accessible(ctx) {
            // Former handleSendEffect (3)
            if ctx.model.current_track_bank().can_edit_send(3) {
                new_mode = Some(if ctx.modes.is_active(Modes::Send4) { Modes::Track } else { Modes::Send4 });
            }
        } else {
            ctx.model.track_bank_mut().select_parent();
        }
        ctx.schedule(0, move |ctx| {
            if let Some(mode) = new_mode {
                ctx.set_active_mode(mode);
            }
            if let Some(active) = ctx.modes.active_id() {
                config::set_mixer_mode(&mut ctx.config, active);
            }
        });
    }

    fn cursor_option(ctx: &Ctx) -> i32 {
        if ctx.surface.is_shift_pressed() {
            ctx.config.cursor_keys_track_shifted_option()
        } else {
            ctx.config.cursor_keys_track_option()
        }
    }

    fn update_display1_scenes(&self, ctx: &Ctx, display: &mut TextDisplay) {
        let max_rows = if self.row_display_mode == RowDisplayMode::All { 8 } else { 4 };
        let bank = scene_bank(ctx);
        for col in 0..8 {
            for row in 0..max_rows {
                let mut scene_index = (max_rows - 1 - row) * 8 + col;
                if self.row_display_mode == RowDisplayMode::Lower {
                    scene_index += 32;
                }
                let scene = bank.item(scene_index);
                if !scene.does_exist() {
                    continue;
                }
                let is_sel = scene.is_selected();
                let n = shorten_and_fix_ascii(&scene.name_limited(8), if is_sel { 7 } else { 8 });
                display.set_cell(row, col, &if is_sel { format!("{SELECT_ARROW}{n}") } else { n });
            }
        }
    }

    /// The columns and rows of the clips and the slot (track, slot index) of a cell.
    fn clip_layout(&self, ctx: &Ctx) -> (usize, usize, impl Fn(usize, usize) -> (usize, usize)) {
        let tb = ctx.model.current_track_bank();
        let num_tracks = tb.page_size();
        let num_scenes = tb.scene_bank().page_size();
        let flip_session = ctx.config.is_flip_session();
        let max_cols = if flip_session { num_scenes } else { num_tracks };
        let mut max_rows = if flip_session { num_tracks } else { num_scenes };
        if self.row_display_mode != RowDisplayMode::All {
            max_rows /= 2;
        }
        let is_lower = self.row_display_mode == RowDisplayMode::Lower;
        let position = move |col: usize, row: usize| {
            let mut x = if flip_session { row } else { col };
            let mut y = if flip_session { col } else { row };
            if is_lower {
                if flip_session {
                    x += max_rows;
                } else {
                    y += max_rows;
                }
            }
            (x, y)
        };
        (max_cols, max_rows, position)
    }

    fn update_display1_clips(&self, ctx: &Ctx, display: &mut TextDisplay) {
        let (max_cols, max_rows, position) = self.clip_layout(ctx);
        let tb = ctx.model.current_track_bank();
        for col in 0..max_cols {
            for row in 0..max_rows {
                let (x, y) = position(col, row);
                let track = tb.item(x);
                let slot = track.slot_bank().item(y);
                if !slot.does_exist() {
                    continue;
                }
                let mut name = slot.name_limited(8);
                if track.is_group() {
                    if name.is_empty() {
                        name = format!("Scene {}", slot.position() + 1);
                    }
                } else if name.is_empty() {
                    // TODO Bugfix required: Workaround to displaying unnamed clips, since
                    // doesExist does not work reliably
                    let color = slot.color();
                    if color.red != 0.0 || color.green != 0.0 || color.blue != 0.0 {
                        name = "--------".to_string();
                    }
                }
                let prefix = if slot.is_selected() {
                    SELECT_ARROW.to_string()
                } else if slot.is_playing() {
                    ">".to_string()
                } else if slot.is_playing_queued() || slot.is_recording_queued() {
                    ")".to_string()
                } else if track.is_rec_arm() || slot.is_recording() {
                    "*".to_string()
                } else if slot.has_content() {
                    RIGHT_ARROW.to_string()
                } else {
                    DEGREE.to_string()
                };
                display.set_cell(row, col, &shorten_and_fix_ascii(&format!("{prefix}{name}"), 8));
            }
        }
    }

    fn update_display2_scenes(&self, ctx: &Ctx, display: &mut GraphicDisplay) {
        let max_rows = if self.row_display_mode == RowDisplayMode::All { 8 } else { 4 };
        let tb = ctx.model.current_track_bank();
        let bank = scene_bank(ctx);
        let is_pinned = ctx.model.is_cursor_track_pinned();
        for col in 0..8 {
            let scenes = (0..max_rows)
                .map(|row| {
                    let mut scene_index = (max_rows - 1 - row) * 8 + col;
                    if self.row_display_mode == RowDisplayMode::Lower {
                        scene_index += 32;
                    }
                    SceneInfo::from_scene(bank.item(scene_index))
                })
                .collect();
            let track = tb.item(col);
            display.add_scene_list_element(scenes, Some(track.channel_type()), track.name(), track.color(), track.is_selected(), track.is_activated(), is_pinned);
        }
    }

    fn update_display2_clips(&self, ctx: &Ctx, display: &mut GraphicDisplay) {
        let (max_cols, max_rows, position) = self.clip_layout(ctx);
        let tb = ctx.model.current_track_bank();
        let is_pinned = ctx.model.is_cursor_track_pinned();
        for col in 0..max_cols {
            let slots = (0..max_rows)
                .map(|row| {
                    let (x, y) = position(col, row);
                    let track = tb.item(x);
                    SlotInfo::from_track_slot(track, track.slot_bank().item(y))
                })
                .collect();
            let track = tb.item(col);
            display.add_slot_list_element(slots, Some(track.channel_type()), track.name(), track.color(), track.is_selected(), track.is_activated(), is_pinned);
        }
    }

    /// Former `StartSceneCommand.execute` on the 64 scene bank with the selected scene.
    fn start_selected_scene(ctx: &mut Ctx, event: ButtonEvent) {
        if ctx.surface.is_shift_pressed() {
            if event == ButtonEvent::Down {
                ctx.model.project_mut().create_scene_from_playing_launcher_clips();
            }
            return;
        }
        if event == ButtonEvent::Long {
            return;
        }
        let bank = scene_bank(ctx);
        let Some(index) = (0..bank.page_size()).find(|i| bank.item(*i).is_selected()) else {
            return;
        };
        if !bank.item(index).does_exist() {
            return;
        }
        let is_down = event == ButtonEvent::Down;
        if is_down {
            // Delete the scene
            if ctx.surface.is_delete_pressed() {
                with_scene_bank_mut(ctx, |bank| bank.item_mut(index).remove());
                return;
            }
            if ctx.config.is_select_clip_on_launch() {
                with_scene_bank_mut(ctx, |bank| bank.item_mut(index).select());
                delay_display(ctx, move |ctx| scene_bank(ctx).item(index).name());
            }
        }
        with_scene_bank_mut(ctx, |bank| bank.item_mut(index).launch(is_down, false));
    }

    /// Former `StartClipCommand.execute`.
    fn start_selected_clip(ctx: &mut Ctx, event: ButtonEvent) {
        let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() else {
            return;
        };
        let Some(index) = track.slot_bank().selected_item().map(|s| s.index().max(0) as usize) else {
            return;
        };
        track.slot_bank_mut().item_mut(index).launch(event == ButtonEvent::Down, false);
    }
}

impl PushDisplayMode for SessionMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        if config::is_scenes_clip_view_selected(&ctx.config) {
            self.update_display1_scenes(ctx, display);
        } else {
            self.update_display1_clips(ctx, display);
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        if config::is_scenes_clip_view_selected(&ctx.config) {
            self.update_display2_scenes(ctx, display);
        } else {
            self.update_display2_clips(ctx, display);
        }
    }
}

impl Mode for SessionMode {
    forward_base!();

    fn on_knob_value(&mut self, _ctx: &mut Ctx, _index: usize, _value: i32) {
        // Intentionally empty
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, _ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            Self::on_first_row(ctx, index, event);
            return;
        }
        if event != ButtonEvent::Down {
            return;
        }
        if surface::is_modern(ctx) {
            match index {
                0 => {
                    self.row_display_mode = if self.row_display_mode == RowDisplayMode::Upper { RowDisplayMode::All } else { RowDisplayMode::Upper };
                }
                1 => {
                    self.row_display_mode = if self.row_display_mode == RowDisplayMode::Lower { RowDisplayMode::All } else { RowDisplayMode::Lower };
                }
                7 => self.on_second_row_track(ctx, index),
                _ => {}
            }
        } else if index < 2 {
            self.row_display_mode = if self.row_display_mode == RowDisplayMode::Upper { RowDisplayMode::Lower } else { RowDisplayMode::Upper };
        } else if index == 7 {
            ctx.model.track_bank_mut().select_parent();
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if let Some(index) = row_index(1, button) {
            if index == 0 {
                return ctx.colors.color_index(if self.row_display_mode == RowDisplayMode::Upper { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON });
            }
            if index == 1 {
                return ctx.colors.color_index(if self.row_display_mode == RowDisplayMode::Lower { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON });
            }
            if index < 5 {
                return ctx.colors.color_index(BUTTON_COLOR_OFF);
            }
            return if ctx.model.current_track_bank().has_parent() { PUSH2_COLOR2_WHITE } else { PUSH2_COLOR_BLACK };
        }
        if let Some(index) = row_index(0, button) {
            // Former AbstractTrackMode.getButtonColor
            let track = ctx.model.current_track_bank().item(index);
            if !track.does_exist() || !track.is_activated() {
                return ctx.colors.color_index(PUSH_BLACK);
            }
            let cursor_track = ctx.model.cursor_track();
            let sel_index = if cursor_track.does_exist() { cursor_track.index() } else { -1 };
            let is_sel = track.index() == sel_index;
            if track.is_rec_arm() {
                return ctx.colors.color_index(if is_sel { PUSH_RED_HI } else { PUSH_RED_LO });
            }
            return ctx.colors.color_index(if is_sel { PUSH_ORANGE_HI } else { PUSH_YELLOW_LO });
        }
        ctx.colors.color_index(BUTTON_COLOR_OFF)
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.base.select_item(ctx, index);
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        match Self::cursor_option(ctx) {
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE => self.base.select_previous_item_page(ctx),
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_1 => ctx.model.current_track_bank_mut().scroll_backwards(),
            CURSOR_KEYS_TRACK_OPTION_SWAP => ctx.model.cursor_track_swap_with_previous(),
            _ => {}
        }
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        match Self::cursor_option(ctx) {
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE => self.base.select_next_item_page(ctx),
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_1 => ctx.model.current_track_bank_mut().scroll_forwards(),
            CURSOR_KEYS_TRACK_OPTION_SWAP => ctx.model.cursor_track_swap_with_next(),
            _ => {}
        }
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_shifted_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_previous_item_page(ctx)
        } else {
            self.base.has_previous_item(ctx)
        }
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_shifted_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_next_item_page(ctx)
        } else {
            self.base.has_next_item(ctx)
        }
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_previous_item_page(ctx)
        } else {
            self.base.has_previous_item(ctx)
        }
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_next_item_page(ctx)
        } else {
            self.base.has_next_item(ctx)
        }
    }

    impl_as_any!();
}

impl Push3Encoder for SessionMode {
    fn encoder_turn(&mut self, ctx: &mut Ctx, value: i32) {
        let is_left = !ctx.model.value_changer().is_increase(value);
        if config::is_scenes_clip_view_selected(&ctx.config) {
            navigator::navigate_scenes(ctx, is_left);
            return;
        }
        if ctx.config.is_flip_session() {
            navigator::navigate_tracks(&self.navigator, ctx, is_left);
        } else {
            navigator::navigate_clips(ctx, is_left);
        }
    }

    fn encoder_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.encoder_left_right(ctx, event, true);
    }

    fn encoder_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.encoder_left_right(ctx, event, false);
    }

    fn encoder_press(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if config::is_scenes_clip_view_selected(&ctx.config) {
            Self::start_selected_scene(ctx, event);
        } else {
            Self::start_selected_clip(ctx, event);
        }
    }

    fn arrow_center(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        // Same as default
        let bank = ctx.model.scene_bank();
        if let Some(index) = (0..bank.page_size()).find(|i| bank.item(*i).is_selected()) {
            ctx.model.scene_bank_mut().item_mut(index).launch(event == ButtonEvent::Down, true);
        }
    }
}

impl SessionMode {
    fn encoder_left_right(&mut self, ctx: &mut Ctx, event: ButtonEvent, is_left: bool) {
        if event != ButtonEvent::Down {
            return;
        }
        if config::is_scenes_clip_view_selected(&ctx.config) {
            navigator::navigate_scenes(ctx, is_left);
            return;
        }
        if ctx.config.is_flip_session() {
            navigator::navigate_clips(ctx, is_left);
        } else {
            navigator::navigate_tracks(&self.navigator, ctx, is_left);
        }
    }
}
