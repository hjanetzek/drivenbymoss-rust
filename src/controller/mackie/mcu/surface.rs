// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The state of a MCU surface (former `MCUControlSurface`), stored in `ctx.ext` of each surface,
//! and the functions which send to the hardware of the current surface.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::definition::{KNOB_LED_MODE_BOOST_CUT, KNOB_LED_MODE_SINGLE_DOT, SYSEX_HDR, VUMODE_LCD, VUMODE_LED, VUMODE_LED_AND_LCD, VUMODE_OFF};
use super::display::SharedDisplayOptions;
use crate::config::mcu::{DisplayColors, MainDisplay, McuConfig, McuDeviceType};
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;

/// The state shared by all surfaces of an instance.
pub struct McuShared {
    /// The number of surfaces.
    pub num_surfaces: usize,
    /// The options of all text displays.
    pub display_options: SharedDisplayOptions,
    /// The jog wheel controls the last touched parameter (former
    /// `JogWheelCommand.controlLastParamActive`).
    pub jog_wheel_last_param: Rc<Cell<bool>>,
    /// The touched faders of all surfaces (former static `FaderTouchCommand.isTrackTouched`).
    pub touched_faders: [bool; 32],
}

/// The state of a MCU surface.
pub struct McuSurfaceState {
    /// The index of the surface (former `getSurfaceID`).
    pub index: usize,
    /// The index of the first track of the surface (8 per surface).
    pub extender_offset: usize,
    pub is_main_device: bool,
    pub device_type: McuDeviceType,
    pub shared: Rc<RefCell<McuShared>>,
    /// The second text display (former `getTextDisplay (1)`); the first one is
    /// `ctx.text_display`.
    pub second_display: TextDisplay,
    /// The 7-segment display of the position (former `getTextDisplay (2)`).
    pub segment_display: TextDisplay,
    /// The 2 character mode display (former `getTextDisplay (3)`).
    pub assignment_display: TextDisplay,
    /// The master fader mode of the main device (former `MasterVolumeMode`), see
    /// `super::mode::MasterVolume`.
    pub master_volume: Option<super::mode::MasterVolume>,
    knob_values: [i32; 8],
    current_colors: Vec<u8>,
    current_wide_colors: [u16; 8],
    current_asparion_colors: [[i32; 3]; 8],
    item_indices: Vec<i32>,
    active_vu_mode: i32,
}

impl McuSurfaceState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: usize,
        device_type: McuDeviceType,
        shared: Rc<RefCell<McuShared>>,
        second_display: TextDisplay,
        segment_display: TextDisplay,
        assignment_display: TextDisplay,
    ) -> Self {
        McuSurfaceState {
            index,
            extender_offset: 8 * index,
            is_main_device: device_type == McuDeviceType::Main,
            device_type,
            shared,
            second_display,
            segment_display,
            assignment_display,
            master_volume: None,
            knob_values: [-1; 8],
            current_colors: vec![0xFF; 8],
            current_wide_colors: [0; 8],
            current_asparion_colors: [[-1; 3]; 8],
            item_indices: Vec::new(),
            active_vu_mode: VUMODE_LED,
        }
    }

    /// Former `isLastDevice`.
    pub fn is_last_device(&self) -> bool {
        self.index + 1 == self.shared.borrow().num_surfaces
    }
}

/// The state of the current surface.
pub fn mcu(ctx: &Ctx) -> &McuSurfaceState {
    ctx.ext::<McuSurfaceState>()
}

pub fn mcu_mut(ctx: &mut Ctx) -> &mut McuSurfaceState {
    ctx.ext_mut::<McuSurfaceState>()
}

/// The index of the first track of the current surface (former `getExtenderOffset`).
pub fn extender_offset(ctx: &Ctx) -> usize {
    mcu(ctx).extender_offset
}

/// Set the LED ring of a knob (former `setKnobLED`).
pub fn set_knob_led(ctx: &mut Ctx, index: usize, knob_led_mode: i32, value: i32, max_value: i32) {
    let is_asparion = ctx.config.display_colors() == DisplayColors::Asparion;
    let max = max_value.max(1) as f64;
    let v = if is_asparion {
        (value as f64 * 127.0 / max).round() as i32
    } else {
        // value = 0 turns the LEDs off, value range is 1-11, center is 6
        let mut rescale = (value as f64 * 11.0 / max).round() as i32;
        if value > 0 && rescale == 0 {
            rescale = 1;
        }
        (knob_led_mode << 4) + rescale
    };
    let state = mcu_mut(ctx);
    if index >= 8 || state.knob_values[index] == v {
        return;
    }
    state.knob_values[index] = v;
    let output = &ctx.surface.output;
    if is_asparion {
        output.send_cc_ex(if knob_led_mode == KNOB_LED_MODE_BOOST_CUT { 2 } else { 1 }, 0x30 + index as i32, v);
    } else {
        output.send_cc(0x30 + index as i32, v);
    }
}

/// Send the item indices to an Asparion display (former `setItemIndices`).
pub fn set_item_indices(ctx: &mut Ctx, values: &[i32]) {
    if ctx.config.main_display_type() != MainDisplay::Asparion || mcu(ctx).item_indices == values {
        return;
    }
    mcu_mut(ctx).item_indices = values.to_vec();
    let mut msg = format!("{SYSEX_HDR}17 00 ");
    for item_index in values {
        msg.push_str(&format!("{:02X} ", item_index.rem_euclid(128)));
    }
    msg.push_str("F7");
    ctx.surface.output.send_sysex_str(&msg);
}

/// Switch the VU meter mode: LEDs on, LEDs and VU meter on display on, VU meter on display on,
/// all off (former `switchVuMode`).
pub fn switch_vu_mode(ctx: &mut Ctx, mode: i32) {
    let output = ctx.surface.output.clone();
    // Always horizontal
    output.send_sysex_str(&format!("{SYSEX_HDR}21 00 F7"));
    let state = mcu_mut(ctx);
    if state.active_vu_mode != mode {
        if state.active_vu_mode < 5 {
            state.active_vu_mode = mode;
        } else {
            state.active_vu_mode = VUMODE_LED;
        }
    }
    // The MCU changes the VU-meter mode when receiving the corresponding system exclusive
    // message
    let value = match state.active_vu_mode {
        VUMODE_LED => "01",
        VUMODE_LED_AND_LCD => "03",
        VUMODE_LCD => "06",
        VUMODE_OFF => "00",
        _ => return,
    };
    for i in 0..8 {
        // Resets the LEDs (and VU-meters on the display?)
        output.send_channel_aftertouch(0, i << 4, 0);
        output.send_sysex_str(&format!("{SYSEX_HDR}20 0{i} {value} F7"));
    }
}

fn to_index(color: ColorEx) -> u8 {
    let (red, green, blue) = (color.red(), color.green(), color.blue());
    if red < 0.5 && green < 0.5 && blue < 0.5 && (red > 0.0 || green > 0.0 || blue > 0.0) {
        // Use white for dim colors
        return 7;
    }
    let has_red = u8::from(red >= 0.5);
    let has_green = u8::from(green >= 0.5);
    let has_blue = u8::from(blue >= 0.5);
    has_blue * 4 + has_green * 2 + has_red
}

fn to_behringer_125(color: ColorEx) -> u16 {
    let component = |c: f64| ((c * 5.0) as u16).min(4);
    (component(color.green()) << 12) | (component(color.red()) << 8) | component(color.blue())
}

fn notification_colors(ctx: &Ctx, colors: &[ColorEx; 8]) -> [ColorEx; 8] {
    if mcu(ctx).is_main_device && ctx.text_display.as_ref().is_some_and(|d| d.is_notification_active()) {
        [ColorEx::WHITE; 8]
    } else {
        *colors
    }
}

/// Send the colors of the display cells (former `sendDisplayColor`).
pub fn send_display_color(ctx: &mut Ctx, colors: &[ColorEx; 8]) {
    let is_main_device = mcu(ctx).is_main_device;
    match ctx.config.display_colors() {
        DisplayColors::Behringer122 => {
            let cs = notification_colors(ctx, colors);
            let display_colors: Vec<u8> = cs.iter().map(|c| to_index(*c)).collect();
            if display_colors == mcu(ctx).current_colors {
                return;
            }
            let mut msg = vec![0xF0, 0x00, 0x00, 0x66, if is_main_device { 0x14 } else { 0x15 }, 0x72];
            msg.extend_from_slice(&display_colors);
            msg.push(0xF7);
            mcu_mut(ctx).current_colors = display_colors;
            ctx.surface.output.send_sysex(&msg);
        }
        DisplayColors::Behringer125 => {
            let cs = notification_colors(ctx, colors);
            let mut wide = [0u16; 8];
            for (i, c) in cs.iter().enumerate() {
                wide[i] = to_behringer_125(*c);
            }
            if wide == mcu(ctx).current_wide_colors {
                return;
            }
            mcu_mut(ctx).current_wide_colors = wide;
            let mut msg = vec![0xF0, 0x00, 0x00, 0x66, if is_main_device { 0x14 } else { 0x15 }, 0x73];
            for w in wide {
                msg.push((w >> 8) as u8);
                msg.push((w & 0xFF) as u8);
            }
            msg.push(0xF7);
            ctx.surface.output.send_sysex(&msg);
        }
        DisplayColors::Icon => {
            let mut msg = vec![0xF0, 0x00, 0x02, 0x4E, 0x16, 0x14];
            for c in colors {
                let rgb = c.to_int_rgb127();
                msg.extend(rgb.iter().map(|v| *v as u8));
            }
            msg.push(0xF7);
            if msg == mcu(ctx).current_colors {
                return;
            }
            mcu_mut(ctx).current_colors = msg.clone();
            ctx.surface.output.send_sysex(&msg);
        }
        DisplayColors::Asparion => {
            for (i, c) in colors.iter().enumerate() {
                let rgb = c.to_int_rgb127();
                // Same as Java: stops at the first unchanged color
                if rgb == mcu(ctx).current_asparion_colors[i] {
                    return;
                }
                mcu_mut(ctx).current_asparion_colors[i] = rgb;
                let note = 0x20 + i as i32;
                let output = &ctx.surface.output;
                output.send_note_ex(1, note, rgb[0]);
                output.send_note_ex(2, note, rgb[1]);
                output.send_note_ex(3, note, rgb[2]);
            }
        }
        DisplayColors::None => {}
    }
}

/// Reset the controls of the current surface on shutdown (former `internalShutdown`).
pub fn internal_shutdown(ctx: &mut Ctx) {
    let output = ctx.surface.output.clone();
    for i in 0..8 {
        output.send_channel_aftertouch(0, 0x10 * i, 0);
        output.send_pitchbend(i, 0, 0);
    }
    output.send_channel_aftertouch(1, 0, 0);
    output.send_channel_aftertouch(1, 0x10, 0);
    output.send_pitchbend(8, 0, 0);

    set_item_indices(ctx, &[0; 8]);

    // Switch off knob LEDs
    for i in 0..8 {
        set_knob_led(ctx, i, KNOB_LED_MODE_SINGLE_DOT, 0, 127);
    }

    let color = if ctx.config.display_colors() == DisplayColors::Asparion { ColorEx::BLACK } else { ColorEx::WHITE };
    send_display_color(ctx, &[color; 8]);
}
