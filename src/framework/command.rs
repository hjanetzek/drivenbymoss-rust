// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Commands which are bound to hardware controls (former `TriggerCommand`, `ContinuousCommand`,
//! `AftertouchCommand` and `PitchbendCommand`).

use std::cell::RefCell;
use std::rc::Rc;

use super::ctx::Ctx;
use crate::midi::ButtonEvent;

/// A command which is executed by a button.
pub trait TriggerCommand {
    /// Execute the command. The default implementation calls [`TriggerCommand::execute_shifted`]
    /// if Shift is pressed, otherwise [`TriggerCommand::execute_normal`].
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if ctx.surface.is_shift_pressed() {
            self.execute_shifted(ctx, event);
        } else {
            self.execute_normal(ctx, event);
        }
    }

    fn execute_normal(&mut self, _ctx: &mut Ctx, _event: ButtonEvent) {}

    fn execute_shifted(&mut self, _ctx: &mut Ctx, _event: ButtonEvent) {}
}

/// A command which is executed by a continuous control (knob, fader) with a value.
pub trait ContinuousCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32);
}

/// A command which handles aftertouch.
pub trait AftertouchCommand {
    fn on_poly_aftertouch(&mut self, ctx: &mut Ctx, note: i32, value: i32);

    fn on_channel_aftertouch(&mut self, ctx: &mut Ctx, value: i32);
}

/// A command which handles pitchbend.
pub trait PitchbendCommand {
    fn on_pitchbend(&mut self, ctx: &mut Ctx, data1: i32, data2: i32);
}

/// A shared trigger command, which can be bound to a button and also be called by others (e.g.
/// to simulate a button press).
pub type SharedTrigger = Rc<RefCell<dyn TriggerCommand>>;

/// Wraps a command into a [`SharedTrigger`].
pub fn shared<T: TriggerCommand + 'static>(command: T) -> SharedTrigger {
    Rc::new(RefCell::new(command))
}

/// A trigger command implemented by a closure.
pub struct FnTrigger<F: FnMut(&mut Ctx, ButtonEvent, i32)>(pub F);

impl<F: FnMut(&mut Ctx, ButtonEvent, i32)> TriggerCommand for FnTrigger<F> {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        (self.0)(ctx, event, velocity);
    }
}

/// A continuous command implemented by a closure.
pub struct FnContinuous<F: FnMut(&mut Ctx, i32)>(pub F);

impl<F: FnMut(&mut Ctx, i32)> ContinuousCommand for FnContinuous<F> {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        (self.0)(ctx, value);
    }
}
