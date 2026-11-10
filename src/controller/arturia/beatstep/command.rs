// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the Beatstep (former `controller.arturia.beatstep.command`).

use super::view::as_beatstep_view_mut;
use crate::framework::command::{ContinuousCommand, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::Views;
use crate::midi::ButtonEvent;

/// Delegates the moves of a knob to the active view (former `KnobRowViewCommand`).
pub struct KnobRowViewCommand {
    index: usize,
}

impl KnobRowViewCommand {
    /// `index` is the index of the knob, 0-15.
    pub fn new(index: usize) -> Self {
        KnobRowViewCommand { index }
    }

    /// The relative value which is handed to the view: the change of the knob, limited to
    /// -10..10 to tame the weird jumps of the knobs, encoded as offset binary (0 = 64).
    /// `None` if the knob did not change.
    pub fn tamed_value(ctx: &Ctx, value: i32) -> Option<i32> {
        let knob_change = ctx.model.value_changer().calc_knob_change(value);
        if knob_change == 0.0 {
            return None;
        }
        Some(knob_change.clamp(-10.0, 10.0) as i32 + 64)
    }
}

impl ContinuousCommand for KnobRowViewCommand {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        let Some(view) = ctx.active_view() else {
            return;
        };
        let Some(value) = Self::tamed_value(ctx, value) else {
            return;
        };
        let Ok(mut view) = view.try_borrow_mut() else {
            return;
        };
        if let Some(view) = as_beatstep_view_mut(&mut *view) {
            view.on_knob(ctx, self.index, value);
        }
    }
}

/// Uses the step buttons of the Beatstep Pro for a better navigation of the views: the step
/// executes the pad of the shift view (former `StepCommand`). Like in Java, the setup does not
/// register it.
pub struct StepCommand {
    step: i32,
}

impl StepCommand {
    /// `step` is the index of the step, 0-15.
    pub fn new(step: i32) -> Self {
        StepCommand { step }
    }
}

impl TriggerCommand for StepCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        let Some(view) = ctx.views.get(Views::Shift) else {
            return;
        };
        let Ok(mut view) = view.try_borrow_mut() else {
            return;
        };
        view.on_grid_note(ctx, 36 + self.step, if event == ButtonEvent::Down { 127 } else { 0 });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn knob_changes_are_limited() {
        let clamp = |change: f64| change.clamp(-10.0, 10.0) as i32 + 64;
        assert_eq!(clamp(1.09), 65);
        assert_eq!(clamp(-1.09), 63);
        assert_eq!(clamp(25.0), 74);
        assert_eq!(clamp(-63.0), 54);
    }
}
