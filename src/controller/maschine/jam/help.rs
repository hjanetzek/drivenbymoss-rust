// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The help texts of the Maschine JAM (not part of Java, doc/CONTROLLER_HELP_PLAN.md): one table
//! ([`crate::help!`], `framework/help_table.rs`) for the buttons, the pads and the strips, by the
//! held modifiers, the active mode and view. Below it the functions of the state dependent texts.
//!
//! Escape hatch: LEVEL, AUX and CONTROL (`command`) ask their commands, which know the mode they
//! select next (`MaschineJam*Command::help`, [`level_button`], [`aux_button`],
//! [`control_button`]).
//!
//! The texts follow the code (where it differs from the manual, the code wins; the differences
//! are listed in doc/CONTROLLER_HELP_PLAN.md §7).

use crate::config::maschine_jam::MaschineJamConfiguration;
use crate::config::values::{TransportBehavior, NEW_CLIP_LENGTH_VALUES};
use crate::controller::maschine::core::encoder_mode_manager::{EncoderMode, EncoderTarget};
use crate::daw::application::PANEL_LAYOUT_ARRANGE;
use crate::daw::bank::BankItem;
use crate::daw::constants::{GrooveParameterId, Resolution};
use crate::framework::commands::mode::is_send_mode;
use crate::framework::ctx::Ctx;
use crate::framework::help::{help, help_long, lines, Help};
use crate::framework::help_table::{HelpKey, IntoRes, NoHelp, Res};
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::scale::Scales;

use super::setup::{PLAY_VIEWS, SEQUENCER_VIEWS};
use super::surface as jam;
use super::surface_link;

crate::help! {
    global {
        // Modifiers
        Shift => |ctx| shift_button(ctx), long SHIFT_LONG,
        Shift + Select => |ctx| {
            let slow = MaschineJamConfiguration::is_slow_fader_change(&ctx.config);
            format!("Slow fader change: turn it {} (now {})", on_off(!slow), on_off(slow))
        },
        Select + Select => "Held: the buttons do their Select function (see their boxes)",
        Select => "Hold: the Select functions (select without launching, add tracks, ...)" long SELECT_LONG,
        Delete + Delete => "Held: press a pad, scene or group button to delete it",
        Delete => "Hold + scene button: delete the scene; + group button: delete the track" long CLEAR_LONG,
        Shift + Duplicate => "Double the content of the selected clip",
        Duplicate + Duplicate => "Held: press a group button to duplicate the track",
        Duplicate => "Hold + group button: duplicate the track" long DUPLICATE_LONG,

        // Transport
        Select + Shift + Play => |ctx| format!("Punch out: turn it {}", on_off(!ctx.model.transport().is_punch_out_enabled())),
        Shift + Play => "Restart playback from the beginning",
        Select + Play => |ctx| format!("Punch in: turn it {}", on_off(!ctx.model.transport().is_punch_in_enabled())),
        Play => |ctx| play_button(ctx), long PLAY_LONG,
        Shift + Record => |ctx| ctx.config.shifted_record_button_function().name(),
        Record + Record => "Held: press a group button to record arm its track",
        Record => |ctx| ctx.config.record_button_function().name(), long RECORD_LONG,
        Shift + PageLeft => |ctx| format!("Metronome: turn it {}", on_off(!ctx.model.transport().is_metronome_on())),
        Shift + PageRight => |ctx| format!("Loop: turn it {}", on_off(!ctx.model.transport().is_loop())),
        PageLeft | PageRight => none, long PAGE_LONG,
        Shift + Solo => |ctx| format!("Next length for new clips (now {})", NEW_CLIP_LENGTH_VALUES[ctx.config.new_clip_length()]),
        Select + Solo => "Clear solo on all tracks",
        Solo + Solo => "Held: press a group button to solo its track",
        Solo => "Hold + group button: solo the track" long SOLO_LONG,
        Shift + Mute => "Stop all playing clips (playback goes on)",
        Select + Mute => "Clear mute on all tracks",
        Mute + Mute => "Held: press a group button to mute its track",
        Mute => "Hold + group button: mute the track" long MUTE_LONG,
        Select + Repeat => |ctx| format!("Fill mode: turn it {}", on_off(!ctx.model.transport().is_fill_mode_active())),
        Shift + Repeat => "Open the note repeat configuration on the pads",
        Repeat => |ctx| format!("Note repeat: turn it {} (long press: configuration)", on_off(!ctx.config.is_note_repeat_active())), long REPEAT_LONG,
        Select + Automation => |ctx| format!("Next automation write mode (now {})", ctx.model.transport().automation_write_mode().label()),
        Shift + Automation => |ctx| automation(ctx, !ctx.config.is_flip_record()),
        Automation => |ctx| automation(ctx, ctx.config.is_flip_record()), long AUTO_LONG,

        // Strip modes (LEVEL, AUX, CONTROL: the commands know which mode comes next)
        Shift + Track => none,
        Select + Track => "Add an instrument track",
        Track => |ctx| if ctx.modes.is_active(Modes::Track) {
            "Active: the strips control the selected track's volume, panorama, sends"
        } else {
            "Press: the strips to the selected track's volume, panorama, sends"
        }, long MACRO_LONG,
        Shift + Volume => command,
        Select + Volume => "Add an audio track",
        Volume => command, long LEVEL_LONG,
        Shift + Sends => |ctx| format!("Show the {}", if ctx.model.is_effect_track_bank_active() { "audio and instrument tracks" } else { "effect tracks" }),
        Select + Sends => "Add an effect track",
        Sends + Sends => "Held: a group button A-H selects that send",
        Sends => command, long AUX_LONG,
        Select + Device => |ctx| if !surface_link::has_host_pages(ctx) {
            "Toggle project / track parameters"
        } else if surface_link::is_active(ctx) {
            "Page surface: the next source, or off"
        } else {
            "Page surface on (the host's pages, then the next sources)"
        },
        Shift + Device => |ctx| if surface_link::has_host_pages(ctx) {
            "Strips: the host's first remote control page"
        } else {
            "Open / close the window of the selected device"
        },
        Device => command,

        // The group buttons and the scenes: one box per row
        Sends + RowSelect1 .. RowSelect8 => |ctx| send_row(ctx),
        Groove + RowSelect1 .. RowSelect8 => |ctx| resolution_row(ctx),
        Solo + RowSelect1 .. RowSelect8 => "Solo / unsolo the track",
        Mute + RowSelect1 .. RowSelect8 => "Mute / unmute the track",
        Record + RowSelect1 .. RowSelect8 => "Record arm / disarm the track",
        Delete + RowSelect1 .. RowSelect8 => "Delete the track",
        Duplicate + RowSelect1 .. RowSelect8 => "Duplicate the track",
        RowSelect1 .. RowSelect8 => |ctx| track_row(ctx),
        Shift + Scene1 .. Scene8 => |ctx| scene_row(ctx, "Launch the scene (alternative launch)"),
        Select + Scene1 .. Scene8 => |ctx| scene_row(ctx, "Select the scene (no launch)"),
        Sequencer + Scene1 .. Scene8 => |ctx| view_row(ctx, &SCENE_SEQUENCER_VIEWS, "1-6: the sequencers"),
        Note + Scene1 .. Scene8 => |ctx| view_row(ctx, &SCENE_PLAY_VIEWS, "1-4: the play views"),
        Delete + Scene1 .. Scene8 => |ctx| scene_row(ctx, "Delete the scene"),
        Scene1 .. Scene8 => |ctx| scene_row(ctx, "Launch the scene"),

        // Views
        Select + Session => |ctx| if surface_link::is_active(ctx) {
            "Page surface: the next source, or off"
        } else {
            "Page surface on (the pages of the Electra One / the host)"
        },
        Shift + Session => |ctx| format!("Show the {} panel", if ctx.model.application().panel_layout() == PANEL_LAYOUT_ARRANGE { "mix" } else { "arrange" }),
        Session => "Session view: the clips on the pads (long press: birds-eye view)" long SONG_LONG,
        Sequencer + Sequencer => "Held: scene 1-6 select a sequencer view",
        Shift + Sequencer => none,
        Sequencer => |ctx| format!("Press: the {} view (hold + scene 1-6: pick one)", view_name(ctx, next_view(ctx, &SEQUENCER_VIEWS))), long STEP_LONG,
        Note + Note => "Held: scene 1-4 select a play view",
        Shift + Note => none,
        Note => |ctx| format!("Press: the {} view (hold + scene 1-4: pick one)", view_name(ctx, next_view(ctx, &PLAY_VIEWS))), long PAD_MODE_LONG,
        ArrowLeft | ArrowRight | ArrowUp | ArrowDown => none,
        Shift + Browse => |ctx| (!browsing(ctx)).then_some("Browser: insert a device before the selected one"),
        Select + Browse => |ctx| (!browsing(ctx)).then_some("Browser: insert a device after the selected one"),
        Browse => |ctx| if ctx.model.cursor_device().does_exist() {
            "Browser: replace the selected device"
        } else {
            "Browser: add a device to the selected track"
        }, long BROWSE_LONG,
        Footswitch1 | Footswitch2 => none,

        // The encoder section
        Mastertrack => |ctx| monitor_button(ctx, EncoderMode::MasterVolume), long MONITOR_LONG,
        Alt => |ctx| monitor_button(ctx, EncoderMode::PlayPosition), long MONITOR_LONG,
        Metronome => |ctx| monitor_button(ctx, EncoderMode::MetronomeVolume), long MONITOR_LONG,
        Mixer => |ctx| monitor_button(ctx, EncoderMode::CueVolume), long MONITOR_LONG,
        Enter => |ctx| encoder_push(ctx), long PUSH_LONG,
        Shift + MasterKnob => |ctx| encoder(ctx),
        Select + MasterKnob => |ctx| select_changes_encoder(ctx).then(|| encoder(ctx)).flatten(),
        MasterKnob => |ctx| encoder(ctx), long ENCODER_LONG,
        Shift + TapTempo => "Tap the tempo",
        TapTempo => |ctx| format!("Hold + encoder: the tempo (now {:.2})", ctx.model.transport().tempo()), long TEMPO_LONG,
        Shift + Accent => "Toggle the shuffle rate",
        Select + Accent => |ctx| format!("Groove: turn it {}", on_off(ctx.model.groove().parameter(GrooveParameterId::Enabled).value() == 0)),
        Accent => "Hold + encoder: the shuffle amount" long SWING_LONG,
        Shift + Groove => "Next record quantization",
        Select + Groove => "Previous record quantization",
        Groove + Groove => "Held: A-H select the sequencer resolution",
        Groove => "Quantize the selected clip (hold + A-H: sequencer resolution)" long GRID_LONG,
        Shift + Row11 | Row12 | Row13 | Row14 => none,
        // Perform, Notes, Lock, Tune: the options of the view (see the views below)
        Row11 | Row12 | Row14 => none, long OPTION_LONG,
        Row13 => |ctx| view_option_button(ctx, EncoderMode::TemporaryLock), long OPTION_LONG,
    }

    // ------------------------------------------------------------------------------------------
    // The views: the pads and the arrows

    view Play | Piano {
        Row11 => |ctx| view_option_button(ctx, EncoderMode::TemporaryPerform),
        Row12 => |ctx| view_option_button(ctx, EncoderMode::TemporaryNotes),
        Row13 => |ctx| view_option_button(ctx, EncoderMode::TemporaryLock),
        Row14 => |ctx| view_option_button(ctx, EncoderMode::TemporaryTune),
        Delete + grid => "Delete the notes of the pad's note in the clip",
        grid => |ctx| grid_play(ctx), long PLAY_VIEW_LONG,
        Delete => "Hold + pad: delete that note in the clip; + scene / group: delete",
        ArrowLeft => |ctx| format!("The previous scale layout (now {})", ctx.model.scales().scale_layout().name()),
        ArrowRight => |ctx| format!("The next scale layout (now {})", ctx.model.scales().scale_layout().name()),
        ArrowUp => |ctx| if ctx.views.is_active(Views::Piano) { "The piano an octave up" } else { "The notes an octave up" },
        ArrowDown => |ctx| if ctx.views.is_active(Views::Piano) { "The piano an octave down" } else { "The notes an octave down" },
    }
    view Chords {
        Row11 => |ctx| view_option_button(ctx, EncoderMode::TemporaryPerform),
        Row12 => |ctx| view_option_button(ctx, EncoderMode::TemporaryNotes),
        Row13 => |ctx| view_option_button(ctx, EncoderMode::TemporaryLock),
        Row14 => |ctx| view_option_button(ctx, EncoderMode::TemporaryTune),
        Delete + grid => "Delete the notes of the pad's note in the clip",
        grid => "A chord per pad; rows from the bottom: triad, dyad, sus2, sus4, 6, maj7, 9, 11" long CHORDS_LONG,
        Delete => "Hold + pad: delete that note in the clip; + scene / group: delete",
        ArrowUp => "The notes an octave up",
        ArrowDown => "The notes an octave down",
    }
    view Drum64 {
        Delete + grid => "Delete the notes of the pad in the clip",
        grid => "Play 64 drum pads (bottom left 16: as in the drum sequencer)" long DRUM64_LONG,
        Delete => "Hold + drum pad: delete its notes; + scene / group button: delete",
        ArrowUp => "The drum pads 16 up",
        ArrowDown => "The drum pads 16 down",
    }
    view Drum {
        Row14 => |ctx| view_option_button(ctx, EncoderMode::TemporaryTune),
        Select + grid => "Select + step: raise its note repeat by 1",
        Delete + grid => "Delete the notes of the drum pad in the clip",
        grid => "Bottom left: drum pads; bottom right: pages / loop; top 4 rows: 32 steps" long DRUM_LONG,
        Delete => "Hold + drum pad: delete its notes; + scene / group button: delete",
        ArrowLeft => "The previous page of the clip",
        ArrowRight => "The next page of the clip",
        ArrowUp => "The drum pads 16 up",
        ArrowDown => "The drum pads 16 down",
    }
    view Drum4 | Drum8 {
        Row14 => |ctx| view_option_button(ctx, EncoderMode::TemporaryTune),
        Select + grid => "Select + step: raise its note repeat by 1",
        grid => |ctx| if ctx.views.is_active(Views::Drum4) {
            "Each 2 rows are the 16 steps of one of 4 drum sounds"
        } else {
            "Each row is the 8 steps of one of 8 drum sounds"
        }, long DRUM_LANES_LONG,
        Delete => "Hold + drum pad: delete its notes; + scene / group button: delete",
        ArrowLeft => "The previous page of the clip",
        ArrowRight => "The next page of the clip",
        ArrowUp => "The drum sounds up",
        ArrowDown => "The drum sounds down",
    }
    view Sequencer {
        Row11 => |ctx| view_option_button(ctx, EncoderMode::TemporaryPerform),
        Row12 => |ctx| view_option_button(ctx, EncoderMode::TemporaryNotes),
        Row13 => |ctx| view_option_button(ctx, EncoderMode::TemporaryLock),
        Row14 => |ctx| view_option_button(ctx, EncoderMode::TemporaryTune),
        Select + grid => "Select + step: raise its note repeat by 1",
        grid => "Top row: the clip's pages (hold + another: loop); below: 8×7 steps" long SEQUENCER_LONG,
        ArrowLeft => "The previous page of the clip",
        ArrowRight => "The next page of the clip",
        ArrowUp => "The note range up",
        ArrowDown => "The note range down",
    }
    view PolySequencer {
        grid => "Bottom half: play notes; top half: a step with the notes played last" long POLY_LONG,
        ArrowLeft => "The previous page of the clip",
        ArrowRight => "The next page of the clip",
        ArrowUp => "The notes up",
        ArrowDown => "The notes down",
    }
    view Raindrops {
        grid => "Bottom row: the notes; the pads above a note start a raindrop" long RAINDROPS_LONG,
        ArrowRight | ArrowUp => "The notes up",
        ArrowLeft | ArrowDown => "The notes down",
    }
    view Session {
        Delete + grid => "Delete the clip",
        Duplicate + grid => "First a clip with content (source), then an empty slot (copy)",
        Select + Mute + grid => "The alternative stop of the track's clip",
        Mute + grid => "Stop the playing clip of the track",
        Groove + grid => "The track back to the arrangement",
        Browse + grid => "Browse for a clip",
        Select + grid => "Select the clip",
        Shift + grid => "The alternative launch of the clip",
        grid => |ctx| grid_session(ctx), long SESSION_LONG,
        Session => |ctx| if birds_eye(ctx) {
            "Leave the birds-eye view"
        } else {
            "Flip the clips' orientation (long press: the birds-eye view)"
        },
        Delete => "Hold + pad: delete the clip; + scene / group button: delete it",
        Duplicate + Duplicate => "Held: press a source clip, then an empty slot",
        Duplicate => "Hold + clip, then an empty slot: copy the clip; + group button: the track",
        Mute + Mute => "Held: a pad stops its track's clip, a group button mutes",
        Mute => "Hold + group button: mute the track; + pad: stop the track's clip",
        Groove + Groove => "Held: a pad returns its track to the arrangement",
        ArrowLeft | ArrowRight | ArrowUp | ArrowDown => |ctx, key| scroll(ctx, key, false),
    }
    view Shift {
        grid => "Top row: undo, redo, quantize 100% / 50%, semitone −/+, octave −/+" long SHIFT_VIEW_LONG,
        ArrowLeft | ArrowRight | ArrowUp | ArrowDown => |ctx, key| scroll(ctx, key, true),
    }
    view RepeatNote {
        grid => |ctx| grid_note_repeat(ctx), long NOTE_REPEAT_LONG,
        Shift + Repeat => "Close the note repeat configuration",
        Repeat => "Close the note repeat configuration",
    }
    view Tempo {
        grid => |ctx| format!("Shows the tempo ({:.2}); turn the encoder to change it", ctx.model.transport().tempo()),
    }
    view Shuffle {
        grid => "Shows the shuffle amount; turn the encoder to change it",
    }
    view Browser {
        grid => "Columns 1-6: the filter columns, 8: the results; rows 1-4: up 1, up 8, down 8, down 1" long BROWSER_LONG,
        Browse => "Close the browser (discard the selection; the encoder push confirms)",
    }
    view Control {
        grid => |ctx| format!("Bottom right 4×4: the fixed accent velocity (now {})", ctx.config.fixed_accent_value()), long ACCENT_LONG,
    }
    view SurfaceLink {
        grid => "Row 1: the pages (last 2: previous / next); rows 2-7: the cells; row 8: the sections" long SURFACE_LONG,
        ArrowUp => "Page surface: the previous row",
        ArrowDown => "Page surface: the next row",
        ArrowLeft => "Page surface: the previous control set / value of the cell",
        ArrowRight => "Page surface: the next control set / value of the cell",
    }

    // ------------------------------------------------------------------------------------------
    // The modes: the strips and PAGE LEFT / RIGHT

    mode Volume {
        strips => |ctx| strips(ctx, format!("The volume of 8 tracks{}", if ctx.model.transport().is_playing() { " (and their VU)" } else { "" })),
        PageLeft => "The previous 8 tracks",
        PageRight => "The next 8 tracks",
    }
    mode Pan {
        strips => |ctx| strips(ctx, "The panorama of 8 tracks".to_string()),
        PageLeft => "The previous 8 tracks",
        PageRight => "The next 8 tracks",
    }
    mode Send1 | Send2 | Send3 | Send4 | Send5 | Send6 | Send7 | Send8 {
        strips => |ctx| send_strips(ctx),
        PageLeft => "The previous 8 tracks",
        PageRight => "The next 8 tracks",
    }
    mode Track {
        strips => |ctx| match ctx.model.current_track_bank().selected_item() {
            Some(track) => strips(ctx, format!("{}: 1 volume, 2 panorama, 3-8 sends 1-6", track.name())),
            None => strips(ctx, "The selected track's volume, panorama, sends (none selected)".to_string()),
        },
        PageLeft => "The previous 8 tracks",
        PageRight => "The next 8 tracks",
    }
    mode DeviceParams {
        strips => |ctx| device_strips(ctx),
        Select + PageLeft => "The previous device",
        Select + PageRight => "The next device",
        PageLeft => "The previous parameter page",
        PageRight => "The next parameter page",
    }
    mode User {
        strips => |ctx| strips(ctx, format!("The {}", if is_project_mode(ctx) { "project parameters (white)" } else { "track parameters (amber)" })),
        PageLeft => |ctx| format!("The previous {} parameter page", if is_project_mode(ctx) { "project" } else { "track" }),
        PageRight => |ctx| format!("The next {} parameter page", if is_project_mode(ctx) { "project" } else { "track" }),
    }
    mode HostStrips {
        strips => "The active row of the host's page (Select + touch: assign)" long STRIPS_SURFACE_LONG,
        Shift + PageLeft => |ctx| format!("Metronome: turn it {}", on_off(!ctx.model.transport().is_metronome_on())),
        Shift + PageRight => |ctx| format!("Loop: turn it {}", on_off(!ctx.model.transport().is_loop())),
        Select + PageLeft => "Host page strips: the previous row",
        Select + PageRight => "Host page strips: the next row",
        PageLeft => "Host page strips: the previous control set / page",
        PageRight => "Host page strips: the next control set / page",
    }
    mode SurfaceLink {
        strips => "The cells of the page surface's active row" long STRIPS_SURFACE_LONG,
        PageLeft => "Page surface: the previous page",
        PageRight => "Page surface: the next page",
        Enter => "Page surface: reset the selected cell",
        MasterKnob => "Turn: the selected cell (Shift: fine); push: reset it",
    }
}

// ------------------------------------------------------------------------------------------
// The long texts

const SHIFT_LONG: &str = "Hold Shift and press a button: its second function (each box shows it while Shift is held).\nA tap of Shift opens the Shift view on the pads (undo, redo, quantize, transpose); Shift again closes it.\nWith the setting \"Shift Pads While Holding Shift\" the Shift view is shown while Shift is held.\nShift + strips and Shift + encoder: fine changes. Shift + Select: slow fader change on/off.";
const SELECT_LONG: &str = "Hold Select and press a button: its Select function (each box shows it while Select is held).\nSelect + scene: select the scene; + pad (Session): select the clip; + MACRO/LEVEL/AUX: add an instrument/audio/effect track.\nSelect + SOLO/MUTE: clear solo/mute of all tracks; + PLAY: punch in; + NOTE REPEAT: fill mode; + AUTO: automation mode.\nSelect + SONG: the page surface (doc/SURFACE_LINK.md). Shift + Select: slow fader change on/off.";
const CLEAR_LONG: &str = "Hold Clear and press a scene button: delete the scene; a group button: delete the track.\nSession: Clear + pad deletes the clip. Play/Chords: Clear + pad deletes the notes of the pad's note in the clip.\nDrum: Delete the notes of the drum pad in the clip.";
const DUPLICATE_LONG: &str = "Hold Duplicate and press a group button: duplicate the track.\nSession: hold Duplicate, press a clip with content (the source), then an empty slot (the copy); release.\nShift + Duplicate: double the content of the selected clip.";
const PLAY_LONG: &str = "Play: start / stop (the setting \"Behaviour on Pause\"); a double click returns to zero (setting).\nShift + Play: restart from the beginning. Select + Play: punch in. Select + Shift + Play: punch out.";
const RECORD_LONG: &str = "Rec: the setting \"Record Button Function\"; Shift + Rec: \"Shift + Record Button Function\".\nHold Rec and press a group button: record arm the track.";
const PAGE_LONG: &str = "Page Left/Right: the previous / next page of the active strip mode (8 tracks, the parameter pages; Select + Page in the device mode: the device).\nShift + Page Left: metronome on/off. Shift + Page Right: loop on/off.\nOn the host page strips: the control set / page (Select: the row); on the page surface: the page.";
const SOLO_LONG: &str = "Hold Solo and press a group button: solo the track.\nSelect + Solo: clear the solo of all tracks. Shift + Solo: the next length for new clips.";
const MUTE_LONG: &str = "Hold Mute and press a group button: mute the track. Session: Mute + pad stops the track's clip (+ Select: the alternative stop).\nSelect + Mute: clear the mute of all tracks. Shift + Mute: stop all clips (playback goes on).";
const REPEAT_LONG: &str = "Note Repeat: note repeat on/off. Long press or Shift + Note Repeat: the configuration on the pads (period, length, mode); press again to close it.\nSelect + Note Repeat: fill mode on/off.";
const AUTO_LONG: &str = "Auto: arranger automation writing on/off. Shift + Auto: clip launcher automation writing on/off.\nSelect + Auto: the next automation write mode.";
const MACRO_LONG: &str = "Macro: the strips control the selected track: volume, panorama, sends 1-6.\nSelect + Macro: add an instrument track.";
const LEVEL_LONG: &str = "Level: the strips control the volume of 8 tracks, press again: the panorama.\nSelect + Level: add an audio track.";
const AUX_LONG: &str = "Aux: the strips control one send of 8 tracks; press again: the next send.\nHold Aux and press a group button A-H: that send.\nShift + Aux: effect tracks / instrument and audio tracks. Select + Aux: add an effect track.";
const SCENE_LONG: &str = "Scene 1-8: launch the scene. Select + scene: select it without launching. Shift + scene: the alternative launch.\nClear + scene: delete it. Hold Step / Pad Mode + a scene button: select a sequencer / play view.";
const SONG_LONG: &str = "Song: the Session view (the clips on the pads); press again: flip the clips' orientation. Long press: the birds-eye view.\nShift + Song: the arrange / mix panel.\nSelect + Song: the page surface on, the next source, off (doc/SURFACE_LINK.md).";
const STEP_LONG: &str = "Step: the next sequencer (Sequencer, Poly Sequencer, Raindrops, Drum, Drum 4, Drum 8).\nHold Step and press scene 1-6 to select one directly.";
const PAD_MODE_LONG: &str = "Pad Mode: the next play view (Play, Chords, Piano, Drum 64).\nHold Pad Mode and press scene 1-4 to select one directly.";
const BROWSE_LONG: &str = "Browse: the browser for the selected device (replace it), without a device: add one to the selected track.\nShift + Browse: insert a device before the selected one; Select + Browse: after it.\nWhile browsing: the encoder selects (Shift: the tab, Select: before / after), its push confirms, Browse discards.";
const MONITOR_LONG: &str = "MST: master volume, GRP: play position, IN 1: metronome volume, HEADPHONE: cue volume.\nPush the encoder for the second function: master panorama, loop start (hold Select: loop length), cue mix.\nThe level meters show the master track.";
const PUSH_LONG: &str = "Push: toggle the second function of the encoder mode (master panorama, loop start, cue mix); while browsing: confirm the selection.";
const ENCODER_LONG: &str = "Turn: the parameter of the encoder mode (MST, GRP, IN 1, HEADPHONE; push: the second function); Shift: slow.\nHold Tempo / Swing: tempo / shuffle amount. Hold Perform / Notes / Lock / Tune: the options of the view.\nBrowsing: the results (Shift: the tab; Select: insert before / after).";
const TEMPO_LONG: &str = "Hold Tempo: the tempo on the pads, the encoder changes it (Shift: fine).\nShift + Tempo: tap the tempo.";
const SWING_LONG: &str = "Hold Swing: the shuffle amount on the pads, the encoder changes it.\nShift + Swing: the shuffle rate. Select + Swing: the groove on/off.";
const GRID_LONG: &str = "Grid: quantize the selected clip. Shift + Grid: the next record quantization; Select + Grid: the previous one.\nHold Grid + A-H: the resolution of the sequencer. Session: Grid + pad: the track back to the arrangement.";
const OPTION_LONG: &str = "Hold Perform / Notes / Lock / Tune and turn the encoder: the scale / the base note / chromatic / the octave (Play, Chords, Piano, Sequencer); Tune in the drum sequencers: the pads.\nLock: a short press switches the fixed accent (velocity) on/off; hold it: the accent velocity on the pads.";
const PLAY_VIEW_LONG: &str = "Up / down: the octave; left / right: the scale layout (the offsets between the rows). Hold Perform / Notes / Lock / Tune and turn the encoder: the scale / the base note / chromatic / the octave.\nClear + pad: delete that note in the clip. The scale settings are in the document settings.";
const CHORDS_LONG: &str = "One chord per pad; the base note from the scale rises from left to right. The rows from the bottom: triads, dyads (power chords), sus2, sus4, 6, maj7, 9, 11.\nUp / down: the octave; the scale options as in the Play view.";
const DRUM64_LONG: &str = "64 drum pads in the colours of the drum machine; the 16 at the bottom left are the ones of the drum sequencer.\nUp / down: the pads by 16 (the bank of the drum machine). Clear + pad: delete the notes of the pad.";
const DRUM_LONG: &str = "Bottom left 4×4: the drum pads (play and select one); bottom right 4×4: the clip's pages / loop (hold one and press another: the loop), with Lock (fixed accent) the velocities.\nTop 4 rows: the 32 steps of the selected pad. Select + step: the note repeat of the step +1. Clear + drum pad: delete its notes.\nLeft / right: the clip page; up / down: the drum pads by 16. Hold Grid + A-H: the resolution.";
const DRUM_LANES_LONG: &str = "The steps of several drum sounds (the drum machine's pads from the selected one). Select + step: the note repeat of the step +1.\nLeft / right: the clip page; up / down: the drum sounds. Hold Grid + A-H: the resolution.";
const SEQUENCER_LONG: &str = "Top row: the pages of the clip (green: playing, blue: edited); press one to edit it, hold one and press another: the loop.\nBelow: 8×7 steps, the rows are the notes of the scale. Select + step: the note repeat of the step +1.\nLeft / right: the page; up / down: the note range. Perform / Notes / Lock / Tune + encoder: the scale options. Hold Grid + A-H: the resolution.";
const POLY_LONG: &str = "Play one or more notes in the lower half, then press a step in the upper half: the step gets these notes.\nLeft / right: the page; up / down: the note range. Duplicate + step: copy it.";
const RAINDROPS_LONG: &str = "The lowest row shows the playable notes. A pad in the column above a note starts a raindrop; when it reaches the note, the note sounds.\nUp / right: the notes up; down / left: down.";
const SESSION_LONG: &str = "Press: launch (an empty slot of an armed track: record). Shift + pad: the alternative launch. Select + pad: select the clip.\nClear + pad: delete. Duplicate + pad with content, then an empty slot: copy. Mute + pad: stop the track (Select: alternative). Grid + pad: back to the arrangement. Browse + pad: browse for a clip.\nArrows: scroll by 1 track / scene (with the Shift view: by 8). Long press SONG: the birds-eye view.";
const SHIFT_VIEW_LONG: &str = "The top row of the Shift view: undo, redo, quantize the selected clip 100% / 50%, transpose it a semitone down / up, an octave down / up.\nThe arrows scroll the session by 8 tracks / scenes.";
const NOTE_REPEAT_LONG: &str = "The note repeat configuration (long press or Shift + Note Repeat; Note Repeat closes it).\nThe left pair of coloured columns: the repeat period; the right pair: the note length. The two green pads: the previous / next arpeggiator mode.";
const BROWSER_LONG: &str = "Columns 1-6: the first 6 filter columns of the browser, column 8 (yellow): the results.\nRow 1: the selection up by 1, row 2: up by 8, row 3: down by 8, row 4: down by 1.\nRed: discard and close, green: confirm and close; the orange pads at the bottom preview the presets.";
const ACCENT_LONG: &str = "While Lock is held: the 16 pads at the bottom right set the velocity of the fixed accent. A short press of Lock switches the fixed accent on/off.";
const SURFACE_LONG: &str = "The page surface (Select + SONG): row 1 the page list, its last two pads the previous / next page; rows 2-7 the cells of the page (a knob cell selects its row for the strips, a pad cell is pressed); row 8 the sections of the source.\nPAGE LEFT/RIGHT: the page, up / down: the row, left / right: the control set or the value of the selected cell. Without a source the grid shows a red cross.";
const STRIPS_SURFACE_LONG: &str = "Swipe a strip: the value (relative, no jumps); Shift: fine. Select + touch a strip: assign the cell to that remote slot of the host.";

// ------------------------------------------------------------------------------------------
// The state dependent texts

fn on_off(state: bool) -> &'static str {
    if state {
        "on"
    } else {
        "off"
    }
}

fn view_name(ctx: &Ctx, view: Views) -> String {
    ctx.views.get(view).and_then(|v| v.try_borrow().ok().map(|v| v.name().to_string())).unwrap_or_else(|| view.name().to_string())
}

fn browsing(ctx: &Ctx) -> bool {
    ctx.model.browser().is_some_and(|b| b.is_active())
}

fn birds_eye(ctx: &Ctx) -> bool {
    ctx.views.get(Views::Session).and_then(|v| v.try_borrow().ok().map(|v| v.is_birds_eye_active())).unwrap_or(false)
}

fn is_project_mode(ctx: &Ctx) -> bool {
    ctx.modes
        .get(Modes::User)
        .and_then(|m| m.try_borrow().ok().and_then(|m| m.as_any().downcast_ref::<super::mode::MaschineJamUserMode>().map(|m| m.is_project_mode())))
        .unwrap_or(true)
}

fn shift_button(ctx: &Ctx) -> &'static str {
    if ctx.views.is_active(Views::Shift) && !ctx.surface.is_pressed(ButtonId::Shift) {
        return "Shift view is open: press Shift to close it";
    }
    if ctx.surface.is_pressed(ButtonId::Shift) {
        return "Held: the buttons do their Shift function (see their boxes)";
    }
    let id = MaschineJamConfiguration::SHIFT_VIEW_WHILE_HELD;
    if ctx.config.is_setting_active(id) && ctx.config.custom_bool(id) {
        return "Hold: the Shift view on the pads and the Shift functions of the buttons";
    }
    "Hold: the Shift functions of the buttons; tap: the Shift view on the pads"
}

fn play_button(ctx: &Ctx) -> String {
    let short = if ctx.model.transport().is_playing() {
        match ctx.config.behaviour_on_pause() {
            TransportBehavior::ReturnToZero => "Stop playback and return to zero",
            TransportBehavior::Stop => "Stop playback",
            TransportBehavior::Pause => "Pause playback",
        }
    } else {
        "Start playback"
    };
    let double = if ctx.config.is_play_double_click_return_to_zero() { " (double click: to zero)" } else { "" };
    format!("{short}{double}")
}

fn automation(ctx: &Ctx, is_launcher: bool) -> String {
    let transport = ctx.model.transport();
    if is_launcher {
        format!("Clip launcher automation writing: turn it {}", on_off(!transport.is_writing_clip_launcher_automation()))
    } else {
        format!("Arranger automation writing: turn it {}", on_off(!transport.is_writing_arranger_automation()))
    }
}

/// The mode which a `ModeMultiSelectCommand` selects next: the next one after the active one if
/// it was the last selected, else the last selected.
pub fn next_of(ctx: &Ctx, modes: &[Modes], current: Modes) -> Modes {
    if ctx.modes.active_id() != Some(current) {
        return current;
    }
    let index = modes.iter().position(|m| *m == current).unwrap_or(0);
    modes[(index + 1) % modes.len()]
}

/// LEVEL (the escape hatch: the command knows the mode it selects next).
pub fn level_button(ctx: &Ctx, current: Modes) -> Option<Help> {
    let next = next_of(ctx, &[Modes::Volume, Modes::Pan], current);
    let short = match (ctx.modes.active_id(), next) {
        (Some(Modes::Volume), _) => "Press: the strips to the panorama of 8 tracks",
        (Some(Modes::Pan), _) => "Press: the strips back to the volume of 8 tracks",
        (_, Modes::Pan) => "Press: the strips to the panorama of 8 tracks",
        _ => "Press: the strips to the volume of 8 tracks",
    };
    help(short)
}

/// AUX (the escape hatch).
pub fn aux_button(ctx: &Ctx, current: Modes) -> Option<Help> {
    let modes: Vec<Modes> = (0..8).map(|i| Modes::get(Modes::Send1, i)).collect();
    let next = next_of(ctx, &modes, current);
    let to = if ctx.modes.active_id().is_some_and(is_send_mode) { "the next send" } else { "send" };
    help(format!("Press: the strips to {to} {} of 8 tracks; hold + A-H: a send", next.ordinal() - Modes::Send1.ordinal() + 1))
}

/// CONTROL (the escape hatch): `current` is the mode its multi select command selects next.
pub fn control_button(ctx: &Ctx, current: Modes) -> Option<Help> {
    let host = surface_link::has_host_pages(ctx);
    let long = if host {
        lines(&[
            "Control: the strips switch between the device parameters and the host's pages (the host page strips).",
            "Page Left/Right: the parameter pages; on the host page strips the control set (Select: the row).",
            "Shift + Control: the host page strips on the first remote control page.",
            "Select + Control: the page surface on / to the next source (as Select + SONG).",
        ])
    } else {
        lines(&[
            "Control: the strips switch between the device's remote controls (coloured) and the project (white) / track (amber) parameters.",
            "Page Left/Right: the parameter pages (Select + Page: the device).",
            "Select + Control: project / track parameters. Shift + Control: the window of the selected device.",
        ])
    };
    let modes = if host { [Modes::DeviceParams, Modes::HostStrips] } else { [Modes::DeviceParams, Modes::User] };
    let current = if modes.contains(&current) { current } else { Modes::DeviceParams };
    let back = if ctx.modes.is_any_active(&modes) { "back " } else { "" };
    let short = match next_of(ctx, &modes, current) {
        Modes::HostStrips => "Press: the strips to the host's pages (host page strips)".to_string(),
        Modes::User => "Press: the strips to the project / track parameters".to_string(),
        _ => {
            let device = ctx.model.cursor_device();
            if device.does_exist() {
                format!("Press: the strips {back}to the parameters of {}", device.name())
            } else {
                format!("Press: the strips {back}to the device parameters (none selected)")
            }
        }
    };
    help_long(short, long)
}

/// The group buttons: select the tracks (their names in the long text).
fn track_row(ctx: &Ctx) -> Help {
    let bank = ctx.model.current_track_bank();
    let names: Vec<String> = (0..8)
        .filter_map(|i| {
            let track = bank.item(i);
            track.does_exist().then(|| format!("{}: {}{}", (b'A' + i as u8) as char, track.name(), if track.is_selected() { " (selected)" } else { "" }))
        })
        .collect();
    Help::new("Select the track (a selected group track: expand / collapse)").with_long(lines(&[
        &names.join(", "),
        "Hold Rec / Solo / Mute / Clear / Duplicate and press A-H: record arm / solo / mute / delete / duplicate the track.",
        "Hold Aux + A-H: the send. Hold Grid + A-H: the resolution of the sequencer.",
    ]))
}

/// Aux + the group buttons: the sends.
fn send_row(ctx: &Ctx) -> Option<String> {
    let bank = ctx.model.current_track_bank();
    let track = bank.selected_item().or_else(|| Some(bank.item(0)).filter(|t| t.does_exist()))?;
    let sends: Vec<String> = (0..8)
        .filter_map(|i| {
            let send = track.send_bank().item(i);
            send.does_exist().then(|| format!("{}: {}", (b'A' + i as u8) as char, send.name()))
        })
        .collect();
    (!sends.is_empty()).then(|| format!("Strips: the send ({})", sends.join(", ")))
}

/// Grid + the group buttons: the resolution of the sequencer (none outside the sequencers).
struct Resolutions(Option<Help>);

impl IntoRes for Resolutions {
    fn into_res(self) -> Res {
        match self.0 {
            Some(help) => Res::Help(help),
            None => NoHelp.into_res(),
        }
    }
}

fn resolution_row(ctx: &Ctx) -> Resolutions {
    let selected = ctx.active_view().and_then(|view| view.try_borrow().ok().and_then(|view| view.sequencer_resolution_index(ctx)));
    Resolutions(selected.map(|selected| {
        let names: Vec<String> = (0..8).map(|i| format!("{}: {}", (b'A' + i as u8) as char, Resolution::name_at(i))).collect();
        Help::new(format!("The sequencer resolution (now {})", Resolution::name_at(selected))).with_long(names.join(", "))
    }))
}

/// The scene buttons: the text, the scene names in the long text.
fn scene_row(ctx: &Ctx, text: &str) -> Help {
    let bank = ctx.model.scene_bank();
    let names: Vec<String> = (0..8)
        .filter_map(|i| {
            let scene = bank.item(i);
            scene.does_exist().then(|| format!("{}: {}", i + 1, scene.name()))
        })
        .collect();
    Help::new(text).with_long(lines(&[&names.join(", "), SCENE_LONG]))
}

const SCENE_SEQUENCER_VIEWS: [Option<Views>; 8] =
    [Some(Views::Sequencer), Some(Views::PolySequencer), Some(Views::Raindrops), Some(Views::Drum), Some(Views::Drum4), Some(Views::Drum8), None, None];

const SCENE_PLAY_VIEWS: [Option<Views>; 8] = [Some(Views::Play), Some(Views::Chords), Some(Views::Piano), Some(Views::Drum64), None, None, None, None];

/// Step / Pad Mode + the scene buttons: the views.
fn view_row(ctx: &Ctx, views: &[Option<Views>; 8], text: &str) -> String {
    let names: Vec<String> = views.iter().flatten().map(|v| view_name(ctx, *v)).collect();
    format!("{text}: {}", names.join(", "))
}

/// The view a view multi select command (STEP, PAD MODE) selects next.
fn next_view(ctx: &Ctx, views: &[Views]) -> Views {
    let index = ctx.views.active_id().and_then(|id| views.iter().position(|v| *v == id)).map(|i| i + 1).unwrap_or(0);
    views[if index >= views.len() { 0 } else { index }]
}

fn monitor_button(ctx: &Ctx, mode: EncoderMode) -> String {
    let text = match mode {
        EncoderMode::MasterVolume => "Encoder: master volume; push: master panorama",
        EncoderMode::PlayPosition => "Encoder: play position; push: loop start (+ Select: length)",
        EncoderMode::MetronomeVolume => "Encoder: metronome volume",
        _ => "Encoder: cue volume; push: cue mix",
    };
    if jam::encoder_manager(ctx).borrow().is_active_encoder_mode(mode) {
        format!("Active · {text}")
    } else {
        text.to_string()
    }
}

fn encoder_push(ctx: &Ctx) -> Option<String> {
    if browsing(ctx) {
        return Some("Confirm the browser selection and close the browser".to_string());
    }
    let manager = jam::encoder_manager(ctx);
    let manager = manager.borrow();
    let mode = manager.active_encoder_mode()?;
    let second = match mode {
        EncoderMode::MasterVolume | EncoderMode::MasterPanning => "the master panorama",
        EncoderMode::PlayPosition => "the loop start",
        EncoderMode::CueVolume | EncoderMode::CueMix => "the cue mix",
        EncoderMode::SelectedTrackVolume | EncoderMode::SelectedTrackPanning => "the selected track's panorama",
        _ => return Some("Push: no second function in this encoder mode".to_string()),
    };
    if manager.is_function_toggled() {
        return Some(format!("Encoder back to {}", mode.label().unwrap_or_default()));
    }
    Some(format!("Encoder: {second}"))
}

/// Select changes what the encoder does: the loop length, the browser's insert position.
fn select_changes_encoder(ctx: &Ctx) -> bool {
    let manager = jam::encoder_manager(ctx);
    let manager = manager.borrow();
    manager.temporary_encoder_mode() == Some(EncoderMode::TemporaryBrowser) || (manager.temporary_encoder_mode().is_none() && manager.target() == Some(EncoderTarget::LoopStart))
}

fn encoder(ctx: &Ctx) -> Option<String> {
    let manager = jam::encoder_manager(ctx);
    let manager = manager.borrow();
    let (shift, select) = (ctx.surface.is_shift_pressed(), ctx.surface.is_select_pressed());
    if let Some(temporary) = manager.temporary_encoder_mode() {
        return Some(match temporary {
            EncoderMode::TemporaryTempo => format!("Turn: the tempo (now {:.2})", ctx.model.transport().tempo()),
            EncoderMode::TemporarySwing => "Turn: the shuffle amount".to_string(),
            EncoderMode::TemporaryBrowser if shift => "Turn: the browser tab".to_string(),
            EncoderMode::TemporaryBrowser if select => "Turn: insert the new device before / after the selected one".to_string(),
            EncoderMode::TemporaryBrowser => "Turn: browse the results".to_string(),
            mode => view_option(ctx, mode).unwrap_or_else(|| "Turn: no option in this view".to_string()),
        });
    }
    let target = match manager.target()? {
        EncoderTarget::MasterVolume => "the master volume",
        EncoderTarget::MasterPan => "the master panorama",
        EncoderTarget::SelectedTrackVolume => "the selected track's volume",
        EncoderTarget::SelectedTrackPan => "the selected track's panorama",
        EncoderTarget::MetronomeVolume => "the metronome volume",
        EncoderTarget::CueVolume => "the cue volume",
        EncoderTarget::CueMix => "the cue mix",
        EncoderTarget::PlayPosition => "the play position",
        EncoderTarget::LoopStart if select => "the loop length",
        EncoderTarget::LoopStart => "the loop start (Select: the length)",
    };
    Some(format!("Turn: {target}{}", if shift { " (slow)" } else { "" }))
}

/// What the encoder changes while Perform / Notes / Lock / Tune is held, in the active view
/// (former `IMaschineView.changeOption`; None: the view has no such option).
fn view_option(ctx: &Ctx, mode: EncoderMode) -> Option<String> {
    let view = match ctx.views.active_id()? {
        // The accent view (Lock held) forwards to the view below
        Views::Control => ctx.views.active_id_ignore_temporary()?,
        view => view,
    };
    let scales = ctx.model.scales();
    match view {
        Views::Play | Views::Piano | Views::Chords | Views::Sequencer => Some(match mode {
            EncoderMode::TemporaryPerform => format!("Turn: the scale (now {})", scales.scale().name()),
            EncoderMode::TemporaryNotes => format!("Turn: the base note (now {})", Scales::BASES[scales.scale_offset_index()]),
            EncoderMode::TemporaryLock => format!("Turn: chromatic on/off (now {})", on_off(scales.is_chromatic())),
            EncoderMode::TemporaryTune if view == Views::Sequencer => "Turn: the note range up / down".to_string(),
            EncoderMode::TemporaryTune => format!("Turn: the octave (now {})", scales.octave()),
            _ => return None,
        }),
        Views::Drum if mode == EncoderMode::TemporaryTune => Some("Turn: the drum pads up / down".to_string()),
        Views::Drum4 | Views::Drum8 if mode == EncoderMode::TemporaryTune => Some("Turn: the drum sounds up / down".to_string()),
        _ => None,
    }
}

/// PERFORM, NOTES, LOCK, TUNE.
fn view_option_button(ctx: &Ctx, mode: EncoderMode) -> Option<String> {
    let option = view_option(ctx, mode).map(|text| text.replacen("Turn: ", "", 1));
    if mode == EncoderMode::TemporaryLock {
        let accent = on_off(!ctx.config.is_accent_active());
        return Some(match option {
            Some(option) => format!("Tap: fixed accent {accent}; hold + encoder: {option}"),
            None => format!("Tap: fixed accent {accent}; hold: the accent velocity on the pads"),
        });
    }
    option.map(|option| format!("Hold + encoder: {option}"))
}

fn grid_play(ctx: &Ctx) -> String {
    if ctx.views.is_active(Views::Piano) {
        return "A piano, 4 octaves (white: white keys, track colour: black keys)".to_string();
    }
    let scales = ctx.model.scales();
    format!("Play {} {} (layout {}); root notes in the track colour", Scales::BASES[scales.scale_offset_index()], scales.scale().name(), scales.scale_layout().name())
}

fn grid_session(ctx: &Ctx) -> &'static str {
    if birds_eye(ctx) {
        "Birds-eye: a pad selects a block of 8 × 8 clips"
    } else if ctx.config.is_flip_session() {
        "Launch / record the clips (a row per track)"
    } else {
        "Launch / record the clips (a column per track)"
    }
}

/// The arrows of the Session view (by 1) and of the Shift view (by 8).
fn scroll(ctx: &Ctx, key: HelpKey, by_page: bool) -> String {
    let button = match key {
        HelpKey::Button(button) => button,
        _ => ButtonId::ArrowDown,
    };
    let horizontal = matches!(button, ButtonId::ArrowLeft | ButtonId::ArrowRight);
    let forwards = matches!(button, ButtonId::ArrowRight | ButtonId::ArrowDown);
    let what = if horizontal != ctx.config.is_flip_session() { "track" } else { "scene" };
    if by_page {
        format!("The {} 8 {what}s", if forwards { "next" } else { "previous" })
    } else {
        format!("Scroll the clips 1 {what} {}", if forwards { "forwards" } else { "backwards" })
    }
}

fn grid_note_repeat(ctx: &Ctx) -> String {
    let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
    let period = Resolution::name_at(Resolution::get_match(note_repeat.period));
    let length = Resolution::name_at(Resolution::get_match(note_repeat.note_length));
    format!("Left coloured pair: the period (now {period}); right pair: length ({length})")
}

/// The strips of a mode: its text and how the strips change.
fn strips(ctx: &Ctx, short: String) -> Help {
    let slow = MaschineJamConfiguration::is_slow_fader_change(&ctx.config);
    let long = format!(
        "{short}.\nTouch and swipe a strip to change the value; with Shift fine. Shift + Select: slow fader change permanently (now {}).\nMACRO, LEVEL, AUX, CONTROL select what the strips control; PAGE LEFT/RIGHT the page.",
        on_off(slow)
    );
    Help::new(short).with_long(long)
}

fn send_strips(ctx: &Ctx) -> Help {
    let index = ctx.modes.active_id().map(|m| m.ordinal() - Modes::Send1.ordinal()).unwrap_or(0);
    let bank = ctx.model.current_track_bank();
    let track = bank.selected_item().or_else(|| Some(bank.item(0)).filter(|t| t.does_exist()));
    let name = track.map(|t| t.send_bank().item(index)).filter(|s| s.does_exist()).map(|s| format!(" ({})", s.name())).unwrap_or_default();
    strips(ctx, format!("Send {}{name} of 8 tracks", index + 1))
}

fn device_strips(ctx: &Ctx) -> Help {
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return strips(ctx, "The device parameters (no device selected)".to_string());
    }
    let page = device.parameter_bank().map(|bank| bank.page_bank().selected_item()).filter(|p| !p.trim().is_empty());
    let page = page.map(|p| format!(", page {p}")).unwrap_or_default();
    strips(ctx, format!("The parameters of {}{page}", device.name()))
}
