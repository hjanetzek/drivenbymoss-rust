// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! More transport commands (former `TempoCommand`, `command.trigger.transport`:
//! `WindCommand`, `StopCommand`, `TapTempoCommand`, `ToggleLoopCommand`, `AutomationCommand`
//! and `command.continuous.SwingCommand`).
//!
//! The repeating commands (tempo, wind) keep their pressed state in a shared flag, which is
//! checked by the scheduled repetition task.

use std::cell::Cell;
use std::rc::Rc;

use super::mv_helper;
use super::transport::DoubleClick;
use crate::config::TransportBehavior;
use crate::daw::constants::GrooveParameterId;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// Changes the tempo with a knob (relative or absolute) or repeatedly while a button is pressed
/// (former `TempoCommand`).
pub struct TempoCommand {
    do_increase: bool,
    is_absolute: bool,
    min_tempo: f64,
    tempo_range: f64,
    is_tempo_change: Rc<Cell<bool>>,
}

impl TempoCommand {
    /// A relative knob (former `TempoCommand (model, surface)`).
    pub fn new() -> Self {
        Self::create(false, false, -1, -1)
    }

    /// An absolute knob with the tempo range (former `TempoCommand (model, surface, min, max)`).
    pub fn absolute(min_tempo: i32, max_tempo: i32) -> Self {
        Self::create(false, true, min_tempo, max_tempo)
    }

    /// A button which increases/decreases the tempo (former `TempoCommand (doIncrease, ..)`).
    pub fn button(do_increase: bool) -> Self {
        Self::create(do_increase, false, -1, -1)
    }

    fn create(do_increase: bool, is_absolute: bool, min_tempo: i32, max_tempo: i32) -> Self {
        let min = if min_tempo == -1 { 20 } else { min_tempo };
        let max = if max_tempo == -1 { 666 } else { max_tempo };
        TempoCommand { do_increase, is_absolute, min_tempo: min as f64, tempo_range: (max - min) as f64, is_tempo_change: Rc::new(Cell::new(false)) }
    }

    fn do_change_tempo(ctx: &mut Ctx, flag: Rc<Cell<bool>>, do_increase: bool) {
        if !flag.get() {
            return;
        }
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_tempo(do_increase, slow);
        ctx.schedule(200, move |ctx| Self::do_change_tempo(ctx, flag, do_increase));
    }

    /// Former `execute (event, velocity)` (can be called by modes).
    pub fn execute_button(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        match event {
            ButtonEvent::Down => self.is_tempo_change.set(true),
            ButtonEvent::Up => self.is_tempo_change.set(false),
            ButtonEvent::Long => {}
        }
        // As in Java a long press starts a second repetition, which doubles the speed
        Self::do_change_tempo(ctx, self.is_tempo_change.clone(), self.do_increase);
    }
}

impl Default for TempoCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ContinuousCommand for TempoCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if self.is_absolute {
            let tempo = self.min_tempo + ctx.model.value_changer().to_normalized_value(value as f64) * self.tempo_range;
            let tempo = if ctx.surface.is_shift_pressed() { tempo } else { tempo.round() };
            ctx.model.transport_mut().set_tempo(tempo);
            return;
        }
        let increase = ctx.model.value_changer().is_increase(value);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_tempo(increase, slow);
    }
}

impl TriggerCommand for TempoCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        self.execute_button(ctx, event);
    }
}

/// Rewinds or fast forwards while pressed, slow with Shift (former `WindCommand`).
pub struct WindCommand {
    is_fast_forwarding: bool,
    is_rewinding: Rc<Cell<bool>>,
    is_forwarding: Rc<Cell<bool>>,
}

impl WindCommand {
    pub fn new(is_fast_forward: bool) -> Self {
        WindCommand { is_fast_forwarding: is_fast_forward, is_rewinding: Rc::new(Cell::new(false)), is_forwarding: Rc::new(Cell::new(false)) }
    }

    pub fn is_rewinding(&self) -> bool {
        self.is_rewinding.get()
    }

    pub fn is_forwarding(&self) -> bool {
        self.is_forwarding.get()
    }

    fn handle(&mut self, ctx: &mut Ctx, event: ButtonEvent, slow: bool) {
        let flag = if self.is_fast_forwarding { &self.is_forwarding } else { &self.is_rewinding };
        match event {
            ButtonEvent::Down => flag.set(true),
            ButtonEvent::Up => flag.set(false),
            ButtonEvent::Long => {}
        }
        // As in Java a long press starts a second repetition, which doubles the speed
        Self::do_change_position(ctx, self.is_rewinding.clone(), self.is_forwarding.clone(), slow);
    }

    fn do_change_position(ctx: &mut Ctx, rewinding: Rc<Cell<bool>>, forwarding: Rc<Cell<bool>>, slow: bool) {
        if !rewinding.get() && !forwarding.get() {
            return;
        }
        ctx.model.transport_mut().change_position(forwarding.get(), slow);
        ctx.schedule(100, move |ctx| Self::do_change_position(ctx, rewinding, forwarding, slow));
    }
}

impl TriggerCommand for WindCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.handle(ctx, event, false);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        self.handle(ctx, event, true);
    }
}

/// Stops playback; a double click moves the play cursor to the end; with Shift all clips are
/// stopped (former `StopCommand`).
#[derive(Default)]
pub struct StopCommand {
    double_click: DoubleClick,
}

impl StopCommand {
    pub fn new() -> Self {
        Self::default()
    }

    fn execute_single_click(&mut self, ctx: &mut Ctx) {
        if ctx.model.transport().is_playing() {
            Self::handle_stop_options(ctx);
        } else {
            ctx.model.transport_mut().stop_and_rewind();
            self.double_click.start(ctx);
        }
    }

    fn execute_double_click(ctx: &mut Ctx) {
        if ctx.config.is_enable_double_click_stop_button() {
            ctx.model.transport_mut().set_position_to_end();
        }
    }

    /// Former `handleStopOptions`.
    pub fn handle_stop_options(ctx: &mut Ctx) {
        let behavior = ctx.config.behaviour_on_stop();
        let transport = ctx.model.transport_mut();
        match behavior {
            TransportBehavior::ReturnToZero => transport.stop_and_rewind(),
            TransportBehavior::Stop => transport.stop(),
            TransportBehavior::Pause => {
                if transport.is_playing() {
                    transport.play();
                }
            }
        }
    }
}

impl TriggerCommand for StopCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if self.double_click.take() {
            Self::execute_double_click(ctx);
            return;
        }
        self.execute_single_click(ctx);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.track_bank_mut().stop(false);
        }
    }
}

/// Tap tempo; with Shift toggles the metronome (former `TapTempoCommand`).
#[derive(Default)]
pub struct TapTempoCommand;

impl TapTempoCommand {
    pub fn new() -> Self {
        TapTempoCommand
    }
}

impl TriggerCommand for TapTempoCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        ctx.model.transport_mut().tap_tempo();
        let message = format!("Tempo: {:.2}", ctx.model.transport().tempo());
        ctx.notify(&message);
    }

    fn execute_shifted(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.transport_mut().toggle_metronome();
        }
    }
}

/// Toggles the loop on button release (former `ToggleLoopCommand`).
#[derive(Default)]
pub struct ToggleLoopCommand;

impl ToggleLoopCommand {
    pub fn new() -> Self {
        ToggleLoopCommand
    }
}

impl TriggerCommand for ToggleLoopCommand {
    fn execute_normal(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        if event == ButtonEvent::Up {
            ctx.model.transport_mut().toggle_loop();
        }
    }
}

/// Toggles the arranger (launcher with Shift, flipped by the setting) automation write mode;
/// with Select the next automation write mode is selected (former `AutomationCommand`).
#[derive(Default)]
pub struct AutomationCommand;

impl AutomationCommand {
    pub fn new() -> Self {
        AutomationCommand
    }

    /// Is clip launcher automation the target?
    fn is_launcher(ctx: &Ctx) -> bool {
        let is_shift = ctx.surface.is_shift_pressed();
        let flip_record = ctx.config.is_flip_record();
        is_shift != flip_record
    }

    /// Former `isLit`.
    pub fn is_lit(ctx: &Ctx) -> bool {
        let transport = ctx.model.transport();
        if Self::is_launcher(ctx) {
            transport.is_writing_clip_launcher_automation()
        } else {
            transport.is_writing_arranger_automation()
        }
    }
}

impl TriggerCommand for AutomationCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if event != ButtonEvent::Down {
            return;
        }
        if ctx.surface.is_select_pressed() {
            ctx.model.transport_mut().next_automation_write_mode(true);
            mv_helper::delay_display(ctx, |ctx| ctx.model.transport().automation_write_mode().label().to_string());
            return;
        }
        if Self::is_launcher(ctx) {
            ctx.model.transport_mut().toggle_write_clip_launcher_automation();
        } else {
            ctx.model.transport_mut().toggle_write_arranger_automation();
        }
    }
}

/// Changes the shuffle amount of the groove (former `SwingCommand`).
#[derive(Default)]
pub struct SwingCommand;

impl SwingCommand {
    pub fn new() -> Self {
        SwingCommand
    }
}

impl ContinuousCommand for SwingCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        ctx.model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount).change_value(value);
    }
}
