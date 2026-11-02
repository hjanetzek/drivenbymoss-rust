// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The views of the APC mini (former `controller.akai.apcmini.view`): play, session, drum,
//! sequencer, raindrops, browser and shift view, and the track button functions (former
//! `TrackButtons` and the `APCminiView` interface).

use super::colors::{mk1, mk2};
use super::definition::ApcMiniDefinition;
use super::surface::{self as apc, APC_BUTTON_STATE_OFF, APC_BUTTON_STATE_ON};
use crate::config::apcmini as config;
use crate::daw::bank::BankItem;
use crate::framework::color::{BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::abstract_session_view::{COLOR_SCENE, COLOR_SCENE_OFF, COLOR_SELECTED_SCENE};
use crate::framework::command::TriggerCommand;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::mode::is_send_mode;
use crate::framework::commands::mv_helper;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::grid::LightInfo;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::view::base::{is_scene_button, scene_index};
use crate::framework::view::browser_view::BrowserViewBase;
use crate::framework::view::play_view::{play_scale_matrix, PlayViewBase};
use crate::framework::view::sequencer::abstract_drum_view::{DefaultDrumHooks, DrumBase};
use crate::framework::view::sequencer::abstract_note_sequencer_view::{DefaultNoteSequencerHooks, NoteSequencerBase};
use crate::framework::view::sequencer::abstract_raindrops_view::{RaindropsBase, NUM_OCTAVE};
use crate::framework::view::session_view::{SessionColors, SessionHooks, SessionViewBase};
use crate::framework::view::shift_view::ShiftViewBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::ButtonEvent;
use crate::scale::Scales;

/// The track buttons of the APC mini are controlled by the active view (former `APCminiView`).
pub trait ApcMiniView {
    /// A track button was pressed (former `onSelectTrack`).
    fn on_select_track(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent);

    /// The state of a track button (former `getTrackButtonColor`).
    fn track_button_color(&self, ctx: &Ctx, index: usize) -> i32;
}

/// Get the APC mini interface of a view (former cast to `APCminiView`).
pub fn as_mini_view(view: &dyn View) -> Option<&dyn ApcMiniView> {
    let any = view.as_any();
    if let Some(v) = any.downcast_ref::<PlayView>() {
        return Some(v);
    }
    if let Some(v) = any.downcast_ref::<SessionView>() {
        return Some(v);
    }
    if let Some(v) = any.downcast_ref::<SequencerView>() {
        return Some(v);
    }
    if let Some(v) = any.downcast_ref::<DrumView>() {
        return Some(v);
    }
    if let Some(v) = any.downcast_ref::<RaindropsView>() {
        return Some(v);
    }
    if let Some(v) = any.downcast_ref::<ApcMiniBrowserView>() {
        return Some(v);
    }
    if let Some(v) = any.downcast_ref::<ShiftView>() {
        return Some(v);
    }
    None
}

/// Get the mutable APC mini interface of a view.
pub fn as_mini_view_mut(view: &mut dyn View) -> Option<&mut dyn ApcMiniView> {
    let any = view.as_any_mut();
    if any.is::<PlayView>() {
        return any.downcast_mut::<PlayView>().map(|v| v as &mut dyn ApcMiniView);
    }
    if any.is::<SessionView>() {
        return any.downcast_mut::<SessionView>().map(|v| v as &mut dyn ApcMiniView);
    }
    if any.is::<SequencerView>() {
        return any.downcast_mut::<SequencerView>().map(|v| v as &mut dyn ApcMiniView);
    }
    if any.is::<DrumView>() {
        return any.downcast_mut::<DrumView>().map(|v| v as &mut dyn ApcMiniView);
    }
    if any.is::<RaindropsView>() {
        return any.downcast_mut::<RaindropsView>().map(|v| v as &mut dyn ApcMiniView);
    }
    if any.is::<ApcMiniBrowserView>() {
        return any.downcast_mut::<ApcMiniBrowserView>().map(|v| v as &mut dyn ApcMiniView);
    }
    any.downcast_mut::<ShiftView>().map(|v| v as &mut dyn ApcMiniView)
}

fn on_off(is_on: bool) -> i32 {
    if is_on {
        APC_BUTTON_STATE_ON
    } else {
        APC_BUTTON_STATE_OFF
    }
}

fn state_on_off(is_on: bool) -> &'static str {
    if is_on {
        BUTTON_STATE_ON
    } else {
        BUTTON_STATE_OFF
    }
}

/// The resolution of the sequencers on the scene buttons (former `getButtonColorID` of the
/// sequencer views).
fn resolution_color_id(ctx: &Ctx, button: ButtonId, resolution_index: usize) -> &'static str {
    let index = button.ordinal() as i32 - ButtonId::Scene1.ordinal() as i32;
    let res = 7 - index;
    let is_keyboard_enabled = ctx.model.can_selected_track_hold_notes();
    state_on_off(is_keyboard_enabled && res == resolution_index as i32)
}

// ------------------------------------------------------------------------------------------
// TrackButtons

/// The functions of the track buttons in the play and session views (former `TrackButtons`).
pub mod track_buttons {
    use super::*;

    /// Former `getTrackButtonColor`.
    pub fn track_button_color(ctx: &Ctx, index: usize) -> i32 {
        let track = ctx.model.current_track_bank().item(index);
        match apc::track_state(ctx) {
            apc::TRACK_STATE_CLIP_STOP => on_off(ctx.surface.is_pressed(ButtonId::get(ButtonId::RowSelect1, index))),
            apc::TRACK_STATE_SOLO => on_off(track.is_solo()),
            apc::TRACK_STATE_REC_ARM => on_off(track.is_rec_arm()),
            apc::TRACK_STATE_MUTE => on_off(!track.is_mute()),
            apc::TRACK_STATE_SELECT => on_off(track.is_selected()),
            _ => APC_BUTTON_STATE_OFF,
        }
    }

    /// Former `onSelectTrack`.
    pub fn on_select_track(view: &mut dyn View, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let state = apc::track_state(ctx);
        let track = ctx.model.current_track_bank_mut().item_mut(index);
        match state {
            apc::TRACK_STATE_CLIP_STOP => track.stop(false),
            apc::TRACK_STATE_SOLO => track.toggle_solo(),
            apc::TRACK_STATE_REC_ARM => track.toggle_rec_arm(),
            apc::TRACK_STATE_MUTE => track.toggle_mute(),
            apc::TRACK_STATE_SELECT => view.select_track(ctx, index),
            _ => {}
        }
    }
}

// ------------------------------------------------------------------------------------------
// PlayView

/// The play view (former `PlayView`).
pub struct PlayView {
    play: PlayViewBase,
}

impl PlayView {
    pub fn new(use_track_color: bool) -> Self {
        PlayView { play: PlayViewBase::new("Play", use_track_color) }
    }
}

impl View for PlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.play.on_activate(ctx, play_scale_matrix(ctx));
    }

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        if button == ButtonId::Scene3 {
            BUTTON_STATE_OFF
        } else {
            BUTTON_STATE_ON
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down || !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        match button {
            ButtonId::Scene1 | ButtonId::Scene2 => {
                if button == ButtonId::Scene1 {
                    ctx.model.scales_mut().next_scale_layout();
                } else {
                    ctx.model.scales_mut().prev_scale_layout();
                }
                let message = format!("Scale layout: {}", ctx.model.scales().scale_layout().name());
                ctx.notify(&message);
            }
            ButtonId::Scene4 | ButtonId::Scene5 => {
                if button == ButtonId::Scene4 {
                    ctx.model.scales_mut().prev_scale();
                } else {
                    ctx.model.scales_mut().next_scale();
                }
                let name = ctx.model.scales().scale().name().to_string();
                ctx.notify(&name);
            }
            ButtonId::Scene6 => {
                ctx.model.scales_mut().toggle_chromatic();
                let message = if ctx.model.scales().is_chromatic() { "Chromatic" } else { "In Key" };
                ctx.notify(message);
            }
            ButtonId::Scene7 => self.on_octave_up(ctx, event),
            ButtonId::Scene8 => self.on_octave_down(ctx, event),
            _ => {}
        }
        // Former updateScale
        self.update_note_mapping(ctx);
        crate::framework::setup_helpers::update_scale_properties(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.play.draw_grid(ctx);
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

impl ApcMiniView for PlayView {
    fn on_select_track(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        track_buttons::on_select_track(self, ctx, index, event);
    }

    fn track_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        track_buttons::track_button_color(ctx, index)
    }
}

// ------------------------------------------------------------------------------------------
// SessionView

/// The overridden functions of the APC mini session view.
struct ApcMiniSessionHooks;

impl SessionHooks for ApcMiniSessionHooks {
    fn do_select_clip_on_launch(&self, ctx: &Ctx) -> bool {
        ctx.config.is_select_clip_on_launch()
    }
}

/// The session view (former `SessionView`).
pub struct SessionView {
    name: String,
    session: SessionViewBase,
}

impl SessionView {
    pub fn new(use_track_color: bool) -> Self {
        let mut session = SessionViewBase::new(8, 8, use_track_color);
        session.clip_colors = if use_track_color {
            session.ignore_clip_color_for_play_and_record = true;
            SessionColors {
                is_recording: LightInfo::new(mk2::RED, mk2::RED, false),
                is_recording_queued: LightInfo::new(mk2::RED, mk2::RED, true),
                is_playing: LightInfo::new(mk2::GREEN, mk2::GREEN, false),
                is_playing_queued: LightInfo::new(mk2::GREEN, mk2::GREEN, true),
                is_stop_queued: LightInfo::new(mk2::GREEN, mk2::GREEN, true),
                has_content: LightInfo::new(mk2::ORANGE, mk2::WHITE, false),
                has_no_content: LightInfo::new(mk2::BLACK, -1, false),
                is_rec_armed: LightInfo::new(mk2::DARK_RED, -1, false),
                is_muted: LightInfo::new(mk2::DARK_GRAY, -1, false),
            }
        } else {
            SessionColors {
                is_recording: LightInfo::new(mk1::APC_COLOR_RED, -1, false),
                is_recording_queued: LightInfo::new(mk1::APC_COLOR_RED_BLINK, mk1::APC_COLOR_RED_BLINK, false),
                is_playing: LightInfo::new(mk1::APC_COLOR_GREEN, -1, false),
                is_playing_queued: LightInfo::new(mk1::APC_COLOR_GREEN_BLINK, mk1::APC_COLOR_GREEN_BLINK, false),
                is_stop_queued: LightInfo::new(mk1::APC_COLOR_GREEN, mk1::APC_COLOR_GREEN_BLINK, false),
                has_content: LightInfo::new(mk1::APC_COLOR_YELLOW, mk1::APC_COLOR_YELLOW_BLINK, false),
                has_no_content: LightInfo::new(mk1::APC_COLOR_BLACK, -1, false),
                is_rec_armed: LightInfo::new(mk1::APC_COLOR_BLACK, -1, false),
                is_muted: LightInfo::new(mk1::APC_COLOR_BLACK, -1, false),
            }
        };
        SessionView { name: "Session".to_string(), session }
    }
}

impl View for SessionView {
    fn name(&self) -> &str {
        &self.name
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let scene = button.ordinal() as i32 - ButtonId::Scene1.ordinal() as i32;
        if !(0..8).contains(&scene) {
            return BUTTON_COLOR_OFF;
        }
        let s = ctx.model.scene_bank().item(scene as usize);
        if s.does_exist() {
            return if s.is_selected() { COLOR_SELECTED_SCENE } else { COLOR_SCENE };
        }
        COLOR_SCENE_OFF
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.session.draw_grid(ctx, &ApcMiniSessionHooks, false);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.session.on_grid_note(ctx, &ApcMiniSessionHooks, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if is_scene_button(button) {
            SessionViewBase::on_scene_button(ctx, &ApcMiniSessionHooks, button, event);
            if event == ButtonEvent::Up && ctx.surface.is_shift_pressed() {
                self.session.set_alternate_interaction_used(true);
            }
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

impl crate::daw::clip::SessionAlternative for SessionView {
    fn was_alternate_interaction_used(&self) -> bool {
        self.session.was_alternate_interaction_used()
    }

    fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.session.set_alternate_interaction_used(was_used);
    }
}

impl ApcMiniView for SessionView {
    fn on_select_track(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        track_buttons::on_select_track(self, ctx, index, event);
    }

    fn track_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        track_buttons::track_button_color(ctx, index)
    }
}

// ------------------------------------------------------------------------------------------
// SequencerView

/// The note sequencer view (former `SequencerView`).
pub struct SequencerView {
    base: NoteSequencerBase,
    definition: ApcMiniDefinition,
}

impl SequencerView {
    pub fn new(use_track_color: bool, definition: ApcMiniDefinition) -> Self {
        SequencerView { base: NoteSequencerBase::new("Sequencer", use_track_color), definition }
    }

    /// The lowest note row of the sequencer (changed by the octave buttons).
    pub fn offset_y(&self) -> i32 {
        self.base.offset_y
    }
}

impl View for SequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, update_control_surface, on_sequencer_left,
        on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractNoteSequencerView
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        resolution_color_id(ctx, button, self.base.seq.resolution_index(ctx))
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut DefaultNoteSequencerHooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.seq.on_button(ctx, button, event);
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for SequencerView {
    // AbstractNoteSequencerView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

impl ApcMiniView for SequencerView {
    fn on_select_track(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        match self.definition.swap_shifted_track_indices(index) {
            0 => self.base.on_octave_up(ctx, event),
            1 => self.base.on_octave_down(ctx, event),
            2 => self.base.seq.on_left(ctx, event),
            3 => self.base.seq.on_right(ctx, event),
            _ => {}
        }
        self.base.update_scale(ctx);
    }

    fn track_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let octave = ctx.model.scales().octave();
        let clip = self.base.seq.clip_ref(ctx);
        let exists = clip.is_some_and(|c| c.does_exist());
        match self.definition.swap_shifted_track_indices(index) {
            0 => on_off(octave < Scales::OCTAVE_RANGE),
            1 => on_off(octave > -Scales::OCTAVE_RANGE),
            2 => on_off(exists && clip.is_some_and(|c| c.can_scroll_steps_backwards())),
            3 => on_off(exists && clip.is_some_and(|c| c.can_scroll_steps_forwards())),
            _ => APC_BUTTON_STATE_OFF,
        }
    }
}

// ------------------------------------------------------------------------------------------
// DrumView

/// The drum sequencer view (former `DrumView`).
pub struct DrumView {
    base: DrumBase,
    definition: ApcMiniDefinition,
}

impl DrumView {
    pub fn new(use_track_color: bool, definition: ApcMiniDefinition) -> Self {
        let mut base = DrumBase::new("Drum", 4, 4, use_track_color);
        // Former getPadContentColor override
        base.fixed_pad_content_color = true;
        DrumView { base, definition }
    }
}

impl View for DrumView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, on_sequencer_left, on_sequencer_right,
        sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractDrumView
    crate::forward_view!(base => on_activate, on_deactivate, draw_grid, update_control_surface,
        update_note_mapping, on_model_event);

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        resolution_color_id(ctx, button, self.base.seq.resolution_index(ctx))
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut DefaultDrumHooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.seq.on_button(ctx, button, event);
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for DrumView {
    // AbstractDrumView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

impl ApcMiniView for DrumView {
    fn on_select_track(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        match self.definition.swap_shifted_track_indices(index) {
            0 => self.base.on_octave_up(ctx, event),
            1 => self.base.on_octave_down(ctx, event),
            2 => self.base.seq.on_left(ctx, event),
            3 => self.base.seq.on_right(ctx, event),
            _ => {}
        }
    }

    fn track_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let clip = self.base.seq.clip_ref(ctx);
        let exists = clip.is_some_and(|c| c.does_exist());
        match self.definition.swap_shifted_track_indices(index) {
            0 => on_off(ctx.model.scales().can_scroll_drum_octave_up()),
            1 => on_off(ctx.model.scales().can_scroll_drum_octave_down()),
            2 => on_off(exists && clip.is_some_and(|c| c.can_scroll_steps_backwards())),
            3 => on_off(exists && clip.is_some_and(|c| c.can_scroll_steps_forwards())),
            _ => APC_BUTTON_STATE_OFF,
        }
    }
}

// ------------------------------------------------------------------------------------------
// RaindropsView

/// The raindrops sequencer view (former `RaindropsView`).
pub struct RaindropsView {
    base: RaindropsBase,
    definition: ApcMiniDefinition,
}

impl RaindropsView {
    pub fn new(use_track_color: bool, definition: ApcMiniDefinition) -> Self {
        RaindropsView { base: RaindropsBase::new("Raindrops", use_track_color), definition }
    }
}

impl View for RaindropsView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, update_control_surface, on_sequencer_left,
        on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractRaindropsView
    crate::forward_view!(base => on_activate, draw_grid, on_grid_note, update_note_mapping);

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        resolution_color_id(ctx, button, self.base.seq.resolution_index(ctx))
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        self.base.on_button(ctx, button, event);
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for RaindropsView {
    // AbstractRaindropsView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

impl ApcMiniView for RaindropsView {
    fn on_select_track(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        match self.definition.swap_shifted_track_indices(index) {
            0 => self.base.on_octave_up(ctx, event),
            1 => self.base.on_octave_down(ctx, event),
            _ => {}
        }
        self.base.update_scale(ctx);
    }

    fn track_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let num_rows = self.base.seq.clip_ref(ctx).map_or(0, |c| c.num_rows() as i32);
        match self.definition.swap_shifted_track_indices(index) {
            0 => on_off(self.base.offset_y + NUM_OCTAVE <= num_rows - NUM_OCTAVE),
            1 => on_off(self.base.offset_y - NUM_OCTAVE >= 0),
            _ => APC_BUTTON_STATE_OFF,
        }
    }
}

// ------------------------------------------------------------------------------------------
// APCMiniBrowserView

/// The browser view (former `APCMiniBrowserView`).
pub struct ApcMiniBrowserView {
    browser: BrowserViewBase,
}

impl ApcMiniBrowserView {
    pub fn new() -> Self {
        ApcMiniBrowserView { browser: BrowserViewBase::new() }
    }
}

impl Default for ApcMiniBrowserView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ApcMiniBrowserView {
    fn name(&self) -> &str {
        &self.browser.base.name
    }

    // BrowserView
    crate::forward_view!(browser => on_activate, draw_grid, on_grid_note);

    fn button_color_id(&self, _ctx: &Ctx, _button: ButtonId) -> &'static str {
        self.browser.button_color_id()
    }

    impl_as_any!();
}

impl ApcMiniView for ApcMiniBrowserView {
    fn on_select_track(&mut self, _ctx: &mut Ctx, _index: usize, _event: ButtonEvent) {
        // Intentionally empty
    }

    fn track_button_color(&self, _ctx: &Ctx, _index: usize) -> i32 {
        mk1::APC_COLOR_BLACK
    }
}

// ------------------------------------------------------------------------------------------
// ShiftView

/// The positions of the base notes on the first two pad rows (former `TRANSLATE`).
const TRANSLATE: [i32; 16] = [0, 2, 4, 6, 1, 3, 5, -1, -1, 10, 8, -1, 11, 9, 7, -1];

/// The colors of the shift view.
struct ShiftColors {
    green: i32,
    yellow: i32,
    red: i32,
    black: i32,
    red_blink: i32,
    green_blink: i32,
}

/// The shift view (former `ShiftView`).
pub struct ShiftView {
    shift: ShiftViewBase,
    play_command: PlayCommand,
    new_command: NewCommand,
    definition: ApcMiniDefinition,
    colors: ShiftColors,
}

impl ShiftView {
    pub fn new(definition: ApcMiniDefinition) -> Self {
        let colors = if definition.has_rgb_colors() {
            ShiftColors {
                green: mk2::GREEN,
                yellow: mk2::ORANGE,
                red: mk2::RED,
                black: mk2::BLACK,
                green_blink: mk2::BRIGHT_GREEN,
                red_blink: mk2::BRIGHT_RED,
            }
        } else {
            ShiftColors {
                green: mk1::APC_COLOR_GREEN,
                yellow: mk1::APC_COLOR_YELLOW,
                red: mk1::APC_COLOR_RED,
                black: mk1::APC_COLOR_BLACK,
                green_blink: mk1::APC_COLOR_GREEN_BLINK,
                red_blink: mk1::APC_COLOR_RED_BLINK,
            }
        };
        ShiftView { shift: ShiftViewBase::new("Shift"), play_command: PlayCommand::new(), new_command: NewCommand::new(), definition, colors }
    }

    /// Former `switchToView`. The shift view is active (and borrowed), therefore the view is
    /// switched after the current event.
    fn switch_to_view(ctx: &mut Ctx, view_id: Views) {
        ctx.schedule(0, move |ctx| {
            ctx.set_active_view(view_id);
            let name = ctx.views.get(view_id).and_then(|view| view.try_borrow().ok().map(|v| v.name().to_string()));
            if let Some(name) = name {
                ctx.notify(&name);
            }
        });
    }

    /// Former `scrollParameterBank`.
    fn scroll_parameter_bank(ctx: &mut Ctx, scroll_back: bool) {
        let Some(parameter_bank) = ctx.model.cursor_device_mut().parameter_bank_mut() else {
            return;
        };
        if scroll_back {
            if !parameter_bank.can_scroll_page_backwards() {
                return;
            }
            parameter_bank.scroll_backwards();
        } else {
            if !parameter_bank.can_scroll_page_forwards() {
                return;
            }
            parameter_bank.scroll_forwards();
        }
        mv_helper::notify_selected_parameter_page(ctx);
    }

    fn select_mode(ctx: &mut Ctx, mode: Modes, name: &str) {
        ctx.set_active_mode(mode);
        config::set_fader_ctrl(&mut ctx.config, name);
        ctx.notify(name);
    }
}

impl View for ShiftView {
    fn name(&self) -> &str {
        &self.shift.base.name
    }

    fn on_activate(&mut self, _ctx: &mut Ctx) {
        self.shift.on_activate();
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let c = &self.colors;
        // Draw the keyboard
        let scale_offset = ctx.model.scales().scale_offset_index();
        let model_transport = ctx.model.transport();
        let (is_playing, is_recording) = (model_transport.is_playing(), model_transport.is_recording());
        let is_launcher_overdub = ctx.model.is_launcher_overdub();
        let previous_view_id = ctx.views.active_id_ignore_temporary();
        let pick = |offset: usize, base_color: i32| if scale_offset == offset { c.green } else { base_color };
        let view_color = |view: Views| if previous_view_id == Some(view) { c.green } else { c.yellow };

        let grid = ctx.surface.grid();
        // 0'C', 1'G', 2'D', 3'A', 4'E', 5'B', 6'F', 7'Bb', 8'Eb', 9'Ab', 10'Db', 11'Gb'
        for i in 7..64 {
            grid.light(36 + i, c.black);
        }
        grid.light(36, pick(0, c.yellow));
        grid.light(36 + 1, pick(2, c.yellow));
        grid.light(36 + 2, pick(4, c.yellow));
        grid.light(36 + 3, pick(6, c.yellow));
        grid.light(36 + 4, pick(1, c.yellow));
        grid.light(36 + 5, pick(3, c.yellow));
        grid.light(36 + 6, pick(5, c.yellow));
        grid.light(36 + 9, pick(10, c.red));
        grid.light(36 + 10, pick(8, c.red));
        grid.light(36 + 12, pick(11, c.red));
        grid.light(36 + 13, pick(9, c.red));
        grid.light(36 + 14, pick(7, c.red));

        // Device Parameters up/down
        grid.light(36 + 24, c.yellow);
        grid.light(36 + 25, c.yellow);
        // Device up/down
        grid.light(36 + 32, c.green);
        grid.light(36 + 33, c.green);

        // Change the scale
        grid.light(36 + 35, c.red);
        grid.light(36 + 36, c.red);
        grid.light(36 + 27, c.green);

        // Draw the view selection: Session, Note, Drum, Sequencer
        grid.light(36 + 56, view_color(Views::Session));
        grid.light(36 + 57, view_color(Views::Play));
        grid.light(36 + 58, view_color(Views::Drum));
        grid.light(36 + 59, view_color(Views::Sequencer));
        grid.light(36 + 60, view_color(Views::Raindrops));

        // Draw transport
        grid.light(36 + 63, if is_playing { c.green_blink } else { c.green });
        grid.light(36 + 55, if is_recording { c.red_blink } else { c.red });
        grid.light(36 + 47, c.yellow);
        grid.light(36 + 39, c.yellow);

        grid.light(36 + 62, c.yellow);
        grid.light(36 + 54, if is_launcher_overdub { c.red_blink } else { c.red });
        grid.light(36 + 46, c.yellow);
        grid.light(36 + 38, c.yellow);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity != 0 {
            return;
        }
        self.shift.set_was_used();

        let n = ctx.surface.grid_ref().translate_to_controller(note).1;
        match n {
            // Flip views
            56 => Self::switch_to_view(ctx, Views::Session),
            57 => Self::switch_to_view(ctx, Views::Play),
            58 => Self::switch_to_view(ctx, Views::Drum),
            59 => Self::switch_to_view(ctx, Views::Sequencer),
            60 => Self::switch_to_view(ctx, Views::Raindrops),

            // Last row transport
            63 => {
                self.play_command.execute_normal(ctx, ButtonEvent::Up);
                ctx.notify("Start/Stop");
            }
            55 => {
                ctx.model.transport_mut().start_recording();
                ctx.notify("Record");
            }
            47 => {
                ctx.model.transport_mut().toggle_loop();
                ctx.notify("Toggle Loop");
            }
            39 => {
                ctx.model.transport_mut().toggle_metronome();
                ctx.notify("Toggle Click");
            }

            // Navigation
            62 => {
                self.new_command.execute_now(ctx);
                ctx.notify("New clip");
            }
            54 => {
                ctx.model.toggle_launcher_overdub();
                ctx.notify("Toggle Launcher Overdub");
            }
            46 => {
                let amount = ctx.config.quantize_amount() as f64 / 100.0;
                if let Some(clip) = ctx.model.cursor_clip() {
                    if clip.does_exist() {
                        clip.quantize(amount);
                    }
                }
                ctx.notify("Quantize");
            }
            38 => {
                ctx.model.application_mut().undo();
                ctx.notify("Undo");
            }

            // Device Parameters up/down
            24 => Self::scroll_parameter_bank(ctx, true),
            25 => Self::scroll_parameter_bank(ctx, false),

            // Device up/down
            32 => {
                if ctx.model.cursor_device().can_select_previous() {
                    ctx.model.cursor_device_mut().select_previous();
                    let message = format!("Device: {}", ctx.model.cursor_device().name());
                    ctx.notify(&message);
                }
            }
            33 => {
                if ctx.model.cursor_device().can_select_next() {
                    ctx.model.cursor_device_mut().select_next();
                    let message = format!("Device: {}", ctx.model.cursor_device().name());
                    ctx.notify(&message);
                }
            }

            // Change the scale
            35 | 36 => {
                if n == 35 {
                    ctx.model.scales_mut().prev_scale();
                } else {
                    ctx.model.scales_mut().next_scale();
                }
                let name = ctx.model.scales().scale().name().to_string();
                ctx.config.set_scale(&name);
                ctx.notify(&name);
            }
            27 => {
                let is_chromatic = !ctx.config.is_scale_in_key();
                ctx.config.set_scale_in_key(is_chromatic);
                ctx.notify(if is_chromatic { "Chromatic" } else { "In Key" });
            }

            // Scale Base note selection
            _ => {
                if !(0..=15).contains(&n) {
                    return;
                }
                let pos = TRANSLATE[n as usize];
                if pos == -1 {
                    return;
                }
                ctx.model.scales_mut().set_scale_offset_by_index(pos);
                let base = Scales::BASES[pos as usize];
                ctx.config.set_scale_base(base);
                ctx.notify(base);
                // The shift view is the active view
                self.update_note_mapping(ctx);
            }
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down {
            return;
        }
        match button {
            ButtonId::Scene6 => {
                ctx.model.toggle_current_track_bank();
                let message = if ctx.model.is_effect_track_bank_active() { "Effect Tracks" } else { "Instrument/Audio Tracks" };
                ctx.notify(message);
            }
            ButtonId::Scene7 => ctx.model.cursor_device_mut().toggle_window_open(),
            ButtonId::Scene8 => ctx.model.current_track_bank_mut().stop(false),
            _ => {
                let index = self.definition.swap_shifted_scene_indices(scene_index(button).unwrap_or(0));
                apc::set_track_state(ctx, index);
                let soft_keys = config::SOFT_KEYS_OPTIONS[index];
                config::set_soft_keys(&mut ctx.config, soft_keys);
                ctx.notify(soft_keys);
            }
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let track_state = apc::track_state(ctx);
        // Draw the track states on the scene buttons
        let index = button.ordinal().wrapping_sub(ButtonId::Scene1.ordinal());
        match self.definition.swap_shifted_scene_indices(index) {
            0 => state_on_off(track_state == apc::TRACK_STATE_CLIP_STOP),
            1 => state_on_off(track_state == apc::TRACK_STATE_SOLO),
            2 => state_on_off(track_state == apc::TRACK_STATE_REC_ARM),
            3 => state_on_off(track_state == apc::TRACK_STATE_MUTE),
            4 => state_on_off(track_state == apc::TRACK_STATE_SELECT),
            5 => state_on_off(ctx.model.is_effect_track_bank_active()),
            _ => BUTTON_STATE_OFF,
        }
    }

    fn was_shift_view_used(&self) -> bool {
        self.shift.was_used()
    }

    impl_as_any!();
}

impl ApcMiniView for ShiftView {
    fn on_select_track(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match self.definition.swap_shifted_track_indices(index) {
            0 => ctx.model.current_track_bank_mut().scene_bank_mut().select_previous_page(),
            1 => ctx.model.current_track_bank_mut().scene_bank_mut().select_next_page(),
            2 => ctx.model.current_track_bank_mut().select_previous_page(),
            3 => ctx.model.current_track_bank_mut().select_next_page(),
            4 => Self::select_mode(ctx, Modes::Volume, "Volume"),
            5 => Self::select_mode(ctx, Modes::Pan, "Pan"),
            6 => {
                // Former Modes.get (modeManager.getActiveID (), 1): the next send mode
                let next = ctx.modes.active_id().and_then(|id| Modes::ALL.get(id.ordinal() + 1).copied());
                let mode = match next {
                    Some(mode) if is_send_mode(mode) => {
                        if ctx.model.current_track_bank().can_edit_send(mode.ordinal() - Modes::Send1.ordinal()) {
                            mode
                        } else {
                            Modes::Send1
                        }
                    }
                    _ => Modes::Send1,
                };
                let name = format!("Send {}", mode.ordinal() - Modes::Send1.ordinal() + 1);
                Self::select_mode(ctx, mode, &name);
            }
            7 => {
                if ctx.modes.is_active(Modes::DeviceParams) {
                    let device = ctx.model.cursor_device();
                    let (name, position) = (device.name().to_string(), device.position());
                    if let Some(browser) = ctx.model.browser_mut() {
                        browser.replace_device(&name, position);
                    }
                } else {
                    Self::select_mode(ctx, Modes::DeviceParams, "Device");
                }
            }
            _ => {}
        }
    }

    fn track_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let tb = ctx.model.current_track_bank();
        let scene_bank = tb.scene_bank();
        let mode = ctx.modes.active_id();
        match self.definition.swap_shifted_track_indices(index) {
            0 => on_off(scene_bank.can_scroll_page_backwards()),
            1 => on_off(scene_bank.can_scroll_page_forwards()),
            2 => on_off(tb.can_scroll_page_backwards()),
            3 => on_off(tb.can_scroll_page_forwards()),
            4 => on_off(mode == Some(Modes::Volume)),
            5 => on_off(mode == Some(Modes::Pan)),
            6 => on_off(mode.is_some_and(is_send_mode)),
            7 => on_off(mode == Some(Modes::DeviceParams)),
            // Never reached
            _ => mk1::APC_COLOR_BLACK,
        }
    }
}
