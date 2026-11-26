// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The help texts of the Push 1, 2 and 3 (not part of Java, doc/CONTROLLER_HELP_PLAN.md §13): one
//! table ([`crate::help!`], `framework/help_table.rs`) for the buttons, the pads, the 8 knobs above
//! the display and the other encoders, by the held modifiers, the active mode and view. The models
//! share the setup, so they share the table; rows which differ ask [`push::version`]. The Push 3's
//! own buttons are `none` on the Push 1 and 2 (they have no such control: the rows exist only
//! when the button is registered).
//!
//! The rows of buttons are one box each: the 8 buttons below the display (`Row11`-`Row18`, the
//! Push 1's first row), the 8 above it (`Row21`-`Row28`, the Push 1's second row below the
//! display), the scene buttons (`Scene1`-`Scene8`, 1/4 .. 1/32t). The texts of the modes'
//! button rows are summaries (the display labels the buttons).
//!
//! The texts follow the code (where it differs from the manual, the code wins; the differences
//! are listed in doc/CONTROLLER_HELP_PLAN.md §13).

use crate::config::push::{self as config, LockState, PushVersion};
use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;
use crate::framework::help::{automation, browsing, lines, on_off, play_button, Help};
use crate::framework::ids::{Modes, Views};

use super::surface as push;

crate::help! {
    global {
        // Modifiers
        Shift + Shift => "Held: the buttons do their Shift function; the knobs change finely",
        Shift => "Hold: the Shift functions of the buttons (see their boxes); knobs: fine" long SHIFT_LONG,
        Shift + Select => "Hold with Shift: Shift + Select functions (punch out, alternative stop all)",
        Select + Select => "Held: the buttons do their Select function (see their boxes)",
        Select => |ctx| select_button(ctx), long SELECT_LONG,
        Delete + Delete => "Held: a pad, scene, track or device button deletes; a touched knob resets",
        Delete => "As the Delete key; hold + pad / scene / button: delete; + touch a knob: reset" long DELETE_LONG,

        // Transport
        Select + Shift + Play => |ctx| format!("Punch out: turn it {}", on_off(!ctx.model.transport().is_punch_out_enabled())),
        Shift + Play => |ctx| format!("Loop: turn it {}", on_off(!ctx.model.transport().is_loop())),
        Select + Play => |ctx| format!("Punch in: turn it {}", on_off(!ctx.model.transport().is_punch_in_enabled())),
        Play => |ctx| play_button(ctx), long PLAY_LONG,
        Shift + Record => |ctx| record_text(ctx, !ctx.config.is_flip_record()),
        Select + Record => "A new clip on the selected track (as New)",
        Record + Record => "Held: a button below the display record arms its track",
        Record => |ctx| record_text(ctx, ctx.config.is_flip_record()), long RECORD_LONG,
        Shift + New => "A new clip on the selected track and slot, start playback (no overdub)",
        New => "A new clip on the selected track and slot, start playback and overdub" long NEW_LONG,
        Delete + Automation => "Reset the automation overrides",
        Shift + Automation => |ctx| automation(ctx, !ctx.config.is_flip_record()),
        Automation => |ctx| format!("{} (long press: the automation mode)", automation(ctx, ctx.config.is_flip_record())), long AUTOMATION_LONG,
        FixedLength => "The length for new clips on the display (press again: close)",
        Shift + Duplicate => "A new scene from the playing clips",
        Select + Duplicate => "Create a new scene (at the end of the scene list)",
        Duplicate + Duplicate => "Held: a pad, scene or track button duplicates it",
        Duplicate => "Hold + pad / scene / track button: duplicate; press: the selected clip" long DUPLICATE_LONG,
        Shift + Quantize => "The quantize and groove settings on the display",
        Quantize => |ctx| if ctx.modes.is_active(Modes::RecArm) { "Close the quantize settings" } else { "Quantize the selected clip (long press: the settings)" }, long QUANTIZE_LONG,
        Double => "Double the content of the selected clip",
        Shift + Undo => "Redo",
        Undo => "Undo (Shift: redo)",
        Shift + Metronome => "Metronome ticks: on / off",
        Metronome => |ctx| format!("Metronome: turn it {} (long press: its settings)", on_off(!ctx.model.transport().is_metronome_on())), long METRONOME_LONG,
        TapTempo => |ctx| format!("Tap the tempo (now {:.2})", ctx.model.transport().tempo()),

        // The modes of the knobs
        Shift + Track => |ctx| format!("VU meters: turn them {}", on_off(!ctx.config.is_enable_vu_meters())),
        Track => |ctx| track_button(ctx), long TRACK_LONG,
        Device => |ctx| if is_device_mode(ctx) { "Active: the knobs control the device parameters" } else { "Press: the knobs to the device parameters" },
        Clip => |ctx| clip_button(ctx),
        Shift + Mastertrack => "Back to the arrangement (all tracks)",
        Mastertrack => "Select the master track (long press: perspectives and panels)" long MASTER_LONG,
        Volume => |ctx| (push::version(ctx) == PushVersion::Version1).then_some("Press: the volume of 8 tracks (again: the crossfader)"),
        PanSend => |ctx| (push::version(ctx) == PushVersion::Version1).then_some("Press: the panorama, again: the sends 1-8"),
        Shift + Setup => |ctx| setup_button(ctx, true),
        Setup => |ctx| setup_button(ctx, false),
        User => "Press: the knobs to the project / track parameters",
        Scales => |ctx| if ctx.modes.is_active(Modes::Scales) { "Close the scale settings" } else { "The scale settings on the display (Shift: the layout)" },
        Accent => |ctx| format!("Fixed accent: turn it {} (long press: its velocity)", on_off(!ctx.config.is_accent_active())),

        // Mute, solo, clip stop (Push 1: select the 2nd row's state; Push 2/3: the selected track)
        Select + Mute => "Clear mute on all tracks",
        Select + Solo => "Clear solo on all tracks",
        Shift + Select + StopClip => "The alternative stop of all clips",
        Select + StopClip => "Stop all playing clips",
        Shift + Mute => |ctx| lock_text(ctx, LockState::Mute, "mute"),
        Shift + Solo => |ctx| lock_text(ctx, LockState::Solo, "solo"),
        Shift + StopClip => |ctx| lock_text(ctx, LockState::ClipStop, "clip stop"),
        LockMode + Mute => |ctx| lock_text(ctx, LockState::Mute, "mute"),
        LockMode + Solo => |ctx| lock_text(ctx, LockState::Solo, "solo"),
        LockMode + StopClip => |ctx| lock_text(ctx, LockState::ClipStop, "clip stop"),
        Mute => |ctx| state_button(ctx, LockState::Mute, "Mute / unmute", "mute"), long MUTE_LONG,
        Solo => |ctx| state_button(ctx, LockState::Solo, "Solo / unsolo", "solo"), long MUTE_LONG,
        StopClip => |ctx| state_button(ctx, LockState::ClipStop, "Stop the clip of", "stop the clips of"), long MUTE_LONG,

        // Adding things, the browser
        Select + AddEffect => "The favorite devices on the display (add track menu)",
        Shift + AddEffect => "Browser: insert a device before the selected one",
        AddEffect => "Browser: insert a device after the selected one",
        AddTrack => "The add track menu on the display (instrument, audio, effect, favorites)",
        Shift + Browse => |ctx| if browsing(ctx) { "Close the browser and discard the selection" } else { "Browser: insert a device before the selected one" },
        Select + Browse => |ctx| (!browsing(ctx)).then_some("Browser: insert a device after the selected one"),
        Browse => |ctx| browse_button(ctx), long BROWSE_LONG,
        Shift + Convert => "Slice the selected audio clip to a sampler (multi-sample)",
        Convert => "Slice the selected audio clip to a drum machine",

        // Views
        Note => |ctx| if ctx.views.active_id().is_some_and(super::setup::is_session_view) {
            "Back to the play view of the track"
        } else {
            "The play and sequencer views on the display (press again: close)"
        }, long NOTE_LONG,
        Session => |ctx| if ctx.views.active_id().is_some_and(super::setup::is_session_view) {
            "The session view selection on the display (session, flipped, scenes, ...)"
        } else {
            "Session view (hold: only while held)"
        }, long SESSION_LONG,
        Shift + Layout => "Toggle between the play and the sequencer views",
        Layout => "The next layout of the play / sequencer / session views" long LAYOUT_LONG,
        Select + Repeat => |ctx| format!("Fill mode: turn it {}", on_off(!ctx.model.transport().is_fill_mode_active())),
        Shift + Repeat => "The note repeat settings on the display",
        Repeat + Repeat => "Held: a scene button selects the note repeat period",
        Repeat => |ctx| if ctx.modes.is_active(Modes::RepeatNote) {
            "Close the note repeat settings".to_string()
        } else {
            format!("Note repeat: turn it {} (long press: its settings)", on_off(!ctx.config.is_note_repeat_active()))
        }, long REPEAT_LONG,

        // Navigation (the views change the page and octave buttons, the modes left / right)
        Shift + ArrowDown .. ArrowUp => |ctx| scene_arrows(ctx, true),
        ArrowDown .. ArrowUp => |ctx| scene_arrows(ctx, false),
        Shift + ArrowLeft .. ArrowRight => "The previous / next item of the mode (track, device, parameter)",
        ArrowLeft .. ArrowRight => "The previous / next page of the mode (8 tracks, devices, parameters)" long ARROWS_LONG,
        PageLeft .. PageRight => none,
        OctaveDown .. OctaveUp => none,

        // The Push 3
        Shift + InsertScene => "A new scene from the playing clips",
        InsertScene => "Create a new scene",
        Load => "Open a project (the Open dialog)",
        Save => |ctx| if ctx.model.project().is_dirty() { "Save the project (it has changes)" } else { "Save the project" },
        Help => "Show the learning resources of the DAW",
        LockMode => "Hold + Mute / Solo / Stop Clip: lock that state for the buttons above the display",
        ToggleClipView => |ctx| if ctx.modes.is_active(Modes::Session) { "Display: the markers instead of the clips" } else { "Display: the clips and scenes (session mode)" },
        Swing => "Small knob: switch between the tempo and the shuffle amount",
        ToggleMasterCue => "Medium knob: switch between the master and the cue volume",
        EncoderPush => |ctx| jog_press(ctx), long JOG_LONG,
        Left .. Right => |ctx| jog_tilt(ctx), long JOG_LONG,
        ArrowCenter => |ctx| if has_encoder_mode(ctx) { "The mode's center function (browser: insert before / after)" } else { "Launch the selected scene (alternative launch)" },
        Footswitch2 => none,

        // The continuous controls
        Knob1 | Knob2 | Knob3 | Knob4 | Knob5 | Knob6 | Knob7 | Knob8 => none,
        Select + MasterKnob => "Turn: the arranger zoom",
        MasterKnob => "Turn: the master volume (Push 3: or the cue volume; touch: the master mode)",
        Select + Tempo => |ctx| (!browsing(ctx)).then_some("Turn: the loop start (Shift: fine)"),
        Tempo => |ctx| if browsing(ctx) { "Turn: the selection of the selected browser column".to_string() } else { format!("Turn: the tempo (now {:.2}; Push 3: or the shuffle; Shift: fine)", ctx.model.transport().tempo()) },
        Select + PlayPosition => |ctx| (!has_encoder_mode(ctx)).then_some("Turn: the loop length (Shift: fine)"),
        PlayPosition => |ctx| play_position(ctx), long JOG_LONG,
        Shift + Touchstrip => "Touch: the touch strip's settings on the display",
        Touchstrip => |ctx| touchstrip(ctx),

        // The rows of buttons: by the mode (below)
        Shift + Row21 .. Row28 => |ctx| is_mix_mode(ctx).then_some("4: the previous send page; the others as without Shift"),
        Row11 .. Row18 => |ctx| format!("Below the display: the options of the {} mode", mode_name(ctx)),
        Row21 .. Row28 => |ctx| format!("Above the display: the options of the {} mode", mode_name(ctx)),
        Scene1 .. Scene8 => "1/4 .. 1/32t: by the view (see the views)",
    }

    // ------------------------------------------------------------------------------------------
    // The views: the pads, the scene buttons, page and octave

    view Play | Chords | Piano | Drum64 {
        Delete + grid => "Delete the pad's note in the selected clip",
        grid => |ctx| play_grid(ctx), long PLAY_LONG_VIEW,
        Repeat + Scene1 .. Scene8 => "The note repeat period (1/4 .. 1/32t)",
        Delete + Scene1 .. Scene8 => "Delete the scene",
        Shift + Scene1 .. Scene8 => "The alternative launch of the scene",
        Select + Scene1 .. Scene8 => "Select the scene (no launch)",
        Scene1 .. Scene8 => |ctx| scene_row(ctx, "Launch the scene"),
        OctaveDown .. OctaveUp => "The notes an octave down / up",
        PageLeft .. PageRight => none,
    }
    view Session {
        Delete + grid => "Delete the clip",
        Duplicate + grid => "First a clip with content (source), then an empty slot (copy)",
        Browse + grid => "Browse for a clip for the slot",
        Shift + Select + grid => "The birds-eye view: a pad selects a block of 8 × 8 clips",
        Select + grid => "Select the clip (no launch)",
        Shift + grid => "The alternative launch of the clip",
        grid => |ctx| if ctx.config.is_flip_session() { "Launch / record the clips (a row per track)" } else { "Launch / record the clips (a column per track)" }, long SESSION_VIEW_LONG,
        Delete + Scene1 .. Scene8 => "Delete the scene",
        Shift + Scene1 .. Scene8 => "The alternative launch of the scene",
        Select + Scene1 .. Scene8 => "Select the scene (no launch)",
        Scene1 .. Scene8 => |ctx| scene_row(ctx, "Launch the scene"),
        OctaveDown .. OctaveUp => "Down: the next page of scenes; up: the previous",
        PageLeft .. PageRight => "The previous / next 8 tracks",
    }
    view ScenePlay {
        Delete + grid => "Delete the scene",
        Select + grid => "Select the scene (no launch)",
        Shift + grid => "The alternative launch of the scene",
        grid => "The 64 scenes: launch (in the colour of their first clip)",
        Scene1 .. Scene8 => |ctx| scene_row(ctx, "Launch the scene"),
        OctaveDown .. OctaveUp => none,
        PageLeft .. PageRight => none,
    }
    view Sequencer | PolySequencer | Drum | Drum4 | Drum8 | DrumXox {
        Shift + Select + grid => "Lower the note repeat of the step by 1",
        Shift + grid => "Raise the note repeat of the step by 1",
        Select + grid => "Select the step's note for editing",
        Mute + grid => "Mute / unmute the note of the step (drum pads: the pad)",
        Duplicate + grid => "A step with a note: copy it; an empty step: paste it",
        grid => |ctx| sequencer_grid(ctx), long SEQUENCER_LONG,
        Repeat + Scene1 .. Scene8 => "The note repeat period (1/4 .. 1/32t)",
        Scene1 .. Scene8 => |ctx| resolution_row(ctx),
        PageLeft .. PageRight => "The previous / next page of the clip",
        Shift + OctaveDown .. OctaveUp => "The notes / drum pads down / up a little (a scale step, 4 pads)",
        OctaveDown .. OctaveUp => "The notes / drum pads down / up (an octave, 16 pads)",
    }
    view Raindrops {
        grid => "Bottom row: the notes; the pads above a note start a raindrop" long RAINDROPS_LONG,
        Scene1 .. Scene8 => |ctx| resolution_row(ctx),
        PageLeft .. PageRight => none,
        Shift + OctaveDown .. OctaveUp => "The notes down / up by 1 step",
        OctaveDown .. OctaveUp => "The notes down / up",
    }
    view PrgChange {
        grid => "Send program changes (the pads: 64 programs of the bank)",
        Scene1 .. Scene8 => "The bank (press twice: programs 0-63 / 64-127)",
        OctaveDown .. OctaveUp => none,
        PageLeft .. PageRight => none,
    }
    view ClipLength {
        grid => "The length of the selected clip's loop (a pad per length)",
        Scene1 .. Scene8 => none,
        OctaveDown .. OctaveUp => none,
        PageLeft .. PageRight => none,
    }
    view Color {
        grid => "Pick the colour of the track, clip or layer",
        Scene1 .. Scene8 => none,
        OctaveDown .. OctaveUp => none,
        PageLeft .. PageRight => none,
    }

    // ------------------------------------------------------------------------------------------
    // The modes: the knobs, the button rows

    mode Track | Volume | Pan | Crossfader | Send1 | Send2 | Send3 | Send4 | Send5 | Send6 | Send7 | Send8 {
        knobs => |ctx| mix_knobs(ctx),
        Duplicate + Row11 .. Row18 => "Duplicate the track",
        Delete + Row11 .. Row18 => "Delete the track",
        StopClip + Row11 .. Row18 => "Stop the track's clip (alternative stop)",
        Record + Row11 .. Row18 => "Record arm / disarm the track",
        Select + Row11 .. Row18 => "Add / remove the track to the selection (multi select)",
        Row11 .. Row18 => |ctx| Help::new("Select the track; again: into the group or the device (long press: out)").with_long(lines(&[&bank_names(ctx), ROW1_TRACKS_LONG])),
        Row21 .. Row28 => |ctx| mix_row2(ctx),
    }
    mode TrackDetails | DeviceLayerDetails {
        knobs => "The settings of the selected track / layer (shown on the display)",
        Row11 .. Row18 => "Toggle the states: active, solo, mute, rec arm, monitor, ... (display)",
        Row21 .. Row28 => "The colour, the MIDI edit channel and more (display); Select: close",
    }
    mode Master | MasterTemp {
        knobs => "The master track: volume, panorama, cue volume and mix, ...",
        Row11 .. Row18 => "Master track options; 7/8: the previous / next opened project",
        Row21 .. Row28 => "The audio engine on / off and the master options (display)",
    }
    mode Clip {
        knobs => "The selected clip: loop, play range, shuffle and accent (display)",
        Row11 .. Row18 => "The clip's options (display)",
        Row21 .. Row28 => "The clip's options (display)",
    }
    mode AddTrack {
        knobs => none,
        Row11 .. Row18 => "Add an instrument / audio / effect track or a favorite device (display)",
        Row21 .. Row28 => "Add an instrument / audio / effect track or a favorite device (display)",
    }
    mode DeviceParams | DeviceChains {
        knobs => |ctx| device_knobs(ctx),
        Delete + Row11 .. Row18 => "Delete the device",
        Row11 .. Row18 => "Select the device or the parameter page (long press: up the hierarchy)" long DEVICE_ROW1_LONG,
        Row21 .. Row28 => "1 on/off, 2 remote controls, 3 expand, 4 chains, 5 page / device, 6 pin, 7 window, 8 up" long DEVICE_ROW2_LONG,
    }
    mode DeviceLayer | DeviceLayerVolume | DeviceLayerPan | DeviceLayerSend1 | DeviceLayerSend2 | DeviceLayerSend3 | DeviceLayerSend4 | DeviceLayerSend5 | DeviceLayerSend6 | DeviceLayerSend7 | DeviceLayerSend8 {
        knobs => "The layers of the selected device: volume, panorama or a send",
        Row11 .. Row18 => "Select the layer; again: into it (long press: up)",
        Row21 .. Row28 => "The layer modes: volume, panorama, sends (Push 1: the 2nd row's state)",
    }
    mode Browser {
        knobs => "1-7: the filter columns, 8: the results (Shift: fast; Delete + touch: reset)" long BROWSER_LONG,
        Shift + Row11 .. Row18 => "8: preview on / off",
        Row11 .. Row18 => "Select the previous filter item / result",
        Row21 .. Row28 => "Select the next filter item / result",
        ArrowLeft .. ArrowRight => "The previous / next browser tab",
        Browse => "Confirm the selection and close the browser (Shift: discard)",
    }
    mode User {
        knobs => "The project / track parameters of the page",
        Row11 .. Row18 => "Select the parameter page",
        Row21 .. Row28 => "1, 2: the project / track parameters",
    }
    mode Note | Frame | Groove | RecArm | Accent | Scales | ScaleLayout | Fixed | Ribbon | ViewSelect | Automation | Transport | Markers | Session | SessionViewSelect | RepeatNote | Configuration | Info | Setup | Audio {
        knobs => |ctx| format!("The settings of the {} mode (shown on the display)", mode_name(ctx)),
    }
    mode ViewSelect {
        Row11 .. Row18 => "Select a play view: Play, Chords, Piano, Drum 64, ... (display)",
        Row21 .. Row28 => "Select a sequencer: Sequencer, Poly, Drum, Drum 4 / 8, XoX, ... (display)",
    }
    mode SessionViewSelect {
        Row11 .. Row18 => "Select the session view: session, flipped, scenes; the display modes",
        Row21 .. Row28 => "Select the session view: session, flipped, scenes; the display modes",
    }
    mode Scales {
        Row11 .. Row18 => "Select the scale (the base note with Shift: the layout)",
        Row21 .. Row28 => "Select the base note; in key / chromatic",
    }
    mode Fixed {
        Row11 .. Row18 => "Create a new clip of the length",
        Row21 .. Row28 => "Select the length for new clips",
    }
    mode Markers {
        Delete + Row11 .. Row18 => "Delete the marker",
        Row11 .. Row18 => "Move the play position to the marker (and start, see 7 / 8 above)",
        Row21 .. Row28 => "1: add a marker at the play position; 7 / 8: select only / start playback",
    }
    mode Session {
        Row11 .. Row18 => "Launch the clip / scene shown above the button",
        Row21 .. Row28 => "Launch the clip / scene shown below the button",
    }
}

// ------------------------------------------------------------------------------------------
// The long texts

const SHIFT_LONG: &str = "Hold Shift and press a button: its second function (each box shows it while Shift is held). Shift + knobs: fine changes.\nShift + Select + pad (Session): the birds-eye view. Shift in the scale mode: the scale layout.";
const SELECT_LONG: &str = "Tap Select: the track details (in a layer mode: the layer details). Hold Select and press a button: its Select function.\nSelect + pad: select a clip / scene / note without launching; + Play: punch in; + Record: a new clip; + Mute / Solo: clear all; + Stop Clip: stop all; + Repeat: fill mode.\nSelect + master knob: zoom; + small knobs: loop start / length.";
const DELETE_LONG: &str = "Delete: as the Delete key of the DAW. Hold Delete and press a pad (clip, scene, note), a scene button, a track or device button: delete it.\nDelete + touch a knob: reset its value. Delete + Automation: reset the automation overrides.";
const PLAY_LONG: &str = "Play: start / stop (the setting \"Behaviour on Pause\"); a double click returns to zero (setting).\nShift + Play: loop on/off. Select + Play: punch in. Select + Shift + Play: punch out.";
const RECORD_LONG: &str = "Record: arranger recording; Shift + Record: launcher overdub (a recording selected clip: stop it). The setting \"Flip arranger and clip record\" swaps them.\nSelect + Record: a new clip (as New). Hold Record + a button below the display: record arm the track.";
const NEW_LONG: &str = "New: a new clip on the selected track and slot (the length: Fixed Length), start playback, launcher overdub on. Shift + New: without overdub.";
const AUTOMATION_LONG: &str = "Automate: arranger automation writing on/off; Shift: clip launcher automation (the setting flips them). Long press: the automation mode.\nDelete + Automate: reset the automation overrides.";
const DUPLICATE_LONG: &str = "Hold Duplicate and press a scene button (the scene), a track button (the track); Session: a clip with content, then an empty slot (copy).\nPress: duplicate the selected clip. Shift + Duplicate: a new scene from the playing clips. Select + Duplicate: a new scene at the end.";
const QUANTIZE_LONG: &str = "Quantize: the selected clip (the setting \"Quantize Amount\"). Long press or Shift + Quantize: the quantize and groove settings; press again to close.";
const METRONOME_LONG: &str = "Metronome on/off. Long press: the metronome settings (volume, pre-roll). Shift + Metronome: the metronome ticks.";
const TRACK_LONG: &str = "Push 2/3 Mix: the mix modes (the buttons above the display select volume, panorama, crossfader, sends); press again: the effect tracks / the instrument and audio tracks.\nPush 1 Track: the track mode; again: the effect tracks. Shift: the VU meters on/off.";
const MASTER_LONG: &str = "Master: select the master track (the master mode); again: back to the track selected before. Long press: perspectives and panels (frame mode).\nShift + Master: back to the arrangement.";
const MUTE_LONG: &str = "Push 2/3: Mute / Solo / Stop Clip act on the selected track (in a layer mode the layer, in the master mode the master); hold one: the buttons above the display mute / solo / stop the tracks; Shift (Push 3: Lock) + it: lock that state.\nSelect + Mute / Solo: clear all; Select + Stop Clip: stop all clips (Shift: alternative).\nPush 1: the button selects the state of the 2nd row (mute, solo, stop).";
const BROWSE_LONG: &str = "Browse (Push 3: Hot Swap): the presets of the selected device (replace it), without a device: add one. Shift + Browse: insert before, Select + Browse: after.\nWhile browsing: Browse confirms, Shift + Browse discards; the knobs select in the filter columns and the results.";
const NOTE_LONG: &str = "Note: the play view of the track; in a play view: the view selection (Play, Chords, Piano, Drum 64, the sequencers) on the display.";
const SESSION_LONG: &str = "Session: the session view; hold it to use it only while held. Again: the session view selection (session, flipped, scenes; the clip / marker / mixer display).";
const LAYOUT_LONG: &str = "Layout: Play → Chords → Piano → Drum 64; Sequencer → Raindrops → Drum → Drum 4 → Drum 8; Session → flipped → Scenes. Shift + Layout: play ↔ sequencer.";
const REPEAT_LONG: &str = "Repeat: note repeat on/off. Long press or Shift + Repeat: the settings; press again to close. Repeat + scene button: the period.\nSelect + Repeat: fill mode on/off.";
const ARROWS_LONG: &str = "Up / down: the scenes (by 1 or a page, the settings \"Cursor Keys Scene Option\"; Shift: the shifted option). Left / right: the page of the active mode (Shift: the item); in the browser: the tabs.";
const JOG_LONG: &str = "The Push 3's large knob: turn: the play position (Select: the loop length); in the clip, browser and session modes: their selection.\nPress: start / stop playback (the modes: launch / confirm). Tilt left / right: the play position to the loop start / end, the project start / end.";
const PLAY_LONG_VIEW: &str = "Play the notes of the scale (Scale: the scale and base note on the display; Shift: the layout). Octave up / down. Delete + pad: delete the note in the selected clip.\nThe scene buttons launch the scenes (Repeat + scene button: the note repeat period).";
const SESSION_VIEW_LONG: &str = "Press: launch / record a clip. Shift + pad: alternative launch; Select + pad: select; Shift + Select: birds-eye view. Delete + pad: delete. Duplicate: source, then empty slot. Browse + pad: browse.\nThe scene buttons launch scenes. Up / down: scenes, Page ◂ ▸: 8 tracks, Octave: pages of scenes.";
const SEQUENCER_LONG: &str = "Press a step: set / remove a note (with the velocity); hold one and press a later one: the length; long press: edit the note on the display.\nShift + step: note repeat +1 (Select: −1); Select + step: select for editing; Mute + step: mute; Duplicate: copy / paste.\nThe scene buttons: the resolution (1/4 .. 1/32t). Page ◂ ▸: the clip page.";
const RAINDROPS_LONG: &str = "The lowest row shows the playable notes. A pad above a note starts a raindrop which plays the note when it reaches it.";
const ROW1_TRACKS_LONG: &str = "Press: select the track; again: into a group (Shift: expand) or the device mode; long press: out of the group.\nHold Record: rec arm, Delete: delete, Duplicate: duplicate, Stop Clip: stop (alternative), Select: multi select.";
const DEVICE_ROW1_LONG: &str = "The buttons below the display select the device or the parameter page (button 5 above toggles); again: into the device's layers; long press: up. Delete + button: delete the device.";
const DEVICE_ROW2_LONG: &str = "Above the display: 1 the device on/off, 2 the remote controls section, 3 expanded / small, 4 the chains, 5 device / page navigation, 6 pin, 7 the plugin window, 8 up to the track mode.";
const BROWSER_LONG: &str = "Knobs 1-7: the filter columns; knob 8: the results (Shift: fast). Below the display: the previous, above: the next filter item / result; Shift + 8 below: preview.\nLeft / right: the browser tabs. Browse: confirm (Shift: discard). Delete + touch a knob: reset the filter.";
const KNOBS_LONG: &str = "Turn a knob: its value (Shift: fine). Delete + touch a knob: reset it.";

// ------------------------------------------------------------------------------------------
// The state dependent texts

fn mode_name(ctx: &Ctx) -> String {
    ctx.active_mode().and_then(|m| m.try_borrow().ok().map(|m| m.name().to_string())).unwrap_or_default()
}

fn is_mix_mode(ctx: &Ctx) -> bool {
    ctx.modes.active_id().is_some_and(|m| (Modes::Track..=Modes::Send).contains(&m))
}

fn is_device_mode(ctx: &Ctx) -> bool {
    ctx.modes.active_id().is_some_and(|m| m == Modes::DeviceParams || (Modes::DeviceLayer..=Modes::DeviceLayerDetails).contains(&m))
}

/// The Push 3's large knob talks to the clip, browser and session modes.
fn has_encoder_mode(ctx: &Ctx) -> bool {
    ctx.modes.is_any_active(&[Modes::Clip, Modes::Browser, Modes::Session])
}

fn select_button(ctx: &Ctx) -> &'static str {
    if ctx.modes.is_any_active(&[Modes::TrackDetails, Modes::DeviceLayerDetails]) {
        "Tap: close the details; hold: the Select functions"
    } else {
        "Tap: the track / layer details; hold: the Select functions (see their boxes)"
    }
}

fn record_text(ctx: &Ctx, launcher: bool) -> String {
    if launcher {
        format!("Launcher overdub: turn it {} (a recording selected clip: stop it)", on_off(!ctx.model.is_launcher_overdub()))
    } else if ctx.model.transport().is_recording() {
        "Stop the arranger recording".to_string()
    } else {
        "Start the arranger recording".to_string()
    }
}

fn track_button(ctx: &Ctx) -> &'static str {
    let push1 = push::version(ctx) == PushVersion::Version1;
    match (push1, ctx.modes.active_id()) {
        (false, Some(Modes::Track | Modes::Volume | Modes::Crossfader | Modes::Pan)) => "Press: the effect tracks / the instrument and audio tracks",
        (true, Some(Modes::Track)) => "Press: the effect tracks / the instrument and audio tracks",
        (false, _) => "Press: the knobs to the mix (the last mix mode)",
        (true, _) => "Press: the knobs to the selected track (track mode)",
    }
}

fn clip_button(ctx: &Ctx) -> &'static str {
    match (ctx.modes.is_active(Modes::Clip), push::is_modern(ctx)) {
        (true, true) => "Toggle the piano roll on the display",
        (true, false) => "Close the clip mode",
        _ => "Press: the clip's settings (loop, play range, shuffle, accent)",
    }
}

fn setup_button(ctx: &Ctx, shift: bool) -> Option<&'static str> {
    Some(match (push::version(ctx), shift) {
        (PushVersion::Version1, true) => "The hardware configuration (pad sensitivity, ...)",
        (PushVersion::Version1, false) => "The project / track parameters (user mode)",
        (_, true) => return None,
        _ => "The hardware settings (pads, display, LEDs, ...) on the display",
    })
}

fn lock_text(ctx: &Ctx, lock: LockState, name: &str) -> Option<String> {
    push::is_modern(ctx).then(|| {
        if push::lock_state(ctx) == lock {
            format!("Unlock: the buttons above the display stop doing {name}")
        } else {
            format!("Lock: the buttons above the display do {name}")
        }
    })
}

/// MUTE, SOLO, STOP CLIP.
fn state_button(ctx: &Ctx, lock: LockState, action: &str, row_action: &str) -> String {
    if !push::is_modern(ctx) {
        return format!("The 2nd row below the display: {row_action} the tracks");
    }
    if push::lock_state(ctx) == lock {
        return format!("Locked: the buttons above the display {row_action} the tracks");
    }
    let what = if ctx.modes.is_active(Modes::Master) { "the master track" } else { "the selected track" };
    format!("{action} {what} (hold: the buttons above the display)")
}

fn browse_button(ctx: &Ctx) -> &'static str {
    if browsing(ctx) {
        "Confirm the selection and close the browser (Shift: discard)"
    } else if ctx.model.cursor_device().does_exist() {
        "Browser: the presets of the selected device (replace it)"
    } else {
        "Browser: add a device to the selected track"
    }
}

fn scene_arrows(ctx: &Ctx, shift: bool) -> &'static str {
    use crate::config::values::{CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_1, CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_PAGE};
    let option = if shift || ctx.views.is_active(Views::ScenePlay) { ctx.config.cursor_keys_scene_shifted_option() } else { ctx.config.cursor_keys_scene_option() };
    match option {
        CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_1 => "The scenes up / down by 1",
        CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_PAGE => "The scenes up / down by a page",
        _ => "Nothing (the setting \"Cursor Keys Scene Option\")",
    }
}

fn jog_press(ctx: &Ctx) -> &'static str {
    match ctx.modes.active_id() {
        Some(Modes::Browser) => "Confirm the browser selection (as Browse)",
        Some(Modes::Clip | Modes::Session) => "Launch the selected clip / scene",
        _ => "Start / stop playback",
    }
}

fn jog_tilt(ctx: &Ctx) -> &'static str {
    match ctx.modes.active_id() {
        Some(Modes::Browser) => "The previous / next filter or result column",
        Some(Modes::Clip | Modes::Session) => "The previous / next track or scene",
        _ => "The play position to the loop start / end, the project start / end",
    }
}

fn play_position(ctx: &Ctx) -> String {
    if push::version(ctx) != PushVersion::Version3 {
        return "Turn: the play position (Shift: fine)".to_string();
    }
    match ctx.modes.active_id() {
        Some(Modes::Browser) => "Turn: the previous / next item of the selected column".to_string(),
        Some(Modes::Clip | Modes::Session) => "Turn: the previous / next clip / scene".to_string(),
        _ => format!("Turn: the play position (now {}; Shift: fine)", ctx.model.transport().position_text()),
    }
}

fn touchstrip(ctx: &Ctx) -> String {
    if ctx.views.is_active(Views::Session) {
        return "The crossfader (Shift + touch: center it)".to_string();
    }
    let mode = config::RIBBON_MODE_VALUES.get(config::ribbon_mode(&ctx.config)).copied().unwrap_or("Pitch");
    if ctx.config.is_note_repeat_active() && config::ribbon_note_repeat(&ctx.config) > config::NOTE_REPEAT_OFF {
        return "The note repeat (the setting \"Function if Note Repeat is active\")".to_string();
    }
    format!("Ribbon: {mode} (Shift + touch: the settings)")
}

fn bank_names(ctx: &Ctx) -> String {
    let bank = ctx.model.current_track_bank();
    let names: Vec<String> = (0..8)
        .filter_map(|i| {
            let track = bank.item(i);
            track.does_exist().then(|| format!("{}: {}{}", i + 1, track.name(), if track.is_selected() { " (selected)" } else { "" }))
        })
        .collect();
    names.join(", ")
}

fn mix_knobs(ctx: &Ctx) -> Help {
    let short = match ctx.modes.active_id() {
        Some(Modes::Volume) => "The volume of 8 tracks".to_string(),
        Some(Modes::Pan) => "The panorama of 8 tracks".to_string(),
        Some(Modes::Crossfader) => "The crossfader assignment of 8 tracks".to_string(),
        Some(mode) if (Modes::Send1..=Modes::Send8).contains(&mode) => format!("Send {} of 8 tracks", mode.ordinal() - Modes::Send1.ordinal() + 1),
        _ => match ctx.model.current_track_bank().selected_item() {
            Some(track) => format!("{}: volume, panorama, crossfader, sends", track.name()),
            None => "The selected track: volume, panorama, crossfader, sends".to_string(),
        },
    };
    let long = format!("{short}: {}.\n{KNOBS_LONG}", bank_names(ctx));
    Help::new(short).with_long(long)
}

/// The buttons above the display in the mix modes.
fn mix_row2(ctx: &Ctx) -> Help {
    let long_mute = ctx.surface.is_long_pressed(crate::framework::ids::ButtonId::Mute);
    let long_solo = ctx.surface.is_long_pressed(crate::framework::ids::ButtonId::Solo);
    let long_stop = ctx.surface.is_long_pressed(crate::framework::ids::ButtonId::StopClip);
    let short = if push::is_mute_state(ctx, long_mute) {
        "Mute / unmute the track"
    } else if push::is_solo_state(ctx, long_solo) {
        "Solo / unsolo the track"
    } else if push::is_clip_stop_state(ctx, long_stop) {
        "Stop the track's clip (Shift: back to the arrangement)"
    } else if push::is_modern(ctx) {
        "1 Volume, 2 Pan, 3 Crossfader, 4 the next sends, 5-8 the sends (again: track)"
    } else {
        "The state of Mute / Solo / Stop Clip"
    };
    Help::new(short).with_long(lines(&[
        &bank_names(ctx),
        "Push 2/3: 1 volume, 2 panorama, 3 crossfader, 4 the next page of sends (Shift: the previous), 5-8 the sends of the page (again: back to the track mode; 8 without a 4th send: out of the group).",
        "With Mute / Solo / Stop Clip held or locked (Push 1: selected): mute / solo / stop the tracks (Stop + Shift: back to the arrangement).",
    ]))
}

fn device_knobs(ctx: &Ctx) -> Help {
    let device = ctx.model.cursor_device();
    if !device.does_exist() {
        return Help::new("The device parameters (no device selected)").with_long(KNOBS_LONG);
    }
    let page = device.parameter_bank().map(|bank| bank.page_bank().selected_item()).filter(|p| !p.trim().is_empty());
    let short = format!("The parameters of {}{}", device.name(), page.map(|p| format!(", page {p}")).unwrap_or_default());
    let names: Vec<String> = (0..8)
        .filter_map(|i| device.parameter_bank().map(|b| b.item(i)).filter(|p| p.does_exist()).map(|p| format!("{}: {}", i + 1, p.name())))
        .collect();
    let long = format!("{short}: {}.\n{KNOBS_LONG}", names.join(", "));
    Help::new(short).with_long(long)
}

fn play_grid(ctx: &Ctx) -> String {
    let scales = ctx.model.scales();
    match ctx.views.active_id() {
        Some(Views::Piano) => "A piano, 4 octaves (white: white keys, track colour: black keys)".to_string(),
        Some(Views::Drum64) => "Play 64 drum pads (bottom left 16: as in the drum sequencer)".to_string(),
        Some(Views::Chords) => "A chord per pad; rows from the bottom: triad, dyad, sus2, sus4, 6, maj7, 9, 11".to_string(),
        _ => format!("Play {} {} (layout {})", crate::scale::Scales::BASES[scales.scale_offset_index()], scales.scale().name(), scales.scale_layout().name()),
    }
}

fn sequencer_grid(ctx: &Ctx) -> &'static str {
    match ctx.views.active_id() {
        Some(Views::Sequencer) => "Top row: the clip's pages (hold + another: loop); below: 8×7 steps",
        Some(Views::PolySequencer) => "Bottom half: play notes; top half: a step with the notes played last",
        Some(Views::Drum) => "Bottom left: drum pads; bottom right: pages / loop; top 4 rows: 32 steps",
        Some(Views::Drum4) => "Each 2 rows are the 16 steps of one of 4 drum sounds",
        Some(Views::Drum8) => "Each row is the 8 steps of one of 8 drum sounds",
        _ => "Rows 1-2: the clips; 3-4: the drum pads; 5-8: the steps of the selected pad",
    }
}

fn scene_row(ctx: &Ctx, text: &str) -> Help {
    Help::new(text).with_long(lines(&[&crate::framework::help::scene_names(ctx, 8), "Shift + scene: the alternative launch; Select: select it; Delete: delete it; Duplicate: duplicate it."]))
}

fn resolution_row(ctx: &Ctx) -> String {
    let selected = ctx.active_view().and_then(|view| view.try_borrow().ok().and_then(|view| view.sequencer_resolution_index(ctx)));
    match selected {
        Some(index) => format!("The resolution of the steps (now {})", crate::daw::constants::Resolution::name_at(index)),
        None => "The resolution of the steps (1/4 .. 1/32t)".to_string(),
    }
}
