// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The views of the Fire (former `PlayView`, `PianoView`, `DrumView64`, `Drum4View`,
//! `SequencerView`, `PolySequencerView`, `SessionView`, `MixView` and `ShiftView`).
//!
//! All views implement [`FireView`] (former `IFireView`): the SELECT knob and the LEDs next to
//! the solo buttons. The XoX drum view (former `DrumXoXView`) is not ported, see the module
//! documentation of the Fire.
//!
//! Differences to Java:
//! - The Drum 4 view does not support BROWSE + pad (Reaper has no browser for drum pads).
//! - Views which are switched from the shift view are switched after the current event.

use super::colors::*;
use super::mode::DRUM_PADS;
use super::surface::is_alt_pressed;
use crate::daw::bank::BankItem;
use crate::daw::clip::{NotePosition, SessionAlternative, StepState};
use crate::daw::color::{ColorEx, DawColor};
use crate::daw::constants::{ChannelType, Resolution};
use crate::framework::commands::mv_helper::{self, delay_display};
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::view::drum64_view::Drum64ViewBase;
use crate::framework::view::play_view::{draw_piano_grid, play_scale_matrix, PlayViewBase};
use crate::framework::view::sequencer::abstract_drum_lane_view::{DrumLaneBase, DrumLaneHooks};
use crate::framework::view::sequencer::abstract_note_sequencer_view::{NoteSequencerBase, NoteSequencerHooks};
use crate::framework::view::sequencer::abstract_poly_sequencer_view::{PolySequencerBase, PolySequencerHooks};
use crate::framework::view::session_view::{SessionColors, SessionHooks, SessionViewBase};
use crate::framework::view::shift_view::ShiftViewBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::{NoteMatrix, Scales};

/// The functions of the views for the SELECT knob and the solo LEDs (former `IFireView`).
pub trait FireView {
    /// The SELECT knob was turned (former `onSelectKnobValue`).
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32);

    /// The color state of the LED next to a solo button: 0 gray, 1 dark red, 2 dark green, 3
    /// red, 4 green (former `getSoloButtonColor`).
    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32;
}

macro_rules! fire_view_casts {
    ($($t:ty),*) => {
        /// Get the Fire functions of a view (former `instanceof IFireView`).
        pub fn as_fire_view(view: &dyn View) -> Option<&dyn FireView> {
            let any = view.as_any();
            $(if let Some(v) = any.downcast_ref::<$t>() {
                return Some(v);
            })*
            None
        }

        /// Get the Fire functions of a view (former `instanceof IFireView`).
        pub fn as_fire_view_mut(view: &mut dyn View) -> Option<&mut dyn FireView> {
            let any = view.as_any_mut();
            $(if any.is::<$t>() {
                return any.downcast_mut::<$t>().map(|v| v as &mut dyn FireView);
            })*
            None
        }
    };
}

fire_view_casts!(PlayView, Drum64View, Drum4View, SequencerView, PolySequencerView, SessionView, MixView, ShiftView);

/// The solo LED color of the active view (former LED5-8 supplier).
pub fn solo_button_color(ctx: &Ctx, index: usize) -> i32 {
    let Some(view) = ctx.active_view() else {
        return 0;
    };
    let Ok(view) = view.try_borrow() else {
        return 0;
    };
    as_fire_view(&*view).map(|v| v.solo_button_color(ctx, index)).unwrap_or(0)
}

/// Forward the SELECT knob to a view.
pub fn on_select_knob_value(ctx: &mut Ctx, view_id: Option<Views>, value: i32) {
    let view = match view_id {
        Some(id) => ctx.views.get(id),
        None => ctx.active_view(),
    };
    let Some(view) = view else {
        return;
    };
    let Ok(mut view) = view.try_borrow_mut() else {
        return;
    };
    if let Some(fire_view) = as_fire_view_mut(&mut *view) {
        fire_view.on_select_knob_value(ctx, value);
    }
}

/// Mark the note of the active sequencer view as edited (former `setNoteEdited`). If the view is
/// borrowed (the event comes from it) the flag is set after the event.
pub fn set_active_sequencer_note_edited(ctx: &mut Ctx) {
    let Some(view) = ctx.active_view() else {
        return;
    };
    let is_borrowed = match view.try_borrow_mut() {
        Ok(mut view) => {
            let any = view.as_any_mut();
            if let Some(v) = any.downcast_mut::<SequencerView>() {
                v.base.seq.set_note_edited();
            } else if let Some(v) = any.downcast_mut::<PolySequencerView>() {
                v.base.seq.set_note_edited();
            } else if let Some(v) = any.downcast_mut::<Drum4View>() {
                v.base.drum.seq.set_note_edited();
            }
            false
        }
        Err(_) => true,
    };
    if is_borrowed {
        ctx.schedule(0, set_active_sequencer_note_edited);
    }
}

/// The solo LEDs of the play and sequencer views: mute, solo and rec arm of the cursor track.
fn cursor_track_solo_color(ctx: &Ctx, index: usize) -> i32 {
    let track = ctx.model.cursor_track();
    if !track.does_exist() {
        return 0;
    }
    match index {
        1 if track.is_mute() => 3,
        2 if track.is_solo() => 4,
        3 if track.is_rec_arm() => 1,
        _ => 0,
    }
}

/// The scene buttons of the play and sequencer views: stop, mute, solo and rec arm of the cursor
/// track.
fn cursor_track_scene_button(ctx: &mut Ctx, button: ButtonId) {
    let track = ctx.model.cursor_track_mut();
    if !track.does_exist() {
        return;
    }
    match button {
        ButtonId::Scene1 => track.stop(false),
        ButtonId::Scene2 => track.toggle_mute(),
        ButtonId::Scene3 => track.toggle_solo(),
        ButtonId::Scene4 => track.toggle_rec_arm(),
        _ => {}
    }
}

fn pressed_color(ctx: &Ctx, button: ButtonId) -> i32 {
    if ctx.surface.is_pressed(button) {
        2
    } else {
        1
    }
}

fn scene_index(button: ButtonId) -> Option<usize> {
    let index = button.ordinal().checked_sub(ButtonId::Scene1.ordinal())?;
    (index < 4).then_some(index)
}

/// Change the scale settings with the arrow buttons (former `handleArrowLeft/Right` of the play
/// and sequencer views). Returns true if the scale changed.
fn change_scale(ctx: &mut Ctx, is_right: bool, change_layout_with_alt: bool, toggle_chromatic: bool) -> bool {
    let is_shift = ctx.surface.is_shift_pressed();
    let is_alt = is_alt_pressed(ctx);
    let scales = ctx.model.scales_mut();
    if is_shift {
        if is_alt && change_layout_with_alt {
            if is_right {
                scales.next_scale_layout();
            } else {
                scales.prev_scale_layout();
            }
            delay_display(ctx, |ctx| ctx.model.scales().scale_layout().name().to_string());
        } else if is_alt {
            if is_right {
                scales.next_scale_offset();
            } else {
                scales.prev_scale_offset();
            }
            delay_display(ctx, |ctx| Scales::BASES[ctx.model.scales().scale_offset_index()].to_string());
        } else {
            if is_right {
                scales.next_scale();
            } else {
                scales.prev_scale();
            }
            delay_display(ctx, |ctx| ctx.model.scales().scale().name().to_string());
        }
        return true;
    }
    if !toggle_chromatic {
        return false;
    }
    if is_alt {
        if is_right {
            scales.next_scale_offset();
        } else {
            scales.prev_scale_offset();
        }
        delay_display(ctx, |ctx| Scales::BASES[ctx.model.scales().scale_offset_index()].to_string());
    } else {
        scales.toggle_chromatic();
        delay_display(ctx, |ctx| if ctx.model.scales().is_chromatic() { "Chromatic" } else { "In Scale" }.to_string());
    }
    true
}

// ------------------------------------------------------------------------------------------
// Play and piano view

fn piano_matrix(ctx: &Ctx) -> NoteMatrix {
    ctx.model.scales().piano_matrix(4, 16)
}

/// The play view and the piano view (former `PlayView` and `PianoView`).
pub struct PlayView {
    play: PlayViewBase,
    is_piano: bool,
}

impl PlayView {
    pub fn new() -> Self {
        PlayView { play: PlayViewBase::new("Play", true), is_piano: false }
    }

    pub fn new_piano() -> Self {
        PlayView { play: PlayViewBase::new("Piano", true), is_piano: true }
    }

    fn matrix(&self, ctx: &Ctx) -> NoteMatrix {
        if self.is_piano {
            piano_matrix(ctx)
        } else {
            play_scale_matrix(ctx)
        }
    }
}

impl View for PlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        let matrix = self.matrix(ctx);
        self.play.on_activate(ctx, matrix);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if self.is_piano {
            draw_piano_grid(ctx, &self.play.base.key_manager, 36, 4, 16);
        } else {
            self.play.draw_grid(ctx);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.play.on_grid_note(ctx, note, velocity);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if scene_index(button).is_some() {
            pressed_color(ctx, button)
        } else {
            0
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        match button {
            ButtonId::ArrowLeft | ButtonId::ArrowRight => {
                change_scale(ctx, button == ButtonId::ArrowRight, true, true);
                // Former updateScale
                self.update_note_mapping(ctx);
                crate::framework::setup_helpers::update_scale_properties(ctx);
            }
            _ => cursor_track_scene_button(ctx, button),
        }
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let chord = self.play.chord_name();
        self.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let matrix = self.matrix(ctx);
        self.play.update_note_mapping(ctx, matrix);
    }

    // AbstractPlayView
    crate::forward_view!(play => on_model_event);

    fn on_setting_changed(&mut self, ctx: &mut Ctx, id: crate::config::SettingId) {
        if id == crate::config::SettingId::ACTIVATE_FIXED_ACCENT || id == crate::config::SettingId::FIXED_ACCENT_VALUE {
            self.play.init_max_velocity(ctx);
        }
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PlayView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if !self.is_piano {
            self.play.on_octave_down(ctx, event, play_scale_matrix);
            return;
        }
        if event != ButtonEvent::Down {
            return;
        }
        self.play.base.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().dec_piano_octave();
        self.update_note_mapping(ctx);
        let text = ctx.model.scales().piano_range_text();
        ctx.notify(&text);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if !self.is_piano {
            self.play.on_octave_up(ctx, event, play_scale_matrix);
            return;
        }
        if event != ButtonEvent::Down {
            return;
        }
        self.play.base.key_manager.clear_pressed_keys();
        ctx.model.scales_mut().inc_piano_octave();
        self.update_note_mapping(ctx);
        let text = ctx.model.scales().piano_range_text();
        ctx.notify(&text);
    }

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool {
        self.play.is_octave_up_button_on(ctx)
    }

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool {
        self.play.is_octave_down_button_on(ctx)
    }

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        if self.is_piano {
            self.play.reset_octave(ctx, piano_matrix);
        } else {
            self.play.reset_octave(ctx, play_scale_matrix);
        }
    }
}

impl FireView for PlayView {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        let is_increase = ctx.model.value_changer().is_increase(value);
        let scales = ctx.model.scales_mut();
        if self.is_piano {
            if is_increase {
                scales.inc_piano_octave();
            } else {
                scales.dec_piano_octave();
            }
            delay_display(ctx, |ctx| ctx.model.scales().piano_range_text());
        } else {
            if is_increase {
                scales.inc_octave();
            } else {
                scales.dec_octave();
            }
            delay_display(ctx, |ctx| ctx.model.scales().range_text());
        }
        self.update_note_mapping(ctx);
    }

    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        cursor_track_solo_color(ctx, index)
    }
}

// ------------------------------------------------------------------------------------------
// Drum 64 view

/// The drum view with 64 pads (former `DrumView64`). SOLO 3 + pad solos the pad, SOLO 4 + pad
/// mutes it.
pub struct Drum64View {
    drum: Drum64ViewBase,
}

impl Drum64View {
    pub fn new() -> Self {
        Drum64View { drum: Drum64ViewBase::new(16, 4) }
    }
}

impl View for Drum64View {
    fn name(&self) -> &str {
        &self.drum.base.name
    }

    crate::forward_view!(drum => on_activate, on_deactivate, draw_grid, update_note_mapping, on_model_event);

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        match button {
            ButtonId::Scene3 | ButtonId::Scene4 => pressed_color(ctx, button),
            _ => 0,
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let Some(played_pad) = self.drum.on_grid_note(ctx, note, velocity) else {
            return;
        };
        // Former handleSoloButton / handleMuteButton
        let is_solo = ctx.surface.is_pressed(ButtonId::Scene3);
        let is_mute = ctx.surface.is_pressed(ButtonId::Scene4);
        if is_solo || is_mute {
            if let Some(bank) = self.drum.drum_pad_bank_mut(ctx) {
                let index = played_pad as usize % bank.page_size().max(1);
                let pad = bank.item_mut(index);
                if is_solo {
                    pad.toggle_solo();
                } else {
                    pad.toggle_mute();
                }
            }
            ctx.surface.set_trigger_consumed(if is_solo { ButtonId::Scene3 } else { ButtonId::Scene4 });
        }
        self.drum.handle_button_combinations(ctx, played_pad, |drum, ctx, played_pad| {
            // Former handleDeleteButton
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            let edit_midi_channel = ctx.config.midi_edit_channel();
            ctx.model.note_clip(8, 128).clear_row(edit_midi_channel, drum.offset_y + played_pad);
        });
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for Drum64View {
    crate::forward_transpose_view!(drum => on_octave_down, on_octave_up, is_octave_up_button_on, is_octave_down_button_on, reset_octave);
}

impl FireView for Drum64View {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        if ctx.model.value_changer().is_increase(value) {
            self.drum.on_octave_up(ctx, ButtonEvent::Down);
        } else {
            self.drum.on_octave_down(ctx, ButtonEvent::Down);
        }
    }

    fn solo_button_color(&self, _ctx: &Ctx, index: usize) -> i32 {
        match index {
            2 => 4,
            3 => 1,
            _ => 0,
        }
    }
}

// ------------------------------------------------------------------------------------------
// Sequencer helpers

/// The repeat operator with PATTERN UP/DOWN + pad (former override of
/// `handleSequencerAreaButtonCombinations`). Returns true if handled.
fn handle_repeat_operator(ctx: &mut Ctx, seq: &crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase, note_position: &NotePosition, velocity: i32) -> bool {
    let is_up_pressed = ctx.surface.is_pressed(ButtonId::ArrowUp);
    if !is_up_pressed && !ctx.surface.is_pressed(ButtonId::ArrowDown) {
        return false;
    }
    ctx.surface.set_trigger_consumed(if is_up_pressed { ButtonId::ArrowUp } else { ButtonId::ArrowDown });
    if velocity > 0 {
        seq.handle_sequencer_area_repeat_operator(ctx, note_position, velocity, is_up_pressed);
    }
    true
}

/// GRID LEFT/RIGHT of the sequencers: ALT changes the resolution, otherwise the clip page is
/// scrolled.
fn sequencer_navigation(ctx: &mut Ctx, seq: &mut crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase, is_right: bool) {
    if is_alt_pressed(ctx) {
        let index = seq.resolution_index(ctx) as i32 + if is_right { 1 } else { -1 };
        seq.set_resolution_index(ctx, index);
        return;
    }
    let clip_key = seq.clip_key();
    let clip = seq.clip(ctx);
    if is_right {
        clip.scroll_steps_page_forward();
    } else {
        clip.scroll_steps_page_backwards();
    }
    mv_helper::notify_edit_page(ctx, clip_key);
}

// ------------------------------------------------------------------------------------------
// Sequencer view

struct SequencerHooks;

impl NoteSequencerHooks for SequencerHooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32) -> bool {
        handle_repeat_operator(ctx, &base.seq, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity)
    }
}

/// The note sequencer (former `SequencerView`): 16 steps, 4 rows, no loop area.
pub struct SequencerView {
    pub base: NoteSequencerBase,
}

impl SequencerView {
    pub fn new() -> Self {
        let mut base = NoteSequencerBase::with_size("Sequencer", 16, 4, true);
        base.num_display_rows = 4;
        SequencerView { base }
    }
}

impl View for SequencerView {
    crate::forward_view!(base.seq => name, update_control_surface, sequencer_resolution_index, set_sequencer_resolution_index);
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);

    fn button_color(&self, _ctx: &Ctx, _button: ButtonId) -> i32 {
        0
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut SequencerHooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.seq.is_active() {
            return;
        }
        let index = note - 36;
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, index.max(0) as usize));
        let y = index / self.base.num_display_cols;
        if y >= self.base.seq.num_sequencer_rows {
            return;
        }
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), index % self.base.num_display_cols, self.base.seq.view.key_manager.map(y));
        self.base.seq.edit_note(ctx, &note_position, false);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        match button {
            ButtonId::ArrowLeft | ButtonId::ArrowRight => {
                let is_right = button == ButtonId::ArrowRight;
                if change_scale(ctx, is_right, false, false) {
                    self.base.update_scale(ctx);
                } else {
                    sequencer_navigation(ctx, &mut self.base.seq, is_right);
                }
            }
            _ => cursor_track_scene_button(ctx, button),
        }
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for SequencerView {
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on, is_octave_down_button_on, reset_octave);
}

impl FireView for SequencerView {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        if ctx.model.value_changer().is_increase(value) {
            self.base.on_octave_up(ctx, ButtonEvent::Down);
        } else {
            self.base.on_octave_down(ctx, ButtonEvent::Down);
        }
        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
        }
    }

    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        cursor_track_solo_color(ctx, index)
    }
}

// ------------------------------------------------------------------------------------------
// Poly sequencer view

struct PolyHooks;

impl PolySequencerHooks for PolyHooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut PolySequencerBase, ctx: &mut Ctx, channel: i32, step: i32) -> bool {
        // With the note mode active, edit all notes of the step
        if ctx.modes.is_active(Modes::Note) {
            let clip_key = base.seq.clip_key();
            for row in 0..128 {
                let note_position = NotePosition::new(channel, step, row);
                let state = ctx.model.note_clip(clip_key.0, clip_key.1).step(&note_position).state();
                if state == StepState::Start {
                    base.seq.edit_note(ctx, &note_position, true);
                }
            }
            if base.seq.edit_notes(ctx).is_empty() {
                ctx.notify("Edit Notes: Off");
            }
            return true;
        }
        base.handle_sequencer_area_button_combinations(ctx, channel, step)
    }
}

/// The poly sequencer (former `PolySequencerView`): 2 rows of steps, 2 rows of notes.
pub struct PolySequencerView {
    pub base: PolySequencerBase,
}

impl PolySequencerView {
    pub fn new(use_track_color: bool) -> Self {
        PolySequencerView { base: PolySequencerBase::with_size(use_track_color, 16, 4, 2) }
    }
}

impl View for PolySequencerView {
    crate::forward_view!(base.seq => name, update_control_surface, sequencer_resolution_index, set_sequencer_resolution_index);
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping, on_model_event, on_grid_note_long_press);

    fn button_color(&self, _ctx: &Ctx, _button: ButtonId) -> i32 {
        0
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut PolyHooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        match button {
            ButtonId::ArrowLeft | ButtonId::ArrowRight => sequencer_navigation(ctx, &mut self.base.seq, button == ButtonId::ArrowRight),
            _ => cursor_track_scene_button(ctx, button),
        }
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PolySequencerView {
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on, is_octave_down_button_on, reset_octave);
}

impl FireView for PolySequencerView {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        if ctx.model.value_changer().is_increase(value) {
            self.base.on_octave_up(ctx, ButtonEvent::Down);
        } else {
            self.base.on_octave_down(ctx, ButtonEvent::Down);
        }
    }

    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        cursor_track_solo_color(ctx, index)
    }
}

// ------------------------------------------------------------------------------------------
// Drum 4 view

struct Drum4Hooks;

impl DrumLaneHooks for Drum4Hooks {
    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumLaneBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32, accent_velocity: i32) -> bool {
        handle_repeat_operator(ctx, &base.drum.seq, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity, accent_velocity)
    }
}

/// The drum sequencer with 4 lanes of 16 steps (former `Drum4View`). The solo buttons mute (with
/// SHIFT solo) the drum pads of the lanes.
pub struct Drum4View {
    pub base: DrumLaneBase,
    block_select_knob: bool,
}

impl Drum4View {
    pub fn new() -> Self {
        Drum4View { base: DrumLaneBase::with_layout("Drum 4", 4, 4, 16, 16, true, false), block_select_knob: false }
    }

    fn has_drum_pads(ctx: &Ctx) -> bool {
        ctx.model.drum_device().is_some_and(|d| d.has_drum_pads())
    }

    /// Former `selectDrumPad`.
    fn select_drum_pad(ctx: &mut Ctx, index: usize) {
        let Some(device) = ctx.model.drum_device_mut() else {
            return;
        };
        let pad = device.drum_pad_bank_mut().item_mut(index);
        pad.select();
        let (exists, name) = (pad.does_exist(), pad.name().to_string());
        super::surface::with_active_mode(ctx, |mode, ctx| {
            if let Some(m) = mode.as_any_mut().downcast_mut::<super::mode::FireChannelMode>() {
                m.base.parameters_adjusted(ctx);
            }
        });
        if exists {
            let name: String = name.chars().take(14).collect();
            ctx.notify(&name);
        }
    }
}

impl View for Drum4View {
    crate::forward_view!(base.drum.seq => name, sequencer_resolution_index, set_sequencer_resolution_index);
    crate::forward_view!(base => on_activate, on_deactivate, draw_grid, update_control_surface, update_note_mapping, on_model_event);

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(scene) = scene_index(button) else {
            return 0;
        };
        let Some(device) = ctx.model.drum_device().filter(|d| d.has_drum_pads()) else {
            return 0;
        };
        let pad = device.drum_pad_bank().item(3 - scene);
        if !pad.does_exist() {
            return 0;
        }
        if pad.is_solo() {
            2
        } else if pad.is_mute() {
            0
        } else {
            1
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut Drum4Hooks, ctx, note, velocity);
    }

    fn on_grid_note_long_press(&mut self, ctx: &mut Ctx, note: i32) {
        if !self.base.drum.seq.is_active() {
            return;
        }
        let index = note - 36;
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Pad1, index.max(0) as usize));
        let y = index / 16;
        // Sequencer steps?
        if y < self.base.drum.play_rows {
            return;
        }
        let step_y = ctx.model.scales().drum_offset() + y;
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), index % 16, step_y);
        self.base.drum.seq.edit_note(ctx, &note_position, false);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down || !self.base.drum.seq.is_active() {
            return;
        }
        if button == ButtonId::ArrowLeft || button == ButtonId::ArrowRight {
            sequencer_navigation(ctx, &mut self.base.drum.seq, button == ButtonId::ArrowRight);
            return;
        }
        let Some(scene) = scene_index(button) else {
            return;
        };
        if !Self::has_drum_pads(ctx) {
            return;
        }
        let is_shift = ctx.surface.is_shift_pressed();
        let is_alt = is_alt_pressed(ctx);
        let Some(device) = ctx.model.drum_device_mut() else {
            return;
        };
        let pad = device.drum_pad_bank_mut().item_mut(3 - scene);
        if !pad.does_exist() {
            return;
        }
        if is_shift {
            pad.toggle_solo();
        } else if is_alt {
            let name = pad.name().to_string();
            ctx.notify(&name);
        } else {
            pad.toggle_mute();
        }
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for Drum4View {
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up);
    crate::forward_transpose_view!(base.drum => is_octave_up_button_on, is_octave_down_button_on, reset_octave);
}

impl FireView for Drum4View {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        if !Self::has_drum_pads(ctx) || self.block_select_knob {
            return;
        }
        let is_up = ctx.model.value_changer().is_increase(value);
        let Some(bank) = ctx.model.drum_device().map(|d| d.drum_pad_bank()) else {
            return;
        };
        let page_size = bank.page_size();
        let selected = bank.selected_index();
        let index = if is_up { selected.map_or(page_size as i32, |s| s as i32 + 1) } else { selected.map_or(-1, |s| s as i32 - 1) };
        if index == page_size as i32 || index == -1 {
            // Former adjustPage
            let selection = if is_up { 0 } else { page_size - 1 };
            self.base.drum.change_octave(ctx, ButtonEvent::Down, is_up, 4, true, true);
            ctx.schedule(100, move |ctx| Self::select_drum_pad(ctx, selection));
            return;
        }
        Self::select_drum_pad(ctx, index as usize);
    }

    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let is_selected = self.base.drum.seq.is_active() && ctx.model.drum_device().filter(|d| d.has_drum_pads()).is_some_and(|d| d.drum_pad_bank().item(3 - index.min(3)).is_selected());
        if is_selected {
            self.base.lanes
        } else {
            0
        }
    }
}

// ------------------------------------------------------------------------------------------
// Session view

struct FireSessionHooks;

impl SessionHooks for FireSessionHooks {
    fn is_button_combination(&self, ctx: &mut Ctx, button: ButtonId) -> bool {
        if ctx.is_button_combination(button) {
            return true;
        }
        (button == ButtonId::Delete && ctx.config.is_delete_mode_active()) || (button == ButtonId::Duplicate && ctx.config.is_duplicate_mode_active())
    }

    fn is_scene_select_action(&self, ctx: &Ctx) -> bool {
        is_alt_pressed(ctx)
    }

    fn do_select_clip_on_launch(&self, ctx: &Ctx) -> bool {
        ctx.config.is_select_clip_on_launch()
    }
}

/// The session view with 16 tracks and 4 scenes (former `SessionView`).
pub struct SessionView {
    pub session: SessionViewBase,
}

impl SessionView {
    pub fn new() -> Self {
        let mut session = SessionViewBase::new(4, 16, true);
        let light = super::surface::light_info;
        session.clip_colors = SessionColors {
            is_recording: light(FIRE_COLOR_RED, FIRE_COLOR_DARKER_RED, false),
            is_recording_queued: light(FIRE_COLOR_RED, FIRE_COLOR_GRAY, true),
            is_playing: light(FIRE_COLOR_GREEN, FIRE_COLOR_DARK_GREEN, false),
            is_playing_queued: light(FIRE_COLOR_GREEN, FIRE_COLOR_GRAY, true),
            is_stop_queued: light(FIRE_COLOR_GREEN, FIRE_COLOR_GRAY, true),
            has_content: light(FIRE_COLOR_ORANGE, FIRE_COLOR_WHITE, false),
            has_no_content: light(FIRE_COLOR_BLACK, -1, false),
            is_rec_armed: light(FIRE_COLOR_DARK_RED, -1, false),
            is_muted: light(FIRE_COLOR_GRAY, -1, false),
        };
        session.bird_color_has_content = session.clip_colors.has_content;
        session.bird_color_selected = session.clip_colors.is_playing;
        SessionView { session }
    }

    /// Fire specific pad combinations (former override of `handleButtonCombinations`): ALT
    /// selects the clip, STOP stops the track. Returns true if handled.
    fn handle_fire_combinations(&self, ctx: &mut Ctx, note: i32) -> bool {
        let (track_index, slot_index) = FireSessionHooks.pad(ctx, &self.session, note);
        if !ctx.model.current_track_bank().item(track_index).does_exist() || ctx.surface.is_pressed(ButtonId::Browse) || ctx.config.is_delete_mode_active() || ctx.config.is_duplicate_mode_active() {
            return false;
        }
        if ctx.is_button_combination(ButtonId::Alt) {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).select();
            return true;
        }
        if ctx.is_button_combination(ButtonId::Stop) {
            ctx.model.current_track_bank_mut().item_mut(track_index).stop(false);
            return true;
        }
        false
    }
}

impl View for SessionView {
    fn name(&self) -> &str {
        "Session"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.session.draw_grid(ctx, &FireSessionHooks, false);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        // Birds-eye-view navigation
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
        if velocity > 0 && self.handle_fire_combinations(ctx, note) {
            ctx.surface.consume_pads();
            return;
        }
        self.session.on_grid_note(ctx, &FireSessionHooks, note, velocity);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(scene) = scene_index(button) else {
            return 0;
        };
        if !ctx.model.scene_bank().item(scene).does_exist() {
            return 0;
        }
        pressed_color(ctx, button)
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if scene_index(button).is_some() && event == ButtonEvent::Up && ctx.surface.is_shift_pressed() {
            self.session.set_alternate_interaction_used(true);
            if is_alt_pressed(ctx) {
                ctx.model.current_track_bank_mut().stop(false);
                return;
            }
        }
        match button {
            ButtonId::ArrowLeft => {
                if event == ButtonEvent::Down {
                    ctx.model.current_track_bank_mut().select_previous_page();
                }
            }
            ButtonId::ArrowRight => {
                if event == ButtonEvent::Down {
                    ctx.model.current_track_bank_mut().select_next_page();
                }
            }
            _ => SessionViewBase::on_scene_button(ctx, &FireSessionHooks, button, event),
        }
    }

    fn is_birds_eye_active(&self) -> bool {
        self.session.is_birds_eye_active
    }

    fn set_birds_eye_active(&mut self, is_active: bool) {
        self.session.is_birds_eye_active = is_active;
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

impl FireView for SessionView {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        let is_increase = ctx.model.value_changer().is_increase(value);
        let is_select = ctx.surface.is_select_pressed();
        let mut scene_bank = ctx.model.scene_bank_mut();
        if is_increase {
            if is_select {
                if scene_bank.can_scroll_page_forwards() {
                    scene_bank.select_next_page();
                    return;
                }
                let position_of_last_item = scene_bank.position_of_last_item();
                if position_of_last_item >= 0 {
                    let index = position_of_last_item as usize % scene_bank.page_size().max(1);
                    let last_item = scene_bank.item_mut(index);
                    if !last_item.is_selected() {
                        last_item.select();
                    }
                }
                return;
            }
            scene_bank.scroll_forwards();
            return;
        }
        if is_select {
            if scene_bank.can_scroll_page_backwards() {
                scene_bank.select_previous_page();
                return;
            }
            let first_item = scene_bank.item_mut(0);
            if !first_item.is_selected() {
                first_item.select();
            }
            return;
        }
        scene_bank.scroll_backwards();
    }

    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let scene = ctx.model.scene_bank().item(index);
        if scene.does_exist() && scene.is_selected() {
            4
        } else {
            0
        }
    }
}

// ------------------------------------------------------------------------------------------
// Mix view

/// The mixer: select, mute, solo and rec arm of 16 tracks (former `MixView`).
pub struct MixView;

fn track_state_color(state: bool, color_track_states: bool, active_color: i32, inactive_color: i32) -> i32 {
    if state {
        active_color
    } else if color_track_states {
        inactive_color
    } else {
        FIRE_COLOR_BLACK
    }
}

impl View for MixView {
    fn name(&self) -> &str {
        "Mix"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let color_track_states = ctx.config.is_color_track_states();
        let mut lights = Vec::new();
        let tb = ctx.model.current_track_bank();
        for i in 0..tb.page_size() {
            let track = tb.item(i);
            let x = i as i32;
            if track.does_exist() {
                let color_index = ctx.colors.color_index(DawColor::color_id(track.color()));
                lights.push((x, 0, color_index, if track.is_selected() { FIRE_COLOR_WHITE } else { -1 }));
                lights.push((x, 1, track_state_color(track.is_mute(), color_track_states, FIRE_COLOR_ORANGE, FIRE_COLOR_DARKER_ORANGE), -1));
                lights.push((x, 2, track_state_color(track.is_solo(), color_track_states, FIRE_COLOR_YELLOW, FIRE_COLOR_DARKER_YELLOW), -1));
                lights.push((x, 3, track_state_color(track.is_rec_arm(), color_track_states, FIRE_COLOR_RED, FIRE_COLOR_DARKER_RED), -1));
            } else {
                for y in 0..4 {
                    lights.push((x, y, FIRE_COLOR_BLACK, -1));
                }
            }
        }
        let grid = ctx.surface.grid();
        for (x, y, color, blink) in lights {
            grid.light_ex_blink(x, y, color, blink, false);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let n = note - 36;
        let index = (n % 16) as usize;
        let what = n / 16;
        let is_delete = ctx.config.is_delete_mode_active();
        let is_duplicate = ctx.config.is_duplicate_mode_active();
        if what == 3 {
            if is_delete {
                ctx.config.toggle_delete_mode_active();
            } else if is_duplicate {
                ctx.config.toggle_duplicate_mode_active();
            }
        }
        let track = ctx.model.current_track_bank_mut().item_mut(index);
        match what {
            3 => {
                if is_delete {
                    track.remove();
                } else if is_duplicate {
                    track.duplicate();
                } else {
                    track.select_or_expand_group();
                }
            }
            2 => track.toggle_mute(),
            1 => track.toggle_solo(),
            0 => track.toggle_rec_arm(),
            _ => {}
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        match button {
            ButtonId::Scene2 | ButtonId::Scene3 => pressed_color(ctx, button),
            _ => 0,
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        match button {
            ButtonId::ArrowLeft => ctx.model.current_track_bank_mut().select_previous_page(),
            ButtonId::ArrowRight => ctx.model.current_track_bank_mut().select_next_page(),
            ButtonId::Scene1 => ctx.model.transport_mut().select_loop_start(),
            ButtonId::Scene2 => ctx.model.track_bank_mut().clear_mute(),
            ButtonId::Scene3 => ctx.model.track_bank_mut().clear_solo(),
            ButtonId::Scene4 => ctx.model.transport_mut().select_loop_end(),
            _ => {}
        }
    }

    impl_as_any!();
}

impl FireView for MixView {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        let increase = ctx.model.value_changer().is_increase(value);
        let slow = ctx.surface.is_shift_pressed();
        ctx.model.transport_mut().change_position(increase, slow);
        super::command::notify_play_position(ctx);
    }

    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        match index {
            1 if ctx.model.track_bank().has_mute() => 3,
            2 if ctx.model.track_bank().has_solo() => 4,
            _ => 0,
        }
    }
}

// ------------------------------------------------------------------------------------------
// Shift view

/// The shift view: note repeat, new clip length, automation, pinning, duplicate, delete and new
/// tracks (former `ShiftView`). The buttons, the SELECT knob and the solo LEDs are relayed to
/// the view below.
pub struct ShiftView {
    shift: ShiftViewBase,
}

impl ShiftView {
    pub fn new() -> Self {
        ShiftView { shift: ShiftViewBase::new("Shift") }
    }

    fn previous_view(ctx: &Ctx) -> Option<Views> {
        ctx.views.active_id_ignore_temporary().filter(|id| *id != Views::Shift)
    }

    fn set_period(ctx: &mut Ctx, index: usize) {
        let resolution = Resolution::ALL[index];
        ctx.config.set_note_repeat_period(resolution);
        ctx.schedule(100, move |ctx| ctx.notify(&format!("Period: {}", resolution.name())));
    }

    fn set_note_length(ctx: &mut Ctx, index: usize) {
        let resolution = Resolution::ALL[index];
        ctx.config.set_note_repeat_length(resolution);
        ctx.schedule(100, move |ctx| ctx.notify(&format!("Note Len: {}", resolution.name())));
    }
}

fn daw(color: DawColor) -> &'static str {
    color.id()
}

fn on_off_color(is_on: bool, on: DawColor, off: DawColor) -> &'static str {
    daw(if is_on { on } else { off })
}

impl View for ShiftView {
    fn name(&self) -> &str {
        &self.shift.base.name
    }

    fn on_activate(&mut self, _ctx: &mut Ctx) {
        self.shift.on_activate();
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let mut lights: Vec<(i32, &'static str)> = Vec::new();
        let config = &ctx.config;
        // Note Repeat on/off
        lights.push((84, on_off_color(config.is_note_repeat_active(), DawColor::Green, DawColor::Gray)));
        // Octave (Reaper does not support note repeat octaves)
        for i in 0..4 {
            lights.push((36 + i, daw(DawColor::ColorOff)));
            lights.push((52 + i, daw(DawColor::ColorOff)));
        }
        for note in [68, 69, 70, 71, 85] {
            lights.push((note, daw(DawColor::ColorOff)));
        }
        // Dec/Inc Arp Mode
        lights.push((86, DawColor::color_id(ColorEx::WHITE)));
        lights.push((87, DawColor::color_id(ColorEx::WHITE)));
        // Note Repeat period and length
        let period_index = Resolution::ALL.iter().position(|r| *r == config.note_repeat_period()).unwrap_or(0);
        let length_index = Resolution::ALL.iter().position(|r| *r == config.note_repeat_length()).unwrap_or(0);
        for (i, (period_note, length_note)) in [(88, 90), (89, 91), (72, 74), (73, 75), (56, 58), (57, 59), (40, 42), (41, 43)].into_iter().enumerate() {
            let off = if i % 2 == 0 { DawColor::Blue } else { DawColor::Pink };
            lights.push((period_note, on_off_color(period_index == i, DawColor::Green, off)));
            lights.push((length_note, on_off_color(length_index == i, DawColor::Green, off)));
        }
        // New clip length
        let clip_length_index = config.new_clip_length();
        for i in 0..8 {
            lights.push((44 + i as i32, on_off_color(i == clip_length_index, DawColor::Red, DawColor::LightOrange)));
        }
        // Automation Mode
        let transport = ctx.model.transport();
        let write_modes = transport.automation_write_modes();
        let write_mode = transport.automation_write_mode();
        for i in 0..8 {
            let color = match write_modes.get(i) {
                Some(mode) => on_off_color(*mode == write_mode, DawColor::Red, DawColor::Gray),
                None => daw(DawColor::ColorOff),
            };
            lights.push((60 + i as i32, color));
        }
        // Not used
        for i in 0..5 {
            lights.push((76 + i, daw(DawColor::ColorOff)));
        }
        // Pinning (the cursor track cannot be pinned)
        lights.push((81, daw(DawColor::Green)));
        lights.push((82, on_off_color(ctx.model.cursor_device().is_pinned(), DawColor::Red, DawColor::Green)));
        lights.push((83, on_off_color(ctx.model.cursor_clip().is_some_and(|c| c.is_pinned()), DawColor::Red, DawColor::Green)));
        lights.push((92, daw(DawColor::ColorOff)));
        lights.push((96, daw(DawColor::ColorOff)));
        // Add tracks
        lights.push((97, DawColor::color_id(ColorEx::ORANGE)));
        lights.push((98, DawColor::color_id(ColorEx::BLUE)));
        lights.push((99, DawColor::color_id(ColorEx::PINK)));

        let is_duplicate = ctx.config.is_duplicate_mode_active();
        let is_delete = ctx.config.is_delete_mode_active();
        let indices: Vec<(i32, i32)> = lights.into_iter().map(|(note, id)| (note, ctx.colors.color_index(id))).collect();
        let grid = ctx.surface.grid();
        for (note, color) in indices {
            grid.light(note, color);
        }
        // Duplicate
        if is_duplicate {
            grid.light_blink(93, FIRE_COLOR_DARK_OCEAN, FIRE_COLOR_BLUE, true);
        } else {
            grid.light(93, FIRE_COLOR_DARK_OCEAN);
        }
        grid.light(94, FIRE_COLOR_GREEN);
        // Delete
        if is_delete {
            grid.light_blink(95, FIRE_COLOR_DARK_RED, FIRE_COLOR_RED, true);
        } else {
            grid.light(95, FIRE_COLOR_DARK_RED);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        self.shift.set_was_used();
        match note {
            // Note repeat octaves are not supported by Reaper
            36..=39 | 52..=55 | 68 => {}
            86 | 87 => {
                let mode = if note == 86 { ctx.config.prev_arpeggiator_mode() } else { ctx.config.next_arpeggiator_mode() };
                if let Some(mode) = mode {
                    ctx.config.set_note_repeat_mode(mode);
                    let name = mode.name().to_string();
                    delay_display(ctx, move |_| format!("Arp: {name}"));
                }
            }
            84 => {
                ctx.config.toggle_note_repeat_active();
                delay_display(ctx, |ctx| format!("Note Repeat: {}", if ctx.config.is_note_repeat_active() { "On" } else { "Off" }));
            }
            88 => Self::set_period(ctx, 0),
            89 => Self::set_period(ctx, 1),
            72 => Self::set_period(ctx, 2),
            73 => Self::set_period(ctx, 3),
            56 => Self::set_period(ctx, 4),
            57 => Self::set_period(ctx, 5),
            40 => Self::set_period(ctx, 6),
            41 => Self::set_period(ctx, 7),
            90 => Self::set_note_length(ctx, 0),
            91 => Self::set_note_length(ctx, 1),
            74 => Self::set_note_length(ctx, 2),
            75 => Self::set_note_length(ctx, 3),
            58 => Self::set_note_length(ctx, 4),
            59 => Self::set_note_length(ctx, 5),
            42 => Self::set_note_length(ctx, 6),
            43 => Self::set_note_length(ctx, 7),
            44..=51 => {
                let new_clip_length = (note - 44) as usize;
                ctx.config.set_new_clip_length(new_clip_length);
                let text = format!("Clip len: {}", crate::config::values::NEW_CLIP_LENGTH_VALUES[new_clip_length.min(7)]);
                ctx.notify(&text);
            }
            60..=67 => {
                let modes = ctx.model.transport().automation_write_modes();
                if let Some(mode) = modes.get((note - 60) as usize).copied() {
                    ctx.model.transport_mut().set_automation_write_mode(mode);
                    super::command::notify_automation_write_mode(ctx);
                }
            }
            // Pinning (the cursor track cannot be pinned)
            81 => {}
            82 => ctx.model.cursor_device_mut().toggle_pinned(),
            83 => {
                if let Some(clip) = ctx.model.cursor_clip_mut() {
                    clip.toggle_pinned();
                }
            }
            93 => {
                ctx.config.toggle_duplicate_mode_active();
                let text = format!("Duplicate {}", if ctx.config.is_duplicate_mode_active() { "Active" } else { "Off" });
                ctx.notify(&text);
            }
            94 => {
                let exists = ctx.model.cursor_clip().is_some_and(|c| c.does_exist());
                if exists {
                    if let Some(clip) = ctx.model.cursor_clip() {
                        clip.duplicate_content();
                    }
                    ctx.notify("Double clip");
                } else {
                    ctx.notify("No clip.");
                }
            }
            95 => {
                ctx.config.toggle_delete_mode_active();
                let text = format!("Delete {}", if ctx.config.is_delete_mode_active() { "Active" } else { "Off" });
                ctx.notify(&text);
            }
            97 => ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]),
            98 => ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]),
            99 => ctx.model.application_mut().add_effect_track(),
            _ => {}
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(view) = Self::previous_view(ctx).and_then(|id| ctx.views.get(id)) else {
            return 0;
        };
        view.try_borrow().map(|v| v.button_color(ctx, button)).unwrap_or(0)
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, velocity: i32) {
        // Relay to the actually active view
        let Some(view) = Self::previous_view(ctx).and_then(|id| ctx.views.get(id)) else {
            return;
        };
        let result = view.try_borrow_mut().map(|mut view| view.on_button(ctx, button, event, velocity));
        drop(result);
    }

    fn was_shift_view_used(&self) -> bool {
        self.shift.was_used()
    }

    impl_as_any!();
}

impl FireView for ShiftView {
    fn on_select_knob_value(&mut self, ctx: &mut Ctx, value: i32) {
        // Relay to the actually active view
        let previous = Self::previous_view(ctx);
        if previous.is_some() {
            on_select_knob_value(ctx, previous, value);
        }
    }

    fn solo_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let Some(view) = Self::previous_view(ctx).and_then(|id| ctx.views.get(id)) else {
            return 0;
        };
        let Ok(view) = view.try_borrow() else {
            return 0;
        };
        as_fire_view(&*view).map(|v| v.solo_button_color(ctx, index)).unwrap_or(0)
    }
}

/// Is the drum device with 16 pads used by the layer modes available?
pub fn has_drum_device_16(ctx: &Ctx) -> bool {
    ctx.model.drum_device_sized(DRUM_PADS).is_some()
}

macro_rules! impl_default {
    ($($t:ty),*) => {
        $(impl Default for $t {
            fn default() -> Self {
                Self::new()
            }
        })*
    };
}

impl_default!(PlayView, Drum64View, SequencerView, Drum4View, SessionView, ShiftView);
