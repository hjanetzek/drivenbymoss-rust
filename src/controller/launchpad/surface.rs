// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Launchpad specific parts of the control surface (former `LaunchpadControlSurface` and
//! `LaunchpadPadGrid`).

use std::cell::RefCell;
use std::rc::Rc;

use super::definition::{LaunchpadDefinition, LogoCommand, SCENE_CONTROLS};
use crate::framework::color::ColorManager;
use crate::framework::ctx::Ctx;
use crate::framework::grid::{BlinkMode, PadGrid};
use crate::framework::virtual_fader::{self, VirtualFader, VirtualFaderCallback};
use crate::midi::{BindType, DeviceInquiry, MidiOutput};

pub const LAUNCHPAD_FADER_1: i32 = 21;
pub const PRO3_LAUNCHPAD_FIXED_LENGTH: i32 = 30;
pub const LAUNCHPAD_TRACK1: i32 = 101;

pub const LAUNCHPAD_BUTTON_STATE_OFF: i32 = 0;
pub const LAUNCHPAD_BUTTON_STATE_ON: i32 = 1;
pub const LAUNCHPAD_BUTTON_STATE_HI: i32 = 4;

/// Translates the grid notes (36..99) to the Launchpad programmer mode notes (11..88).
pub fn translate_to_controller(note: i32) -> (i32, i32) {
    let index = note - 36;
    (0, (index / 8 + 1) * 10 + index % 8 + 1)
}

/// Translates a Launchpad programmer mode note (11..88) back to the grid note.
pub fn translate_from_controller(note: i32) -> Option<i32> {
    let (row, col) = (note / 10, note % 10);
    if (1..=8).contains(&row) && (1..=8).contains(&col) {
        Some(36 + (row - 1) * 8 + col - 1)
    } else {
        None
    }
}

/// Create the 8x8 pad grid of the Launchpad.
pub fn create_pad_grid(colors: &ColorManager) -> PadGrid {
    PadGrid::new(colors, 8, 8, 36).with_translation(translate_to_controller).with_blink_mode(BlinkMode::Collect)
}

/// The Launchpad specific state of a controller instance (stored in [`Ctx::ext`]).
pub struct LaunchpadExt {
    pub definition: LaunchpadDefinition,
    /// The 8 virtual faders drawn into the columns of the pad grid.
    pub virtual_faders: Vec<Rc<RefCell<VirtualFader>>>,
    /// Set by the session view (former `SelectSessionViewCommand.setTemporary`): the session
    /// view was used while the Session button was held, the view is restored when the button is
    /// released. The select session view command resets it on button down.
    pub session_temporary: bool,
}

impl LaunchpadExt {
    pub fn new(definition: LaunchpadDefinition) -> Self {
        let virtual_faders = (0..8)
            .map(|i| VirtualFader::new(Rc::new(super::view::virtual_fader_view_callback::VirtualFaderViewCallback::new(i)), Some(i as i32)).shared())
            .collect();
        LaunchpadExt { definition, virtual_faders, session_temporary: false }
    }
}

/// Set the color of a virtual fader and if it is a panorama fader.
pub fn setup_fader(ctx: &mut Ctx, index: usize, color: i32, is_pan: bool) {
    ext(ctx).virtual_faders[index].borrow_mut().setup(color, is_pan);
}

/// Set the value of a virtual fader and draw it into its column of the pad grid.
pub fn set_fader_value(ctx: &mut Ctx, index: usize, value: i32) {
    let fader = ext(ctx).virtual_faders[index].clone();
    fader.borrow_mut().set_value(ctx.surface.pad_grid.as_mut(), value);
}

/// Reset all virtual faders.
pub fn clear_faders(ctx: &mut Ctx) {
    for i in 0..8 {
        setup_fader(ctx, i, -1, false);
    }
}

/// Move a virtual fader to the value of the given row. Fader views which call this from their
/// own functions must hand in themselves as the `direct` callback (the active view is borrowed
/// in that case), see [`virtual_fader::move_to`].
pub fn move_fader(ctx: &mut Ctx, index: usize, row: i32, velocity: i32, direct: Option<&dyn VirtualFaderCallback>) {
    let fader = ext(ctx).virtual_faders[index].clone();
    virtual_fader::move_to(&fader, ctx, row, velocity, direct);
}

/// Get the Launchpad state of the context.
pub fn ext(ctx: &Ctx) -> &LaunchpadExt {
    ctx.ext::<LaunchpadExt>()
}

pub fn ext_mut(ctx: &mut Ctx) -> &mut LaunchpadExt {
    ctx.ext_mut::<LaunchpadExt>()
}

pub fn is_pro(ctx: &Ctx) -> bool {
    ext(ctx).definition.is_pro()
}

pub fn has_track_selection_buttons(ctx: &Ctx) -> bool {
    ext(ctx).definition.has_track_selection_buttons()
}

/// Send a Launchpad sysex command (the header and the closing F7 are added).
pub fn send_launchpad_sysex(output: &MidiOutput, definition: &LaunchpadDefinition, data: &str) {
    output.send_sysex_str(&format!("{}{} F7", definition.sysex_header(), data));
}

pub fn set_launchpad_to_standalone(ctx: &mut Ctx) {
    let definition = ext(ctx).definition.clone();
    send_launchpad_sysex(&ctx.surface.output, &definition, definition.standalone_mode_command());
}

/// Switch the Launchpad to programmer mode and force an update of all button lights.
pub fn set_launchpad_to_prg_mode(ctx: &mut Ctx) {
    let definition = ext(ctx).definition.clone();
    send_launchpad_sysex(&ctx.surface.output, &definition, definition.program_mode_command());
    // Note: the Java condition (PAD1 < key || PAD64 > key) is true for all buttons
    ctx.surface.flush_button_leds();
}

/// Reset the Launchpad to its default mode (on shutdown).
pub fn reset_mode(ctx: &mut Ctx) {
    let definition = ext(ctx).definition.clone();
    if let Some(command) = definition.reset_mode_command() {
        send_launchpad_sysex(&ctx.surface.output, &definition, command);
    }
}

pub fn set_logo_color(ctx: &mut Ctx, color: i32) {
    let definition = ext(ctx).definition.clone();
    match definition.logo_command(color) {
        LogoCommand::None => {}
        LogoCommand::Sysex(data) => send_launchpad_sysex(&ctx.surface.output, &definition, &data),
        LogoCommand::Trigger(cc, value) => ctx.surface.set_trigger_cc(cc, value),
    }
}

/// Send the configured pad brightness to the device (if supported).
pub fn update_brightness(ctx: &mut Ctx, pad_brightness: i32) {
    if let Some(sysex) = ext(ctx).definition.brightness_sysex(pad_brightness) {
        ctx.surface.output.send_sysex_str(&sysex);
    }
}

/// Install the Launchpad specific trigger sender: on the non-Pro models the scene buttons are
/// lit with notes, everything else with CCs.
pub fn install_trigger_sender(ctx: &mut Ctx) {
    let is_pro = is_pro(ctx);
    ctx.surface.trigger_sender = Rc::new(move |output: &MidiOutput, _bind_type: BindType, _channel, cc, state| {
        if !is_pro && SCENE_CONTROLS.contains(&cc) {
            output.send_note(cc, state);
        } else {
            output.send_cc(cc, state);
        }
    });
}

/// Send the collected blink states of the pads as sysex (called after flushing the lights).
pub fn flush_pad_grid(ctx: &mut Ctx) {
    let Some(grid) = &mut ctx.surface.pad_grid else {
        return;
    };
    if grid.pending_blinks.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut grid.pending_blinks);
    let updates = ext(ctx).definition.build_led_update(&pending);
    for update in updates {
        ctx.surface.output.send_sysex_str(&update);
    }
}

/// Handle a sysex message from the device: print the firmware version from the device inquiry
/// response.
pub fn handle_sysex(ctx: &mut Ctx, data: &str) {
    let inquiry = DeviceInquiry::new(&crate::midi::from_hex_str(data));
    if !inquiry.is_valid() {
        return;
    }
    let revision = inquiry.revision_level();
    if revision.len() == 4 {
        let version: String = revision.iter().map(|d| d.to_string()).collect();
        let version = version.strip_prefix('0').unwrap_or(&version).to_string();
        ctx.host.println(&format!("Firmware version: {version}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation() {
        assert_eq!(translate_to_controller(36), (0, 11));
        assert_eq!(translate_to_controller(43), (0, 18));
        assert_eq!(translate_to_controller(44), (0, 21));
        assert_eq!(translate_to_controller(99), (0, 88));
        assert_eq!(translate_from_controller(88), Some(99));
        assert_eq!(translate_from_controller(19), None);
    }
}
