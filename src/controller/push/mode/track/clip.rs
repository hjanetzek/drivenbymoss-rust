// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The clip mode (former `ClipMode`).

use crate::daw::clip::{CursorClip, NotePosition};
use crate::daw::constants::Capability;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::Mode;
use crate::framework::graphics::{GraphicDisplay, MidiClipInfo};
use crate::framework::ids::ButtonId;
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::mode::parameter_mode::button_row_index;
use crate::framework::string_utils::format_measures;
use crate::framework::view::color_select_mode::ColorSelectMode;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::{update_display, PushDisplayMode};
use super::{host_supports, select_color_view, update_type, AbstractTrackMode, Push3Encoder};

const PLEASE_SELECT_A_CLIP_PUSH1: &str = "      Pleaseselect a clip.";
const PLEASE_SELECT_A_CLIP_PUSH2: &str = "Please select a clip.";

/// The note clip which is used if the piano roll is shown and the active view is not a
/// sequencer (former `model.getNoteClip (8, 128)`).
pub const PIANO_ROLL_CLIP: ClipKey = (8, 128);

/// Which clip the mode edits (former `getMidiClip`).
#[derive(Clone, Copy)]
enum ClipSelection {
    /// The cursor clip of the model.
    Cursor,
    /// The note clip with the size.
    Key(ClipKey),
}

/// The clip mode (former `ClipMode`).
pub struct ClipMode {
    pub track: AbstractTrackMode,
    display_midi_notes: bool,
    active_note_position: Option<NotePosition>,
}

impl Default for ClipMode {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipMode {
    pub fn new() -> Self {
        let base = crate::framework::mode::parameter_mode::ParameterModeBase::new(
            "Clip",
            true,
            crate::framework::mode::parameter_mode::ModeBank::TrackBank,
            crate::framework::mode::parameter_mode::default_knob_ids(),
        );
        ClipMode {
            track: AbstractTrackMode { base, state: Default::default(), menu: vec![(" ".to_string(), false); 8] },
            display_midi_notes: false,
            active_note_position: None,
        }
    }

    /// Former `togglePianoRoll` (called by the clip command).
    pub fn toggle_piano_roll(&mut self) {
        self.display_midi_notes = !self.display_midi_notes;
    }

    /// True if the piano roll is shown.
    pub fn is_piano_roll(&self) -> bool {
        self.display_midi_notes
    }

    fn selection(&self, ctx: &Ctx) -> ClipSelection {
        if self.display_midi_notes {
            let key = ctx.active_view().and_then(|v| v.try_borrow().ok().and_then(|v| v.sequencer_clip_key()));
            return ClipSelection::Key(key.unwrap_or(PIANO_ROLL_CLIP));
        }
        ClipSelection::Cursor
    }

    fn clip<'a>(&self, ctx: &'a Ctx) -> Option<&'a CursorClip> {
        match self.selection(ctx) {
            ClipSelection::Cursor => ctx.model.cursor_clip(),
            ClipSelection::Key((cols, rows)) => ctx.model.note_clip_ref(cols, rows),
        }
    }

    fn clip_mut<'a>(&self, ctx: &'a mut Ctx) -> Option<&'a mut CursorClip> {
        match self.selection(ctx) {
            ClipSelection::Cursor => ctx.model.cursor_clip_mut(),
            ClipSelection::Key((cols, rows)) => Some(ctx.model.note_clip(cols, rows)),
        }
    }

    fn format_measures(ctx: &Ctx, time: f64, start_offset: i32) -> String {
        format_measures(ctx.model.transport().quarters_per_measure(), time, start_offset, false)
    }

    fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if self.display_midi_notes {
            self.display_midi_notes = false;
            return;
        }
        if index == 0 {
            if let Some(clip) = self.clip_mut(ctx) {
                clip.toggle_pinned();
            }
            return;
        }
        if index == 7 {
            select_color_view(ctx, ColorSelectMode::Clip);
        }
    }
}

impl PushDisplayMode for ClipMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let Some(clip) = self.clip(ctx).filter(|c| c.does_exist()) else {
            // Former display.notify: the display is taken out of the context while drawing
            ctx.schedule(0, |ctx| ctx.notify(PLEASE_SELECT_A_CLIP_PUSH1));
            return;
        };
        let ctx = &*ctx;
        display.set_cell(0, 0, "PlayStrt").set_cell(1, 0, &Self::format_measures(ctx, clip.play_start(), 1));
        display.set_cell(0, 1, "Play End").set_cell(1, 1, &Self::format_measures(ctx, clip.play_end(), 1));
        display.set_cell(0, 2, "LoopStrt").set_cell(1, 2, &Self::format_measures(ctx, clip.loop_start(), 1));
        display.set_cell(0, 3, "LopLngth").set_cell(1, 3, &Self::format_measures(ctx, clip.loop_length(), 0));
        display.set_cell(0, 4, "Loop").set_cell(1, 4, if clip.is_loop_enabled() { "On" } else { "Off" });
        display.set_cell(0, 6, "Shuffle").set_cell(1, 6, if clip.is_shuffle_enabled() { "On" } else { "Off" });
        display.set_cell(0, 7, "Accent").set_cell(1, 7, &clip.formatted_accent());
        self.track.draw_row4(ctx, display);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let ctx = &*ctx;
        let Some(clip) = self.clip(ctx).filter(|c| c.does_exist()) else {
            display.add_empty_element();
            display.set_notification_message(PLEASE_SELECT_A_CLIP_PUSH2, ctx.host.now());
            return;
        };

        if self.display_midi_notes {
            display.set_midi_clip_element(MidiClipInfo::from_clip(clip), ctx.model.transport().quarters_per_measure(), self.active_note_position);
            return;
        }

        let tb = ctx.model.current_track_bank();
        let can_pin = host_supports(ctx, Capability::HasPinning);
        let is_pinned = can_pin && clip.is_pinned();
        let on_off = |b: bool| if b { "On" } else { "Off" }.to_string();
        let columns: [(&str, bool, &str, String, bool); 8] = [
            (if can_pin { "Pin clip" } else { "" }, is_pinned, "Play Start", Self::format_measures(ctx, clip.play_start(), 1), self.track.base.is_knob_touched(0)),
            ("", false, "Play End", Self::format_measures(ctx, clip.play_end(), 1), self.track.base.is_knob_touched(1)),
            ("", false, "Loop Start", Self::format_measures(ctx, clip.loop_start(), 1), self.track.base.is_knob_touched(2)),
            ("", false, "Loop Lngth", Self::format_measures(ctx, clip.loop_length(), 0), self.track.base.is_knob_touched(3)),
            ("", false, "Loop", on_off(clip.is_loop_enabled()), self.track.base.is_knob_touched(4)),
            ("", false, "", String::new(), false),
            ("", false, "Shuffle", on_off(clip.is_shuffle_enabled()), self.track.base.is_knob_touched(6)),
            ("Select color", false, "Accent", clip.formatted_accent(), self.track.base.is_knob_touched(7)),
        ];
        for (i, (top_menu, is_top_menu_on, name, value, is_touched)) in columns.iter().enumerate() {
            let t = tb.item(i);
            display.add_parameter_element(top_menu, *is_top_menu_on, t.name(), Some(update_type(t)), t.color(), t.is_selected(), name, -1, value, *is_touched, -1);
        }
    }
}

impl Mode for ClipMode {
    fn name(&self) -> &str {
        self.track.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.track.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.track.base.on_deactivate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if let Some(index) = button_row_index(1, button) {
            if index == 0 {
                if !host_supports(ctx, Capability::HasPinning) {
                    return PUSH2_COLOR2_BLACK;
                }
                let is_pinned = self.clip(ctx).is_some_and(|c| c.is_pinned());
                return if is_pinned { PUSH2_COLOR2_GREEN } else { PUSH2_COLOR2_WHITE };
            }
            if index == 7 {
                return if self.display_midi_notes { PUSH2_COLOR_BLACK } else { PUSH2_COLOR2_WHITE };
            }
            return PUSH2_COLOR_BLACK;
        }
        self.track.button_color(ctx, button)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !self.track.state.increase_knob_movement() {
            return;
        }
        let shift_pressed = ctx.surface.is_shift_pressed();
        let Some(clip) = self.clip_mut(ctx) else {
            return;
        };
        if !clip.does_exist() {
            return;
        }
        match index {
            0 => clip.change_play_start(value, shift_pressed),
            1 => clip.change_play_end(value, shift_pressed),
            2 => clip.change_loop_start(value, shift_pressed),
            3 => clip.change_loop_length(value, shift_pressed),
            4 => clip.set_loop_enabled(value <= 61),
            6 => clip.set_shuffle_enabled(value <= 61),
            7 => clip.change_accent(value, shift_pressed),
            _ => {}
        }
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.track.base.set_touched_knob(index, is_touched);
        if index == 7 && is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            if let Some(clip) = self.clip_mut(ctx) {
                if clip.does_exist() {
                    clip.reset_accent();
                }
            }
        }
    }

    fn is_any_knob_touched(&self) -> bool {
        self.track.base.is_any_knob_touched()
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.track.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.track.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.track.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.track.select_previous_item_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.track.select_next_item_page(ctx);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.track.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.track.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.track.has_previous_item_page(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.track.has_next_item_page(ctx)
    }

    impl_as_any!();
}

impl Push3Encoder for ClipMode {
    fn encoder_turn(&mut self, ctx: &mut Ctx, value: i32) {
        // Set the next/previous note active
        let is_increase = ctx.model.value_changer().is_increase(value);
        let active = self.active_note_position;
        if let Some(clip) = self.clip(ctx) {
            self.active_note_position = if is_increase { clip.next_note(active.as_ref(), true) } else { clip.previous_note(active.as_ref(), true) };
        }
    }

    fn encoder_press(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let Some(position) = self.active_note_position else {
            return;
        };
        if let Some(view) = ctx.active_view() {
            if let Ok(mut view) = view.try_borrow_mut() {
                if view.sequencer_clip_key().is_some() {
                    view.sequencer_edit_note(ctx, &position, false);
                }
            }
        }
    }

    fn encoder_left(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if let Some(view) = ctx.active_view() {
            if let Ok(mut view) = view.try_borrow_mut() {
                if view.sequencer_clip_key().is_some() {
                    view.on_sequencer_left(ctx, event);
                    self.active_note_position = None;
                }
            }
        }
    }

    fn encoder_right(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if let Some(view) = ctx.active_view() {
            if let Ok(mut view) = view.try_borrow_mut() {
                if view.sequencer_clip_key().is_some() {
                    view.on_sequencer_right(ctx, event);
                    self.active_note_position = None;
                }
            }
        }
    }

    fn arrow_center(&mut self, _ctx: &mut Ctx, _event: ButtonEvent) {
        // Not used
    }
}
