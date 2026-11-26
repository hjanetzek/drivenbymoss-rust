// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The help texts of the Maschine Mk2 (not part of Java, doc/CONTROLLER_HELP_PLAN.md): one table
//! ([`crate::help!`], `framework/help_table.rs`) for the buttons, the pads, the knobs and the
//! encoder, by the held modifiers, the active mode and view. Below it the functions of the state
//! dependent texts. The Mk3, the Mikro Mk3 and the Plus share the setup but have no table yet.
//!
//! The Mk2's controls in the shared setup (the template's CCs, `mk/surface.rs`): CONTROL is
//! `Drum` (CHANNEL), STEP `Row14`, SAMPLING `DeviceOnOff`, ALL `AddTrack` (FILE), AUTO
//! `Automation`, ENTER `Overdub` (LOCK), the arrows under the encoder `ArrowLeft` .. `ArrowRight` (one box),
//! the encoder push `FaderTouch1`, RESTART `Loop`, the transport's ◂ `Stop` and ▸ `TapTempo`,
//! GRID `Flip`, ERASE `Delete`, SCENE `Scene1`, PATTERN `Clip`, PAD MODE `Row11`, NAVIGATE
//! `ToggleDevice`, the buttons above the displays `Row21`-`Row28`, the group buttons
//! `Row31`-`Row38`. The buttons of the Mk3 which the Mk2 doesn't have (no control of its
//! template) are `none`.
//!
//! The texts follow the code (where it differs from the manual, the code wins; the differences
//! are listed in doc/CONTROLLER_HELP_PLAN.md §12).

use crate::config::maschine::MaschineConfiguration;
use crate::daw::bank::BankItem;
use crate::daw::constants::Resolution;
use crate::framework::commands::mode::is_send_mode;
use crate::framework::ctx::Ctx;
use crate::framework::help::{automation, browsing, group_track_names, lines, on_off, play_button, scene_names, shift_button, Help};
use crate::framework::help_table::{IntoRes, NoHelp, Res};
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::scale::Scales;

use super::command::trigger::{drum_view_state, play_view_state};
use super::mode::browse::BrowseMode;
use super::mode::edit_note::EditNoteMode;
use super::mode::parameters_mode::MaschineParametersMode;
use super::mode::track_modes::MaschineVolumeMode;
use super::mode::user_mode::MaschineUserMode;
use super::surface::{ext, has_shift};

crate::help! {
    global {
        // Modifiers
        Shift => |ctx| shift_button(ctx, MaschineConfiguration::SHIFT_VIEW_WHILE_HELD), long SHIFT_LONG,
        Shift + Select => |ctx| format!("Edit note mode (the knobs edit the selected steps): turn it {}", on_off(!ctx.modes.is_active(Modes::Note))),
        Select + Select => "Held: the buttons and pads do their Select function (see their boxes)",
        Select => "Hold: the Select functions (select without launching, punch in, fill, ...)" long SELECT_LONG,
        Delete + Delete => "Held: a pad or group button deletes, a touched knob resets its value",
        Delete => "Hold + pad: delete the scene / clip / notes; + group button: the track" long ERASE_LONG,
        Duplicate + Duplicate => "Held: a pad duplicates the scene / clip (copies a step), a group button the track",
        Duplicate => "Hold + pad: duplicate the scene / clip; + group button: the track" long DUPLICATE_LONG,

        // Transport
        Select + Shift + Play => |ctx| format!("Punch out: turn it {}", on_off(!ctx.model.transport().is_punch_out_enabled())),
        Shift + Play => |ctx| loop_text(ctx),
        Select + Play => |ctx| format!("Punch in: turn it {}", on_off(!ctx.model.transport().is_punch_in_enabled())),
        Play => |ctx| play_button(ctx), long PLAY_LONG,
        Shift + Record => |ctx| ctx.config.shifted_record_button_function().name(),
        Record + Record => "Held: press a group button to record arm its track",
        Record => |ctx| ctx.config.record_button_function().name(), long RECORD_LONG,
        Shift + Loop => none,
        Loop => |ctx| loop_text(ctx), long RESTART_LONG,
        Stop => |ctx| stop_button(ctx), long STOP_LONG,
        Shift + TapTempo => |ctx| format!("Metronome: turn it {}", on_off(!ctx.model.transport().is_metronome_on())),
        TapTempo => |ctx| format!("Tap the tempo (now {:.2})", ctx.model.transport().tempo()), long TAP_LONG,
        Flip => |ctx| grid_button(ctx), long GRID_LONG,

        // The upper left buttons
        Stop + Drum => "Add an audio track",
        Drum => "Add an instrument track (hold the transport's ◂ for an audio track)" long CONTROL_LONG,
        Row14 => |ctx| step_button(ctx), long STEP_LONG,
        Shift + Browse => |ctx| (ctx.surface.is_pressed(ButtonId::Shift) && !browsing(ctx)).then_some("Browser: insert a device before the selected one"),
        Select + Browse => |ctx| (!browsing(ctx)).then_some("Browser: insert a device after the selected one"),
        Browse => |ctx| browse_button(ctx), long BROWSE_LONG,
        Shift + DeviceOnOff => |ctx| slice(ctx, "Slice the selected audio clip to a sampler (multi-sample)"),
        DeviceOnOff => |ctx| slice(ctx, "Slice the selected audio clip to a drum machine"), long SAMPLING_LONG,
        AddTrack => "Save the project",
        Select + Automation => |ctx| format!("Next automation write mode (now {})", ctx.model.transport().automation_write_mode().label()),
        Shift + Automation => |ctx| automation(ctx, !ctx.config.is_flip_record()),
        Automation => |ctx| automation(ctx, ctx.config.is_flip_record()), long AUTO_LONG,

        // The master section
        Shift + Volume => |ctx| (!is_note_editing(ctx)).then(|| format!("Volume mode: the VU meters on the display: turn them {}", on_off(!volume_vu(ctx)))),
        Volume => |ctx| volume_button(ctx), long VOLUME_LONG,
        Shift + Swing => |ctx| (!is_note_editing(ctx)).then_some("Press: the previous of play position / loop start / loop length"),
        Swing => |ctx| swing_button(ctx), long SWING_LONG,
        Shift + TempoTouch => none,
        TempoTouch => |ctx| tempo_button(ctx), long TEMPO_LONG,
        Select + Repeat => |ctx| format!("Fill mode: turn it {}", on_off(!ctx.model.transport().is_fill_mode_active())),
        Shift + Repeat => "The note repeat configuration on the knobs",
        Repeat => |ctx| if ctx.modes.is_active(Modes::RepeatNote) {
            "Close the note repeat configuration".to_string()
        } else {
            format!("Note repeat: turn it {} (long press: the configuration)", on_off(!ctx.config.is_note_repeat_active()))
        }, long REPEAT_LONG,
        Select + Overdub => |ctx| format!("Clip launcher automation writing: turn it {}", on_off(!ctx.model.transport().is_writing_clip_launcher_automation())),
        Shift + Overdub => |ctx| format!("Clip launcher overdub: turn it {}", on_off(!ctx.model.is_launcher_overdub())),
        Overdub => |ctx| format!("Arranger overdub: turn it {}", on_off(!ctx.model.is_arranger_overdub())), long ENTER_LONG,
        // The arrows under the encoder: one box, the modes below
        ArrowLeft .. ArrowRight => none, long ARROWS_LONG,
        FaderTouch1 => |ctx| encoder_push(ctx), long PUSH_LONG,
        Scene1 + MasterKnob => |ctx| (!last_param(ctx)).then_some("Turn: the previous / next page of 16 scenes"),
        Clip + MasterKnob => |ctx| (!last_param(ctx)).then_some("Turn: the previous / next page of 16 clips"),
        Solo + MasterKnob => |ctx| (!last_param(ctx)).then_some("Turn: the previous / next 8 tracks"),
        Mute + MasterKnob => |ctx| (!last_param(ctx)).then_some("Turn: the previous / next 8 tracks"),
        MasterKnob => |ctx| encoder(ctx), long ENCODER_LONG,
        // The knobs are one box (`knobs`, the modes below)
        Knob1 | Knob2 | Knob3 | Knob4 | Knob5 | Knob6 | Knob7 | Knob8 => none,
        Shift + Row21 .. Row28 => "Send − / +: the previous send; Pin, Active: as without Shift; the others: nothing",
        Row21 .. Row28 => |ctx| display_buttons(ctx),

        // The column left of the pads
        Shift + Scene1 | Clip | ToggleDevice => none,
        Scene1 => |ctx| view_button(ctx, Views::ScenePlay, "the 16 scenes (hold + pad: stop all clips)"), long SCENE_LONG,
        Clip => |ctx| view_button(ctx, Views::Session, "the 16 clips of the selected track (hold + pad: stop)"), long PATTERN_LONG,
        Shift + Row11 => |ctx| pad_mode_shifted(ctx),
        Row11 => |ctx| pad_mode(ctx), long PAD_MODE_LONG,
        ToggleDevice => |ctx| view_button(ctx, Views::Device, "the device parameters, devices and pages"), long NAVIGATE_LONG,
        Shift + Solo => "Clear solo on all tracks",
        Solo + Solo => "Held: a group button solos its track, the encoder pages the tracks",
        Solo => "Hold + group button: solo the track" long SOLO_LONG,
        Shift + Mute => "Clear mute on all tracks",
        Mute + Mute => "Held: a group button mutes its track, the encoder pages the tracks",
        Mute => "Hold + group button: mute the track" long MUTE_LONG,

        // The group buttons: one box
        Solo + Row31 .. Row38 => "Solo / unsolo the track",
        Mute + Row31 .. Row38 => "Mute / unmute the track",
        Record + Row31 .. Row38 => "Record arm / disarm the track",
        Delete + Row31 .. Row38 => "Delete the track",
        Duplicate + Row31 .. Row38 => "Duplicate the track",
        Row31 .. Row38 => |ctx| group_row(ctx),

        // The Mk3's buttons, strip and footswitches, which the Mk2 doesn't have
        New | F1 | F2 | F3 | F4 | Device | AddEffect | Accent | Note | ArrowUp | ArrowDown | PageLeft | PageRight | LayoutArrange | Mixer => none,
        Row12 | Row13 | Footswitch1 | Footswitch2 | Footswitch3 | Footswitch4 | Crossfader => none,
    }

    // ------------------------------------------------------------------------------------------
    // The views: the pads

    view ScenePlay {
        Scene1 + grid => "Stop all clips (Shift: the alternative stop)",
        Duplicate + grid => "Duplicate the scene",
        Delete + grid => "Delete the scene",
        Select + grid => "Select the scene (no launch)",
        Shift + grid => "The alternative launch of the scene",
        grid => |ctx| Help::new("Launch the 16 scenes of the page").with_long(lines(&[&scene_names(ctx, 16), SCENE_VIEW_LONG])),
        Duplicate => "Hold + pad: duplicate the scene; + group button: the track",
        Delete => "Hold + pad: delete the scene; + group button: the track; + knob: reset",
    }
    view Session {
        Duplicate + grid => "Duplicate the clip",
        Clip + grid => "Stop the selected track's clip (Shift: the alternative stop)",
        Browse + grid => "Browse for a clip for the slot",
        Delete + grid => "Delete the clip",
        Select + grid => "Select the clip (no launch)",
        Shift + grid => "The alternative launch of the clip",
        grid => |ctx| clip_grid(ctx), long CLIP_VIEW_LONG,
        Duplicate => "Hold + pad: duplicate the clip; + group button: the track",
        Delete => "Hold + pad: delete the clip; + group button: the track; + knob: reset",
    }
    view Play {
        Mute + grid => |ctx| step_editor(ctx).then_some("Mute / unmute the note of the step"),
        Duplicate + grid => |ctx| step_editor(ctx).then_some("A step with a note: copy it; an empty step: paste it"),
        grid => |ctx| play_grid(ctx), long PLAY_VIEW_LONG,
        Duplicate => |ctx| duplicate_in_sequencer(ctx),
        Delete => "Hold + group button: delete the track; + knob: reset its value",
    }
    view Drum {
        Mute + grid => |ctx| if step_editor(ctx) { "Mute / unmute the note of the step" } else { "Mute / unmute the drum pad" },
        Duplicate + grid => |ctx| step_editor(ctx).then_some("A step with a note: copy it; an empty step: paste it"),
        Select + grid => |ctx| if step_editor(ctx) { "Raise the note repeat of the step by 1" } else { "Select the drum pad (no note)" },
        Delete + grid => |ctx| (!step_editor(ctx)).then_some("Delete the notes of the drum pad in the clip"),
        Solo + grid => |ctx| (!step_editor(ctx)).then_some("Solo / unsolo the drum pad"),
        Browse + grid => |ctx| (!step_editor(ctx)).then_some("Browse for the drum pad's sound"),
        grid => |ctx| drum_grid(ctx), long DRUM_VIEW_LONG,
        Duplicate => |ctx| duplicate_in_sequencer(ctx),
        Delete => |ctx| if step_editor(ctx) {
            "Hold + group button: delete the track; + knob: reset its value"
        } else {
            "Hold + drum pad: delete its notes; + group button: the track"
        },
    }
    view Device {
        grid => |ctx| device_grid(ctx), long DEVICE_VIEW_LONG,
    }
    view RepeatNote {
        grid => "7/8: the arpeggiator mode; 9/10: the note length; 13/14: the period" long NOTE_REPEAT_VIEW_LONG,
    }
    view Shift {
        grid => "1 undo, 2 redo, 5 quantize, 6 quantize 50%, 9 clear, 13-16 transpose" long SHIFT_VIEW_LONG,
    }

    // ------------------------------------------------------------------------------------------
    // The modes: the knobs, the arrows under the encoder

    mode Volume {
        knobs => |ctx| track_knobs(ctx, "The volume of 8 tracks"),
        Shift + ArrowLeft .. ArrowRight => "The previous / next 8 tracks",
        ArrowLeft .. ArrowRight => "Select the previous / next track (the encoder's)",
    }
    mode Pan {
        knobs => |ctx| track_knobs(ctx, "The panorama of 8 tracks"),
        Shift + ArrowLeft .. ArrowRight => "The previous / next 8 tracks",
        ArrowLeft .. ArrowRight => "Select the previous / next track (the encoder's)",
    }
    mode Send1 | Send2 | Send3 | Send4 | Send5 | Send6 | Send7 | Send8 {
        knobs => |ctx| send_knobs(ctx),
        Shift + ArrowLeft .. ArrowRight => "The previous / next 8 tracks",
        ArrowLeft .. ArrowRight => "Select the previous / next track (the encoder's)",
    }
    mode DeviceParams {
        knobs => |ctx| device_knobs(ctx),
        Shift + ArrowLeft .. ArrowRight => "The previous / next parameter page",
        ArrowLeft .. ArrowRight => "The encoder: the previous / next parameter",
    }
    mode User {
        knobs => |ctx| knobs(ctx, format!("The 8 {} parameters of the page", if is_project_mode(ctx) { "project" } else { "track" })),
        Shift + ArrowLeft .. ArrowRight => "The previous / next parameter page",
        ArrowLeft .. ArrowRight => "The encoder: the previous / next parameter",
    }
    mode Browser {
        knobs => "1-7: the filter columns, 8: the results" long BROWSER_LONG,
        Shift + ArrowLeft .. ArrowRight => "The previous / next browser tab (content type)",
        ArrowLeft .. ArrowRight => "The encoder: the previous / next column",
    }
    mode Tempo {
        knobs => |ctx| knobs(ctx, format!("Any knob: the tempo (now {:.2})", ctx.model.transport().tempo())),
        ArrowLeft .. ArrowRight => none,
    }
    mode Position {
        knobs => |ctx| knobs(ctx, "Any knob: the play position".to_string()),
        ArrowLeft .. ArrowRight => none,
    }
    mode LoopStart {
        knobs => |ctx| knobs(ctx, "Any knob: the loop start".to_string()),
        ArrowLeft .. ArrowRight => none,
    }
    mode LoopLength {
        knobs => |ctx| knobs(ctx, "Any knob: the loop length".to_string()),
        ArrowLeft .. ArrowRight => none,
    }
    mode RepeatNote {
        knobs => |ctx| note_repeat_knobs(ctx),
        ArrowLeft .. ArrowRight => "The encoder: the previous / next setting",
        Repeat => "Close the note repeat configuration",
    }
    mode Scales {
        knobs => |ctx| scale_knobs(ctx),
        Row11 => "Close the scale settings",
        Shift + Row11 => "Close the scale settings",
        ArrowLeft .. ArrowRight => "The encoder: the previous / next setting",
    }
    mode PlayOptions {
        knobs => |ctx| knobs(ctx, format!("7-8: the drum pads 16 up / down (offset {})", ctx.model.scales().drum_offset())),
        Row11 => "Close the drum settings",
        Shift + Row11 => "Close the drum settings",
        ArrowLeft .. ArrowRight => none,
    }
    mode Note {
        knobs => |ctx| note_knobs(ctx),
        Shift + ArrowLeft .. ArrowRight => |ctx| format!("The previous / next page of the note attributes (now {})", note_page(ctx)),
        ArrowLeft .. ArrowRight => "The encoder: the previous / next note attribute",
    }
}

// ------------------------------------------------------------------------------------------
// The long texts

const SHIFT_LONG: &str = "Hold Shift and press a button: its second function (each box shows it while Shift is held).\nA tap of Shift opens the Shift view on the pads (undo, redo, quantize, clear, transpose); Shift again closes it.\nWith the setting \"Shift Pads While Holding Shift\" the Shift view is shown while Shift is held. Shift + knobs / encoder: fine changes.\nWith NI's driver (no dbm-nihost) Shift is not sent: the transport's ◂ (STEP LEFT) is Shift then.";
const SELECT_LONG: &str = "Hold Select and press a button: its Select function (each box shows it while Select is held).\nSelect + pad: select the scene / clip without launching it (Drum: the drum pad; its sequencer: the step's note repeat +1).\nSelect + PLAY: punch in (+ Shift: punch out); + NOTE REPEAT: fill mode; + AUTO: the automation write mode; + ENTER: clip launcher automation; + BROWSE: insert a device after the selected one.\nShift + Select: the edit note mode.";
const ERASE_LONG: &str = "Hold Erase and press a pad: delete the scene (Scene view), the clip (Clip view), the notes of the drum pad (Drum view).\nErase + group button: delete the track. Erase + touch a knob or the encoder: reset its value to the default.";
const DUPLICATE_LONG: &str = "Hold Duplicate and press a pad: duplicate the scene (Scene view) or the clip (Clip view); in the step sequencer: copy a step with a note, then paste it to empty steps.\nDuplicate + group button: duplicate the track.";
const PLAY_LONG: &str = "Play: start / stop (the setting \"Behaviour on Pause\"); a double click returns to zero (setting).\nShift + Play: loop on/off. Select + Play: punch in. Select + Shift + Play: punch out.";
const RECORD_LONG: &str = "Rec: the setting \"Record Button Function\"; Shift + Rec: \"Shift + Record Button Function\".\nHold Rec and press a group button: record arm the track.";
const RESTART_LONG: &str = "Restart: the loop on/off (as Shift + Play).";
const STOP_LONG: &str = "The transport's ◂ (STEP LEFT) is Stop: stop playback (the setting \"Behaviour on Stop\"); stopped: back to the start; a double click: to the end (setting).\nHold it and press CONTROL: an audio track.\nWith NI's driver (no dbm-nihost, which sends the Shift button) it is Shift as well: hold it for the Shift functions, release it without using one to stop.";
const TAP_LONG: &str = "The transport's ▸ (STEP RIGHT): tap the tempo. Shift + ▸: the metronome on/off.";
const GRID_LONG: &str = "Grid: the grid editor of the step sequencer on the pads (Play and Drum view): pads 1-8 the resolution, 13/14 the page of the clip.";
const CONTROL_LONG: &str = "Control: add an instrument track; hold the transport's ◂ (Stop) and press Control: an audio track.";
const STEP_LONG: &str = "Step: the step sequencer on the pads (Play and Drum view): the 16 steps of the last played note / the selected drum pad.\nGrid: its resolution and page. Shift + Select: the edit note mode (the knobs edit the pressed steps).";
const BROWSE_LONG: &str = "Browse: the browser for the selected device (replace it), without a device: add one to the selected track.\nShift + Browse: insert a device before the selected one; Select + Browse: after it.\nWhile browsing: the knobs select in the filter columns and the results, the encoder in the selected column (◂ ▸), its push confirms, Browse discards.\nClip view: hold Browse + pad: browse for a clip.";
const SAMPLING_LONG: &str = "Sampling: slice the selected audio clip to a drum machine. Shift + Sampling: to a sampler (multi-sample).";
const AUTO_LONG: &str = "Auto: arranger automation writing on/off. Shift + Auto: clip launcher automation writing on/off (the setting flips them).\nSelect + Auto: the next automation write mode.";
const VOLUME_LONG: &str = "Volume: the knobs control the volume of 8 tracks, press again: the panorama, then the sends 1-8; the encoder controls the selected track.\nShift + Volume: the VU meters in the volume mode on/off.\nIn the edit note mode it switches the encoder's note attribute velocity / gain (on the Mk2 the knobs edit the attributes).";
const SWING_LONG: &str = "Swing: the knobs and the encoder change the play position; press again: the loop start, then the loop length. Shift + Swing: the other way.\nIn the edit note mode: the encoder's attribute panorama / duration (on the Mk2 the knobs edit the attributes).";
const TEMPO_LONG: &str = "Tempo: the knobs and the encoder change the tempo (Shift: fine).\nIn the edit note mode: the encoder's attribute pitch / pressure (on the Mk2 the knobs edit the attributes).";
const REPEAT_LONG: &str = "Note Repeat: note repeat on/off. Long press or Shift + Note Repeat: the configuration on the knobs (period, length, arpeggiator mode); press again to close it.\nSelect + Note Repeat: fill mode on/off.";
const ENTER_LONG: &str = "Enter: arranger overdub on/off. Shift + Enter: clip launcher overdub on/off. Select + Enter: clip launcher automation writing on/off.";
const ARROWS_LONG: &str = "The arrows under the encoder select the item of the active mode the encoder controls (the track, the parameter, the setting); with Shift the page (8 tracks, the parameter page, the browser tab).";
const PUSH_LONG: &str = "Push the encoder: the encoder controls the last touched parameter of the DAW (on/off); while browsing: confirm the selection and close the browser.";
const ENCODER_LONG: &str = "Turn: the selected item of the active mode (the selected track's volume / panorama / send, the selected parameter, the tempo, ...); Shift: fine.\nHold SCENE / PATTERN: the page of scenes / clips; hold SOLO / MUTE: the page of 8 tracks.\nPush: the last touched parameter of the DAW instead. Erase + touch: reset.";
const SCENE_LONG: &str = "Scene: the 16 scenes of the page on the pads. Hold Scene and press a pad: stop all clips (Shift: the alternative stop). Hold Scene and turn the encoder: the page of scenes.";
const PATTERN_LONG: &str = "Pattern: the 16 clips of the selected track on the pads. Hold Pattern and press a pad: stop the track's clip. Hold Pattern and turn the encoder: the page of clips.";
const PAD_MODE_LONG: &str = "Pad Mode: the Drum view, press again: the Play view, again: the chords, again: the Drum view.\nShift + Pad Mode: the settings of the view on the knobs (Drum: the drum pads' offset; Play: the scale, base, layout, in key, octave); Pad Mode or Shift + Pad Mode closes them.";
const NAVIGATE_LONG: &str = "Navigate: the device parameters on the pads: pads 1-8 select the parameter of the encoder, 13/14 the device, 15/16 the parameter page.";
const SOLO_LONG: &str = "Hold Solo and press a group button: solo the track; turn the encoder: the page of 8 tracks.\nShift + Solo: clear the solo of all tracks.";
const MUTE_LONG: &str = "Hold Mute and press a group button: mute the track; turn the encoder: the page of 8 tracks. Step sequencer: Mute + step mutes its note; Drum view: Mute + drum pad.\nShift + Mute: clear the mute of all tracks.";
const SCENE_VIEW_LONG: &str = "Press: launch the scene. Shift + pad: the alternative launch. Select + pad: select the scene. Erase / Duplicate + pad: delete / duplicate it.\nHold Scene + pad: stop all clips. Hold Scene + encoder: the page of 16 scenes.";
const CLIP_VIEW_LONG: &str = "Press: launch the clip (an empty slot of an armed track: record). Shift + pad: the alternative launch. Select + pad: select the clip.\nErase / Duplicate + pad: delete / duplicate the clip. Hold Pattern + pad: stop the track. Browse + pad: browse for a clip. Hold Pattern + encoder: the page of 16 clips.";
const PLAY_VIEW_LONG: &str = "The notes of the scale (root notes in the track colour); PAD MODE again: the chords (two thirds on top of the played note).\nStep (the 16 steps of the last played note): press a step to set / remove a note, hold a step and press a later one: the length; Mute + step: mute; Duplicate + step: copy / paste; Shift + Select: edit the notes on the knobs.\nGrid: the resolution (pads 1-8) and the page (13/14). Shift + Pad Mode: the scale settings on the knobs.";
const DRUM_VIEW_LONG: &str = "The 16 drum pads (play, Select + pad: select it without a note; Erase / Mute / Solo / Browse + pad).\nStep: the 16 steps of the selected drum pad; hold a step and press a later one: the length; Select + step: its note repeat +1; Mute + step: mute; Duplicate + step: copy / paste.\nGrid: the resolution (pads 1-8) and the page (13/14). Shift + Pad Mode: the drum pads' offset on the knobs.";
const DEVICE_VIEW_LONG: &str = "Pads 1-8: select the parameter the encoder controls (in the parameter colours, the selected one blinks); 13/14: the previous / next device; 15/16: the previous / next parameter page.";
const NOTE_REPEAT_VIEW_LONG: &str = "The note repeat settings on the pads: 7/8 the arpeggiator mode, 9/10 the note length, 13/14 the period (the Mk2 uses the knobs, see Note Repeat).";
const SHIFT_VIEW_LONG: &str = "The Shift view (tap Shift): 1 undo, 2 redo, 5 quantize the selected clip 100%, 6 50%, 9 delete all its notes, 13/14 transpose it a semitone down / up, 15/16 an octave down / up.";
const BROWSER_LONG: &str = "The knobs 1-7: the selection in the filter columns of the browser; 8: the results. The encoder: the selected column (◂ ▸), Shift + ◂ ▸: the tab.\nPush the encoder: confirm and close; Browse: discard and close.";
const KNOBS_LONG: &str = "Turn a knob: its value (Shift: fine). Erase + touch a knob: reset it.\nThe buttons above the displays select what the knobs control: Volume, Pan, Send − / +, User, Params; Volume, Swing, Tempo too.";

// ------------------------------------------------------------------------------------------
// The state dependent texts

fn loop_text(ctx: &Ctx) -> String {
    format!("Loop: turn it {}", on_off(!ctx.model.transport().is_loop()))
}

/// The transport's ◂: Stop, or the emulated Shift with NI's driver.
fn stop_button(ctx: &Ctx) -> &'static str {
    if !has_shift(ctx) {
        return "Hold: Shift (NI's driver doesn't send it); release unused: stop";
    }
    if ctx.model.transport().is_playing() {
        "Stop playback (double click: to the end)"
    } else {
        "Stopped: back to the start (double click: to the end)"
    }
}

fn is_play_or_drum(ctx: &Ctx) -> bool {
    ctx.views.is_any_active(&[Views::Drum, Views::Play])
}

fn sequencer_visible(ctx: &Ctx) -> bool {
    drum_view_state(ctx, |v| v.is_sequencer_visible()).unwrap_or(false)
}

fn grid_editor(ctx: &Ctx) -> bool {
    drum_view_state(ctx, |v| v.is_grid_editor()).unwrap_or(false)
}

/// The pads show the steps (not the grid editor).
fn step_editor(ctx: &Ctx) -> bool {
    sequencer_visible(ctx) && !grid_editor(ctx)
}

fn is_note_editing(ctx: &Ctx) -> bool {
    ctx.modes.is_active(Modes::Note) && is_play_or_drum(ctx)
}

fn last_param(ctx: &Ctx) -> bool {
    ext(ctx).control_last_param_active
}

/// DUPLICATE in the Play and Drum view: the steps of the sequencer, else the tracks only.
fn duplicate_in_sequencer(ctx: &Ctx) -> &'static str {
    if step_editor(ctx) {
        "Hold + step: copy a note, then paste it to empty steps; + group button: the track"
    } else {
        "Hold + group button: duplicate the track"
    }
}

/// GRID: the grid editor in the Play and Drum view, nothing elsewhere.
fn grid_button(ctx: &Ctx) -> Option<String> {
    is_play_or_drum(ctx).then(|| format!("Grid editor (resolution, page) on the pads: turn it {}", on_off(!grid_editor(ctx))))
}

fn step_button(ctx: &Ctx) -> String {
    let state = on_off(!sequencer_visible(ctx));
    if is_play_or_drum(ctx) {
        format!("Step sequencer on the pads: turn it {state}")
    } else {
        format!("Step sequencer of the Play / Drum view: turn it {state}")
    }
}

fn browse_button(ctx: &Ctx) -> &'static str {
    if browsing(ctx) {
        "Close the browser (discard the selection; the encoder push confirms)"
    } else if ctx.model.cursor_device().does_exist() {
        "Browser: replace the selected device"
    } else {
        "Browser: add a device to the selected track"
    }
}

fn slice(ctx: &Ctx, text: &str) -> String {
    if ctx.model.can_convert_clip() {
        text.to_string()
    } else {
        format!("{text} (no audio clip selected)")
    }
}

fn volume_vu(ctx: &Ctx) -> bool {
    ctx.modes.get(Modes::Volume).and_then(|m| m.try_borrow().ok().and_then(|m| m.as_any().downcast_ref::<MaschineVolumeMode>().map(|m| m.is_display_vu()))).unwrap_or(false)
}

/// The name of a mode for "the knobs to ...".
fn mode_text(mode: Modes) -> String {
    match mode {
        Modes::Volume => "the volume of 8 tracks".to_string(),
        Modes::Pan => "the panorama of 8 tracks".to_string(),
        Modes::Position => "the play position".to_string(),
        Modes::LoopStart => "the loop start".to_string(),
        Modes::LoopLength => "the loop length".to_string(),
        mode if is_send_mode(mode) => format!("send {} of 8 tracks", mode.ordinal() - Modes::Send1.ordinal() + 1),
        mode => mode.name().to_string(),
    }
}

/// VOLUME, SWING: the next mode of a multi select command if one of its modes is active (else
/// the command's last selected, which only the command knows).
fn multi_select(ctx: &Ctx, modes: &[Modes], fallback: &str) -> String {
    match ctx.modes.active_id().and_then(|active| modes.iter().position(|m| *m == active)) {
        Some(index) => format!("Press: the knobs to {}", mode_text(modes[(index + 1) % modes.len()])),
        None => format!("Press: the knobs to {fallback}"),
    }
}

const VOLUME_MODES: [Modes; 10] =
    [Modes::Volume, Modes::Pan, Modes::Send1, Modes::Send2, Modes::Send3, Modes::Send4, Modes::Send5, Modes::Send6, Modes::Send7, Modes::Send8];

fn volume_button(ctx: &Ctx) -> String {
    if is_note_editing(ctx) {
        return "Edit note: the encoder's attribute velocity / gain (the knobs edit them)".to_string();
    }
    multi_select(ctx, &VOLUME_MODES, "the volume / panorama / sends (the last used)")
}

fn swing_button(ctx: &Ctx) -> String {
    if is_note_editing(ctx) {
        return "Edit note: the encoder's attribute panorama / duration (the knobs edit them)".to_string();
    }
    multi_select(ctx, &[Modes::Position, Modes::LoopStart, Modes::LoopLength], "the play position / loop start / length (the last used)")
}

fn tempo_button(ctx: &Ctx) -> String {
    if is_note_editing(ctx) {
        return "Edit note: the encoder's attribute pitch / pressure (the knobs edit them)".to_string();
    }
    if ctx.modes.is_active(Modes::Tempo) {
        return format!("Active: the knobs and the encoder change the tempo (now {:.2})", ctx.model.transport().tempo());
    }
    format!("Press: the knobs and the encoder to the tempo (now {:.2})", ctx.model.transport().tempo())
}

fn encoder_push(ctx: &Ctx) -> String {
    if ctx.modes.is_active(Modes::Browser) {
        return "Confirm the browser selection and close the browser".to_string();
    }
    format!("The encoder controls the last touched parameter: turn it {}", on_off(!last_param(ctx)))
}

fn selected_track_name(ctx: &Ctx) -> Option<String> {
    ctx.model.current_track_bank().selected_item().map(|t| t.name().to_string())
}

/// The encoder: the selected item of the active mode.
fn encoder(ctx: &Ctx) -> String {
    if last_param(ctx) {
        return "Turn: the last touched parameter of the DAW (push: back to the mode)".to_string();
    }
    let track = |what: &str| match selected_track_name(ctx) {
        Some(name) => format!("Turn: the {what} of the selected track ({name})"),
        None => format!("Turn: the {what} of the selected track (none selected)"),
    };
    match ctx.modes.active_id() {
        Some(Modes::Volume) => track("volume"),
        Some(Modes::Pan) => track("panorama"),
        Some(mode) if is_send_mode(mode) => track(&format!("send {}", mode.ordinal() - Modes::Send1.ordinal() + 1)),
        Some(Modes::DeviceParams) => {
            let index = mode_state(ctx, Modes::DeviceParams, |m: &MaschineParametersMode| m.selected_parameter()).unwrap_or(0);
            let name = ctx.model.cursor_device().parameter_bank().map(|b| b.item(index)).filter(|p| p.does_exist()).map(|p| p.name().to_string());
            format!("Turn: the selected parameter {}{} (◂ ▸ select)", index + 1, name.map(|n| format!(" ({n})")).unwrap_or_default())
        }
        Some(Modes::User) => {
            let index = mode_state(ctx, Modes::User, |m: &MaschineUserMode| m.selected_parameter()).unwrap_or(0);
            format!("Turn: the selected parameter {} (◂ ▸ select)", index + 1)
        }
        Some(Modes::Browser) => {
            let column = mode_state(ctx, Modes::Browser, |m: &BrowseMode| m.selected_parameter()).unwrap_or(0);
            if column >= 7 {
                "Turn: browse the results (◂ ▸: the column)".to_string()
            } else {
                format!("Turn: the selection of filter column {} (◂ ▸: the column)", column + 1)
            }
        }
        Some(Modes::Tempo) => format!("Turn: the tempo (now {:.2}; Shift: fine)", ctx.model.transport().tempo()),
        Some(Modes::Position) => "Turn: the play position (Shift: fine)".to_string(),
        Some(Modes::LoopStart) => "Turn: the loop start (Shift: fine)".to_string(),
        Some(Modes::LoopLength) => "Turn: the loop length (Shift: fine)".to_string(),
        Some(Modes::Note) => "Turn: the selected note attribute (◂ ▸ select)".to_string(),
        Some(Modes::PlayOptions) => "Turn: the drum pads' offset (16 up / down)".to_string(),
        Some(_) => "Turn: the selected setting (◂ ▸ select)".to_string(),
        None => "Turn: nothing (no mode)".to_string(),
    }
}

/// Read the state of a mode.
fn mode_state<M: 'static, R>(ctx: &Ctx, mode: Modes, f: impl FnOnce(&M) -> R) -> Option<R> {
    let mode = ctx.modes.get(mode)?;
    let mode = mode.try_borrow().ok()?;
    mode.as_any().downcast_ref::<M>().map(f)
}

fn is_project_mode(ctx: &Ctx) -> bool {
    mode_state(ctx, Modes::User, |m: &MaschineUserMode| m.is_project_mode()).unwrap_or(true)
}

fn note_page(ctx: &Ctx) -> &'static str {
    mode_state(ctx, Modes::Note, |m: &EditNoteMode| m.page_name()).unwrap_or("Note")
}

/// The buttons above the displays: the modes of the knobs (one box).
fn display_buttons(ctx: &Ctx) -> Help {
    let active = ctx.modes.active_id();
    let is_device = active == Some(Modes::DeviceParams);
    let what = if is_device { "device" } else { "track" };
    let short = format!("Knobs: Volume, Pan, Send − / +, User, Params; Pin, Active: the {what}");
    let (pinned, enabled) = if is_device {
        (ctx.model.cursor_device().is_pinned(), ctx.model.cursor_device().is_enabled())
    } else {
        let track = ctx.model.cursor_track();
        (ctx.model.is_cursor_track_pinned(), track.does_exist() && track.is_activated())
    };
    let active_name = active.map(|m| m.name().to_string()).unwrap_or_default();
    let long = lines(&[
        &format!("The knobs now: {active_name}."),
        "1 Volume: the volume of 8 tracks. 2 Pan: the panorama. 3 Send −, 4 Send +: the previous / next send (1-8).",
        &format!("5 Pin: pin the selected {what} (now {}). 6 Active: the selected {what} on / off (now {}).", on_off(pinned), on_off(enabled)),
        "7 User Params: the project / track parameters. 8 Parameters: the device parameters.",
        "With Shift: Send − and Send + select the previous send.",
    ]);
    Help::new(short).with_long(long)
}

/// SCENE, PATTERN, NAVIGATE: a view on the pads.
fn view_button(ctx: &Ctx, view: Views, text: &str) -> String {
    if ctx.views.is_active(view) {
        format!("Active · pads: {text}")
    } else {
        format!("Press · pads: {text}")
    }
}

/// PAD MODE (`MaschineMk2PlayViewSwitchCommand`): the next of Drum, Play, chords.
fn pad_mode(ctx: &Ctx) -> String {
    if ctx.modes.is_any_active(&[Modes::PlayOptions, Modes::Scales]) {
        return "Close the settings of the view".to_string();
    }
    let chords = play_view_state(ctx, |v| v.is_chord_mode()).unwrap_or(false);
    match ctx.views.active_id() {
        Some(Views::Drum) => "Press: the Play view (then the chords, then Drum)".to_string(),
        Some(Views::Play) if chords => "Press: the Drum view (then Play, then the chords)".to_string(),
        Some(Views::Play) => "Press: the chords in the Play view (then Drum)".to_string(),
        _ => "Press: the Drum view (again: the Play view, the chords)".to_string(),
    }
}

/// Shift + PAD MODE: the settings of the Drum or Play view on the knobs.
struct ViewSettings(Option<&'static str>);

impl IntoRes for ViewSettings {
    fn into_res(self) -> Res {
        match self.0 {
            Some(text) => text.into_res(),
            None => NoHelp.into_res(),
        }
    }
}

fn pad_mode_shifted(ctx: &Ctx) -> ViewSettings {
    ViewSettings(match ctx.views.active_id_ignore_temporary() {
        Some(Views::Drum) => Some("The drum settings on the knobs (the pads' offset)"),
        Some(Views::Play) => Some("The scale settings on the knobs (scale, base, layout, octave)"),
        _ => None,
    })
}

/// The group buttons: select the tracks (their names in the long text).
fn group_row(ctx: &Ctx) -> Help {
    Help::new("Select the track (a selected group track: expand / collapse)").with_long(lines(&[
        &group_track_names(ctx, 8),
        "Hold Rec / Solo / Mute / Erase / Duplicate and press A-H: record arm / solo / mute / delete / duplicate the track.",
    ]))
}

fn clip_grid(ctx: &Ctx) -> String {
    match selected_track_name(ctx) {
        Some(name) => format!("Launch / record the 16 clips of {name}"),
        None => "Launch the clips of the selected track (none selected)".to_string(),
    }
}

fn grid_editor_text(ctx: &Ctx) -> String {
    let resolution = drum_view_state(ctx, |v| v.resolution_index(ctx)).unwrap_or(0);
    format!("Grid editor: pads 1-8 the resolution (now {}), 13/14 the page", Resolution::name_at(resolution))
}

fn play_grid(ctx: &Ctx) -> String {
    if grid_editor(ctx) {
        return grid_editor_text(ctx);
    }
    if sequencer_visible(ctx) {
        return "The 16 steps of the last played note (hold one + a later one: length)".to_string();
    }
    let scales = ctx.model.scales();
    let chords = if play_view_state(ctx, |v| v.is_chord_mode()).unwrap_or(false) { "Chords in" } else { "Play" };
    format!("{chords} {} {} (layout {})", Scales::BASES[scales.scale_offset_index()], scales.scale().name(), scales.scale_layout().name())
}

fn drum_grid(ctx: &Ctx) -> String {
    if grid_editor(ctx) {
        return grid_editor_text(ctx);
    }
    let pad = drum_view_state(ctx, |v| v.base.selected_pad()).unwrap_or(0);
    if sequencer_visible(ctx) {
        return format!("The 16 steps of drum pad {} (hold one + a later one: length)", pad + 1);
    }
    format!("Play the 16 drum pads (selected: pad {})", pad + 1)
}

fn device_grid(ctx: &Ctx) -> String {
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return "1-8: the parameter of the encoder; 13/14: device; 15/16: page (no device)".to_string();
    }
    format!("{}: 1-8 the encoder's parameter; 13/14 device; 15/16 page", device.name())
}

/// The knobs of a mode: its text and how the knobs change.
fn knobs(_ctx: &Ctx, short: String) -> Help {
    let long = format!("{short}.\n{KNOBS_LONG}");
    Help::new(short).with_long(long)
}

/// The knobs of the track modes: the names of the 8 tracks in the long text.
fn track_knobs(ctx: &Ctx, short: &str) -> Help {
    let bank = ctx.model.current_track_bank();
    let names: Vec<String> = (0..8)
        .filter_map(|i| {
            let track = bank.item(i);
            track.does_exist().then(|| format!("{}: {}", i + 1, track.name()))
        })
        .collect();
    let long = format!("{short}: {}.\nThe encoder: the selected track (◂ ▸ select it, Shift + ◂ ▸: the 8 tracks).\n{KNOBS_LONG}", names.join(", "));
    Help::new(short).with_long(long)
}

fn send_knobs(ctx: &Ctx) -> Help {
    let index = ctx.modes.active_id().map(|m| m.ordinal() - Modes::Send1.ordinal()).unwrap_or(0);
    let bank = ctx.model.current_track_bank();
    let track = bank.selected_item().or_else(|| Some(bank.item(0)).filter(|t| t.does_exist()));
    let name = track.map(|t| t.send_bank().item(index)).filter(|s| s.does_exist()).map(|s| format!(" ({})", s.name())).unwrap_or_default();
    track_knobs(ctx, &format!("Send {}{name} of 8 tracks", index + 1))
}

fn device_knobs(ctx: &Ctx) -> Help {
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return knobs(ctx, "The device parameters (no device selected)".to_string());
    }
    let page = device.parameter_bank().map(|bank| bank.page_bank().selected_item()).filter(|p| !p.trim().is_empty());
    let short = format!("The parameters of {}{}", device.name(), page.map(|p| format!(", page {p}")).unwrap_or_default());
    let names: Vec<String> = (0..8)
        .filter_map(|i| device.parameter_bank().map(|b| b.item(i)).filter(|p| p.does_exist()).map(|p| format!("{}: {}", i + 1, p.name())))
        .collect();
    let long = format!("{short}: {}.\nThe encoder: the selected parameter (◂ ▸), Shift + ◂ ▸: the page.\n{KNOBS_LONG}", names.join(", "));
    Help::new(short).with_long(long)
}

fn note_repeat_knobs(ctx: &Ctx) -> Help {
    let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
    let period = Resolution::name_at(Resolution::get_match(note_repeat.period));
    let length = Resolution::name_at(Resolution::get_match(note_repeat.note_length));
    knobs(ctx, format!("1-2: the period ({period}), 3-4: the length ({length}), 5-6: the arpeggiator mode"))
}

fn scale_knobs(ctx: &Ctx) -> Help {
    let scales = ctx.model.scales();
    let short = format!("1-3 scale ({}), 4 base ({}), 5-6 layout, 7 in key, 8 octave", scales.scale().name(), Scales::BASES[scales.scale_offset_index()]);
    knobs(ctx, short)
}

fn note_knobs(ctx: &Ctx) -> Help {
    let page = note_page(ctx);
    let short = match page {
        "Expressions" => "2-3: length, 4 gain, 5 panorama, 6 pitch, 7 timbre, 8 pressure",
        "Repeat" => "2-3: length, 4 velocity, 5 repeat count, 6 curve, 7 velocity curve, 8 end",
        _ => "2-3: length, 4 velocity, 5 spread, 6 release velocity, 7 chance, 8 occurrence",
    };
    let long = format!(
        "Edit the selected notes (press the steps in the step sequencer), the page {page}: {short}.\nShift + ◂ ▸: the pages Note, Expressions, Repeat (with the host's step grid). Erase + touch a knob: reset."
    );
    Help::new(short).with_long(long)
}
