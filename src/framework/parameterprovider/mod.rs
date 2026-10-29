// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Parameter providers (former `framework.parameterprovider`): they provide the parameters which
//! are bound to the knobs/faders of a mode (see `framework::mode::parameter_mode`).
//!
//! Differences to Java:
//! - Since all parameters are owned by the model, a provider does not return parameter objects
//!   but [`ParamRef`]s, which address a parameter in the model. They are resolved lazily when a
//!   value is received or displayed. A `ParamRef` for e.g. "the volume of the 3rd track of the
//!   page" or "the 2nd parameter of the current page of the cursor device" therefore always
//!   addresses the current state of the bank. This replaces the parameter observers
//!   (`IParametersAdjustObserver`) of the Java version, which re-bind the controls when a bank
//!   is scrolled. Controls only need to be re-bound if a mode switches its provider.
//! - Parameters which are not part of the model (e.g. the Java `NoteParameter`) are implemented
//!   with the [`CustomParameter`] trait and wrapped in [`ParamRef::Custom`].
//! - Reaper has no effect track bank, therefore the track providers always use the current
//!   track bank.

pub mod device;
pub mod special;
pub mod track;

use std::fmt;
use std::rc::Rc;

use crate::daw::color::ColorEx;
use crate::daw::constants::DeviceId;
use crate::daw::events::ParameterBankId;
use crate::daw::model::Model;
use crate::daw::parameter::Parameter;
use crate::daw::parameter_bank::ParameterBank;
use crate::framework::ctx::Ctx;

pub use device::{BankParameterProvider, LayerOrDrumPadParameterProvider, LayerParameter};
pub use special::{CombinedParameterProvider, EmptyParameterProvider, FixedParameterProvider, RangeFilterParameterProvider};
pub use track::{PanParameterProvider, SelectedTrackParameterProvider, SendParameterProvider, VolumeParameterProvider};

/// A parameter which is not part of the model (e.g. the attributes of a note in a clip). All
/// values are in the range of the value changer of the model.
pub trait CustomParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool;

    fn name(&self, _ctx: &Ctx) -> String {
        String::new()
    }

    fn value(&self, ctx: &Ctx) -> i32;

    fn displayed_value(&self, _ctx: &Ctx) -> String {
        String::new()
    }

    /// Set the value (in the range of the value changer).
    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        let normalized = ctx.model.value_changer().to_normalized_value(value as f64);
        self.set_normalized_value(ctx, normalized);
    }

    /// Set the normalized value [0..1].
    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64);

    /// Change the value with a relative control value.
    fn change_value(&self, ctx: &mut Ctx, control: i32);

    fn reset_value(&self, _ctx: &mut Ctx) {}

    fn touch_value(&self, _ctx: &mut Ctx, _is_touched: bool) {}
}

/// Identifies the device of drum pad / layer parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrumDeviceRef {
    /// The first instrument of the cursor track (former `model.getSpecificDevice (FIRST_INSTRUMENT)`).
    FirstInstrument,
    /// The main drum device of the model (former `model.getDrumDevice ()`).
    Drum,
    /// The cursor device.
    Cursor,
}

/// Addresses a parameter of the model (or a [`CustomParameter`]). Indices are indices in the
/// current page of the bank, positions are absolute positions.
#[derive(Clone, Default)]
pub enum ParamRef {
    /// A parameter which does not exist (former `EmptyParameter`).
    #[default]
    Empty,
    /// The parameter at the index of the current page of a parameter bank.
    BankItem { bank: ParameterBankId, index: usize },
    /// The parameter at the absolute position of a parameter bank.
    BankPosition { bank: ParameterBankId, position: usize },
    /// The volume of the track at the index of the page of the current track bank.
    TrackVolume { index: usize },
    /// The panorama of the track at the index of the page of the current track bank.
    TrackPan { index: usize },
    /// A send of the track at the index of the page of the current track bank.
    TrackSend { index: usize, send: usize },
    /// The volume of the selected track of the current track bank.
    SelectedTrackVolume,
    /// The panorama of the selected track of the current track bank.
    SelectedTrackPan,
    /// A send of the selected track of the current track bank.
    SelectedTrackSend { send: usize },
    MasterVolume,
    MasterPan,
    /// The metronome volume of the transport.
    MetronomeVolume,
    /// The crossfader of the master track.
    Crossfader,
    /// The volume of a drum pad (or layer) at the index of the page of the drum pad bank.
    DrumPadVolume { device: DrumDeviceRef, index: usize },
    /// The panorama of a drum pad (or layer).
    DrumPadPan { device: DrumDeviceRef, index: usize },
    /// A send of a drum pad (or layer).
    DrumPadSend { device: DrumDeviceRef, index: usize, send: usize },
    /// The band type of the equalizer device (former `EqBandTypeParameterImpl`).
    EqBandType { band: usize },
    /// A parameter which is not part of the model.
    Custom(Rc<dyn CustomParameter>),
}

impl fmt::Debug for ParamRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamRef::Empty => write!(f, "Empty"),
            ParamRef::BankItem { bank, index } => write!(f, "BankItem({bank:?}, {index})"),
            ParamRef::BankPosition { bank, position } => write!(f, "BankPosition({bank:?}, {position})"),
            ParamRef::TrackVolume { index } => write!(f, "TrackVolume({index})"),
            ParamRef::TrackPan { index } => write!(f, "TrackPan({index})"),
            ParamRef::TrackSend { index, send } => write!(f, "TrackSend({index}, {send})"),
            ParamRef::SelectedTrackVolume => write!(f, "SelectedTrackVolume"),
            ParamRef::SelectedTrackPan => write!(f, "SelectedTrackPan"),
            ParamRef::SelectedTrackSend { send } => write!(f, "SelectedTrackSend({send})"),
            ParamRef::MasterVolume => write!(f, "MasterVolume"),
            ParamRef::MasterPan => write!(f, "MasterPan"),
            ParamRef::MetronomeVolume => write!(f, "MetronomeVolume"),
            ParamRef::Crossfader => write!(f, "Crossfader"),
            ParamRef::DrumPadVolume { device, index } => write!(f, "DrumPadVolume({device:?}, {index})"),
            ParamRef::DrumPadPan { device, index } => write!(f, "DrumPadPan({device:?}, {index})"),
            ParamRef::DrumPadSend { device, index, send } => write!(f, "DrumPadSend({device:?}, {index}, {send})"),
            ParamRef::EqBandType { band } => write!(f, "EqBandType({band})"),
            ParamRef::Custom(_) => write!(f, "Custom"),
        }
    }
}

impl PartialEq for ParamRef {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ParamRef::Custom(a), ParamRef::Custom(b)) => Rc::ptr_eq(a, b),
            (ParamRef::Custom(_), _) | (_, ParamRef::Custom(_)) => false,
            _ => format!("{self:?}") == format!("{other:?}"),
        }
    }
}

/// Get a parameter bank of the model.
pub fn parameter_bank(model: &Model, bank: ParameterBankId) -> Option<&ParameterBank> {
    match bank {
        ParameterBankId::CursorDevice => model.cursor_device().parameter_bank(),
        ParameterBankId::FirstInstrument => model.specific_device(DeviceId::FirstInstrument)?.parameter_bank(),
        ParameterBankId::Eq => model.specific_device(DeviceId::Eq)?.parameter_bank(),
        ParameterBankId::DrumDevice => model.drum_device()?.parameter_bank(),
        ParameterBankId::CursorTrack => model.cursor_track_parameter_bank(),
        ParameterBankId::Project => model.project().parameter_bank(),
    }
}

/// Get a parameter bank of the model for modification.
pub fn parameter_bank_mut(model: &mut Model, bank: ParameterBankId) -> Option<&mut ParameterBank> {
    match bank {
        ParameterBankId::CursorDevice => model.cursor_device_mut().parameter_bank_mut(),
        ParameterBankId::FirstInstrument => model.specific_device_mut(DeviceId::FirstInstrument)?.parameter_bank_mut(),
        ParameterBankId::Eq => model.specific_device_mut(DeviceId::Eq)?.parameter_bank_mut(),
        ParameterBankId::DrumDevice => model.drum_device_mut()?.parameter_bank_mut(),
        ParameterBankId::CursorTrack => model.cursor_track_parameter_bank_mut(),
        ParameterBankId::Project => model.project_mut().parameter_bank_mut(),
    }
}

fn drum_device(model: &Model, device: DrumDeviceRef) -> Option<&crate::daw::device::SpecificDevice> {
    match device {
        DrumDeviceRef::FirstInstrument => model.specific_device(DeviceId::FirstInstrument),
        DrumDeviceRef::Drum => model.drum_device(),
        DrumDeviceRef::Cursor => Some(model.cursor_device()),
    }
}

fn drum_device_mut(model: &mut Model, device: DrumDeviceRef) -> Option<&mut crate::daw::device::SpecificDevice> {
    match device {
        DrumDeviceRef::FirstInstrument => model.specific_device_mut(DeviceId::FirstInstrument),
        DrumDeviceRef::Drum => model.drum_device_mut(),
        DrumDeviceRef::Cursor => Some(model.cursor_device_mut()),
    }
}

/// The parameter (if the reference addresses a [`Parameter`] of the model).
pub fn resolve<'a>(model: &'a Model, parameter: &ParamRef) -> Option<&'a Parameter> {
    match parameter {
        ParamRef::BankItem { bank, index } => Some(parameter_bank(model, *bank)?.item(*index)),
        ParamRef::BankPosition { bank, position } => Some(parameter_bank(model, *bank)?.unpaged_item(*position)),
        ParamRef::TrackVolume { index } => Some(model.current_track_bank().item(*index).volume_parameter()),
        ParamRef::TrackPan { index } => Some(model.current_track_bank().item(*index).pan_parameter()),
        ParamRef::TrackSend { index, send } => {
            let send_bank = model.current_track_bank().item(*index).send_bank();
            (send_bank.item_count() > 0).then(|| &**send_bank.item(*send))
        }
        ParamRef::SelectedTrackVolume => Some(model.current_track_bank().selected_item()?.volume_parameter()),
        ParamRef::SelectedTrackPan => Some(model.current_track_bank().selected_item()?.pan_parameter()),
        ParamRef::SelectedTrackSend { send } => {
            let send_bank = model.current_track_bank().selected_item()?.send_bank();
            (*send < send_bank.item_count()).then(|| &**send_bank.item(*send))
        }
        ParamRef::MasterVolume => Some(model.master_track().volume_parameter()),
        ParamRef::MasterPan => Some(model.master_track().pan_parameter()),
        ParamRef::Crossfader => model.crossfade_parameter(),
        ParamRef::DrumPadVolume { device, index } => Some(drum_device(model, *device)?.drum_pad_bank().item(*index).volume_parameter()),
        ParamRef::DrumPadPan { device, index } => Some(drum_device(model, *device)?.drum_pad_bank().item(*index).pan_parameter()),
        ParamRef::DrumPadSend { device, index, send } => {
            let send_bank = drum_device(model, *device)?.drum_pad_bank().item(*index).send_bank();
            (send_bank.item_count() > 0).then(|| &**send_bank.item(*send))
        }
        ParamRef::Empty | ParamRef::MetronomeVolume | ParamRef::EqBandType { .. } | ParamRef::Custom(_) => None,
    }
}

/// The parameter (if the reference addresses a [`Parameter`] of the model) for modification.
pub fn resolve_mut<'a>(model: &'a mut Model, parameter: &ParamRef) -> Option<&'a mut Parameter> {
    match parameter {
        ParamRef::BankItem { bank, index } => Some(parameter_bank_mut(model, *bank)?.item_mut(*index)),
        ParamRef::BankPosition { bank, position } => {
            let bank = parameter_bank_mut(model, *bank)?;
            (*position < bank.item_count()).then(|| bank.unpaged_item_mut(*position))
        }
        ParamRef::TrackVolume { index } => Some(model.current_track_bank_mut().item_mut(*index).volume_parameter_mut()),
        ParamRef::TrackPan { index } => Some(model.current_track_bank_mut().item_mut(*index).pan_parameter_mut()),
        ParamRef::TrackSend { index, send } => {
            let send_bank = model.current_track_bank_mut().item_mut(*index).send_bank_mut();
            (send_bank.item_count() > 0).then(|| &mut **send_bank.item_mut(*send))
        }
        ParamRef::SelectedTrackVolume => Some(model.current_track_bank_mut().selected_item_mut()?.volume_parameter_mut()),
        ParamRef::SelectedTrackPan => Some(model.current_track_bank_mut().selected_item_mut()?.pan_parameter_mut()),
        ParamRef::SelectedTrackSend { send } => {
            let send_bank = model.current_track_bank_mut().selected_item_mut()?.send_bank_mut();
            (*send < send_bank.item_count()).then(|| &mut **send_bank.item_mut(*send))
        }
        ParamRef::MasterVolume => Some(model.master_track_mut().volume_parameter_mut()),
        ParamRef::MasterPan => Some(model.master_track_mut().pan_parameter_mut()),
        ParamRef::Crossfader => model.crossfade_parameter_mut(),
        ParamRef::DrumPadVolume { device, index } => Some(drum_device_mut(model, *device)?.drum_pad_bank_mut().item_mut(*index).volume_parameter_mut()),
        ParamRef::DrumPadPan { device, index } => Some(drum_device_mut(model, *device)?.drum_pad_bank_mut().item_mut(*index).pan_parameter_mut()),
        ParamRef::DrumPadSend { device, index, send } => {
            let send_bank = drum_device_mut(model, *device)?.drum_pad_bank_mut().item_mut(*index).send_bank_mut();
            (send_bank.item_count() > 0).then(|| &mut **send_bank.item_mut(*send))
        }
        ParamRef::Empty | ParamRef::MetronomeVolume | ParamRef::EqBandType { .. } | ParamRef::Custom(_) => None,
    }
}

const EQ_BAND_TYPE_COUNT: usize = 7;

fn eq_device(model: &Model) -> Option<&crate::daw::device::SpecificDevice> {
    model.specific_device(DeviceId::Eq)
}

impl ParamRef {
    pub fn is_empty(&self) -> bool {
        matches!(self, ParamRef::Empty)
    }

    pub fn does_exist(&self, ctx: &Ctx) -> bool {
        match self {
            ParamRef::Empty => false,
            ParamRef::MetronomeVolume => true,
            ParamRef::EqBandType { band } => *band < crate::daw::device::EQ_NUMBER_OF_BANDS && eq_device(&ctx.model).is_some_and(|d| d.does_exist()),
            ParamRef::Custom(custom) => custom.does_exist(ctx),
            _ => resolve(&ctx.model, self).is_some_and(|p| p.does_exist()),
        }
    }

    pub fn name(&self, ctx: &Ctx) -> String {
        match self {
            ParamRef::Empty => String::new(),
            ParamRef::MetronomeVolume => ctx.model.transport().metronome_volume_parameter().name().to_string(),
            ParamRef::EqBandType { .. } => String::new(),
            ParamRef::Custom(custom) => custom.name(ctx),
            _ => resolve(&ctx.model, self).map(|p| p.name().to_string()).unwrap_or_default(),
        }
    }

    /// The value in the range of the value changer (0 if it does not exist).
    pub fn value(&self, ctx: &Ctx) -> i32 {
        match self {
            ParamRef::Empty => 0,
            ParamRef::MetronomeVolume => ctx.model.transport().metronome_volume_parameter().value(),
            ParamRef::EqBandType { band } => {
                let ordinal = eq_device(&ctx.model).map(|d| d.type_id(*band) as usize).unwrap_or(0);
                (ordinal as f64 * (127.0 / (EQ_BAND_TYPE_COUNT - 1) as f64)).round() as i32
            }
            ParamRef::Custom(custom) => custom.value(ctx),
            _ => resolve(&ctx.model, self).map(|p| p.value()).unwrap_or(0),
        }
    }

    pub fn displayed_value(&self, ctx: &Ctx) -> String {
        match self {
            ParamRef::Empty => String::new(),
            ParamRef::MetronomeVolume => ctx.model.transport().metronome_volume_parameter().displayed_value().to_string(),
            ParamRef::EqBandType { band } => format!("Band Type {}", band + 1),
            ParamRef::Custom(custom) => custom.displayed_value(ctx),
            _ => resolve(&ctx.model, self).map(|p| p.displayed_value()).unwrap_or_default(),
        }
    }

    /// Set the value (in the range of the value changer).
    pub fn set_value(&self, ctx: &mut Ctx, value: i32) {
        match self {
            ParamRef::Empty => {}
            ParamRef::MetronomeVolume => ctx.model.transport_mut().metronome_volume_parameter_mut().set_value(value),
            ParamRef::EqBandType { .. } => {
                let normalized = ctx.model.value_changer().to_normalized_value(value as f64);
                self.set_normalized_value(ctx, normalized);
            }
            ParamRef::Custom(custom) => custom.clone().set_value(ctx, value),
            _ => {
                if let Some(p) = resolve_mut(&mut ctx.model, self) {
                    p.set_value(value);
                }
            }
        }
    }

    /// Set the normalized value [0..1].
    pub fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        match self {
            ParamRef::Empty => {}
            ParamRef::MetronomeVolume => ctx.model.transport_mut().metronome_volume_parameter_mut().set_normalized_value(value),
            ParamRef::EqBandType { band } => {
                let type_index = (value * (EQ_BAND_TYPE_COUNT - 1) as f64).round().clamp(0.0, (EQ_BAND_TYPE_COUNT - 1) as f64) as usize;
                let band_type = crate::daw::device::EqualizerBandType::ALL[type_index];
                if let Some(device) = ctx.model.specific_device_mut(DeviceId::Eq) {
                    device.set_type(*band, band_type);
                }
            }
            ParamRef::Custom(custom) => custom.clone().set_normalized_value(ctx, value),
            _ => {
                if let Some(p) = resolve_mut(&mut ctx.model, self) {
                    p.set_normalized_value(value);
                }
            }
        }
    }

    /// Change the value with a relative control value.
    pub fn change_value(&self, ctx: &mut Ctx, control: i32) {
        match self {
            ParamRef::Empty => {}
            ParamRef::MetronomeVolume => ctx.model.transport_mut().metronome_volume_parameter_mut().change_value(control),
            ParamRef::EqBandType { band } => {
                let increment = if ctx.model.value_changer().calc_knob_change(control) > 0.0 { 1 } else { -1 };
                if let Some(device) = ctx.model.specific_device_mut(DeviceId::Eq) {
                    device.change_type(*band, increment);
                }
            }
            ParamRef::Custom(custom) => custom.clone().change_value(ctx, control),
            _ => {
                if let Some(p) = resolve_mut(&mut ctx.model, self) {
                    p.change_value(control);
                }
            }
        }
    }

    pub fn reset_value(&self, ctx: &mut Ctx) {
        match self {
            ParamRef::Empty => {}
            ParamRef::MetronomeVolume => ctx.model.transport_mut().metronome_volume_parameter_mut().reset_value(),
            ParamRef::EqBandType { band } => {
                if let Some(device) = ctx.model.specific_device_mut(DeviceId::Eq) {
                    device.set_type(*band, crate::daw::device::EqualizerBandType::Off);
                }
            }
            ParamRef::Custom(custom) => custom.clone().reset_value(ctx),
            _ => {
                if let Some(p) = resolve_mut(&mut ctx.model, self) {
                    p.reset_value();
                }
            }
        }
    }

    pub fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        match self {
            ParamRef::Empty | ParamRef::MetronomeVolume | ParamRef::EqBandType { .. } => {}
            ParamRef::Custom(custom) => custom.clone().touch_value(ctx, is_touched),
            _ => {
                if let Some(p) = resolve_mut(&mut ctx.model, self) {
                    p.touch_value(is_touched);
                }
            }
        }
    }
}

/// Provides the parameters for the controls of a mode (former `IParameterProvider`).
pub trait ParameterProvider {
    /// The number of provided parameters.
    fn size(&self) -> usize;

    /// Get the parameter at the given index.
    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef;

    /// Get the color of the parameter at the given index, if any.
    fn color(&self, _ctx: &Ctx, _index: usize) -> Option<ColorEx> {
        None
    }
}

/// A shared parameter provider.
pub type SharedProvider = Rc<dyn ParameterProvider>;

/// Wrap a provider into a [`SharedProvider`].
pub fn provider<P: ParameterProvider + 'static>(provider: P) -> SharedProvider {
    Rc::new(provider)
}

/// A provider implemented with a closure (size, index to parameter).
pub struct FnParameterProvider<F: Fn(&Ctx, usize) -> ParamRef> {
    size: usize,
    function: F,
}

impl<F: Fn(&Ctx, usize) -> ParamRef> FnParameterProvider<F> {
    pub fn new(size: usize, function: F) -> Self {
        FnParameterProvider { size, function }
    }
}

impl<F: Fn(&Ctx, usize) -> ParamRef> ParameterProvider for FnParameterProvider<F> {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef {
        if index < self.size {
            (self.function)(ctx, index)
        } else {
            ParamRef::Empty
        }
    }
}
