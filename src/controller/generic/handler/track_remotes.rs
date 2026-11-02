// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for the remote parameter commands of the cursor track (former
//! `TrackRemotesHandler`). The implementation is shared with the project remotes handler.

use super::project_remotes::{remotes_command_value, remotes_handle, Remotes};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::events::ParameterBankId;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

const TRACK_REMOTES: Remotes = Remotes {
    category: CommandCategory::TrackRemotes,
    bank: ParameterBankId::CursorTrack,
    set_parameter_1: FlexiCommand::TrackSetParameter1,
    toggle_parameter_1: FlexiCommand::TrackToggleParameter1,
    reset_parameter_1: FlexiCommand::TrackResetParameter1,
    select_parameter_page_1: FlexiCommand::TrackSelectParameterPage1,
    select_previous_page: FlexiCommand::TrackSelectPreviousPage,
    select_next_page: FlexiCommand::TrackSelectNextPage,
    scroll_parameter_pages: FlexiCommand::TrackScrollParameterPages,
    select_previous_parameter_bank: FlexiCommand::TrackSelectPreviousParameterBank,
    select_next_parameter_bank: FlexiCommand::TrackSelectNextParameterBank,
    scroll_parameter_banks: FlexiCommand::TrackScrollParameterBanks,
    notify_page: mv_helper::notify_selected_track_parameter_page,
};

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    remotes_command_value(&TRACK_REMOTES, ctx, command)
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    remotes_handle(&TRACK_REMOTES, ctx, command, knob_mode, value);
}
