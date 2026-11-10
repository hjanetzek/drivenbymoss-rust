// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The layout, the panels, the arranger and the mixer (former `LayoutModule`).

use super::super::address::{ArrangerToggle, LayoutIn, MixerToggle, Panel};
use super::{to_string, OscResult, OscShared, Value};
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::{Key, OscWriter};

/// Former `LayoutModule`.
pub struct LayoutModule {
    layout: Key<str>,
    arranger: Vec<(ArrangerToggle, Key<bool>)>,
    mixer: Vec<(MixerToggle, Key<bool>)>,
}

impl LayoutModule {
    pub fn new(w: &mut OscWriter) -> Self {
        LayoutModule {
            layout: w.key("/layout"),
            arranger: ArrangerToggle::ALL.iter().map(|&t| (t, w.key(&format!("/arranger/{}", t.text())))).collect(),
            mixer: MixerToggle::ALL.iter().map(|&t| (t, w.key(&format!("/mixer/{}", t.text())))).collect(),
        }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: LayoutIn, value: Value) -> OscResult {
        match command {
            LayoutIn::Layout {} => {
                let layout = to_string(value)?.to_uppercase();
                ctx.model.application_mut().set_panel_layout(&layout);
            }
            LayoutIn::Panel { panel } => {
                let app = ctx.model.application_mut();
                match panel {
                    Panel::NoteEditor => app.toggle_note_editor(),
                    Panel::AutomationEditor => app.toggle_automation_editor(),
                    Panel::Devices => app.toggle_devices(),
                    Panel::Mixer => app.toggle_mixer(),
                    Panel::Fullscreen => app.toggle_full_screen(),
                }
            }
            LayoutIn::Arranger { toggle } => {
                let arranger = ctx.model.arranger_mut();
                match toggle {
                    ArrangerToggle::CueMarkerVisibility => arranger.toggle_cue_marker_visibility(),
                    ArrangerToggle::PlaybackFollow => arranger.toggle_playback_follow(),
                    ArrangerToggle::TrackRowHeight => arranger.toggle_track_row_height(),
                    ArrangerToggle::ClipLauncherSectionVisibility => arranger.toggle_clip_launcher(),
                    ArrangerToggle::TimeLineVisibility => arranger.toggle_time_line(),
                    ArrangerToggle::IoSectionVisibility => arranger.toggle_io_section(),
                    ArrangerToggle::EffectTracksVisibility => arranger.toggle_effect_tracks(),
                }
            }
            LayoutIn::Mixer { toggle } => {
                let mixer = ctx.model.mixer_mut();
                match toggle {
                    MixerToggle::ClipLauncherSectionVisibility => mixer.toggle_clip_launcher_section_visibility(),
                    MixerToggle::CrossFadeSectionVisibility => mixer.toggle_cross_fade_section_visibility(),
                    MixerToggle::DeviceSectionVisibility => mixer.toggle_device_section_visibility(),
                    MixerToggle::SendsSectionVisibility => mixer.toggle_sends_section_visibility(),
                    MixerToggle::IoSectionVisibility => mixer.toggle_io_section_visibility(),
                    MixerToggle::MeterSectionVisibility => mixer.toggle_meter_section_visibility(),
                }
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let app = ctx.model.application();
        w.put(self.layout, &app.panel_layout().to_lowercase(), dump);

        let arranger = ctx.model.arranger();
        for &(toggle, key) in &self.arranger {
            let value = match toggle {
                ArrangerToggle::CueMarkerVisibility => arranger.are_cue_markers_visible(),
                ArrangerToggle::PlaybackFollow => arranger.is_playback_follow_enabled(),
                ArrangerToggle::TrackRowHeight => arranger.has_double_row_track_height(),
                ArrangerToggle::ClipLauncherSectionVisibility => arranger.is_clip_launcher_visible(),
                ArrangerToggle::TimeLineVisibility => arranger.is_timeline_visible(),
                ArrangerToggle::IoSectionVisibility => arranger.is_io_section_visible(),
                ArrangerToggle::EffectTracksVisibility => arranger.are_effect_tracks_visible(),
            };
            w.put(key, value, dump);
        }

        let mixer = ctx.model.mixer();
        for &(toggle, key) in &self.mixer {
            let value = match toggle {
                MixerToggle::ClipLauncherSectionVisibility => mixer.is_clip_launcher_section_visible(),
                MixerToggle::CrossFadeSectionVisibility => mixer.is_cross_fade_section_visible(),
                MixerToggle::DeviceSectionVisibility => mixer.is_device_section_visible(),
                MixerToggle::SendsSectionVisibility => mixer.is_send_section_visible(),
                MixerToggle::IoSectionVisibility => mixer.is_io_section_visible(),
                MixerToggle::MeterSectionVisibility => mixer.is_meter_section_visible(),
            };
            w.put(key, value, dump);
        }
    }
}
