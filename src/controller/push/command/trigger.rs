// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The trigger commands of the Push (former `controller.ableton.push.command.trigger`).

use super::modes;
use crate::config::push::{self as config, LockState, PushVersion};
use crate::config::values::{CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_1, CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_PAGE};
use crate::controller::push::setup::is_session_view;
use crate::controller::push::surface as push;
use crate::controller::push::view::is_layer_mode;
use crate::controller::push::view::session_view::SessionView;
use crate::daw::bank::BankItem;
use crate::daw::constants::Capability;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::clip::QuantizeCommand;
use crate::framework::commands::cursor::{can_scroll, Direction, ScrollStates};
use crate::framework::commands::misc::BrowserCommand;
use crate::framework::commands::transport::MetronomeCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::midi::ButtonEvent;

/// True for the send modes.
fn is_send_mode(mode: Modes) -> bool {
    (Modes::Send1..=Modes::Send8).contains(&mode)
}

/// Former `modeManager.isActive (mode) ? restore () : setTemporary (mode)`.
fn toggle_temporary_mode(ctx: &mut Ctx, mode: Modes) {
    if ctx.modes.is_active(mode) {
        ctx.restore_mode();
    } else {
        ctx.set_temporary_mode(mode);
    }
}

// ----------------------------------------------------------------------------------------------

/// Toggles the fixed accent, a long press shows the accent mode (former `AccentCommand`).
#[derive(Default)]
pub struct AccentCommand {
    quit_accent_mode: bool,
}

impl TriggerCommand for AccentCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        match event {
            ButtonEvent::Down => self.quit_accent_mode = false,
            ButtonEvent::Long => {
                self.quit_accent_mode = true;
                ctx.set_temporary_mode(Modes::Accent);
            }
            ButtonEvent::Up => {
                if self.quit_accent_mode {
                    ctx.restore_mode();
                } else {
                    let active = ctx.config.is_accent_active();
                    ctx.config.set_accent_enabled(!active);
                }
            }
        }
    }
}

/// Activates the clip mode, toggles the piano roll on the Push 2/3 (former `ClipCommand`).
#[derive(Default)]
pub struct ClipCommand;

impl TriggerCommand for ClipCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.modes.is_active(Modes::Clip) {
            if push::is_modern(ctx) {
                modes::toggle_piano_roll(ctx);
            } else {
                ctx.restore_mode();
            }
        } else {
            ctx.set_active_mode(Modes::Clip);
        }
    }
}

/// Stops the clip of the cursor track or locks the clip stop state (former `ClipStopCommand`).
#[derive(Default)]
pub struct ClipStopCommand;

impl TriggerCommand for ClipStopCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        ctx.update_view_note_mapping();

        if ctx.surface.is_select_pressed() {
            if event == ButtonEvent::Up {
                ctx.surface.set_trigger_consumed(ButtonId::Select);
                let is_shift = ctx.surface.is_shift_pressed();
                ctx.model.current_track_bank_mut().stop(is_shift);
            }
            return;
        }

        if !push::is_modern(ctx) {
            if event == ButtonEvent::Down {
                push::set_lock_state(ctx, LockState::ClipStop);
            }
            return;
        }

        if event != ButtonEvent::Up {
            return;
        }

        if ctx.surface.is_shift_pressed() || ctx.surface.is_pressed(ButtonId::LockMode) {
            let state = if push::lock_state(ctx) == LockState::ClipStop { LockState::Off } else { LockState::ClipStop };
            push::set_lock_state(ctx, state);
            return;
        }

        if push::lock_state(ctx) == LockState::ClipStop {
            return;
        }
        let is_shift = ctx.surface.is_shift_pressed();
        ctx.model.cursor_track_mut().stop(is_shift);
    }
}

/// Activates the device parameters mode (former `DeviceCommand`).
#[derive(Default)]
pub struct DeviceCommand;

impl TriggerCommand for DeviceCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            ctx.set_active_mode(Modes::DeviceParams);
        }
    }
}

/// Shows the fixed length mode (former `FixedLengthCommand`).
#[derive(Default)]
pub struct FixedLengthCommand;

impl TriggerCommand for FixedLengthCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Up {
            toggle_temporary_mode(ctx, Modes::Fixed);
        }
    }
}

/// Cycles through the layouts of the play and sequencer views (former `LayoutCommand`).
#[derive(Default)]
pub struct LayoutCommand;

impl TriggerCommand for LayoutCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let next = match ctx.views.active_id() {
            Some(Views::Play) => Some(Views::Chords),
            Some(Views::Chords) => Some(Views::Piano),
            Some(Views::Piano) => Some(Views::Drum64),
            Some(Views::Drum64) => Some(Views::Play),
            Some(Views::Sequencer) => Some(Views::Raindrops),
            Some(Views::Raindrops) => Some(Views::Drum),
            Some(Views::Drum) => Some(Views::Drum4),
            Some(Views::Drum4) => Some(Views::Drum8),
            Some(Views::Drum8) => Some(Views::Sequencer),
            _ => None,
        };
        if let Some(view) = next {
            ctx.activate_preferred_view(view);
            return;
        }
        if ctx.views.is_active(Views::Session) {
            if ctx.config.is_flip_session() {
                ctx.set_active_view(Views::ScenePlay);
            } else {
                config::set_flip_session(&mut ctx.config, true);
            }
        } else if ctx.views.is_active(Views::ScenePlay) {
            config::set_flip_session(&mut ctx.config, false);
            ctx.set_active_view(Views::Session);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.views.active_id().is_some_and(is_sequencer_view) {
            ctx.activate_preferred_view(Views::Play);
        } else if ctx.views.contains(Views::Sequencer) {
            ctx.activate_preferred_view(Views::Sequencer);
        }
    }
}

/// True for the sequencer views (former `Views.isSequencerView`).
pub fn is_sequencer_view(view: Views) -> bool {
    matches!(view, Views::Drum | Views::Drum4 | Views::Drum8 | Views::Sequencer | Views::Raindrops | Views::PolySequencer)
}

/// Selects the master track, a long press shows the frame mode (former `MastertrackCommand`).
pub struct MastertrackCommand {
    quit_master_mode: bool,
    selected_track_before_master_mode: i32,
}

impl Default for MastertrackCommand {
    fn default() -> Self {
        MastertrackCommand { quit_master_mode: false, selected_track_before_master_mode: -1 }
    }
}

impl MastertrackCommand {
    fn handle_button_up(&mut self, ctx: &mut Ctx) {
        if self.quit_master_mode {
            ctx.restore_mode();
            return;
        }
        if ctx.modes.active_id() == Some(Modes::Master) {
            if self.selected_track_before_master_mode >= 0 {
                ctx.model.current_track_bank_mut().item_mut(self.selected_track_before_master_mode as usize).select();
            }
            return;
        }
        ctx.set_active_mode(Modes::Master);
        ctx.model.master_track_mut().select();
        let cursor_track = ctx.model.cursor_track();
        self.selected_track_before_master_mode = if cursor_track.does_exist() { cursor_track.index() } else { -1 };
    }
}

impl TriggerCommand for MastertrackCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if ctx.modes.is_active(Modes::Browser) {
            return;
        }
        match event {
            ButtonEvent::Down => self.quit_master_mode = false,
            ButtonEvent::Up => self.handle_button_up(ctx),
            ButtonEvent::Long => {
                self.quit_master_mode = true;
                ctx.set_temporary_mode(Modes::Frame);
            }
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            ctx.model.transport_mut().return_to_arrangement();
        }
    }
}

/// Toggles mute or solo of the cursor track / layer / master or locks the state (former
/// `MuteCommand` and `SoloCommand`, which only differ in the state).
pub struct MuteSoloCommand {
    is_solo: bool,
}

impl MuteSoloCommand {
    pub fn mute() -> Self {
        MuteSoloCommand { is_solo: false }
    }

    pub fn solo() -> Self {
        MuteSoloCommand { is_solo: true }
    }

    fn lock(&self) -> LockState {
        if self.is_solo {
            LockState::Solo
        } else {
            LockState::Mute
        }
    }

    fn button(&self) -> ButtonId {
        if self.is_solo {
            ButtonId::Solo
        } else {
            ButtonId::Mute
        }
    }
}

impl TriggerCommand for MuteSoloCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        ctx.update_view_note_mapping();

        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            if event == ButtonEvent::Up {
                if self.is_solo {
                    ctx.model.track_bank_mut().clear_solo();
                } else {
                    ctx.model.track_bank_mut().clear_mute();
                }
            }
            return;
        }

        let lock = self.lock();
        if !push::is_modern(ctx) {
            if event == ButtonEvent::Down {
                push::set_lock_state(ctx, lock);
            }
            return;
        }

        if event == ButtonEvent::Long {
            ctx.surface.set_trigger_consumed(self.button());
            return;
        }
        if event != ButtonEvent::Up {
            return;
        }

        if ctx.surface.is_shift_pressed() || ctx.surface.is_pressed(ButtonId::LockMode) {
            let state = if push::lock_state(ctx) == lock { LockState::Off } else { lock };
            push::set_lock_state(ctx, state);
            return;
        }

        if ctx.modes.is_active(Modes::Master) {
            let master = ctx.model.master_track_mut();
            if self.is_solo {
                master.toggle_solo();
            } else {
                master.toggle_mute();
            }
            return;
        }

        if push::lock_state(ctx) == lock {
            return;
        }

        if is_layer_mode(ctx.modes.active_id()) {
            if let Some(layer) = ctx.model.cursor_device_mut().layer_bank_mut().selected_item_mut() {
                if self.is_solo {
                    layer.toggle_solo();
                } else {
                    layer.toggle_mute();
                }
            }
        } else {
            let track = ctx.model.cursor_track_mut();
            if self.is_solo {
                track.toggle_solo();
            } else {
                track.toggle_mute();
            }
        }
    }
}

/// Octave up/down of the active (transposable) view (former `OctaveCommand`).
pub struct OctaveCommand {
    is_up: bool,
}

impl OctaveCommand {
    pub fn new(is_up: bool) -> Self {
        OctaveCommand { is_up }
    }
}

impl TriggerCommand for OctaveCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        let Some(view) = ctx.active_view() else {
            return;
        };
        let Ok(mut view) = view.try_borrow_mut() else {
            return;
        };
        if let Some(transpose_view) = view.as_transpose_view_mut() {
            if self.is_up {
                transpose_view.on_octave_up(ctx, event);
            } else {
                transpose_view.on_octave_down(ctx, event);
            }
        }
    }
}

/// Scrolls the track bank page (session) or the clip page of a sequencer (former
/// `PageLeftCommand` and `PageRightCommand`).
pub struct PageCommand {
    is_left: bool,
}

impl PageCommand {
    pub fn new(is_left: bool) -> Self {
        PageCommand { is_left }
    }
}

impl TriggerCommand for PageCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.views.is_active(Views::Session) {
            if event == ButtonEvent::Down {
                let tb = ctx.model.current_track_bank_mut();
                if self.is_left {
                    tb.select_previous_page();
                } else {
                    tb.select_next_page();
                }
            }
            return;
        }
        let Some(view) = ctx.active_view() else {
            return;
        };
        let Ok(mut view) = view.try_borrow_mut() else {
            return;
        };
        if self.is_left {
            view.on_sequencer_left(ctx, event);
        } else {
            view.on_sequencer_right(ctx, event);
        }
    }
}

/// Cycles through pan and the send modes (former `PanSendCommand`, Push 1).
#[derive(Default)]
pub struct PanSendCommand;

impl TriggerCommand for PanSendCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let current_mode = ctx.modes.active_id_ignore_temporary();
        if !push::is_modern(ctx) && ctx.surface.is_select_pressed() && is_layer_mode(current_mode) {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            let mut mode = current_mode.and_then(|m| Modes::from_ordinal(m.ordinal() + 1)).unwrap_or(Modes::DeviceLayerPan);
            if mode < Modes::DeviceLayerPan || mode > Modes::DeviceLayerSend8 {
                mode = Modes::DeviceLayerPan;
            }
            ctx.set_active_mode(mode);
            return;
        }

        let mut mode = match current_mode {
            Some(m) if is_send_mode(m) => {
                let next = Modes::from_ordinal(m.ordinal() + 1).unwrap_or(Modes::Pan);
                if next > Modes::Send8 {
                    Modes::Pan
                } else {
                    next
                }
            }
            _ => Modes::Send1,
        };
        if !is_send_mode(mode) || !ctx.model.track_bank().can_edit_send(mode.ordinal() - Modes::Send1.ordinal()) {
            mode = Modes::Pan;
        }
        ctx.set_active_mode(mode);
    }
}

/// Opens the browser to add a device, Select shows the add track mode for devices (former
/// `PushAddEffectCommand`).
#[derive(Default)]
pub struct PushAddEffectCommand;

impl TriggerCommand for PushAddEffectCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Select);
            if event == ButtonEvent::Up {
                modes::set_add_mode_device(ctx);
                ctx.set_active_mode(Modes::AddTrack);
            }
            return;
        }
        // Former AddEffectCommand.execute
        if event != ButtonEvent::Down {
            return;
        }
        let is_shift = ctx.surface.is_shift_pressed();
        BrowserCommand::start_browser(ctx, true, is_shift);
    }
}

/// Toggles the automation write mode, a long press shows the automation mode (former
/// `PushAutomationCommand`).
#[derive(Default)]
pub struct PushAutomationCommand {
    quit_automation_mode: bool,
}

impl TriggerCommand for PushAutomationCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            if event == ButtonEvent::Down {
                ctx.model.transport_mut().reset_automation_overrides();
            }
            return;
        }
        match event {
            ButtonEvent::Down => self.quit_automation_mode = false,
            ButtonEvent::Long => {
                self.quit_automation_mode = true;
                ctx.set_temporary_mode(Modes::Automation);
            }
            ButtonEvent::Up => {
                if self.quit_automation_mode {
                    ctx.restore_mode();
                } else {
                    let is_shift = ctx.surface.is_shift_pressed();
                    let flip_record = ctx.config.is_flip_record();
                    if is_shift && !flip_record || !is_shift && flip_record {
                        ctx.model.transport_mut().toggle_write_clip_launcher_automation();
                    } else {
                        ctx.model.transport_mut().toggle_write_arranger_automation();
                    }
                }
            }
        }
    }
}

/// The cursor keys (former `PushCursorCommand`): up/down scroll the scenes (the 64 scenes of the
/// scene play view), left/right the pages of the active mode.
pub struct PushCursorCommand {
    direction: Direction,
}

impl PushCursorCommand {
    pub fn new(direction: Direction) -> Self {
        PushCursorCommand { direction }
    }

    fn is_scene_play(ctx: &Ctx) -> bool {
        ctx.views.is_active(Views::ScenePlay)
    }

    fn scene_option(ctx: &Ctx) -> i32 {
        if ctx.surface.is_shift_pressed() || Self::is_scene_play(ctx) {
            ctx.config.cursor_keys_scene_shifted_option()
        } else {
            ctx.config.cursor_keys_scene_option()
        }
    }

    /// Former `getSceneBank`: execute the function with the scene bank.
    fn with_scene_bank(ctx: &mut Ctx, f: impl FnOnce(&mut crate::daw::scene::SceneBank)) {
        if Self::is_scene_play(ctx) {
            if let Some(bank) = ctx.model.scene_bank_sized_mut(64) {
                f(bank);
                return;
            }
        }
        let mut bank = ctx.model.current_track_bank_mut().scene_bank_mut();
        f(&mut bank);
    }

    fn scene_bank(ctx: &Ctx) -> &crate::daw::scene::SceneBank {
        if Self::is_scene_play(ctx) {
            if let Some(bank) = ctx.model.scene_bank_sized(64) {
                return bank;
            }
        }
        ctx.model.current_track_bank().scene_bank()
    }

    /// Former `updateArrowStates`.
    pub fn arrow_states(&self, ctx: &Ctx) -> ScrollStates {
        let mut states = ScrollStates::default();
        let scene_bank = Self::scene_bank(ctx);
        match Self::scene_option(ctx) {
            CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_PAGE => {
                states.can_scroll_up = scene_bank.can_scroll_page_backwards();
                states.can_scroll_down = scene_bank.can_scroll_page_forwards();
            }
            CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_1 => {
                states.can_scroll_up = scene_bank.can_scroll_backwards();
                states.can_scroll_down = scene_bank.can_scroll_forwards();
            }
            _ => {}
        }
        let shift_pressed = ctx.surface.is_shift_pressed();
        if let Some(mode) = ctx.active_mode() {
            if let Ok(mode) = mode.try_borrow() {
                states.can_scroll_left = if shift_pressed { mode.has_previous_item(ctx) } else { mode.has_previous_item_page(ctx) };
                states.can_scroll_right = if shift_pressed { mode.has_next_item(ctx) } else { mode.has_next_item_page(ctx) };
            }
        }
        states
    }

    /// Former `canScroll`.
    pub fn can_scroll(&self, ctx: &Ctx) -> bool {
        can_scroll(&self.arrow_states(ctx), self.direction)
    }
}

impl TriggerCommand for PushCursorCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        match self.direction {
            Direction::Up | Direction::Down => {
                let is_up = self.direction == Direction::Up;
                let option = Self::scene_option(ctx);
                Self::with_scene_bank(ctx, |bank| match option {
                    CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_PAGE => {
                        if is_up {
                            bank.select_previous_page();
                        } else {
                            bank.select_next_page();
                        }
                    }
                    CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_1 => {
                        if is_up {
                            bank.scroll_backwards();
                        } else {
                            bank.scroll_forwards();
                        }
                    }
                    _ => {}
                });
            }
            Direction::Left | Direction::Right => {
                let Some(mode) = ctx.active_mode() else {
                    return;
                };
                let Ok(mut mode) = mode.try_borrow_mut() else {
                    return;
                };
                if self.direction == Direction::Left {
                    mode.select_previous_item_page(ctx);
                } else {
                    mode.select_next_item_page(ctx);
                }
            }
        }
    }
}

/// The metronome, a long press shows the transport mode (former `PushMetronomeCommand`).
pub struct PushMetronomeCommand {
    metronome: MetronomeCommand,
}

impl Default for PushMetronomeCommand {
    fn default() -> Self {
        PushMetronomeCommand { metronome: MetronomeCommand::new(false) }
    }
}

impl TriggerCommand for PushMetronomeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Long {
            ctx.surface.set_trigger_consumed(ButtonId::Metronome);
            ctx.set_temporary_mode(Modes::Transport);
            return;
        }
        if event == ButtonEvent::Up && ctx.modes.is_active(Modes::Transport) {
            ctx.restore_mode();
            return;
        }
        self.metronome.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.metronome.execute_shifted(ctx, event);
    }
}

/// Quantizes the clip, a long press (or Shift) shows the quantize mode (former
/// `PushQuantizeCommand`).
#[derive(Default)]
pub struct PushQuantizeCommand;

impl TriggerCommand for PushQuantizeCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Long || event == ButtonEvent::Down && ctx.surface.is_shift_pressed() {
            ctx.set_temporary_mode(Modes::RecArm);
            ctx.surface.set_trigger_consumed(ButtonId::Quantize);
            return;
        }
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.modes.active_id() == Some(Modes::RecArm) {
            ctx.restore_mode();
        } else {
            QuantizeCommand::quantize(ctx);
        }
    }
}

/// Shows the scales mode (former `ScalesCommand`).
#[derive(Default)]
pub struct ScalesCommand;

impl TriggerCommand for ScalesCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            toggle_temporary_mode(ctx, Modes::Scales);
        }
    }
}

/// Shows the track or layer details (former `SelectCommand`).
#[derive(Default)]
pub struct SelectCommand;

impl TriggerCommand for SelectCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        ctx.update_view_note_mapping();
        if ctx.modes.is_active(Modes::Browser) {
            return;
        }
        if event == ButtonEvent::Up {
            if ctx.modes.is_any_active(&[Modes::TrackDetails, Modes::DeviceLayerDetails]) {
                ctx.restore_mode();
            } else {
                let mode = if is_layer_mode(ctx.modes.active_id()) { Modes::DeviceLayerDetails } else { Modes::TrackDetails };
                ctx.set_temporary_mode(mode);
            }
        }
    }
}

/// Recalls the play view of the cursor track or shows the view selection (former
/// `SelectPlayViewCommand`).
#[derive(Default)]
pub struct SelectPlayViewCommand;

impl TriggerCommand for SelectPlayViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.views.active_id().is_some_and(is_session_view) {
            ctx.recall_preferred_view();
            return;
        }
        toggle_temporary_mode(ctx, Modes::ViewSelect);
    }
}

/// Activates the session view (or shows the session view selection); restores the previous view
/// if the session view was only used while the button was held (former
/// `SelectSessionViewCommand`).
#[derive(Default)]
pub struct SelectSessionViewCommand {
    is_temporary: bool,
}

impl SelectSessionViewCommand {
    /// Takes the temporary flag of the session view (former `setTemporary`).
    fn sync_temporary(&mut self, ctx: &mut Ctx) {
        if let Some(view) = ctx.views.get(Views::Session) {
            if let Ok(mut view) = view.try_borrow_mut() {
                if let Some(session) = view.as_any_mut().downcast_mut::<SessionView>() {
                    if session.take_temporary_request() {
                        self.is_temporary = true;
                    }
                }
            }
        }
    }
}

impl TriggerCommand for SelectSessionViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        self.sync_temporary(ctx);
        if event == ButtonEvent::Down {
            self.is_temporary = false;
            if ctx.views.active_id().is_some_and(is_session_view) {
                toggle_temporary_mode(ctx, Modes::SessionViewSelect);
                return;
            }
            let view = if config::is_scenes_clip_view_selected(&ctx.config) { Views::ScenePlay } else { Views::Session };
            ctx.set_active_view(view);
            return;
        }
        if event == ButtonEvent::Up && self.is_temporary {
            ctx.restore_view();
        }
    }
}

/// Shows the setup mode (Push 2/3) or the user/configuration mode (Push 1) (former
/// `SetupCommand`).
pub struct SetupCommand {
    version: PushVersion,
}

impl SetupCommand {
    pub fn new(version: PushVersion) -> Self {
        SetupCommand { version }
    }

    fn mode(&self, ctx: &Ctx) -> Modes {
        if self.version != PushVersion::Version1 {
            return Modes::Setup;
        }
        if ctx.surface.is_shift_pressed() {
            Modes::Configuration
        } else {
            Modes::User
        }
    }
}

impl TriggerCommand for SetupCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event == ButtonEvent::Down {
            let mode = self.mode(ctx);
            toggle_temporary_mode(ctx, mode);
        }
    }
}

/// Shift: shows the scale layout in the scales mode and switches the knob sensitivity (former
/// `ShiftCommand` of the Push).
#[derive(Default)]
pub struct PushShiftCommand;

impl TriggerCommand for PushShiftCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        let cm = ctx.modes.active_id();
        if event == ButtonEvent::Down && cm == Some(Modes::Scales) {
            ctx.set_temporary_mode(Modes::ScaleLayout);
        } else if event == ButtonEvent::Up && cm == Some(Modes::ScaleLayout) {
            ctx.restore_mode();
        }
        let is_slow = ctx.surface.is_shift_pressed();
        ctx.surface.set_knob_sensitivity_is_slow(is_slow);
    }
}

/// Activates the mix modes, toggles the track banks; Shift toggles the VU meters (former
/// `TrackCommand`).
#[derive(Default)]
pub struct TrackCommand;

impl TriggerCommand for TrackCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }

        if ctx.surface.is_shift_pressed() {
            let enabled = ctx.config.is_enable_vu_meters();
            ctx.config.set_vu_meters_enabled(!enabled);
            return;
        }

        match ctx.modes.active_id() {
            Some(current_mode) => {
                if push::is_modern(ctx) {
                    if matches!(current_mode, Modes::Track | Modes::Volume | Modes::Crossfader | Modes::Pan) {
                        ctx.model.toggle_current_track_bank();
                    } else if is_send_mode(current_mode) {
                        ctx.set_active_mode(Modes::Track);
                        ctx.model.toggle_current_track_bank();
                    } else if let Some(mode) = config::current_mix_mode(&ctx.config) {
                        ctx.set_active_mode(mode);
                    }
                } else {
                    if ctx.surface.is_select_pressed() && is_layer_mode(Some(current_mode)) {
                        ctx.surface.set_trigger_consumed(ButtonId::Select);
                        ctx.set_active_mode(Modes::DeviceLayer);
                        return;
                    }
                    if current_mode == Modes::Track {
                        ctx.model.toggle_current_track_bank();
                    } else {
                        ctx.set_active_mode(Modes::Track);
                    }
                }
            }
            None => ctx.set_active_mode(Modes::Track),
        }

        if let Some(mode) = ctx.modes.active_id() {
            config::set_mixer_mode(&mut ctx.config, mode);
        }
        let tb = ctx.model.current_track_bank_mut();
        if tb.selected_item().is_none() {
            tb.item_mut(0).select();
        }
    }
}

/// Activates the volume mode, the crossfader mode if it is active (former `VolumeCommand`, Push
/// 1).
#[derive(Default)]
pub struct VolumeCommand;

impl TriggerCommand for VolumeCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let current_mode = ctx.modes.active_id();
        if !push::is_modern(ctx) && ctx.surface.is_select_pressed() && is_layer_mode(current_mode) {
            ctx.set_active_mode(Modes::DeviceLayerVolume);
            return;
        }
        if current_mode == Some(Modes::Volume) {
            if crate::controller::push::mode::track::host_supports(ctx, Capability::HasCrossfader) {
                ctx.set_active_mode(Modes::Crossfader);
            }
        } else {
            ctx.set_active_mode(Modes::Volume);
        }
    }
}
