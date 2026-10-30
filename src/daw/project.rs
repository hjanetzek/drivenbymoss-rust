// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The project (former `IProject` / `ProjectImpl`). Solo/mute functions of the project are on
//! the track bank (`TrackBank::has_solo` etc.), scene creation is
//! [`Project::create_scene`] which takes the new clip length (from the configuration).

use crate::backend::Processor;

use super::context::DawContext;
use super::events::ParameterBankId;
use super::parameter::{Parameter, ParameterKind};
use super::parameter_bank::ParameterBank;

/// The project.
pub struct Project {
    ctx: DawContext,
    name: String,
    is_dirty: bool,
    parameter_bank: Option<ParameterBank>,
    /// ProjectImpl (Bitwig): `ParameterImpl` on `project.cueVolume ()` and `cueMix ()`; they exist
    /// once the host reports them.
    cue_volume: Parameter,
    cue_mix: Parameter,
    num_params: usize,
}

impl Project {
    pub fn new(ctx: DawContext, num_params: usize) -> Self {
        let parameter_bank =
            if num_params > 0 { Some(ParameterBank::new(ctx.clone(), ParameterBankId::Project, Processor::Master, num_params, false)) } else { None };
        let cue_volume = Parameter::new(ctx.clone(), ParameterKind::ProjectCue("cueVolume"), 0, 0.0);
        let cue_mix = Parameter::new(ctx.clone(), ParameterKind::ProjectCue("cueMix"), 0, 0.0);
        Project { ctx, name: "None".into(), is_dirty: false, parameter_bank, cue_volume, cue_mix, num_params }
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
        self.ctx.send(Processor::Project, "previous");
    }

    /// Switch to the next project tab.
    pub fn next(&mut self) {
        self.ctx.send(Processor::Project, "next");
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
        self.ctx.send(Processor::Project, "save");
    }

    pub fn load(&mut self) {
        self.ctx.send(Processor::Project, "load");
    }

    /// The parameter bank of the project (master FX).
    pub fn parameter_bank(&self) -> Option<&ParameterBank> {
        self.parameter_bank.as_ref()
    }

    pub fn parameter_bank_mut(&mut self) -> Option<&mut ParameterBank> {
        self.parameter_bank.as_mut()
    }

    /// The project's remote controls of the host, for the parser (Bitwig: ProjectImpl,
    /// `rootTrackGroup.createCursorRemoteControlsPage`, reported as `/project/param/…` and
    /// `/project/page/…`). The first report replaces the bank on the master FX (Reaper) with one
    /// whose commands go to the project (`project param/<i>/value`, `project parameterPage`).
    pub fn host_parameter_bank_mut(&mut self) -> Option<&mut ParameterBank> {
        if self.num_params == 0 {
            return None;
        }
        if self.parameter_bank.as_ref().is_none_or(|b| b.processor() != Processor::Project) {
            self.parameter_bank = Some(ParameterBank::new(self.ctx.clone(), ParameterBankId::Project, Processor::Project, self.num_params, false));
        }
        self.parameter_bank.as_mut()
    }

    // Cue volume and mix (ProjectImpl of the Java version for Bitwig); Reaper has none, they do
    // not exist there (nothing is sent)

    /// The cue volume parameter (exists if the host reports it).
    pub fn cue_volume_parameter(&self) -> &Parameter {
        &self.cue_volume
    }

    pub fn cue_volume_parameter_mut(&mut self) -> &mut Parameter {
        &mut self.cue_volume
    }

    pub fn cue_volume_str(&self) -> String {
        self.cue_volume.displayed_value()
    }

    /// The cue volume text shortened by the host (ProjectImpl.getCueVolumeStr (limit)).
    pub fn cue_volume_str_limited(&self, limit: usize) -> String {
        self.cue_volume.displayed_value_limited(limit)
    }

    pub fn cue_volume(&self) -> i32 {
        self.cue_volume.value()
    }

    pub fn change_cue_volume(&mut self, control: i32) {
        if self.cue_volume.does_exist() {
            self.cue_volume.change_value(control);
        }
    }

    pub fn set_cue_volume(&mut self, value: i32) {
        self.cue_volume.set_value(value);
    }

    pub fn reset_cue_volume(&mut self) {
        if self.cue_volume.does_exist() {
            self.cue_volume.reset_value();
        }
    }

    pub fn touch_cue_volume(&mut self, touched: bool) {
        if self.cue_volume.does_exist() {
            self.cue_volume.touch_value(touched);
        }
    }

    /// The cue mix parameter (exists if the host reports it).
    pub fn cue_mix_parameter(&self) -> &Parameter {
        &self.cue_mix
    }

    pub fn cue_mix_parameter_mut(&mut self) -> &mut Parameter {
        &mut self.cue_mix
    }

    pub fn cue_mix_str(&self) -> String {
        self.cue_mix.displayed_value()
    }

    /// The cue mix text shortened by the host (ProjectImpl.getCueMixStr (limit)).
    pub fn cue_mix_str_limited(&self, limit: usize) -> String {
        self.cue_mix.displayed_value_limited(limit)
    }

    pub fn cue_mix(&self) -> i32 {
        self.cue_mix.value()
    }

    pub fn change_cue_mix(&mut self, control: i32) {
        if self.cue_mix.does_exist() {
            self.cue_mix.change_value(control);
        }
    }

    pub fn set_cue_mix(&mut self, value: i32) {
        self.cue_mix.set_value(value);
    }

    pub fn reset_cue_mix(&mut self) {
        if self.cue_mix.does_exist() {
            self.cue_mix.reset_value();
        }
    }

    pub fn touch_cue_mix(&mut self, touched: bool) {
        if self.cue_mix.does_exist() {
            self.cue_mix.touch_value(touched);
        }
    }

    // Internal setters

    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_dirty(&mut self, is_dirty: bool) {
        self.is_dirty = is_dirty;
    }
}
