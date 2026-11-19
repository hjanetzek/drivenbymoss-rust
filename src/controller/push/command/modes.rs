// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The access of the commands to specific Push modes (former casts of
//! `modeManager.get (Modes.X)` to the mode class and `instanceof IPush3Encoder`).

use crate::controller::push::mode::misc as misc_modes;
use crate::controller::push::mode::track::add_track::{AddMode, AddTrackMode};
use crate::controller::push::mode::track::clip::ClipMode;
use crate::framework::ctx::Ctx;
use crate::framework::ids::Modes;
use crate::midi::ButtonEvent;

/// Former `((ClipMode) modeManager.get (Modes.CLIP)).togglePianoRoll ()`.
pub fn toggle_piano_roll(ctx: &mut Ctx) {
    if let Some(mode) = ctx.modes.get(Modes::Clip) {
        if let Ok(mut mode) = mode.try_borrow_mut() {
            if let Some(clip_mode) = mode.as_any_mut().downcast_mut::<ClipMode>() {
                clip_mode.toggle_piano_roll();
            }
        }
    }
}

/// Former `((AddTrackMode) modeManager.get (Modes.ADD_TRACK)).setAddMode (AddMode.DEVICE)`.
pub fn set_add_mode_device(ctx: &mut Ctx) {
    if let Some(mode) = ctx.modes.get(Modes::AddTrack) {
        if let Ok(mut mode) = mode.try_borrow_mut() {
            if let Some(add_track) = mode.as_any_mut().downcast_mut::<AddTrackMode>() {
                add_track.set_add_mode(AddMode::Device);
            }
        }
    }
}

/// Former `((DeviceBrowserMode) modeManager.get (Modes.BROWSER)).changeSelectedColumnValue
/// (value)`. Returns false if the browser mode does not offer it.
pub fn change_selected_column_value(ctx: &mut Ctx, value: i32) -> bool {
    let Some(mode) = ctx.modes.get(Modes::Browser) else {
        return false;
    };
    let Ok(mut mode) = mode.try_borrow_mut() else {
        return false;
    };
    match mode.as_any_mut().downcast_mut::<crate::controller::push::mode::device::browser::DeviceBrowserMode>() {
        Some(browser) => {
            browser.change_selected_column_value(ctx, value);
            true
        }
        None => false,
    }
}

/// A function of the Push 3 encoder (former `IPush3Encoder`).
#[derive(Debug, Clone, Copy)]
pub enum EncoderAction {
    Turn(i32),
    Press(ButtonEvent),
    Left(ButtonEvent),
    Right(ButtonEvent),
    Center(ButtonEvent),
}

/// Executes the encoder function on the active mode if it implements `IPush3Encoder`. Returns
/// false if the active mode does not implement it.
///
/// Uses `mode::misc::as_push3_encoder` (SessionMode; ClipMode and DeviceBrowserMode are added
/// there by the track/device agents).
pub fn execute_encoder(ctx: &mut Ctx, action: EncoderAction) -> bool {
    let Some(mode) = ctx.active_mode() else {
        return false;
    };
    let Ok(mut mode) = mode.try_borrow_mut() else {
        return false;
    };
    if let Some(encoder) = misc_modes::as_push3_encoder(&mut *mode) {
        match action {
            EncoderAction::Turn(value) => encoder.encoder_turn(ctx, value),
            EncoderAction::Press(event) => encoder.encoder_press(ctx, event),
            EncoderAction::Left(event) => encoder.encoder_left(ctx, event),
            EncoderAction::Right(event) => encoder.encoder_right(ctx, event),
            EncoderAction::Center(event) => encoder.arrow_center(ctx, event),
        }
        return true;
    }
    false
}
