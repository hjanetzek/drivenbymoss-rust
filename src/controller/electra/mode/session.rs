// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The session page of the normal model (former `SessionMode`): 5 scenes and 5x5 clips, with a
//! clip function and a navigation layer.

use super::{electra_mode_common, play_position_parameter, NormalModeBase};
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::daw::slot::Slot;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::parameterprovider::{provider, CombinedParameterProvider, EmptyParameterProvider, FixedParameterProvider, ParamRef};
use crate::midi::ButtonEvent;

/// The first group of the scene labels (former `FIRST_TRACK_GROUP`).
const FIRST_TRACK_GROUP: i32 = 550;

const FUNCTION_NAMES: [&str; 5] = ["New", "Delete", "Duplicate", "Quantize", "Stop"];
const NAVIGATION_NAMES: [&str; 5] = ["Tracks -", "Tracks +", "", "Scenes -", "Scenes+"];
const NAVIGATION_COLORS: [ColorEx; 5] = [ColorEx::PURPLE, ColorEx::PURPLE, ColorEx::BLACK, ColorEx::OLIVE, ColorEx::OLIVE];

/// The layers of the page (former `SessionUI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionUi {
    Normal,
    Functions,
    Navigation,
}

/// The session page (former `SessionMode`).
pub struct SessionMode {
    pub base: NormalModeBase,
    session_ui: SessionUi,
    clip_function: usize,
}

impl SessionMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = NormalModeBase::new(5, "Session");
        let empty5 = provider(EmptyParameterProvider::new(5));
        let empty1 = provider(EmptyParameterProvider::new(1));
        let mut providers = vec![
            // Row 1
            empty5.clone(),
            provider(FixedParameterProvider::single(ParamRef::MasterVolume)),
            // Row 2
            empty5.clone(),
            provider(FixedParameterProvider::single(play_position_parameter())),
        ];
        // Rows 3 to 6
        for _ in 0..4 {
            providers.push(empty5.clone());
            providers.push(empty1.clone());
        }
        base.electra.base.set_parameter_provider(ctx, provider(CombinedParameterProvider::new(providers)));
        SessionMode { base, session_ui: SessionUi::Normal, clip_function: 4 }
    }

    /// Former `getPadColor`.
    pub fn pad_color(ctx: &Ctx, slot: &Slot, is_armed: bool) -> ColorEx {
        if slot.is_selected() {
            return ColorEx::WHITE;
        }
        if slot.is_recording_queued() {
            return ColorEx::DARK_RED;
        }
        if slot.is_recording() {
            return ColorEx::RED;
        }
        if slot.is_playing_queued() {
            return ColorEx::DARK_GREEN;
        }
        if slot.is_playing() {
            return ColorEx::GREEN;
        }
        if slot.has_content() {
            return slot.color();
        }
        if slot.does_exist() && is_armed && ctx.config.is_draw_record_stripe() {
            ColorEx::RED_WINE
        } else {
            ColorEx::BLACK
        }
    }

    /// A pad of the clip rows (row 1 is the first track).
    fn on_clip_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        let track_index = row - 1;
        let bank = ctx.model.current_track_bank();
        let track = bank.item(track_index);
        if !track.does_exist() {
            return;
        }
        let slot = track.slot_bank().item(index);
        let (slot_exists, has_content, is_rec_arm) = (slot.does_exist(), slot.has_content(), track.is_rec_arm());
        match self.session_ui {
            SessionUi::Normal => {
                if !slot_exists {
                    return;
                }
                if !is_rec_arm || has_content {
                    ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(index).launch(event == ButtonEvent::Down, false);
                } else if let Some(track) = ctx.model.current_track_bank().ref_at(track_index) {
                    ctx.model.record_note_clip(track, index);
                }
            }
            SessionUi::Functions => {
                if event != ButtonEvent::Down {
                    return;
                }
                let slot_index = ctx.model.current_track_bank().item(track_index).slot_bank().item(index).index();
                match self.clip_function {
                    // New
                    0 => {
                        let length_in_beats = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
                        let track = ctx.model.current_track_bank_mut().item_mut(track_index);
                        track.create_clip(slot_index, length_in_beats);
                        track.slot_bank_mut().item_mut(index).select();
                    }
                    // Delete
                    1 => ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(index).remove(),
                    // Duplicate
                    2 => ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(index).duplicate(),
                    // Quantize
                    3 => {
                        ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(index).select();
                        ctx.schedule(100, |ctx| {
                            let amount = ctx.config.quantize_amount() as f64 / 100.0;
                            if let Some(clip) = ctx.model.cursor_clip() {
                                if clip.does_exist() {
                                    clip.quantize(amount);
                                }
                            }
                        });
                    }
                    // Stop
                    _ => ctx.model.current_track_bank_mut().item_mut(track_index).stop(false),
                }
            }
            SessionUi::Navigation => {
                if event != ButtonEvent::Down {
                    return;
                }
                // Calculate the page offsets
                let bank = ctx.model.current_track_bank();
                let num_tracks = bank.page_size().max(1) as i32;
                let scene_bank = ctx.model.scene_bank();
                let num_scenes = scene_bank.page_size().max(1) as i32;
                let track_position = bank.item(0).position() / num_tracks;
                let scene_position = scene_bank.scroll_position() / num_scenes;
                let pads_x = 5;
                let pads_y = 5;
                let offset_x = track_position / pads_x * pads_x;
                let offset_y = scene_position / pads_y * pads_y;
                let scene_target = offset_x * num_scenes + index as i32 * pads_y;
                let track_target = offset_y * num_tracks + track_index as i32 * pads_x;
                ctx.model.scene_bank_mut().scroll_to(scene_target.max(0) as usize, true);
                ctx.model.current_track_bank_mut().scroll_to(track_target.max(0) as usize, true);
            }
        }
    }
}

impl Mode for SessionMode {
    electra_mode_common!(base.electra);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event == ButtonEvent::Long {
            return;
        }
        if index == 5 {
            if event != ButtonEvent::Down {
                return;
            }
            match row {
                2 => self.session_ui = if self.session_ui == SessionUi::Navigation { SessionUi::Normal } else { SessionUi::Navigation },
                3 => self.session_ui = if self.session_ui == SessionUi::Functions { SessionUi::Normal } else { SessionUi::Functions },
                // Record
                4 => ctx.model.transport_mut().start_recording(),
                // Play
                5 => self.base.toggle_play(ctx),
                _ => {}
            }
            return;
        }

        // Scenes
        if row == 0 {
            match self.session_ui {
                SessionUi::Normal => ctx.model.scene_bank_mut().item_mut(index).launch(event == ButtonEvent::Down, false),
                SessionUi::Functions => {
                    if event == ButtonEvent::Down {
                        self.clip_function = index;
                    }
                }
                SessionUi::Navigation => {
                    if event == ButtonEvent::Down {
                        match index {
                            0 => ctx.model.current_track_bank_mut().select_previous_page(),
                            1 => ctx.model.current_track_bank_mut().select_next_page(),
                            3 => ctx.model.scene_bank_mut().select_previous_page(),
                            4 => ctx.model.scene_bank_mut().select_next_page(),
                            _ => {}
                        }
                    }
                }
            }
            return;
        }

        self.on_clip_button(ctx, row, index, event);
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let session_ui = self.session_ui;
        let clip_function = self.clip_function;
        {
            let cache = self.base.cache();
            cache.update_element(2, 5, None, Some(if session_ui == SessionUi::Navigation { ColorEx::SKY_BLUE } else { ColorEx::DARK_GRAY }), Some(true));
            cache.update_element(3, 5, None, Some(if session_ui == SessionUi::Functions { ColorEx::SKY_BLUE } else { ColorEx::DARK_GRAY }), Some(true));
        }

        let columns = 5;
        let rows = 5;
        let bank = ctx.model.current_track_bank();
        let scene_bank = ctx.model.scene_bank();

        if session_ui != SessionUi::Navigation {
            for (column, function_name) in FUNCTION_NAMES.iter().enumerate().take(columns) {
                let scene = scene_bank.item(column);
                let scene_label = if scene.does_exist() { format!("{}: {}", scene.position() + 1, scene.name()) } else { String::new() };
                let (scene_exists, scene_name, scene_color) = (scene.does_exist(), scene.name().to_string(), scene.color());
                let pads: Vec<(String, ColorEx, bool)> = (1..=rows)
                    .map(|row| {
                        let track = bank.item(row - 1);
                        let slot = track.slot_bank().item(column);
                        // Set the track name if the slot doesn't have one
                        let slot_exists = slot.does_exist();
                        let mut slot_name = if slot_exists && slot.has_content() { slot.name().to_string() } else { String::new() };
                        if slot_name.trim().is_empty() {
                            slot_name = if slot_exists { format!("({})", track.name()) } else { "-".to_string() };
                        }
                        (slot_name, Self::pad_color(ctx, slot, track.is_rec_arm()), slot_exists)
                    })
                    .collect();

                let cache = self.base.cache();
                cache.update_group_label(FIRST_TRACK_GROUP + column as i32, &scene_label);
                if session_ui == SessionUi::Functions {
                    cache.update_element(0, column, Some(function_name), Some(if clip_function == column { ColorEx::SKY_BLUE } else { ColorEx::DARK_GRAY }), Some(true));
                } else {
                    cache.update_element(0, column, Some(&scene_name), Some(scene_color), Some(scene_exists));
                }
                for (row, (name, color, exists)) in pads.into_iter().enumerate() {
                    cache.update_element(row + 1, column, Some(&name), Some(color), Some(exists));
                }
            }
        } else {
            // Navigation
            let num_tracks = bank.page_size().max(1) as i32;
            let num_scenes = scene_bank.page_size().max(1) as i32;
            let scene_count = scene_bank.item_count() as i32;
            let track_count = bank.item_count() as i32;
            let max_scene_pads = scene_count / num_scenes + i32::from(scene_count % num_scenes > 0);
            let max_track_pads = track_count / num_tracks + i32::from(track_count % num_tracks > 0);
            let scene_position = scene_bank.scroll_position();
            let track_position = bank.item(0).position();
            let scene_selection = scene_position / num_scenes + i32::from(scene_position % num_scenes > 0);
            let track_selection = track_position / num_tracks + i32::from(track_position % num_tracks > 0);
            let pads_x = columns as i32;
            let pads_y = rows as i32;
            let offset_x = track_selection / pads_x * pads_x;
            let offset_y = scene_selection / pads_y * pads_y;
            let max_x = max_track_pads - offset_x;
            let max_y = max_scene_pads - offset_y;
            let sel_x = track_selection - offset_x;
            let sel_y = scene_selection - offset_y;

            let cache = self.base.cache();
            for column in 0..columns {
                cache.update_element(0, column, Some(NAVIGATION_NAMES[column]), Some(NAVIGATION_COLORS[column]), Some(true));
                let row_color = if (column as i32) < max_x { ColorEx::RED } else { ColorEx::BLACK };
                for y in 0..rows {
                    let exists = (y as i32) < max_y;
                    let mut color = if exists { row_color } else { ColorEx::BLACK };
                    if sel_x == column as i32 && sel_y == y as i32 {
                        color = ColorEx::ORANGE;
                    }
                    let name = if exists && (column as i32) < max_x { format!("Sc. {} - Tr. {}", 1 + 5 * column, 1 + 5 * y) } else { "-".to_string() };
                    cache.update_element(column + 1, y, Some(&name), Some(color), Some(true));
                }
            }
        }

        self.base.update_master_column(ctx);
        self.base.flush(ctx);
    }
}
