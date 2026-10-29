// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the main encoder (former `EncoderMode`, `EncoderModeManager` and
//! `MaschineMonitorEncoderCommand`).
//!
//! Differences to Java: the Java manager binds a parameter to the hardware encoder. Here the
//! manager is the continuous command of the encoder and stores which parameter is bound
//! ([`EncoderTarget`]), the parameter is looked up when the encoder is turned (the selected
//! track is the one which is selected at that time).

use std::cell::RefCell;
use std::rc::Rc;

use crate::daw::constants::GrooveParameterId;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::ButtonEvent;

/// The modes of the main encoder (former `EncoderMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncoderMode {
    Off,
    MasterVolume,
    MasterPanning,
    MetronomeVolume,
    SelectedTrackVolume,
    SelectedTrackPanning,
    CueVolume,
    CueMix,
    TemporaryTempo,
    TemporarySwing,
    PlayPosition,
    TemporaryPerform,
    TemporaryNotes,
    TemporaryLock,
    TemporaryTune,
    TemporaryBrowser,
}

impl EncoderMode {
    /// The label of the mode (former `MODE_LABELS`).
    pub fn label(self) -> Option<&'static str> {
        Some(match self {
            EncoderMode::Off => "Off",
            EncoderMode::MasterVolume => "Master: Volume",
            EncoderMode::MasterPanning => "Master: Panning",
            EncoderMode::SelectedTrackVolume => "Selected track: Volume",
            EncoderMode::SelectedTrackPanning => "Selected track: Panning",
            EncoderMode::MetronomeVolume => "Metronome: Volume",
            EncoderMode::CueVolume => "Cue: Volume",
            EncoderMode::CueMix => "Cue: Mix",
            EncoderMode::TemporaryTempo => "Tempo",
            EncoderMode::TemporarySwing => "Swing",
            EncoderMode::PlayPosition => "Play Position",
            _ => return None,
        })
    }

    /// The label of the mode if the function is toggled (former `TOGGLED_MODE_LABELS`).
    pub fn toggled_label(self) -> Option<&'static str> {
        match self {
            EncoderMode::PlayPosition => Some("Loop Start"),
            _ => self.label(),
        }
    }
}

/// The parameter which is controlled by the encoder (former bound `IParameter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderTarget {
    MasterVolume,
    MasterPan,
    SelectedTrackVolume,
    SelectedTrackPan,
    MetronomeVolume,
    CueVolume,
    CueMix,
    PlayPosition,
    LoopStart,
}

/// Is called for the temporary modes Perform, Notes, Lock and Tune (former
/// `IMaschineView.changeOption` of the active view).
pub type ChangeOptionHandler = Rc<dyn Fn(&mut Ctx, EncoderMode, i32)>;

/// Manages the modes of the main encoder (former `EncoderModeManager`).
pub struct EncoderModeManager {
    active_encoder_mode: Option<EncoderMode>,
    temporary_encoder_mode: Option<EncoderMode>,
    is_function_toggled: bool,
    /// The bound parameter and if changing it marks the Shift button as consumed (the Java
    /// parameter wrapper).
    target: Option<(EncoderTarget, bool)>,
    change_option: Option<ChangeOptionHandler>,
}

/// A shared encoder mode manager.
pub type SharedEncoderModeManager = Rc<RefCell<EncoderModeManager>>;

impl Default for EncoderModeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EncoderModeManager {
    pub fn new() -> Self {
        EncoderModeManager { active_encoder_mode: None, temporary_encoder_mode: None, is_function_toggled: false, target: None, change_option: None }
    }

    pub fn shared(self) -> SharedEncoderModeManager {
        Rc::new(RefCell::new(self))
    }

    /// Set the handler for the temporary modes which change an option of the active view.
    pub fn set_change_option_handler(&mut self, handler: ChangeOptionHandler) {
        self.change_option = Some(handler);
    }

    pub fn active_encoder_mode(&self) -> Option<EncoderMode> {
        self.active_encoder_mode
    }

    pub fn temporary_encoder_mode(&self) -> Option<EncoderMode> {
        self.temporary_encoder_mode
    }

    pub fn target(&self) -> Option<EncoderTarget> {
        self.target.map(|(target, _)| target)
    }

    pub fn is_function_toggled(&self) -> bool {
        self.is_function_toggled
    }

    pub fn set_active_encoder_mode(&mut self, ctx: &mut Ctx, encoder_mode: EncoderMode) {
        if self.active_encoder_mode == Some(encoder_mode) {
            return;
        }
        self.active_encoder_mode = Some(encoder_mode);
        self.update_binding(ctx);
    }

    pub fn toggle_function(&mut self, ctx: &mut Ctx) {
        self.is_function_toggled = !self.is_function_toggled;
        self.update_binding(ctx);
    }

    pub fn is_active_encoder_mode(&self, encoder_mode: EncoderMode) -> bool {
        self.active_encoder_mode == Some(encoder_mode)
    }

    pub fn enable_temporary_encode_mode(&mut self, ctx: &mut Ctx, encoder_mode: EncoderMode) {
        self.temporary_encoder_mode = Some(encoder_mode);
        self.target = None;
        delay_display(ctx, move |_ctx| encoder_mode.label().unwrap_or_default().to_string());
    }

    pub fn disable_temporary_encode_mode(&mut self, ctx: &mut Ctx) {
        self.temporary_encoder_mode = None;
        self.update_binding(ctx);
    }

    fn update_binding(&mut self, ctx: &mut Ctx) {
        if self.temporary_encoder_mode.is_some() {
            return;
        }
        let Some(mode) = self.active_encoder_mode else {
            return;
        };
        let toggled = self.is_function_toggled;
        let target = match mode {
            EncoderMode::Off => None,
            EncoderMode::PlayPosition => Some(if toggled { EncoderTarget::LoopStart } else { EncoderTarget::PlayPosition }),
            EncoderMode::SelectedTrackVolume | EncoderMode::SelectedTrackPanning => {
                if ctx.model.track_bank().selected_item().is_none() {
                    return;
                }
                Some(if toggled || mode == EncoderMode::SelectedTrackPanning { EncoderTarget::SelectedTrackPan } else { EncoderTarget::SelectedTrackVolume })
            }
            EncoderMode::MetronomeVolume => Some(EncoderTarget::MetronomeVolume),
            EncoderMode::CueVolume | EncoderMode::CueMix => {
                Some(if toggled || mode == EncoderMode::CueMix { EncoderTarget::CueMix } else { EncoderTarget::CueVolume })
            }
            _ => Some(if toggled || mode == EncoderMode::MasterPanning { EncoderTarget::MasterPan } else { EncoderTarget::MasterVolume }),
        };
        self.target = target.map(|t| (t, true));
        self.notify_mode(ctx);
    }

    fn notify_mode(&self, ctx: &mut Ctx) {
        let Some(mode) = self.active_encoder_mode else {
            return;
        };
        let label = if self.is_function_toggled { mode.toggled_label() } else { mode.label() };
        let label = label.unwrap_or_default().to_string();
        delay_display(ctx, move |_ctx| label);
    }

    /// Call on a change of the track selection (former `handleTrackChange`). Note: as in Java
    /// the volume parameter of the selected track is bound for both selected track modes.
    pub fn handle_track_change(&mut self, ctx: &Ctx, is_selected: bool) {
        if !is_selected || !matches!(self.active_encoder_mode, Some(EncoderMode::SelectedTrackVolume) | Some(EncoderMode::SelectedTrackPanning)) {
            return;
        }
        if ctx.model.track_bank().selected_item().is_some() {
            self.target = Some((EncoderTarget::SelectedTrackVolume, false));
        }
    }

    /// Is the encoder mode active (button light)?
    pub fn is_lit(&self, encoder_mode: EncoderMode) -> bool {
        self.is_active_encoder_mode(encoder_mode)
    }

    fn change_target(ctx: &mut Ctx, target: EncoderTarget, value: i32) {
        let slow = ctx.surface.is_knob_sensitivity_slow();
        let is_increase = ctx.model.value_changer().is_increase(value);
        match target {
            EncoderTarget::MasterVolume => ctx.model.master_track_mut().change_volume(value),
            EncoderTarget::MasterPan => ctx.model.master_track_mut().change_pan(value),
            EncoderTarget::SelectedTrackVolume => {
                if let Some(track) = ctx.model.track_bank_mut().selected_item_mut() {
                    track.change_volume(value);
                }
            }
            EncoderTarget::SelectedTrackPan => {
                if let Some(track) = ctx.model.track_bank_mut().selected_item_mut() {
                    track.change_pan(value);
                }
            }
            EncoderTarget::MetronomeVolume => ctx.model.transport_mut().change_metronome_volume(value),
            EncoderTarget::CueVolume => ctx.model.project_mut().change_cue_volume(value),
            EncoderTarget::CueMix => ctx.model.project_mut().change_cue_mix(value),
            // Former PlayPositionParameter
            EncoderTarget::PlayPosition => ctx.model.transport_mut().change_position(is_increase, slow),
            // Former LoopStartParameter
            EncoderTarget::LoopStart => {
                if ctx.surface.is_select_pressed() {
                    ctx.model.transport_mut().change_loop_length(is_increase, slow);
                } else {
                    ctx.model.transport_mut().change_loop_start(is_increase, slow);
                }
            }
        }
    }

    fn handle_browser(ctx: &mut Ctx, value: i32) {
        let increase = ctx.model.value_changer().is_increase(value);
        if ctx.surface.is_shift_pressed() {
            if let Some(browser) = ctx.model.browser_mut() {
                if increase {
                    browser.next_content_type();
                } else {
                    browser.previous_content_type();
                }
            }
            return;
        }

        if ctx.surface.is_select_pressed() {
            let device = ctx.model.cursor_device();
            if !device.does_exist() {
                return;
            }
            let (name, position) = (device.name().to_string(), device.position());
            if let Some(browser) = ctx.model.browser_mut() {
                if increase {
                    browser.insert_after_cursor_device(&name, position);
                } else {
                    browser.insert_before_cursor_device(&name, position);
                }
            }
            return;
        }

        if let Some(browser) = ctx.model.browser_mut() {
            if increase {
                browser.select_next_result();
            } else {
                browser.select_previous_result();
            }
        }
    }
}

/// The continuous command of the encoder which uses a shared [`EncoderModeManager`].
pub struct EncoderModeCommand {
    manager: SharedEncoderModeManager,
}

impl EncoderModeCommand {
    pub fn new(manager: SharedEncoderModeManager) -> Self {
        EncoderModeCommand { manager }
    }
}

impl ContinuousCommand for EncoderModeCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        let (target, temporary, change_option) = {
            let manager = self.manager.borrow();
            (manager.target, manager.temporary_encoder_mode, manager.change_option.clone())
        };

        // A bound parameter
        if let Some((target, consume_shift)) = target {
            EncoderModeManager::change_target(ctx, target, value);
            if consume_shift {
                ctx.surface.set_trigger_consumed(ButtonId::Shift);
            }
            return;
        }

        let Some(temporary) = temporary else {
            return;
        };
        match temporary {
            EncoderMode::TemporaryTempo => {
                // Former TempoCommand.execute (relative)
                let is_increase = ctx.model.value_changer().is_increase(value);
                let slow = ctx.surface.is_knob_sensitivity_slow();
                ctx.model.transport_mut().change_tempo(is_increase, slow);
            }
            // Former continuous SwingCommand
            EncoderMode::TemporarySwing => ctx.model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount).change_value(value),
            EncoderMode::TemporaryPerform | EncoderMode::TemporaryNotes | EncoderMode::TemporaryLock | EncoderMode::TemporaryTune => {
                if let Some(handler) = change_option {
                    handler(ctx, temporary, value);
                }
            }
            EncoderMode::TemporaryBrowser => EncoderModeManager::handle_browser(ctx, value),
            _ => {}
        }
    }
}

/// Selects an encoder mode (former `MaschineMonitorEncoderCommand`).
pub struct MaschineMonitorEncoderCommand {
    manager: SharedEncoderModeManager,
    encoder_mode: EncoderMode,
}

impl MaschineMonitorEncoderCommand {
    pub fn new(manager: SharedEncoderModeManager, encoder_mode: EncoderMode) -> Self {
        MaschineMonitorEncoderCommand { manager, encoder_mode }
    }

    pub fn is_lit(&self) -> bool {
        self.manager.borrow().is_lit(self.encoder_mode)
    }
}

impl TriggerCommand for MaschineMonitorEncoderCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            self.manager.borrow_mut().set_active_encoder_mode(ctx, self.encoder_mode);
        }
    }
}
