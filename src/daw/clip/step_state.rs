// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The state of a sequencer step (former `StepState`).

/// Enumeration for the state of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StepState {
    /// Step contains no note.
    #[default]
    Off,
    /// A note starts at this step.
    Start,
    /// A started note continues at that step.
    Continue,
}
