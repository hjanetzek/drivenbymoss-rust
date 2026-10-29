// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Note clips (former `framework.daw.clip` + `CursorClipImpl`).
//!
//! * [`CursorClip`] - the cursor clip (former `IClip` + `INoteClip` + `CursorClipImpl`)
//! * [`StepInfo`] - the data of a sequencer step (former `IStepInfo`, `DefaultStepInfo`,
//!   `StepInfoImpl`, `EmptyStepInfo`)
//! * [`NotePosition`], [`StepState`], [`NoteOccurrenceType`]
//! * [`Note`] / [`parse_notes`] - notes as sent by Reaper (also used for the playing notes)

mod cursor_clip;
mod note;
mod note_occurrence;
mod note_position;
mod session_alternative;
mod step_info;
mod step_state;

pub use cursor_clip::{CursorClip, NUM_CHANNELS};
pub use note::{parse_notes, Note};
pub use note_occurrence::NoteOccurrenceType;
pub use note_position::NotePosition;
pub use session_alternative::SessionAlternative;
pub use step_info::StepInfo;
pub use step_state::StepState;
