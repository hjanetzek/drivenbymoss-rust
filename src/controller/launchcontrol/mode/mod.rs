// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Launch Control XL (former `launchcontrol.mode`).

pub mod buttons;
pub mod faders;
pub mod main;
pub mod note_parameter;

use crate::framework::mode::note_editor::ClipKey;

/// The clip which is edited by the sequencer modes: 8 steps, 127 rows.
pub const CLIP_KEY: ClipKey = (8, 127);
