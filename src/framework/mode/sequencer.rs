// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The base of sequencer modes (former `AbstractSequencerMode`). Embed it into a mode and call
//! [`SequencerModeBase::on_activate`] from the `on_activate` of the mode.

use crate::daw::clip::CursorClip;
use crate::daw::constants::Resolution;
use crate::framework::ctx::Ctx;
use crate::framework::mode::note_editor::ClipKey;

/// The state of a sequencer mode (former `AbstractSequencerMode`).
#[derive(Debug, Clone)]
pub struct SequencerModeBase {
    pub name: String,
    pub is_absolute: bool,
    pub clip_rows: usize,
    pub clip_cols: usize,
    pub use_daw_colors: bool,
    pub selected_resolution_index: usize,
}

impl SequencerModeBase {
    /// Creates the cursor clip of the given size in the model.
    pub fn new(name: &str, ctx: &mut Ctx, is_absolute: bool, clip_rows: usize, clip_cols: usize, use_daw_colors: bool) -> Self {
        ctx.model.note_clip(clip_cols, clip_rows);
        SequencerModeBase { name: name.to_string(), is_absolute, clip_rows, clip_cols, use_daw_colors, selected_resolution_index: 4 }
    }

    /// The key of the clip of the mode.
    pub fn clip_key(&self) -> ClipKey {
        (self.clip_cols, self.clip_rows)
    }

    /// Former `getClip`.
    pub fn clip<'a>(&self, ctx: &'a mut Ctx) -> &'a mut CursorClip {
        ctx.model.note_clip(self.clip_cols, self.clip_rows)
    }

    pub fn clip_ref<'a>(&self, ctx: &'a Ctx) -> Option<&'a CursorClip> {
        ctx.model.note_clip_ref(self.clip_cols, self.clip_rows)
    }

    /// Sets the step length of the clip to the selected resolution.
    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        let length = Resolution::value_at(self.selected_resolution_index);
        self.clip(ctx).set_step_length(length);
    }

    pub fn selected_resolution_index(&self) -> usize {
        self.selected_resolution_index
    }

    pub fn set_selected_resolution_index(&mut self, index: usize) {
        self.selected_resolution_index = index;
    }
}
