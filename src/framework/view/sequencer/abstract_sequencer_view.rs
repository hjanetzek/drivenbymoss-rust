// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The state and functions shared by all sequencer views (former `AbstractSequencerView`).

use crate::daw::bank::BankItem;
use crate::daw::clip::{CursorClip, NotePosition, StepInfo, StepState};
use crate::daw::color::{ColorEx, DawColor};
use crate::daw::constants::Resolution;
use crate::framework::color_ids::abstract_sequencer_view::*;
use crate::framework::ctx::Ctx;
use crate::framework::feature::BUTTON_COLOR_OFF;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::mode::note_editor::{ClipKey, NoteEditor};
use crate::framework::view::base::{scene_index, ViewBase};
use crate::midi::ButtonEvent;
use crate::scale::{NoteMatrix, Scales};

/// The state of a sequencer view.
pub struct SequencerBase {
    pub view: ViewBase,
    pub clip_rows: usize,
    pub clip_cols: usize,
    pub use_daw_colors: bool,
    pub num_sequencer_rows: i32,
    pub is_note_edited: bool,
    is_sequencer_active: bool,
    /// If true, the state of a step is the combined state of all notes of the step (former
    /// override of `getStepState` and `updateEditMode` in `AbstractPolySequencerView`).
    pub poly_steps: bool,
}

impl SequencerBase {
    /// Constructor.
    ///
    /// * `clip_rows` - The rows of the monitored clip
    /// * `clip_cols` - The columns of the monitored clip
    /// * `num_sequencer_rows` - The number of displayed rows of the sequencer
    /// * `use_daw_colors` - True to use the DAW color of items for coloring (full RGB devices)
    pub fn new(name: &str, clip_rows: usize, clip_cols: usize, num_sequencer_rows: i32, use_daw_colors: bool) -> Self {
        crate::daw::clip::request_note_clip(clip_cols, clip_rows);
        SequencerBase {
            view: ViewBase::new(name),
            clip_rows,
            clip_cols,
            use_daw_colors,
            num_sequencer_rows,
            is_note_edited: false,
            is_sequencer_active: false,
            poly_steps: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.view.name
    }

    /// The key of the monitored clip.
    pub fn clip_key(&self) -> ClipKey {
        (self.clip_cols, self.clip_rows)
    }

    /// Get the clip (it is created if it does not exist yet).
    pub fn clip<'a>(&self, ctx: &'a mut Ctx) -> &'a mut CursorClip {
        ctx.model.note_clip(self.clip_cols, self.clip_rows)
    }

    /// Get the clip if it was already created (see [`SequencerBase::clip`]).
    pub fn clip_ref<'a>(&self, ctx: &'a Ctx) -> Option<&'a CursorClip> {
        ctx.model.note_clip_ref(self.clip_cols, self.clip_rows)
    }

    /// Former `updateControlSurface`: updates the active state of the sequencer and the
    /// display of the active mode.
    pub fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let exists = self.clip(ctx).does_exist();
        self.set_sequencer_active(ctx.model.can_selected_track_hold_notes() && exists);
        ctx.update_active_mode_display();
    }

    /// Former `onButton`: the scene buttons select the resolution.
    pub fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down || !self.is_active() {
            return;
        }
        self.set_resolution_index(ctx, 7 - index as i32);
    }

    /// Scroll clip left.
    pub fn on_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        self.clip(ctx).scroll_steps_page_backwards();
        self.notify_edit_page(ctx);
        self.clear_edit_notes(ctx);
    }

    /// Scroll clip right.
    pub fn on_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        self.clip(ctx).scroll_steps_page_forward();
        self.notify_edit_page(ctx);
        self.clear_edit_notes(ctx);
    }

    /// Former `MVHelper.notifyEditPage`.
    fn notify_edit_page(&self, ctx: &mut Ctx) {
        let (cols, rows) = self.clip_key();
        ctx.schedule(100, move |ctx| {
            let clip = ctx.model.note_clip(cols, rows);
            if clip.does_exist() {
                let message = format!("Edit page: {}", clip.edit_page() + 1);
                ctx.notify(&message);
            }
        });
    }

    /// Set the resolution index (0-7).
    pub fn set_resolution_index(&mut self, ctx: &mut Ctx, selected_resolution_index: i32) {
        let resolution = Resolution::ALL[selected_resolution_index.clamp(0, 7) as usize];
        self.clip(ctx).set_step_length(resolution.value());
        ctx.notify(&format!("Grid res.: {}", resolution.name()));
    }

    /// Get the resolution index (0-7).
    pub fn resolution_index(&self, ctx: &Ctx) -> usize {
        let step_length = self.clip_ref(ctx).map(|c| c.step_length()).unwrap_or(Resolution::Res1_16.value());
        Resolution::get_match(step_length)
    }

    // The sequencer functions of the View trait, so that the views can forward them with
    // crate::forward_view!.

    /// [`Self::on_left`] as [`View::on_sequencer_left`](crate::framework::feature::View::on_sequencer_left).
    pub fn on_sequencer_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.on_left(ctx, event);
    }

    /// [`Self::on_right`] as [`View::on_sequencer_right`](crate::framework::feature::View::on_sequencer_right).
    pub fn on_sequencer_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.on_right(ctx, event);
    }

    /// [`Self::resolution_index`] as
    /// [`View::sequencer_resolution_index`](crate::framework::feature::View::sequencer_resolution_index).
    pub fn sequencer_resolution_index(&self, ctx: &Ctx) -> Option<usize> {
        Some(self.resolution_index(ctx))
    }

    /// [`Self::set_resolution_index`] as
    /// [`View::set_sequencer_resolution_index`](crate::framework::feature::View::set_sequencer_resolution_index).
    pub fn set_sequencer_resolution_index(&mut self, ctx: &mut Ctx, index: i32) -> bool {
        self.set_resolution_index(ctx, index);
        true
    }

    /// Former `getButtonColorID`.
    pub fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if !self.is_active() {
            return COLOR_RESOLUTION_OFF;
        }
        match scene_index(button) {
            None => BUTTON_COLOR_OFF,
            Some(index) if index == 7 - self.resolution_index(ctx) => COLOR_RESOLUTION_SELECTED,
            Some(_) => COLOR_RESOLUTION,
        }
    }

    /// Signal that a note has been edited.
    pub fn set_note_edited(&mut self) {
        self.is_note_edited = true;
    }

    /// Checks if the given step is in the current display (edit page).
    pub fn is_in_x_range(clip: &CursorClip, x: i32) -> bool {
        let step_size = clip.num_steps() as i32;
        let start = clip.edit_page() * step_size;
        x >= start && x < start + step_size
    }

    /// Calculates the length of one sequencer page which is the number of displayed steps
    /// multiplied with the current grid resolution.
    pub fn length_of_one_page(&self, ctx: &Ctx, num_of_steps: i32) -> i32 {
        (num_of_steps as f64 * Resolution::value_at(self.resolution_index(ctx))).floor() as i32
    }

    /// Get the color for a sequencer page.
    pub fn page_color(loop_start_page: i32, loop_end_page: i32, play_page: i32, selected_page: i32, page: i32) -> &'static str {
        if page == play_page {
            return COLOR_ACTIVE_PAGE;
        }
        if page == selected_page {
            return COLOR_SELECTED_PAGE;
        }
        if page < loop_start_page || page >= loop_end_page {
            return COLOR_NO_CONTENT;
        }
        COLOR_PAGE
    }

    /// Handle repeat operator quick change. If repeat count is off it is set to 4 otherwise it
    /// is increased or decreased depending on the parameter.
    pub fn handle_sequencer_area_repeat_operator(&self, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32, increase: bool) {
        let clip = self.clip(ctx);
        if clip.step(note_position).state() == StepState::Off {
            clip.toggle_step(note_position, velocity);
        }
        let step_info = clip.step(note_position).clone();
        if !step_info.is_repeat_enabled() {
            clip.update_step_is_repeat_enabled(note_position, true);
        }
        let repeat_count = step_info.repeat_count();
        let repeat_count = if increase { (repeat_count + 1).min(127) } else { (repeat_count - 1).max(-127) };
        clip.update_step_repeat_count(note_position, repeat_count);
        let repeat_count_str = match repeat_count {
            c if c > 0 => (c + 1).to_string(),
            0 => "Off".to_string(),
            c => format!("1/{}", 1 - c),
        };
        ctx.notify(&format!("Note repeat: {repeat_count_str}"));
    }

    /// Check if there is a note clip to edit: true if the selected track can hold notes and the
    /// selected clip exists.
    pub fn is_active(&self) -> bool {
        self.is_sequencer_active
    }

    pub fn set_sequencer_active(&mut self, is_sequencer_active: bool) {
        self.is_sequencer_active = is_sequencer_active;
    }

    /// If there is a note editor mode available this method handles adding and removing notes
    /// from it. Returns true if handled.
    pub fn handle_note_editor(&mut self, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        if !ctx.modes.is_active(Modes::Note) {
            return false;
        }
        if velocity > 0 {
            self.update_edit_mode(ctx, note_position);
            return true;
        }
        if self.is_note_edited {
            self.is_note_edited = false;
        }
        true
    }

    /// Store existing note for editing, remove it from the list or close the edit mode.
    pub fn update_edit_mode(&mut self, ctx: &mut Ctx, note_position: &NotePosition) {
        if self.step_state(ctx, note_position) == StepState::Off {
            self.exit_edit_mode(ctx);
        } else if self.poly_steps {
            self.edit_notes_of_step(ctx, note_position.channel, note_position.step);
        } else {
            self.edit_note(ctx, note_position, true);
        }
    }

    /// Edit all notes which start at the given step (former
    /// `AbstractPolySequencerView.editNotesOfStep`).
    pub fn edit_notes_of_step(&mut self, ctx: &mut Ctx, channel: i32, step: i32) {
        let mut note_position = NotePosition::new(channel, step, 0);
        for row in 0..128 {
            note_position.note = row;
            if self.clip(ctx).step(&note_position).state() == StepState::Start {
                self.edit_note(ctx, &note_position, true);
            }
        }
    }

    /// Get the state of a note at a position.
    pub fn step_state(&self, ctx: &mut Ctx, note_position: &NotePosition) -> StepState {
        let clip = self.clip(ctx);
        if !self.poly_steps {
            return clip.step(note_position).state();
        }
        let mut row_note_position = *note_position;
        for row in 0..128 {
            row_note_position.note = row;
            let state = clip.step(&row_note_position).state();
            if state != StepState::Off {
                return state;
            }
        }
        StepState::Off
    }

    /// Set or add a note to the active/related note edit mode. Removes the note if already
    /// edited. Activates the editor mode.
    ///
    /// * `add_note` - Add the note to the edited notes otherwise clear the already selected and
    ///   add only the new one
    pub fn edit_note(&mut self, ctx: &mut Ctx, note_position: &NotePosition, add_note: bool) {
        if !activate_note_editor(ctx) {
            return;
        }
        let state = self.step_state(ctx, note_position);
        if state == StepState::Off {
            self.exit_edit_mode(ctx);
            return;
        }
        if state == StepState::Continue {
            return;
        }

        let clip = self.clip_key();
        let mut is_empty = false;
        let handled = with_note_editor(ctx, |editor| {
            if editor.is_note_edited(clip, note_position) {
                editor.remove_note(clip, note_position);
                is_empty = editor.notes().is_empty();
                return true;
            }
            if add_note {
                editor.add_note(clip, note_position);
            } else {
                editor.set_note(clip, note_position);
            }
            false
        });
        if handled == Some(true) && is_empty {
            self.exit_edit_mode(ctx);
        }
    }

    /// Exits the note edit mode.
    pub fn exit_edit_mode(&mut self, ctx: &mut Ctx) {
        self.is_note_edited = false;
        ctx.notify("Edit Notes: Off");

        // Clear all edit notes
        self.clear_edit_notes(ctx);

        // Close edit mode
        let is_note_editor_mode = ctx.active_mode().is_some_and(|m| m.try_borrow().is_ok_and(|m| m.note_editor().is_some()));
        if is_note_editor_mode {
            ctx.restore_mode();
        }

        // Prevent note deletion on button-up!
        ctx.surface.consume_pads();
    }

    /// Clear all edit notes.
    pub fn clear_edit_notes(&self, ctx: &mut Ctx) {
        with_note_editor(ctx, |editor| editor.clear_notes());
    }

    /// Get the positions of the notes which are currently selected for editing.
    pub fn edit_notes(&self, ctx: &Ctx) -> Vec<NotePosition> {
        if let Some(mode) = ctx.modes.get(Modes::Note) {
            if let Ok(mode) = mode.try_borrow() {
                if let Some(editor) = mode.note_editor() {
                    return editor.notes().to_vec();
                }
            }
        }
        if let Some(view) = ctx.views.get(Views::NoteEditView) {
            if let Ok(view) = view.try_borrow() {
                if let Some(editor) = view.note_editor() {
                    return editor.notes().to_vec();
                }
            }
        }
        Vec::new()
    }

    /// Test if at the given channel / step / note is a note which is selected for editing.
    pub fn is_edit(channel: i32, step: i32, note: i32, edit_notes: &[NotePosition]) -> bool {
        edit_notes.iter().any(|n| n.channel == channel && n.step == step && n.note == note)
    }

    /// Get the color for a step. If the step is off, the pad color is used.
    #[allow(clippy::too_many_arguments)]
    pub fn step_color_with_pad(
        &self,
        ctx: &Ctx,
        step_info: Option<&StepInfo>,
        highlight: bool,
        channel: i32,
        step: i32,
        pad: i32,
        note: i32,
        edit_notes: &[NotePosition],
    ) -> &'static str {
        if step_info.is_none_or(|s| s.state() == StepState::Off) {
            if highlight {
                return COLOR_STEP_HILITE_NO_CONTENT;
            }
            let track = if self.use_daw_colors { Some(ctx.model.cursor_track()) } else { None };
            let pad_color = self.view.pad_color(ctx, pad, track);
            if ctx.config.is_turn_off_scale_pads() && pad_color == Scales::SCALE_COLOR_NOTE {
                return Scales::SCALE_COLOR_OFF;
            }
            return pad_color;
        }
        self.step_color(step_info, highlight, None, channel, step, note, edit_notes)
    }

    /// Get the color for a step.
    ///
    /// * `row_color` - The color to use for content notes (with DAW colors)
    #[allow(clippy::too_many_arguments)]
    pub fn step_color(
        &self,
        step_info: Option<&StepInfo>,
        highlight: bool,
        row_color: Option<ColorEx>,
        channel: i32,
        step: i32,
        note: i32,
        edit_notes: &[NotePosition],
    ) -> &'static str {
        let state = step_info.map(|s| s.state()).unwrap_or(StepState::Off);
        let is_muted = step_info.is_some_and(|s| s.is_muted());
        match state {
            StepState::Start => {
                if highlight {
                    return COLOR_STEP_HILITE_CONTENT;
                }
                if Self::is_edit(channel, step, note, edit_notes) {
                    return COLOR_STEP_SELECTED;
                }
                if is_muted {
                    return COLOR_STEP_MUTED;
                }
                match row_color {
                    Some(color) if self.use_daw_colors => DawColor::color_id(color),
                    _ => COLOR_CONTENT,
                }
            }
            StepState::Continue => {
                if highlight {
                    return COLOR_STEP_HILITE_CONTENT;
                }
                if Self::is_edit(channel, step, note, edit_notes) {
                    return COLOR_STEP_SELECTED;
                }
                if is_muted {
                    return COLOR_STEP_MUTED_CONT;
                }
                match row_color {
                    Some(color) if self.use_daw_colors => DawColor::color_id(ColorEx::darker(color)),
                    _ => COLOR_CONTENT_CONT,
                }
            }
            StepState::Off => {
                if highlight {
                    return COLOR_STEP_HILITE_NO_CONTENT;
                }
                if step / 4 % 2 == 1 {
                    COLOR_NO_CONTENT_4
                } else {
                    COLOR_NO_CONTENT
                }
            }
        }
    }

    /// Get the velocity to use for a pad: the fixed accent value if active, otherwise the
    /// velocity with which the pad was pressed.
    pub fn pad_velocity(ctx: &Ctx, pad_button: ButtonId) -> i32 {
        if ctx.config.is_accent_active() {
            return ctx.config.fixed_accent_value();
        }
        ctx.surface.button(pad_button).map(|b| b.pressed_velocity()).unwrap_or(0)
    }

    /// Get the velocity with which the pad (index of the grid) was pressed.
    pub fn pressed_velocity(ctx: &Ctx, pad_index: i32) -> i32 {
        ctx.surface.button(ButtonId::get(ButtonId::Pad1, pad_index as usize)).map(|b| b.pressed_velocity()).unwrap_or(0)
    }

    /// Set a sequencer matrix (which has less than 128 entries) as the note matrix of the key
    /// manager. Unlike full matrices it does not change the key translation table of the
    /// surface (see Java `delayedUpdateNoteMapping`).
    pub fn set_sequencer_matrix(&mut self, matrix: &[i32]) {
        let mut note_matrix: NoteMatrix = Scales::empty_matrix();
        for (i, note) in matrix.iter().take(128).enumerate() {
            note_matrix[i] = *note;
        }
        self.view.key_manager.set_note_matrix(note_matrix);
    }
}

/// Get the related note editor (a note editor mode or the note edit view) and make sure it is
/// active. Returns false if there is none (former `getNoteEditor`).
///
/// The Java version activates the note edit view immediately as a temporary view. Since the
/// calling view is still in use (borrowed) at that moment, the view switch is scheduled.
pub fn activate_note_editor(ctx: &mut Ctx) -> bool {
    if let Some(mode) = ctx.modes.get(Modes::Note) {
        let has_editor = mode.try_borrow().is_ok_and(|m| m.note_editor().is_some());
        if has_editor {
            if !ctx.modes.is_active(Modes::Note) {
                ctx.set_active_mode(Modes::Note);
            }
            return true;
        }
    }
    if let Some(view) = ctx.views.get(Views::NoteEditView) {
        let has_editor = view.try_borrow().is_ok_and(|v| v.note_editor().is_some());
        if has_editor {
            if !ctx.views.is_active(Views::NoteEditView) {
                ctx.schedule(0, |ctx| {
                    if !ctx.views.is_active(Views::NoteEditView) {
                        ctx.set_temporary_view(Views::NoteEditView);
                    }
                });
            }
            return true;
        }
    }
    false
}

/// Execute the function with the related note editor (of the note mode or the note edit view)
/// without activating it. Returns `None` if there is no note editor.
pub fn with_note_editor<R>(ctx: &mut Ctx, f: impl FnOnce(&mut NoteEditor) -> R) -> Option<R> {
    if let Some(mode) = ctx.modes.get(Modes::Note) {
        if let Ok(mut mode) = mode.try_borrow_mut() {
            if let Some(editor) = mode.note_editor_mut() {
                return Some(f(editor));
            }
        }
    }
    if let Some(view) = ctx.views.get(Views::NoteEditView) {
        if let Ok(mut view) = view.try_borrow_mut() {
            if let Some(editor) = view.note_editor_mut() {
                return Some(f(editor));
            }
        }
    }
    None
}

/// Get the selected track index of the current track bank (for the note observers).
pub fn selected_track_index(ctx: &Ctx) -> Option<i32> {
    ctx.model.current_track_bank().selected_item().map(|t| t.index())
}
