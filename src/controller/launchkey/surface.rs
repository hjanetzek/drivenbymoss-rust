// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Launchkey specific parts of the control surface (former `LaunchkeyMk3ControlSurface`,
//! `LaunchkeyMiniMk3ControlSurface` and the two `LaunchkeyPadGrid`s) and the controller state,
//! which is stored in `ctx.ext` ([`LaunchkeyState`]).
//!
//! The pad grid has 2 rows of 8 pads (notes 36-51). The device sends and receives different
//! notes for them depending on its pad mode: session (channel 1), drum (channel 10) and device
//! select (channel 1, Mk3 only). Each pad is therefore bound three times (twice on the Mini, which
//! has no device select mode): `PAD1-16` for session, `PAD17-32` for drum and `PAD33-48` for
//! device select. Each of them has a light which sends the state of the pad with the mapping of
//! the active pad view, as Java does.

use std::cell::Cell;
use std::rc::Rc;

use super::definition::*;
use crate::framework::color::ColorManager;
use crate::framework::ctx::{Ctx, ModeManagerId};
use crate::framework::grid::PadGrid;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::surface::ButtonAction;
use crate::midi::{BindType, DeviceInquiry, MidiInput};

// The controller notes of the pads, top row first (the grid notes are 36-43 for the lower and
// 44-51 for the upper row)

const MAP_DRUM: [i32; 16] = [36, 37, 38, 39, 44, 45, 46, 47, 40, 41, 42, 43, 48, 49, 50, 51];
const MAP_SESSION: [i32; 16] = [112, 113, 114, 115, 116, 117, 118, 119, 96, 97, 98, 99, 100, 101, 102, 103];
const MAP_DEVICE_SELECT: [i32; 16] = [80, 81, 82, 83, 84, 85, 86, 87, 64, 65, 66, 67, 68, 69, 70, 71];

/// A function which runs after the active view has handled a button.
pub type AfterViewTask = Box<dyn FnOnce(&mut Ctx)>;

/// The controller specific state.
pub struct LaunchkeyState {
    pub model: LaunchkeyModel,
    /// The view of the pad grid which selects the note mapping (former `LaunchkeyPadGrid.setView`).
    pub pad_view: Option<Views>,
    pub is_daw_connected: bool,
    /// True if the Mk3 has faders (only the 49 and 61 key models, detected with the device inquiry).
    pub has_faders: bool,
    /// True if the Mk3 is in DAW mode and the modes are synchronized (the setup's `isReady`).
    pub is_ready: bool,
    /// The mode manager of the faders (Mk3).
    pub fader_mode_manager: ModeManagerId,
    /// True if the buttons below the faders select tracks, otherwise they arm them (former
    /// static `ButtonAreaCommand.isSelect`).
    pub is_select: bool,
    /// The last received program change (Mini, former `lastPrgChange`).
    pub last_prg_change: i32,
    /// Selects the header of the 88 key model of the Mk3 display (former `setModel`).
    pub is_88_key_version: Rc<Cell<bool>>,
    /// The keyboard input (keys, modulation, sustain, pitchbend, ...); kept for its note input.
    pub keys_input: Option<MidiInput>,
    /// The function of the lower pad row in the session view, `None` for clips (former
    /// `SessionView.padMode`).
    pub session_pad_mode: Option<Modes>,
    /// The functions to run after the active view has handled a button (e.g. to switch the view,
    /// which is borrowed while it handles the button), see [`after_view`].
    pub after_view: Vec<AfterViewTask>,
}

impl LaunchkeyState {
    pub fn new(model: LaunchkeyModel) -> Self {
        LaunchkeyState {
            model,
            pad_view: None,
            is_daw_connected: false,
            has_faders: true,
            is_ready: false,
            fader_mode_manager: 0,
            is_select: true,
            last_prg_change: -1,
            is_88_key_version: Rc::new(Cell::new(false)),
            keys_input: None,
            session_pad_mode: None,
            after_view: Vec::new(),
        }
    }
}

pub fn state(ctx: &Ctx) -> &LaunchkeyState {
    ctx.ext::<LaunchkeyState>()
}

pub fn state_mut(ctx: &mut Ctx) -> &mut LaunchkeyState {
    ctx.ext_mut::<LaunchkeyState>()
}

pub fn is_mini(ctx: &Ctx) -> bool {
    state(ctx).model.is_mini()
}

/// The mode manager of the faders (former `getFaderModeManager`).
pub fn fader_mode_manager(ctx: &Ctx) -> ModeManagerId {
    state(ctx).fader_mode_manager
}

/// Former `hasFaders`.
pub fn has_faders(ctx: &Ctx) -> bool {
    state(ctx).has_faders
}

/// Former `isDAWConnected`.
pub fn is_daw_connected(ctx: &Ctx) -> bool {
    state(ctx).is_daw_connected
}

/// Run a function after the active view has handled the button (the view is borrowed while
/// it handles it, so it cannot switch the view itself).
pub fn after_view(ctx: &mut Ctx, f: impl FnOnce(&mut Ctx) + 'static) {
    state_mut(ctx).after_view.push(Box::new(f));
}

/// Run the functions registered with [`after_view`].
pub fn run_after_view(ctx: &mut Ctx) {
    let tasks = std::mem::take(&mut state_mut(ctx).after_view);
    for task in tasks {
        task(ctx);
    }
}

/// Forward a button to the active view and run the functions registered with [`after_view`]
/// (former `ViewButtonCommand`).
pub fn view_button(ctx: &mut Ctx, button: ButtonId, event: crate::midi::ButtonEvent, velocity: i32) {
    if let Some(view) = ctx.active_view() {
        if let Ok(mut view) = view.try_borrow_mut() {
            view.on_button(ctx, button, event, velocity);
        }
    }
    run_after_view(ctx);
}

/// Former `LaunchkeyPadGrid.setView`.
pub fn set_pad_view(ctx: &mut Ctx, view: Views) {
    state_mut(ctx).pad_view = Some(view);
}

/// Translate a grid note to the MIDI channel and note of the controller for the pad view
/// (former static `LaunchkeyPadGrid.translateToController`).
pub fn translate_to_controller(model: LaunchkeyModel, view: Option<Views>, note: i32) -> (i32, i32) {
    let n = (note - 36).clamp(0, 15) as usize;
    if model.is_mini() {
        return match view {
            None | Some(Views::Session) => (0, MAP_SESSION[n]),
            _ => (9, MAP_DRUM[n]),
        };
    }
    match view {
        None => (0, note),
        Some(Views::Drum) => (9, MAP_DRUM[n]),
        Some(Views::Device) => (0, MAP_DEVICE_SELECT[n]),
        _ => (0, MAP_SESSION[n]),
    }
}

/// Create the pad grid (former `LaunchkeyPadGrid`): 2 rows of 8 pads.
pub fn create_pad_grid(colors: &ColorManager) -> PadGrid {
    PadGrid::new(colors, 2, 8, 36)
}

/// Send the state of a pad with the mapping of the pad view (former `PadGridImpl.sendState`
/// with the `sendNoteState` and `sendBlinkState` of `LaunchkeyPadGrid`).
pub fn send_pad_state(ctx: &mut Ctx, note: i32) {
    let (model, view) = (state(ctx).model, state(ctx).pad_view);
    let info = ctx.surface.grid_ref().light_info(note);
    let (_, controller_note) = translate_to_controller(model, view, note);
    let is_drum = view == Some(Views::Drum);
    let output = &ctx.surface.output;
    output.send_note_ex(if is_drum { 9 } else { 0 }, controller_note, info.color.max(0));
    let blink_color = info.blink_color;
    if blink_color > 0 && blink_color < 128 {
        let channel = if info.fast { 1 } else { 2 };
        output.send_note_ex(if is_drum { 9 + channel } else { channel }, controller_note, blink_color);
    }
}

/// Create the pads (former `createPads` of both surfaces): the session pads `PAD1-16`, the drum
/// pads `PAD17-32` and on the Mk3 the device select pads `PAD33-48`.
pub fn create_pads(ctx: &mut Ctx) {
    let model = state(ctx).model;
    let mut views = vec![(ButtonId::Pad1, "P", if model.is_mini() { None } else { Some(Views::Session) }), (ButtonId::Pad17, "D", Some(Views::Drum))];
    if !model.is_mini() {
        views.push((ButtonId::Pad33, "DS", Some(Views::Device)));
    }
    for (first, label, view) in views {
        for i in 0..16 {
            let note = 36 + i as i32;
            let id = ButtonId::get(first, i);
            let (channel, control) = translate_to_controller(model, view, note);
            let button = ctx.surface.create_button(id, &format!("{label} {}", i + 1));
            button.action = ButtonAction::GridNote(note);
            ctx.surface.create_light(
                None,
                Some(id),
                Rc::new(move |ctx: &Ctx| ctx.surface.grid_ref().light_info(note).encoded()),
                Rc::new(move |ctx: &mut Ctx, _state| send_pad_state(ctx, note)),
            );
            ctx.surface.bind_button(id, BindType::Note, channel, control, -1);
        }
    }
}

/// Set the Launchkey to DAW mode (former `setLaunchpadToDAW`).
pub fn set_launchkey_to_daw(ctx: &Ctx, enable: bool) {
    ctx.surface.output.send_note_ex(0x0F, LAUNCHKEY_DAW_ONLINE, if enable { 0x7F } else { 0x00 });
}

/// Activate a Launchkey knob mode (former `setKnobMode`).
pub fn set_knob_mode(ctx: &Ctx, knob_mode: i32) {
    ctx.surface.output.send_cc_ex(0x0F, LAUNCHKEY_MODE_SELECT, knob_mode);
}

/// Activate a Launchkey pad mode (former `setPadMode`).
pub fn set_pad_mode(ctx: &Ctx, pad_mode: i32) {
    ctx.surface.output.send_cc_ex(0x0F, LAUNCHKEY_VIEW_SELECT, pad_mode);
}

/// Format the firmware version of a device inquiry response.
fn firmware_version(inquiry: &DeviceInquiry) -> String {
    let version: String = inquiry.revision_level().iter().map(|d| d.to_string()).collect();
    match version.strip_prefix('0') {
        Some(stripped) => stripped.to_string(),
        None => version,
    }
}

/// Handle a sysex message from the device: the response of the device inquiry (former
/// `handleSysEx` and `handleDeviceInquiryResponse`).
pub fn handle_sysex(ctx: &mut Ctx, data: &str) {
    let inquiry = DeviceInquiry::new(&crate::midi::from_hex_str(data));
    if !inquiry.is_valid() {
        return;
    }
    let version = firmware_version(&inquiry);
    if is_mini(ctx) {
        ctx.host.println(&format!("Firmware version: {version}"));
        return;
    }
    let family = inquiry.device_family_code().first().copied().unwrap_or(0) as i32;
    state_mut(ctx).has_faders = family >= LAUNCHKEY_49;
    if family == LAUNCHKEY_88 {
        state(ctx).is_88_key_version.set(true);
        if let Some(display) = &mut ctx.text_display {
            display.force_flush();
        }
    }
    let name = model_name(family).unwrap_or("null");
    ctx.host.println(&format!("Detected {name} (Firmware version: {version})"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation() {
        let mk3 = LaunchkeyModel::Mk3;
        let mini = LaunchkeyModel::MiniMk3;
        // Lower left pad and upper right pad
        assert_eq!(translate_to_controller(mk3, Some(Views::Session), 36), (0, 112));
        assert_eq!(translate_to_controller(mk3, Some(Views::Session), 51), (0, 103));
        assert_eq!(translate_to_controller(mk3, Some(Views::Drum), 40), (9, 44));
        assert_eq!(translate_to_controller(mk3, Some(Views::Drum), 44), (9, 40));
        assert_eq!(translate_to_controller(mk3, Some(Views::Device), 44), (0, 64));
        assert_eq!(translate_to_controller(mk3, None, 40), (0, 40));
        assert_eq!(translate_to_controller(mk3, Some(Views::Browser), 36), (0, 112));
        assert_eq!(translate_to_controller(mini, None, 37), (0, 113));
        assert_eq!(translate_to_controller(mini, Some(Views::Play), 37), (9, 37));
        assert_eq!(translate_to_controller(mini, Some(Views::Drum), 48), (9, 48));
    }
}
