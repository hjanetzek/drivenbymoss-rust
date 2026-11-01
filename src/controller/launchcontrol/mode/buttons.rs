// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the 2nd button row (former `launchcontrol.mode.buttons`).
//!
//! The 'temporary' modes (former `XLTemporaryButtonMode`) store their 'has been used' flag in
//! the controller state (see `surface::has_been_used`), since the main modes need to access it.
//! The sequencer resolution (former `XLDrumSequencerMode.selectedResolutionIndex`) is stored
//! there as well.

use super::super::colors::*;
use super::super::surface::{self as xl, reset_has_been_used, set_has_been_used};
use super::CLIP_KEY;
use crate::daw::bank::BankItem;
use crate::daw::clip::{CursorClip, NotePosition, StepInfo, StepState};
use crate::daw::constants::Resolution;
use crate::daw::events::ParameterBankId;
use crate::framework::color_ids::abstract_sequencer_view as seq;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::mv_helper;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::commands::transport_ext::WindCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::layer::{self, LayerMuteMode, LayerSoloMode};
use crate::framework::mode::sequencer::SequencerModeBase;
use crate::framework::mode::track::{TrackMuteMode, TrackRecArmMode, TrackSoloMode};
use crate::framework::parameterprovider::parameter_bank;
use crate::midi::ButtonEvent;

/// The index of a button of the 2nd row (0-7) or `None`.
pub fn row2_index(button: ButtonId) -> Option<usize> {
    let ordinal = button.ordinal();
    let first = ButtonId::Row21.ordinal();
    (ordinal >= first && ordinal < first + 8).then(|| ordinal - first)
}

// ------------------------------------------------------------------------------------------
// Track and layer states

/// Which state of a track or layer is toggled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    Mute,
    Solo,
    RecArm,
}

/// Toggles mute, solo or rec arm of the tracks (former `XLTrackMuteMode`, `XLTrackSoloMode`,
/// `XLTrackRecArmMode`) or mute/solo of the layers (former `XLLayerMuteMode`,
/// `XLLayerSoloMode`).
pub struct XlChannelStateMode {
    mode: Box<dyn Mode>,
    state: ChannelState,
    is_layer: bool,
}

impl XlChannelStateMode {
    pub fn track(ctx: &Ctx, state: ChannelState) -> Self {
        let mode: Box<dyn Mode> = match state {
            ChannelState::Mute => Box::new(TrackMuteMode::new(ctx)),
            ChannelState::Solo => Box::new(TrackSoloMode::new(ctx)),
            ChannelState::RecArm => Box::new(TrackRecArmMode::new(ctx)),
        };
        XlChannelStateMode { mode, state, is_layer: false }
    }

    pub fn layer(ctx: &Ctx, state: ChannelState) -> Self {
        let mode: Box<dyn Mode> = match state {
            ChannelState::Solo => Box::new(LayerSoloMode::new(ctx)),
            _ => Box::new(LayerMuteMode::new(ctx)),
        };
        XlChannelStateMode { mode, state, is_layer: true }
    }
}

impl Mode for XlChannelStateMode {
    // TrackMuteMode, TrackSoloMode, TrackRecArmMode, LayerMuteMode, LayerSoloMode
    crate::forward_mode!(mode => name, on_activate, on_deactivate, on_button);

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let (exists, is_on) = if self.is_layer {
            match layer::layer(ctx, index) {
                Some(layer) => (layer.does_exist(), if self.state == ChannelState::Solo { layer.is_solo() } else { layer.is_mute() }),
                None => (false, false),
            }
        } else {
            let track = ctx.model.track_bank().item(index);
            let is_on = match self.state {
                ChannelState::Mute => track.is_mute(),
                ChannelState::Solo => track.is_solo(),
                ChannelState::RecArm => track.is_rec_arm(),
            };
            (track.does_exist(), is_on)
        };
        if !exists {
            return LAUNCHCONTROL_COLOR_BLACK;
        }
        match (self.state, is_on) {
            (ChannelState::Mute, true) => LAUNCHCONTROL_COLOR_AMBER,
            (ChannelState::Mute, false) => LAUNCHCONTROL_COLOR_AMBER_LO,
            (ChannelState::Solo, true) => LAUNCHCONTROL_COLOR_GREEN,
            (ChannelState::Solo, false) => LAUNCHCONTROL_COLOR_GREEN_LO,
            (ChannelState::RecArm, true) => LAUNCHCONTROL_COLOR_RED,
            (ChannelState::RecArm, false) => LAUNCHCONTROL_COLOR_RED_LO,
        }
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// Temporary button modes

/// Select a parameter page of a device (former `XLSelectDeviceParamsPageMode`).
pub struct XlSelectDeviceParamsPageMode {
    id: Modes,
    bank: ParameterBankId,
}

impl XlSelectDeviceParamsPageMode {
    /// `id` is the ID with which the mode is registered, `bank` the parameter bank of the device.
    pub fn new(id: Modes, bank: ParameterBankId) -> Self {
        XlSelectDeviceParamsPageMode { id, bank }
    }
}

impl Mode for XlSelectDeviceParamsPageMode {
    fn name(&self) -> &str {
        "Select Device Page"
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        reset_has_been_used(ctx, self.id);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down || row != 0 {
            return;
        }
        set_has_been_used(ctx, self.id);
        let count = parameter_bank(&ctx.model, self.bank).map_or(0, |b| b.page_bank().item_count());
        if index < count {
            if let Some(bank) = crate::framework::parameterprovider::parameter_bank_mut(&mut ctx.model, self.bank) {
                bank.select_page(index);
            }
            mv_helper::notify_selected_device_and_parameter_page(ctx);
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let Some(bank) = parameter_bank(&ctx.model, self.bank) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let page_bank = bank.page_bank();
        if index >= page_bank.item_count() {
            return LAUNCHCONTROL_COLOR_BLACK;
        }
        if index == page_bank.selected_item_index() {
            LAUNCHCONTROL_COLOR_YELLOW
        } else {
            LAUNCHCONTROL_COLOR_YELLOW_LO
        }
    }

    crate::impl_as_any!();
}

/// Transport functions (former `XLTransportMode`).
pub struct XlTransportMode {
    rwd_command: WindCommand,
    fwd_command: WindCommand,
    play_command: PlayCommand,
}

impl XlTransportMode {
    pub fn new() -> Self {
        XlTransportMode { rwd_command: WindCommand::new(false), fwd_command: WindCommand::new(true), play_command: PlayCommand::new() }
    }
}

impl Default for XlTransportMode {
    fn default() -> Self {
        Self::new()
    }
}

impl Mode for XlTransportMode {
    fn name(&self) -> &str {
        "Transport"
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        reset_has_been_used(ctx, Modes::Transport);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 {
            return;
        }
        set_has_been_used(ctx, Modes::Transport);
        let is_down = event == ButtonEvent::Down;
        match index {
            0 => self.play_command.execute_normal(ctx, event),
            1 if is_down => {
                ctx.model.transport_mut().start_recording();
            }
            2 => self.rwd_command.execute_normal(ctx, event),
            3 => self.fwd_command.execute_normal(ctx, event),
            4 if is_down => {
                ctx.model.transport_mut().toggle_loop();
            }
            5 if is_down => {
                ctx.model.transport_mut().toggle_metronome();
            }
            6 if is_down => {
                ctx.model.transport_mut().toggle_write_arranger_automation();
            }
            7 if is_down => {
                ctx.model.track_bank_mut().clear_mute();
                ctx.model.track_bank_mut().clear_solo();
            }
            _ => {}
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let transport = ctx.model.transport();
        let choose = |on: bool, hi: i32, lo: i32| if on { hi } else { lo };
        match index {
            0 => choose(transport.is_playing(), LAUNCHCONTROL_COLOR_GREEN, LAUNCHCONTROL_COLOR_GREEN_LO),
            1 => choose(transport.is_recording(), LAUNCHCONTROL_COLOR_RED, LAUNCHCONTROL_COLOR_RED_LO),
            2 | 3 => LAUNCHCONTROL_COLOR_AMBER_LO,
            4 => choose(transport.is_loop(), LAUNCHCONTROL_COLOR_YELLOW, LAUNCHCONTROL_COLOR_YELLOW_LO),
            5 => choose(transport.is_metronome_on(), LAUNCHCONTROL_COLOR_GREEN, LAUNCHCONTROL_COLOR_GREEN_LO),
            6 => choose(transport.is_writing_arranger_automation(), LAUNCHCONTROL_COLOR_RED, LAUNCHCONTROL_COLOR_RED_LO),
            7 => LAUNCHCONTROL_COLOR_AMBER_LO,
            _ => LAUNCHCONTROL_COLOR_BLACK,
        }
    }

    crate::impl_as_any!();
}

/// Select and launch scenes (former `XLScenesMode`).
#[derive(Default)]
pub struct XlScenesMode;

impl Mode for XlScenesMode {
    fn name(&self) -> &str {
        "Select Scene"
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        reset_has_been_used(ctx, Modes::Clip);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 || event == ButtonEvent::Long {
            return;
        }
        if !ctx.model.scene_bank().item(index).does_exist() {
            return;
        }
        let is_down = event == ButtonEvent::Down;
        if is_down {
            set_has_been_used(ctx, Modes::Clip);
            ctx.model.scene_bank_mut().item_mut(index).select();
        }
        ctx.model.scene_bank_mut().item_mut(index).launch(is_down, false);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let scene = ctx.model.scene_bank().item(index);
        if !scene.does_exist() {
            return LAUNCHCONTROL_COLOR_BLACK;
        }
        if scene.is_selected() {
            LAUNCHCONTROL_COLOR_YELLOW
        } else {
            LAUNCHCONTROL_COLOR_YELLOW_LO
        }
    }

    crate::impl_as_any!();
}

/// Select the step resolution of the sequencer (former `XLSequencerResolutionMode`).
#[derive(Default)]
pub struct XlSequencerResolutionMode;

impl Mode for XlSequencerResolutionMode {
    fn name(&self) -> &str {
        "Step Resolution"
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        reset_has_been_used(ctx, Modes::Configuration);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 || event != ButtonEvent::Down {
            return;
        }
        ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1).set_step_length(Resolution::value_at(index));
        xl::xl_mut(ctx).drum_resolution_index = index;
        ctx.host.show_notification(&format!("Step Resolution: {}", Resolution::name_at(index)));
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        if index == xl::xl(ctx).drum_resolution_index {
            LAUNCHCONTROL_COLOR_GREEN
        } else {
            LAUNCHCONTROL_COLOR_GREEN_LO
        }
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// Sequencer modes

const SEQUENCER_STEPS: usize = 8;

/// Former `isInXRange`.
fn is_in_x_range(clip: &CursorClip, x: i32) -> bool {
    let step_size = clip.num_steps() as i32;
    let start = clip.edit_page() * step_size;
    x >= start && x < start + step_size
}

/// Former `XLDrumSequencerMode.getStepColor` (without DAW colors).
fn step_color(step_info: &StepInfo, hilite: bool, step: usize) -> &'static str {
    match step_info.state() {
        // Note starts
        StepState::Start => {
            if hilite {
                seq::COLOR_STEP_HILITE_CONTENT
            } else if step_info.is_muted() {
                seq::COLOR_STEP_MUTED
            } else {
                seq::COLOR_CONTENT
            }
        }
        // Note continues
        StepState::Continue => {
            if hilite {
                seq::COLOR_STEP_HILITE_CONTENT
            } else if step_info.is_muted() {
                seq::COLOR_STEP_MUTED_CONT
            } else {
                seq::COLOR_CONTENT_CONT
            }
        }
        // Empty
        _ => {
            if hilite {
                seq::COLOR_STEP_HILITE_NO_CONTENT
            } else if step / 4 % 2 == 1 {
                seq::COLOR_NO_CONTENT_4
            } else {
                seq::COLOR_NO_CONTENT
            }
        }
    }
}

/// Toggle the steps of the selected drum pad (former `XLDrumSequencerMode`).
pub struct XlDrumSequencerMode {
    pub base: SequencerModeBase,
}

impl XlDrumSequencerMode {
    pub fn new(ctx: &mut Ctx) -> Self {
        XlDrumSequencerMode { base: SequencerModeBase::new("Note Sequencer", ctx, true, CLIP_KEY.1, CLIP_KEY.0, false) }
    }

    fn note_row(ctx: &Ctx) -> i32 {
        ctx.model.scales().drum_offset() + xl::xl(ctx).selected_pad as i32
    }

    fn is_active(ctx: &Ctx, clip: &CursorClip) -> bool {
        ctx.model.can_selected_track_hold_notes() && clip.does_exist()
    }

    /// Former `getSequencerStepColor`.
    fn sequencer_step_color(ctx: &Ctx, clip: &CursorClip, note_row: i32, column: usize) -> i32 {
        let step = clip.current_step();
        let hi_step = if is_in_x_range(clip, step) { step % SEQUENCER_STEPS as i32 } else { -1 };
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), column as i32, note_row);
        let step_info = clip.step(&note_position);
        let hilite = column as i32 == hi_step;
        let color_id = if Self::is_active(ctx, clip) { step_color(step_info, hilite, column) } else { seq::COLOR_NO_CONTENT };
        ctx.colors.color_index(color_id)
    }
}

impl Mode for XlDrumSequencerMode {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.selected_resolution_index = xl::xl(ctx).drum_resolution_index;
        self.base.on_activate(ctx);
    }

    fn on_button(&mut self, ctx: &mut Ctx, _row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let note_row = Self::note_row(ctx);
        let note_position = NotePosition::new(ctx.config.midi_edit_channel(), index as i32, note_row);
        self.base.clip(ctx).toggle_step(&note_position, 127);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let Some(clip) = self.base.clip_ref(ctx) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        Self::sequencer_step_color(ctx, clip, Self::note_row(ctx), index)
    }

    crate::impl_as_any!();
}

/// Mute/unmute the highest note of the steps (former `XLNoteSequencerMode`).
pub struct XlNoteSequencerMode {
    pub base: SequencerModeBase,
}

impl XlNoteSequencerMode {
    pub fn new(ctx: &mut Ctx) -> Self {
        XlNoteSequencerMode { base: SequencerModeBase::new("Note Sequencer", ctx, true, CLIP_KEY.1, CLIP_KEY.0, false) }
    }

    /// Former `getSequencerStepColor`.
    fn sequencer_step_color(ctx: &Ctx, clip: &CursorClip, column: usize) -> i32 {
        let step = clip.current_step();
        let hi_step = if is_in_x_range(clip, step) { step % SEQUENCER_STEPS as i32 } else { -1 };
        let hilite = column as i32 == hi_step;
        let channel = ctx.config.midi_edit_channel();
        let highest_row = clip.highest_row(channel, column as i32);
        let step_info = clip.step(&NotePosition::new(channel, column as i32, highest_row));
        let color_id = if step_info.state() == StepState::Start && step_info.is_muted() {
            if hilite {
                seq::COLOR_STEP_HILITE_CONTENT
            } else {
                seq::COLOR_STEP_MUTED
            }
        } else if hilite {
            seq::COLOR_STEP_HILITE_NO_CONTENT
        } else {
            seq::COLOR_NO_CONTENT
        };
        ctx.colors.color_index(color_id)
    }
}

impl Mode for XlNoteSequencerMode {
    fn name(&self) -> &str {
        &self.base.name
    }

    // AbstractSequencerMode
    crate::forward_mode!(base => on_activate);

    fn on_button(&mut self, ctx: &mut Ctx, _row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        let channel = ctx.config.midi_edit_channel();
        let clip = self.base.clip(ctx);
        let highest_row = clip.highest_row(channel, index as i32);
        let note_position = NotePosition::new(channel, index as i32, highest_row);
        let step = clip.step(&note_position);
        if step.state() == StepState::Start {
            let is_muted = step.is_muted();
            clip.update_step_mute_state(&note_position, !is_muted);
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let Some(clip) = self.base.clip_ref(ctx) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        Self::sequencer_step_color(ctx, clip, index)
    }

    crate::impl_as_any!();
}

/// Set the loop length of the clip (former `XLLoopLengthMode`).
#[derive(Default)]
pub struct XlLoopLengthMode;

impl Mode for XlLoopLengthMode {
    fn name(&self) -> &str {
        "Set loop length"
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 || event != ButtonEvent::Down {
            return;
        }
        let resolution = Resolution::value_at(xl::xl(ctx).drum_resolution_index);
        let clip = ctx.model.note_clip(CLIP_KEY.0, CLIP_KEY.1);
        let steps = clip.edit_page() * SEQUENCER_STEPS as i32 + index as i32 + 1;
        clip.set_loop_length(steps as f64 * resolution);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let Some(index) = row2_index(button) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let Some(clip) = ctx.model.note_clip_ref(CLIP_KEY.0, CLIP_KEY.1) else {
            return LAUNCHCONTROL_COLOR_BLACK;
        };
        let resolution = Resolution::value_at(xl::xl(ctx).drum_resolution_index);
        let mut number_of_active_steps = (clip.loop_length() / resolution).floor() as i32;
        number_of_active_steps -= clip.edit_page() * SEQUENCER_STEPS as i32;
        let index = index as i32;
        if index >= number_of_active_steps {
            return LAUNCHCONTROL_COLOR_BLACK;
        }
        if index == number_of_active_steps - 1 {
            LAUNCHCONTROL_COLOR_AMBER
        } else {
            LAUNCHCONTROL_COLOR_AMBER_LO
        }
    }

    crate::impl_as_any!();
}
