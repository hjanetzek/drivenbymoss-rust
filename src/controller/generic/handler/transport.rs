// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for transport commands (former `TransportHandler`).

use super::{absolute_value_changer, is_absolute, is_button_pressed, is_increase, relative_value_changer, to_midi_value, unsupported};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::controller::generic::surface::flexi;
use crate::daw::constants::AutomationMode;
use crate::framework::command::SharedTrigger;
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let transport = ctx.model.transport();
    match command {
        FlexiCommand::TransportPlay => to_midi_value(transport.is_playing()),
        FlexiCommand::TransportStop => to_midi_value(!transport.is_playing()),
        FlexiCommand::TransportToggleRepeat => to_midi_value(transport.is_loop()),
        FlexiCommand::TransportToggleMetronome => to_midi_value(transport.is_metronome_on()),
        FlexiCommand::TransportSetMetronomeVolume => transport.metronome_volume(),
        FlexiCommand::TransportToggleMetronomeInPreroll => to_midi_value(transport.is_preroll_metronome_enabled()),
        FlexiCommand::TransportTogglePunchIn => to_midi_value(transport.is_punch_in_enabled()),
        FlexiCommand::TransportTogglePunchOut => to_midi_value(transport.is_punch_out_enabled()),
        FlexiCommand::TransportToggleRecord => to_midi_value(transport.is_recording()),
        FlexiCommand::TransportToggleArrangerOverdub => to_midi_value(ctx.model.is_arranger_overdub()),
        FlexiCommand::TransportToggleClipOverdub => to_midi_value(ctx.model.is_launcher_overdub()),
        FlexiCommand::TransportToggleArrangerAutomationWrite => to_midi_value(transport.is_writing_arranger_automation()),
        FlexiCommand::TransportToggleClipAutomationWrite => to_midi_value(transport.is_writing_clip_launcher_automation()),
        _ => -1,
    }
}

/// Execute a trigger command with the button state.
fn execute(ctx: &mut Ctx, command: SharedTrigger, is_button_pressed: bool) {
    let (event, velocity) = if is_button_pressed { (ButtonEvent::Down, 127) } else { (ButtonEvent::Up, 0) };
    command.borrow_mut().execute(ctx, event, velocity);
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);
    // All commands except these are only executed when the button is pressed
    let is_ungated = matches!(
        command,
        FlexiCommand::TransportPlay
            | FlexiCommand::TransportRewind
            | FlexiCommand::TransportFastForward
            | FlexiCommand::TransportSetMetronomeVolume
            | FlexiCommand::TransportSetTempo
            | FlexiCommand::TransportMovePlayCursor
    );
    if !is_button_pressed && !is_ungated {
        return;
    }
    let transport = ctx.model.transport_mut();
    match command {
        FlexiCommand::TransportPlay => {
            let play = flexi(ctx).play_command.clone();
            execute(ctx, play, is_button_pressed);
        }
        FlexiCommand::TransportStop => transport.stop(),
        FlexiCommand::TransportRestart => transport.restart(),
        FlexiCommand::TransportRewind => {
            let rewind = flexi(ctx).rewind_command.clone();
            execute(ctx, rewind, is_button_pressed);
        }
        FlexiCommand::TransportFastForward => {
            let fast_forward = flexi(ctx).fast_forward_command.clone();
            execute(ctx, fast_forward, is_button_pressed);
        }
        FlexiCommand::TransportToggleRepeat => transport.toggle_loop(),
        FlexiCommand::TransportToggleMetronome => transport.toggle_metronome(),
        FlexiCommand::TransportSetMetronomeVolume => handle_metronome_volume(ctx, knob_mode, value),
        FlexiCommand::TransportToggleMetronomeInPreroll => transport.toggle_preroll_metronome(),
        FlexiCommand::TransportTogglePunchIn => transport.toggle_punch_in(),
        FlexiCommand::TransportTogglePunchOut => transport.toggle_punch_out(),
        FlexiCommand::TransportToggleRecord => transport.start_recording(),
        FlexiCommand::TransportToggleArrangerOverdub => ctx.model.toggle_overdub(),
        FlexiCommand::TransportToggleClipOverdub => ctx.model.toggle_launcher_overdub(),
        FlexiCommand::TransportToggleArrangerAutomationWrite => transport.toggle_write_arranger_automation(),
        FlexiCommand::TransportToggleClipAutomationWrite => transport.toggle_write_clip_launcher_automation(),
        FlexiCommand::TransportSetWriteModeLatch => transport.set_automation_write_mode(AutomationMode::Latch),
        FlexiCommand::TransportSetWriteModeTouch => transport.set_automation_write_mode(AutomationMode::Touch),
        FlexiCommand::TransportSetWriteModeWrite => transport.set_automation_write_mode(AutomationMode::Write),
        FlexiCommand::TransportSetTempo => handle_tempo(ctx, knob_mode, value),
        FlexiCommand::TransportTapTempo => transport.tap_tempo(),
        FlexiCommand::TransportMovePlayCursor => handle_play_cursor(ctx, knob_mode, value),
        _ => unsupported(ctx, command),
    }
}

fn handle_play_cursor(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    // Only relative modes are supported
    if !is_absolute(knob_mode) {
        let increase = is_increase(ctx, knob_mode, value);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_position(increase, slow);
    }
}

fn handle_tempo(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) {
        let transport = ctx.model.transport_mut();
        let tempo = transport.scale_tempo(value.value() as f64, if value.is_high_res() { 16384 } else { 128 });
        transport.set_tempo(tempo);
    } else {
        let increase = is_increase(ctx, knob_mode, value);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_tempo(increase, slow);
    }
}

/// Former `handleMetronomeVolume` (with the Reaper `MetronomeVolumeParameterImpl.setValue` and
/// `changeValue` of a value changer).
fn handle_metronome_volume(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    let val = value.value();
    if is_absolute(knob_mode) {
        let normalized = absolute_value_changer(ctx, value).to_normalized_value(val as f64);
        ctx.model.transport_mut().metronome_volume_parameter_mut().set_normalized_value(normalized);
    } else {
        let changer = relative_value_changer(ctx, knob_mode);
        let parameter = ctx.model.transport_mut().metronome_volume_parameter_mut();
        let new_value = changer.change_value(val, parameter.value());
        parameter.set_value(new_value);
    }
}
