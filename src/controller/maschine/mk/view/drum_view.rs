// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The drum view (former `DrumView`): the 16 pads play the drum pads, alternatively the pads
//! show the 16 sequencer steps of the selected drum pad or the grid editor (resolution and
//! page selection).

use crate::config::SettingId;
use crate::controller::maschine::core::colors::{COLOR_BLACK, COLOR_BLUE, COLOR_SKIN};
use crate::daw::clip::{NotePosition, StepState};
use crate::daw::constants::Resolution;
use crate::framework::command::AftertouchCommand;
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::commands::mv_helper::notify_edit_page;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::view::sequencer::abstract_drum_view::{apply_lights, DrumBase, DrumHooks, PadLight};
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

/// The number of steps of the clip.
pub const NUM_STEPS: usize = 16;
/// The key of the monitored clip (columns, rows).
pub const CLIP_KEY: (usize, usize) = (NUM_STEPS, 128);

/// The Maschine specific behaviour of the drum sequencer.
struct Hooks;

impl Hooks {
    /// Former `getPadIndex`.
    fn pad_index(base: &DrumBase, step: i32) -> i32 {
        let x = step % base.num_columns;
        let y = step / base.num_columns;
        y * base.num_columns + x
    }
}

impl DrumHooks for Hooks {
    fn handle_sequencer_area(&mut self, base: &mut DrumBase, ctx: &mut Ctx, index: i32, x: i32, y: i32, offset_y: i32, velocity: i32) {
        let y_mod = 3 - y;
        base.handle_sequencer_area(self, ctx, index, x, y_mod, offset_y, velocity);
    }

    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        if ctx.is_button_combination(ButtonId::Mute) {
            let clip = base.seq.clip(ctx);
            let step_info = clip.step(note_position);
            if step_info.state() == StepState::Start {
                let is_muted = step_info.is_muted();
                clip.update_step_mute_state(note_position, !is_muted);
            }
            return true;
        }

        if ctx.modes.is_active(Modes::Note) {
            ctx.host.show_notification(&format!("Note {} - Step {}", Scales::format_note_and_octave(note_position.note, -3), note_position.step + 1));
            base.seq.edit_note(ctx, note_position, true);
            return true;
        }

        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            if velocity > 0 {
                base.seq.handle_sequencer_area_repeat_operator(ctx, note_position, velocity, true);
            }
            return true;
        }

        // The generic part (former super call) with the Maschine pad index
        if ctx.is_button_combination(ButtonId::Duplicate) {
            let clip = base.seq.clip(ctx);
            let note_step = clip.step(note_position);
            if note_step.state() == StepState::Start {
                base.copy_note = Some(note_step.clone());
            } else if let Some(copy_note) = &base.copy_note {
                clip.set_step_from(note_position, copy_note);
            }
            return true;
        }

        let mut np = *note_position;
        let step = np.step;
        let note = np.note;
        for s in 0..step {
            let pad = Self::pad_index(base, s);
            let id = ButtonId::get(base.first_pad, pad as usize);
            if ctx.surface.is_long_pressed(id) {
                ctx.surface.set_trigger_consumed(id);
                let length = step - s + 1;
                let duration = length as f64 * Resolution::value_at(base.seq.resolution_index(ctx));
                np.step = s;
                let clip = base.seq.clip(ctx);
                let state = if note < 0 { StepState::Off } else { clip.step(&np).state() };
                if state == StepState::Start {
                    clip.update_step_duration(&np, duration);
                } else {
                    clip.set_step(&np, velocity, duration);
                }
                return true;
            }
        }
        false
    }
}

/// The drum view.
pub struct DrumView {
    pub base: DrumBase,
    hooks: Hooks,
    aftertouch: Box<dyn AftertouchCommand>,
    is_shifted: bool,
    is_sequencer_visible: bool,
    is_grid_editor: bool,
}

impl DrumView {
    pub fn new(_ctx: &Ctx) -> Self {
        let mut base = DrumBase::with_layout("Drum", 0, 4, 4, 128, NUM_STEPS, true, true);
        base.sequencer_steps = NUM_STEPS as i32;
        DrumView {
            base,
            hooks: Hooks,
            aftertouch: Box::new(AftertouchViewCommand::new()),
            is_shifted: false,
            is_sequencer_visible: false,
            is_grid_editor: false,
        }
    }

    pub fn toggle_sequencer_visible(&mut self, ctx: &mut Ctx) {
        self.is_sequencer_visible = !self.is_sequencer_visible;
        self.update_note_mapping(ctx);
    }

    pub fn is_sequencer_visible(&self) -> bool {
        self.is_sequencer_visible
    }

    pub fn toggle_grid_editor(&mut self, ctx: &mut Ctx) {
        self.is_grid_editor = !self.is_grid_editor;
        self.update_note_mapping(ctx);
    }

    pub fn is_grid_editor(&self) -> bool {
        self.is_grid_editor
    }

    pub fn toggle_shifted(&mut self, ctx: &mut Ctx) {
        self.is_shifted = !self.is_shifted;
        self.update_note_mapping(ctx);
    }

    pub fn is_shifted(&self) -> bool {
        self.is_shifted
    }

    /// Get the resolution index (0-7) of the sequencer.
    pub fn resolution_index(&self, ctx: &Ctx) -> usize {
        self.base.seq.resolution_index(ctx)
    }

    /// Change the drum octave (former `changeOctave`).
    pub fn change_octave(&mut self, ctx: &mut Ctx, event: ButtonEvent, is_up: bool, offset: i32, adjust_page: bool, notify: bool) {
        self.base.change_octave(ctx, event, is_up, offset, adjust_page, notify);
    }

    fn draw_shifted_grid(ctx: &mut Ctx) {
        let is_keyboard_enabled = ctx.model.can_selected_track_hold_notes();
        let grid = ctx.surface.grid();
        for i in 36..50 {
            grid.light(i, COLOR_BLACK);
        }
        for i in 50..52 {
            grid.light(i, if is_keyboard_enabled { COLOR_SKIN } else { COLOR_BLACK });
        }
    }
}

impl View for DrumView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, button_color_id, sequencer_resolution_index,
        set_sequencer_resolution_index);
    // AbstractDrumView
    crate::forward_view!(base => on_activate, on_deactivate, update_control_surface,
        on_model_event);

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        if self.is_shifted || self.is_sequencer_visible || self.is_grid_editor {
            self.base.seq.view.clear_note_mapping(ctx);
        } else {
            self.base.update_note_mapping(ctx);
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if self.is_grid_editor {
            let ids: Vec<&'static str> = (0..8).map(|i| self.base.seq.button_color_id(ctx, ButtonId::get(ButtonId::Scene1, 7 - i))).collect();
            let Ctx { surface, colors, .. } = ctx;
            let grid = surface.grid();
            for (i, id) in ids.iter().enumerate() {
                grid.light_id(colors, 36 + i as i32, id);
            }
            for i in 8..12 {
                grid.light(36 + i, COLOR_BLACK);
            }
            for i in 12..14 {
                grid.light(36 + i, COLOR_BLUE);
            }
            for i in 14..16 {
                grid.light(36 + i, COLOR_BLACK);
            }
            return;
        }

        if self.is_sequencer_visible {
            self.base.seq.clip(ctx);
            let is_active = self.base.seq.is_active();
            let selected_pad = self.base.selected_pad();
            let note_row = ctx.model.scales().drum_offset() + selected_pad;
            let row_color = self.base.drum_device(ctx).and_then(|primary| self.base.pad_color(primary, selected_pad));
            let mut lights = Vec::new();
            self.base.draw_sequencer_steps(ctx, is_active, note_row, row_color, &mut lights);
            let lights: Vec<(i32, i32, PadLight)> = lights.into_iter().map(|(x, y, light)| (x, 3 - y, light)).collect();
            apply_lights(ctx, lights);
            return;
        }

        if self.is_shifted {
            Self::draw_shifted_grid(ctx);
        } else {
            self.base.draw_grid(ctx);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let index = note - ctx.surface.grid_ref().start_note();
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }

        if self.is_grid_editor {
            if velocity == 0 {
                return;
            }
            if index < 8 {
                self.base.seq.set_resolution_index(ctx, index);
            } else {
                let clip = self.base.seq.clip(ctx);
                if index == 12 {
                    clip.scroll_steps_page_backwards();
                } else if index == 13 {
                    clip.scroll_steps_page_forward();
                }
                notify_edit_page(ctx, CLIP_KEY);
            }
            return;
        }

        if self.is_sequencer_visible {
            let x = index % self.base.num_columns;
            let y = index / self.base.num_columns;
            let offset_y = ctx.model.scales().drum_offset();
            if self.base.seq.is_active() {
                self.hooks.handle_sequencer_area(&mut self.base, ctx, index, x, y, offset_y, velocity);
            }
            return;
        }

        if !self.is_shifted {
            self.base.on_grid_note(&mut self.hooks, ctx, note, velocity);
            return;
        }

        if velocity == 0 {
            return;
        }
        match index {
            14 => self.base.on_octave_down(ctx, ButtonEvent::Down),
            15 => self.base.on_octave_up(ctx, ButtonEvent::Down),
            _ => {}
        }
        self.update_note_mapping(ctx);
    }

    fn on_setting_changed(&mut self, _ctx: &mut Ctx, _id: SettingId) {}

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for DrumView {
    // AbstractDrumView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}
