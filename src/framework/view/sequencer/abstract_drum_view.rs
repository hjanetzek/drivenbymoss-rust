// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A drum sequencer: drum pads in the lower left, clip pages in the lower right and the
//! sequencer steps of the selected pad in the upper part (former `AbstractDrumView` and
//! `AbstractDrumExView`, whose extra buttons can be enabled with
//! [`DrumBase::with_extra_buttons`]).

use std::cell::Cell;
use std::rc::Rc;

use super::abstract_sequencer_view::{selected_track_index, SequencerBase};
use crate::daw::bank::BankItem;
use crate::daw::clip::{NotePosition, StepInfo, StepState};
use crate::daw::color::{ColorEx, DawColor};
use crate::daw::constants::Resolution;
use crate::daw::device::{DrumPadBank, SpecificDevice};
use crate::daw::events::ModelEvent;
use crate::framework::color_ids::abstract_drum_ex_view::*;
use crate::framework::color_ids::abstract_drum_view::*;
use crate::framework::color_ids::abstract_sequencer_view::COLOR_NO_CONTENT;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;
use crate::scale::{NoteMatrix, Scales};

pub const GRID_COLUMNS: i32 = 8;

/// `MaschineColorManager.COLOR_GREEN`, used as a raw color index for the accent velocity pads.
const ACCENT_COLOR_SELECTED: i32 = 30;
/// `MaschineColorManager.COLOR_LIME_LO`, used as a raw color index for the accent velocity pads.
const ACCENT_COLOR: i32 = 33;

/// A light of a pad: either a color ID or a raw color index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadLight {
    Id(&'static str),
    Index(i32),
}

/// The hooks of the drum sequencer, which controller specific views can override (former
/// overridable methods). The defaults call the generic implementation.
pub trait DrumHooks {
    /// Former `handleSequencerArea`.
    #[allow(clippy::too_many_arguments)]
    fn handle_sequencer_area(&mut self, base: &mut DrumBase, ctx: &mut Ctx, index: i32, x: i32, y: i32, offset_y: i32, velocity: i32)
    where
        Self: Sized,
    {
        base.handle_sequencer_area(self, ctx, index, x, y, offset_y, velocity);
    }

    /// Former `handleSequencerAreaButtonCombinations`.
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        base.handle_sequencer_area_button_combinations(ctx, note_position, velocity)
    }

    /// Former `handleLoopArea`.
    fn handle_loop_area(&mut self, base: &mut DrumBase, ctx: &mut Ctx, pad: i32, velocity: i32) {
        base.handle_loop_area(ctx, pad, velocity);
    }

    /// Former `playNote`, intentionally empty by default.
    fn play_note(&mut self, _base: &mut DrumBase, _ctx: &mut Ctx, _drum_pad: i32, _velocity: i32) {}
}

/// The default hooks.
pub struct DefaultDrumHooks;

impl DrumHooks for DefaultDrumHooks {}

/// The state of the extra buttons (former `AbstractDrumExView`).
#[derive(Debug, Clone, Default)]
pub struct DrumExState {
    pub extra_buttons_on: bool,
    pub note_repeat_period_on: bool,
    pub note_repeat_length_on: bool,
    pub use_extra_toggle_button: bool,
    pub first_extra_pad: i32,
}

/// The state of a drum sequencer view.
pub struct DrumBase {
    pub seq: SequencerBase,
    pub loop_pad_pressed: i32,
    pub sequencer_lines: i32,
    pub play_rows: i32,
    pub num_columns: i32,
    pub all_rows: i32,
    pub sequencer_steps: i32,
    pub play_columns: i32,
    pub copy_note: Option<StepInfo>,
    pub selected_pad: i32,
    pub scroll_position: i32,
    pub first_pad: ButtonId,
    pub button_select: ButtonId,
    pub button_browse: ButtonId,
    pub button_solo: ButtonId,
    pub button_mute: ButtonId,
    pub button_delete: ButtonId,
    /// The extra buttons of the former `AbstractDrumExView`, if enabled.
    pub ex: Option<DrumExState>,
    /// True for the lane views (former `AbstractDrumLaneView`): the pads never play notes.
    pub lane_mode: bool,
    /// True to always use `COLOR_PAD_HAS_CONTENT` for drum pads with content, even with DAW
    /// colors (an override of the former `getPadContentColor`, e.g. the APC mini).
    pub fixed_pad_content_color: bool,
    follow_selection: bool,
    last_selected_pad: Option<usize>,
    /// True while the view is active; read by the button monitors.
    monitor_active: Rc<Cell<bool>>,
    monitors_registered: bool,
}

impl DrumBase {
    /// Constructor with 8 columns and a clip with 128 rows.
    pub fn new(name: &str, num_sequencer_lines: i32, num_play_lines: i32, use_daw_colors: bool) -> Self {
        Self::with_layout(name, num_sequencer_lines, num_play_lines, GRID_COLUMNS, 128, (num_sequencer_lines * GRID_COLUMNS) as usize, true, use_daw_colors)
    }

    /// Constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn with_layout(
        name: &str,
        num_sequencer_lines: i32,
        num_play_rows: i32,
        num_columns: i32,
        clip_rows: usize,
        clip_cols: usize,
        follow_selection: bool,
        use_daw_colors: bool,
    ) -> Self {
        DrumBase {
            seq: SequencerBase::new(name, clip_rows, clip_cols, clip_rows as i32, use_daw_colors),
            loop_pad_pressed: -1,
            sequencer_lines: num_sequencer_lines,
            play_rows: num_play_rows,
            num_columns,
            all_rows: num_sequencer_lines + num_play_rows,
            sequencer_steps: num_sequencer_lines * num_columns,
            // This layout is currently fixed to a 4 width
            play_columns: 4,
            copy_note: None,
            selected_pad: 0,
            scroll_position: -1,
            first_pad: ButtonId::Pad1,
            button_select: ButtonId::Select,
            button_browse: ButtonId::Browse,
            button_solo: ButtonId::Solo,
            button_mute: ButtonId::Mute,
            button_delete: ButtonId::Delete,
            ex: None,
            lane_mode: false,
            fixed_pad_content_color: false,
            follow_selection,
            last_selected_pad: None,
            monitor_active: Rc::new(Cell::new(false)),
            monitors_registered: false,
        }
    }

    /// Enable the extra buttons of the former `AbstractDrumExView`.
    pub fn with_extra_buttons(mut self) -> Self {
        self.ex = Some(DrumExState { use_extra_toggle_button: true, first_extra_pad: (self.play_rows - 2) * self.play_columns, ..Default::default() });
        self
    }

    pub fn drum_device<'a>(&self, ctx: &'a Ctx) -> Option<&'a SpecificDevice> {
        ctx.model.drum_device()
    }

    fn drum_pad_bank_mut<'a>(&self, ctx: &'a mut Ctx) -> Option<&'a mut DrumPadBank> {
        ctx.model.drum_device_mut().map(|d| d.drum_pad_bank_mut())
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.seq.clip(ctx);
        self.update_note_mapping(ctx);
        // Show the pads of the drum offset (the Java version relies on the position of Bitwig's
        // drum pad bank; Reaper's pages are aligned anyway)
        let drum_offset = ctx.model.scales().drum_offset().max(0) as usize;
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.scroll_to(drum_offset, false);
            bank.set_indication(true);
        }
        self.register_button_monitors(ctx);
        self.monitor_active.set(true);
    }

    pub fn on_deactivate(&mut self, ctx: &mut Ctx) {
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.set_indication(false);
        }
        self.monitor_active.set(false);
    }

    /// The modifier buttons update the note mapping (so that the pads do not play while they are
    /// pressed). The handlers are registered once and are only effective while the view is
    /// active (the Java version registers/unregisters them on (de-)activation).
    fn register_button_monitors(&mut self, ctx: &mut Ctx) {
        if self.monitors_registered {
            return;
        }
        self.monitors_registered = true;
        let mut buttons = vec![self.button_select, self.button_solo, self.button_mute, self.button_delete, self.button_browse];
        buttons.dedup();
        for button in buttons {
            if ctx.surface.button(button).is_none() {
                continue;
            }
            for event in [ButtonEvent::Down, ButtonEvent::Up] {
                let active = self.monitor_active.clone();
                ctx.surface.add_button_event_handler(
                    button,
                    event,
                    Rc::new(move |ctx: &mut Ctx, _event| {
                        if active.get() {
                            if let Some(view) = ctx.active_view() {
                                if let Ok(mut view) = view.try_borrow_mut() {
                                    view.update_note_mapping(ctx);
                                }
                            }
                        }
                    }),
                );
            }
        }
    }

    /// Former `updateControlSurface`. Also follows the selection of the drum pads (former
    /// selection observer of the drum pad bank).
    pub fn update_control_surface(&mut self, ctx: &mut Ctx) {
        if self.follow_selection {
            let selected = self.drum_device(ctx).and_then(|d| d.drum_pad_bank().selected_index());
            if selected != self.last_selected_pad {
                self.last_selected_pad = selected;
                if let Some(index) = selected {
                    self.selected_pad = index as i32;
                }
            }
        }
        self.seq.update_control_surface(ctx);
    }

    /// Handles the observed model events (track selection and played notes).
    pub fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        match event {
            ModelEvent::TrackSelectionChanged { .. } => self.seq.view.key_manager.clear_pressed_keys(),
            ModelEvent::TrackNote { track_index, note, velocity } if selected_track_index(ctx) == Some(*track_index) && (0..128).contains(note) => {
                self.seq.view.key_manager.set_key_pressed(*note, *velocity);
            }
            _ => {}
        }
    }

    pub fn on_grid_note<H: DrumHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let index = note - ctx.surface.grid_ref().start_note();
        let x = index % self.num_columns;
        let y = index / self.num_columns;
        let offset_y = ctx.model.scales().drum_offset();

        // Sequencer steps
        if y >= self.play_rows {
            if self.seq.is_active() {
                hooks.handle_sequencer_area(self, ctx, index, x, y, offset_y, velocity);
            }
            return;
        }

        // halfColumns x playLines Drum Pad Grid
        if x < self.play_columns {
            self.handle_note_area(hooks, ctx, x, y, offset_y, velocity);
            return;
        }

        if !self.seq.is_active() {
            return;
        }

        // Clip length/loop area
        let pad = (self.play_rows - 1 - y) * self.play_columns + x - self.play_columns;
        hooks.handle_loop_area(self, ctx, pad, velocity);
    }

    pub fn handle_note_area<H: DrumHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, x: i32, y: i32, offset_y: i32, velocity: i32) {
        self.set_selected_pad_with_velocity(ctx, self.play_columns * y + x, velocity);

        // Mark selected note
        let key = offset_y + self.selected_pad;
        if (0..128).contains(&key) {
            self.seq.view.key_manager.set_key_pressed(key, velocity);
        }
        hooks.play_note(self, ctx, key, velocity);

        if velocity == 0 {
            return;
        }
        self.handle_note_area_button_combinations(ctx, self.selected_pad);
    }

    /// The default handling of a pad in the sequencer area.
    #[allow(clippy::too_many_arguments)]
    pub fn handle_sequencer_area<H: DrumHooks>(&mut self, hooks: &mut H, ctx: &mut Ctx, index: i32, x: i32, y: i32, offset_y: i32, velocity: i32) {
        // Toggle the note on up, so we can intercept the long presses
        if velocity != 0 {
            return;
        }
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), self.num_columns * (self.all_rows - 1 - y) + x, offset_y + self.selected_pad);
        let vel = self.velocity(ctx, index);
        if hooks.handle_sequencer_area_button_combinations(self, ctx, &note_position, vel) || self.seq.handle_note_editor(ctx, &note_position, vel) {
            return;
        }
        self.seq.clip(ctx).toggle_step(&note_position, vel);
    }

    /// Get the velocity of the pad with the index (or the fixed accent value).
    pub fn velocity(&self, ctx: &Ctx, index: i32) -> i32 {
        SequencerBase::pad_velocity(ctx, ButtonId::get(self.first_pad, index as usize))
    }

    /// The default handling of button combinations in the sequencer area.
    pub fn handle_sequencer_area_button_combinations(&mut self, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        // Handle note duplicate function
        if ctx.is_button_combination(ButtonId::Duplicate) {
            let clip = self.seq.clip(ctx);
            let note_step = clip.step(note_position);
            if note_step.state() == StepState::Start {
                self.copy_note = Some(note_step.clone());
            } else if let Some(copy_note) = &self.copy_note {
                clip.set_step_from(note_position, copy_note);
            }
            return true;
        }

        if ctx.is_button_combination(ButtonId::Mute) {
            let clip = self.seq.clip(ctx);
            let step_info = clip.step(note_position);
            if step_info.state() == StepState::Start {
                let is_muted = step_info.is_muted();
                clip.update_step_mute_state(note_position, !is_muted);
            }
            return true;
        }

        // Change length of a note or create a new one with a length
        let mut np = *note_position;
        let step = np.step;
        let note = np.note;
        for s in 0..step {
            let pad = self.pad_index(s);
            let id = ButtonId::get(self.first_pad, pad as usize);
            if ctx.surface.is_long_pressed(id) {
                ctx.surface.set_trigger_consumed(id);
                let length = step - s + 1;
                let duration = length as f64 * Resolution::value_at(self.seq.resolution_index(ctx));
                np.step = s;
                let clip = self.seq.clip(ctx);
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

    /// Get the index of the pad which displays the given step.
    pub fn pad_index(&self, step: i32) -> i32 {
        let x = step % self.num_columns;
        let y = self.all_rows - 1 - step / self.num_columns;
        y * self.num_columns + x
    }

    /// The default handling of the pads of the clip length/loop area (including the extra
    /// buttons of the former `AbstractDrumExView`, if enabled).
    pub fn handle_loop_area(&mut self, ctx: &mut Ctx, pad: i32, velocity: i32) {
        if let Some(ex) = &mut self.ex {
            if ex.extra_buttons_on && pad >= ex.first_extra_pad {
                if velocity == 0 {
                    return;
                }
                if pad == ex.first_extra_pad + 4 {
                    ctx.config.toggle_note_repeat_active();
                } else if pad == ex.first_extra_pad + 5 {
                    ex.note_repeat_period_on = !ex.note_repeat_period_on;
                    if ex.note_repeat_period_on {
                        ex.note_repeat_length_on = false;
                    }
                } else if pad == ex.first_extra_pad + 6 {
                    ex.note_repeat_length_on = !ex.note_repeat_length_on;
                    if ex.note_repeat_length_on {
                        ex.note_repeat_period_on = false;
                    }
                }
                return;
            }
        }

        if ctx.config.is_accent_active() {
            if velocity == 0 {
                return;
            }
            let sel_pad = (3 - pad / 4) * 4 + pad % 4;
            ctx.config.set_fixed_accent_value((sel_pad + 1) * 8 - 1);
            return;
        }

        // Button pressed?
        if velocity > 0 {
            // Not yet a button pressed, store it
            if self.loop_pad_pressed == -1 {
                self.loop_pad_pressed = pad;
            }
            return;
        }
        if self.loop_pad_pressed == -1 {
            return;
        }

        let length_of_one_pad = self.seq.length_of_one_page(ctx, self.sequencer_steps) as f64;
        let clip = self.seq.clip(ctx);
        if pad == self.loop_pad_pressed && pad != clip.edit_page() {
            // Only single pad pressed -> page selection
            clip.scroll_to_page(pad);
        } else {
            let start = self.loop_pad_pressed.min(pad);
            let end = self.loop_pad_pressed.max(pad) + 1;
            // Set a new loop between the 2 selected pads
            let new_start = start as f64 * length_of_one_pad;
            clip.set_loop_start(new_start);
            clip.set_loop_length((end - start) as f64 * length_of_one_pad);
            clip.set_play_range(new_start, end as f64 * length_of_one_pad);
        }
        self.loop_pad_pressed = -1;
    }

    /// Toggle the extra buttons (former `AbstractDrumExView.toggleExtraButtons`).
    pub fn toggle_extra_buttons(&mut self) {
        if let Some(ex) = &mut self.ex {
            ex.extra_buttons_on = !ex.extra_buttons_on;
        }
    }

    pub fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !ctx.model.can_selected_track_hold_notes() {
            ctx.surface.grid().turn_off();
            return;
        }
        self.seq.clip(ctx);
        let lights = self.grid_lights(ctx);
        apply_lights(ctx, lights);
    }

    /// Compute the lights of all pads (x, y, light).
    pub fn grid_lights(&self, ctx: &Ctx) -> Vec<(i32, i32, PadLight)> {
        let mut lights = Vec::new();
        let Some(primary) = self.drum_device(ctx) else {
            return lights;
        };
        // halfColumns x playLines Drum Pad Grid
        self.draw_drum_pads(ctx, primary.drum_pad_bank(), &mut lights);
        if self.sequencer_lines > 0 {
            let is_active = self.seq.is_active();
            self.draw_pages(ctx, is_active, &mut lights);
            let note_row = ctx.model.scales().drum_offset() + self.selected_pad;
            let row_color = self.pad_color(primary, self.selected_pad);
            self.draw_sequencer_steps(ctx, is_active, note_row, row_color, &mut lights);
        }
        lights
    }

    fn draw_drum_pads(&self, ctx: &Ctx, drum_pad_bank: &DrumPadBank, lights: &mut Vec<(i32, i32, PadLight)>) {
        let is_recording = ctx.model.has_recording_state();
        for y in 0..self.play_rows {
            for x in 0..self.play_columns {
                let index = self.play_columns * y + x;
                lights.push((x, self.all_rows - 1 - y, PadLight::Id(self.drum_pad_color(ctx, index, drum_pad_bank, is_recording))));
            }
        }
    }

    /// Get the color of the drum pad (if it exists).
    pub fn pad_color(&self, primary: &SpecificDevice, drum_pad_index: i32) -> Option<ColorEx> {
        if !primary.has_drum_pads() || drum_pad_index < 0 {
            return None;
        }
        let item = primary.drum_pad_bank().item(drum_pad_index as usize);
        if item.does_exist() {
            Some(item.color())
        } else {
            None
        }
    }

    /// Get the color ID of a drum pad.
    pub fn drum_pad_color(&self, ctx: &Ctx, index: i32, drum_pad_bank: &DrumPadBank, is_recording: bool) -> &'static str {
        let offset_y = ctx.model.scales().drum_offset();

        // Playing note?
        let key = offset_y + index;
        if (0..128).contains(&key) && self.seq.view.key_manager.is_key_pressed(key) {
            return if is_recording { COLOR_PAD_RECORD } else { COLOR_PAD_PLAY };
        }

        // Selected?
        if self.selected_pad == index {
            return COLOR_PAD_SELECTED;
        }

        // Exists and active?
        let drum_pad = drum_pad_bank.item(index as usize);
        if !drum_pad.does_exist() || !drum_pad.is_activated() {
            return if ctx.config.is_turn_off_empty_drum_pads() { COLOR_PAD_OFF } else { COLOR_PAD_NO_CONTENT };
        }

        // Muted or soloed?
        if drum_pad.is_mute() || drum_pad_bank.has_soloed_pads() && !drum_pad.is_solo() {
            return COLOR_PAD_MUTED;
        }

        if self.seq.use_daw_colors && !self.fixed_pad_content_color {
            DawColor::color_id(drum_pad.color())
        } else {
            COLOR_PAD_HAS_CONTENT
        }
    }

    pub fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        if self.lane_mode {
            // The lane views never play notes
            let table = ctx.model.scales().translate_matrix_to_grid(&Scales::empty_matrix());
            ctx.surface.set_key_translation_table(table.to_vec());
            return;
        }
        let matrix: NoteMatrix = if self.can_pads_be_turned_on(ctx) { ctx.model.scales().drum_matrix() } else { Scales::empty_matrix() };
        self.seq.view.update_note_mapping(ctx, matrix);
    }

    pub fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        let offset = if ctx.surface.is_shift_pressed() { 4 } else { ctx.model.scales().drum_default_offset() };
        self.change_octave(ctx, event, false, offset, false, true);
    }

    pub fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        let offset = if ctx.surface.is_shift_pressed() { 4 } else { ctx.model.scales().drum_default_offset() };
        self.change_octave(ctx, event, true, offset, false, true);
    }

    pub fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        ctx.model.scales().can_scroll_drum_octave_up()
    }

    pub fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        ctx.model.scales().can_scroll_drum_octave_down()
    }

    /// Change the drum offset by the given amount.
    pub fn change_octave(&mut self, ctx: &mut Ctx, event: ButtonEvent, is_up: bool, offset: i32, adjust_page: bool, notify: bool) {
        if event != ButtonEvent::Down {
            return;
        }
        self.seq.view.key_manager.clear_pressed_keys();
        let scales = ctx.model.scales_mut();
        if is_up {
            scales.inc_drum_offset(offset);
        } else {
            scales.dec_drum_offset(offset);
        }
        self.update_note_mapping(ctx);
        if notify {
            let text = ctx.model.scales().drum_range_text();
            ctx.notify(&text);
        }
        let drum_offset = ctx.model.scales().drum_offset().max(0) as usize;
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.scroll_to(drum_offset, adjust_page);
        }
        self.seq.clear_edit_notes(ctx);
    }

    pub fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.seq.view.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().reset_drum_octave();
        self.update_note_mapping(ctx);
        let drum_offset = ctx.model.scales().drum_offset().max(0) as usize;
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.scroll_to(drum_offset, true);
        }
    }

    pub fn handle_note_area_button_combinations(&mut self, ctx: &mut Ctx, played_pad: i32) {
        if self.is_delete_trigger(ctx) {
            // Delete all of the notes on that 'pad'
            self.handle_delete_button(ctx, played_pad);
            return;
        }
        if self.is_mute_trigger(ctx) {
            // Mute that 'pad'
            self.handle_mute_button(ctx, played_pad);
            return;
        }
        if self.is_solo_trigger(ctx) {
            // Solo that 'pad'
            self.handle_solo_button(ctx, played_pad);
            return;
        }
        if self.is_browse_trigger(ctx) {
            ctx.surface.set_trigger_consumed(self.button_browse);
            let Some(primary) = self.drum_device(ctx) else {
                return;
            };
            if !primary.has_drum_pads() {
                return;
            }
            let bank = primary.drum_pad_bank();
            self.scroll_position = bank.scroll_position();
            let pad = bank.item(played_pad as usize);
            let (owner, name, position) = (bank.owner(), pad.name().to_string(), pad.position());
            // Only with a host browser, Reaper has none for drum pads
            if let Some(browser) = ctx.model.browser_mut() {
                browser.replace_drum_pad(owner, &name, position);
            }
            return;
        }
        if self.is_select_trigger(ctx) || ctx.config.is_auto_select_drum() {
            // Also select the matching device layer channel of the pad
            self.handle_select_button(ctx, played_pad);
        }
    }

    pub fn is_select_trigger(&self, ctx: &Ctx) -> bool {
        let pressed = ctx.surface.is_pressed(self.button_select);
        if ctx.config.is_combination_button_to_sound_drum_pads() {
            return !pressed;
        }
        pressed
    }

    pub fn is_browse_trigger(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_pressed(self.button_browse)
    }

    pub fn is_solo_trigger(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_pressed(self.button_solo)
    }

    pub fn is_mute_trigger(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_pressed(self.button_mute)
    }

    pub fn is_delete_trigger(&self, ctx: &mut Ctx) -> bool {
        ctx.is_button_combination(self.button_delete)
    }

    fn handle_delete_button(&mut self, ctx: &mut Ctx, played_pad: i32) {
        ctx.surface.set_trigger_consumed(self.button_delete);
        let channel = ctx.config.midi_edit_channel();
        let row = ctx.model.scales().drum_offset() + played_pad;
        self.seq.clip(ctx).clear_row(channel, row);
    }

    fn handle_mute_button(&mut self, ctx: &mut Ctx, played_pad: i32) {
        ctx.surface.set_trigger_consumed(self.button_mute);
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.item_mut(played_pad as usize).toggle_mute();
        }
    }

    fn handle_solo_button(&mut self, ctx: &mut Ctx, played_pad: i32) {
        ctx.surface.set_trigger_consumed(self.button_solo);
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.item_mut(played_pad as usize).toggle_solo();
        }
    }

    fn handle_select_button(&mut self, ctx: &mut Ctx, played_pad: i32) {
        ctx.surface.set_trigger_consumed(self.button_select);
        if let Some(bank) = self.drum_pad_bank_mut(ctx) {
            bank.select_pad(played_pad as usize);
        }
    }

    fn can_pads_be_turned_on(&self, ctx: &mut Ctx) -> bool {
        if !ctx.model.can_selected_track_hold_notes() {
            return false;
        }
        !(self.is_select_trigger(ctx) || self.is_delete_trigger(ctx) || self.is_mute_trigger(ctx) || self.is_solo_trigger(ctx) || self.is_browse_trigger(ctx))
    }

    /// Draw the sequencer steps of the given note row.
    pub fn draw_sequencer_steps(&self, ctx: &Ctx, is_active: bool, note_row: i32, row_color: Option<ColorEx>, lights: &mut Vec<(i32, i32, PadLight)>) {
        let Some(clip) = self.seq.clip_ref(ctx) else {
            return;
        };
        let step = clip.current_step();
        let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % self.sequencer_steps } else { -1 };
        let edit_notes = self.seq.edit_notes(ctx);
        let mut note_position = NotePosition::new(ctx.config.midi_edit_channel(), 0, note_row);
        for col in 0..self.sequencer_steps {
            note_position.step = col;
            let step_info = clip.step(&note_position);
            let x = col % self.num_columns;
            let y = col / self.num_columns;
            let color = if is_active {
                self.seq.step_color(Some(step_info), col == hi_step, row_color, note_position.channel, col, note_row, &edit_notes)
            } else {
                COLOR_NO_CONTENT
            };
            lights.push((x, y, PadLight::Id(color)));
        }
    }

    /// Draw the clip pages (or the accent velocities) and the extra buttons.
    pub fn draw_pages(&self, ctx: &Ctx, is_active: bool, lights: &mut Vec<(i32, i32, PadLight)>) {
        self.draw_pages_base(ctx, is_active, lights);

        // Draw the extra buttons (former AbstractDrumExView.drawPages)
        let Some(ex) = &self.ex else {
            return;
        };
        if ex.extra_buttons_on {
            let mut row = self.all_rows - 2;
            let pick = |on: bool, on_id: &'static str, off_id: &'static str| PadLight::Id(if on { on_id } else { off_id });
            lights.push((4, row, pick(self.is_select_trigger(ctx), COLOR_EX_SELECT_ON, COLOR_EX_SELECT_OFF)));
            lights.push((5, row, pick(self.is_mute_trigger(ctx), COLOR_EX_MUTE_ON, COLOR_EX_MUTE_OFF)));
            lights.push((6, row, pick(self.is_solo_trigger(ctx), COLOR_EX_SOLO_ON, COLOR_EX_SOLO_OFF)));
            lights.push((7, row, pick(self.is_browse_trigger(ctx), COLOR_EX_BROWSE_ON, COLOR_EX_BROWSE_OFF)));
            let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
            row += 1;
            lights.push((4, row, pick(note_repeat.is_active, COLOR_EX_NOTE_REPEAT_ON, COLOR_EX_NOTE_REPEAT_OFF)));
            lights.push((5, row, pick(ex.note_repeat_period_on, COLOR_EX_NOTE_REPEAT_PERIOD_ON, COLOR_EX_NOTE_REPEAT_PERIOD_OFF)));
            lights.push((6, row, pick(ex.note_repeat_length_on, COLOR_EX_NOTE_REPEAT_LENGTH_ON, COLOR_EX_NOTE_REPEAT_LENGTH_OFF)));
        }
        if ex.use_extra_toggle_button || ex.extra_buttons_on {
            lights.push((7, self.all_rows - 1, PadLight::Id(if ex.extra_buttons_on { COLOR_EX_TOGGLE_ON } else { COLOR_EX_TOGGLE_OFF })));
        }
    }

    fn draw_pages_base(&self, ctx: &Ctx, is_active: bool, lights: &mut Vec<(i32, i32, PadLight)>) {
        if ctx.config.is_accent_active() {
            let mut selected_velocity_pad = 15 - ctx.config.fixed_accent_value() / 8;
            let sel_y = selected_velocity_pad / 4;
            let sel_x = selected_velocity_pad % 4;
            selected_velocity_pad = sel_y * 4 + 3 - sel_x;
            for pad in 0..16 {
                let x = self.play_columns + pad % self.play_columns;
                let y = self.sequencer_lines + pad / self.play_columns;
                lights.push((x, y, PadLight::Index(if pad == selected_velocity_pad { ACCENT_COLOR_SELECTED } else { ACCENT_COLOR })));
            }
            return;
        }

        let Some(clip) = self.seq.clip_ref(ctx) else {
            return;
        };
        let step = clip.current_step();
        let length_of_one_pad = self.seq.length_of_one_page(ctx, self.sequencer_steps) as f64;
        let loop_start = clip.loop_start();
        let loop_start_pad = (loop_start / length_of_one_pad).ceil() as i32;
        let loop_end_pad = ((loop_start + clip.loop_length()) / length_of_one_pad).ceil() as i32;
        let current_page = step / self.sequencer_steps;
        let num_of_pages = self.number_of_available_pages();
        for pad in 0..num_of_pages {
            let x = self.play_columns + pad % self.play_columns;
            let y = self.sequencer_lines + pad / self.play_columns;
            let color = if is_active { SequencerBase::page_color(loop_start_pad, loop_end_pad, current_page, clip.edit_page(), pad) } else { COLOR_NO_CONTENT };
            lights.push((x, y, PadLight::Id(color)));
        }
    }

    /// The number of pages which can be displayed in the loop area.
    pub fn number_of_available_pages(&self) -> i32 {
        // Remove the last 8 buttons so we can use it for something else if extra buttons are
        // active
        let extra = if self.ex.as_ref().is_some_and(|ex| ex.extra_buttons_on) { 8 } else { 0 };
        self.play_columns * self.play_rows - extra
    }

    /// Select a drum pad (and the pad in the drum device).
    pub fn set_selected_pad(&mut self, ctx: &mut Ctx, selected_pad: i32) {
        self.set_selected_pad_with_velocity(ctx, selected_pad, 127);
    }

    fn set_selected_pad_with_velocity(&mut self, ctx: &mut Ctx, selected_pad: i32, velocity: i32) {
        if self.selected_pad == selected_pad {
            return;
        }
        self.selected_pad = selected_pad;
        self.seq.clear_edit_notes(ctx);
        if velocity > 0 {
            if let Some(bank) = self.drum_pad_bank_mut(ctx) {
                bank.select_pad(selected_pad as usize);
            }
        }
    }

    pub fn selected_pad(&self) -> i32 {
        self.selected_pad
    }

    /// Scroll the drum pad bank back to the page which was active when browsing started.
    pub fn reposition_bank_page(&mut self, ctx: &mut Ctx) {
        if self.scroll_position >= 0 {
            let position = self.scroll_position as usize;
            if let Some(bank) = self.drum_pad_bank_mut(ctx) {
                bank.scroll_to(position, true);
            }
        }
    }
}

/// Light the pads of the grid.
pub fn apply_lights(ctx: &mut Ctx, lights: Vec<(i32, i32, PadLight)>) {
    let Ctx { surface, colors, .. } = ctx;
    let grid = surface.grid();
    for (x, y, light) in lights {
        match light {
            PadLight::Id(id) => grid.light_ex_id(colors, x, y, id),
            PadLight::Index(index) => grid.light_ex(x, y, index),
        }
    }
}
