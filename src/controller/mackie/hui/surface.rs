// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The state of a HUI surface (former `HUIControlSurface`), stored in `ctx.ext` of each surface,
//! and the MIDI protocol: the buttons are selected by a zone and a port (CC 0x0F / 0x2F), the
//! faders have a high and a low byte, the knobs are relative.

use std::collections::HashMap;

use super::definition::KNOB_LED_MODE_OFF;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::ids::{ButtonId, ContinuousId};
use crate::framework::value_changer::{RelativeEncoding, ValueChanger};
use crate::midi::{ButtonEvent, CMD_CC};

/// The state of a HUI surface.
pub struct HuiSurfaceState {
    /// The index of the surface (former `getSurfaceID`).
    pub index: usize,
    /// The index of the first track of the surface (8 per surface).
    pub extender_offset: usize,
    /// The position/tempo display (former `getSegmentDisplay`); the text display is
    /// `ctx.text_display`.
    pub segment_display: TextDisplay,
    /// The buttons by their HUI control (zone * 8 + port).
    pub hui_buttons: HashMap<i32, ButtonId>,
    knob_values: [i32; 8],
    vu_values_left: [i32; 8],
    vu_values_right: [i32; 8],
    fader_values: [i32; 9],
    /// The currently selected zone (area of a group of buttons).
    zone: i32,
    fader_hi_values: [i32; 9],
}

impl HuiSurfaceState {
    pub fn new(index: usize, segment_display: TextDisplay) -> Self {
        HuiSurfaceState {
            index,
            extender_offset: 8 * index,
            segment_display,
            hui_buttons: HashMap::new(),
            knob_values: [-1; 8],
            vu_values_left: [-1; 8],
            vu_values_right: [-1; 8],
            fader_values: [-1; 9],
            zone: 0,
            fader_hi_values: [0; 9],
        }
    }
}

/// The state of the current surface.
pub fn hui(ctx: &Ctx) -> &HuiSurfaceState {
    ctx.ext::<HuiSurfaceState>()
}

pub fn hui_mut(ctx: &mut Ctx) -> &mut HuiSurfaceState {
    ctx.ext_mut::<HuiSurfaceState>()
}

/// The index of the first track of the current surface (former `getExtenderOffset`).
pub fn extender_offset(ctx: &Ctx) -> usize {
    hui(ctx).extender_offset
}

/// Turn the LED of a button on or off (former `setTrigger`): select the zone, then the port.
pub fn set_trigger(ctx: &Ctx, control: i32, value: i32) {
    let output = &ctx.surface.output;
    // Select the zone
    output.send_cc(0x0C, control / 8);
    // Turn on / off button
    output.send_cc(0x2C, if value > 0 { 0x40 } else { 0x00 } + control % 8);
}

/// Set the LED ring of a knob (former `setKnobLED`).
pub fn set_knob_led(ctx: &mut Ctx, index: usize, knob_led_mode: i32, value: i32, max_value: i32) {
    let v = if knob_led_mode == KNOB_LED_MODE_OFF {
        0
    } else {
        let rescale = (value as f64 * 10.0 / max_value.max(1) as f64).round() as i32;
        0x10 * knob_led_mode + rescale + 1
    };
    let state = hui_mut(ctx);
    if index >= 8 || state.knob_values[index] == v {
        return;
    }
    state.knob_values[index] = v;
    ctx.surface.output.send_cc(0x10 + index as i32, v);
}

/// Move the motor fader (former `updateFaders`).
pub fn update_fader(ctx: &mut Ctx, channel: usize, value: i32) {
    let state = hui_mut(ctx);
    if state.fader_values[channel] == value {
        return;
    }
    state.fader_values[channel] = value;
    let output = &ctx.surface.output;
    output.send_cc(channel as i32, value / 128);
    output.send_cc(0x20 + channel as i32, value % 128);
}

/// Update the VU meters of a channel (former `updateVuMeters`).
pub fn update_vu_meters(ctx: &mut Ctx, channel: usize, vu_left: i32, vu_right: i32, upper_bound: f64) {
    if hui(ctx).vu_values_left[channel] != vu_left {
        hui_mut(ctx).vu_values_left[channel] = vu_left;
        let scaled = (vu_left as f64 * 12.0 / upper_bound).floor() as i32;
        ctx.surface.output.send_polyphonic_aftertouch(0, channel as i32, scaled);
    }
    if hui(ctx).vu_values_right[channel] != vu_right {
        hui_mut(ctx).vu_values_right[channel] = vu_right;
        let scaled = (vu_right as f64 * 12.0 / upper_bound).floor() as i32;
        ctx.surface.output.send_polyphonic_aftertouch(0, channel as i32, 0x10 + scaled);
    }
}

fn execute_continuous(ctx: &mut Ctx, id: ContinuousId, value: i32) {
    if let Some(command) = ctx.surface.continuous(id).and_then(|c| c.command.clone()) {
        if let Ok(mut command) = command.try_borrow_mut() {
            command.execute(ctx, value);
        }
    }
}

/// Handle the MIDI of the HUI (former `HUIControlSurface.handleMidi`, only CC).
pub fn handle_midi(ctx: &mut Ctx, status: i32, data1: i32, data2: i32) {
    if status & 0xF0 != CMD_CC {
        return;
    }
    match data1 {
        // Move fader high-byte (0x08 is an iCON extension)
        0x00..=0x08 => hui_mut(ctx).fader_hi_values[data1 as usize] = data2,
        0x0D => {
            let encoder = ValueChanger::twos_complement(128, 1);
            let decoder = ValueChanger::new(RelativeEncoding::SignedBit, 128, 1);
            let d = encoder.encode(decoder.decode(data2));
            execute_continuous(ctx, ContinuousId::PlayPosition, d);
        }
        // Button zone selection
        0x0F => hui_mut(ctx).zone = data2,
        // Move fader low-byte: the commands get the high resolution value (former
        // `WorkaroundFader.executeHiRes`; `execute (value * 128)` was for the simulator)
        0x20..=0x27 => {
            let channel = (data1 - 0x20) as usize;
            let value = (hui(ctx).fader_hi_values[channel] << 7) + data2;
            execute_continuous(ctx, ContinuousId::get(ContinuousId::Fader1, channel), value);
        }
        0x28 => {
            let value = (hui(ctx).fader_hi_values[8] << 7) + data2;
            execute_continuous(ctx, ContinuousId::FaderMaster, value);
        }
        // Button port up/down (a button in the selected row)
        0x2F => {
            let is_down = data2 >= 0x40;
            let zone = hui(ctx).zone;
            let button_index = zone * 8 + data2 % 8;
            match hui(ctx).hui_buttons.get(&button_index).copied() {
                Some(id) => {
                    if is_down {
                        ctx.trigger_button(id, ButtonEvent::Down, 1.0);
                    } else {
                        ctx.trigger_button(id, ButtonEvent::Up, 0.0);
                    }
                }
                None => ctx.host.error(&format!("Button {button_index} not supported (Zone: {zone} Index: {})", data2 % 8)),
            }
        }
        0x40..=0x47 => {
            let channel = (data1 - 0x40) as usize;
            let v = if data2 > 0x40 { data2 - 0x40 } else { 128 - data2 };
            execute_continuous(ctx, ContinuousId::get(ContinuousId::Knob1, channel), v);
        }
        _ => ctx.host.println(&format!("Unhandled MIDI CC: {data1}")),
    }
}
