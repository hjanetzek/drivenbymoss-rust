// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The application (former `IApplication` / `ApplicationImpl`), the arranger (former
//! `IArranger` / `ArrangerImpl`) and the mixer window (former `IMixer` / `MixerImpl`).

use crate::protocol::{Cmd, Dir};

use super::color::ColorEx;
use super::constants::ChannelType;
use super::context::{DawContext, HostFeature};
use super::track_bank::add_channel;

pub const PANEL_LAYOUT_ARRANGE: &str = "ARRANGE";
pub const PANEL_LAYOUT_MIX: &str = "MIX";
pub const PANEL_LAYOUT_EDIT: &str = "EDIT";
pub const PANEL_LAYOUT_PLAY: &str = "PLAY";

const PANEL_LAYOUTS: [&str; 4] = [PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_MIX, PANEL_LAYOUT_EDIT, PANEL_LAYOUT_PLAY];

/// The application.
pub struct Application {
    ctx: DawContext,
    panel_layout: &'static str,
    engine_active: bool,
    window_layout: i32,
    can_undo_state: bool,
    can_redo_state: bool,
}

impl Application {
    pub fn new(ctx: DawContext) -> Self {
        Application { ctx, panel_layout: PANEL_LAYOUT_ARRANGE, engine_active: true, window_layout: 0, can_undo_state: true, can_redo_state: true }
    }

    pub fn is_engine_active(&self) -> bool {
        self.engine_active
    }

    pub fn set_engine_active(&mut self, active: bool) {
        self.ctx.send(Cmd::ProjectEngine { value: active });
    }

    pub fn toggle_engine_active(&mut self) {
        let v = !self.engine_active;
        self.set_engine_active(v);
    }

    /// Set the panel layout (one of the `PANEL_LAYOUT_*` constants).
    pub fn set_panel_layout(&mut self, panel_layout: &str) {
        match PANEL_LAYOUTS.iter().position(|l| *l == panel_layout) {
            Some(index) => {
                self.panel_layout = PANEL_LAYOUTS[index];
                self.window_layout = index as i32;
                self.ctx.send(Cmd::ApplicationLayout { value: index as i32 });
            }
            None => self.ctx.host().println(&format!("Not a supported layout: {panel_layout}")),
        }
    }

    pub fn panel_layout(&self) -> &str {
        self.panel_layout
    }

    /// ApplicationImpl.previousPanelLayout: `application.previousPanelLayout ()` on Bitwig.
    pub fn previous_panel_layout(&mut self) {
        if self.ctx.host_supports(HostFeature::LayoutStep) {
            self.ctx.send(Cmd::ApplicationLayoutStep { dir: Dir::Prev });
            return;
        }
        self.set_panel_layout_index(self.window_layout - 1);
    }

    /// ApplicationImpl.nextPanelLayout: `application.nextPanelLayout ()` on Bitwig.
    pub fn next_panel_layout(&mut self) {
        if self.ctx.host_supports(HostFeature::LayoutStep) {
            self.ctx.send(Cmd::ApplicationLayoutStep { dir: Dir::Next });
            return;
        }
        self.set_panel_layout_index(self.window_layout + 1);
    }

    /// The host reported its panel layout (Bitwig: `application.panelLayout ()`).
    pub fn set_internal_panel_layout(&mut self, panel_layout: &str) {
        if let Some(index) = PANEL_LAYOUTS.iter().position(|l| *l == panel_layout) {
            self.panel_layout = PANEL_LAYOUTS[index];
            self.window_layout = index as i32;
        }
    }

    fn set_panel_layout_index(&mut self, index: i32) {
        // Java: index % 4, which might be negative
        let index = index % 4;
        if index < 0 {
            return;
        }
        self.window_layout = index;
        self.panel_layout = PANEL_LAYOUTS[index as usize];
        self.ctx.send(Cmd::ApplicationLayout { value: index });
    }

    pub fn is_arrange_layout(&self) -> bool {
        self.panel_layout == PANEL_LAYOUT_ARRANGE
    }

    pub fn is_mixer_layout(&self) -> bool {
        self.panel_layout == PANEL_LAYOUT_MIX
    }

    pub fn is_edit_layout(&self) -> bool {
        self.panel_layout == PANEL_LAYOUT_EDIT
    }

    pub fn is_play_layout(&self) -> bool {
        self.panel_layout == PANEL_LAYOUT_PLAY
    }

    pub fn toggle_note_editor(&mut self) {
        self.ctx.send(Cmd::ApplicationToggleNoteEditor {});
    }

    pub fn toggle_automation_editor(&mut self) {
        self.ctx.send(Cmd::ApplicationToggleAutomationEditor {});
    }

    pub fn toggle_devices(&mut self) {
        self.ctx.send(Cmd::ApplicationToggleDevices {});
    }

    pub fn toggle_inspector(&mut self) {
        self.ctx.send(Cmd::ApplicationToggleInspector {});
    }

    pub fn toggle_mixer(&mut self) {
        self.ctx.send(Cmd::ApplicationToggleMixer {});
    }

    pub fn toggle_full_screen(&mut self) {
        self.ctx.send(Cmd::ApplicationToggleFullScreen {});
    }

    pub fn toggle_browser_visibility(&mut self) {
        self.ctx.send(Cmd::ApplicationToggleBrowser {});
    }

    pub fn duplicate(&mut self) {
        self.ctx.send(Cmd::ApplicationDuplicate {});
    }

    pub fn delete_selection(&mut self) {
        self.ctx.send(Cmd::ApplicationDeleteSelection {});
    }

    pub fn undo(&mut self) {
        self.ctx.send(Cmd::Undo {});
    }

    pub fn can_undo(&self) -> bool {
        self.can_undo_state
    }

    pub fn redo(&mut self) {
        self.ctx.send(Cmd::Redo {});
    }

    pub fn can_redo(&self) -> bool {
        self.can_redo_state
    }

    pub fn add_audio_track(&mut self) {
        add_channel(&self.ctx, ChannelType::Audio, None, None, &[]);
    }

    pub fn add_effect_track(&mut self) {
        add_channel(&self.ctx, ChannelType::Effect, None, None, &[]);
    }

    pub fn add_instrument_track(&mut self) {
        add_channel(&self.ctx, ChannelType::Instrument, None, None, &[]);
    }

    /// Add a track. `devices` are the creation names of devices to add.
    pub fn add_channel(&mut self, channel_type: ChannelType, name: Option<&str>, color: Option<ColorEx>, devices: &[&str]) {
        add_channel(&self.ctx, channel_type, name, color, devices);
    }

    pub fn arrow_key_left(&mut self) {
        self.ctx.send(Cmd::ApplicationArrowLeft {});
    }

    pub fn arrow_key_up(&mut self) {
        self.ctx.send(Cmd::ApplicationArrowUp {});
    }

    pub fn arrow_key_right(&mut self) {
        self.ctx.send(Cmd::ApplicationArrowRight {});
    }

    pub fn arrow_key_down(&mut self) {
        self.ctx.send(Cmd::ApplicationArrowDown {});
    }

    pub fn zoom_out(&mut self) {
        self.ctx.send(Cmd::ApplicationZoomOut {});
    }

    pub fn zoom_in(&mut self) {
        self.ctx.send(Cmd::ApplicationZoomIn {});
    }

    /// Change the horizontal zoom with a relative knob value (former zoom parameter).
    pub fn change_zoom(&mut self, control: i32) {
        if self.ctx.value_changer().is_increase(control) {
            self.zoom_in();
        } else {
            self.zoom_out();
        }
    }

    pub fn dec_track_height(&mut self) {
        self.ctx.send(Cmd::ApplicationDecTrackHeight {});
    }

    pub fn inc_track_height(&mut self) {
        self.ctx.send(Cmd::ApplicationIncTrackHeight {});
    }

    /// Change the track height with a relative knob value (former track height parameter).
    pub fn change_track_height(&mut self, control: i32) {
        if self.ctx.value_changer().is_increase(control) {
            self.inc_track_height();
        } else {
            self.dec_track_height();
        }
    }

    /// Not supported by the Reaper backend (sending key presses needs a robot).
    pub fn enter(&mut self) {
        self.ctx.send(Cmd::ApplicationEnter {});
    }

    /// Not supported by the Reaper backend (sending key presses needs a robot).
    pub fn escape(&mut self) {
        self.ctx.send(Cmd::ApplicationEscape {});
    }

    pub fn slice_to_sampler(&mut self) {
        self.ctx.send(Cmd::ApplicationSliceToSampler {});
    }

    pub fn slice_to_drum_machine(&mut self) {
        self.ctx.send(Cmd::ApplicationSliceToDrumMachine {});
    }

    /// Invoke an action by its (named) ID.
    pub fn invoke_action(&mut self, id: &str) {
        self.ctx.send(Cmd::Action { id });
    }

    /// Not supported.
    pub fn show_help(&mut self) {}

    // Internal setters

    pub fn set_internal_engine_active(&mut self, active: bool) {
        self.engine_active = active;
    }

    pub fn set_can_undo_state(&mut self, state: bool) {
        self.can_undo_state = state;
    }

    pub fn set_can_redo_state(&mut self, state: bool) {
        self.can_redo_state = state;
    }
}

/// The arranger.
pub struct Arranger {
    ctx: DawContext,
    follow_playback: bool,
    /// The visibility flags reported by the host (ArrangerImpl of the Java version for Bitwig):
    /// cueMarkers, doubleRowTrackHeight, clipLauncher, timeline, ioSection, effectTracks.
    flags: Flags,
}

/// Visibility flags reported by the host by name, false if not reported (Reaper).
#[derive(Default)]
struct Flags(Vec<(String, bool)>);

impl Flags {
    fn get(&self, name: &str) -> bool {
        self.0.iter().any(|(n, v)| n == name && *v)
    }

    fn set(&mut self, name: &str, value: bool) {
        match self.0.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value,
            None => self.0.push((name.to_string(), value)),
        }
    }
}

impl Arranger {
    pub fn new(ctx: DawContext) -> Self {
        Arranger { ctx, follow_playback: false, flags: Flags::default() }
    }

    /// The host reported a visibility flag (`/arranger/<name>`).
    pub fn set_flag_state(&mut self, name: &str, value: bool) {
        self.flags.set(name, value);
    }

    pub fn are_cue_markers_visible(&self) -> bool {
        self.flags.get("cueMarkers")
    }

    pub fn toggle_cue_marker_visibility(&mut self) {
        self.ctx.send(Cmd::ArrangerToggleCueMarkers {});
    }

    pub fn is_playback_follow_enabled(&self) -> bool {
        self.follow_playback
    }

    pub fn toggle_playback_follow(&mut self) {
        self.ctx.send(Cmd::ArrangerTogglePlaybackFollow {});
    }

    pub fn has_double_row_track_height(&self) -> bool {
        self.flags.get("doubleRowTrackHeight")
    }

    pub fn toggle_track_row_height(&mut self) {
        self.ctx.send(Cmd::ArrangerToggleTrackRowHeight {});
    }

    pub fn is_clip_launcher_visible(&self) -> bool {
        self.flags.get("clipLauncher")
    }

    pub fn toggle_clip_launcher(&mut self) {
        self.ctx.send(Cmd::ArrangerToggleClipLauncher {});
    }

    pub fn is_timeline_visible(&self) -> bool {
        self.flags.get("timeline")
    }

    pub fn toggle_time_line(&mut self) {
        self.ctx.send(Cmd::ArrangerToggleTimeLine {});
    }

    pub fn is_io_section_visible(&self) -> bool {
        self.flags.get("ioSection")
    }

    pub fn toggle_io_section(&mut self) {
        self.ctx.send(Cmd::ArrangerToggleIoSection {});
    }

    pub fn are_effect_tracks_visible(&self) -> bool {
        self.flags.get("effectTracks")
    }

    pub fn toggle_effect_tracks(&mut self) {
        self.ctx.send(Cmd::ArrangerToggleEffectTracks {});
    }

    /// Internal setter.
    pub fn set_playback_follow(&mut self, enabled: bool) {
        self.follow_playback = enabled;
    }
}

/// The mixer window.
pub struct Mixer {
    ctx: DawContext,
    /// The section visibility flags reported by the host (MixerImpl of the Java version for
    /// Bitwig): clipLauncherSection, crossFadeSection, deviceSection, ioSection, meterSection,
    /// sendSection.
    flags: Flags,
}

impl Mixer {
    pub fn new(ctx: DawContext) -> Self {
        Mixer { ctx, flags: Flags::default() }
    }

    /// The host reported a visibility flag (`/mixer/<name>`).
    pub fn set_flag_state(&mut self, name: &str, value: bool) {
        self.flags.set(name, value);
    }

    pub fn is_clip_launcher_section_visible(&self) -> bool {
        self.flags.get("clipLauncherSection")
    }

    pub fn toggle_clip_launcher_section_visibility(&mut self) {
        self.ctx.send(Cmd::MixerToggleClipLauncherSection {});
    }

    pub fn is_cross_fade_section_visible(&self) -> bool {
        self.flags.get("crossFadeSection")
    }

    pub fn toggle_cross_fade_section_visibility(&mut self) {
        self.ctx.send(Cmd::MixerToggleCrossFadeSection {});
    }

    pub fn is_device_section_visible(&self) -> bool {
        self.flags.get("deviceSection")
    }

    pub fn toggle_device_section_visibility(&mut self) {
        self.ctx.send(Cmd::MixerToggleDeviceSection {});
    }

    pub fn is_io_section_visible(&self) -> bool {
        self.flags.get("ioSection")
    }

    pub fn toggle_io_section_visibility(&mut self) {
        self.ctx.send(Cmd::MixerToggleIoSection {});
    }

    pub fn is_meter_section_visible(&self) -> bool {
        self.flags.get("meterSection")
    }

    pub fn toggle_meter_section_visibility(&mut self) {
        self.ctx.send(Cmd::MixerToggleMeterSection {});
    }

    pub fn is_send_section_visible(&self) -> bool {
        self.flags.get("sendSection")
    }

    pub fn toggle_sends_section_visibility(&mut self) {
        self.ctx.send(Cmd::MixerToggleSendsSection {});
    }
}
