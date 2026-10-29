// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the Maschine JAM (former package `controller.ni.maschine.jam.command.trigger`).
//! Most of them wrap a generic command and consume the Shift button after its use (the Shift
//! button of the JAM is not a real button but sent via system exclusive messages).

use crate::config::maschine_jam::MaschineJamConfiguration;
use crate::config::values::NEW_CLIP_LENGTH_VALUES;
use crate::controller::maschine::core::colors::{dim_or_highlight_color, COLOR_BLUE, COLOR_GREEN, COLOR_GREY};
use crate::controller::maschine::core::encoder_mode_manager::{EncoderMode, SharedEncoderModeManager};
use crate::daw::application::{PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_MIX};
use crate::daw::bank::BankItem;
use crate::daw::constants::GrooveParameterId;
use crate::framework::color::BUTTON_STATE_OFF;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::clip::QuantizeCommand;
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::misc::{BrowserCommand, DoubleCommand, StartSceneCommand};
use crate::framework::commands::mode::{ModeMultiSelectCommand, ModeSelectCommand};
use crate::framework::commands::mv_helper::delay_display;
use crate::framework::commands::transport::{ConfiguredRecordCommand, PlayCommand};
use crate::framework::commands::transport_ext::{AutomationCommand, TapTempoCommand};
use crate::framework::commands::view::ViewButtonCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::midi::ButtonEvent;

use super::mode::toggle_user_mode;

/// Former `MaschineJamAutomationCommand`.
pub struct MaschineJamAutomationCommand {
    inner: AutomationCommand,
}

impl MaschineJamAutomationCommand {
    pub fn new() -> Self {
        MaschineJamAutomationCommand { inner: AutomationCommand::new() }
    }
}

impl TriggerCommand for MaschineJamAutomationCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        self.inner.execute(ctx, event, velocity);
        if event == ButtonEvent::Down && ctx.surface.is_shift_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Former `MaschineJamAuxCommand`: selects the send modes.
pub struct MaschineJamAuxCommand {
    inner: ModeMultiSelectCommand,
}

impl MaschineJamAuxCommand {
    pub fn new() -> Self {
        let modes: Vec<Modes> = (0..8).map(|i| Modes::get(Modes::Send1, i)).collect();
        MaschineJamAuxCommand { inner: ModeMultiSelectCommand::new(&modes) }
    }
}

impl TriggerCommand for MaschineJamAuxCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Select) {
            ctx.model.application_mut().add_effect_track();
        } else {
            self.inner.execute_normal(ctx, event);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        ctx.model.toggle_current_track_bank();
        delay_display(ctx, |ctx| if ctx.model.is_effect_track_bank_active() { "Effect Tracks" } else { "Audio/Instrument Tracks" }.to_string());
        ctx.surface.set_trigger_consumed(ButtonId::Shift);
    }
}

/// Former `MaschineJamControlCommand`: selects the parameter and the user mode.
pub struct MaschineJamControlCommand {
    inner: ModeMultiSelectCommand,
}

impl MaschineJamControlCommand {
    pub fn new() -> Self {
        MaschineJamControlCommand { inner: ModeMultiSelectCommand::new(&[Modes::DeviceParams, Modes::User]) }
    }
}

impl TriggerCommand for MaschineJamControlCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Select) {
            toggle_user_mode(ctx);
            return;
        }
        if ctx.surface.is_shift_pressed() {
            self.execute_shifted(ctx, event);
        } else {
            self.execute_normal(ctx, event);
        }
    }

    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        ctx.model.cursor_device_mut().toggle_window_open();
        ctx.surface.set_trigger_consumed(ButtonId::Shift);
    }
}

/// Former `MaschineJamDoubleCommand`.
pub struct MaschineJamDoubleCommand {
    inner: DoubleCommand,
}

impl MaschineJamDoubleCommand {
    pub fn new() -> Self {
        MaschineJamDoubleCommand { inner: DoubleCommand::new(true) }
    }
}

impl TriggerCommand for MaschineJamDoubleCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_shifted(ctx, event);
        if event == ButtonEvent::Down && ctx.surface.is_shift_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Former `MaschineJamLevelCommand`: selects the volume and the panorama mode.
pub struct MaschineJamLevelCommand {
    inner: ModeMultiSelectCommand,
}

impl MaschineJamLevelCommand {
    pub fn new() -> Self {
        MaschineJamLevelCommand { inner: ModeMultiSelectCommand::new(&[Modes::Volume, Modes::Pan]) }
    }
}

impl TriggerCommand for MaschineJamLevelCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Select) {
            ctx.model.application_mut().add_audio_track();
        } else {
            self.inner.execute_normal(ctx, event);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_shifted(ctx, event);
    }
}

/// Former `MaschineJamMacroCommand`: selects the track mode.
pub struct MaschineJamMacroCommand {
    inner: ModeSelectCommand,
}

impl MaschineJamMacroCommand {
    pub fn new() -> Self {
        MaschineJamMacroCommand { inner: ModeSelectCommand::new(Modes::Track) }
    }
}

impl TriggerCommand for MaschineJamMacroCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_pressed(ButtonId::Select) {
            ctx.model.application_mut().add_instrument_track();
        } else {
            self.inner.execute_normal(ctx, event);
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_shifted(ctx, event);
    }
}

/// Former `MaschineJamMuteCommand`.
pub struct MaschineJamMuteCommand;

impl TriggerCommand for MaschineJamMuteCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up && ctx.surface.is_pressed(ButtonId::Select) {
            ctx.model.track_bank_mut().clear_mute();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.track_bank_mut().stop(false);
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Former `MaschineJamSoloCommand`.
pub struct MaschineJamSoloCommand;

impl TriggerCommand for MaschineJamSoloCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up && ctx.surface.is_pressed(ButtonId::Select) {
            ctx.model.track_bank_mut().clear_solo();
        }
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.config.next_new_clip_length();
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
            delay_display(ctx, |ctx| NEW_CLIP_LENGTH_VALUES[ctx.config.new_clip_length()].to_string());
        }
    }
}

/// Former `MaschineJamPageLeftCommand` and `MaschineJamPageRightCommand`: select the previous /
/// next page of the active mode, with Shift toggle the metronome / loop.
pub struct MaschineJamPageCommand {
    inner: CursorCommand,
    is_left: bool,
}

impl MaschineJamPageCommand {
    pub fn new_left() -> Self {
        MaschineJamPageCommand { inner: CursorCommand::new_mode_cursor(Direction::Down, false), is_left: true }
    }

    pub fn new_right() -> Self {
        MaschineJamPageCommand { inner: CursorCommand::new_mode_cursor(Direction::Up, false), is_left: false }
    }
}

impl TriggerCommand for MaschineJamPageCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_shift_pressed() {
            if self.is_left {
                ctx.model.transport_mut().toggle_metronome();
            } else {
                ctx.model.transport_mut().toggle_loop();
            }
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        } else {
            self.inner.execute(ctx, event, velocity);
        }
    }
}

/// Former `MaschineJamPlayCommand`: with Shift the playback is restarted.
pub struct MaschineJamPlayCommand {
    inner: PlayCommand,
}

impl MaschineJamPlayCommand {
    pub fn new() -> Self {
        MaschineJamPlayCommand { inner: PlayCommand::new() }
    }
}

impl TriggerCommand for MaschineJamPlayCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Down && !ctx.surface.is_pressed(ButtonId::Select) {
            ctx.model.transport_mut().restart();
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
            return;
        }
        self.inner.execute_shifted(ctx, event);
    }
}

/// Former `MaschineJamQuantizeCommand`.
pub struct MaschineJamQuantizeCommand {
    inner: QuantizeCommand,
}

impl MaschineJamQuantizeCommand {
    pub fn new() -> Self {
        MaschineJamQuantizeCommand { inner: QuantizeCommand::new() }
    }
}

impl TriggerCommand for MaschineJamQuantizeCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.inner.execute_shifted(ctx, event);
        if event == ButtonEvent::Up {
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Former `MaschineJamRecordCommand`.
pub struct MaschineJamRecordCommand {
    inner: ConfiguredRecordCommand,
}

impl MaschineJamRecordCommand {
    pub fn new() -> Self {
        MaschineJamRecordCommand { inner: ConfiguredRecordCommand::new() }
    }
}

impl TriggerCommand for MaschineJamRecordCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        self.inner.execute(ctx, event, velocity);
        if event == ButtonEvent::Up {
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Former `MaschineJamSelectCommand`: Shift + Select toggles the slow fader change.
pub struct MaschineJamSelectCommand;

impl TriggerCommand for MaschineJamSelectCommand {
    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            MaschineJamConfiguration::toggle_slow_fader_change(&mut ctx.config);
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Former `MaschineJamSessionViewCommand`: activates the session view, toggles the flipped
/// session and the birds eye view.
pub struct MaschineJamSessionViewCommand;

impl MaschineJamSessionViewCommand {
    fn notify_view_name(ctx: &mut Ctx, is_birds_eye: bool, is_flipped: bool) {
        if is_birds_eye {
            ctx.notify("Session - Birds Eye");
        } else {
            ctx.notify(if is_flipped { "Session - Flipped" } else { "Session" });
        }
    }

    fn with_session_view<R>(ctx: &Ctx, f: impl FnOnce(&mut dyn crate::framework::feature::View) -> R) -> Option<R> {
        let view = ctx.views.get(Views::Session)?;
        let mut view = view.try_borrow_mut().ok()?;
        Some(f(&mut *view))
    }
}

impl TriggerCommand for MaschineJamSessionViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        match event {
            ButtonEvent::Up => {
                if ctx.surface.is_shift_pressed() {
                    let application = ctx.model.application_mut();
                    let is_arrange = application.panel_layout() == PANEL_LAYOUT_ARRANGE;
                    application.set_panel_layout(if is_arrange { PANEL_LAYOUT_MIX } else { PANEL_LAYOUT_ARRANGE });
                    ctx.surface.set_trigger_consumed(ButtonId::Shift);
                    return;
                }

                if ctx.views.is_active(Views::Session) {
                    let is_birds_eye = Self::with_session_view(ctx, |v| v.is_birds_eye_active()).unwrap_or(false);
                    if is_birds_eye {
                        Self::with_session_view(ctx, |v| v.set_birds_eye_active(false));
                        let flipped = ctx.config.is_flip_session();
                        Self::notify_view_name(ctx, false, flipped);
                        return;
                    }
                    let flipped = !ctx.config.is_flip_session();
                    ctx.config.set_flip_session(flipped);
                    Self::notify_view_name(ctx, false, flipped);
                    return;
                }

                ctx.set_active_view(Views::Session);
                let flipped = ctx.config.is_flip_session();
                Self::notify_view_name(ctx, false, flipped);
            }
            ButtonEvent::Long => {
                ctx.surface.set_trigger_consumed(ButtonId::Session);
                Self::with_session_view(ctx, |v| v.set_birds_eye_active(true));
                let flipped = ctx.config.is_flip_session();
                Self::notify_view_name(ctx, true, flipped);
            }
            ButtonEvent::Down => {}
        }
    }
}

const SCENE_SEQUENCER_VIEWS: [Option<Views>; 8] =
    [Some(Views::Sequencer), Some(Views::PolySequencer), Some(Views::Raindrops), Some(Views::Drum), Some(Views::Drum4), Some(Views::Drum8), None, None];

const SCENE_PLAY_VIEWS: [Option<Views>; 8] = [Some(Views::Play), Some(Views::Chords), Some(Views::Piano), Some(Views::Drum64), None, None, None, None];

/// Former `MaschineJamStartSceneCommand`: launches a scene or selects a play/sequencer view in
/// combination with the Step or Pad Mode button.
pub struct MaschineJamStartSceneCommand {
    inner: StartSceneCommand,
    index: usize,
}

impl MaschineJamStartSceneCommand {
    pub fn new(index: usize) -> Self {
        MaschineJamStartSceneCommand { inner: StartSceneCommand::new(index as i32), index }
    }

    /// Former `getButtonColor`.
    pub fn button_color(ctx: &Ctx, index: usize) -> i32 {
        if ctx.surface.is_pressed(ButtonId::Sequencer) {
            return match SCENE_SEQUENCER_VIEWS[index] {
                None => ctx.colors.color_index(BUTTON_STATE_OFF),
                Some(view) => {
                    if ctx.views.is_active(view) {
                        COLOR_GREEN
                    } else {
                        COLOR_GREY
                    }
                }
            };
        }
        if ctx.surface.is_pressed(ButtonId::Note) {
            return match SCENE_PLAY_VIEWS[index] {
                None => ctx.colors.color_index(BUTTON_STATE_OFF),
                Some(view) => {
                    if ctx.views.is_active(view) {
                        COLOR_BLUE
                    } else {
                        COLOR_GREY
                    }
                }
            };
        }
        let scene = ctx.model.scene_bank().item(index);
        if scene.does_exist() {
            return dim_or_highlight_color(&ctx.colors, scene.color(), scene.is_selected());
        }
        0
    }

    fn activate_view(ctx: &mut Ctx, view: Views) {
        ctx.activate_preferred_view(view);
        delay_display(ctx, |ctx| ctx.active_view().and_then(|v| v.try_borrow().ok().map(|v| v.name().to_string())).unwrap_or_default());
    }
}

impl TriggerCommand for MaschineJamStartSceneCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }

        if ctx.surface.is_select_pressed() {
            if ctx.model.scene_bank().item(self.index).does_exist() {
                ctx.model.scene_bank_mut().item_mut(self.index).select();
            }
            return;
        }

        if ctx.surface.is_pressed(ButtonId::Sequencer) {
            ctx.surface.set_trigger_consumed(ButtonId::Sequencer);
            if let Some(view) = SCENE_SEQUENCER_VIEWS[self.index] {
                Self::activate_view(ctx, view);
            }
            return;
        }

        if ctx.surface.is_pressed(ButtonId::Note) {
            ctx.surface.set_trigger_consumed(ButtonId::Note);
            if let Some(view) = SCENE_PLAY_VIEWS[self.index] {
                Self::activate_view(ctx, view);
            }
            return;
        }

        self.inner.execute_normal(ctx, event);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Long {
            return;
        }
        if ctx.model.scene_bank().item(self.index).does_exist() {
            ctx.model.scene_bank_mut().item_mut(self.index).launch(event == ButtonEvent::Down, true);
        }
        if event == ButtonEvent::Down {
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Toggle a groove parameter between 0 and the maximum (former `setValue (value == 0 ? max :
/// 0)`) and display its value.
fn toggle_groove_parameter(ctx: &mut Ctx, id: GrooveParameterId) {
    let max = ctx.model.value_changer().upper_bound() - 1;
    let parameter = ctx.model.groove_mut().parameter_mut(id);
    let value = if parameter.value() == 0 { max } else { 0 };
    parameter.set_value(value);
    delay_display(ctx, move |ctx| {
        let parameter = ctx.model.groove().parameter(id);
        format!("{}: {}", parameter.name(), parameter.displayed_value())
    });
}

/// Former `MaschineJamSwingCommand`: shows the shuffle view while pressed, the encoder changes
/// the swing amount.
pub struct MaschineJamSwingCommand {
    encoder_manager: SharedEncoderModeManager,
}

impl MaschineJamSwingCommand {
    pub fn new(encoder_manager: SharedEncoderModeManager) -> Self {
        MaschineJamSwingCommand { encoder_manager }
    }
}

impl TriggerCommand for MaschineJamSwingCommand {
    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        toggle_groove_parameter(ctx, GrooveParameterId::ShuffleRate);
        ctx.surface.set_trigger_consumed(ButtonId::Shift);
    }

    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if ctx.surface.is_select_pressed() {
            if event == ButtonEvent::Down {
                toggle_groove_parameter(ctx, GrooveParameterId::Enabled);
            }
            return;
        }
        match event {
            ButtonEvent::Down => {
                ctx.set_temporary_view(Views::Shuffle);
                self.encoder_manager.borrow_mut().enable_temporary_encode_mode(ctx, EncoderMode::TemporarySwing);
            }
            ButtonEvent::Up => {
                ctx.restore_view();
                self.encoder_manager.borrow_mut().disable_temporary_encode_mode(ctx);
            }
            ButtonEvent::Long => {}
        }
    }
}

/// Former `MaschineJamTapTempoCommand`: shows the tempo view while pressed, the encoder changes
/// the tempo; Shift + Tempo taps the tempo.
pub struct MaschineJamTapTempoCommand {
    inner: TapTempoCommand,
    encoder_manager: SharedEncoderModeManager,
}

impl MaschineJamTapTempoCommand {
    pub fn new(encoder_manager: SharedEncoderModeManager) -> Self {
        MaschineJamTapTempoCommand { inner: TapTempoCommand::new(), encoder_manager }
    }
}

impl TriggerCommand for MaschineJamTapTempoCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.surface.is_shift_pressed() {
            self.inner.execute_normal(ctx, event);
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
            return;
        }
        match event {
            ButtonEvent::Down => {
                ctx.set_temporary_view(Views::Tempo);
                self.encoder_manager.borrow_mut().enable_temporary_encode_mode(ctx, EncoderMode::TemporaryTempo);
            }
            ButtonEvent::Up => {
                ctx.restore_view();
                self.encoder_manager.borrow_mut().disable_temporary_encode_mode(ctx);
            }
            ButtonEvent::Long => {}
        }
    }
}

/// Former `MaschineJamViewButtonCommand`: hands the arrow buttons to the active view.
pub struct MaschineJamViewButtonCommand {
    inner: ViewButtonCommand,
}

impl MaschineJamViewButtonCommand {
    pub fn new(button: ButtonId) -> Self {
        MaschineJamViewButtonCommand { inner: ViewButtonCommand::new(button) }
    }

    /// The direction of an arrow button.
    pub fn direction(button: ButtonId) -> Direction {
        match button {
            ButtonId::ArrowLeft => Direction::Left,
            ButtonId::ArrowRight => Direction::Right,
            ButtonId::ArrowUp => Direction::Up,
            _ => Direction::Down,
        }
    }

    /// Former `canScroll`.
    pub fn can_scroll(ctx: &Ctx, button: ButtonId) -> bool {
        super::view::can_scroll(ctx, Self::direction(button))
    }
}

impl TriggerCommand for MaschineJamViewButtonCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        self.inner.execute(ctx, event, velocity);
    }
}

/// Former `MaschineJamViewCommand`: the Perform, Notes, Lock and Tune buttons, which change view
/// options with the encoder while pressed. Lock shows the accent view, a short press toggles the
/// fixed accent.
pub struct MaschineJamViewCommand {
    encoder_manager: SharedEncoderModeManager,
    encoder_mode: EncoderMode,
    was_used: bool,
}

impl MaschineJamViewCommand {
    pub fn new(encoder_manager: SharedEncoderModeManager, encoder_mode: EncoderMode) -> Self {
        MaschineJamViewCommand { encoder_manager, encoder_mode, was_used: false }
    }
}

impl TriggerCommand for MaschineJamViewCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        let is_lock = self.encoder_mode == EncoderMode::TemporaryLock;
        match event {
            ButtonEvent::Long => self.was_used = true,
            ButtonEvent::Down => {
                self.was_used = false;
                self.encoder_manager.borrow_mut().enable_temporary_encode_mode(ctx, self.encoder_mode);
                if is_lock {
                    ctx.set_temporary_view(Views::Control);
                }
            }
            ButtonEvent::Up => {
                self.encoder_manager.borrow_mut().disable_temporary_encode_mode(ctx);
                if !self.was_used && is_lock {
                    let is_active = ctx.config.is_accent_active();
                    ctx.config.set_accent_enabled(!is_active);
                }
                if is_lock {
                    ctx.restore_view();
                }
            }
        }
    }
}

/// The browser command of the JAM: the browser is discarded, the selection is confirmed with
/// the encoder (former anonymous `BrowserCommand` with `getCommit` returning false).
pub struct MaschineJamBrowserCommand {
    inner: BrowserCommand,
}

impl MaschineJamBrowserCommand {
    pub fn new() -> Self {
        MaschineJamBrowserCommand { inner: BrowserCommand::new() }
    }
}

impl TriggerCommand for MaschineJamBrowserCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if event == ButtonEvent::Up && ctx.model.browser().is_some_and(|b| b.is_active()) {
            BrowserCommand::discard_browser(ctx, false);
            return;
        }
        self.inner.execute(ctx, event, velocity);
    }
}

/// Implements `Default` with `new()`.
macro_rules! default_from_new {
    ($($name:ident),*) => {
        $(impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        })*
    };
}

default_from_new!(MaschineJamAutomationCommand, MaschineJamAuxCommand, MaschineJamControlCommand, MaschineJamDoubleCommand, MaschineJamLevelCommand, MaschineJamMacroCommand, MaschineJamPlayCommand, MaschineJamQuantizeCommand, MaschineJamRecordCommand, MaschineJamBrowserCommand);
