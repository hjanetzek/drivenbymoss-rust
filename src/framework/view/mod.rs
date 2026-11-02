// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Generic views (former `framework.view` package). Controller specific views build on them by
//! composition: they own the generic part and forward the [`crate::framework::feature::View`]
//! functions they do not change.

pub mod base;
pub mod sequencer;
pub mod transpose_view;
pub mod play_view;
pub mod session_view;
pub mod scene_play_view;
pub mod browser_view;
pub mod number_display_view;
pub mod shift_view;
pub mod drum64_view;
pub mod deferred;
pub mod control_only_view;
