// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The views of the Beatstep (former `controller.arturia.beatstep.view`): track, device, play,
//! drum, sequencer, session, browser and shift view, the knob functions which several views
//! share (former `TrackEditing`) and the `BeatstepView` interface.
//!
//! The 16 pads are the grid notes 36-51 (36-43 the lower row). Unlike the default pad grid, the
//! Java `BeatstepPadGrid.lightEx (x, y)` counts the rows from the bottom; the views here light
//! the notes directly with [`light_xy`].
//!
//! Differences to Java:
//! - The track bank observers of the drum and sequencer views (selection: clear the pressed
//!   keys, notes: mark the played notes) are model events ([`View::on_model_event`]).
//! - The browser view restores the previous view after the current pad event (the view is
//!   borrowed while it handles the pad).
//! - `layer.enter ()` of the device view does nothing: the model has no nested devices of
//!   layers (as `DeviceLayerRightCommand`).

use super::colors::{BEATSTEP_BUTTON_STATE_BLUE, BEATSTEP_BUTTON_STATE_OFF, BEATSTEP_BUTTON_STATE_PINK, BEATSTEP_BUTTON_STATE_RED};
use crate::daw::bank::BankItem;
use crate::daw::clip::{NotePosition, StepState};
use crate::daw::constants::Resolution;
use crate::daw::events::ModelEvent;
use crate::framework::command::{AftertouchCommand, TriggerCommand};
use crate::framework::commands::aftertouch::AftertouchViewCommand;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::Views;
use crate::framework::view::play_view::{play_scale_matrix, PlayViewBase};
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::{ButtonEvent, CMD_NOTE_ON};
use crate::scale::{NoteMatrix, Scales};

/// The number of displayed sequencer steps (former `NUM_DISPLAY_COLS`).
const NUM_DISPLAY_COLS: i32 = 16;

/// The knobs of the Beatstep are handled by the active view (former `BeatstepView`).
pub trait BeatstepView {
    /// A knob has been turned (former `onKnob`). `index` is 0-15, `value` the relative value.
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32);
}

/// Get the Beatstep interface of a view (former cast to `BeatstepView`).
pub fn as_beatstep_view_mut(view: &mut dyn View) -> Option<&mut dyn BeatstepView> {
    let any = view.as_any_mut();
    if any.is::<TrackView>() {
        return any.downcast_mut::<TrackView>().map(|v| v as &mut dyn BeatstepView);
    }
    if any.is::<DeviceView>() {
        return any.downcast_mut::<DeviceView>().map(|v| v as &mut dyn BeatstepView);
    }
    if any.is::<PlayView>() {
        return any.downcast_mut::<PlayView>().map(|v| v as &mut dyn BeatstepView);
    }
    if any.is::<DrumView>() {
        return any.downcast_mut::<DrumView>().map(|v| v as &mut dyn BeatstepView);
    }
    if any.is::<SequencerView>() {
        return any.downcast_mut::<SequencerView>().map(|v| v as &mut dyn BeatstepView);
    }
    if any.is::<SessionView>() {
        return any.downcast_mut::<SessionView>().map(|v| v as &mut dyn BeatstepView);
    }
    if any.is::<BrowserView>() {
        return any.downcast_mut::<BrowserView>().map(|v| v as &mut dyn BeatstepView);
    }
    any.downcast_mut::<ShiftView>().map(|v| v as &mut dyn BeatstepView)
}

/// Light a pad by its column and row, row 0 is the lower row (former
/// `BeatstepPadGrid.lightEx`).
fn light_xy(ctx: &mut Ctx, x: i32, y: i32, color: i32) {
    ctx.surface.grid().light(36 + x + 8 * y, color);
}

fn light(ctx: &mut Ctx, note: i32, color: i32) {
    ctx.surface.grid().light(note, color);
}

/// True if the track with the index is the selected track of the current track bank.
fn is_selected_track(ctx: &Ctx, track_index: i32) -> bool {
    track_index >= 0 && ctx.model.current_track_bank().selected_index() == Some(track_index as usize)
}

fn is_increase(ctx: &Ctx, value: i32) -> bool {
    ctx.model.value_changer().is_increase(value)
}

// ------------------------------------------------------------------------------------------
// TrackEditing

/// A knob is moved for changing a track parameter (former `TrackEditing.onTrackKnob`).
///
/// Java passes `surface.isShiftPressed ()` to the tempo and the play position change, which is
/// always false on the Beatstep (the flag of `BeatstepControlSurface` is never set).
pub fn on_track_knob(ctx: &mut Ctx, index: usize, value: i32) {
    if ctx.model.current_track_bank().selected_item().is_none() {
        return;
    }
    let is_increase = is_increase(ctx, value);
    match index {
        0..=4 | 8..=11 => {
            let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() else {
                return;
            };
            match index {
                0 => track.change_volume(value),
                1 => track.change_pan(value),
                2 => track.set_mute(is_increase),
                3 => track.set_solo(is_increase),
                // The crossfade mode parameter of the track (Reaper has none)
                4 => {}
                // Send 1 - 4
                _ => track.send_bank_mut().item_mut(index - 8).change_value(value),
            }
        }
        5 => ctx.model.transport_mut().change_tempo(is_increase, false),
        6 => ctx.model.transport_mut().change_position(is_increase, false),
        7 => ctx.model.master_track_mut().change_volume(value),
        // Not used
        _ => {}
    }
}

// ------------------------------------------------------------------------------------------
// TrackView

/// The track view (former `TrackView`).
#[derive(Default)]
pub struct TrackView;

impl TrackView {
    pub fn new() -> Self {
        TrackView
    }
}

impl BeatstepView for TrackView {
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if index < 12 {
            on_track_knob(ctx, index, value);
            return;
        }
        if ctx.model.current_track_bank().selected_item().is_none() {
            return;
        }
        match index {
            // Send 5 - 6
            12 | 13 => {
                if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
                    track.send_bank_mut().item_mut(index - 8).change_value(value);
                }
            }
            // Crossfader
            15 => ctx.model.change_crossfade(value),
            // 14 is not used
            _ => {}
        }
    }
}

impl View for TrackView {
    fn name(&self) -> &str {
        "Track"
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let index = note - 36;
        match index {
            0..=2 => {
                if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
                    match index {
                        0 => track.toggle_is_activated(),
                        1 => track.toggle_rec_arm(),
                        _ => track.toggle_group_expanded(),
                    }
                }
            }
            3 => ctx.model.application_mut().add_instrument_track(),
            4 => ctx.model.application_mut().add_audio_track(),
            5 => ctx.model.application_mut().add_effect_track(),
            6 => ctx.model.current_track_bank_mut().select_previous_page(),
            7 => ctx.model.current_track_bank_mut().select_next_page(),
            // 8-15
            _ => self.select_track(ctx, (index - 8) as usize),
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let tb = ctx.model.current_track_bank();
        let selected: [bool; 8] = std::array::from_fn(|i| tb.item(i).is_selected());
        let sel = tb.selected_item();
        let activated = sel.is_some_and(|t| t.is_activated());
        let rec_arm = sel.is_some_and(|t| t.is_rec_arm());
        let group_color = match sel {
            Some(t) if t.is_group() => {
                if t.is_group_expanded() {
                    BEATSTEP_BUTTON_STATE_PINK
                } else {
                    BEATSTEP_BUTTON_STATE_BLUE
                }
            }
            _ => BEATSTEP_BUTTON_STATE_OFF,
        };
        for (i, is_selected) in selected.iter().enumerate() {
            light(ctx, 44 + i as i32, if *is_selected { BEATSTEP_BUTTON_STATE_BLUE } else { BEATSTEP_BUTTON_STATE_OFF });
        }
        light(ctx, 36, if activated { BEATSTEP_BUTTON_STATE_BLUE } else { BEATSTEP_BUTTON_STATE_OFF });
        light(ctx, 37, if rec_arm { BEATSTEP_BUTTON_STATE_RED } else { BEATSTEP_BUTTON_STATE_OFF });
        light(ctx, 38, group_color);
        light(ctx, 39, BEATSTEP_BUTTON_STATE_PINK);
        light(ctx, 40, BEATSTEP_BUTTON_STATE_PINK);
        light(ctx, 41, BEATSTEP_BUTTON_STATE_PINK);
        light(ctx, 42, BEATSTEP_BUTTON_STATE_BLUE);
        light(ctx, 43, BEATSTEP_BUTTON_STATE_BLUE);
    }

    impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// DeviceView

/// The device view (former `DeviceView`).
#[derive(Default)]
pub struct DeviceView {
    is_layer: bool,
}

impl DeviceView {
    pub fn new() -> Self {
        DeviceView { is_layer: false }
    }

    /// True if the pads navigate the layers instead of the devices.
    pub fn is_layer(&self) -> bool {
        self.is_layer
    }
}

impl BeatstepView for DeviceView {
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if index < 8 {
            on_track_knob(ctx, index, value);
            return;
        }
        if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
            bank.item_mut(index - 8).change_value(value);
        }
    }
}

impl View for DeviceView {
    fn name(&self) -> &str {
        "Device"
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 || !ctx.model.has_selected_device() {
            return;
        }

        let sel = ctx.model.cursor_device().layer_bank().selected_index();
        let cd = ctx.model.cursor_device_mut();
        match note - 36 {
            // Toggle device on/off
            0 => cd.toggle_enabled_state(),

            // Device Left
            1 => {
                if self.is_layer {
                    let index = sel.map_or(0, |idx| idx as i32 - 1);
                    if index >= 0 {
                        cd.layer_bank_mut().item_mut(index as usize).select();
                    }
                } else {
                    cd.select_previous();
                }
            }

            // Device Right
            2 => {
                if self.is_layer {
                    let index = sel.map_or(0, |idx| idx + 1);
                    cd.layer_bank_mut().item_mut(index.min(7)).select();
                } else {
                    cd.select_next();
                }
            }

            // Enter layer
            3 => {
                if !cd.has_layers() {
                    return;
                }
                if self.is_layer {
                    // Former sel.enter (): the model has no nested devices of layers
                } else if sel.is_none() {
                    cd.layer_bank_mut().item_mut(0).select();
                }
                self.is_layer = !self.is_layer;
            }

            // Exit layer
            4 => {
                if self.is_layer {
                    self.is_layer = false;
                } else if cd.is_nested() {
                    cd.select_parent();
                    cd.select_channel();
                    self.is_layer = true;
                }
            }

            // Intentionally empty
            5 => {}

            // Parameter bank down
            6 => {
                if let Some(bank) = cd.parameter_bank_mut() {
                    bank.scroll_backwards();
                }
            }

            // Parameter bank page up
            7 => {
                if let Some(bank) = cd.parameter_bank_mut() {
                    bank.scroll_forwards();
                }
            }

            index => {
                if let Some(bank) = cd.parameter_bank_mut() {
                    bank.select_page((index - 8).max(0) as usize);
                }
            }
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let cd = ctx.model.cursor_device();
        let (page_size, selected_item_index) =
            cd.parameter_bank().map_or((0, usize::MAX), |bank| (bank.page_bank().page_size(), bank.page_bank().selected_item_index()));
        let is_enabled = cd.is_enabled();
        for i in 0..page_size {
            light(ctx, 44 + i as i32, if i == selected_item_index { BEATSTEP_BUTTON_STATE_BLUE } else { BEATSTEP_BUTTON_STATE_OFF });
        }
        light(ctx, 36, if is_enabled { BEATSTEP_BUTTON_STATE_RED } else { BEATSTEP_BUTTON_STATE_OFF });
        light(ctx, 37, BEATSTEP_BUTTON_STATE_BLUE);
        light(ctx, 38, BEATSTEP_BUTTON_STATE_BLUE);
        light(ctx, 39, BEATSTEP_BUTTON_STATE_RED);
        light(ctx, 40, BEATSTEP_BUTTON_STATE_RED);
        light(ctx, 41, BEATSTEP_BUTTON_STATE_OFF);
        light(ctx, 42, BEATSTEP_BUTTON_STATE_BLUE);
        light(ctx, 43, BEATSTEP_BUTTON_STATE_BLUE);
    }

    impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// PlayView

/// The play view (former `PlayView`).
pub struct PlayView {
    play: PlayViewBase,
    aftertouch: Box<dyn AftertouchCommand>,
}

impl Default for PlayView {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayView {
    /// The play view with its aftertouch command (former `registerAftertouchCommand` in
    /// `registerContinuousCommands`).
    pub fn new() -> Self {
        PlayView { play: PlayViewBase::new("Play", false), aftertouch: Box::new(AftertouchViewCommand::new()) }
    }

    /// Former `getPadColor`.
    fn pad_color(&self, ctx: &Ctx, is_keyboard_enabled: bool, pad: i32) -> i32 {
        if !is_keyboard_enabled {
            return BEATSTEP_BUTTON_STATE_OFF;
        }
        let key_manager = &self.play.base.key_manager;
        if key_manager.is_key_pressed(pad) {
            return BEATSTEP_BUTTON_STATE_PINK;
        }
        ctx.colors.color_index(key_manager.color(ctx.model.scales(), pad))
    }
}

impl BeatstepView for PlayView {
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let is_increase = is_increase(ctx, value);
        match index {
            // Chromatic
            12 => {
                ctx.model.scales_mut().set_chromatic(!is_increase);
                ctx.config.set_scale_in_key(is_increase);
                ctx.notify(if is_increase { "In Key" } else { "Chromatic" });
            }

            // Base Note
            13 => {
                if is_increase {
                    ctx.model.scales_mut().next_scale_offset();
                } else {
                    ctx.model.scales_mut().prev_scale_offset();
                }
                let scale_base = Scales::BASES[ctx.model.scales().scale_offset_index()];
                ctx.notify(scale_base);
                ctx.config.set_scale_base(scale_base);
            }

            // Scale
            14 => {
                if is_increase {
                    ctx.model.scales_mut().next_scale();
                } else {
                    ctx.model.scales_mut().prev_scale();
                }
                let scale = ctx.model.scales().scale().name();
                ctx.config.set_scale(scale);
                ctx.notify(scale);
            }

            // Octave
            15 => {
                self.play.base.key_manager.clear_pressed_keys();
                if is_increase {
                    ctx.model.scales_mut().inc_octave();
                } else {
                    ctx.model.scales_mut().dec_octave();
                }
                let octave = ctx.model.scales().octave();
                let message = format!("Octave {}{octave} ({})", if octave > 0 { "+" } else { "" }, ctx.model.scales().range_text());
                ctx.notify(&message);
            }

            // 0-11
            _ => on_track_knob(ctx, index, value),
        }

        self.update_note_mapping(ctx);
    }
}

impl View for PlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.play.on_activate(ctx, play_scale_matrix(ctx));
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let is_keyboard_enabled = ctx.model.can_selected_track_hold_notes();
        for i in 36..52 {
            let color = self.pad_color(ctx, is_keyboard_enabled, i);
            light(ctx, i, color);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.play.on_grid_note(ctx, note, velocity);
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        let chord = self.play.chord_name();
        self.play.update_control_surface(ctx, chord);
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        self.play.update_note_mapping(ctx, play_scale_matrix(ctx));
    }

    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        Some(&mut self.aftertouch)
    }

    // AbstractPlayView
    crate::forward_view!(play => on_model_event);

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for PlayView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.on_octave_down(ctx, event, play_scale_matrix);
    }

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.play.on_octave_up(ctx, event, play_scale_matrix);
    }

    // AbstractPlayView
    crate::forward_transpose_view!(play => is_octave_up_button_on, is_octave_down_button_on);

    fn reset_octave(&mut self, ctx: &mut Ctx) {
        self.play.reset_octave(ctx, play_scale_matrix);
    }
}

// ------------------------------------------------------------------------------------------
// BaseSequencerView

/// The state shared by the drum and the sequencer view (former `BaseSequencerView`).
pub struct BaseSequencer {
    pub seq: SequencerBase,
    /// The selected pad, 0-15.
    pub selected_pad: i32,
    /// True if the pads play (and select) notes, false if they edit the steps.
    pub is_play_mode: bool,
}

impl BaseSequencer {
    /// The 16 steps of a clip with 128 rows.
    fn new(name: &str) -> Self {
        BaseSequencer { seq: SequencerBase::new(name, 128, NUM_DISPLAY_COLS as usize, 128, false), selected_pad: 0, is_play_mode: true }
    }

    /// Former `changeScrollPosition`.
    fn change_scroll_position(&mut self, ctx: &mut Ctx, is_turned_right: bool) {
        if !self.seq.is_active() {
            return;
        }
        let clip = self.seq.clip(ctx);
        if is_turned_right {
            clip.scroll_steps_page_forward();
        } else {
            clip.scroll_steps_page_backwards();
        }
    }

    /// Former `changeResolution`.
    fn change_resolution(&mut self, ctx: &mut Ctx, value: i32) {
        if !self.seq.is_active() {
            return;
        }
        let is_inc = value >= 65;
        let selected_resolution_index = Resolution::change(self.seq.resolution_index(ctx), is_inc);
        self.seq.clip(ctx).set_step_length(Resolution::value_at(selected_resolution_index));
    }

    /// The knobs 12, 13 and 15 of the drum and the sequencer view.
    fn on_sequencer_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32, is_increase: bool) {
        match index {
            12 => self.change_scroll_position(ctx, is_increase),
            13 => {
                self.change_resolution(ctx, value);
                let name = Resolution::name_at(self.seq.resolution_index(ctx));
                ctx.notify(name);
            }
            // Toggle play / sequencer
            15 => {
                self.is_play_mode = !is_increase;
                ctx.notify(if self.is_play_mode { "Play/Select" } else { "Sequence" });
            }
            _ => {}
        }
    }

    /// Toggle the step of a pad in sequence mode (the upper pads are the steps 1-8). The
    /// velocity is the fixed accent if active.
    fn toggle_step(&mut self, ctx: &mut Ctx, index: i32, note: i32, velocity: i32) {
        let step = if index < 8 { index + 8 } else { index - 8 };
        let position = NotePosition::new(ctx.config.midi_edit_channel(), step, note);
        let velocity = if ctx.config.is_accent_active() { ctx.config.fixed_accent_value() } else { velocity };
        self.seq.clip(ctx).toggle_step(&position, velocity);
    }

    /// Paint the 16 sequencer steps of a note (former part of `drawGrid`).
    fn draw_steps(&mut self, ctx: &mut Ctx, note: i32) {
        let channel = ctx.config.midi_edit_channel();
        let clip = self.seq.clip(ctx);
        let step = clip.current_step();
        let hi_step = if SequencerBase::is_in_x_range(clip, step) { step % NUM_DISPLAY_COLS } else { -1 };
        let mut colors = [BEATSTEP_BUTTON_STATE_OFF; NUM_DISPLAY_COLS as usize];
        for (col, color) in colors.iter_mut().enumerate() {
            let position = NotePosition::new(channel, col as i32, note);
            *color = sequencer_pad_color(clip.step(&position).state(), col as i32 == hi_step);
        }
        for (col, color) in colors.iter().enumerate() {
            let col = col as i32;
            light_xy(ctx, col % 8, 1 - col / 8, *color);
        }
    }
}

/// Former `getSequencerPadColor` / `getSequencerColor`.
fn sequencer_pad_color(step_state: StepState, hilite: bool) -> i32 {
    if hilite {
        return BEATSTEP_BUTTON_STATE_PINK;
    }
    if step_state != StepState::Off {
        BEATSTEP_BUTTON_STATE_BLUE
    } else {
        BEATSTEP_BUTTON_STATE_OFF
    }
}

// ------------------------------------------------------------------------------------------
// DrumView

/// The drum view (former `DrumView`).
pub struct DrumView {
    base: BaseSequencer,
}

impl Default for DrumView {
    fn default() -> Self {
        Self::new()
    }
}

impl DrumView {
    pub fn new() -> Self {
        DrumView { base: BaseSequencer::new("Drum") }
    }

    /// True if the pads play the drum pads, false if they edit the steps.
    pub fn is_play_mode(&self) -> bool {
        self.base.is_play_mode
    }

    /// The selected pad, 0-15.
    pub fn selected_pad(&self) -> i32 {
        self.base.selected_pad
    }

    /// Former `getDrumPadColor`.
    fn drum_pad_color(&self, ctx: &Ctx, index: i32) -> i32 {
        let offset_y = ctx.model.scales().drum_offset();

        // Playing note?
        if self.base.seq.view.key_manager.is_key_pressed(offset_y + index) {
            return BEATSTEP_BUTTON_STATE_PINK;
        }

        // Selected?
        if self.base.selected_pad == index {
            return BEATSTEP_BUTTON_STATE_RED;
        }

        // Exists and active?
        let Some(primary) = ctx.model.drum_device() else {
            return BEATSTEP_BUTTON_STATE_OFF;
        };
        let drum_pad_bank = primary.drum_pad_bank();
        let drum_pad = drum_pad_bank.item(index as usize);
        if !drum_pad.does_exist() || !drum_pad.is_activated() {
            return BEATSTEP_BUTTON_STATE_OFF;
        }

        // Muted or soloed?
        if drum_pad.is_mute() || drum_pad_bank.has_soloed_pads() && !drum_pad.is_solo() {
            return BEATSTEP_BUTTON_STATE_OFF;
        }
        BEATSTEP_BUTTON_STATE_BLUE
    }
}

/// Scroll the drum pads of the drum device to the next or previous page and select the first pad
/// of the page after 75 ms (former `IDrumPadBank.selectNextPage` / `selectPreviousPage` of
/// `AbstractItemBankImpl`).
fn select_drum_pad_page(ctx: &mut Ctx, is_next: bool) {
    let Some(bank) = ctx.model.drum_device_mut().map(|device| device.drum_pad_bank_mut()) else {
        return;
    };
    if is_next {
        if !bank.can_scroll_page_forwards() {
            return;
        }
        bank.scroll_page_forwards();
    } else {
        if !bank.can_scroll_page_backwards() {
            return;
        }
        bank.scroll_page_backwards();
    }
    ctx.schedule(75, |ctx| {
        if let Some(device) = ctx.model.drum_device_mut() {
            device.drum_pad_bank_mut().select_pad(0);
        }
    });
}

impl BeatstepView for DrumView {
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let is_increase = is_increase(ctx, value);
        match index {
            12 | 13 | 15 => self.base.on_sequencer_knob(ctx, index, value, is_increase),

            // Up/Down
            14 => {
                self.base.seq.view.key_manager.clear_pressed_keys();
                if is_increase {
                    ctx.model.scales_mut().inc_drum_octave();
                    select_drum_pad_page(ctx, true);
                } else {
                    ctx.model.scales_mut().dec_drum_octave();
                    select_drum_pad_page(ctx, false);
                }
                self.update_note_mapping(ctx);
                let text = ctx.model.scales().drum_range_text();
                ctx.notify(&text);
            }

            // 0-11
            _ => on_track_knob(ctx, index, value),
        }
        if index == 15 {
            self.update_note_mapping(ctx);
        }
    }
}

impl View for DrumView {
    fn name(&self) -> &str {
        self.base.seq.name()
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        self.base.seq.update_control_surface(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let index = note - 36;
        let offset_y = ctx.model.scales().drum_offset();
        if self.base.is_play_mode {
            self.base.selected_pad = index; // 0-16

            // Mark selected note
            self.base.seq.view.key_manager.set_key_pressed(offset_y + self.base.selected_pad, velocity);
        } else if velocity != 0 {
            self.base.toggle_step(ctx, index, offset_y + self.base.selected_pad, velocity);
        }
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let matrix: NoteMatrix = if ctx.model.can_selected_track_hold_notes() && self.base.is_play_mode {
            ctx.model.scales().drum_matrix()
        } else {
            Scales::empty_matrix()
        };
        self.base.seq.view.update_note_mapping(ctx, matrix);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !ctx.model.can_selected_track_hold_notes() {
            ctx.surface.grid().turn_off();
            return;
        }

        if self.base.is_play_mode {
            for y in 0..2 {
                for x in 0..8 {
                    let color = self.drum_pad_color(ctx, 8 * y + x);
                    light_xy(ctx, x, y, color);
                }
            }
            return;
        }

        let note = ctx.model.scales().drum_offset() + self.base.selected_pad;
        self.base.draw_steps(ctx, note);
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        match event {
            ModelEvent::TrackSelectionChanged { .. } => self.base.seq.view.key_manager.clear_pressed_keys(),
            // Former updateNote
            ModelEvent::TrackNote { track_index, note, velocity } if is_selected_track(ctx, *track_index) => {
                self.base.seq.view.key_manager.set_key_pressed(*note, *velocity);
            }
            _ => {}
        }
    }

    // AbstractSequencerView
    crate::forward_view!(base.seq => on_sequencer_left, on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);

    impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// SequencerView

/// The note sequencer view (former `SequencerView`).
pub struct SequencerView {
    base: BaseSequencer,
    /// The first note of the scale matrix (former `offsetY`).
    offset_y: i32,
}

/// The note of the first pad (former `START_KEY`).
const START_KEY: i32 = 36;

impl Default for SequencerView {
    fn default() -> Self {
        Self::new()
    }
}

impl SequencerView {
    pub fn new() -> Self {
        SequencerView { base: BaseSequencer::new("Sequencer"), offset_y: START_KEY }
    }

    /// True if the pads play (and select) notes, false if they edit the steps.
    pub fn is_play_mode(&self) -> bool {
        self.base.is_play_mode
    }

    /// The selected pad, 0-15.
    pub fn selected_pad(&self) -> i32 {
        self.base.selected_pad
    }

    /// The note of the selected pad in the scale matrix.
    fn selected_note(&self, ctx: &Ctx) -> i32 {
        let y = (self.offset_y + self.base.selected_pad).clamp(0, 127) as usize;
        ctx.model.scales().note_matrix()[y]
    }
}

impl BeatstepView for SequencerView {
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let is_increase = is_increase(ctx, value);
        match index {
            12 | 13 | 15 => {
                self.base.on_sequencer_knob(ctx, index, value, is_increase);
                if index == 15 {
                    self.update_note_mapping(ctx);
                }
            }

            // Up/Down
            14 => {
                self.base.seq.view.key_manager.clear_pressed_keys();
                if is_increase {
                    ctx.model.scales_mut().inc_octave();
                } else {
                    ctx.model.scales_mut().dec_octave();
                }
                self.update_note_mapping(ctx);
                let text = ctx.model.scales().range_text();
                ctx.notify(&text);
            }

            // 0-11
            _ => on_track_knob(ctx, index, value),
        }
    }
}

impl View for SequencerView {
    fn name(&self) -> &str {
        self.base.seq.name()
    }

    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        self.base.seq.update_control_surface(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        let index = note - 36;
        if self.base.is_play_mode {
            self.base.selected_pad = index; // 0-16

            // Mark selected notes
            let key_manager = &mut self.base.seq.view.key_manager;
            let mapped = key_manager.map(note);
            for i in 0..128 {
                if mapped == key_manager.map(i) {
                    key_manager.set_key_pressed(i, velocity);
                }
            }
        } else if velocity != 0 {
            let map = self.selected_note(ctx);
            self.base.toggle_step(ctx, index, map, velocity);
        }
    }

    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let matrix = if ctx.model.can_selected_track_hold_notes() && self.base.is_play_mode {
            ctx.model.scales().note_matrix()
        } else {
            Scales::empty_matrix()
        };
        self.base.seq.view.update_note_mapping(ctx, matrix);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if !ctx.model.can_selected_track_hold_notes() {
            ctx.surface.grid().turn_off();
            return;
        }

        if self.base.is_play_mode {
            for i in 36..52 {
                let key_manager = &self.base.seq.view.key_manager;
                let color = if key_manager.is_key_pressed(i) || self.base.selected_pad == i - 36 {
                    BEATSTEP_BUTTON_STATE_PINK
                } else {
                    ctx.colors.color_index(key_manager.color(ctx.model.scales(), i))
                };
                light(ctx, i, color);
            }
        } else {
            let note = self.selected_note(ctx);
            self.base.draw_steps(ctx, note);
        }
    }

    fn on_model_event(&mut self, ctx: &mut Ctx, event: &ModelEvent) {
        match event {
            ModelEvent::TrackSelectionChanged { .. } => self.base.seq.view.key_manager.clear_pressed_keys(),
            // Former updateNote: light notes sent from the sequencer
            ModelEvent::TrackNote { track_index, note, velocity } => {
                if !is_selected_track(ctx, *track_index) {
                    return;
                }
                let key_manager = &mut self.base.seq.view.key_manager;
                for i in 0..128 {
                    if key_manager.map(i) == *note {
                        key_manager.set_key_pressed(i, *velocity);
                    }
                }
            }
            _ => {}
        }
    }

    // AbstractSequencerView
    crate::forward_view!(base.seq => on_sequencer_left, on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);

    impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// SessionView

/// The session view (former `SessionView`).
#[derive(Default)]
pub struct SessionView;

impl SessionView {
    pub fn new() -> Self {
        SessionView
    }
}

impl BeatstepView for SessionView {
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        // Knob 12-15 are currently not used
        if index < 12 {
            on_track_knob(ctx, index, value);
        }
    }
}

impl View for SessionView {
    fn name(&self) -> &str {
        "Session"
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        let index = note - 36;
        match index {
            6 => {
                if velocity > 0 {
                    ctx.model.scene_bank_mut().select_previous_page();
                }
            }
            7 => {
                if velocity > 0 {
                    ctx.model.scene_bank_mut().select_next_page();
                }
            }
            8..=15 => ctx.model.scene_bank_mut().item_mut((index - 8) as usize).launch(velocity > 0, false),
            // 0-5: not used
            _ => {}
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        for i in 0..6 {
            light(ctx, 36 + i, BEATSTEP_BUTTON_STATE_OFF);
        }
        for i in 6..16 {
            light(ctx, 36 + i, BEATSTEP_BUTTON_STATE_BLUE);
        }
    }

    impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// BrowserView

/// The browser view (former `BrowserView`).
#[derive(Default)]
pub struct BrowserView;

impl BrowserView {
    pub fn new() -> Self {
        BrowserView
    }
}

impl BeatstepView for BrowserView {
    fn on_knob(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        if !ctx.model.browser().is_some_and(|b| b.is_active()) {
            return;
        }
        let value_changer = ctx.model.value_changer();
        let steps = value_changer.calc_stepped_knob_change(value).abs();
        let is_increase = value_changer.is_increase(value);
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        match index {
            8..=14 => {
                let column = index - 8;
                for _ in 0..steps {
                    if is_increase {
                        browser.select_next_filter_item(column);
                    } else {
                        browser.select_previous_filter_item(column);
                    }
                }
            }
            15 => {
                for _ in 0..steps {
                    if is_increase {
                        browser.select_next_result();
                    } else {
                        browser.select_previous_result();
                    }
                }
            }
            // Not used
            _ => {}
        }
    }
}

impl View for BrowserView {
    fn name(&self) -> &str {
        "Browser"
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.browser().is_some_and(|b| b.is_active()) {
            return;
        }
        let preview_note = match note - 36 {
            // Cancel, OK
            index @ (0 | 7) => {
                if velocity == 0 {
                    return;
                }
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.stop_browsing(index == 7);
                }
                crate::framework::view::deferred::restore_view_later(ctx);
                return;
            }
            // Notes for preview
            2 => 12,
            3 => 24,
            4 => 36,
            5 => 48,
            10 => 60,
            11 => 72,
            12 => 84,
            13 => 96,
            // Not used
            _ => return,
        };
        ctx.surface.send_midi_event(CMD_NOTE_ON, preview_note, velocity);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        light(ctx, 36, BEATSTEP_BUTTON_STATE_RED);
        light(ctx, 37, BEATSTEP_BUTTON_STATE_OFF);
        for i in 2..6 {
            light(ctx, 36 + i, BEATSTEP_BUTTON_STATE_PINK);
        }
        light(ctx, 42, BEATSTEP_BUTTON_STATE_OFF);
        light(ctx, 43, BEATSTEP_BUTTON_STATE_BLUE);
        light(ctx, 44, BEATSTEP_BUTTON_STATE_OFF);
        light(ctx, 45, BEATSTEP_BUTTON_STATE_OFF);
        for i in 10..14 {
            light(ctx, 36 + i, BEATSTEP_BUTTON_STATE_PINK);
        }
        light(ctx, 50, BEATSTEP_BUTTON_STATE_OFF);
        light(ctx, 51, BEATSTEP_BUTTON_STATE_OFF);
    }

    impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// ShiftView

/// The views which can be selected with the pads 9-14 in the shift view (former `VIEWS`).
pub const SHIFT_VIEWS: [Views; 6] = [Views::Track, Views::Device, Views::Play, Views::Drum, Views::Sequencer, Views::Session];

/// The shift view (former `ShiftView`): transport, browser and view selection.
pub struct ShiftView {
    play_command: PlayCommand,
}

impl Default for ShiftView {
    fn default() -> Self {
        Self::new()
    }
}

impl ShiftView {
    pub fn new() -> Self {
        ShiftView { play_command: PlayCommand::new() }
    }
}

impl BeatstepView for ShiftView {
    fn on_knob(&mut self, _ctx: &mut Ctx, _index: usize, _value: i32) {
        // Knobs not used in Shift view
    }
}

impl View for ShiftView {
    fn name(&self) -> &str {
        "Shift"
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let t = ctx.model.transport();
        let pink_or = |is_on: bool, color: i32| if is_on { BEATSTEP_BUTTON_STATE_PINK } else { color };
        let colors = [
            pink_or(t.is_playing(), BEATSTEP_BUTTON_STATE_BLUE),
            pink_or(t.is_recording(), BEATSTEP_BUTTON_STATE_RED),
            pink_or(t.is_loop(), BEATSTEP_BUTTON_STATE_OFF),
            pink_or(t.is_metronome_on(), BEATSTEP_BUTTON_STATE_OFF),
            BEATSTEP_BUTTON_STATE_OFF,
            BEATSTEP_BUTTON_STATE_RED,
            BEATSTEP_BUTTON_STATE_RED,
            BEATSTEP_BUTTON_STATE_RED,
            BEATSTEP_BUTTON_STATE_RED,
            BEATSTEP_BUTTON_STATE_RED,
            BEATSTEP_BUTTON_STATE_PINK,
            BEATSTEP_BUTTON_STATE_PINK,
            BEATSTEP_BUTTON_STATE_PINK,
            BEATSTEP_BUTTON_STATE_BLUE,
            BEATSTEP_BUTTON_STATE_OFF,
            pink_or(ctx.model.cursor_device().is_window_open(), BEATSTEP_BUTTON_STATE_BLUE),
        ];
        for (i, color) in colors.iter().enumerate() {
            light(ctx, 36 + i as i32, *color);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }

        match note - 36 {
            // Play
            0 => self.play_command.execute_normal(ctx, ButtonEvent::Up),

            // Record
            1 => ctx.model.transport_mut().start_recording(),

            // Repeat
            2 => ctx.model.transport_mut().toggle_loop(),

            // Click
            3 => ctx.model.transport_mut().toggle_metronome(),

            // Tap Tempo
            4 => ctx.model.transport_mut().tap_tempo(),

            // Insert device before / after current
            index @ (5 | 6) => {
                let device = ctx.model.cursor_device();
                let (name, position) = (device.name().to_string(), device.position());
                if let Some(browser) = ctx.model.browser_mut() {
                    if index == 5 {
                        browser.insert_before_cursor_device(&name, position);
                    } else {
                        browser.insert_after_cursor_device(&name, position);
                    }
                }
            }

            // Open the browser
            7 => ctx.model.browse_cursor_device_presets(),

            // Toggle window of VSTs
            15 => ctx.model.cursor_device_mut().toggle_window_open(),

            _ => {
                let view_index = note - 44;
                if !(0..6).contains(&view_index) {
                    return;
                }
                let view_id = SHIFT_VIEWS[view_index as usize];
                ctx.views.set_previous_id(Some(view_id));
                let name = ctx.views.get(view_id).map(|view| view.borrow().name().to_string());
                if let Some(name) = name {
                    ctx.notify(&name);
                }
            }
        }
    }

    impl_as_any!();
}
