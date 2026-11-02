// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Akai controllers (former `controller.akai`): APC mini Mk1/Mk2, APC40
//! Mk1/Mk2 and the MPC/Force (ACVS).

pub mod acvs;
pub mod apc;
pub mod apcmini;
pub mod fire;

use std::rc::Rc;

use crate::controller::ControllerFactory;
use crate::framework::ctx::Ctx;
use crate::framework::grid::LightInfo;
use crate::framework::ids::ButtonId;
use crate::framework::surface::ButtonAction;
use crate::midi::BindType;

/// The factories of all Akai controllers.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    let mut factories = apcmini::factories();
    factories.extend(apc::factories());
    factories.extend(acvs::factories());
    factories.extend(fire::factories());
    factories
}

/// The port names for the automatic detection of a device with the same input and output name
/// (former `DefaultControllerDefinition.createDeviceDiscoveryPairs`; the Java version uses the
/// same names on all operating systems).
pub fn device_discovery_pairs(device_name: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let pair = |name: String| (vec![name.clone()], vec![name]);
    // Former createWindowsDeviceDiscoveryPairs ("%s" + deviceName, "%s" + deviceName)
    let mut results = vec![pair(device_name.to_string())];
    for i in 1..20 {
        results.push(pair(format!("{i}- {device_name}")));
    }
    for i in 1..20 {
        results.push(pair(format!("{device_name} MIDI {i}")));
        results.push(pair(format!("{device_name} {i} MIDI 1")));
    }
    results
}

/// Sends the state of a pad: the context, the MIDI channel and note of the pad and its light.
pub type PadSender = Rc<dyn Fn(&Ctx, i32, i32, LightInfo)>;

/// Create the pad buttons of the grid with their lights and MIDI bindings (like
/// `Ctx::create_pads`, but with a controller specific function to send the pad state, the
/// former overridden `PadGridImpl.sendNoteState` and `sendBlinkState`).
pub fn create_grid_pads(ctx: &mut Ctx, sender: PadSender) {
    let Some(grid) = &ctx.surface.pad_grid else {
        return;
    };
    let size = grid.rows() * grid.cols();
    let start_note = grid.start_note();
    for i in 0..size {
        let note = start_note + i;
        let id = ButtonId::get(ButtonId::Pad1, i as usize);
        let (channel, control) = ctx.surface.grid_ref().translate_to_controller(note);
        let button = ctx.surface.create_button(id, &format!("P {}", i + 1));
        button.action = ButtonAction::GridNote(note);
        let sender = sender.clone();
        ctx.surface.create_light(
            None,
            Some(id),
            Rc::new(move |ctx: &Ctx| ctx.surface.grid_ref().light_info(note).encoded()),
            Rc::new(move |ctx: &mut Ctx, _state| {
                let info = ctx.surface.grid_ref().light_info(note);
                sender(ctx, channel.max(0), control, info);
            }),
        );
        ctx.surface.bind_button(id, BindType::Note, channel, control, -1);
    }
}

/// A light which maps a state to a color ID (former `addButton` with color IDs): a negative
/// state is `BUTTON_STATE_OFF`.
pub fn state_light(color_ids: &'static [&'static str], state: impl Fn(&Ctx) -> i32 + 'static) -> crate::framework::surface::LightSupplier {
    Rc::new(move |ctx: &Ctx| {
        let state = state(ctx);
        let id = if state < 0 { crate::framework::color::BUTTON_STATE_OFF } else { color_ids[(state as usize).min(color_ids.len() - 1)] };
        ctx.colors.color_index(id)
    })
}

/// The colors of a boolean light (former `addButton` with a `BooleanSupplier`).
pub const ON_HI: &[&str] = &[crate::framework::color::BUTTON_STATE_ON, crate::framework::color::BUTTON_STATE_HI];

/// A light for a boolean state with the colors `BUTTON_STATE_ON` and `BUTTON_STATE_HI` (former
/// `addButton` with a `BooleanSupplier`).
pub fn bool_light(is_on: impl Fn(&Ctx) -> bool + 'static) -> crate::framework::surface::LightSupplier {
    state_light(ON_HI, move |ctx| i32::from(is_on(ctx)))
}

/// Add a button which has no light (former `addButton (..., hasLight = false, ...)`).
pub fn add_button_without_light(ctx: &mut Ctx, id: ButtonId, label: &str, command: crate::framework::command::SharedTrigger, bind_type: BindType, channel: i32, control: i32) {
    let button = ctx.surface.create_button(id, label);
    button.action = ButtonAction::Command(command);
    ctx.surface.bind_button(id, bind_type, channel, control, -1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_pairs() {
        let pairs = device_discovery_pairs("APC MINI");
        assert_eq!(pairs.len(), 20 + 38);
        assert_eq!(pairs[0], (vec!["APC MINI".to_string()], vec!["APC MINI".to_string()]));
        assert_eq!(pairs[1].0[0], "1- APC MINI");
        assert_eq!(pairs[20].0[0], "APC MINI MIDI 1");
        assert_eq!(pairs[21].1[0], "APC MINI 1 MIDI 1");
    }
}
