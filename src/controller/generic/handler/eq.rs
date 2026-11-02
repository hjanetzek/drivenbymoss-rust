// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for equalizer commands (former `EqHandler`).
//!
//! The band type is not a parameter of the Rust model; the functions of the Reaper
//! `EqBandTypeParameterImpl` (value, set and change) are implemented here.

use super::device::{handle_parameter, parameter_bank, DeviceRef};
use super::{is_absolute, relative_value_changer, absolute_value_changer};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::controller::generic::surface::{flexi, flexi_mut};
use crate::daw::constants::DeviceId;
use crate::daw::device::{EqualizerBandType, SpecificDevice};
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;

const EQ: DeviceRef = DeviceRef::Specific(DeviceId::Eq);

/// The commands of the band parameters: the first command, the offset of the parameter in the
/// parameter bank (3 parameters per band).
const BAND_PARAMETERS: [(FlexiCommand, usize); 3] = [(FlexiCommand::EqSetFreq1, 0), (FlexiCommand::EqSetGain1, 1), (FlexiCommand::EqSetQ1, 2)];

/// The index in the parameter bank of a band parameter command.
fn band_parameter(command: FlexiCommand) -> Option<(usize, usize)> {
    BAND_PARAMETERS.iter().find_map(|(first, offset)| command.index_in(*first, 8).map(|band| (band, 3 * band + offset)))
}

fn equalizer(ctx: &Ctx) -> Option<&SpecificDevice> {
    ctx.model.specific_device(DeviceId::Eq)
}

/// Former `EqBandTypeParameterImpl.getValue`.
fn type_value(equalizer: &SpecificDevice, band: usize) -> i32 {
    let types = EqualizerBandType::ALL.len() as f64;
    (equalizer.type_id(band) as usize as f64 * (127.0 / (types - 1.0))).round() as i32
}

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    if let Some(band) = command.index_in(FlexiCommand::EqSetType1, 8) {
        return equalizer(ctx).map_or(-1, |equalizer| type_value(equalizer, band));
    }
    if let Some((_, index)) = band_parameter(command) {
        return parameter_bank(ctx, EQ).map_or(-1, |bank| bank.item(index).value());
    }
    -1
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    if !equalizer(ctx).is_some_and(|equalizer| equalizer.does_exist()) {
        if ctx.model.cursor_track_exists() && !flexi(ctx).eq_requested {
            ctx.model.cursor_track_mut().add_equalizer_device();
            flexi_mut(ctx).eq_requested = true;
        }
        return;
    }
    flexi_mut(ctx).eq_requested = false;

    if let Some(band) = command.index_in(FlexiCommand::EqSetType1, 8) {
        handle_type(ctx, band, knob_mode, value);
        return;
    }
    if let Some((band, index)) = band_parameter(command) {
        ensure_band_is_on(ctx, band);
        handle_parameter(ctx, EQ, knob_mode, index, value);
        notify_parameter(ctx, index);
        return;
    }
    super::unsupported(ctx, command);
}

/// Set or change the type of a band (former `handleParameter` with an
/// `EqBandTypeParameterImpl`). As in Java, there is no notification since the type parameter
/// does not exist.
fn handle_type(ctx: &mut Ctx, band: usize, knob_mode: KnobMode, value: MidiValue) {
    if is_absolute(knob_mode) {
        let normalized = absolute_value_changer(ctx, value).to_normalized_value(value.value() as f64);
        let index = (normalized * (EqualizerBandType::ALL.len() - 1) as f64).round() as usize;
        let band_type = EqualizerBandType::ALL[index.min(EqualizerBandType::ALL.len() - 1)];
        if let Some(equalizer) = ctx.model.specific_device_mut(DeviceId::Eq) {
            equalizer.set_type(band, band_type);
        }
    } else {
        let increment = if relative_value_changer(ctx, knob_mode).calc_knob_change(value.value()) > 0.0 { 1 } else { -1 };
        if let Some(equalizer) = ctx.model.specific_device_mut(DeviceId::Eq) {
            equalizer.change_type(band, increment);
        }
    }
}

/// Switch a band on (to Bell) if it is off (former `ensureBandIsOn`).
fn ensure_band_is_on(ctx: &mut Ctx, band: usize) {
    if let Some(equalizer) = ctx.model.specific_device_mut(DeviceId::Eq) {
        if equalizer.type_id(band) == EqualizerBandType::Off {
            equalizer.set_type(band, EqualizerBandType::Bell);
        }
    }
}

/// Former `MVHelper.notifyParameter`.
fn notify_parameter(ctx: &mut Ctx, index: usize) {
    if !parameter_bank(ctx, EQ).is_some_and(|bank| bank.item(index).does_exist()) {
        return;
    }
    mv_helper::delay_display(ctx, move |ctx| {
        parameter_bank(ctx, EQ).map(|bank| bank.item(index)).map(|p| format!("{}: {}", p.name(), p.displayed_value())).unwrap_or_default()
    });
}
