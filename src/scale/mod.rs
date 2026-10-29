// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Scales and scale layouts (former `framework.scale`) and the key manager.
//!
//! * [`Scale`] - all supported musical scales with their intervals.
//! * [`ScaleLayout`] - the layouts in which a scale can be applied to a pad grid.
//! * [`Scales`] - the main helper which creates the note matrices for a pad grid (scale, drum,
//!   piano, sequencer and chord layouts), holds octave/base note settings and formats notes.
//! * [`KeyManager`] - tracks the pressed keys of a pad grid and maps pads to MIDI notes (former
//!   `framework.utils.KeyManager`).
//! * [`MpeStatus`] - stores the note and slide status of MIDI channels for MPE.
//!
//! Note matrices are `[i32; 128]` arrays which map a pad (incoming MIDI note) to the MIDI note to
//! play, where `-1` means "no note".

mod chord_grid;
mod key_manager;
mod mpe_status;
#[allow(clippy::module_inception)]
mod scale;
mod scale_grid;
mod scale_layout;
mod scales;

pub use chord_grid::ChordGrid;
pub use key_manager::KeyManager;
pub use mpe_status::MpeStatus;
pub use scale::Scale;
pub use scale_grid::{Orientation, ScaleGrid};
pub use scale_layout::ScaleLayout;
pub use scales::{GridTranslator, NoteMatrix, ScaleProperties, Scales};
