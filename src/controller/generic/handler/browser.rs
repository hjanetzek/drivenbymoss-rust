// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for browser commands (former `BrowserHandler`).

use super::{increase_knob_movement, is_absolute, is_button_pressed, is_increase, unsupported};
use crate::controller::generic::command::{CommandCategory, FlexiCommand};
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::browser::Browser;
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(_ctx: &Ctx, _command: FlexiCommand) -> i32 {
    -1
}

fn with_browser(ctx: &mut Ctx, function: impl FnOnce(&mut Browser)) {
    if let Some(browser) = ctx.model.browser_mut() {
        function(browser);
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    let is_button_pressed = is_button_pressed(knob_mode, value);

    if let Some(column) = command.index_in(FlexiCommand::BrowserScrollFilterInColumn1, 8) {
        scroll_filter_column(ctx, knob_mode, column, value);
        return;
    }
    match command {
        FlexiCommand::BrowserScrollPresets => {
            scroll_preset_column(ctx, knob_mode, value);
            return;
        }
        FlexiCommand::BrowserScrollTabs => {
            scroll_browser_tabs(ctx, knob_mode, value);
            return;
        }
        _ => {}
    }

    if !is_button_pressed {
        return;
    }
    if let Some(column) = command.index_in(FlexiCommand::BrowserSelectPreviousFilterInColumn1, 8) {
        with_browser(ctx, |browser| browser.select_previous_filter_item(column));
        return;
    }
    if let Some(column) = command.index_in(FlexiCommand::BrowserSelectNextFilterInColumn1, 8) {
        with_browser(ctx, |browser| browser.select_next_filter_item(column));
        return;
    }
    if let Some(column) = command.index_in(FlexiCommand::BrowserResetFilterColumn1, 8) {
        with_browser(ctx, |browser| browser.reset_filter_column(column));
        return;
    }

    let device_name = ctx.model.cursor_device().name().to_string();
    let device_position = ctx.model.cursor_device().position();
    match command {
        FlexiCommand::BrowserBrowsePresets => ctx.model.browse_cursor_device_presets(),
        FlexiCommand::BrowserInsertDeviceBeforeCurrent => with_browser(ctx, |browser| browser.insert_before_cursor_device(&device_name, device_position)),
        FlexiCommand::BrowserInsertDeviceAfterCurrent => with_browser(ctx, |browser| browser.insert_after_cursor_device(&device_name, device_position)),
        FlexiCommand::BrowserCommitSelection => with_browser(ctx, |browser| browser.stop_browsing(true)),
        FlexiCommand::BrowserCancelSelection => with_browser(ctx, |browser| browser.stop_browsing(false)),
        FlexiCommand::BrowserSelectThePreviousPreset => with_browser(ctx, |browser| browser.select_previous_result()),
        FlexiCommand::BrowserSelectTheNextPreset => with_browser(ctx, |browser| browser.select_next_result()),
        FlexiCommand::BrowserSelectThePreviousTab => with_browser(ctx, |browser| browser.previous_content_type()),
        FlexiCommand::BrowserSelectTheNextTab => with_browser(ctx, |browser| browser.next_content_type()),
        _ => unsupported(ctx, command),
    }
}

fn scroll_filter_column(ctx: &mut Ctx, knob_mode: KnobMode, filter_column: usize, value: MidiValue) {
    if is_absolute(knob_mode) {
        return;
    }
    let increase = is_increase(ctx, knob_mode, value);
    with_browser(ctx, |browser| if increase { browser.select_next_filter_item(filter_column) } else { browser.select_previous_filter_item(filter_column) });
}

fn scroll_preset_column(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) {
        return;
    }
    let increase = is_increase(ctx, knob_mode, value);
    with_browser(ctx, |browser| if increase { browser.select_next_result() } else { browser.select_previous_result() });
}

fn scroll_browser_tabs(ctx: &mut Ctx, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) || !increase_knob_movement(ctx, CommandCategory::Browser) {
        return;
    }
    let increase = is_increase(ctx, knob_mode, value);
    with_browser(ctx, |browser| if increase { browser.next_content_type() } else { browser.previous_content_type() });
}
