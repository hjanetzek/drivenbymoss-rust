// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for layout commands (former `LayoutHandler`).

use super::{is_button_pressed, to_midi_value, unsupported};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::application::{PANEL_LAYOUT_ARRANGE, PANEL_LAYOUT_EDIT, PANEL_LAYOUT_MIX};
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let application = ctx.model.application();
    let arranger = ctx.model.arranger();
    let mixer = ctx.model.mixer();
    match command {
        FlexiCommand::LayoutArrangerZoomIn | FlexiCommand::LayoutArrangerZoomOut => 0,
        FlexiCommand::LayoutSetArrangeLayout => to_midi_value(application.is_arrange_layout()),
        FlexiCommand::LayoutSetMixLayout => to_midi_value(application.is_mixer_layout()),
        FlexiCommand::LayoutSetEditLayout => to_midi_value(application.is_edit_layout()),
        FlexiCommand::LayoutToggleArrangerCueMarkers => to_midi_value(arranger.are_cue_markers_visible()),
        FlexiCommand::LayoutToggleArrangerPlaybackFollow => to_midi_value(arranger.is_playback_follow_enabled()),
        FlexiCommand::LayoutToggleArrangerClipLauncherSection => to_midi_value(arranger.is_clip_launcher_visible()),
        FlexiCommand::LayoutToggleArrangerTimeLine => to_midi_value(arranger.is_timeline_visible()),
        FlexiCommand::LayoutToggleArrangerIoSection => to_midi_value(arranger.is_io_section_visible()),
        FlexiCommand::LayoutToggleArrangerEffectTracks => to_midi_value(arranger.are_effect_tracks_visible()),
        FlexiCommand::LayoutToggleMixerClipLauncherSection => to_midi_value(mixer.is_clip_launcher_section_visible()),
        FlexiCommand::LayoutToggleMixerCrossFadeSection => to_midi_value(mixer.is_cross_fade_section_visible()),
        FlexiCommand::LayoutToggleMixerDeviceSection => to_midi_value(mixer.is_device_section_visible()),
        FlexiCommand::LayoutToggleMixerSendssection => to_midi_value(mixer.is_send_section_visible()),
        FlexiCommand::LayoutToggleMixerIoSection => to_midi_value(mixer.is_io_section_visible()),
        FlexiCommand::LayoutToggleMixerMeterSection => to_midi_value(mixer.is_meter_section_visible()),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    // Note: as in Java zoom out does not check if the button is pressed
    if command == FlexiCommand::LayoutArrangerZoomOut {
        ctx.model.application_mut().zoom_out();
        return;
    }
    if !is_button_pressed(knob_mode, value) {
        return;
    }
    match command {
        FlexiCommand::LayoutArrangerZoomIn => ctx.model.application_mut().zoom_in(),
        FlexiCommand::LayoutSetArrangeLayout => ctx.model.application_mut().set_panel_layout(PANEL_LAYOUT_ARRANGE),
        FlexiCommand::LayoutSetMixLayout => ctx.model.application_mut().set_panel_layout(PANEL_LAYOUT_MIX),
        FlexiCommand::LayoutSetEditLayout => ctx.model.application_mut().set_panel_layout(PANEL_LAYOUT_EDIT),
        FlexiCommand::LayoutToggleNoteEditor => ctx.model.application_mut().toggle_note_editor(),
        FlexiCommand::LayoutToggleAutomationEditor => ctx.model.application_mut().toggle_automation_editor(),
        FlexiCommand::LayoutToggleDevicesPanel => ctx.model.application_mut().toggle_devices(),
        FlexiCommand::LayoutToggleMixerPanel => ctx.model.application_mut().toggle_mixer(),
        FlexiCommand::LayoutToggleFullscreen => ctx.model.application_mut().toggle_full_screen(),
        FlexiCommand::LayoutToggleArrangerCueMarkers => ctx.model.arranger_mut().toggle_cue_marker_visibility(),
        FlexiCommand::LayoutToggleArrangerPlaybackFollow => ctx.model.arranger_mut().toggle_playback_follow(),
        FlexiCommand::LayoutToggleArrangerTrackRowHeight => ctx.model.arranger_mut().toggle_track_row_height(),
        FlexiCommand::LayoutToggleArrangerClipLauncherSection => ctx.model.arranger_mut().toggle_clip_launcher(),
        FlexiCommand::LayoutToggleArrangerTimeLine => ctx.model.arranger_mut().toggle_time_line(),
        FlexiCommand::LayoutToggleArrangerIoSection => ctx.model.arranger_mut().toggle_io_section(),
        FlexiCommand::LayoutToggleArrangerEffectTracks => ctx.model.arranger_mut().toggle_effect_tracks(),
        FlexiCommand::LayoutToggleMixerClipLauncherSection => ctx.model.mixer_mut().toggle_clip_launcher_section_visibility(),
        FlexiCommand::LayoutToggleMixerCrossFadeSection => ctx.model.mixer_mut().toggle_cross_fade_section_visibility(),
        FlexiCommand::LayoutToggleMixerDeviceSection => ctx.model.mixer_mut().toggle_device_section_visibility(),
        FlexiCommand::LayoutToggleMixerSendssection => ctx.model.mixer_mut().toggle_sends_section_visibility(),
        FlexiCommand::LayoutToggleMixerIoSection => ctx.model.mixer_mut().toggle_io_section_visibility(),
        FlexiCommand::LayoutToggleMixerMeterSection => ctx.model.mixer_mut().toggle_meter_section_visibility(),
        _ => unsupported(ctx, command),
    }
}
