// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Further generic commands (former `OverdubCommand`, `PaneCommand`, `LayoutCommand`,
//! `PanelLayoutCommand`, `BrowserCommand`, `AddEffectCommand`, `AddTrackCommand`,
//! `ConvertCommand`, `DoubleCommand`, `StartSceneCommand` and `FootswitchCommand`).

use super::application::UndoCommand;
use super::clip::NewCommand;
use super::mv_helper;
use super::transport::{ConfiguredRecordCommand, PlayCommand};
use super::transport_ext::TapTempoCommand;
use crate::config::values::*;
use crate::daw::application::{PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_EDIT, PANEL_LAYOUT_MIX};
use crate::daw::bank::BankItem;
use crate::daw::browser::DeviceChainTarget;
use crate::daw::constants::ChannelType;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::{ButtonEvent, CMD_CC};

/// Toggles the arranger overdub, with Shift the launcher overdub, with Select the clip launcher
/// automation (former `OverdubCommand`).
#[derive(Default)]
pub struct OverdubCommand;

impl OverdubCommand {
    pub fn new() -> Self {
        OverdubCommand
    }

    /// Former `isActive`.
    pub fn is_active(ctx: &Ctx) -> bool {
        if ctx.surface.is_select_pressed() {
            return ctx.model.transport().is_writing_clip_launcher_automation();
        }
        if ctx.surface.is_shift_pressed() {
            ctx.model.is_launcher_overdub()
        } else {
            ctx.model.is_arranger_overdub()
        }
    }
}

impl TriggerCommand for OverdubCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_select_pressed() {
            ctx.model.transport_mut().toggle_write_clip_launcher_automation();
        } else {
            ctx.model.toggle_overdub();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.toggle_launcher_overdub();
        }
    }
}

/// The panels which can be toggled by [`PaneCommand`] (former `PaneCommand.Panels`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panels {
    Note,
    Automation,
    Device,
    Mixer,
}

/// Toggles a panel of the application (former `PaneCommand`).
pub struct PaneCommand {
    panel: Panels,
}

impl PaneCommand {
    pub fn new(panel: Panels) -> Self {
        PaneCommand { panel }
    }
}

impl TriggerCommand for PaneCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let application = ctx.model.application_mut();
        match self.panel {
            Panels::Note => application.toggle_note_editor(),
            Panels::Automation => application.toggle_automation_editor(),
            Panels::Device => application.toggle_devices(),
            Panels::Mixer => application.toggle_mixer(),
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.application_mut().toggle_devices();
        }
    }
}

/// Selects a panel layout or the next one (former `LayoutCommand`).
pub struct LayoutCommand {
    layout: Option<&'static str>,
}

impl LayoutCommand {
    /// Selects the next panel layout.
    pub fn new() -> Self {
        LayoutCommand { layout: None }
    }

    pub fn with_layout(layout: &'static str) -> Self {
        LayoutCommand { layout: Some(layout) }
    }
}

impl Default for LayoutCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for LayoutCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let application = ctx.model.application_mut();
        match self.layout {
            None => application.next_panel_layout(),
            Some(layout) => application.set_panel_layout(layout),
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.application_mut().set_panel_layout(PANEL_LAYOUT_EDIT);
        }
    }
}

/// Toggles the device window, with Select expands the device, with Shift the next panel layout
/// (former `PanelLayoutCommand`).
#[derive(Default)]
pub struct PanelLayoutCommand;

impl PanelLayoutCommand {
    pub fn new() -> Self {
        PanelLayoutCommand
    }
}

impl TriggerCommand for PanelLayoutCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.surface.is_select_pressed() {
            if event == ButtonEvent::Down {
                ctx.model.cursor_device_mut().toggle_expanded();
            }
            return;
        }
        if ctx.surface.is_shift_pressed() {
            self.execute_shifted(ctx, event);
        } else {
            self.execute_normal(ctx, event);
        }
    }

    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.cursor_device_mut().toggle_window_open();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.application_mut().next_panel_layout();
        }
    }
}

/// Opens (or closes) the browser to replace or insert a device (former `BrowserCommand`).
pub struct BrowserCommand {
    first_trigger: ButtonId,
    second_trigger: Option<ButtonId>,
    /// If true, only inserts a device (former `AddEffectCommand`).
    add_effect: bool,
}

impl BrowserCommand {
    /// Former `BrowserCommand (model, surface)` with Shift and Select as triggers.
    pub fn new() -> Self {
        Self::with_triggers(ButtonId::Shift, Some(ButtonId::Select))
    }

    pub fn with_triggers(first_trigger: ButtonId, second_trigger: Option<ButtonId>) -> Self {
        BrowserCommand { first_trigger, second_trigger, add_effect: false }
    }

    /// Former `startBrowser`.
    pub fn start_browser(ctx: &mut Ctx, insert_device: bool, before_current: bool) {
        let Some(is_active) = ctx.model.browser().map(|b| b.is_active()) else {
            return;
        };
        // Patch Browser already active?
        if is_active {
            let commit = !ctx.surface.is_shift_pressed();
            Self::discard_browser(ctx, commit);
            return;
        }
        Self::activate_browser(ctx, insert_device, before_current);
    }

    fn activate_browser(ctx: &mut Ctx, insert_device: bool, before_current: bool) -> bool {
        let device = ctx.model.cursor_device();
        let has_cursor_device = device.does_exist();
        let device_name = device.name().to_string();
        let device_position = device.position();
        if has_cursor_device {
            let Some(browser) = ctx.model.browser_mut() else {
                return false;
            };
            // Replace the cursor device
            if !insert_device {
                browser.replace_device(&device_name, device_position);
                return true;
            }
            // Reaper has no device layers, therefore adding a device to a layer is not possible
            if before_current {
                browser.insert_before_cursor_device(&device_name, device_position);
            } else {
                browser.insert_after_cursor_device(&device_name, device_position);
            }
            return true;
        }

        // No cursor device, add to the selected channel, if any
        let channel = match ctx.model.current_track_bank().selected_item() {
            Some(track) => Some((track.name().to_string(), DeviceChainTarget::Track(track.position()))),
            None if ctx.model.master_track().is_selected() => Some((ctx.model.master_track().name().to_string(), DeviceChainTarget::Master)),
            None => None,
        };
        let Some((channel_name, target)) = channel else {
            return false;
        };
        if let Some(browser) = ctx.model.browser_mut() {
            browser.add_device(&channel_name, target);
        }
        true
    }

    /// Former `discardBrowser`.
    pub fn discard_browser(ctx: &mut Ctx, commit: bool) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.stop_browsing(commit);
        }
    }
}

impl Default for BrowserCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for BrowserCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if self.add_effect {
            // Former AddEffectCommand.executeNormal (AbstractTriggerCommand.execute)
            if event == ButtonEvent::Up && !ctx.surface.is_shift_pressed() {
                Self::start_browser(ctx, true, false);
            }
            return;
        }
        if event != ButtonEvent::Up {
            return;
        }
        if let Some(second) = self.second_trigger {
            if ctx.surface.is_pressed(self.first_trigger) {
                Self::start_browser(ctx, true, true);
            } else {
                let insert = ctx.surface.is_pressed(second);
                Self::start_browser(ctx, insert, false);
            }
            return;
        }
        let insert = ctx.surface.is_pressed(self.first_trigger);
        Self::start_browser(ctx, insert, false);
    }
}

/// Opens the browser to insert a device after the cursor device (former `AddEffectCommand`).
pub struct AddEffectCommand;

impl AddEffectCommand {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> BrowserCommand {
        BrowserCommand { first_trigger: ButtonId::Shift, second_trigger: Some(ButtonId::Select), add_effect: true }
    }
}

/// Adds an instrument track, with the first combination button an effect track, with the second
/// an audio track (former `AddTrackCommand`).
pub struct AddTrackCommand {
    combi1: Option<ButtonId>,
    combi2: Option<ButtonId>,
}

impl AddTrackCommand {
    pub fn new() -> Self {
        Self::with_combinations(Some(ButtonId::Shift), Some(ButtonId::Select))
    }

    pub fn with_combinations(combi1: Option<ButtonId>, combi2: Option<ButtonId>) -> Self {
        AddTrackCommand { combi1, combi2 }
    }
}

impl Default for AddTrackCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerCommand for AddTrackCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if self.combi1.is_some_and(|b| ctx.surface.is_pressed(b)) {
            ctx.model.application_mut().add_effect_track();
            // There is no effect track bank in Reaper
            return;
        } else if self.combi2.is_some_and(|b| ctx.surface.is_pressed(b)) {
            ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]);
        } else {
            ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]);
        }
        ctx.schedule(200, |ctx| {
            let bank = ctx.model.track_bank_mut();
            let count = bank.item_count();
            if count == 0 {
                ctx.host.error("Warning: No track created.");
                return;
            }
            let pos = count - 1;
            bank.scroll_to(pos, true);
            let page_size = bank.page_size().max(1);
            bank.item_mut(pos % page_size).select();
        });
    }
}

/// Slices the clip to a drum machine (sampler with Shift) (former `ConvertCommand`).
#[derive(Default)]
pub struct ConvertCommand;

impl ConvertCommand {
    pub fn new() -> Self {
        ConvertCommand
    }
}

impl TriggerCommand for ConvertCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down && ctx.model.can_convert_clip() {
            ctx.model.application_mut().slice_to_drum_machine();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down && ctx.model.can_convert_clip() {
            ctx.model.application_mut().slice_to_sampler();
        }
    }
}

/// Doubles the content of the cursor clip (former `DoubleCommand`).
#[derive(Default)]
pub struct DoubleCommand {
    on_shift: bool,
}

impl DoubleCommand {
    pub fn new(on_shift: bool) -> Self {
        DoubleCommand { on_shift }
    }

    fn double_clip(ctx: &mut Ctx) {
        if let Some(clip) = ctx.model.cursor_clip() {
            if clip.does_exist() {
                clip.duplicate_content();
            }
        }
    }
}

impl TriggerCommand for DoubleCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down && !self.on_shift {
            Self::double_clip(ctx);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down && self.on_shift {
            Self::double_clip(ctx);
        }
    }
}

/// Launches a scene of the scene bank of the model (the selected scene with a negative index)
/// (former `StartSceneCommand`).
pub struct StartSceneCommand {
    pub index: i32,
}

impl StartSceneCommand {
    pub fn new(index: i32) -> Self {
        StartSceneCommand { index }
    }

    fn scene_index(&self, ctx: &Ctx) -> Option<usize> {
        if self.index >= 0 {
            return Some(self.index as usize);
        }
        let bank = ctx.model.scene_bank();
        (0..bank.page_size()).find(|i| bank.item(*i).is_selected())
    }
}

impl TriggerCommand for StartSceneCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Long {
            return;
        }
        let Some(index) = self.scene_index(ctx) else {
            return;
        };
        if !ctx.model.scene_bank().item(index).does_exist() {
            return;
        }
        let is_down = event == ButtonEvent::Down;
        if is_down {
            // Delete the scene
            if ctx.surface.is_delete_pressed() {
                ctx.model.scene_bank_mut().item_mut(index).remove();
                return;
            }
            if ctx.config.is_select_clip_on_launch() {
                ctx.model.scene_bank_mut().item_mut(index).select();
                mv_helper::delay_display(ctx, move |ctx| ctx.model.scene_bank().item(index).name());
            }
        }
        ctx.model.scene_bank_mut().item_mut(index).launch(is_down, false);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.project_mut().create_scene_from_playing_launcher_clips();
        }
    }
}

/// Executes the function which is configured for a footswitch (former `FootswitchCommand`).
pub struct FootswitchCommand {
    pub index: usize,
    new_command: NewCommand,
    record_command: ConfiguredRecordCommand,
    undo_command: UndoCommand,
    tap_tempo_command: TapTempoCommand,
    play_command: PlayCommand,
}

impl FootswitchCommand {
    pub fn new(index: usize) -> Self {
        FootswitchCommand {
            index,
            new_command: NewCommand::new(),
            record_command: ConfiguredRecordCommand::new(),
            undo_command: UndoCommand::new(),
            tap_tempo_command: TapTempoCommand::new(),
            play_command: PlayCommand::new(),
        }
    }

    /// Former `getSetting`.
    pub fn setting(&self, ctx: &Ctx) -> i32 {
        ctx.config.footswitch(self.index)
    }

    fn handle_view_command(&mut self, ctx: &mut Ctx, event: ButtonEvent, setting: i32) -> bool {
        match setting {
            FOOTSWITCH_TOGGLE_PLAY => self.play_command.execute(ctx, event, 127),
            FOOTSWITCH_TOGGLE_RECORD => self.record_command.execute(ctx, event, 127),
            FOOTSWITCH_UNDO => self.undo_command.execute(ctx, event, 127),
            FOOTSWITCH_TAP_TEMPO => self.tap_tempo_command.execute(ctx, event, 127),
            FOOTSWITCH_NEW_BUTTON => self.new_command.execute(ctx, event, 127),
            FOOTSWITCH_CLIP_BASED_LOOPER => self.handle_looper(ctx, event),
            _ => return false,
        }
        true
    }

    fn handle_looper(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Long {
            return;
        }
        if !ctx.model.cursor_track().does_exist() {
            ctx.notify("Please select an Instrument track first.");
            return;
        }
        let slot_bank = ctx.model.cursor_track().slot_bank();
        let slot_index = slot_bank.selected_item().map(|s| s.index().max(0) as usize).unwrap_or(0);
        let has_content = slot_bank.item(slot_index).has_content();
        let is_down = event == ButtonEvent::Down;
        if is_down && !has_content {
            // If there is no clip in the selected slot, create a clip and begin record mode.
            // Releasing it ends record mode.
            self.new_command.execute_now(ctx);
            ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index).select();
        }
        ctx.model.set_launcher_overdub(is_down);
        // Start transport if not already playing
        ctx.model.cursor_track_mut().slot_bank_mut().item_mut(slot_index).launch(true, false);
    }
}

impl TriggerCommand for FootswitchCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        let setting = self.setting(ctx);
        self.execute_setting(ctx, event, setting);
    }
}

impl FootswitchCommand {
    /// Execute the function with the given index (one of `FOOTSWITCH_VALUES`), for commands which
    /// read the function from another setting (former overrides of `getSetting`, e.g. the
    /// assignable buttons of the Mackie MCU/HUI).
    pub fn execute_setting(&mut self, ctx: &mut Ctx, event: ButtonEvent, setting: i32) {
        if self.handle_view_command(ctx, event, setting) || event == ButtonEvent::Long {
            return;
        }
        let is_down = event == ButtonEvent::Down;
        match setting {
            FOOTSWITCH_STOP_ALL_CLIPS => {
                if is_down {
                    ctx.model.current_track_bank_mut().stop(false);
                }
            }
            FOOTSWITCH_TOGGLE_CLIP_OVERDUB => {
                if is_down {
                    ctx.model.toggle_launcher_overdub();
                }
            }
            FOOTSWITCH_PANEL_LAYOUT_ARRANGE => {
                if is_down {
                    ctx.model.application_mut().set_panel_layout(PANEL_LAYOUT_ARRANGE);
                }
            }
            FOOTSWITCH_PANEL_LAYOUT_MIX => {
                if is_down {
                    ctx.model.application_mut().set_panel_layout(PANEL_LAYOUT_MIX);
                }
            }
            FOOTSWITCH_PANEL_LAYOUT_EDIT => {
                if is_down {
                    ctx.model.application_mut().set_panel_layout(PANEL_LAYOUT_EDIT);
                }
            }
            FOOTSWITCH_ADD_INSTRUMENT_TRACK => {
                if is_down {
                    ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]);
                }
            }
            FOOTSWITCH_ADD_AUDIO_TRACK => {
                if is_down {
                    ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]);
                }
            }
            FOOTSWITCH_ADD_EFFECT_TRACK => {
                if is_down {
                    ctx.model.application_mut().add_effect_track();
                }
            }
            FOOTSWITCH_QUANTIZE => {
                if is_down {
                    let amount = ctx.config.quantize_amount() as f64 / 100.0;
                    if let Some(clip) = ctx.model.cursor_clip() {
                        if clip.does_exist() {
                            clip.quantize(amount);
                        }
                    }
                }
            }
            FOOTSWITCH_SUSTAIN_PEDAL => ctx.surface.send_midi_event(CMD_CC, 64, if is_down { 127 } else { 0 }),
            _ => ctx.host.error(&format!("Unknown footswitch command called: {setting}")),
        }
    }
}
