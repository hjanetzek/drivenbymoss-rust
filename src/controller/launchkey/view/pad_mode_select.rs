// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The view which is shown while the second scene button is held in the session view (former
//! `PadModeSelectView` of both packages). The lower row selects the pad mode of the session
//! view. The upper row selects the length of new clips on the Mk3; on the Mini it has the
//! metronome, tap tempo, undo, redo, quantize and the commands to add tracks.

use super::super::colors::*;
use super::super::surface::{after_view, state, state_mut};
use super::session::PAD_MODES;
use crate::config::values::NEW_CLIP_LENGTH_VALUES;
use crate::daw::bank::BankItem;
use crate::framework::commands::clip::QuantizeCommand;
use crate::framework::command::TriggerCommand;
use crate::framework::commands::mv_helper;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::view::base::scene_index;
use crate::midi::ButtonEvent;

/// The names of the pad modes (former `PAD_MODE_NAMES`).
const PAD_MODE_NAMES: [&str; 6] = ["Clips", "Record Arm", "Track Select", "Mute", "Solo", "Stop Clips"];

/// The pad mode select view.
pub struct PadModeSelectView {
    is_mini: bool,
    is_consumed: bool,
}

impl PadModeSelectView {
    pub fn new(is_mini: bool) -> Self {
        PadModeSelectView { is_mini, is_consumed: false }
    }

    fn select_pad_mode(ctx: &mut Ctx, index: usize) {
        state_mut(ctx).session_pad_mode = if index == 0 { None } else { Some(PAD_MODES[index - 1]) };
        ctx.notify(PAD_MODE_NAMES[index]);
    }

    /// The upper row of the Mini.
    fn on_mini_function(ctx: &mut Ctx, index: usize) {
        match index {
            8 => {
                ctx.model.transport_mut().toggle_metronome();
                mv_helper::delay_display(ctx, |ctx| if ctx.model.transport().is_metronome_on() { "Metronome: On" } else { "Metronome: Off" }.to_string());
            }
            9 => {
                ctx.model.transport_mut().tap_tempo();
                // Former notifyTempo
                mv_helper::delay_display(ctx, |ctx| {
                    let transport = ctx.model.transport();
                    format!("Tempo: {}", transport.format_tempo(transport.tempo()))
                });
            }
            10 => {
                ctx.model.application_mut().undo();
                ctx.notify("Undo");
            }
            11 => {
                ctx.model.application_mut().redo();
                ctx.notify("Redo");
            }
            12 => {
                QuantizeCommand::new().execute_normal(ctx, ButtonEvent::Up);
                ctx.notify("Quantize");
            }
            13 => {
                ctx.model.application_mut().add_instrument_track();
                ctx.notify("Add Instrument Track");
            }
            14 => {
                ctx.model.application_mut().add_audio_track();
                ctx.notify("Add Audio Track");
            }
            15 => {
                ctx.model.application_mut().add_effect_track();
                ctx.notify("Add Effect Track");
            }
            // Ignore 6 and 7
            _ => {}
        }
    }
}

impl View for PadModeSelectView {
    fn name(&self) -> &str {
        "Pad Mode Select"
    }

    fn on_activate(&mut self, _ctx: &mut Ctx) {
        self.is_consumed = false;
    }

    fn button_color(&self, _ctx: &Ctx, button: ButtonId) -> i32 {
        if button == ButtonId::Scene2 {
            LAUNCHKEY_COLOR_WHITE
        } else {
            LAUNCHKEY_COLOR_BLACK
        }
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        if self.is_mini {
            let metronome = if ctx.model.transport().is_metronome_on() { LAUNCHKEY_COLOR_GREEN_HI } else { LAUNCHKEY_COLOR_GREEN_LO };
            let colors = [
                metronome,
                LAUNCHKEY_COLOR_BLUE_LO,
                LAUNCHKEY_COLOR_RED_LO,
                LAUNCHKEY_COLOR_RED_LO,
                LAUNCHKEY_COLOR_WHITE,
                LAUNCHKEY_COLOR_AMBER_YELLOW,
                LAUNCHKEY_COLOR_AMBER_YELLOW,
                LAUNCHKEY_COLOR_AMBER_YELLOW,
            ];
            let grid = ctx.surface.grid();
            for (x, color) in colors.iter().enumerate() {
                grid.light_ex(x as i32, 0, *color);
            }
        } else {
            let clip_length_index = ctx.config.new_clip_length() as i32;
            let crate::framework::ctx::Ctx { surface, colors, .. } = ctx;
            let grid = surface.grid();
            for x in 0..8 {
                grid.light_ex_id(colors, x, 0, if x == clip_length_index { "DAW_COLOR_RED" } else { "DAW_COLOR_LIGHT_ORANGE" });
            }
        }

        let pad_mode = state(ctx).session_pad_mode;
        let pick = |mode: Option<Modes>, on: i32, off: i32| if pad_mode == mode { on } else { off };
        let colors = [
            pick(None, LAUNCHKEY_COLOR_GREEN_HI, LAUNCHKEY_COLOR_GREEN_LO),
            pick(Some(Modes::RecArm), LAUNCHKEY_COLOR_RED_HI, LAUNCHKEY_COLOR_RED_LO),
            pick(Some(Modes::TrackSelect), LAUNCHKEY_COLOR_WHITE, LAUNCHKEY_COLOR_GREY_LO),
            pick(Some(Modes::Mute), LAUNCHKEY_COLOR_AMBER_HI, LAUNCHKEY_COLOR_AMBER_LO),
            pick(Some(Modes::Solo), LAUNCHKEY_COLOR_YELLOW_HI, LAUNCHKEY_COLOR_YELLOW_LO),
            pick(Some(Modes::StopClip), LAUNCHKEY_COLOR_PINK_HI, LAUNCHKEY_COLOR_ROSE),
            LAUNCHKEY_COLOR_BLACK,
            LAUNCHKEY_COLOR_BLACK,
        ];
        let grid = ctx.surface.grid();
        for (x, color) in colors.iter().enumerate() {
            grid.light_ex(x as i32, 1, *color);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }
        let index = note - 36;
        if self.is_mini {
            match index {
                0..=5 => Self::select_pad_mode(ctx, index as usize),
                _ => Self::on_mini_function(ctx, index as usize),
            }
        } else {
            if index > 5 && index < 8 {
                return;
            }
            if index <= 5 {
                Self::select_pad_mode(ctx, index.max(0) as usize);
            } else {
                let new_clip_length = (index - 8) as usize;
                ctx.config.set_new_clip_length(new_clip_length);
                ctx.notify(&format!("New Clip Length:{}", NEW_CLIP_LENGTH_VALUES[new_clip_length.min(7)]));
            }
        }
        self.is_consumed = true;
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        let Some(index) = scene_index(button) else {
            return;
        };
        match event {
            ButtonEvent::Long => self.is_consumed = true,
            ButtonEvent::Down => {}
            ButtonEvent::Up => {
                after_view(ctx, |ctx| ctx.restore_view());
                if self.is_consumed {
                    return;
                }
                let mut scene_bank = ctx.model.scene_bank_mut();
                let scene = scene_bank.item_mut(index);
                scene.select();
                scene.launch(true, false);
            }
        }
    }

    crate::impl_as_any!();
}
