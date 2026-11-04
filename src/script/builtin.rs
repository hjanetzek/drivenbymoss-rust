// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The built-in modes, parameter providers, parameters and commands which a script creates by
//! name. The options come from the script engine as plain values.

use std::cell::RefCell;
use std::rc::Rc;

use super::ctx_stack::{ScriptError, ScriptResult};
use crate::daw::events::ParameterBankId;
use crate::framework::command::{shared, ContinuousCommand, SharedTrigger, TriggerCommand};
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::mode::{ButtonRowModeCommand, KnobRowModeCommand, ModeSelectCommand};
use crate::framework::commands::transport::PlayCommand;
use crate::framework::commands::transport_ext::{TempoCommand, WindCommand};
use crate::framework::ctx::{Ctx, ModeManagerId, MAIN_MODE_MANAGER};
use crate::framework::feature::Mode;
use crate::framework::ids::{ContinuousId, Modes};
use crate::framework::mode::dummy::DummyMode;
use crate::framework::mode::layer::{LayerMuteMode, LayerSoloMode};
use crate::framework::mode::master::MasterAndFXVolumeMode;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::mode::sequencer::SequencerModeBase;
use crate::framework::mode::track::{TrackMode, TrackMuteMode, TrackPanMode, TrackRecArmMode, TrackSelectMode, TrackSendMode, TrackSoloMode, TrackStopClipMode, TrackVolumeMode};
use crate::framework::parameterprovider::{
    provider, BankParameterProvider, CombinedParameterProvider, DrumDeviceRef, EmptyParameterProvider, FixedParameterProvider, LayerOrDrumPadParameterProvider,
    PanParameterProvider, ParamRef, SendParameterProvider, SharedProvider, VolumeParameterProvider,
};

// ------------------------------------------------------------------------------------------
// Modes

/// The built-in modes by name (the names of the Rust types without `Mode`).
pub const MODE_KINDS: &[&str] = &[
    "Parameter",
    "Sequencer",
    "Dummy",
    "MasterAndFXVolume",
    "Track",
    "TrackSelect",
    "TrackMute",
    "TrackSolo",
    "TrackRecArm",
    "TrackStopClip",
    "TrackVolume",
    "TrackPan",
    "TrackSend",
    "LayerMute",
    "LayerSolo",
];

/// The options of a built-in mode.
#[derive(Default)]
pub struct ModeOptions {
    /// The name of the mode (`Parameter`, `Sequencer`), default: the kind.
    pub name: Option<String>,
    /// The controls of the mode (default: none).
    pub controls: Vec<ContinuousId>,
    /// Absolute controls (default: true).
    pub is_absolute: Option<bool>,
    /// The bank which the mode navigates (`Parameter`).
    pub bank: Option<ModeBank>,
    /// The parameter provider (`Parameter`).
    pub provider: Option<SharedProvider>,
    /// The size of the edited clip (`Sequencer`): columns (steps) and rows.
    pub clip: Option<(usize, usize)>,
    /// The send (`TrackSend`).
    pub send_index: i32,
}

/// A parameter mode with the functions of [`ParameterModeBase`] (the generic part of the former
/// `AbstractParameterMode` sub-classes).
pub struct ParameterMode {
    pub base: ParameterModeBase,
}

impl Mode for ParameterMode {
    crate::forward_mode!(base => name, on_activate, on_deactivate, knob_value, on_knob_value, on_knob_touch, is_any_knob_touched, select_item,
        select_previous_item, select_next_item, select_previous_item_page, select_next_item_page, select_item_page, has_previous_item,
        has_next_item, has_previous_item_page, has_next_item_page);

    crate::impl_as_any!();
}

/// A sequencer mode with the functions of [`SequencerModeBase`] (former
/// `AbstractSequencerMode`).
pub struct SequencerMode {
    pub base: SequencerModeBase,
}

impl Mode for SequencerMode {
    fn name(&self) -> &str {
        &self.base.name
    }

    crate::forward_mode!(base => on_activate);

    crate::impl_as_any!();
}

/// Create a built-in mode.
pub fn create_mode(ctx: &mut Ctx, kind: &str, options: ModeOptions) -> ScriptResult<Box<dyn Mode>> {
    let is_absolute = options.is_absolute.unwrap_or(true);
    let controls = options.controls;
    let has_controls = !controls.is_empty();
    let name = options.name.unwrap_or_else(|| kind.to_string());
    let mode: Box<dyn Mode> = match kind {
        "Parameter" => {
            let mut base = ParameterModeBase::new(&name, is_absolute, options.bank.unwrap_or(ModeBank::None), controls);
            if let Some(provider) = options.provider {
                base.set_parameter_provider(ctx, provider);
            }
            Box::new(ParameterMode { base })
        }
        "Sequencer" => {
            let (cols, rows) = options.clip.ok_or_else(|| ScriptError::new("a Sequencer mode needs the clip size: clip = {steps, rows}"))?;
            Box::new(SequencerMode { base: SequencerModeBase::new(&name, ctx, is_absolute, rows, cols, false) })
        }
        "Dummy" => Box::new(DummyMode::new(ctx, controls)),
        "MasterAndFXVolume" => Box::new(MasterAndFXVolumeMode::new(ctx, is_absolute, controls)),
        "Track" if has_controls => Box::new(TrackMode::new_with_controls(ctx, is_absolute, controls)),
        "Track" => Box::new(TrackMode::new(ctx, is_absolute)),
        "TrackSelect" => Box::new(TrackSelectMode::new(ctx)),
        "TrackMute" => Box::new(TrackMuteMode::new(ctx)),
        "TrackSolo" => Box::new(TrackSoloMode::new(ctx)),
        "TrackRecArm" => Box::new(TrackRecArmMode::new(ctx)),
        "TrackStopClip" => Box::new(TrackStopClipMode::new(ctx)),
        "TrackVolume" if has_controls => Box::new(TrackVolumeMode::new_with_controls(ctx, is_absolute, controls)),
        "TrackVolume" => Box::new(TrackVolumeMode::new(ctx, is_absolute)),
        "TrackPan" if has_controls => Box::new(TrackPanMode::new_with_controls(ctx, is_absolute, controls)),
        "TrackPan" => Box::new(TrackPanMode::new(ctx, is_absolute)),
        "TrackSend" if has_controls => Box::new(TrackSendMode::new_with_controls(options.send_index, ctx, is_absolute, controls)),
        "TrackSend" => Box::new(TrackSendMode::new(options.send_index, ctx, is_absolute)),
        "LayerMute" => Box::new(LayerMuteMode::new(ctx)),
        "LayerSolo" => Box::new(LayerSoloMode::new(ctx)),
        _ => return Err(ScriptError::new(format!("unknown built-in mode '{kind}', one of {}", MODE_KINDS.join(", ")))),
    };
    Ok(mode)
}

/// The bank of a parameter mode by name: `none`, `track`, a parameter bank (`CursorDevice`, …),
/// or `drum_pads:FirstInstrument|Drum|Cursor` (the drum pads, or the layers of a device without
/// drum pads).
pub fn mode_bank(name: &str) -> ScriptResult<ModeBank> {
    match name {
        "none" => Ok(ModeBank::None),
        "track" => Ok(ModeBank::TrackBank),
        _ => {
            if let Some(device) = name.strip_prefix("drum_pads:") {
                return Ok(ModeBank::DrumPadBank(drum_device(device)?));
            }
            Ok(ModeBank::ParameterBank(super::names::parameter_bank(name)?))
        }
    }
}

pub fn drum_device(name: &str) -> ScriptResult<DrumDeviceRef> {
    match name {
        "FirstInstrument" => Ok(DrumDeviceRef::FirstInstrument),
        "Drum" => Ok(DrumDeviceRef::Drum),
        "Cursor" => Ok(DrumDeviceRef::Cursor),
        _ => Err(ScriptError::new(format!("unknown drum device '{name}'"))),
    }
}

// ------------------------------------------------------------------------------------------
// Parameter providers and parameters

/// A built-in parameter provider.
pub enum ProviderSpec {
    /// The volumes of the tracks of the current bank.
    Volume,
    /// The panoramas of the tracks of the current bank.
    Pan,
    /// A send of the tracks of the current bank (send index, offset).
    Send(i32, usize),
    /// The parameters of a parameter bank.
    Bank(ParameterBankId),
    /// The volumes of the layers or drum pads of the first instrument.
    LayerVolume,
    LayerPan,
    LayerSend(usize),
    /// The providers one after the other.
    Combined(Vec<SharedProvider>),
    /// A fixed list of parameters.
    Fixed(Vec<ParamRef>),
    /// The given number of empty parameters.
    Empty(usize),
}

pub fn create_provider(ctx: &Ctx, spec: ProviderSpec) -> SharedProvider {
    match spec {
        ProviderSpec::Volume => provider(VolumeParameterProvider::new(ctx)),
        ProviderSpec::Pan => provider(PanParameterProvider::new(ctx)),
        ProviderSpec::Send(index, offset) => provider(SendParameterProvider::new(ctx, index, offset)),
        ProviderSpec::Bank(bank) => provider(BankParameterProvider::new(ctx, bank)),
        ProviderSpec::LayerVolume => provider(LayerOrDrumPadParameterProvider::volume(ctx)),
        ProviderSpec::LayerPan => provider(LayerOrDrumPadParameterProvider::pan(ctx)),
        ProviderSpec::LayerSend(index) => provider(LayerOrDrumPadParameterProvider::send(ctx, index)),
        ProviderSpec::Combined(providers) => provider(CombinedParameterProvider::new(providers)),
        ProviderSpec::Fixed(parameters) => provider(FixedParameterProvider::new(parameters)),
        ProviderSpec::Empty(size) => provider(EmptyParameterProvider::new(size)),
    }
}

/// A provider without parameters (needs no context).
pub fn empty_provider() -> SharedProvider {
    provider(EmptyParameterProvider::new(0))
}

// ------------------------------------------------------------------------------------------
// Commands

/// The built-in commands by name.
pub const COMMAND_KINDS: &[&str] = &["ButtonRowMode", "KnobRowMode", "ModeCursor", "Cursor", "ModeSelect", "New", "Tempo", "Wind", "Play"];

/// The options of a built-in command.
#[derive(Default)]
pub struct CommandOptions {
    pub row: usize,
    pub index: usize,
    pub manager: Option<ModeManagerId>,
    pub direction: Option<Direction>,
    pub notify: bool,
    /// `Tempo`: increase; `Wind`: fast forward.
    pub increase: bool,
    pub mode: Option<Modes>,
    pub toggle: bool,
    pub temporary: bool,
}

/// A built-in command. The commands with functions which are not part of the command traits
/// keep their type.
#[derive(Clone)]
pub enum Command {
    Trigger(SharedTrigger),
    Cursor(Rc<RefCell<CursorCommand>>),
    New(Rc<RefCell<NewCommand>>),
    Tempo(Rc<RefCell<TempoCommand>>),
    Continuous(Rc<RefCell<dyn ContinuousCommand>>),
}

impl Command {
    /// The command as a trigger (for a button), `None` for a continuous command.
    pub fn trigger(&self) -> Option<SharedTrigger> {
        match self {
            Command::Trigger(c) => Some(c.clone()),
            Command::Cursor(c) => Some(c.clone()),
            Command::New(c) => Some(c.clone()),
            Command::Tempo(c) => Some(c.clone()),
            Command::Continuous(_) => None,
        }
    }

    /// The command as a continuous command (for a knob or fader).
    pub fn continuous(&self) -> Option<Rc<RefCell<dyn ContinuousCommand>>> {
        match self {
            Command::Continuous(c) => Some(c.clone()),
            _ => None,
        }
    }

    /// Run `f` with the trigger command; an error if it runs already (re-entrancy).
    pub fn with_trigger<R>(&self, f: impl FnOnce(&mut dyn TriggerCommand) -> R) -> ScriptResult<R> {
        let trigger = self.trigger().ok_or_else(|| ScriptError::new("not a trigger command"))?;
        let mut command = trigger.try_borrow_mut().map_err(|_| ScriptError::new("the command is running (re-entrant call)"))?;
        Ok(f(&mut *command))
    }
}

pub fn create_command(kind: &str, options: CommandOptions) -> ScriptResult<Command> {
    let manager = options.manager.unwrap_or(MAIN_MODE_MANAGER);
    let direction = || options.direction.ok_or_else(|| ScriptError::new(format!("the command {kind} needs a direction")));
    Ok(match kind {
        "ButtonRowMode" => Command::Trigger(shared(ButtonRowModeCommand::with_manager(manager, options.row, options.index))),
        "KnobRowMode" => Command::Continuous(Rc::new(RefCell::new(KnobRowModeCommand::with_manager(manager, options.index)))),
        "ModeCursor" => Command::Cursor(Rc::new(RefCell::new(CursorCommand::new_mode_cursor(direction()?, options.notify)))),
        "Cursor" => Command::Cursor(Rc::new(RefCell::new(CursorCommand::new(direction()?, options.notify)))),
        "ModeSelect" => {
            let mode = options.mode.ok_or_else(|| ScriptError::new("the command ModeSelect needs a mode"))?;
            Command::Trigger(shared(ModeSelectCommand::with_options(manager, mode, options.toggle, options.notify, options.temporary)))
        }
        "New" => Command::New(Rc::new(RefCell::new(NewCommand::new()))),
        "Tempo" => Command::Tempo(Rc::new(RefCell::new(TempoCommand::button(options.increase)))),
        "Wind" => Command::Trigger(shared(WindCommand::new(options.increase))),
        "Play" => Command::Trigger(shared(PlayCommand::new())),
        _ => return Err(ScriptError::new(format!("unknown built-in command '{kind}', one of {}", COMMAND_KINDS.join(", ")))),
    })
}
