// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for effect track commands (former `FxTrackHandler`).
//!
//! All commands work on the effect track bank, which only exists when the host reports one
//! (Bitwig). Reaper has none (as `IModel.getEffectTrackBank` of the Java Reaper version, which
//! returns null): then all commands do nothing and have no value.
//!
//! Java quirks which are kept: the value of the navigation commands is never reported and "Scroll
//! Bank Page by 1 left/right" does nothing (their cases in Java use the track commands, which this
//! handler never receives); the navigation commands select in the *current* track bank.

use super::track::TrackState;
use super::{increase_knob_movement, is_absolute, is_button_pressed, is_increase, set_or_change, to_midi_value, unsupported, value_changers};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::bank::BankItem;
use crate::daw::track_bank::TrackBank;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

/// The commands of a track state: the first toggle command (track 1), the first set command.
const STATE_COMMANDS: [(TrackState, FlexiCommand, FlexiCommand); 6] = [
    (TrackState::Active, FlexiCommand::FxTrack1ToggleActive, FlexiCommand::FxTrack1SetActive),
    (TrackState::Mute, FlexiCommand::FxTrack1ToggleMute, FlexiCommand::FxTrack1SetMute),
    (TrackState::Solo, FlexiCommand::FxTrack1ToggleSolo, FlexiCommand::FxTrack1SetSolo),
    (TrackState::RecArm, FlexiCommand::FxTrack1ToggleArm, FlexiCommand::FxTrack1SetArm),
    (TrackState::Monitor, FlexiCommand::FxTrack1ToggleMonitor, FlexiCommand::FxTrack1SetMonitor),
    (TrackState::AutoMonitor, FlexiCommand::FxTrack1ToggleAutoMonitor, FlexiCommand::FxTrack1SetAutoMonitor),
];

/// The state, the track index in the page and if it is a toggle command.
fn state_command(command: FlexiCommand) -> Option<(TrackState, usize, bool)> {
    STATE_COMMANDS.into_iter().find_map(|(state, first_toggle, first_set)| {
        command.index_in(first_toggle, 8).map(|index| (state, index, true)).or_else(|| command.index_in(first_set, 8).map(|index| (state, index, false)))
    })
}

/// The track and send index of a send command, the track index is -1 for the selected track.
fn send_command(command: FlexiCommand) -> Option<(i32, usize)> {
    if let Some(index) = command.index_in(FlexiCommand::FxTrack1SetSend1, 64) {
        return Some(((index % 8) as i32, index / 8));
    }
    command.index_in(FlexiCommand::FxTrackSelectedSetSend1, 8).map(|send| (-1, send))
}

/// Former `getTrack`: the selected effect track for a negative index, None if it does not exist.
fn track_index(bank: &TrackBank, index: i32) -> Option<usize> {
    if index < 0 {
        return bank.selected_index();
    }
    let index = index as usize;
    bank.item(index).does_exist().then_some(index)
}

/// Former `getCommandValue`: -1 without an effect track bank.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let Some(bank) = ctx.model.effect_track_bank() else {
        return -1;
    };
    if let Some((state, index, _)) = state_command(command) {
        return to_midi_value(state.get(bank.item(index)));
    }
    if let Some(index) = command.index_in(FlexiCommand::FxTrack1Select, 8) {
        return to_midi_value(bank.item(index).is_selected());
    }
    if let Some(index) = command.index_in(FlexiCommand::FxTrack1SetVolume, 8) {
        return bank.item(index).volume();
    }
    if let Some(index) = command.index_in(FlexiCommand::FxTrack1SetPanning, 8) {
        return bank.item(index).pan();
    }
    if let Some((track_index, send_index)) = send_command(command) {
        let Some(index) = self::track_index(bank, track_index) else {
            return 0;
        };
        let send_bank = bank.item(index).send_bank();
        return if send_index < send_bank.page_size() { send_bank.item(send_index).value() } else { 0 };
    }
    -1
}

/// Former `handle`: does nothing without an effect track bank.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    if ctx.model.effect_track_bank().is_none() {
        return;
    }
    let is_button_pressed = is_button_pressed(knob_mode, value);

    if let Some((state, index, is_toggle)) = state_command(command) {
        if is_button_pressed {
            if let Some(bank) = ctx.model.effect_track_bank_mut() {
                let track = bank.item_mut(index);
                if is_toggle {
                    state.toggle(track);
                } else {
                    state.set(track, value.is_positive());
                }
            }
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::FxTrack1Select, 8) {
        if is_button_pressed {
            if let Some(bank) = ctx.model.effect_track_bank_mut() {
                bank.item_mut(index).select();
            }
            mv_helper::notify_selected_track(ctx);
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::FxTrack1SetVolume, 8) {
        change_value(ctx, knob_mode, index as i32, None, value, |track| track.volume_parameter_mut());
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::FxTrack1SetPanning, 8) {
        change_value(ctx, knob_mode, index as i32, None, value, |track| track.pan_parameter_mut());
        return;
    }
    if let Some((track_index, send_index)) = send_command(command) {
        change_value(ctx, knob_mode, track_index, Some(send_index), value, |track| track.volume_parameter_mut());
        return;
    }

    match command {
        FlexiCommand::FxTrackSelectPreviousBankPage => {
            if is_button_pressed {
                scroll_track_left(ctx, true);
            }
        }
        FlexiCommand::FxTrackSelectNextBankPage => {
            if is_button_pressed {
                scroll_track_right(ctx, true);
            }
        }
        FlexiCommand::FxTrackSelectPreviousTrack => {
            if is_button_pressed {
                scroll_track_left(ctx, false);
            }
        }
        FlexiCommand::FxTrackSelectNextTrack => {
            if is_button_pressed {
                scroll_track_right(ctx, false);
            }
        }
        // The cases of Java use TRACK_SCROLL_BANK_PAGE_BY_1_LEFT/RIGHT: never executed
        FlexiCommand::FxTrackScrollBankPageBy1Left | FlexiCommand::FxTrackScrollBankPageBy1Right => {}
        FlexiCommand::FxTrackScrollTracks => {
            if is_absolute(knob_mode) || !increase_knob_movement(ctx, CommandCategory::FxTrack) {
                return;
            }
            if is_increase(ctx, knob_mode, value) {
                scroll_track_right(ctx, false);
            } else {
                scroll_track_left(ctx, false);
            }
        }
        _ => unsupported(ctx, command),
    }
}

/// Set (absolute) or change (relative) the volume, panorama (`send` None, `parameter` selects
/// it) or a send of an effect track.
fn change_value(ctx: &mut Ctx, knob_mode: KnobMode, track_index: i32, send: Option<usize>, value: MidiValue, parameter: fn(&mut crate::daw::track::Track) -> &mut crate::daw::parameter::Parameter) {
    let changers = value_changers(ctx, knob_mode, value);
    let Some(bank) = ctx.model.effect_track_bank_mut() else {
        return;
    };
    let Some(index) = self::track_index(bank, track_index) else {
        return;
    };
    let track = bank.item_mut(index);
    match send {
        None => set_or_change(parameter(track), &changers, knob_mode, value),
        Some(send_index) => {
            let send_bank = track.send_bank_mut();
            if send_index < send_bank.page_size() {
                set_or_change(send_bank.item_mut(send_index), &changers, knob_mode, value);
            }
        }
    }
}

/// Former `scrollTrackLeft`: in the current track bank (as Java), the previous track or page.
fn scroll_track_left(ctx: &mut Ctx, switch_bank: bool) {
    let bank = ctx.model.current_track_bank_mut();
    let index = bank.selected_index().map_or(0, |i| i as i32 - 1);
    if index == -1 || switch_bank {
        bank.select_previous_page();
        return;
    }
    bank.item_mut(index as usize).select();
}

/// Former `scrollTrackRight`: in the current track bank (as Java), the next track or page.
fn scroll_track_right(ctx: &mut Ctx, switch_bank: bool) {
    let bank = ctx.model.current_track_bank_mut();
    let index = bank.selected_index().map_or(0, |i| i + 1);
    if index == 8 || switch_bank {
        bank.select_next_page();
        return;
    }
    bank.item_mut(index).select();
}
