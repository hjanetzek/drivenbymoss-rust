// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The layout and panel mode (former `FrameMode`).
//!
//! Difference to Java: the arranger and mixer have no observers to enable (the host reports
//! their states always).

use super::{forward_base, row_index};
use crate::controller::push::mode::PushDisplayMode;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR2_HI, BUTTON_COLOR2_ON, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::ButtonId;
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

const ROW0: &str = "Layouts:                  Panels:                                   ";
const ROW1: &str = "Arrange  Mix     Edit     Notes   Automate Device  Mixer    Inspectr";
const ARRANGER_ROWS: [&str; 2] = ["Arranger:                                                           ", "ClpLnchr I/O     Markers  TimelineFXTracks Follow  TrckHght Full    "];
const MIXER_ROWS: [&str; 2] = ["Mixer:                                                              ", "ClpLnchr I/O     CrossFde Device  Meters   Sends            Full    "];
const EMPTY_ROWS: [&str; 2] = ["                                                                    ", "                                                                    "];
const LAYOUTS1: [&str; 8] = ["Layouts", "", "", "Panels", "", "", "", ""];
const LAYOUTS2: [&str; 8] = ["Arrange", "Mix", "Edit", "Notes", "Automate", "Device", "Mixer", "Inspector"];
const ARRANGER_OPTIONS: [[&str; 8]; 2] = [["Arranger", "", "", "", "", "", "", ""], ["Clip Launcher", "I/O", "Markers", "Timeline", "FX Tracks", "Follow", "Track Height", "Fullscreen"]];
const MIXER_OPTIONS: [[&str; 8]; 2] = [["Mixer", "", "", "", "", "", "", ""], ["Clip Launcher", "I/O", "Crossfader", "Device", "Meters", "Sends", "", "Fullscreen"]];
const EMPTY_OPTIONS: [[&str; 8]; 2] = [[""; 8], [""; 8]];

/// Select the panel layout and toggle the panels of the host.
pub struct FrameMode {
    base: ParameterModeBase,
}

impl FrameMode {
    pub fn new() -> Self {
        FrameMode { base: super::base("Frame") }
    }

    fn first_row_button_state(ctx: &Ctx, index: usize) -> bool {
        let app = ctx.model.application();
        match index {
            0 => app.is_arrange_layout(),
            1 => app.is_mixer_layout(),
            2 => app.is_edit_layout(),
            _ => false,
        }
    }

    fn second_row_button_state(ctx: &Ctx, index: usize) -> i32 {
        let app = ctx.model.application();
        if app.is_arrange_layout() {
            let arrange = ctx.model.arranger();
            let state = match index {
                0 => arrange.is_clip_launcher_visible(),
                1 => arrange.is_io_section_visible(),
                2 => arrange.are_cue_markers_visible(),
                3 => arrange.is_timeline_visible(),
                4 => arrange.are_effect_tracks_visible(),
                5 => arrange.is_playback_follow_enabled(),
                6 => arrange.has_double_row_track_height(),
                _ => false,
            };
            return i32::from(state);
        }
        if app.is_mixer_layout() {
            let mix = ctx.model.mixer();
            let state = match index {
                0 => mix.is_clip_launcher_section_visible(),
                1 => mix.is_io_section_visible(),
                2 => mix.is_cross_fade_section_visible(),
                3 => mix.is_device_section_visible(),
                4 => mix.is_meter_section_visible(),
                5 => mix.is_send_section_visible(),
                6 => return -1,
                _ => false,
            };
            return i32::from(state);
        }
        -1
    }
}

impl Default for FrameMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for FrameMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let app = ctx.model.application();
        let rows34 = if app.is_arrange_layout() {
            ARRANGER_ROWS
        } else if app.is_mixer_layout() {
            MIXER_ROWS
        } else {
            EMPTY_ROWS
        };
        display.set_row(0, ROW0).set_row(1, ROW1).set_row(2, rows34[0]).set_row(3, rows34[1]);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let app = ctx.model.application();
        let options = if app.is_arrange_layout() {
            ARRANGER_OPTIONS
        } else if app.is_mixer_layout() {
            MIXER_OPTIONS
        } else {
            EMPTY_OPTIONS
        };
        for i in 0..8 {
            display.add_option_element(options[0][i], options[1][i], Self::second_row_button_state(ctx, i) > 0, LAYOUTS1[i], LAYOUTS2[i], Self::first_row_button_state(ctx, i), false);
        }
    }
}

impl Mode for FrameMode {
    forward_base!();

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up {
            return;
        }
        if row == 0 {
            let app = ctx.model.application_mut();
            match index {
                0 => app.set_panel_layout("ARRANGE"),
                1 => app.set_panel_layout("MIX"),
                2 => app.set_panel_layout("EDIT"),
                3 => app.toggle_note_editor(),
                4 => app.toggle_automation_editor(),
                5 => app.toggle_devices(),
                6 => app.toggle_mixer(),
                7 => app.toggle_inspector(),
                _ => {}
            }
            return;
        }

        if ctx.model.application().is_arrange_layout() {
            if index == 7 {
                ctx.model.application_mut().toggle_full_screen();
                return;
            }
            let arrange = ctx.model.arranger_mut();
            match index {
                0 => arrange.toggle_clip_launcher(),
                1 => arrange.toggle_io_section(),
                2 => arrange.toggle_cue_marker_visibility(),
                3 => arrange.toggle_time_line(),
                4 => arrange.toggle_effect_tracks(),
                5 => arrange.toggle_playback_follow(),
                6 => arrange.toggle_track_row_height(),
                _ => {}
            }
        } else if ctx.model.application().is_mixer_layout() {
            if index == 7 {
                ctx.model.application_mut().toggle_full_screen();
                return;
            }
            let mix = ctx.model.mixer_mut();
            match index {
                0 => mix.toggle_clip_launcher_section_visibility(),
                1 => mix.toggle_io_section_visibility(),
                2 => mix.toggle_cross_fade_section_visibility(),
                3 => mix.toggle_device_section_visibility(),
                4 => mix.toggle_meter_section_visibility(),
                5 => mix.toggle_sends_section_visibility(),
                _ => {}
            }
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        if let Some(index) = row_index(0, button) {
            return if Self::first_row_button_state(ctx, index) { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
        }
        if let Some(index) = row_index(1, button) {
            return match Self::second_row_button_state(ctx, index) {
                0 => BUTTON_COLOR2_ON,
                1 => BUTTON_COLOR2_HI,
                _ => BUTTON_COLOR_OFF,
            };
        }
        BUTTON_COLOR_OFF
    }

    impl_as_any!();
}
