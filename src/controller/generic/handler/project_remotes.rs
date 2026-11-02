// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for project remote parameter commands (former `ProjectRemotesHandler`), and the
//! implementation which is shared with the track remotes handler (the Java classes are the same
//! except for the parameter bank and the commands).

use super::{increase_knob_movement, is_absolute, is_button_pressed, is_increase, set_or_change, to_midi_value, toggle_parameter, unsupported, value_changers};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::events::ParameterBankId;
use crate::daw::parameter_bank::ParameterBank;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::parameterprovider::{parameter_bank, parameter_bank_mut};

/// The commands and the parameter bank of a remotes handler.
pub struct Remotes {
    pub category: CommandCategory,
    pub bank: ParameterBankId,
    pub set_parameter_1: FlexiCommand,
    pub toggle_parameter_1: FlexiCommand,
    pub reset_parameter_1: FlexiCommand,
    pub select_parameter_page_1: FlexiCommand,
    pub select_previous_page: FlexiCommand,
    pub select_next_page: FlexiCommand,
    pub scroll_parameter_pages: FlexiCommand,
    pub select_previous_parameter_bank: FlexiCommand,
    pub select_next_parameter_bank: FlexiCommand,
    pub scroll_parameter_banks: FlexiCommand,
    /// The notification of the previous/next page commands.
    pub notify_page: fn(&mut Ctx),
}

const PROJECT_REMOTES: Remotes = Remotes {
    category: CommandCategory::ProjectRemotes,
    bank: ParameterBankId::Project,
    set_parameter_1: FlexiCommand::ProjectSetParameter1,
    toggle_parameter_1: FlexiCommand::ProjectToggleParameter1,
    reset_parameter_1: FlexiCommand::ProjectResetParameter1,
    select_parameter_page_1: FlexiCommand::ProjectSelectParameterPage1,
    select_previous_page: FlexiCommand::ProjectSelectPreviousPage,
    select_next_page: FlexiCommand::ProjectSelectNextPage,
    scroll_parameter_pages: FlexiCommand::ProjectScrollParameterPages,
    select_previous_parameter_bank: FlexiCommand::ProjectSelectPreviousParameterBank,
    select_next_parameter_bank: FlexiCommand::ProjectSelectNextParameterBank,
    scroll_parameter_banks: FlexiCommand::ProjectScrollParameterBanks,
    notify_page: mv_helper::notify_selected_project_parameter_page,
};

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    remotes_command_value(&PROJECT_REMOTES, ctx, command)
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    remotes_handle(&PROJECT_REMOTES, ctx, command, knob_mode, value);
}

/// Former `getCommandValue` of the remotes handlers.
pub fn remotes_command_value(remotes: &Remotes, ctx: &Ctx, command: FlexiCommand) -> i32 {
    let Some(bank) = parameter_bank(&ctx.model, remotes.bank) else {
        return -1;
    };
    if let Some(index) = command.index_in(remotes.set_parameter_1, 8) {
        return bank.item(index).value();
    }
    if let Some(index) = command.index_in(remotes.toggle_parameter_1, 8) {
        return to_midi_value(bank.item(index).value() > 0);
    }
    if let Some(index) = command.index_in(remotes.select_parameter_page_1, 8) {
        // Note: as in Java it checks the parameter, not the page
        return if bank.item(index).is_selected() { 127 } else { 0 };
    }
    -1
}

fn with_bank(remotes: &Remotes, ctx: &mut Ctx, function: impl FnOnce(&mut ParameterBank)) {
    if let Some(bank) = parameter_bank_mut(&mut ctx.model, remotes.bank) {
        function(bank);
    }
}

/// Former `handle` of the remotes handlers.
pub fn remotes_handle(remotes: &Remotes, ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    if parameter_bank(&ctx.model, remotes.bank).is_none() {
        return;
    }
    let is_button_pressed = is_button_pressed(knob_mode, value);

    if let Some(index) = command.index_in(remotes.set_parameter_1, 8) {
        let changers = value_changers(ctx, knob_mode, value);
        with_bank(remotes, ctx, |bank| set_or_change(bank.item_mut(index), &changers, knob_mode, value));
        return;
    }
    if let Some(index) = command.index_in(remotes.reset_parameter_1, 8) {
        if is_button_pressed {
            with_bank(remotes, ctx, |bank| bank.item_mut(index).reset_value());
        }
        return;
    }
    if let Some(index) = command.index_in(remotes.toggle_parameter_1, 8) {
        if is_button_pressed {
            let upper_bound = ctx.model.value_changer().upper_bound();
            with_bank(remotes, ctx, |bank| toggle_parameter(upper_bound, bank.item_mut(index)));
        }
        return;
    }
    if let Some(index) = command.index_in(remotes.select_parameter_page_1, 8) {
        if is_button_pressed {
            with_bank(remotes, ctx, |bank| bank.select_page(index));
            mv_helper::notify_selected_device_and_parameter_page(ctx);
        }
        return;
    }

    if command == remotes.select_previous_page || command == remotes.select_next_page {
        if is_button_pressed {
            let forwards = command == remotes.select_next_page;
            with_bank(remotes, ctx, |bank| if forwards { bank.scroll_forwards() } else { bank.scroll_backwards() });
            (remotes.notify_page)(ctx);
        }
    } else if command == remotes.scroll_parameter_pages {
        scroll(remotes, ctx, knob_mode, value, false);
    } else if command == remotes.select_previous_parameter_bank {
        if is_button_pressed {
            with_bank(remotes, ctx, |bank| bank.select_previous_page());
        }
    } else if command == remotes.select_next_parameter_bank {
        if is_button_pressed {
            with_bank(remotes, ctx, |bank| bank.select_next_page());
        }
    } else if command == remotes.scroll_parameter_banks {
        scroll(remotes, ctx, knob_mode, value, true);
    } else {
        unsupported(ctx, command);
    }
}

/// Former `scrollParameterPage` and `scrollParameterBank`.
fn scroll(remotes: &Remotes, ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue, is_bank: bool) {
    if is_absolute(knob_mode) || !increase_knob_movement(ctx, remotes.category) {
        return;
    }
    let increase = is_increase(ctx, knob_mode, value);
    with_bank(remotes, ctx, |bank| match (is_bank, increase) {
        (false, true) => bank.scroll_forwards(),
        (false, false) => bank.scroll_backwards(),
        (true, true) => bank.select_next_page(),
        (true, false) => bank.select_previous_page(),
    });
    mv_helper::notify_selected_parameter_page(ctx);
}
