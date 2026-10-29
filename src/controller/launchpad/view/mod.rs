// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Launchpad views (former `controller.novation.launchpad.view`).

pub mod virtual_fader_view_callback;
pub mod sequencer_common;
pub mod sequencer_view;
pub mod play_controls;
pub mod play_view;
pub mod piano_view;
pub mod chords_view;
pub mod drum64_view;
pub mod poly_sequencer_view;
pub mod raindrops_view;
pub mod drum_view;
pub mod drum_lane_view;
pub mod note_edit_view;
pub mod session_view;
pub mod abstract_fader_view;
pub mod volume_view;
pub mod pan_view;
pub mod sends_view;
pub mod device_view;
pub mod user_view;
#[cfg(test)]
mod sequencer_tests;
pub mod mix_view;
pub mod shift_view;
pub mod project_view;
pub mod note_view_select_view;
pub mod browser_view;
pub mod tempo_view;
