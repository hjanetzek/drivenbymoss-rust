// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the knobs of the Push (former `controller.ableton.push.command.continuous`
//! and the knob commands of `command.trigger`: `PushMasterVolumeCommand`,
//! `MastertrackTouchCommand`, `ConfigurePitchbendCommand`, `RasteredKnobCommand`,
//! `PlayPositionKnobCommand` and `Push3EncoderCommand`).

use super::modes::{self, EncoderAction};
use crate::config::push as config;
use crate::daw::constants::GrooveParameterId;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::commands::channel::PlayPositionCommand;
use crate::framework::commands::general::{focused_parameter, ZoomParameter};
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::commands::transport_ext::TempoCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::parameterprovider::CustomParameter;
use crate::midi::ButtonEvent;

/// Former `MVHelper.notifyTempo`.
pub fn notify_tempo(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| {
        let transport = ctx.model.transport();
        format!("Tempo: {}", transport.format_tempo(transport.tempo()))
    });
}

/// Former `MVHelper.notifyShuffle`.
pub fn notify_shuffle(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| format!("Shuffle: {}", ctx.model.groove().parameter(GrooveParameterId::ShuffleAmount).displayed_value()));
}

// ----------------------------------------------------------------------------------------------

/// The master knob: master volume or cue volume (Push 3), Select + knob zooms (former
/// `PushMasterVolumeCommand`).
pub struct PushMasterVolumeCommand {
    is_master_volume_mode: bool,
}

impl Default for PushMasterVolumeCommand {
    fn default() -> Self {
        PushMasterVolumeCommand { is_master_volume_mode: true }
    }
}

impl PushMasterVolumeCommand {
    /// Former `toggleMasterVolumeAndCue`.
    pub fn toggle_master_volume_and_cue(&mut self) {
        self.is_master_volume_mode = !self.is_master_volume_mode;
    }

    pub fn is_master_volume_mode(&self) -> bool {
        self.is_master_volume_mode
    }

    /// Former `notifyMode`.
    pub fn notify_mode(&self, ctx: &mut Ctx) {
        ctx.notify(if self.is_master_volume_mode { "Master Volume" } else { "Cue Volume" });
    }
}

impl ContinuousCommand for PushMasterVolumeCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if ctx.surface.is_select_pressed() {
            ZoomParameter.change_value(ctx, value);
            return;
        }
        if self.is_master_volume_mode {
            ctx.model.master_track_mut().change_volume(value);
        } else {
            ctx.model.project_mut().change_cue_volume(value);
        }
    }
}

/// The touch of the master knob: shows the master mode temporarily, Delete resets the volume
/// (former `MastertrackTouchCommand`).
#[derive(Default)]
pub struct MastertrackTouchCommand;

impl TriggerCommand for MastertrackTouchCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        let is_touched = event != ButtonEvent::Up;

        // Avoid accidentally leaving the browser
        if ctx.modes.is_active(Modes::Browser) {
            return;
        }

        ctx.model.master_track_mut().touch_volume(is_touched);

        if ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            ctx.model.master_track_mut().reset_volume();
            return;
        }

        let is_master_mode = ctx.modes.is_active(Modes::Master);
        if is_touched && is_master_mode {
            return;
        }
        if is_touched {
            ctx.set_temporary_mode(Modes::MasterTemp);
        } else if !is_master_mode {
            ctx.restore_mode();
        }
    }
}

/// The touch of the ribbon: Delete resets the parameter of the ribbon, Shift shows the ribbon
/// mode (former `ConfigurePitchbendCommand`).
#[derive(Default)]
pub struct ConfigurePitchbendCommand;

impl TriggerCommand for ConfigurePitchbendCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        // Reset parameters if Delete button is held
        if event != ButtonEvent::Down || !ctx.surface.is_delete_pressed() {
            return;
        }
        ctx.surface.set_trigger_consumed(ButtonId::Delete);
        match config::ribbon_mode(&ctx.config) {
            config::RIBBON_MODE_FADER => ctx.model.cursor_track_mut().reset_volume(),
            config::RIBBON_MODE_LAST_TOUCHED => {
                let parameter = focused_parameter(ctx);
                if !parameter.is_empty() {
                    parameter.reset_value(ctx);
                }
            }
            // Not used in other modes
            _ => {}
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if !ctx.modes.is_active(Modes::Ribbon) {
            ctx.set_temporary_mode(Modes::Ribbon);
        }
    }
}

/// The small rastered knob: tempo or shuffle, Select + knob moves the loop start; changes the
/// selected browser column while browsing (former `RasteredKnobCommand`).
pub struct RasteredKnobCommand {
    tempo: TempoCommand,
    is_tempo_mode: bool,
}

impl Default for RasteredKnobCommand {
    fn default() -> Self {
        RasteredKnobCommand { tempo: TempoCommand::new(), is_tempo_mode: true }
    }
}

impl RasteredKnobCommand {
    /// Former `notifyMode`.
    pub fn notify_mode(&self, ctx: &mut Ctx) {
        if ctx.modes.is_active(Modes::Browser) {
            return;
        }
        if self.is_tempo_mode {
            notify_tempo(ctx);
        } else {
            notify_shuffle(ctx);
        }
    }

    /// Former `toggleMode`.
    pub fn toggle_mode(&mut self) {
        self.is_tempo_mode = !self.is_tempo_mode;
    }

    pub fn is_tempo_mode(&self) -> bool {
        self.is_tempo_mode
    }
}

impl ContinuousCommand for RasteredKnobCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if ctx.modes.is_active(Modes::Browser) {
            modes::change_selected_column_value(ctx, value);
            return;
        }

        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            let increase = ctx.model.value_changer().is_increase(value);
            let slow = ctx.surface.is_knob_sensitivity_slow();
            ctx.model.transport_mut().change_loop_start(increase, slow);
            return;
        }

        if self.is_tempo_mode {
            ContinuousCommand::execute(&mut self.tempo, ctx, value);
            notify_tempo(ctx);
        } else {
            ctx.model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount).change_value(value);
            notify_shuffle(ctx);
        }
    }
}

impl TriggerCommand for RasteredKnobCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        let activate = event != ButtonEvent::Up;
        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            if activate {
                delay_display(ctx, |ctx| format!("Loop Start: {}", ctx.model.transport().loop_start_beat_text()));
            }
            return;
        }

        if ctx.modes.is_active(Modes::Browser) {
            return;
        }

        if self.is_tempo_mode {
            ctx.model.transport_mut().set_tempo_indication(activate);
            if activate {
                notify_tempo(ctx);
            }
        } else if activate {
            notify_shuffle(ctx);
        }
    }
}

/// Former `PlayPositionKnobCommand.displayPosition`.
pub fn display_position(ctx: &mut Ctx) {
    delay_display(ctx, |ctx| {
        let transport = ctx.model.transport();
        format!("{} - {}", transport.beat_text(), transport.position_text())
    });
}

/// The small knob 2 (Push 1/2): play position, Select + knob changes the loop length (former
/// `PlayPositionKnobCommand`).
#[derive(Default)]
pub struct PlayPositionKnobCommand;

impl PlayPositionKnobCommand {
    /// Former `execute (value)`.
    pub fn change(ctx: &mut Ctx, value: i32) {
        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            let increase = ctx.model.value_changer().is_increase(value);
            let slow = ctx.surface.is_knob_sensitivity_slow();
            ctx.model.transport_mut().change_loop_length(increase, slow);
            return;
        }
        PlayPositionCommand::change_position(ctx, value);
        display_position(ctx);
    }

    /// Former `execute (event, velocity)` (the touch of the knob).
    pub fn touch(ctx: &mut Ctx, event: ButtonEvent) {
        let activate = event != ButtonEvent::Up;
        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            if activate {
                delay_display(ctx, |ctx| format!("Loop Length: {}", ctx.model.transport().loop_length_beat_text()));
            }
            return;
        }
        ctx.model.transport_mut().set_tempo_indication(activate);
        if activate {
            display_position(ctx);
        }
    }
}

impl ContinuousCommand for PlayPositionKnobCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        Self::change(ctx, value);
    }
}

impl TriggerCommand for PlayPositionKnobCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        Self::touch(ctx, event);
    }
}

/// The encoder of the Push 3 with its press, left, right and the center button of the arrows
/// (former `Push3EncoderCommand`): calls the active mode if it implements `IPush3Encoder`,
/// otherwise changes the play position.
#[derive(Default)]
pub struct Push3EncoderCommand;

impl Push3EncoderCommand {
    /// Former `left`: move the play position to the loop end, start or the project start.
    pub fn left(ctx: &mut Ctx, event: ButtonEvent) {
        if modes::execute_encoder(ctx, EncoderAction::Left(event)) || event != ButtonEvent::Down {
            return;
        }
        let transport = ctx.model.transport();
        let position = transport.position();
        if position > transport.loop_end() {
            ctx.model.transport_mut().select_loop_end();
        } else if position > transport.loop_start() {
            ctx.model.transport_mut().select_loop_start();
        } else {
            ctx.model.transport_mut().set_position(0.0);
        }
        display_position(ctx);
    }

    /// Former `right`: move the play position to the loop start, end or the project end.
    pub fn right(ctx: &mut Ctx, event: ButtonEvent) {
        if modes::execute_encoder(ctx, EncoderAction::Right(event)) || event != ButtonEvent::Down {
            return;
        }
        let transport = ctx.model.transport();
        let position = transport.position();
        if position < transport.loop_start() {
            ctx.model.transport_mut().select_loop_start();
        } else if position < transport.loop_end() {
            ctx.model.transport_mut().select_loop_end();
        } else {
            ctx.model.transport_mut().set_position_to_end();
        }
        display_position(ctx);
    }

    /// Former `center`: launch the selected scene (alternative launch).
    pub fn center(ctx: &mut Ctx, event: ButtonEvent) {
        if modes::execute_encoder(ctx, EncoderAction::Center(event)) {
            return;
        }
        let index = ctx.model.scene_bank().selected_index();
        if let Some(index) = index {
            ctx.model.scene_bank_mut().item_mut(index).launch(event == ButtonEvent::Down, true);
        }
    }
}

impl ContinuousCommand for Push3EncoderCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        // Encoder turn
        if modes::execute_encoder(ctx, EncoderAction::Turn(value)) {
            return;
        }
        // Change play position and loop length
        PlayPositionKnobCommand::change(ctx, value);
    }
}

impl TriggerCommand for Push3EncoderCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        // Encoder pressed
        if modes::execute_encoder(ctx, EncoderAction::Press(event)) {
            return;
        }
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().play();
        }
    }
}
