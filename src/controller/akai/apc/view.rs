// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The views of the APC40 (former `controller.akai.apc.view` and the `TempoView` of the
//! framework): play, session, sequencer, drum, raindrops, shift and tempo view.

use super::colors::*;
use super::command::start_browser;
use super::mode::ApcMode;
use crate::daw::bank::BankItem;
use crate::daw::clip::{NotePosition, StepState};
use crate::daw::color::DawColor;
use crate::daw::constants::{ChannelType, Resolution};
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::color_ids::abstract_session_view::{COLOR_SCENE, COLOR_SCENE_OFF, COLOR_SELECTED_SCENE};
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::grid::LightInfo;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::view::base::{is_scene_button, scene_index};
use crate::framework::view::number_display_view::{tempo_number, NumberDisplayViewBase};
use crate::framework::view::play_view::{play_scale_matrix, PlayViewBase};
use crate::framework::view::sequencer::abstract_drum_view::{DrumBase, DrumHooks};
use crate::framework::view::sequencer::abstract_note_sequencer_view::{NoteSequencerBase, NoteSequencerHooks};
use crate::framework::view::sequencer::abstract_raindrops_view::RaindropsBase;
use crate::framework::view::sequencer::abstract_sequencer_view::SequencerBase;
use crate::framework::view::session_view::{SessionColors, SessionHooks, SessionViewBase};
use crate::framework::view::shift_view::ShiftViewBase;
use crate::framework::view::transpose_view::TransposeView;
use crate::impl_as_any;
use crate::midi::{ButtonEvent, CMD_NOTE_ON};
use crate::scale::Scales;

/// Mark the note of a sequencer view as edited (former `AbstractSequencerView.setNoteEdited`).
pub fn set_note_edited(view: &mut dyn View) {
    let any = view.as_any_mut();
    if let Some(v) = any.downcast_mut::<ApcSequencerView>() {
        v.base.seq.set_note_edited();
    } else if let Some(v) = any.downcast_mut::<ApcDrumView>() {
        v.base.seq.set_note_edited();
    } else if let Some(v) = any.downcast_mut::<ApcRaindropsView>() {
        v.base.seq.set_note_edited();
    }
}

/// The resolution index of the view if it is a sequencer (former `instanceof
/// AbstractSequencerView` + `getResolutionIndex`).
pub fn as_sequencer_resolution(view: &dyn View, ctx: &Ctx) -> Option<usize> {
    view.sequencer_resolution_index(ctx)
}

fn on_off(is_on: bool) -> &'static str {
    if is_on {
        BUTTON_STATE_ON
    } else {
        BUTTON_STATE_OFF
    }
}

/// Former `notifyScale` of the sequencer views.
fn notify_scale(ctx: &mut Ctx) {
    let name = ctx.model.scales().scale().name().to_string();
    ctx.config.set_scale(&name);
    ctx.notify(&name);
}

/// Toggle chromatic (scene 3 of the play and sequencer views).
fn toggle_chromatic(ctx: &mut Ctx) {
    ctx.model.scales_mut().toggle_chromatic();
    let is_chromatic = ctx.model.scales().is_chromatic();
    ctx.config.set_scale_in_key(!is_chromatic);
    ctx.notify(if is_chromatic { "Chromatic" } else { "In Key" });
}

/// Turn the note mode on if an existing note is pressed, off on release (part of the former
/// `handleSequencerArea` of the sequencer and drum views). Returns true if the release was
/// consumed because the note was edited.
fn handle_note_mode(seq: &mut SequencerBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
    if velocity > 0 {
        let clip_key = seq.clip_key();
        if seq.clip(ctx).step(note_position).state() == StepState::Start {
            if let Some(mode) = ctx.modes.get(Modes::Note) {
                if let Ok(mut mode) = mode.try_borrow_mut() {
                    if let Some(note_mode) = mode.as_any_mut().downcast_mut::<ApcMode>() {
                        note_mode.set_values(clip_key, note_position);
                    }
                }
            }
            ctx.set_active_mode(Modes::Note);
        }
        return false;
    }
    // Turn off Note mode
    if ctx.modes.is_active(Modes::Note) {
        ctx.restore_mode();
    }
    if seq.is_note_edited {
        seq.is_note_edited = false;
        return true;
    }
    false
}

/// The up and down arrows in combination with a step change the note repeat (former override of
/// `handleSequencerAreaButtonCombinations`).
fn handle_repeat_combination(seq: &SequencerBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
    let is_up_pressed = ctx.surface.is_pressed(ButtonId::ArrowUp);
    if is_up_pressed || ctx.surface.is_pressed(ButtonId::ArrowDown) {
        ctx.surface.set_trigger_consumed(if is_up_pressed { ButtonId::ArrowUp } else { ButtonId::ArrowDown });
        if velocity > 0 {
            seq.handle_sequencer_area_repeat_operator(ctx, note_position, velocity, is_up_pressed);
        }
        return true;
    }
    false
}

// ------------------------------------------------------------------------------------------
// PlayView

/// The play view (former `PlayView`).
pub struct ApcPlayView {
    play: PlayViewBase,
    is_mk2: bool,
}

impl ApcPlayView {
    pub fn new(is_mk2: bool) -> Self {
        ApcPlayView { play: PlayViewBase::new("Play", is_mk2), is_mk2 }
    }
}

impl View for ApcPlayView {
    fn name(&self) -> &str {
        &self.play.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.play.on_activate(ctx, play_scale_matrix(ctx));
    }

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        match scene_index(button) {
            Some(index) => on_off(index != 2),
            None => BUTTON_STATE_OFF,
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down {
            return;
        }
        match button {
            ButtonId::Scene1 | ButtonId::Scene2 => {
                if button == ButtonId::Scene1 {
                    ctx.model.scales_mut().next_scale();
                } else {
                    ctx.model.scales_mut().prev_scale();
                }
                // Former updateScale
                self.update_note_mapping(ctx);
                crate::framework::setup_helpers::update_scale_properties(ctx);
                let message = format!("Scale: {}", ctx.model.scales().scale().name());
                ctx.notify(&message);
            }
            ButtonId::Scene3 => toggle_chromatic(ctx),
            ButtonId::Scene4 => self.on_octave_up(ctx, event),
            ButtonId::Scene5 => self.on_octave_down(ctx, event),
            _ => {}
        }
        self.update_note_mapping(ctx);
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.play.draw_grid(ctx);
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if !ctx.model.can_selected_track_hold_notes() {
            return;
        }
        self.play.on_grid_note(ctx, note, velocity);
        // Former playNote: the pads of the Mk1 do not send notes, which could be translated
        let mapped = self.play.base.key_manager.map(note);
        if !self.is_mk2 && mapped >= 0 {
            ctx.surface.send_midi_event(CMD_NOTE_ON, mapped, velocity);
        }
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

impl TransposeView for ApcPlayView {
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
// SessionView

/// The overridden functions of the APC40 session view.
struct ApcSessionHooks;

impl SessionHooks for ApcSessionHooks {
    fn is_scene_select_action(&self, _ctx: &Ctx) -> bool {
        // Sadly, no additional button available
        false
    }
}

/// The session view (former `SessionView`).
pub struct ApcSessionView {
    name: String,
    pub session: SessionViewBase,
}

impl ApcSessionView {
    pub fn new(is_mk2: bool) -> Self {
        let mut session = SessionViewBase::new(5, 8, is_mk2);
        session.clip_colors = if is_mk2 {
            SessionColors {
                is_recording: LightInfo::new(APC_MKII_COLOR_RED_HI, APC_MKII_COLOR_RED_HI, false),
                is_recording_queued: LightInfo::new(APC_MKII_COLOR_RED_HI, APC_MKII_COLOR_RED_HI, true),
                is_playing: LightInfo::new(APC_MKII_COLOR_GREEN_HI, APC_MKII_COLOR_GREEN_HI, false),
                is_playing_queued: LightInfo::new(APC_MKII_COLOR_GREEN_HI, APC_MKII_COLOR_GREEN_HI, true),
                is_stop_queued: LightInfo::new(APC_MKII_COLOR_GREEN_HI, APC_MKII_COLOR_GREEN_HI, true),
                has_content: LightInfo::new(APC_MKII_COLOR_AMBER, APC_MKII_COLOR_WHITE, false),
                has_no_content: LightInfo::new(APC_MKII_COLOR_BLACK, -1, false),
                is_rec_armed: LightInfo::new(APC_MKII_COLOR_RED_LO, -1, false),
                is_muted: LightInfo::new(APC_MKII_COLOR_GREY_LO, -1, false),
            }
        } else {
            SessionColors {
                is_recording: LightInfo::new(APC_COLOR_RED, -1, false),
                is_recording_queued: LightInfo::new(APC_COLOR_RED, APC_COLOR_RED_BLINK, false),
                is_playing: LightInfo::new(APC_COLOR_GREEN, -1, false),
                is_playing_queued: LightInfo::new(APC_COLOR_GREEN, APC_COLOR_GREEN_BLINK, false),
                is_stop_queued: LightInfo::new(APC_COLOR_GREEN, APC_COLOR_GREEN_BLINK, false),
                has_content: LightInfo::new(APC_COLOR_YELLOW, APC_COLOR_YELLOW_BLINK, false),
                has_no_content: LightInfo::new(APC_COLOR_BLACK, -1, false),
                is_rec_armed: LightInfo::new(APC_COLOR_BLACK, -1, false),
                is_muted: LightInfo::new(APC_COLOR_BLACK, -1, false),
            }
        };
        ApcSessionView { name: "Session".to_string(), session }
    }

    /// Former `toggleBirdsEyeView`.
    pub fn toggle_birds_eye_view(&mut self) {
        self.session.toggle_birds_eye_view();
    }

    /// Former override of `handleButtonCombinations`: Select + pad duplicates the slot, Stop +
    /// pad deletes it.
    fn handle_button_combinations(&mut self, ctx: &mut Ctx, track_index: usize, slot_index: usize) -> bool {
        if self.session.handle_button_combinations(ctx, &ApcSessionHooks, track_index, slot_index) {
            return true;
        }
        let index = ctx.model.current_track_bank().item(track_index).index();
        if index < 0 {
            return true;
        }
        // Duplicate the slot with Select button
        if ctx.is_button_combination(ButtonId::get(ButtonId::Row11, index as usize)) {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).duplicate();
            return true;
        }
        // Delete the slot with Stop Clip button
        if ctx.is_button_combination(ButtonId::get(ButtonId::Row61, index as usize)) {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).remove();
            return true;
        }
        false
    }
}

impl View for ApcSessionView {
    fn name(&self) -> &str {
        &self.name
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let scene = button.ordinal() as i32 - ButtonId::Scene1.ordinal() as i32;
        if !(0..8).contains(&scene) {
            return BUTTON_COLOR_OFF;
        }
        let s = ctx.model.scene_bank().item(scene as usize);
        if !s.does_exist() {
            return COLOR_SCENE_OFF;
        }
        if s.is_selected() {
            return COLOR_SELECTED_SCENE;
        }
        if self.session.use_clip_color {
            DawColor::color_id(s.color())
        } else {
            COLOR_SCENE
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.session.draw_grid(ctx, &ApcSessionHooks, false);
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

        // Former AbstractSessionView.onGridNote with the extended handleButtonCombinations
        let hooks = ApcSessionHooks;
        if hooks.is_alternate_function(ctx) {
            self.session.set_alternate_interaction_used(true);
        }
        let (track_index, slot_index) = hooks.pad(ctx, &self.session, note);
        let is_pressed = velocity != 0;
        if is_pressed {
            if self.handle_button_combinations(ctx, track_index, slot_index) {
                ctx.surface.consume_pads();
                return;
            }
            if hooks.do_select_clip_on_launch(ctx) {
                ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).select();
            }
        }
        let is_alternate_function = hooks.is_alternate_function(ctx);
        let track = ctx.model.current_track_bank().item(track_index);
        if !track.is_rec_arm() || track.slot_bank().item(slot_index).has_content() {
            ctx.model.current_track_bank_mut().item_mut(track_index).slot_bank_mut().item_mut(slot_index).launch(is_pressed, is_alternate_function);
            return;
        }
        SessionViewBase::handle_recording(ctx, track_index, slot_index);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if is_scene_button(button) {
            SessionViewBase::on_scene_button(ctx, &ApcSessionHooks, button, event);
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

impl crate::daw::clip::SessionAlternative for ApcSessionView {
    fn was_alternate_interaction_used(&self) -> bool {
        self.session.was_alternate_interaction_used()
    }

    fn set_alternate_interaction_used(&mut self, was_used: bool) {
        self.session.set_alternate_interaction_used(was_used);
    }
}

// ------------------------------------------------------------------------------------------
// SequencerView

/// The APC specific behaviour of the note sequencer.
struct SequencerHooks;

impl NoteSequencerHooks for SequencerHooks {
    fn handle_sequencer_area(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, index: i32, x: i32, y: i32, velocity: i32) {
        if !base.seq.is_active() {
            return;
        }
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), x, base.seq.view.key_manager.map(y));
        if handle_note_mode(&mut base.seq, ctx, &note_position, velocity) {
            return;
        }
        base.handle_sequencer_area(self, ctx, index, x, y, velocity);
    }

    fn handle_sequencer_area_button_combinations(&mut self, base: &mut NoteSequencerBase, ctx: &mut Ctx, note_position: &NotePosition, row: i32, velocity: i32) -> bool {
        handle_repeat_combination(&base.seq, ctx, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, row, velocity)
    }
}

/// The note sequencer view (former `SequencerView`): 4 sequencer rows and the clip pages.
pub struct ApcSequencerView {
    base: NoteSequencerBase,
}

impl ApcSequencerView {
    pub fn new(is_mk2: bool) -> Self {
        let mut base = NoteSequencerBase::with_size("Sequencer", 8, 4, is_mk2);
        base.num_display_rows = 5;
        ApcSequencerView { base }
    }
}

impl View for ApcSequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, update_control_surface, on_sequencer_left,
        on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractNoteSequencerView (the overridden updateNoteMapping calls updateScale again,
    // which the base already does)
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        if button == ButtonId::Scene3 {
            return BUTTON_STATE_OFF;
        }
        on_off(self.base.seq.is_active())
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut SequencerHooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        match button {
            ButtonId::Scene1 => {
                ctx.model.scales_mut().next_scale();
                notify_scale(ctx);
            }
            ButtonId::Scene2 => {
                ctx.model.scales_mut().prev_scale();
                notify_scale(ctx);
            }
            ButtonId::Scene3 => toggle_chromatic(ctx),
            ButtonId::Scene4 => self.base.on_octave_up(ctx, event),
            ButtonId::Scene5 => self.base.on_octave_down(ctx, event),
            _ => {}
        }
        self.update_note_mapping(ctx);
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for ApcSequencerView {
    // AbstractNoteSequencerView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

// ------------------------------------------------------------------------------------------
// DrumView

/// The APC specific behaviour of the drum sequencer.
struct DrumViewHooks {
    is_mk2: bool,
}

impl DrumHooks for DrumViewHooks {
    fn handle_sequencer_area(&mut self, base: &mut DrumBase, ctx: &mut Ctx, index: i32, x: i32, y: i32, offset_y: i32, velocity: i32) {
        if !base.seq.is_active() {
            return;
        }
        let step = base.num_columns * (base.all_rows - 1 - y) + x;
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), step, offset_y + base.selected_pad());
        if handle_note_mode(&mut base.seq, ctx, &note_position, velocity) {
            return;
        }
        base.handle_sequencer_area(self, ctx, index, x, y, offset_y, velocity);
    }

    fn handle_sequencer_area_button_combinations(&mut self, base: &mut DrumBase, ctx: &mut Ctx, note_position: &NotePosition, velocity: i32) -> bool {
        handle_repeat_combination(&base.seq, ctx, note_position, velocity) || base.handle_sequencer_area_button_combinations(ctx, note_position, velocity)
    }

    fn play_note(&mut self, _base: &mut DrumBase, ctx: &mut Ctx, drum_pad: i32, velocity: i32) {
        // The pads of the Mk1 do not send notes, which could be translated
        if !self.is_mk2 && (0..128).contains(&drum_pad) {
            ctx.surface.send_midi_event(CMD_NOTE_ON, drum_pad, velocity);
        }
    }
}

/// The drum sequencer view (former `DrumView`, an `AbstractDrumExView` with 2 sequencer and 3
/// play rows).
pub struct ApcDrumView {
    base: DrumBase,
    hooks: DrumViewHooks,
}

impl ApcDrumView {
    pub fn new(is_mk2: bool) -> Self {
        let mut base = DrumBase::new("Drum", 2, 3, is_mk2).with_extra_buttons();
        if let Some(ex) = &mut base.ex {
            ex.use_extra_toggle_button = false;
        }
        base.button_select = ButtonId::Pad13;
        base.button_mute = ButtonId::Pad14;
        base.button_solo = ButtonId::Pad15;
        base.button_browse = ButtonId::Pad16;
        ApcDrumView { base, hooks: DrumViewHooks { is_mk2 } }
    }

    fn ex_flags(&self) -> (bool, bool, bool) {
        self.base.ex.as_ref().map(|ex| (ex.extra_buttons_on, ex.note_repeat_period_on, ex.note_repeat_length_on)).unwrap_or_default()
    }

    /// Former `handleStopButtons`.
    pub fn handle_stop_buttons(&mut self, ctx: &mut Ctx, index: usize) {
        let (_, period_on, length_on) = self.ex_flags();
        let index = index.min(Resolution::ALL.len() - 1);
        if period_on {
            ctx.config.set_note_repeat_period(Resolution::ALL[index]);
            mv_helper::delay_display(ctx, move |_| format!("Period: {}", Resolution::name_at(index)));
            return;
        }
        if length_on {
            ctx.config.set_note_repeat_length(Resolution::ALL[index]);
            mv_helper::delay_display(ctx, move |_| format!("Note Length: {}", Resolution::name_at(index)));
            return;
        }
        self.base.seq.set_resolution_index(ctx, index as i32);
    }

    /// Former `getStopButtonColor`.
    pub fn stop_button_color(&self, ctx: &Ctx, index: usize) -> i32 {
        let (_, period_on, length_on) = self.ex_flags();
        if period_on {
            return i32::from(ctx.config.note_repeat_period().ordinal() == index);
        }
        if length_on {
            return i32::from(ctx.config.note_repeat_length().ordinal() == index);
        }
        i32::from(self.base.seq.resolution_index(ctx) == index)
    }
}

impl View for ApcDrumView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, on_sequencer_left, on_sequencer_right,
        sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractDrumView
    crate::forward_view!(base => on_activate, on_deactivate, draw_grid, update_control_surface,
        update_note_mapping, on_model_event);

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        if button == ButtonId::Scene3 {
            return if self.ex_flags().0 { BUTTON_STATE_HI } else { BUTTON_STATE_OFF };
        }
        on_off(self.base.seq.is_active())
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut self.hooks, ctx, note, velocity);
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        match button {
            ButtonId::Scene1 => self.base.change_octave(ctx, event, true, 4, false, true),
            ButtonId::Scene2 => self.base.change_octave(ctx, event, false, 4, false, true),
            ButtonId::Scene3 => self.base.toggle_extra_buttons(),
            ButtonId::Scene4 => self.base.on_octave_up(ctx, event),
            ButtonId::Scene5 => self.base.on_octave_down(ctx, event),
            _ => {}
        }
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for ApcDrumView {
    // AbstractDrumView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

// ------------------------------------------------------------------------------------------
// RaindropsView

/// The raindrops sequencer view (former `RaindropsView`, 5 rows).
pub struct ApcRaindropsView {
    base: RaindropsBase,
}

impl ApcRaindropsView {
    pub fn new(is_mk2: bool) -> Self {
        let mut base = RaindropsBase::new("Raindrops", is_mk2);
        base.num_display_rows = 5;
        ApcRaindropsView { base }
    }
}

impl View for ApcRaindropsView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, update_control_surface, on_sequencer_left,
        on_sequencer_right, sequencer_resolution_index, set_sequencer_resolution_index);
    // AbstractRaindropsView
    crate::forward_view!(base => on_activate, draw_grid, on_grid_note, update_note_mapping);

    fn button_color_id(&self, _ctx: &Ctx, button: ButtonId) -> &'static str {
        if button == ButtonId::Scene3 {
            return BUTTON_STATE_OFF;
        }
        on_off(self.base.seq.is_active())
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if !is_scene_button(button) || event != ButtonEvent::Down || !self.base.seq.is_active() {
            return;
        }
        self.base.ongoing_resolution_change = true;
        match button {
            ButtonId::Scene1 => {
                ctx.model.scales_mut().next_scale();
                notify_scale(ctx);
            }
            ButtonId::Scene2 => {
                ctx.model.scales_mut().prev_scale();
                notify_scale(ctx);
            }
            ButtonId::Scene3 => toggle_chromatic(ctx),
            ButtonId::Scene4 => self.base.on_octave_up(ctx, event),
            ButtonId::Scene5 => self.base.on_octave_down(ctx, event),
            _ => {}
        }
        self.update_note_mapping(ctx);
        self.base.ongoing_resolution_change = false;
    }

    crate::impl_transpose_view!();

    impl_as_any!();
}

impl TransposeView for ApcRaindropsView {
    // AbstractRaindropsView
    crate::forward_transpose_view!(base => on_octave_down, on_octave_up, is_octave_up_button_on,
        is_octave_down_button_on, reset_octave);
}

// ------------------------------------------------------------------------------------------
// ShiftView

/// The views of the scene buttons (former `VIEW_IDS`).
const VIEW_IDS: [Views; 5] = [Views::Session, Views::Play, Views::Drum, Views::Sequencer, Views::Raindrops];

/// The positions of the base notes on the first two pad rows (former `TRANSLATE`).
const TRANSLATE: [i32; 16] = [0, 2, 4, 6, 1, 3, 5, -1, -1, 10, 8, -1, 11, 9, 7, -1];

/// The shift view (former `ShiftView`).
pub struct ApcShiftView {
    shift: ShiftViewBase,
}

impl ApcShiftView {
    pub fn new() -> Self {
        ApcShiftView { shift: ShiftViewBase::new("Shift") }
    }
}

impl Default for ApcShiftView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ApcShiftView {
    fn name(&self) -> &str {
        &self.shift.base.name
    }

    fn on_activate(&mut self, _ctx: &mut Ctx) {
        self.shift.on_activate();
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let scale_offset = ctx.model.scales().scale_offset_index();
        let Ctx { surface, colors, .. } = ctx;
        let grid = surface.grid();
        for i in 7..64 {
            grid.light(36 + i, APC_COLOR_BLACK);
        }
        // Add tracks
        for i in 0..3 {
            grid.light_id(colors, 36 + 32 + i, COLOR_KEY_WHITE);
        }
        grid.light_id(colors, 36 + 35, COLOR_KEY_SELECTED);
        grid.light_id(colors, 36 + 36, COLOR_KEY_SELECTED);
        grid.light_id(colors, 36 + 38, COLOR_KEY_BLACK);
        grid.light_id(colors, 36 + 39, COLOR_KEY_BLACK);

        // Draw the keyboard
        // 0'C', 1'G', 2'D', 3'A', 4'E', 5'B', 6'F', 7'Bb', 8'Eb', 9'Ab', 10'Db', 11'Gb'
        let keys = [
            (0, 0, COLOR_KEY_WHITE),
            (1, 2, COLOR_KEY_WHITE),
            (2, 4, COLOR_KEY_WHITE),
            (3, 6, COLOR_KEY_WHITE),
            (4, 1, COLOR_KEY_WHITE),
            (5, 3, COLOR_KEY_WHITE),
            (6, 5, COLOR_KEY_WHITE),
            (9, 10, COLOR_KEY_BLACK),
            (10, 8, COLOR_KEY_BLACK),
            (12, 11, COLOR_KEY_BLACK),
            (13, 9, COLOR_KEY_BLACK),
            (14, 7, COLOR_KEY_BLACK),
        ];
        for (pad, offset, color) in keys {
            grid.light_id(colors, 36 + pad, if scale_offset == offset { COLOR_KEY_SELECTED } else { color });
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        self.shift.set_was_used();
        let index = note - 36;
        if index > 15 {
            match index {
                32 => ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]),
                33 => ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]),
                34 => ctx.model.application_mut().add_effect_track(),
                35 | 36 => start_browser(ctx, true, index == 35),
                38 => ctx.model.application_mut().undo(),
                39 => ctx.model.application_mut().redo(),
                _ => {}
            }
            return;
        }
        // Scale Base note selection
        let Some(&pos) = TRANSLATE.get(index.max(0) as usize) else {
            return;
        };
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

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let Some(index) = scene_index(button) else {
            return;
        };
        if event != ButtonEvent::Down || index >= VIEW_IDS.len() {
            return;
        }
        let view_id = VIEW_IDS[index];
        // The shift view is borrowed while it handles the button, the view is switched after the
        // event
        ctx.schedule(0, move |ctx| {
            if ctx.views.active_id_ignore_temporary() == Some(Views::Session) && view_id == Views::Session {
                if let Some(view) = ctx.views.get(Views::Session) {
                    if let Ok(mut view) = view.try_borrow_mut() {
                        if let Some(session) = view.as_any_mut().downcast_mut::<ApcSessionView>() {
                            session.toggle_birds_eye_view();
                        }
                    }
                }
            } else {
                ctx.set_active_view(view_id);
            }
            let name = ctx.views.get(view_id).and_then(|view| view.try_borrow().ok().map(|v| v.name().to_string()));
            if let Some(name) = name {
                ctx.notify(&name);
            }
            if view_id == Views::Session {
                return;
            }
            let cursor_track = ctx.model.cursor_track();
            if cursor_track.does_exist() {
                let position = cursor_track.position();
                ctx.views.set_preferred(position, view_id);
            }
        });
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let previous_view_id = ctx.views.active_id_ignore_temporary();
        let selected = |view: Views| if previous_view_id == Some(view) { COLOR_VIEW_SELECTED } else { COLOR_VIEW_UNSELECTED };
        match button {
            ButtonId::Scene1 => selected(Views::Session),
            ButtonId::Scene2 => selected(Views::Play),
            ButtonId::Scene3 => selected(Views::Drum),
            ButtonId::Scene4 => selected(Views::Sequencer),
            ButtonId::Scene5 => selected(Views::Raindrops),
            _ => BUTTON_STATE_OFF,
        }
    }

    fn was_shift_view_used(&self) -> bool {
        self.shift.was_used()
    }

    impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// TempoView

/// Displays the tempo (former `TempoView` of the framework).
pub struct ApcTempoView {
    display: NumberDisplayViewBase,
}

impl ApcTempoView {
    pub fn new(text_color1: i32, text_color2: i32, background_color: i32) -> Self {
        ApcTempoView { display: NumberDisplayViewBase::new("Tempo", text_color1, text_color2, background_color) }
    }
}

impl View for ApcTempoView {
    fn name(&self) -> &str {
        &self.display.base.name
    }

    // AbstractNumberDisplayView
    crate::forward_view!(display => on_activate, on_grid_note);

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        self.display.draw_number(ctx, tempo_number(ctx));
        self.display.fill_bottom(ctx);
    }

    impl_as_any!();
}
