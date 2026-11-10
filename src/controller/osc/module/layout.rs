// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The layout, the panels, the arranger and the mixer (former `LayoutModule`).

use super::{sub_command, to_string, OscError, OscModule, OscResult, OscShared, Path, Value, TAG_MIXER};
use crate::framework::ctx::Ctx;

/// Former `LayoutModule`.
#[derive(Default)]
pub struct LayoutModule;

impl OscModule for LayoutModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["layout", "panel", "arranger", TAG_MIXER]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        match command {
            "layout" => {
                let layout = to_string(value)?.to_uppercase();
                ctx.model.application_mut().set_panel_layout(&layout);
            }
            "panel" => {
                let sub = sub_command(path)?;
                let app = ctx.model.application_mut();
                match sub.as_str() {
                    "noteEditor" => app.toggle_note_editor(),
                    "automationEditor" => app.toggle_automation_editor(),
                    "devices" => app.toggle_devices(),
                    TAG_MIXER => app.toggle_mixer(),
                    "fullscreen" => app.toggle_full_screen(),
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            "arranger" => {
                let sub = sub_command(path)?;
                let arranger = ctx.model.arranger_mut();
                match sub.as_str() {
                    "cueMarkerVisibility" => arranger.toggle_cue_marker_visibility(),
                    "playbackFollow" => arranger.toggle_playback_follow(),
                    "trackRowHeight" => arranger.toggle_track_row_height(),
                    "clipLauncherSectionVisibility" => arranger.toggle_clip_launcher(),
                    "timeLineVisibility" => arranger.toggle_time_line(),
                    "ioSectionVisibility" => arranger.toggle_io_section(),
                    "effectTracksVisibility" => arranger.toggle_effect_tracks(),
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            TAG_MIXER => {
                let sub = sub_command(path)?;
                let mixer = ctx.model.mixer_mut();
                match sub.as_str() {
                    "clipLauncherSectionVisibility" => mixer.toggle_clip_launcher_section_visibility(),
                    "crossFadeSectionVisibility" => mixer.toggle_cross_fade_section_visibility(),
                    "deviceSectionVisibility" => mixer.toggle_device_section_visibility(),
                    "sendsSectionVisibility" => mixer.toggle_sends_section_visibility(),
                    "ioSectionVisibility" => mixer.toggle_io_section_visibility(),
                    "meterSectionVisibility" => mixer.toggle_meter_section_visibility(),
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            _ => return Err(OscError::UnknownCommand(command.to_string())),
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let app = ctx.model.application();
        w.send_str("/layout", &app.panel_layout().to_lowercase(), dump);

        let arranger = ctx.model.arranger();
        w.send_bool("/arranger/cueMarkerVisibility", arranger.are_cue_markers_visible(), dump);
        w.send_bool("/arranger/playbackFollow", arranger.is_playback_follow_enabled(), dump);
        w.send_bool("/arranger/trackRowHeight", arranger.has_double_row_track_height(), dump);
        w.send_bool("/arranger/clipLauncherSectionVisibility", arranger.is_clip_launcher_visible(), dump);
        w.send_bool("/arranger/timeLineVisibility", arranger.is_timeline_visible(), dump);
        w.send_bool("/arranger/ioSectionVisibility", arranger.is_io_section_visible(), dump);
        w.send_bool("/arranger/effectTracksVisibility", arranger.are_effect_tracks_visible(), dump);

        let mixer = ctx.model.mixer();
        w.send_bool("/mixer/clipLauncherSectionVisibility", mixer.is_clip_launcher_section_visible(), dump);
        w.send_bool("/mixer/crossFadeSectionVisibility", mixer.is_cross_fade_section_visible(), dump);
        w.send_bool("/mixer/deviceSectionVisibility", mixer.is_device_section_visible(), dump);
        w.send_bool("/mixer/sendsSectionVisibility", mixer.is_send_section_visible(), dump);
        w.send_bool("/mixer/ioSectionVisibility", mixer.is_io_section_visible(), dump);
        w.send_bool("/mixer/meterSectionVisibility", mixer.is_meter_section_visible(), dump);
    }
}
