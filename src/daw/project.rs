// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The project (former `IProject` / `ProjectImpl`). Solo/mute functions of the project are on
//! the track bank (`TrackBank::has_solo` etc.), scene creation is
//! [`Project::create_scene`] which takes the new clip length (from the configuration).

use crate::backend::Processor;

use super::actions;
use super::context::DawContext;
use super::events::ParameterBankId;
use super::parameter_bank::ParameterBank;

/// The project.
pub struct Project {
    ctx: DawContext,
    name: String,
    is_dirty: bool,
    parameter_bank: Option<ParameterBank>,
}

impl Project {
    pub fn new(ctx: DawContext, num_params: usize) -> Self {
        let parameter_bank =
            if num_params > 0 { Some(ParameterBank::new(ctx.clone(), ParameterBankId::Project, Processor::Master, num_params, false)) } else { None };
        Project { ctx, name: "None".into(), is_dirty: false, parameter_bank }
    }

    pub fn enable_observers(&self, enable: bool) {
        self.ctx.enable_updates(Processor::Project, enable);
    }

    /// The name of the project ("None" if not set).
    pub fn name(&self) -> &str {
        if self.name.is_empty() {
            "None"
        } else {
            &self.name
        }
    }

    /// Switch to the previous project tab.
    pub fn previous(&mut self) {
        self.ctx.invoke_action(actions::PROJECT_TAB_PREVIOUS);
    }

    /// Switch to the next project tab.
    pub fn next(&mut self) {
        self.ctx.invoke_action(actions::PROJECT_TAB_NEXT);
    }

    pub fn create_scene_from_playing_launcher_clips(&mut self) {
        self.ctx.send(Processor::Project, "createSceneFromPlayingLauncherClips");
    }

    /// Create a new scene with the given length (former `createScene`, the length is
    /// `configuration.getNewClipLenghthInBeats(transport.getQuartersPerMeasure())`).
    pub fn create_scene(&mut self, new_clip_length_in_beats: i32) {
        self.ctx.send_int(Processor::Project, "createScene", new_clip_length_in_beats);
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn save(&mut self) {
        self.ctx.invoke_action(actions::PROJECT_SAVE);
    }

    pub fn load(&mut self) {
        self.ctx.invoke_action(actions::PROJECT_LOAD);
    }

    /// The parameter bank of the project (master FX).
    pub fn parameter_bank(&self) -> Option<&ParameterBank> {
        self.parameter_bank.as_ref()
    }

    pub fn parameter_bank_mut(&mut self) -> Option<&mut ParameterBank> {
        self.parameter_bank.as_mut()
    }

    // Cue volume and mix are not supported in Reaper

    pub fn cue_volume_str(&self) -> &str {
        ""
    }

    pub fn cue_volume(&self) -> i32 {
        0
    }

    pub fn change_cue_volume(&mut self, _control: i32) {}

    pub fn set_cue_volume(&mut self, _value: i32) {}

    pub fn reset_cue_volume(&mut self) {}

    pub fn touch_cue_volume(&mut self, _touched: bool) {}

    pub fn cue_mix_str(&self) -> &str {
        ""
    }

    pub fn cue_mix(&self) -> i32 {
        0
    }

    pub fn change_cue_mix(&mut self, _control: i32) {}

    pub fn set_cue_mix(&mut self, _value: i32) {}

    pub fn reset_cue_mix(&mut self) {}

    pub fn touch_cue_mix(&mut self, _touched: bool) {}

    // Internal setters

    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_dirty(&mut self, is_dirty: bool) {
        self.is_dirty = is_dirty;
    }
}
