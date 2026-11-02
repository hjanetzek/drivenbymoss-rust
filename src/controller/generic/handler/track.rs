// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for track commands (former `TrackHandler`).
//!
//! The numbered commands (track 1-8 of the current track bank page) are handled by ranges of
//! 8 commands, e.g. `Track1ToggleMute` - `Track8ToggleMute`. The send commands are ordered by
//! send, then by track: `Track1SetSend1` - `Track8SetSend1`, `Track1SetSend2`, ...

use super::{is_absolute, is_button_pressed, is_increase, set_or_change, to_midi_value, unsupported, value_changers};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::controller::generic::surface::flexi;
use crate::daw::bank::BankItem;
use crate::daw::constants::ChannelType;
use crate::daw::track::Track;
use crate::framework::clip_launcher_navigator::{navigate_tracks, select_track};
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

/// A state of a track which can be toggled or set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrackState {
    Active,
    Mute,
    Solo,
    RecArm,
    Monitor,
    AutoMonitor,
}

impl TrackState {
    fn get(self, track: &Track) -> bool {
        match self {
            TrackState::Active => track.is_activated(),
            TrackState::Mute => track.is_mute(),
            TrackState::Solo => track.is_solo(),
            TrackState::RecArm => track.is_rec_arm(),
            TrackState::Monitor => track.is_monitor(),
            TrackState::AutoMonitor => track.is_auto_monitor(),
        }
    }

    fn toggle(self, track: &mut Track) {
        match self {
            TrackState::Active => track.toggle_is_activated(),
            TrackState::Mute => track.toggle_mute(),
            TrackState::Solo => track.toggle_solo(),
            TrackState::RecArm => track.toggle_rec_arm(),
            TrackState::Monitor => track.toggle_monitor(),
            TrackState::AutoMonitor => track.toggle_auto_monitor(),
        }
    }

    fn set(self, track: &mut Track, enable: bool) {
        match self {
            TrackState::Active => track.set_is_activated(enable),
            TrackState::Mute => track.set_mute(enable),
            TrackState::Solo => track.set_solo(enable),
            TrackState::RecArm => track.set_rec_arm(enable),
            TrackState::Monitor => track.set_monitor(enable),
            TrackState::AutoMonitor => track.set_auto_monitor(enable),
        }
    }
}

/// The commands of a track state: the first toggle command (track 1), the toggle command of the
/// selected track, the first set command, the set command of the selected track.
const STATE_COMMANDS: [(TrackState, FlexiCommand, FlexiCommand, FlexiCommand, FlexiCommand); 6] = [
    (TrackState::Active, FlexiCommand::Track1ToggleActive, FlexiCommand::TrackSelectedToggleActive, FlexiCommand::Track1SetActive, FlexiCommand::TrackSelectedSetActive),
    (TrackState::Mute, FlexiCommand::Track1ToggleMute, FlexiCommand::TrackSelectedToggleMute, FlexiCommand::Track1SetMute, FlexiCommand::TrackSelectedSetMute),
    (TrackState::Solo, FlexiCommand::Track1ToggleSolo, FlexiCommand::TrackSelectedToggleSolo, FlexiCommand::Track1SetSolo, FlexiCommand::TrackSelectedSetSolo),
    (TrackState::RecArm, FlexiCommand::Track1ToggleArm, FlexiCommand::TrackSelectedToggleArm, FlexiCommand::Track1SetArm, FlexiCommand::TrackSelectedSetArm),
    (TrackState::Monitor, FlexiCommand::Track1ToggleMonitor, FlexiCommand::TrackSelectedToggleMonitor, FlexiCommand::Track1SetMonitor, FlexiCommand::TrackSelectedSetMonitor),
    (
        TrackState::AutoMonitor,
        FlexiCommand::Track1ToggleAutoMonitor,
        FlexiCommand::TrackSelectedToggleAutoMonitor,
        FlexiCommand::Track1SetAutoMonitor,
        FlexiCommand::TrackSelectedSetAutoMonitor,
    ),
];

/// Where a state command applies to.
enum Target {
    /// The track with the index in the page of the current track bank.
    Track(usize),
    /// The cursor track.
    Cursor,
}

/// Find the track state, the target and if it is a toggle command.
fn state_command(command: FlexiCommand) -> Option<(TrackState, Target, bool)> {
    for (state, first_toggle, selected_toggle, first_set, selected_set) in STATE_COMMANDS {
        if let Some(index) = command.index_in(first_toggle, 8) {
            return Some((state, Target::Track(index), true));
        }
        if command == selected_toggle {
            return Some((state, Target::Cursor, true));
        }
        if let Some(index) = command.index_in(first_set, 8) {
            return Some((state, Target::Track(index), false));
        }
        if command == selected_set {
            return Some((state, Target::Cursor, false));
        }
    }
    None
}

/// The track and send index of a send command (`Track1SetSend1` .. `Track8SetSend8`), the track
/// index is -1 for the selected track.
fn send_command(command: FlexiCommand) -> Option<(i32, usize)> {
    if let Some(index) = command.index_in(FlexiCommand::Track1SetSend1, 64) {
        return Some(((index % 8) as i32, index / 8));
    }
    command.index_in(FlexiCommand::TrackSelectedSetSend1, 8).map(|send| (-1, send))
}

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let track_bank = ctx.model.current_track_bank();
    let cursor_track = ctx.model.cursor_track();

    if let Some((state, target, _)) = state_command(command) {
        return to_midi_value(match target {
            Target::Track(index) => state.get(track_bank.item(index)),
            Target::Cursor => cursor_track.does_exist() && state.get(cursor_track),
        });
    }
    if let Some(index) = command.index_in(FlexiCommand::Track1Select, 8) {
        return to_midi_value(track_bank.item(index).is_selected());
    }
    if let Some(index) = command.index_in(FlexiCommand::Track1SetVolume, 8) {
        return track_bank.item(index).volume();
    }
    if let Some(index) = command.index_in(FlexiCommand::Track1SetPanning, 8) {
        return track_bank.item(index).pan();
    }
    if let Some((track_index, send_index)) = send_command(command) {
        return send_value(ctx, track_index, send_index);
    }

    match command {
        FlexiCommand::TrackSelectPreviousBankPage => to_midi_value(track_bank.can_scroll_page_backwards()),
        FlexiCommand::TrackSelectNextBankPage => to_midi_value(track_bank.can_scroll_page_forwards()),
        FlexiCommand::TrackSelectPreviousTrack | FlexiCommand::TrackScrollBankPageBy1Left => to_midi_value(track_bank.can_scroll_backwards()),
        FlexiCommand::TrackSelectNextTrack | FlexiCommand::TrackScrollBankPageBy1Right => to_midi_value(track_bank.can_scroll_forwards()),
        FlexiCommand::TrackSelectedSetVolumeTrack => {
            if cursor_track.does_exist() {
                cursor_track.volume()
            } else {
                0
            }
        }
        FlexiCommand::TrackSelectedSetPanning => {
            if cursor_track.does_exist() {
                cursor_track.pan()
            } else {
                0
            }
        }
        FlexiCommand::TrackSelectedTogglePin | FlexiCommand::TrackSelectedSetPin => to_midi_value(cursor_track.does_exist() && ctx.model.is_cursor_track_pinned()),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);

    if let Some((state, target, is_toggle)) = state_command(command) {
        if !is_button_pressed {
            return;
        }
        let track = match target {
            Target::Track(index) => ctx.model.current_track_bank_mut().item_mut(index),
            Target::Cursor => ctx.model.cursor_track_mut(),
        };
        if is_toggle {
            state.toggle(track);
        } else {
            state.set(track, value.is_positive());
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::Track1Select, 8) {
        if is_button_pressed {
            let navigator = flexi(ctx).navigator.clone();
            select_track(&navigator, ctx, index);
            mv_helper::notify_selected_track(ctx);
        }
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::Track1SetVolume, 8) {
        change_track_volume(ctx, knob_mode, index as i32, value);
        return;
    }
    if let Some(index) = command.index_in(FlexiCommand::Track1SetPanning, 8) {
        change_track_panning(ctx, knob_mode, index as i32, value);
        return;
    }
    if let Some((track_index, send_index)) = send_command(command) {
        change_send_volume(ctx, track_index, send_index, knob_mode, value);
        return;
    }

    match command {
        FlexiCommand::TrackToggleTrackBank => {
            if is_button_pressed {
                toggle_track_banks(ctx);
            }
        }
        FlexiCommand::TrackAddAudioTrack => {
            if is_button_pressed {
                ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]);
            }
        }
        FlexiCommand::TrackAddEffectTrack => {
            if is_button_pressed {
                ctx.model.application_mut().add_effect_track();
            }
        }
        FlexiCommand::TrackAddInstrumentTrack => {
            if is_button_pressed {
                ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]);
            }
        }
        FlexiCommand::TrackSelectPreviousBankPage => {
            if is_button_pressed {
                ctx.model.current_track_bank_mut().select_previous_page();
            }
        }
        FlexiCommand::TrackSelectNextBankPage => {
            if is_button_pressed {
                ctx.model.current_track_bank_mut().select_next_page();
            }
        }
        FlexiCommand::TrackSelectPreviousTrack => {
            if is_button_pressed {
                let navigator = flexi(ctx).navigator.clone();
                navigate_tracks(&navigator, ctx, true);
            }
        }
        FlexiCommand::TrackSelectNextTrack => {
            if is_button_pressed {
                let navigator = flexi(ctx).navigator.clone();
                navigate_tracks(&navigator, ctx, false);
            }
        }
        FlexiCommand::TrackScrollBankPageBy1Left => {
            if is_button_pressed {
                ctx.model.current_track_bank_mut().scroll_backwards();
            }
        }
        FlexiCommand::TrackScrollBankPageBy1Right => {
            if is_button_pressed {
                ctx.model.current_track_bank_mut().scroll_forwards();
            }
        }
        FlexiCommand::TrackScrollTracks => scroll_track(ctx, knob_mode, value),
        FlexiCommand::TrackSelectedSetVolumeTrack => change_track_volume(ctx, knob_mode, -1, value),
        FlexiCommand::TrackSelectedSetPanning => change_track_panning(ctx, knob_mode, -1, value),
        FlexiCommand::TrackSelectedTogglePin => {
            if is_button_pressed {
                ctx.model.toggle_cursor_track_pinned();
            }
        }
        FlexiCommand::TrackSelectedSetPin => {
            if is_button_pressed {
                ctx.model.set_cursor_track_pinned(value.is_positive());
            }
        }
        _ => unsupported(ctx, command),
    }
}

/// Former `ToggleTrackBanksCommand.executeNormal` (Reaper has no effect track bank, it only
/// shows the notification and selects the first track if none is selected).
fn toggle_track_banks(ctx: &mut Ctx) {
    // Only the normal (not shifted) function exists
    if ctx.surface.is_shift_pressed() {
        return;
    }
    ctx.model.toggle_current_track_bank();
    let message = if ctx.model.is_effect_track_bank_active() { "Effect Tracks" } else { "Audio & Instrument Tracks" };
    ctx.notify(message);
    let track_bank = ctx.model.current_track_bank_mut();
    if track_bank.selected_item().is_none() {
        track_bank.item_mut(0).select();
    }
}

/// The index of a track in the current track bank page: the selected track for a negative index,
/// None if the track does not exist (former `getTrack`).
fn track_index(ctx: &Ctx, index: i32) -> Option<usize> {
    let track_bank = ctx.model.current_track_bank();
    if index < 0 {
        return track_bank.selected_index();
    }
    let index = index as usize;
    track_bank.item(index).does_exist().then_some(index)
}

/// Former `getSendValue`.
fn send_value(ctx: &Ctx, track_index: i32, send_index: usize) -> i32 {
    let Some(index) = self::track_index(ctx, track_index) else {
        return 0;
    };
    let send_bank = ctx.model.current_track_bank().item(index).send_bank();
    if send_index >= send_bank.page_size() {
        return 0;
    }
    send_bank.item(send_index).value()
}

fn change_track_volume(ctx: &mut Ctx, knob_mode: KnobMode, track_index: i32, value: MidiValue) {
    let Some(index) = self::track_index(ctx, track_index) else {
        return;
    };
    let changers = value_changers(ctx, knob_mode, value);
    set_or_change(ctx.model.current_track_bank_mut().item_mut(index).volume_parameter_mut(), &changers, knob_mode, value);
}

fn change_track_panning(ctx: &mut Ctx, knob_mode: KnobMode, track_index: i32, value: MidiValue) {
    let Some(index) = self::track_index(ctx, track_index) else {
        return;
    };
    let changers = value_changers(ctx, knob_mode, value);
    set_or_change(ctx.model.current_track_bank_mut().item_mut(index).pan_parameter_mut(), &changers, knob_mode, value);
}

fn change_send_volume(ctx: &mut Ctx, track_index: i32, send_index: usize, knob_mode: KnobMode, value: MidiValue) {
    let Some(index) = self::track_index(ctx, track_index) else {
        return;
    };
    let changers = value_changers(ctx, knob_mode, value);
    let send_bank = ctx.model.current_track_bank_mut().item_mut(index).send_bank_mut();
    if send_index >= send_bank.page_size() {
        return;
    }
    set_or_change(send_bank.item_mut(send_index), &changers, knob_mode, value);
}

fn scroll_track(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    if !is_absolute(knob_mode) {
        let is_left = !is_increase(ctx, knob_mode, value);
        let navigator = flexi(ctx).navigator.clone();
        navigate_tracks(&navigator, ctx, is_left);
    }
}
