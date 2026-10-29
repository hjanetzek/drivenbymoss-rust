// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the main encoder and the touch strip (former `MainKnobRowModeCommand`,
//! `TouchstripCommand` and `RibbonCommand`).

use crate::config::maschine::MaschineConfiguration;
use crate::controller::maschine::core::ribbon_mode::RibbonMode;
use crate::controller::maschine::mk::mode::MAIN_ENCODER;
use crate::controller::maschine::mk::surface::{ext, ext_mut, set_ribbon_value};
use crate::daw::constants::Resolution;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::ButtonId;
use crate::midi::{ButtonEvent, CMD_CC, CMD_PITCHBEND};

/// The command of the main encoder (former `MainKnobRowModeCommand`): edits the selected
/// parameter of the active mode or the last touched parameter; in combination with buttons it
/// switches pages or changes the fixed accent value.
#[derive(Default)]
pub struct MainKnobRowModeCommand;

impl MainKnobRowModeCommand {
    pub fn new() -> Self {
        MainKnobRowModeCommand
    }

    fn switch_page(ctx: &mut Ctx, button: ButtonId, value: i32) {
        let speed = ctx.model.value_changer().decode(value);
        ctx.surface.set_trigger_consumed(button);
        match button {
            ButtonId::Scene1 => {
                let mut scene_bank = ctx.model.scene_bank_mut();
                if speed < 0 {
                    scene_bank.select_previous_page();
                } else {
                    scene_bank.select_next_page();
                }
            }
            ButtonId::Clip => {
                // The slots of the tracks follow the scene bank
                if ctx.model.current_track_bank().selected_item().is_none() {
                    return;
                }
                let mut scene_bank = ctx.model.scene_bank_mut();
                if speed < 0 {
                    scene_bank.select_previous_page();
                } else {
                    scene_bank.select_next_page();
                }
            }
            _ => {
                let track_bank = ctx.model.current_track_bank_mut();
                if speed < 0 {
                    track_bank.select_previous_page();
                } else {
                    track_bank.select_next_page();
                }
            }
        }
    }
}

impl ContinuousCommand for MainKnobRowModeCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if ext(ctx).control_last_param_active {
            let device = ctx.model.cursor_device();
            let index = device.last_touched_parameter_index();
            if device.does_exist() && index >= 0 {
                let increment = ctx.model.value_changer().calc_knob_change(value);
                if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                    if (index as usize) < bank.unpaged_item_count() {
                        let parameter = bank.unpaged_item_mut(index as usize);
                        if parameter.does_exist() {
                            parameter.inc(increment);
                        }
                    }
                }
            }
            return;
        }

        for button in [ButtonId::Scene1, ButtonId::Clip, ButtonId::Track, ButtonId::Solo, ButtonId::Mute] {
            if ctx.surface.is_pressed(button) {
                Self::switch_page(ctx, button, value);
                return;
            }
        }

        if ctx.surface.is_pressed(ButtonId::Accent) {
            let speed = ctx.model.value_changer().decode(value);
            ctx.surface.set_trigger_consumed(ButtonId::Accent);
            let v = (ctx.config.fixed_accent_value() + speed).clamp(1, 127);
            ctx.config.set_fixed_accent_value(v);
            ctx.notify(&format!("Fixed Accent: {v}"));
            return;
        }

        // Former KnobRowModeCommand (-1)
        if let Some(mode) = ctx.active_mode() {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.on_knob_value(ctx, MAIN_ENCODER, value);
            }
        }
    }
}

/// The command of the touch strip (former `TouchstripCommand`): the value (continuous) and the
/// touch (trigger) of the strip.
#[derive(Default)]
pub struct TouchstripCommand;

impl TouchstripCommand {
    pub fn new() -> Self {
        TouchstripCommand
    }

    /// Reset the value of the strip for the ribbon mode (former `resetRibbonValue`).
    pub fn reset_ribbon_value(ctx: &mut Ctx, ribbon_mode: RibbonMode) {
        match ribbon_mode {
            RibbonMode::PitchDown | RibbonMode::PitchUp | RibbonMode::Cc1 | RibbonMode::Cc11 => ext_mut(ctx).ribbon_value = 0,
            RibbonMode::PitchDownUp => ext_mut(ctx).ribbon_value = 64,
            _ => {}
        }
    }

    /// Update the LEDs of the touch strip (former `updateValue`, called on flush).
    pub fn update_value(ctx: &mut Ctx) {
        let ribbon_mode = MaschineConfiguration::ribbon_mode(&ctx.config);
        match ribbon_mode {
            RibbonMode::PitchDown | RibbonMode::PitchUp | RibbonMode::PitchDownUp | RibbonMode::Cc1 | RibbonMode::Cc11 => {
                let value = ext(ctx).ribbon_value;
                set_ribbon_value(ctx, value);
            }
            RibbonMode::MasterVolume => {
                let value = ctx.model.value_changer().to_midi_value(ctx.model.master_track().volume());
                set_ribbon_value(ctx, value);
            }
            RibbonMode::NoteRepeatPeriod | RibbonMode::NoteRepeatLength => {
                let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
                let value = if ribbon_mode == RibbonMode::NoteRepeatPeriod { note_repeat.period } else { note_repeat.note_length };
                let index = Resolution::get_match(value);
                let value = (index as f64 * 127.0 / (Resolution::ALL.len() - 1) as f64).round() as i32;
                set_ribbon_value(ctx, value);
            }
        }
    }
}

impl ContinuousCommand for TouchstripCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        ext_mut(ctx).ribbon_value = value;
        let ribbon_mode = MaschineConfiguration::ribbon_mode(&ctx.config);
        match ribbon_mode {
            RibbonMode::PitchDown => ctx.surface.send_midi_event(CMD_PITCHBEND, 0, (127 - value) / 2),
            RibbonMode::PitchUp => ctx.surface.send_midi_event(CMD_PITCHBEND, 0, 64 + value / 2),
            RibbonMode::PitchDownUp => ctx.surface.send_midi_event(CMD_PITCHBEND, 0, value),
            RibbonMode::Cc1 => ctx.surface.send_midi_event(CMD_CC, 1, value),
            RibbonMode::Cc11 => ctx.surface.send_midi_event(CMD_CC, 11, value),
            RibbonMode::MasterVolume => {
                let volume = ctx.model.value_changer().to_daw_value(value);
                ctx.model.master_track_mut().set_volume(volume);
            }
            RibbonMode::NoteRepeatPeriod | RibbonMode::NoteRepeatLength => {
                let values = Resolution::ALL;
                let scaled = (127 - value) as f64 / 127.0;
                let index = (scaled * (values.len() - 1) as f64).round() as usize;
                let resolution_value = values[values.len() - 1 - index].value();
                let note_repeat = &mut ctx.surface.input.default_note_input_mut().note_repeat;
                if ribbon_mode == RibbonMode::NoteRepeatPeriod {
                    note_repeat.set_period(resolution_value);
                } else {
                    note_repeat.set_note_length(resolution_value);
                }
            }
        }
    }
}

impl TriggerCommand for TouchstripCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Up {
            return;
        }
        match MaschineConfiguration::ribbon_mode(&ctx.config) {
            RibbonMode::PitchDown | RibbonMode::PitchUp => {
                ext_mut(ctx).ribbon_value = 0;
                ctx.surface.send_midi_event(CMD_PITCHBEND, 0, 64);
            }
            RibbonMode::PitchDownUp => {
                ext_mut(ctx).ribbon_value = 64;
                ctx.surface.send_midi_event(CMD_PITCHBEND, 0, 64);
            }
            _ => {}
        }
    }
}

/// Selects one of the ribbon modes of a group (former `RibbonCommand`).
pub struct RibbonCommand {
    modes: Vec<RibbonMode>,
    active_mode: RibbonMode,
}

impl RibbonCommand {
    pub fn new(modes: &[RibbonMode]) -> Self {
        RibbonCommand { modes: modes.to_vec(), active_mode: modes[0] }
    }

    /// Is the active ribbon mode one of the given modes?
    pub fn is_ribbon_mode(ctx: &Ctx, modes: &[RibbonMode]) -> bool {
        modes.contains(&MaschineConfiguration::ribbon_mode(&ctx.config))
    }
}

impl TriggerCommand for RibbonCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        let ribbon_mode = MaschineConfiguration::ribbon_mode(&ctx.config);
        let mut m = self.active_mode;
        if let Some(index) = self.modes.iter().position(|mode| *mode == ribbon_mode) {
            let index = if index + 1 >= self.modes.len() { 0 } else { index + 1 };
            m = self.modes[index];
        }
        self.active_mode = m;
        ctx.notify(m.name());
        MaschineConfiguration::set_ribbon_mode(&mut ctx.config, m);
        TouchstripCommand::reset_ribbon_value(ctx, m);
    }
}
