// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Launch Control XL specific parts of the control surface (former
//! `LaunchControlXLControlSurface`) and the controller specific state, which is stored in
//! `ctx.ext` ([`XlState`]).

use std::collections::HashMap;

use crate::framework::ctx::{Ctx, ModeManagerId};
use crate::framework::ids::{ButtonId, Modes};
use crate::midi::{from_hex_str, BindType};

// Knobs & Faders - MIDI CC

pub const LAUNCHCONTROL_KNOB_SEND_A_1: i32 = 0x0D;
pub const LAUNCHCONTROL_KNOB_SEND_B_1: i32 = 0x1D;
pub const LAUNCHCONTROL_KNOB_PAN_1: i32 = 0x31;
pub const LAUNCHCONTROL_FADER_1: i32 = 0x4D;

// Buttons - MIDI CC

pub const LAUNCHCONTROL_SEND_PREV: i32 = 0x68;
pub const LAUNCHCONTROL_SEND_NEXT: i32 = 0x69;
pub const LAUNCHCONTROL_TRACK_PREV: i32 = 0x6A;
pub const LAUNCHCONTROL_TRACK_NEXT: i32 = 0x6B;

// Buttons - MIDI Notes

pub const LAUNCHCONTROL_DEVICE: i32 = 0x69;
pub const LAUNCHCONTROL_MUTE: i32 = 0x6A;
pub const LAUNCHCONTROL_SOLO: i32 = 0x6B;
pub const LAUNCHCONTROL_RECORD_ARM: i32 = 0x6C;

pub const LAUNCHCONTROL_TRACK_FOCUS_1: i32 = 0x29;
pub const LAUNCHCONTROL_TRACK_FOCUS_5: i32 = 0x39;

pub const LAUNCHCONTROL_TRACK_CONTROL_1: i32 = 0x49;
pub const LAUNCHCONTROL_TRACK_CONTROL_5: i32 = 0x59;

/// The header of the template change system exclusive message (F0h 00h 20h 29h 02h 11h 77h
/// Template F7h).
pub const TEMPLATE_CHANGE_HEADER: [u8; 7] = [0xF0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x77];

/// The modes of the track button mode manager which are only temporarily active (former
/// `XLTemporaryButtonMode`).
pub const TEMPORARY_BUTTON_MODES: [Modes; 5] = [Modes::DeviceParams, Modes::InstrumentDeviceParams, Modes::Transport, Modes::Clip, Modes::Configuration];

/// The controller specific state (parts of the former `LaunchControlXLControlSurface` and
/// `LaunchControlXLConfiguration`, plus state which is shared between the modes).
pub struct XlState {
    /// The active template on the device (0-15), -1 if not yet known.
    pub template_id: i32,
    /// True if the device parameters are controlled by the 3rd knob row, otherwise panning.
    pub is_device_active: bool,
    /// The mode manager for the 2nd button row (former `getTrackButtonModeManager`).
    pub track_modes: ModeManagerId,
    /// The mode manager for the faders (former `getFaderModeManager`).
    pub fader_modes: ModeManagerId,
    /// The bind type and MIDI control of the buttons (they are re-bound on a template change).
    pub button_controls: Vec<(ButtonId, BindType, i32)>,
    /// The selected drum pad of the drum sequencer (former `XLDrumSequencerMode.selectedPad`).
    pub selected_pad: usize,
    /// The sequencer resolution index (former `XLDrumSequencerMode.selectedResolutionIndex`).
    pub drum_resolution_index: usize,
    /// The edited note of the drum pad edit mode (former `XLDrumPadEditMode.note`).
    pub drum_note: i32,
    /// The 'has been used' flags of the temporary button modes.
    temporary_mode_used: HashMap<Modes, bool>,
}

impl XlState {
    pub fn new(track_modes: ModeManagerId, fader_modes: ModeManagerId) -> Self {
        XlState {
            template_id: -1,
            is_device_active: false,
            track_modes,
            fader_modes,
            button_controls: Vec::new(),
            selected_pad: 0,
            drum_resolution_index: 4,
            drum_note: 0,
            temporary_mode_used: HashMap::new(),
        }
    }
}

pub fn xl(ctx: &Ctx) -> &XlState {
    ctx.ext::<XlState>()
}

pub fn xl_mut(ctx: &mut Ctx) -> &mut XlState {
    ctx.ext_mut::<XlState>()
}

pub fn track_modes(ctx: &Ctx) -> ModeManagerId {
    xl(ctx).track_modes
}

pub fn fader_modes(ctx: &Ctx) -> ModeManagerId {
    xl(ctx).fader_modes
}

/// Toggle between panning and device parameters control on the 3rd row (former
/// `toggleDeviceActive` of the configuration).
pub fn toggle_device_active(ctx: &mut Ctx) {
    let state = xl_mut(ctx);
    state.is_device_active = !state.is_device_active;
}

/// Reset the 'has been used' flag of a temporary button mode (former
/// `XLTemporaryButtonMode.onActivate`).
pub fn reset_has_been_used(ctx: &mut Ctx, mode: Modes) {
    xl_mut(ctx).temporary_mode_used.insert(mode, false);
}

/// Former `XLTemporaryButtonMode.setHasBeenUsed`.
pub fn set_has_been_used(ctx: &mut Ctx, mode: Modes) {
    xl_mut(ctx).temporary_mode_used.insert(mode, true);
}

/// Former `XLTemporaryButtonMode.hasBeenUsed`.
pub fn has_been_used(ctx: &Ctx, mode: Modes) -> bool {
    xl(ctx).temporary_mode_used.get(&mode).copied().unwrap_or(false)
}

/// Mark the active track button mode as used if it is a temporary one (former
/// `if (getTrackButtonModeManager ().getActive () instanceof XLTemporaryButtonMode tempMode)
/// tempMode.setHasBeenUsed ()`).
pub fn set_active_track_mode_used(ctx: &mut Ctx) {
    let manager = track_modes(ctx);
    if let Some(id) = ctx.mode_manager(manager).active_id() {
        if TEMPORARY_BUTTON_MODES.contains(&id) {
            set_has_been_used(ctx, id);
        }
    }
}

/// Send the state of a button LED (former `setTrigger`). The MIDI channel is the active
/// template. Nothing is sent as long as the template is not known.
pub fn set_trigger(ctx: &Ctx, bind_type: BindType, control: i32, value: i32) {
    let channel = xl(ctx).template_id;
    if channel < 0 {
        return;
    }
    if bind_type == BindType::Note {
        ctx.surface.output.send_note_ex(channel, control, value);
    } else {
        ctx.surface.output.send_cc_ex(channel, control, value);
    }
}

/// Set the color of a knob LED (former `setKnobLEDColor`). Red and green are in the range of
/// 0-3.
pub fn set_knob_led_color(ctx: &Ctx, row: usize, column: usize, green: i32, red: i32) {
    let midi_channel = xl(ctx).template_id;
    if midi_channel >= 8 {
        let g = green.clamp(0, 3);
        let r = red.clamp(0, 3);
        // Red is in bit 0 and 1. Green is in bit 4 and 5.
        ctx.surface.output.send_note_ex(midi_channel, 13 + row as i32 + column as i32 * 16, 12 + r + (g << 4));
    }
}

/// Select a template on the device (former `selectTemplate`).
pub fn select_template(ctx: &Ctx, template_id: i32) {
    let mut data = TEMPLATE_CHANGE_HEADER.to_vec();
    data.push(template_id as u8);
    data.push(0xF7);
    ctx.surface.output.send_sysex(&data);
}

/// Handle the system exclusive messages of the device (former `handleSysEx`).
pub fn handle_sysex(ctx: &mut Ctx, data: &str) {
    let byte_data = from_hex_str(data);

    // Handle template change - F0h 00h 20h 29h 02h 11h 77h Template F7h
    if byte_data.len() == 9 && byte_data[..TEMPLATE_CHANGE_HEADER.len()] == TEMPLATE_CHANGE_HEADER {
        let template_id = byte_data[7] as i32;
        // Already active?
        if xl(ctx).template_id == template_id {
            return;
        }
        xl_mut(ctx).template_id = template_id;
        super::setup::activate_template(ctx);
        return;
    }

    ctx.host.error(&format!("Unhandled Sysex: {data}"));
}
