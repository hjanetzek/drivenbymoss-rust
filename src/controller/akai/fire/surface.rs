// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Fire specific parts of the control surface (former `FireControlSurface`, `FirePadGrid`,
//! `FireScales` and `FireDisplay`) and the controller state in `ctx.ext` ([`FireState`]).
//!
//! The pads are sent as one system exclusive message with the RGB values of all changed pads;
//! blinking is emulated by toggling the colors every 600ms (former `BlinkingPadGrid`). The
//! display is rendered by the generic [`GraphicDisplay`] which is stored in
//! `ctx.graphics_display` and converted into 8 stripes of 1 bit pixels.

use std::time::Instant;

use super::colors::pad_color;
use crate::daw::color::ColorEx;
use crate::framework::color::ColorManager;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::graphics::{Bitmap, Component, GraphicDisplay, GraphicsConfiguration, GraphicsDimensions, LabelLayout};
use crate::framework::grid::{BlinkMode, LightInfo, PadGrid, BLINK_SPEED};
use crate::framework::ids::{ButtonId, OutputId};
use crate::midi::MidiOutput;
use crate::scale::Scales;

pub const CC_VOLUME: i32 = 0x10;
pub const CC_SELECT: i32 = 0x76;
pub const TOUCH_VOLUME: i32 = 0x10;

pub const SELECT: i32 = 0x19;
pub const FIRE_BANK: i32 = 0x1A;
pub const FIRE_PAT_UP: i32 = 0x1F;
pub const FIRE_PAT_DOWN: i32 = 0x20;
pub const FIRE_BROWSER: i32 = 0x21;
pub const FIRE_GRID_LEFT: i32 = 0x22;
pub const FIRE_GRID_RIGHT: i32 = 0x23;
pub const FIRE_SOLO_1: i32 = 0x24;
pub const FIRE_STEP: i32 = 0x2C;
pub const FIRE_NOTE: i32 = 0x2D;
pub const FIRE_DRUM: i32 = 0x2E;
pub const FIRE_PERFORM: i32 = 0x2F;
pub const FIRE_SHIFT: i32 = 0x30;
pub const FIRE_ALT: i32 = 0x31;
pub const FIRE_PATTERN: i32 = 0x32;
pub const FIRE_PLAY: i32 = 0x33;
pub const FIRE_STOP: i32 = 0x34;
pub const FIRE_REC: i32 = 0x35;

/// The CC of the knob mode LEDs.
pub const CC_KNOB_MODE_LEDS: i32 = 0x1B;
/// The first CC of the LEDs next to the solo buttons.
pub const CC_SOLO_LEDS: i32 = 0x28;

/// The controller notes of the pads (former `TRANSLATE_16x4_MATRIX`).
pub const TRANSLATE_16X4_MATRIX: [i32; 64] = [
    102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, //
    86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, //
    70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, //
    54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69,
];

// OLED (former FireDisplay)

const BIT_MUTATE: [[usize; 7]; 8] = [
    [13, 19, 25, 31, 37, 43, 49],
    [0, 20, 26, 32, 38, 44, 50],
    [1, 7, 27, 33, 39, 45, 51],
    [2, 8, 14, 34, 40, 46, 52],
    [3, 9, 15, 21, 41, 47, 53],
    [4, 10, 16, 22, 28, 48, 54],
    [5, 11, 17, 23, 29, 35, 55],
    [6, 12, 18, 24, 30, 36, 42],
];
pub const STRIPE_SIZE: usize = 147;
const PACKET_SIZE: usize = 4 + STRIPE_SIZE;
/// Send the display at least every 3 seconds to keep it from going into sleep mode.
const KEEP_ALIVE_MS: u128 = 3000;

pub type OledBitmap = [[u8; STRIPE_SIZE]; 8];

/// Convert the 128x64 bitmap into the 8 stripes of the OLED (former `FireDisplay.send`). A
/// pixel is on if the sum of its color bytes, read as signed bytes (Java), is negative.
pub fn encode_oled(image: &Bitmap, oled: &mut OledBitmap) {
    for (stripe, stripe_data) in oled.iter_mut().enumerate() {
        for (y, mutate) in BIT_MUTATE.iter().enumerate() {
            for x in 0..128usize {
                let [r, g, b] = image.pixel(x as i32, (stripe * 8 + y) as i32);
                let is_on = (r as i8 as i32) + (g as i8 as i32) + (b as i8 as i32) < 0;
                // Re-map by tiling 7x8 block of translated pixels
                let remap_bit = mutate[x % 7];
                let idx = x / 7 * 8 + remap_bit / 7;
                if let Some(byte) = stripe_data.get_mut(idx) {
                    if is_on {
                        *byte |= 1 << (remap_bit % 7);
                    } else {
                        *byte &= !(1 << (remap_bit % 7));
                    }
                }
            }
        }
    }
}

/// The system exclusive message of one stripe of the OLED.
pub fn oled_stripe_message(stripe: usize, data: &[u8; STRIPE_SIZE]) -> Vec<u8> {
    let mut message = Vec::with_capacity(12 + STRIPE_SIZE);
    // AKAI, All-Call, Fire, WRITE OLED, payload length
    message.extend_from_slice(&[0xF0, 0x47, 0x7F, 0x43, 0x0E, (PACKET_SIZE / 128) as u8, (PACKET_SIZE % 128) as u8]);
    // Start and end 8-pixel band, start and end column
    message.extend_from_slice(&[stripe as u8, stripe as u8, 0x00, 0x7F]);
    message.extend_from_slice(data);
    message.push(0xF7);
    message
}

/// The controller specific state.
pub struct FireState {
    /// The pad brightness in the range [0.1..1].
    pub pad_brightness: f64,
    /// The pad saturation in the range [0..1].
    pub pad_saturation: f64,
    oled: OledBitmap,
    old_oled: OledBitmap,
    last_send: Option<Instant>,
    /// The stripes sent last (for tests and debugging).
    pub sent_stripes: usize,
}

impl Default for FireState {
    fn default() -> Self {
        FireState { pad_brightness: 1.0, pad_saturation: 1.0, oled: [[0; STRIPE_SIZE]; 8], old_oled: [[0; STRIPE_SIZE]; 8], last_send: None, sent_stripes: 0 }
    }
}

pub fn state(ctx: &Ctx) -> &FireState {
    ctx.ext::<FireState>()
}

pub fn state_mut(ctx: &mut Ctx) -> &mut FireState {
    ctx.ext_mut::<FireState>()
}

/// The grid note to controller translation (former `FirePadGrid.translateToController`).
fn translate_to_controller(note: i32) -> (i32, i32) {
    let index = (note - 36).clamp(0, 63) as usize;
    (0, TRANSLATE_16X4_MATRIX[index])
}

/// Create the pad grid (former `FirePadGrid`).
pub fn create_pad_grid(colors: &ColorManager) -> PadGrid {
    PadGrid::new(colors, 4, 16, 36).with_translation(translate_to_controller).with_blink_mode(BlinkMode::Emulated)
}

/// The scales of the Fire (former `FireScales`): 16x4 pads, the matrix is translated to the
/// controller notes of the pads.
pub fn create_scales(value_changer: &crate::framework::value_changer::ValueChanger) -> Scales {
    Scales::new(value_changer, 36, 100, 16, 4).with_grid_translator(Scales::table_grid_translator(36, TRANSLATE_16X4_MATRIX.to_vec()))
}

/// Create the graphics display (former `FireDisplay` with `ChromaticGraphicsConfiguration`).
pub fn create_display(max_parameter_value: i32) -> GraphicDisplay {
    GraphicDisplay::new(GraphicsConfiguration::chromatic(), GraphicsDimensions::new(128, 64, max_parameter_value))
}

/// The graphics display.
pub fn display(ctx: &mut Ctx) -> &mut GraphicDisplay {
    ctx.graphics_display.as_mut().expect("The Fire has a graphics display")
}

/// Add a component to the display.
pub fn add_element(ctx: &mut Ctx, component: Component) {
    display(ctx).add_element(component);
}

/// Install the trigger sender: the button LEDs are sent as CC (former `setTrigger`).
pub fn install_trigger_sender(ctx: &mut Ctx) {
    ctx.surface.trigger_sender = std::rc::Rc::new(|output, _bind_type, channel, cc, state| output.send_cc_ex(channel, cc, state));
}

/// Is ALT pressed?
pub fn is_alt_pressed(ctx: &Ctx) -> bool {
    ctx.surface.is_pressed(ButtonId::Alt)
}

/// The mode button (BANK) is used as the delete button for resetting knobs (former
/// `isDeletePressed`): consumes the button if it is pressed.
pub fn is_delete_pressed(ctx: &mut Ctx) -> bool {
    let pressed = ctx.surface.is_pressed(ButtonId::BankRight);
    if pressed {
        ctx.surface.set_trigger_consumed(ButtonId::BankRight);
    }
    pressed
}

/// Call a function with the active mode. If the mode is borrowed (the event comes from the mode)
/// nothing happens.
pub fn with_active_mode(ctx: &mut Ctx, f: impl FnOnce(&mut dyn Mode, &mut Ctx)) {
    let Some(mode) = ctx.active_mode() else {
        return;
    };
    let Ok(mut mode) = mode.try_borrow_mut() else {
        return;
    };
    f(&mut *mode, ctx);
}

/// Update the pad brightness and saturation from the configuration (former `configureLEDs`).
pub fn configure_leds(ctx: &mut Ctx) {
    let brightness = (0.1 + 0.9 * crate::config::fire::pad_brightness(&ctx.config) as f64 / 100.0).clamp(0.1, 1.0);
    let saturation = crate::config::fire::pad_saturation(&ctx.config) as f64 / 100.0;
    let state = state_mut(ctx);
    state.pad_brightness = brightness;
    state.pad_saturation = saturation;
    // Force sending all pads
    ctx.surface.grid().force_flush();
}

fn scaled_rgb(cm: &ColorManager, color_index: i32, brightness: f64, saturation: f64, keep_black: bool) -> [i32; 3] {
    let mut color = pad_color(cm, color_index);
    // Do not scale black!
    if !keep_black || color != ColorEx::BLACK {
        color = color.scale(brightness, saturation);
    }
    color.to_int_rgb127()
}

/// Send the changed pads and the blinking pads (former `FirePadGrid.updateController`).
pub fn flush_pads(ctx: &mut Ctx) {
    let now = ctx.host.now();
    let (brightness, saturation) = {
        let state = state(ctx);
        (state.pad_brightness, state.pad_saturation)
    };
    let Some(grid) = ctx.surface.pad_grid.as_mut() else {
        return;
    };
    let mut data = Vec::new();
    let mut count = 0;
    let infos = std::mem::take(&mut grid.blinking.pad_infos);
    for (note, info) in infos {
        let index = note - 54;
        let c = scaled_rgb(&ctx.colors, info.color, brightness, saturation, true);
        data.extend_from_slice(&[index, c[0], c[1], c[2]]);
        count += 1;
        // Hardware does not support blinking, therefore needs to be implemented the hard way
        if info.blink_color > 0 {
            grid.blinking.blinking_lights.insert(index, info);
        } else {
            grid.blinking.blinking_lights.remove(&index);
        }
    }

    // Toggle blink colors every 600ms
    let blinking = &mut grid.blinking;
    let toggle = match blinking.update_time {
        None => {
            blinking.update_time = Some(now);
            false
        }
        Some(time) if now.duration_since(time).as_millis() > BLINK_SPEED => {
            blinking.update_time = Some(now);
            blinking.is_blink = !blinking.is_blink;
            true
        }
        _ => false,
    };
    if toggle {
        let is_blink = blinking.is_blink;
        for (index, info) in &blinking.blinking_lights {
            let color_index = if is_blink { info.blink_color } else { info.color };
            let c = scaled_rgb(&ctx.colors, color_index, brightness, saturation, false);
            data.extend_from_slice(&[*index, c[0], c[1], c[2]]);
            count += 1;
        }
    }

    // No update necessary
    if count == 0 {
        return;
    }
    let length = count * 4;
    let mut message = vec![0xF0, 0x47, 0x7F, 0x43, 0x65, (length / 128) as u8, (length % 128) as u8];
    message.extend(data.iter().map(|v| (*v).clamp(0, 127) as u8));
    message.push(0xF7);
    ctx.surface.output.send_sysex(&message);
}

/// Render the display and send the changed stripes (former `AbstractGraphicDisplay.send` and
/// `FireDisplay.send`).
pub fn send_display(ctx: &mut Ctx) {
    let now = ctx.host.now();
    let Some(display) = ctx.graphics_display.as_mut() else {
        return;
    };
    // Borrow the bitmap and the controller state as separate fields of the context, so that the
    // bitmap (24 KB) is not copied on every flush
    let image = display.render(now);
    let state = ctx.ext.downcast_mut::<FireState>().expect("Wrong controller extension type");
    encode_oled(image, &mut state.oled);
    let output: MidiOutput = ctx.surface.output.clone();
    state.sent_stripes = 0;
    for stripe in 0..8 {
        // Send if content has changed or every 3 seconds if there was no change to keep the
        // display from going into sleep mode
        let is_alive = state.last_send.is_some_and(|last| now.duration_since(last).as_millis() < KEEP_ALIVE_MS);
        if state.oled[stripe] == state.old_oled[stripe] && is_alive {
            continue;
        }
        state.old_oled[stripe] = state.oled[stripe];
        state.last_send = Some(now);
        state.sent_stripes += 1;
        output.send_sysex(&oled_stripe_message(stripe, &state.oled[stripe]));
    }
}

/// Former `internalShutdown`: show "Goodbye", turn off the solo LEDs and the knob mode LEDs.
pub fn shutdown(ctx: &mut Ctx) {
    if let Some(display) = ctx.graphics_display.as_mut() {
        display.cancel_notification();
        display.add_element(Component::label("Goodbye", Some(ColorEx::BLACK), false, false, LabelLayout::Plain));
    }
    send_display(ctx);
    for i in 0..4 {
        ctx.surface.output.send_cc_ex(0, CC_SOLO_LEDS + i, 0);
    }
    // Switch off knob mode LEDs
    ctx.surface.output.send_cc_ex(0, CC_KNOB_MODE_LEDS, 16);
    ctx.surface.grid().turn_off();
    ctx.flush_lights();
    flush_pads(ctx);
}

/// The output ID of the LED of a knob mode or solo button.
pub fn led(index: usize) -> OutputId {
    OutputId::get(OutputId::Led1, index)
}

/// A light info for the pads (former `LightInfo` constructor of the Fire views).
pub fn light_info(color: i32, blink_color: i32, fast: bool) -> LightInfo {
    LightInfo::new(color, blink_color, fast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oled_encoding() {
        let mut image = Bitmap::new(128, 64);
        let mut oled = [[0u8; STRIPE_SIZE]; 8];
        encode_oled(&image, &mut oled);
        assert!(oled.iter().all(|s| s.iter().all(|b| *b == 0)));

        // Pixel 0/0 is bit 13 of the remap: index 1, bit 6
        image.set_pixel(0, 0, [255, 255, 255]);
        // Pixel 1/1 is bit 20: index 2, bit 6 of stripe 0
        image.set_pixel(1, 1, [255, 255, 255]);
        // A dark pixel (sum of signed bytes is positive) is off
        image.set_pixel(2, 2, [100, 0, 0]);
        // Pixel 0/8 is in stripe 1
        image.set_pixel(0, 8, [128, 0, 0]);
        encode_oled(&image, &mut oled);
        assert_eq!(oled[0][1], 1 << 6);
        assert_eq!(oled[0][2], 1 << 6);
        assert_eq!(oled[0].iter().filter(|b| **b != 0).count(), 2);
        assert_eq!(oled[1][1], 1 << 6);

        let message = oled_stripe_message(3, &oled[0]);
        assert_eq!(message.len(), 12 + STRIPE_SIZE);
        assert_eq!(message[..11], [0xF0, 0x47, 0x7F, 0x43, 0x0E, 1, 23, 3, 3, 0, 0x7F]);
        assert_eq!(*message.last().unwrap(), 0xF7);
    }

    #[test]
    fn translation() {
        assert_eq!(translate_to_controller(36), (0, 102));
        assert_eq!(translate_to_controller(99), (0, 69));
        let mut matrix = Scales::empty_matrix();
        matrix[36] = 60;
        matrix[84] = 61;
        let grid = Scales::table_grid_translator(36, TRANSLATE_16X4_MATRIX.to_vec())(&matrix);
        assert_eq!(grid[102], 60);
        assert_eq!(grid[54], 61);
    }
}
