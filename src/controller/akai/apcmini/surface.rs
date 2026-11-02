// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The APC mini specific parts of the control surface (former `APCminiControlSurface` and
//! `APCminiPadGrid`) and the controller state, which is stored in `ctx.ext` ([`ApcMiniState`]).

use std::rc::Rc;

use super::definition::ApcMiniDefinition;
use crate::config::apcmini;
use crate::framework::color::ColorManager;
use crate::framework::ctx::Ctx;
use crate::framework::grid::PadGrid;
use crate::framework::ids::ButtonId;
use crate::framework::surface::ButtonAction;
use crate::midi::BindType;

// MIDI CC

pub const APC_KNOB_TRACK_LEVEL1: i32 = 48;
pub const APC_KNOB_MASTER_LEVEL: i32 = 56;

pub const APC_BUTTON_STATE_OFF: i32 = 0;
pub const APC_BUTTON_STATE_ON: i32 = 1;
pub const APC_BUTTON_STATE_BLINK: i32 = 2;

pub const TRACK_STATE_CLIP_STOP: usize = 0;
pub const TRACK_STATE_SOLO: usize = 1;
pub const TRACK_STATE_REC_ARM: usize = 2;
pub const TRACK_STATE_MUTE: usize = 3;
pub const TRACK_STATE_SELECT: usize = 4;

/// The controller specific state.
pub struct ApcMiniState {
    pub definition: ApcMiniDefinition,
    /// The function of the track buttons (former `trackState`).
    pub track_state: usize,
}

impl ApcMiniState {
    pub fn new(definition: ApcMiniDefinition) -> Self {
        ApcMiniState { definition, track_state: TRACK_STATE_CLIP_STOP }
    }
}

pub fn state(ctx: &Ctx) -> &ApcMiniState {
    ctx.ext::<ApcMiniState>()
}

pub fn state_mut(ctx: &mut Ctx) -> &mut ApcMiniState {
    ctx.ext_mut::<ApcMiniState>()
}

pub fn definition(ctx: &Ctx) -> ApcMiniDefinition {
    state(ctx).definition
}

/// Former `getTrackState`.
pub fn track_state(ctx: &Ctx) -> usize {
    state(ctx).track_state
}

/// Former `setTrackState`.
pub fn set_track_state(ctx: &mut Ctx, track_state: usize) {
    state_mut(ctx).track_state = track_state;
}

/// The grid note to controller translation (former `APCminiPadGrid.translateToController`).
fn translate_to_controller(note: i32) -> (i32, i32) {
    (0, note - 36)
}

/// Create the pad grid (former `APCminiPadGrid`).
pub fn create_pad_grid(colors: &ColorManager) -> PadGrid {
    PadGrid::new(colors, 8, 8, 36).with_translation(translate_to_controller)
}

/// Create the pad buttons with their lights and MIDI bindings (former `createPads` with the
/// `APCminiPadGrid`, which adds the pad brightness to the MIDI channel on the Mk2 and does not
/// send blink states).
pub fn create_pads(ctx: &mut Ctx) {
    let has_brightness = definition(ctx).has_brightness();
    for i in 0..64 {
        let note = 36 + i;
        let id = ButtonId::get(ButtonId::Pad1, i as usize);
        let (channel, control) = translate_to_controller(note);
        let button = ctx.surface.create_button(id, &format!("P {}", i + 1));
        button.action = ButtonAction::GridNote(note);
        ctx.surface.create_light(
            None,
            Some(id),
            Rc::new(move |ctx: &Ctx| ctx.surface.grid_ref().light_info(note).encoded()),
            Rc::new(move |ctx: &mut Ctx, _state| {
                let color = ctx.surface.grid_ref().light_info(note).color.max(0);
                let mut chn = channel;
                if has_brightness {
                    chn += apcmini::pad_brightness(&ctx.config);
                }
                ctx.surface.output.send_note_ex(chn, control, color);
            }),
        );
        ctx.surface.bind_button(id, BindType::Note, channel, control, -1);
    }
}

/// Install the trigger sender: all buttons are notes (former `setTrigger`).
pub fn install_trigger_sender(ctx: &mut Ctx) {
    ctx.surface.trigger_sender = Rc::new(|output, _bind_type, channel, cc, state| output.send_note_ex(channel, cc, state));
}
