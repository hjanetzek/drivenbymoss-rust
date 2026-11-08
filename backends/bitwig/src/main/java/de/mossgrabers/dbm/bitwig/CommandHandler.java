// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Action;
import com.bitwig.extension.controller.api.Channel;
import com.bitwig.extension.controller.api.ClipLauncherSlot;
import com.bitwig.extension.controller.api.CueMarker;
import com.bitwig.extension.controller.api.CursorRemoteControlsPage;
import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.Parameter;
import com.bitwig.extension.controller.api.PinnableCursorClip;
import com.bitwig.extension.controller.api.PinnableCursorDevice;
import com.bitwig.extension.controller.api.Scene;
import com.bitwig.extension.controller.api.Send;
import com.bitwig.extension.controller.api.SettableBeatTimeValue;
import com.bitwig.extension.controller.api.SettableRangedValue;
import com.bitwig.extension.controller.api.Track;

import java.lang.foreign.MemorySegment;
import java.util.EnumSet;
import java.util.Set;
import java.util.function.Consumer;

import de.mossgrabers.dbm.bitwig.Protocol.Cmd;
import de.mossgrabers.dbm.bitwig.ModelReporter.Entity;
import de.mossgrabers.dbm.bitwig.ModelReporter.Family;


/**
 * Executes the commands of the core with the Bitwig API: transport, tracks, master, devices,
 * clips, browser, markers, note repeat and the commands of protocol v2 (application, arranger,
 * mixer, project). The commands arrive in the binary form (doc/TYPED_PROTOCOL_PLAN.md §2.7), one
 * buffer per call of the core; {@link #processCommands(MemorySegment)} dispatches each record by
 * its command with an exhaustive switch over {@link Cmd} (a new command is a compile error until
 * it has a case). The commands which Bitwig cannot execute are listed in one case and in
 * {@link #UNHANDLED} (the bitset passed to the core, kept exact by the coverage test).
 */
public final class CommandHandler
{
    /**
     * The commands this extension does not execute (Reaper's): passed to the core
     * (dbm_set_command_callback), which logs such a command once. The coverage test
     * (CommandCoverageTest) keeps the set exact.
     */
    public static final Set<Cmd> UNHANDLED          = EnumSet.of (Cmd.METRO_VOL_SET, Cmd.METRO_VOL_STEP, Cmd.TRACK_IN_QUANT_RESOLUTION, Cmd.TRACK_RECORD_CLIP, Cmd.TRACK_PIN, Cmd.TRACK_OVERDUB, Cmd.MASTER_CROSSFADER, Cmd.CLIP_NOTE_TOGGLE, Cmd.CLIP_NOTE_SET, Cmd.CLIP_NOTE_UPDATE, Cmd.CLIP_NOTE_CLEAR, Cmd.CLIP_NOTE_MOVE_Y, Cmd.CLIP_NOTE_CLEAR_ROW, Cmd.CLIP_NOTE_CLEAR_POSITION, Cmd.GROOVE_ACTIVE, Cmd.GROOVE_AMOUNT);

    /** The panel layouts in the order of the core's layout index. */
    private static final String []    PANEL_LAYOUTS      = { "ARRANGE", "MIX", "EDIT", "PLAY" };

    /** The delay before the values of a commanded item are reported again, in milliseconds. */
    private static final int          REPORT_AGAIN_DELAY = 300;

    private final BitwigModel         model;
    private final ModelReporter       reporter;
    private final Consumer<String>    unhandled;
    /** The view of the records of a command buffer, reused. */
    private final Protocol.View       view               = new Protocol.View ();
    /** The IDs of unknown commands which were logged. */
    private final Set<Integer>        unknownIds         = new java.util.HashSet<> ();


    /**
     * Constructor.
     *
     * @param model The Bitwig objects
     * @param reporter The reporter, for the refresh command and the reports after a command
     * @param unhandled Called with a description of each command which is not supported
     */
    public CommandHandler (final BitwigModel model, final ModelReporter reporter, final Consumer<String> unhandled)
    {
        this.model = model;
        this.reporter = reporter;
        this.unhandled = unhandled;
    }


    /** The commands of the clip launcher navigator: handled only with a navigator. */
    public static final Set<Cmd>      NAVIGATOR          = EnumSet.of (Cmd.NAVIGATOR_STEP, Cmd.NAVIGATOR_SELECT_TRACK);


    /**
     * The bitset of the handled commands for dbm_set_command_callback (word i bit j = ID 0x8000 +
     * 64 * i + j): all commands of the table except {@link #UNHANDLED}, and the navigator's only
     * with a navigator. The core derives the command features of the extension from it
     * (TYPED_PROTOCOL_PLAN §6.6), e.g. clipLauncherNavigator from the navigator's commands.
     *
     * @param withNavigator True if the model setup has the clip launcher navigator
     * @return The words
     */
    public static long [] handledBitset (final boolean withNavigator)
    {
        int maxIndex = 0;
        for (final Cmd cmd: Cmd.values ())
            maxIndex = Math.max (maxIndex, cmd.id & 0x7FFF);
        final long [] words = new long [maxIndex / 64 + 1];
        for (final Cmd cmd: Cmd.values ())
        {
            if (!UNHANDLED.contains (cmd) && (withNavigator || !NAVIGATOR.contains (cmd)))
                words[(cmd.id & 0x7FFF) / 64] |= 1L << (cmd.id & 0x7FFF) % 64;
        }
        return words;
    }


    /**
     * The bitset of the handled commands of the model setup (see {@link #handledBitset(boolean)}).
     *
     * @return The words
     */
    public long [] handledBitset ()
    {
        return handledBitset (this.model.navigator != null);
    }


    /**
     * Execute the commands of one call of the core. The buffer belongs to the core and is only
     * valid during the call (no view of it is kept).
     *
     * @param buffer The buffer of records
     */
    public void processCommands (final MemorySegment buffer)
    {
        final Protocol.View v = this.view;
        if (!v.reset (buffer))
        {
            this.unhandled.accept ("A command buffer with a wrong header");
            return;
        }
        while (v.next ())
        {
            final Cmd cmd = v.cmd ();
            if (cmd == null)
            {
                if (this.unknownIds.add (Integer.valueOf (v.id ())))
                    this.unhandled.accept ("Unknown command 0x" + Integer.toHexString (v.id ()));
                continue;
            }
            this.reportAgain (v, cmd);
            this.handle (v, cmd);
        }
    }


    /**
     * Execute one command.
     *
     * @param v The view of the record
     * @param cmd The command of the record
     */
    private void handle (final Protocol.View v, final Cmd cmd)
    {
        // 'case null' makes the switch exhaustive: every command needs a case
        switch (cmd)
        {
            case null -> throw new IllegalStateException ();

            // --- Transport, time, tempo, automation

            case TRANSPORT_PLAY -> this.model.transport.play ();
            case TRANSPORT_RESTART -> this.model.transport.restart ();
            case TRANSPORT_STOP, STOP_TOGGLE -> this.model.transport.stop ();
            case TRANSPORT_STOP_AND_REWIND ->
            {
                this.model.transport.stop ();
                // Delay the position movement to make sure that the playback is really stopped
                this.model.host.scheduleTask ( () -> this.model.transport.setPosition (0), 100);
            }
            case TRANSPORT_TOGGLE_METRONOME_TICKS -> this.model.transport.isMetronomeTickPlaybackEnabled ().toggle ();
            case TRANSPORT_TOGGLE_PUNCH_IN -> this.model.transport.isPunchInEnabled ().toggle ();
            case TRANSPORT_TOGGLE_PUNCH_OUT -> this.model.transport.isPunchOutEnabled ().toggle ();
            case TRANSPORT_TOGGLE_FILL -> this.model.transport.isFillModeActive ().toggle ();
            case PLAY_TOGGLE -> this.model.transport.togglePlay ();
            case RECORD_TOGGLE -> this.model.transport.record ();
            case REPEAT_TOGGLE -> this.model.transport.isArrangerLoopEnabled ().toggle ();
            case TRANSPORT_METRONOME -> this.model.transport.isMetronomeEnabled ().set (v.boolValue ());
            case TRANSPORT_TOGGLE_METRONOME -> this.model.transport.isMetronomeEnabled ().toggle ();
            case TRANSPORT_METRONOME_TICKS -> this.model.transport.isMetronomeTickPlaybackEnabled ().set (v.boolValue ());
            case TRANSPORT_TOGGLE_PREROLL_METRONOME -> this.model.transport.isMetronomeAudibleDuringPreRoll ().toggle ();
            // MetronomeVolumeParameterImpl: set, inc
            case TRANSPORT_METRONOME_VOLUME -> this.setRanged (v, cmd, this.model.transport.metronomeVolume ());
            // The pre-roll as TransportImpl.setPrerollMeasures of the Java version (none, 1, 2, 4 bars)
            case INI_FILE_PREROLL_MEASURES ->
            {
                final int bars = v.intValue ();
                if (bars >= 0 && bars < BitwigModel.PREROLLS.length && BitwigModel.PREROLLS[bars] != null)
                    this.model.transport.preRoll ().set (BitwigModel.PREROLLS[bars]);
                else
                    this.notHandled (v, cmd);
            }
            case TRANSPORT_PUNCH_IN -> this.model.transport.isPunchInEnabled ().set (v.boolValue ());
            case TRANSPORT_PUNCH_OUT -> this.model.transport.isPunchOutEnabled ().set (v.boolValue ());
            case TRANSPORT_FILL -> this.model.transport.isFillModeActive ().set (v.boolValue ());
            case TRANSPORT_END -> this.invokeAction ("jump_to_end_of_arrangement");
            case TRANSPORT_TAP_TEMPO -> this.model.transport.tapTempo ();
            case TRANSPORT_OVERDUB_LAUNCHER -> this.model.transport.isClipLauncherOverdubEnabled ().set (v.boolValue ());
            case TRANSPORT_OVERDUB_ARRANGER -> this.model.transport.isArrangerOverdubEnabled ().set (v.boolValue ());
            // TransportImpl.getCrossfadeParameter (a ParameterImpl)
            case TRANSPORT_CROSSFADE -> this.setParameter (v, this.model.transport.crossfade ());
            case TEMPO_STEP -> this.changeTempo (v.step ());
            case TEMPO_SET -> this.model.transport.tempo ().value ().setRaw (v.doubleValue ());
            // The play position and the loop in seconds (the Reaper commands)
            case TIME_SET, TIME_SET_NO_SNAP -> this.setPosition (this.secondsToBeats (v.doubleValue ()));
            case TIME_SET_INT -> this.setPosition (this.secondsToBeats (v.intValue ()));
            case TIME_LOOP_START -> this.model.transport.arrangerLoopStart ().set (this.secondsToBeats (v.doubleValue ()));
            case TIME_LOOP_LENGTH -> this.model.transport.arrangerLoopDuration ().set (this.secondsToBeats (v.doubleValue ()));
            // TransportImpl.setTimeSignature
            case TIME_SIGNATURE ->
            {
                final int numerator = v.pairValue (0);
                final int denominator = v.pairValue (1);
                if (numerator > 0 && denominator > 0)
                {
                    this.model.transport.timeSignature ().numerator ().set (numerator);
                    this.model.transport.timeSignature ().denominator ().set (denominator);
                }
            }
            // The play position and the loop in quarter notes, as TransportImpl of the Java version:
            // there is no snapping, the loop start keeps the duration
            case BEATS_POSITION, BEATS_POSITION_NO_SNAP -> this.setPosition (v.doubleValue ());
            case BEATS_LOOP_START -> this.model.transport.arrangerLoopStart ().set (v.doubleValue ());
            case BEATS_LOOP_LENGTH -> this.model.transport.arrangerLoopDuration ().set (v.doubleValue ());
            case AUTOMATION_MODE -> this.setAutomationWriteMode (v.intValue ());
            case AUTOMATION_TOGGLE_ARRANGER -> this.model.transport.toggleWriteArrangerAutomation ();
            case AUTOMATION_TOGGLE_LAUNCHER -> this.model.transport.toggleWriteClipLauncherAutomation ();
            case AUTOMATION_RESET_OVERRIDES -> this.model.transport.resetAutomationOverrides ();
            case QUANTIZE ->
            {
                // As CursorClipImpl.quantize
                final double amount = v.doubleValue ();
                if (amount >= 0.000001 && amount <= 1)
                    this.model.cursorClip.clip.quantize (amount);
            }
            case UNDO -> this.model.application.undo ();
            case REDO -> this.model.application.redo ();
            // ApplicationImpl.invokeAction: a Bitwig action ID; IDs which Bitwig does not know
            // (e.g. Reaper's) are ignored as in the Java version
            case ACTION ->
            {
                final Action action = this.model.application.getAction (v.text ().trim ());
                if (action != null)
                    action.invoke ();
            }

            // --- Application, arranger, mixer, project

            // ApplicationImpl.setEngineActive
            case PROJECT_ENGINE ->
            {
                if (v.boolValue ())
                    this.model.application.activateEngine ();
                else
                    this.model.application.deactivateEngine ();
            }
            case APPLICATION_LAYOUT ->
            {
                final int index = v.intValue ();
                if (index >= 0 && index < PANEL_LAYOUTS.length)
                    this.model.application.setPanelLayout (PANEL_LAYOUTS[index]);
            }
            // ApplicationImpl.previousPanelLayout / nextPanelLayout
            case APPLICATION_LAYOUT_STEP ->
            {
                if (v.dir () == Protocol.Dir.NEXT)
                    this.model.application.nextPanelLayout ();
                else
                    this.model.application.previousPanelLayout ();
            }
            case APPLICATION_TOGGLE_NOTE_EDITOR -> this.model.application.toggleNoteEditor ();
            case APPLICATION_TOGGLE_AUTOMATION_EDITOR -> this.model.application.toggleAutomationEditor ();
            case APPLICATION_TOGGLE_DEVICES -> this.model.application.toggleDevices ();
            case APPLICATION_TOGGLE_INSPECTOR -> this.model.application.toggleInspector ();
            case APPLICATION_TOGGLE_MIXER -> this.model.application.toggleMixer ();
            case APPLICATION_TOGGLE_FULL_SCREEN -> this.model.application.toggleFullScreen ();
            case APPLICATION_TOGGLE_BROWSER -> this.model.application.toggleBrowserVisibility ();
            case APPLICATION_DUPLICATE -> this.model.application.duplicate ();
            case APPLICATION_DELETE_SELECTION -> this.model.application.remove ();
            case APPLICATION_ARROW_LEFT -> this.model.application.arrowKeyLeft ();
            case APPLICATION_ARROW_UP -> this.model.application.arrowKeyUp ();
            case APPLICATION_ARROW_RIGHT -> this.model.application.arrowKeyRight ();
            case APPLICATION_ARROW_DOWN -> this.model.application.arrowKeyDown ();
            case APPLICATION_ZOOM_OUT -> this.model.application.zoomOut ();
            case APPLICATION_ZOOM_IN -> this.model.application.zoomIn ();
            case APPLICATION_DEC_TRACK_HEIGHT -> this.model.arranger.zoomOutLaneHeightsAll ();
            case APPLICATION_INC_TRACK_HEIGHT -> this.model.arranger.zoomInLaneHeightsAll ();
            case APPLICATION_ENTER -> this.model.application.enter ();
            case APPLICATION_ESCAPE -> this.model.application.escape ();
            case APPLICATION_SLICE_TO_SAMPLER -> this.invokeAction ("slice_to_multi_sampler_track");
            case APPLICATION_SLICE_TO_DRUM_MACHINE -> this.invokeAction ("slice_to_drum_track");
            // ApplicationImpl.setRecordQuantizationGrid
            case APPLICATION_RECORD_QUANTIZATION -> this.model.application.recordQuantizationGrid ().set (v.text ().trim ());
            // ApplicationImpl.toggleRecordQuantizationNoteLength
            case APPLICATION_TOGGLE_RECORD_QUANTIZATION_NOTE_LENGTH -> this.model.application.recordQuantizeNoteLength ().toggle ();
            case ARRANGER_TOGGLE_CUE_MARKERS -> this.model.arranger.areCueMarkersVisible ().toggle ();
            case ARRANGER_TOGGLE_PLAYBACK_FOLLOW -> this.model.arranger.isPlaybackFollowEnabled ().toggle ();
            case ARRANGER_TOGGLE_TRACK_ROW_HEIGHT -> this.model.arranger.hasDoubleRowTrackHeight ().toggle ();
            case ARRANGER_TOGGLE_CLIP_LAUNCHER -> this.model.arranger.isClipLauncherVisible ().toggle ();
            case ARRANGER_TOGGLE_TIME_LINE -> this.model.arranger.isTimelineVisible ().toggle ();
            case ARRANGER_TOGGLE_IO_SECTION -> this.model.arranger.isIoSectionVisible ().toggle ();
            case ARRANGER_TOGGLE_EFFECT_TRACKS -> this.model.arranger.areEffectTracksVisible ().toggle ();
            case MIXER_TOGGLE_CLIP_LAUNCHER_SECTION -> this.model.mixer.isClipLauncherSectionVisible ().toggle ();
            case MIXER_TOGGLE_CROSS_FADE_SECTION -> this.model.mixer.isCrossFadeSectionVisible ().toggle ();
            case MIXER_TOGGLE_DEVICE_SECTION -> this.model.mixer.isDeviceSectionVisible ().toggle ();
            case MIXER_TOGGLE_IO_SECTION -> this.model.mixer.isIoSectionVisible ().toggle ();
            case MIXER_TOGGLE_METER_SECTION -> this.model.mixer.isMeterSectionVisible ().toggle ();
            case MIXER_TOGGLE_SENDS_SECTION -> this.model.mixer.isSendSectionVisible ().toggle ();
            case PROJECT_PREVIOUS -> this.model.application.previousProject ();
            case PROJECT_NEXT -> this.model.application.nextProject ();
            case PROJECT_CREATE_SCENE_FROM_PLAYING_LAUNCHER_CLIPS -> this.model.project.createSceneFromPlayingLauncherClips ();
            case PROJECT_CREATE_SCENE -> this.model.project.createScene ();
            case PROJECT_SAVE -> this.invokeAction ("Save");
            case PROJECT_LOAD -> this.invokeAction ("Open");
            // ProjectImpl.clearSolo / clearMute
            case PROJECT_UNSOLO_ALL -> this.model.project.unsoloAll ();
            case PROJECT_UNMUTE_ALL -> this.model.project.unmuteAll ();
            // ProjectImpl: the cue volume and mix parameters (set, inc, reset, touch)
            case PROJECT_CUE ->
            {
                final Parameter parameter = v.param () == Protocol.CueParam.MIX ? this.model.project.cueMix () : this.model.project.cueVolume ();
                if (v.op () == Protocol.OP_INDICATE)
                    this.notHandled (v, cmd);
                else
                    this.setParameter (v, parameter);
            }

            // --- Channels and banks

            case CHANNEL_COLOR ->
            {
                final Channel channel = this.channel (v, cmd);
                if (channel != null)
                    channel.color ().set (v.colorPart (0) / 255f, v.colorPart (1) / 255f, v.colorPart (2) / 255f);
            }
            case CHANNEL_MUTE ->
            {
                final Channel channel = this.channel (v, cmd);
                if (channel != null)
                    channel.mute ().set (v.boolValue ());
            }
            case CHANNEL_SOLO ->
            {
                final Channel channel = this.channel (v, cmd);
                if (channel != null)
                    channel.solo ().set (v.boolValue ());
            }
            // ChannelImpl.toggleSolo
            case CHANNEL_SOLO_TOGGLE ->
            {
                final Channel channel = this.channel (v, cmd);
                if (channel != null)
                    channel.solo ().toggleUsingPreferences (false);
            }
            // ChannelImpl.isActivated
            case CHANNEL_ACTIVE ->
            {
                final Channel channel = this.channel (v, cmd);
                if (channel != null)
                    channel.isActivated ().set (v.boolValue ());
            }
            case CHANNEL_REMOVE ->
            {
                final Track track = this.bankTrack (v, cmd);
                if (track != null)
                    track.deleteObject ();
            }
            case CHANNEL_DUPLICATE ->
            {
                final Track track = this.bankTrack (v, cmd);
                if (track != null)
                    track.duplicateObject ();
            }
            case CHANNEL_SELECT -> this.selectChannel (v, cmd, v.boolValue ());
            case CHANNEL_SELECT_IN_EDITOR -> this.selectChannel (v, cmd, true);
            case CHANNEL_VOLUME ->
            {
                final Channel channel = this.channel (v, cmd);
                if (channel != null)
                    this.setParameter (v, channel.volume ());
            }
            case CHANNEL_PAN ->
            {
                final Channel channel = this.channel (v, cmd);
                if (channel != null)
                    this.setParameter (v, channel.pan ());
            }
            case CHANNEL_SEND_VOLUME ->
            {
                final Send send = this.send (v, cmd);
                if (send != null)
                    this.setParameter (v, send);
            }
            // Toggle the send (the value is the send volume, which is ignored)
            case CHANNEL_SEND_ACTIVE ->
            {
                final Send send = this.send (v, cmd);
                if (send != null)
                    send.isEnabled ().toggle ();
            }
            // ChannelImpl.toggleMultiSelect: no multi-selection in the Bitwig API
            case TRACK_TOGGLE_MULTI_SELECT -> this.track (v, cmd, false);
            case TRACK_NAME ->
            {
                final Track track = this.track (v, cmd, true);
                if (track != null)
                    track.name ().set (v.text ());
            }
            case TRACK_GROUP_EXPANDED ->
            {
                final Track track = this.track (v, cmd, false);
                if (track != null)
                    track.isGroupExpanded ().set (v.boolValue ());
            }
            case TRACK_REC_ARM ->
            {
                final Track track = this.track (v, cmd, true);
                if (track != null)
                    track.arm ().set (v.boolValue ());
            }
            case TRACK_MONITOR ->
            {
                final Track track = this.track (v, cmd, true);
                if (track != null)
                    track.monitorMode ().set (v.intValue () > 0 ? "ON" : "OFF");
            }
            case TRACK_AUTO_MONITOR ->
            {
                final Track track = this.track (v, cmd, true);
                if (track != null)
                    track.monitorMode ().set (v.boolValue () ? "AUTO" : "OFF");
            }
            case TRACK_STOP ->
            {
                final Track track = this.track (v, cmd, false);
                if (track != null)
                    track.stop ();
            }
            case TRACK_STOP_ALT ->
            {
                final Track track = this.track (v, cmd, false);
                if (track != null)
                    track.stopAlt ();
            }
            case TRACK_RETURN_TO_ARRANGEMENT ->
            {
                final Track track = this.track (v, cmd, false);
                if (track != null)
                    track.returnToArrangement ();
            }
            // createNewLauncherClip needs the absolute slot (as TrackImpl.createClip)
            case TRACK_CREATE_CLIP ->
            {
                final Track track = this.track (v, cmd, false);
                if (track != null && v.slot () >= 0)
                    track.createNewLauncherClip (v.slot (), v.intValue ());
            }
            case TRACK_MOVE_PREV, TRACK_MOVE_NEXT ->
            {
                final Track track = this.track (v, cmd, false);
                if (track != null)
                    this.moveTrack (track, v.chIndex (), cmd == Cmd.TRACK_MOVE_PREV ? -1 : 1);
            }
            // AbstractItemBankImpl.scrollTo: only scrolls (the core pages with "bank track")
            case TRACK_SCROLL_TO ->
            {
                final Track track = this.track (v, cmd, true);
                if (track != null)
                    (v.chProcessor () == Protocol.Processor.FX_TRACK ? this.model.effectTrackBank : this.model.trackBank).scrollPosition ().set (v.chIndex ());
            }
            // CursorTrackImpl.setPinned, togglePinned, selectPrevious, selectNext
            case TRACK_PINNED_TOGGLE -> this.model.cursorTrack.isPinned ().toggle ();
            case TRACK_PINNED -> this.model.cursorTrack.isPinned ().set (v.boolValue ());
            case TRACK_SELECT_PREVIOUS -> this.model.cursorTrack.selectPrevious ();
            case TRACK_SELECT_NEXT -> this.model.cursorTrack.selectNext ();
            case TRACK_ADD -> this.addTrack (v.textList ());
            case BANK_INDICATE -> this.indicateBank (v, cmd);
            case BANK_SCROLL ->
            {
                // Scroll the track or scene window to the core's bank offset (host paging)
                final int offset = v.intValue ();
                switch (v.bank ())
                {
                    case Protocol.Processor.TRACK -> this.model.scrollTrackWindow (offset);
                    case Protocol.Processor.SCENE -> this.model.sceneBank.scrollPosition ().set (Math.max (0, offset));
                    case Protocol.Processor.FX_TRACK -> this.model.effectTrackBank.scrollPosition ().set (Math.max (0, offset));
                    default -> this.notHandled (v, cmd);
                }
            }

            // --- Slots and scenes

            case SLOT_ACTION ->
            {
                final ClipLauncherSlot slot = this.slot (v, cmd);
                if (slot == null)
                    return;
                switch (v.action ())
                {
                    case Protocol.SlotAction.LAUNCH -> slot.launch ();
                    case Protocol.SlotAction.LAUNCH_ALT -> slot.launchAlt ();
                    case Protocol.SlotAction.LAUNCH_RELEASE -> slot.launchRelease ();
                    case Protocol.SlotAction.LAUNCH_RELEASE_ALT -> slot.launchReleaseAlt ();
                    case Protocol.SlotAction.RECORD -> slot.record ();
                    case Protocol.SlotAction.SELECT -> slot.select ();
                    case Protocol.SlotAction.REMOVE -> slot.deleteObject ();
                    case Protocol.SlotAction.DUPLICATE -> slot.duplicateClip ();
                    default -> this.notHandled (v, cmd);
                }
            }
            case SLOT_COLOR ->
            {
                final ClipLauncherSlot slot = this.slot (v, cmd);
                if (slot != null)
                    slot.color ().set (v.colorPart (0) / 255f, v.colorPart (1) / 255f, v.colorPart (2) / 255f);
            }
            case SLOT_PASTE ->
            {
                final ClipLauncherSlot slot = this.slot (v, cmd);
                if (slot == null)
                    return;
                // The source: track and slot
                final Track sourceTrack = this.model.getWindowTrack (v.pairValue (0));
                final ClipLauncherSlot source = sourceTrack == null ? null : this.model.getSlot (sourceTrack, v.pairValue (1));
                if (source != null)
                    slot.replaceInsertionPoint ().copySlotsOrScenes (source);
            }
            case SLOT_INSERT_FILE ->
            {
                final ClipLauncherSlot slot = this.slot (v, cmd);
                if (slot != null)
                    slot.replaceInsertionPoint ().insertFile (v.text ());
            }
            case SCENE_ACTION ->
            {
                final Scene scene = this.scene (v, cmd);
                if (scene == null)
                    return;
                switch (v.action ())
                {
                    case Protocol.SceneAction.LAUNCH -> scene.launch ();
                    case Protocol.SceneAction.LAUNCH_ALT -> scene.launchAlt ();
                    case Protocol.SceneAction.LAUNCH_RELEASE -> scene.launchRelease ();
                    case Protocol.SceneAction.LAUNCH_RELEASE_ALT -> scene.launchReleaseAlt ();
                    case Protocol.SceneAction.SELECT -> scene.selectInEditor ();
                    case Protocol.SceneAction.REMOVE -> scene.deleteObject ();
                    case Protocol.SceneAction.DUPLICATE -> scene.nextSceneInsertionPoint ().copySlotsOrScenes (scene);
                    default -> this.notHandled (v, cmd);
                }
            }
            case SCENE_NAME ->
            {
                final Scene scene = this.scene (v, cmd);
                if (scene != null)
                    scene.name ().set (v.text ());
            }
            case SCENE_COLOR ->
            {
                final Scene scene = this.scene (v, cmd);
                if (scene != null)
                    scene.color ().set (v.colorPart (0) / 255f, v.colorPart (1) / 255f, v.colorPart (2) / 255f);
            }
            case SCENE_STOP_ALL -> this.model.sceneBank.stop ();
            case SCENE_STOP_ALL_ALT -> this.model.sceneBank.stopAlt ();

            // --- Devices

            // The sibling of the window (DeviceImpl)
            case DEVICE_SIBLING ->
            {
                final int index = v.sibling ();
                if (index < 0 || index >= BitwigModel.NUM_SIBLINGS || !deviceAction (this.model.siblings.getItemAt (index), v.action ()))
                    this.notHandled (v, cmd);
            }
            case DEVICE_ACTION ->
            {
                final Device device = this.device (v, cmd);
                if (device != null && !deviceAction (device, v.action ()))
                    this.notHandled (v, cmd);
            }
            case DEVICE_TOGGLE_WINDOW ->
            {
                final Device device = this.device (v, cmd);
                if (device != null)
                    device.isWindowOpen ().toggle ();
            }
            case DEVICE_TOGGLE_EXPANDED ->
            {
                final Device device = this.device (v, cmd);
                if (device != null)
                    device.isExpanded ().toggle ();
            }
            case DEVICE_TOGGLE_PARAMETER_PAGE_SECTION ->
            {
                final Device device = this.device (v, cmd);
                if (device != null)
                    device.isRemoteControlsSectionVisible ().toggle ();
            }
            // The device at a position of the chain (the cursor device's chain)
            case DEVICE_INDEX_BYPASS, DEVICE_INDEX_REMOVE, DEVICE_INDEX_DUPLICATE ->
            {
                final Device device = v.dev () == Protocol.Processor.DEVICE ? this.findSibling (v.device ()) : null;
                if (device == null)
                    this.notHandled (v, cmd);
                // As Reaper's DeviceProcessor: the value is the new bypass state
                else if (cmd == Cmd.DEVICE_INDEX_BYPASS)
                    device.isEnabled ().set (!v.boolValue ());
                else if (cmd == Cmd.DEVICE_INDEX_REMOVE)
                    device.deleteObject ();
                else
                    device.duplicateObject ();
            }
            // The core moves its cursor device, which is Bitwig's
            case DEVICE_INDEX_MOVE_PREV -> this.moveCursorDevice (-1);
            case DEVICE_INDEX_MOVE_NEXT -> this.moveCursorDevice (1);
            case DEVICE_SELECTED ->
            {
                final int index = v.intValue ();
                if (v.dev () != Protocol.Processor.DEVICE)
                    this.notHandled (v, cmd);
                else if (index >= 0 && index < BitwigModel.NUM_SIBLINGS)
                    this.model.siblings.getItemAt (index).selectInEditor ();
            }
            case DEVICE_WINDOW -> this.model.cursorDevice.isWindowOpen ().set (v.boolValue ());
            case DEVICE_EXPAND -> this.model.cursorDevice.isExpanded ().set (v.boolValue ());
            case DEVICE_PAGE_STEP ->
            {
                if (v.dir () == Protocol.Dir.NEXT)
                    this.model.siblings.scrollPageForwards ();
                else
                    this.model.siblings.scrollPageBackwards ();
            }
            case DEVICE_STEP ->
            {
                if (v.dir () == Protocol.Dir.NEXT)
                    this.model.cursorDevice.selectNext ();
                else
                    this.model.cursorDevice.selectPrevious ();
            }
            case DEVICE_PINNED ->
            {
                if (v.dev () == Protocol.Processor.DEVICE)
                    this.model.cursorDevice.isPinned ().set (v.boolValue ());
                else
                    this.notHandled (v, cmd);
            }
            // Reaper's device creation names: Bitwig has no such names (ChannelImpl.addDevice of
            // the Java version uses the device metadata), ignored
            case DEVICE_ADD ->
            {
                // Intentionally empty
            }
            // The result of Reaper's preset browser, only sent without the host's browser (which
            // Bitwig always reports)
            case DEVICE_PRESET ->
            {
                // Intentionally empty
            }
            // Show the page in Bitwig as well (ParameterPageBankImpl.scrollTo)
            case DEVICE_PARAMETER_PAGE -> this.selectParameterPage (v, cmd);
            case DRUM_CLEAR_MUTE, DRUM_CLEAR_SOLO ->
            {
                final SpecificDevice device = this.drumPadDevice (v, cmd);
                if (device == null)
                    return;
                if (cmd == Cmd.DRUM_CLEAR_MUTE)
                    device.drumPadBank.clearMutedPads ();
                else
                    device.drumPadBank.clearSoloedPads ();
            }
            // ChannelImpl.addEqualizerDevice
            case EQ_ADD ->
            {
                if (this.model.eqDevice == null)
                    this.notHandled (v, cmd);
                else
                    this.model.cursorTrack.endOfDeviceChainInsertionPoint ().insertBitwigDevice (EqualizerDevice.ID_BITWIG_EQ_PLUS);
            }
            // The type of a band as the index of the Reaper EQ
            case EQ_BAND_TYPE ->
            {
                if (this.model.eqDevice == null || !this.model.eqDevice.setType (v.band (), v.intValue ()))
                    this.notHandled (v, cmd);
            }

            // --- Parameters

            case PARAM_VALUE, PARAM_TOUCH, PARAM_INDICATE ->
            {
                final Parameter parameter = this.parameter (v, cmd, v.dev (), v.param ());
                if (parameter == null)
                    return;
                if (cmd == Cmd.PARAM_TOUCH)
                    parameter.touch (v.boolValue ());
                else if (cmd == Cmd.PARAM_INDICATE)
                    parameter.setIndication (v.boolValue ());
                else
                    this.setParameter (v, parameter);
            }
            // The remote controls of the cursor track (the core's cursor track is Bitwig's)
            case TRACK_PARAM_VALUE ->
            {
                if (this.model.getTrack (v.track ()) == null)
                {
                    this.notHandled (v, cmd);
                    return;
                }
                final Parameter parameter = this.parameter (v, cmd, Protocol.Processor.TRACK, v.param ());
                if (parameter != null)
                    this.setParameter (v, parameter);
            }

            // --- The cursor clip and the step grids (CursorClipImpl of the Java version)

            case CLIP_PINNED ->
            {
                for (final BitwigNoteClip noteClip: this.model.noteClips)
                    noteClip.clip.isPinned ().set (v.boolValue ());
            }
            case CLIP_NAME -> this.model.cursorClip.clip.setName (v.text ());
            case CLIP_COLOR -> this.model.cursorClip.clip.color ().set (v.colorPart (0) / 255f, v.colorPart (1) / 255f, v.colorPart (2) / 255f);
            // "…/inc": CursorClipImpl.changePlayStart etc. (getPlayStart ().inc (frac))
            case CLIP_START -> this.setBeatTime (v, cmd, this.model.cursorClip.clip.getPlayStart ());
            case CLIP_END -> this.setBeatTime (v, cmd, this.model.cursorClip.clip.getPlayStop ());
            case CLIP_LOOP_START -> this.setBeatTime (v, cmd, this.model.cursorClip.clip.getLoopStart ());
            case CLIP_LOOP_LENGTH -> this.setBeatTime (v, cmd, this.model.cursorClip.clip.getLoopLength ());
            case CLIP_LOOP -> this.model.cursorClip.clip.isLoopEnabled ().set (v.boolValue ());
            case CLIP_SHUFFLE -> this.model.cursorClip.clip.getShuffle ().set (v.boolValue ());
            // "accent/immediately": CursorClipImpl.resetAccent (setImmediately)
            case CLIP_ACCENT -> this.setRanged (v, cmd, this.model.cursorClip.clip.getAccent ());
            case CLIP_DUPLICATE -> this.model.cursorClip.clip.duplicate ();
            case CLIP_DUPLICATE_CONTENT -> this.model.cursorClip.clip.duplicateContent ();
            case CLIP_TRANSPOSE ->
            {
                // Workaround Bitwig crashing when it is not a MIDI clip (CursorClipImpl)
                final PinnableCursorClip clip = this.model.cursorClip.clip;
                if (clip.getTrack ().canHoldNoteData ().get ())
                    clip.transpose (v.intValue ());
            }
            // CursorClipImpl.clearAll (the core's command without a host step grid)
            case CLIP_CLEAR -> this.model.cursorClip.clip.clearSteps ();
            case GRID_STEP_SIZE, GRID_STEP_TOGGLE, GRID_STEP_SET, GRID_STEP_SET_FROM, GRID_STEP_UPDATE, GRID_STEP_CLEAR, GRID_STEP_MOVE_Y, GRID_STEP_ATTRIBUTE, GRID_CLEAR_ALL, GRID_CLEAR_ROW, GRID_CLEAR_COLUMN, GRID_SCROLL, GRID_PAGE -> this.handleGrid (v, cmd);

            // --- Browser (BrowserImpl of the Java version)

            case BROWSER_CONTENT_TYPE, BROWSER_PREVIEW, BROWSER_COLUMN_STEP, BROWSER_COLUMN_PAGE, BROWSER_COLUMN_RESET, BROWSER_REPLACE_DEVICE, BROWSER_REPLACE_SLOT, BROWSER_REPLACE_PAD, BROWSER_ADD_TO_TRACK, BROWSER_ADD_TO_MASTER, BROWSER_INSERT_BEFORE, BROWSER_INSERT_AFTER, BROWSER_STOP, BROWSER_RESULT_STEP, BROWSER_RESULT_PAGE -> this.handleBrowser (v, cmd);

            // --- Markers (MarkerBankImpl and MarkerImpl of the Java version)

            case MARKER_ACTION ->
            {
                final int index = v.marker ();
                if (this.model.markerBank == null || index < 0 || index >= BitwigModel.NUM_MARKERS)
                {
                    this.notHandled (v, cmd);
                    return;
                }
                final CueMarker marker = this.model.markerBank.getItemAt (index);
                switch (v.action ())
                {
                    // The controllers of the Java version launch quantized
                    case Protocol.MarkerAction.LAUNCH -> marker.launch (true);
                    case Protocol.MarkerAction.SELECT -> this.setPosition (marker.position ().get ());
                    case Protocol.MarkerAction.REMOVE -> marker.deleteObject ();
                    default -> this.notHandled (v, cmd);
                }
            }
            case MARKER_ADD -> this.model.transport.addCueMarkerAtPlaybackPosition ();
            case MARKER_PREVIOUS -> this.findAndSelectMarker (false);
            case MARKER_NEXT -> this.findAndSelectMarker (true);

            // --- Navigator, refresh, update hints

            // The clip launcher navigator (host feature clipLauncherNavigator)
            case NAVIGATOR_STEP ->
            {
                if (this.model.navigator == null)
                    this.notHandled (v, cmd);
                else
                    this.model.navigator.step (v.target (), v.dir () == Protocol.Dir.NEXT);
            }
            case NAVIGATOR_SELECT_TRACK ->
            {
                if (this.model.navigator == null)
                    this.notHandled (v, cmd);
                else
                    this.model.navigator.selectTrack (v.intValue ());
            }
            case REFRESH -> this.reporter.requestDump ();
            // Hints for Reaper's updates, nothing to do
            case DELAY_UPDATES, ENABLE_UPDATES ->
            {
                // Intentionally empty
            }

            // --- Note repeat (the arpeggiator of the default note input), groove

            case NOTE_REPEAT_ACTIVE, NOTE_REPEAT_RATE, NOTE_REPEAT_NOTE_LENGTH, NOTE_REPEAT_VELOCITY, NOTE_REPEAT_MODE -> this.handleNoteRepeat (v, cmd);
            // The groove parameters, as GrooveImpl of the Java version
            case GROOVE_PARAM ->
            {
                final int index = v.param ();
                if (index < 0 || index >= this.model.grooveParameters.length)
                    this.notHandled (v, cmd);
                else
                    this.setParameter (v, this.model.grooveParameters[index]);
            }

            // --- The commands of Reaper which Bitwig does not have (UNHANDLED)

            case METRO_VOL_SET, METRO_VOL_STEP, TRACK_IN_QUANT_RESOLUTION, TRACK_RECORD_CLIP, TRACK_PIN, TRACK_OVERDUB, MASTER_CROSSFADER, CLIP_NOTE_TOGGLE, CLIP_NOTE_SET, CLIP_NOTE_UPDATE, CLIP_NOTE_CLEAR, CLIP_NOTE_MOVE_Y, CLIP_NOTE_CLEAR_ROW, CLIP_NOTE_CLEAR_POSITION, GROOVE_ACTIVE, GROOVE_AMOUNT -> this.notHandled (v, cmd);
        }
    }


    /**
     * Log a command which is not handled (once per form, see the consumer).
     */
    private void notHandled (final Protocol.View v, final Cmd cmd)
    {
        final StringBuilder text = new StringBuilder (cmd.template).append (" (").append (cmd.camelName);
        final String [] slots = cmd.slots.isEmpty () ? new String [0] : cmd.slots.split (",");
        for (int i = 0; i < slots.length; i++)
            text.append (i == 0 ? ": " : ", ").append (slots[i]).append ('=').append (v.signedIdx (i));
        this.unhandled.accept (text.append (')').toString ());
    }


    // ------------------------------------------------------------------------------------------
    // Report again

    /**
     * A command which sets a value: report the values of the item again a bit later (see
     * ModelReporter.forgetLater), so that the core gets Bitwig's value even if it did not change.
     * Only the item of the command: a track, the master, the crossfader, or a parameter, pad or
     * layer of a device or the project. As in the text protocol, only the commands with a number
     * value (and the resets).
     */
    private void reportAgain (final Protocol.View v, final Cmd cmd)
    {
        switch (cmd)
        {
            case CHANNEL_MUTE, CHANNEL_SOLO, CHANNEL_ACTIVE, CHANNEL_SELECT, CHANNEL_VOLUME, CHANNEL_PAN, CHANNEL_SEND_VOLUME, CHANNEL_SEND_ACTIVE, TRACK_TOGGLE_MULTI_SELECT, TRACK_GROUP_EXPANDED, TRACK_REC_ARM, TRACK_MONITOR, TRACK_AUTO_MONITOR, TRACK_IN_QUANT_RESOLUTION, TRACK_CREATE_CLIP, TRACK_PIN, TRACK_OVERDUB ->
            {
                final int index = v.chIndex ();
                switch (v.chKind ())
                {
                    case Protocol.ChanKind.MASTER -> this.forgetLater (Family.CHANNEL, Protocol.ChKind.MASTER, 0, 0);
                    case Protocol.ChanKind.TRACK ->
                    {
                        // Not the effect tracks (as in the text protocol)
                        if (v.chProcessor () == Protocol.Processor.TRACK && index >= 0)
                            this.forgetLater (Family.CHANNEL, Protocol.ChKind.TRACK, index, 0);
                    }
                    case Protocol.ChanKind.PAD ->
                    {
                        if (index < 0)
                            return;
                        switch (v.chProcessor ())
                        {
                            case Protocol.Processor.DRUM -> this.forgetLater (Family.PAD, Protocol.PadKind.DRUM_PAD, Protocol.Dev.DEVICE, index);
                            case Protocol.Processor.PRIMARY -> this.forgetLater (Family.PAD, Protocol.PadKind.PAD, Protocol.Dev.PRIMARY, index);
                            case Protocol.Processor.DEVICE -> this.forgetLater (Family.PAD, Protocol.PadKind.PAD, Protocol.Dev.DEVICE, index);
                            default ->
                            {
                                // Nothing reported
                            }
                        }
                    }
                    case Protocol.ChanKind.LAYER ->
                    {
                        if (index < 0)
                            return;
                        switch (v.chProcessor ())
                        {
                            case Protocol.Processor.PRIMARY -> this.forgetLater (Family.PAD, Protocol.PadKind.LAYER, Protocol.Dev.PRIMARY, index);
                            case Protocol.Processor.DEVICE -> this.forgetLater (Family.PAD, Protocol.PadKind.LAYER, Protocol.Dev.DEVICE, index);
                            default ->
                            {
                                // Nothing reported
                            }
                        }
                    }
                    default ->
                    {
                        // Nothing to correct
                    }
                }
            }
            case TRACK_PARAM_VALUE ->
            {
                if (v.track () >= 0)
                    this.forgetLater (Family.CHANNEL, Protocol.ChKind.TRACK, v.track (), 0);
                if (v.param () >= 0)
                    this.forgetLater (Family.PARAM, Protocol.ParamOwner.TRACK_FX, v.param (), 0);
            }
            case PARAM_VALUE, PARAM_TOUCH, PARAM_INDICATE ->
            {
                final int param = v.param ();
                switch (v.dev ())
                {
                    case Protocol.Processor.MASTER -> this.forgetLater (Family.CHANNEL, Protocol.ChKind.MASTER, 0, 0);
                    case Protocol.Processor.TRACK -> this.forgetParam (Protocol.ParamOwner.TRACK_FX, param);
                    case Protocol.Processor.DEVICE -> this.forgetParam (Protocol.ParamOwner.DEVICE, param);
                    case Protocol.Processor.PRIMARY -> this.forgetParam (Protocol.ParamOwner.PRIMARY, param);
                    case Protocol.Processor.EQ -> this.forgetParam (Protocol.ParamOwner.EQ, param);
                    case Protocol.Processor.PROJECT -> this.forgetParam (Protocol.ParamOwner.PROJECT, param);
                    default ->
                    {
                        // Nothing reported
                    }
                }
            }
            case MASTER_CROSSFADER -> this.forgetLater (Family.CHANNEL, Protocol.ChKind.MASTER, 0, 0);
            case TRANSPORT_CROSSFADE -> this.forgetLater (Family.HOST_PARAM, Protocol.HostParamKind.CROSSFADE, 0, 0);
            default ->
            {
                // Nothing to correct
            }
        }
    }


    private void forgetParam (final int owner, final int param)
    {
        if (param >= 0)
            this.forgetLater (Family.PARAM, owner, param, 0);
    }


    private void forgetLater (final Family family, final int i0, final int i1, final int i2)
    {
        this.reporter.forgetLater (new Entity (family, i0, i1, i2), REPORT_AGAIN_DELAY);
    }


    // ------------------------------------------------------------------------------------------
    // Values

    /**
     * Set a parameter by the operation of the command: set, change (inc), reset
     * (ParameterImpl.resetValue), immediately (setValueImmediatly of the Java version: without
     * Bitwig's takeover mode, e.g. for pads), touch, indicate.
     */
    private void setParameter (final Protocol.View v, final Parameter parameter)
    {
        switch (v.op ())
        {
            case Protocol.OP_RESET -> parameter.reset ();
            case Protocol.OP_TOUCH -> parameter.touch (v.boolValue ());
            case Protocol.OP_INDICATE -> parameter.setIndication (v.boolValue ());
            case Protocol.OP_INC -> parameter.value ().inc (v.doubleValue ());
            case Protocol.OP_IMMEDIATELY -> parameter.value ().setImmediately (v.doubleValue ());
            default -> parameter.value ().set (v.doubleValue ());
        }
    }


    /**
     * Set a ranged value which is not a parameter: set, change (inc), immediately; reset sets 0
     * (as the text protocol did); touch and indicate are not supported.
     */
    private void setRanged (final Protocol.View v, final Cmd cmd, final SettableRangedValue rangedValue)
    {
        switch (v.op ())
        {
            case Protocol.OP_TOUCH, Protocol.OP_INDICATE -> this.notHandled (v, cmd);
            case Protocol.OP_INC -> rangedValue.inc (v.doubleValue ());
            case Protocol.OP_IMMEDIATELY -> rangedValue.setImmediately (v.doubleValue ());
            default -> rangedValue.set (v.opValue ());
        }
    }


    /**
     * Set a beat time value, or change it by the delta; reset sets 0 (as the text protocol did).
     */
    private void setBeatTime (final Protocol.View v, final Cmd cmd, final SettableBeatTimeValue beatTime)
    {
        switch (v.op ())
        {
            case Protocol.OP_TOUCH, Protocol.OP_INDICATE -> this.notHandled (v, cmd);
            case Protocol.OP_INC -> beatTime.inc (v.doubleValue ());
            default -> beatTime.set (v.opValue ());
        }
    }


    private double secondsToBeats (final double seconds)
    {
        return seconds * this.model.transport.tempo ().value ().getRaw () / 60.0;
    }


    private void changeTempo (final int step)
    {
        // "+" and "-" are the slow changes: 0.01 as TransportImpl.changeTempo (Reaper: 0.1)
        final double delta = switch (step)
        {
            case Protocol.TempoStep.PLUS -> 0.01;
            case Protocol.TempoStep.PLUS_PLUS -> 1;
            case Protocol.TempoStep.MINUS -> -0.01;
            case Protocol.TempoStep.MINUS_MINUS -> -1;
            default -> 0;
        };
        final double tempo = this.model.transport.tempo ().value ().getRaw ();
        this.model.transport.tempo ().value ().setRaw (tempo + delta);
    }


    /**
     * Set the play start position, jump to it if playing (TransportImpl.setPosition of the Java
     * version).
     */
    private void setPosition (final double beats)
    {
        this.model.transport.playStartPosition ().set (beats);
        if (this.model.transport.isPlaying ().get ())
            this.model.transport.jumpToPlayStartPosition ();
    }


    /**
     * Set the automation write mode by the index of the core's mode, as
     * TransportImpl.setAutomationWriteMode: Read (and Trim/Read) turns both switches off, the other
     * modes turn them on (Latch Preview is Latch).
     */
    private void setAutomationWriteMode (final int index)
    {
        if (index <= BitwigModel.AUTOMATION_READ)
        {
            this.model.transport.isArrangerAutomationWriteEnabled ().set (false);
            this.model.transport.isClipLauncherAutomationWriteEnabled ().set (false);
            return;
        }
        this.model.transport.isArrangerAutomationWriteEnabled ().set (true);
        this.model.transport.isClipLauncherAutomationWriteEnabled ().set (true);
        final String mode = index < BitwigModel.AUTOMATION_MODES.length ? BitwigModel.AUTOMATION_MODES[index] : null;
        this.model.transport.automationWriteMode ().set (mode == null ? "latch" : mode);
    }


    // ------------------------------------------------------------------------------------------
    // Channels

    /**
     * The channel of a command (the index fields ch_kind, ch_processor, ch_index): the master, a
     * track (the window or the large bank), an effect track of the window, a drum pad or layer of
     * the drum machine, the first instrument or the cursor device.
     *
     * @return The channel, null if there is none (logged as not handled where the text protocol
     *         did)
     */
    private Channel channel (final Protocol.View v, final Cmd cmd)
    {
        final int index = v.chIndex ();
        switch (v.chKind ())
        {
            case Protocol.ChanKind.MASTER:
                return this.model.masterTrack;
            case Protocol.ChanKind.TRACK:
                return this.track (v, cmd, true);
            case Protocol.ChanKind.PAD, Protocol.ChanKind.LAYER:
                final boolean isPad = v.chKind () == Protocol.ChanKind.PAD;
                final SpecificDevice device = switch (v.chProcessor ())
                {
                    case Protocol.Processor.DRUM -> this.model.drumDevice;
                    case Protocol.Processor.PRIMARY -> this.model.firstInstrument;
                    // Only the layers of the cursor device
                    case Protocol.Processor.DEVICE -> isPad ? null : this.model.cursorDeviceLayers;
                    default -> null;
                };
                if (device == null)
                {
                    // A missing drum machine or first instrument: ignored as in the text protocol
                    if (v.chProcessor () != Protocol.Processor.DRUM && v.chProcessor () != Protocol.Processor.PRIMARY)
                        this.notHandled (v, cmd);
                    return null;
                }
                if (isPad && index >= 0 && index < device.numPads)
                    return device.drumPadBank.getItemAt (index);
                if (!isPad && index >= 0 && index < device.numLayers)
                    return device.layerBank.getItemAt (index);
                this.notHandled (v, cmd);
                return null;
            default:
                this.notHandled (v, cmd);
                return null;
        }
    }


    /**
     * The track of a command: the tracks of the track bank (from the window if it is visible,
     * else from the large bank) and, if allowed, the effect tracks of the window.
     *
     * @param withEffectTracks True if the command exists for effect tracks
     * @return The track, null if there is none (logged as not handled)
     */
    private Track track (final Protocol.View v, final Cmd cmd, final boolean withEffectTracks)
    {
        Track track = null;
        if (v.chKind () == Protocol.ChanKind.TRACK)
        {
            if (v.chProcessor () == Protocol.Processor.TRACK)
                track = this.model.getTrack (v.chIndex ());
            else if (withEffectTracks && v.chProcessor () == Protocol.Processor.FX_TRACK)
                track = this.model.getEffectTrack (v.chIndex ());
        }
        if (track == null)
            this.notHandled (v, cmd);
        return track;
    }


    /** A track of the track bank (not the effect tracks), see {@link #track}. */
    private Track bankTrack (final Protocol.View v, final Cmd cmd)
    {
        return this.track (v, cmd, false);
    }


    /**
     * The send of a command: of a track of the window (sends only exist there) or of an effect
     * track of the window.
     *
     * @return The send, null if there is none (logged as not handled)
     */
    private Send send (final Protocol.View v, final Cmd cmd)
    {
        Track track = null;
        if (v.chKind () == Protocol.ChanKind.TRACK)
        {
            if (v.chProcessor () == Protocol.Processor.TRACK)
                track = this.model.getWindowTrack (v.chIndex ());
            else if (v.chProcessor () == Protocol.Processor.FX_TRACK)
                track = this.model.getEffectTrack (v.chIndex ());
        }
        final int index = v.send ();
        if (track == null || index < 0 || index >= this.model.numSends)
        {
            this.notHandled (v, cmd);
            return null;
        }
        return track.sendBank ().getItemAt (index);
    }


    /**
     * Select a channel: a track, effect track, the master or a layer as ChannelImpl.select of the
     * Java version, a drum pad only in the editor (DrumPadImpl.select).
     */
    private void selectChannel (final Protocol.View v, final Cmd cmd, final boolean select)
    {
        final Channel channel = this.channel (v, cmd);
        if (channel == null || !select)
            return;
        if (v.chKind () == Protocol.ChanKind.PAD)
            channel.selectInEditor ();
        else
            selectChannel (channel);
    }


    /**
     * Select a channel as ChannelImpl.select of the Java version.
     */
    private static void selectChannel (final Channel channel)
    {
        channel.selectInEditor ();
        channel.selectInMixer ();
        channel.makeVisibleInArranger ();
        channel.makeVisibleInMixer ();
    }


    /**
     * Move a track one position up or down, as CursorTrackImpl.swapWithPrevious / swapWithNext of
     * the Java version (on the track of the command instead of the cursor track): move the
     * neighbour behind or before the track and select it.
     */
    private void moveTrack (final Track track, final int position, final int direction)
    {
        final int neighbourPosition = position + direction;
        if (neighbourPosition < 0 || neighbourPosition >= BitwigModel.NUM_LARGE_TRACKS)
            return;
        final Track neighbour = this.model.largeTrackBank.getItemAt (neighbourPosition);
        if (direction < 0)
            track.afterTrackInsertionPoint ().moveTracks (neighbour);
        else
            track.beforeTrackInsertionPoint ().moveTracks (neighbour);
        neighbour.selectInEditor ();
    }


    /**
     * Add a track: type, name, color (RGB(r,g,b) or empty), device names, as
     * AbstractTrackBankImpl.addChannel of the Java version.
     */
    private void addTrack (final String [] values)
    {
        if (values.length < 1)
            return;
        // AbstractTrackBankImpl.addTrack
        switch (values[0])
        {
            case "INSTRUMENT" -> this.model.application.createInstrumentTrack (-1);
            case "EFFECT" -> this.model.application.createEffectTrack (-1);
            default -> this.model.application.createAudioTrack (-1);
        }
        final String name = values.length > 1 && !values[1].isEmpty () ? values[1] : null;
        final String color = values.length > 2 && !values[2].isEmpty () ? values[2] : null;
        // The devices are creation names of Reaper, which Bitwig does not know
        for (int i = 3; i < values.length; i++)
            this.unhandled.accept ("addTrack device " + values[i]);
        if (name == null && color == null)
            return;
        // Name and color of the new track (the cursor track) 300 ms later, as the Java version;
        // the core pages the track bank itself (no scrollIntoView)
        this.model.host.scheduleTask ( () -> {
            if (!this.model.cursorTrack.exists ().get ())
                return;
            if (name != null)
                this.model.cursorTrack.name ().set (name);
            if (color != null)
            {
                final java.util.regex.Matcher matcher = java.util.regex.Pattern.compile ("RGB\\((\\d+),(\\d+),(\\d+)\\)").matcher (color);
                if (matcher.matches ())
                    this.model.cursorTrack.color ().set (Integer.parseInt (matcher.group (1)) / 255f, Integer.parseInt (matcher.group (2)) / 255f, Integer.parseInt (matcher.group (3)) / 255f);
            }
        }, 300);
    }


    /**
     * The clip launcher feedback of a bank (AbstractTrackBankImpl.setIndication) or the
     * indication of the pads of a drum pad bank.
     */
    private void indicateBank (final Protocol.View v, final Cmd cmd)
    {
        final boolean indicate = v.boolValue ();
        switch (v.bank ())
        {
            case Protocol.Processor.TRACK -> this.model.trackBank.setShouldShowClipLauncherFeedback (indicate);
            case Protocol.Processor.FX_TRACK -> this.model.effectTrackBank.setShouldShowClipLauncherFeedback (indicate);
            case Protocol.Processor.DRUM, Protocol.Processor.PRIMARY ->
            {
                final SpecificDevice device = v.bank () == Protocol.Processor.DRUM ? this.model.drumDevice : this.model.firstInstrument;
                if (device != null)
                    device.drumPadBank.setIndication (indicate);
            }
            default -> this.notHandled (v, cmd);
        }
    }


    // ------------------------------------------------------------------------------------------
    // Slots and scenes

    /**
     * The slot of a command: a slot of a track of the window, by the absolute scene position.
     *
     * @return The slot, null if there is none (logged as not handled)
     */
    private ClipLauncherSlot slot (final Protocol.View v, final Cmd cmd)
    {
        final Track track = this.model.getWindowTrack (v.track ());
        final ClipLauncherSlot slot = track == null ? null : this.model.getSlot (track, v.slot ());
        if (slot == null)
            this.notHandled (v, cmd);
        return slot;
    }


    /**
     * The scene of a command, by its absolute position (only the scenes of the window).
     *
     * @return The scene, null if there is none (logged as not handled)
     */
    private Scene scene (final Protocol.View v, final Cmd cmd)
    {
        final Scene scene = this.model.getScene (v.scene ());
        if (scene == null)
            this.notHandled (v, cmd);
        return scene;
    }


    // ------------------------------------------------------------------------------------------
    // Devices

    /**
     * The device of a command to a device itself: the cursor device, the drum machine, the first
     * instrument or the EQ+.
     *
     * @return The device, null if there is none (a missing drum machine or first instrument is
     *         ignored, the others are logged as not handled)
     */
    private Device device (final Protocol.View v, final Cmd cmd)
    {
        switch (v.dev ())
        {
            case Protocol.Processor.DEVICE:
                return this.model.cursorDevice;
            case Protocol.Processor.DRUM, Protocol.Processor.PRIMARY:
                final SpecificDevice device = v.dev () == Protocol.Processor.DRUM ? this.model.drumDevice : this.model.firstInstrument;
                return device == null ? null : device.device;
            case Protocol.Processor.EQ:
                if (this.model.eqDevice != null)
                    return this.model.eqDevice.device;
                this.notHandled (v, cmd);
                return null;
            default:
                this.notHandled (v, cmd);
                return null;
        }
    }


    /**
     * The device of a drum pad command: the drum machine or the first instrument.
     *
     * @return The device, null if there is none (logged as not handled except a missing drum
     *         machine or first instrument)
     */
    private SpecificDevice drumPadDevice (final Protocol.View v, final Cmd cmd)
    {
        return switch (v.dev ())
        {
            case Protocol.Processor.DRUM -> this.model.drumDevice;
            case Protocol.Processor.PRIMARY -> this.model.firstInstrument;
            default ->
            {
                this.notHandled (v, cmd);
                yield null;
            }
        };
    }


    /**
     * The commands of a device on the device itself, as SpecificDeviceImpl and DeviceImpl of the
     * Java version: toggle the bypass, remove, duplicate, select.
     *
     * @return False if the action is unknown
     */
    private static boolean deviceAction (final Device device, final int action)
    {
        switch (action)
        {
            case Protocol.DeviceAction.BYPASS -> device.isEnabled ().toggle ();
            case Protocol.DeviceAction.REMOVE -> device.deleteObject ();
            case Protocol.DeviceAction.DUPLICATE -> device.afterDeviceInsertionPoint ().copyDevices (device);
            case Protocol.DeviceAction.SELECT -> device.selectInEditor ();
            default ->
            {
                return false;
            }
        }
        return true;
    }


    /**
     * Find the device with the given position in the chain among the siblings.
     */
    private Device findSibling (final int position)
    {
        if (position < 0)
            return null;
        for (int i = 0; i < BitwigModel.NUM_SIBLINGS; i++)
        {
            final Device device = this.model.siblings.getItemAt (i);
            if (device.exists ().get () && device.position ().get () == position)
                return device;
        }
        return null;
    }


    /**
     * Move the cursor device one position to the left or right, as
     * CursorDeviceImpl.swapWithPrevious / swapWithNext of the Java version: move the neighbour
     * behind or before the cursor device and select it.
     */
    private void moveCursorDevice (final int direction)
    {
        final PinnableCursorDevice device = this.model.cursorDevice;
        if (!device.exists ().get ())
            return;
        final int neighbourPosition = device.position ().get () + direction;
        if (neighbourPosition < 0 || neighbourPosition >= BitwigModel.NUM_LARGE_DEVICES)
            return;
        final Device neighbour = this.model.largeDeviceBank.getItemAt (neighbourPosition);
        if (direction < 0)
            device.afterDeviceInsertionPoint ().moveDevices (neighbour);
        else
            device.beforeDeviceInsertionPoint ().moveDevices (neighbour);
        neighbour.selectInEditor ();
    }


    /**
     * Show the parameter page in Bitwig as well (ParameterPageBankImpl.scrollTo): of the cursor
     * device, the first instrument, the drum machine, the cursor track or the project.
     */
    private void selectParameterPage (final Protocol.View v, final Cmd cmd)
    {
        final CursorRemoteControlsPage remoteControls;
        switch (v.dev ())
        {
            case Protocol.Processor.DEVICE -> remoteControls = this.model.remoteControls;
            case Protocol.Processor.TRACK -> remoteControls = this.model.trackRemoteControls;
            case Protocol.Processor.PROJECT -> remoteControls = this.model.projectRemoteControls;
            case Protocol.Processor.DRUM, Protocol.Processor.PRIMARY ->
            {
                final SpecificDevice device = v.dev () == Protocol.Processor.DRUM ? this.model.drumDevice : this.model.firstInstrument;
                if (device == null)
                    return;
                if (device.remoteControls == null)
                {
                    this.notHandled (v, cmd);
                    return;
                }
                remoteControls = device.remoteControls;
            }
            default ->
            {
                this.notHandled (v, cmd);
                return;
            }
        }
        if (remoteControls == null)
            return;
        final int count = remoteControls.pageCount ().get ();
        if (count > 0)
            remoteControls.selectedPageIndex ().set (Math.clamp (v.intValue (), 0, count - 1));
    }


    /**
     * A parameter on the flat parameter list of the page cursors of the cursor device, the first
     * instrument, the drum machine, the cursor track or the project; the parameters of the EQ+.
     *
     * @return The parameter, null if there is none (an index outside of the pages is ignored, a
     *         missing drum machine or first instrument as well)
     */
    private Parameter parameter (final Protocol.View v, final Cmd cmd, final int dev, final int index)
    {
        final CursorRemoteControlsPage [] pages;
        switch (dev)
        {
            case Protocol.Processor.DEVICE -> pages = this.model.pages;
            case Protocol.Processor.TRACK -> pages = this.model.trackPages;
            case Protocol.Processor.PROJECT -> pages = this.model.projectPages;
            case Protocol.Processor.DRUM, Protocol.Processor.PRIMARY ->
            {
                final SpecificDevice device = dev == Protocol.Processor.DRUM ? this.model.drumDevice : this.model.firstInstrument;
                if (device == null)
                    return null;
                pages = device.pages;
            }
            case Protocol.Processor.EQ ->
            {
                final Parameter parameter = this.model.eqDevice == null ? null : this.model.eqDevice.getParameter (index);
                if (parameter == null)
                    this.notHandled (v, cmd);
                return parameter;
            }
            default ->
            {
                this.notHandled (v, cmd);
                return null;
            }
        }
        final int page = index / BitwigModel.PARAMS_PER_PAGE;
        if (index < 0 || page >= pages.length)
            return null;
        return pages[page].getParameter (index % BitwigModel.PARAMS_PER_PAGE);
    }


    // ------------------------------------------------------------------------------------------
    // Clips

    /**
     * The commands of a step grid of a note clip (the grid of the size of the command).
     */
    private void handleGrid (final Protocol.View v, final Cmd cmd)
    {
        BitwigNoteClip grid = null;
        for (final BitwigNoteClip noteClip: this.model.noteClips)
        {
            if (noteClip.numSteps == v.gridSteps () && noteClip.numRows == v.gridRows ())
            {
                grid = noteClip;
                break;
            }
        }
        if (grid == null)
        {
            this.notHandled (v, cmd);
            return;
        }
        final boolean handled = switch (cmd)
        {
            case GRID_STEP_SIZE ->
            {
                grid.setStepSize (v.doubleValue ());
                yield true;
            }
            case GRID_SCROLL ->
            {
                grid.scrollToStep (v.intValue ());
                yield true;
            }
            case GRID_PAGE ->
            {
                grid.page (v.dir () == Protocol.Dir.NEXT);
                yield true;
            }
            case GRID_CLEAR_ALL ->
            {
                grid.clearAll ();
                yield true;
            }
            case GRID_CLEAR_ROW ->
            {
                grid.clearRow (v.channel (), v.row ());
                yield true;
            }
            case GRID_CLEAR_COLUMN ->
            {
                grid.clearColumn (v.channel (), v.column ());
                yield true;
            }
            case GRID_STEP_TOGGLE ->
            {
                grid.toggle (v.channel (), v.step (), v.note (), v.intValue ());
                yield true;
            }
            case GRID_STEP_CLEAR ->
            {
                grid.clear (v.channel (), v.step (), v.note ());
                yield true;
            }
            case GRID_STEP_MOVE_Y ->
            {
                grid.moveY (v.channel (), v.step (), v.note (), v.intValue ());
                yield true;
            }
            // The velocity and the duration
            case GRID_STEP_SET ->
            {
                grid.set (v.channel (), v.step (), v.note (), v.payloadInt (0), v.payloadDouble (8));
                yield true;
            }
            case GRID_STEP_SET_FROM -> grid.setFrom (v.channel (), v.step (), v.note (), v.text ());
            case GRID_STEP_UPDATE -> grid.update (v.channel (), v.step (), v.note (), v.text ());
            // The attribute and the value
            case GRID_STEP_ATTRIBUTE -> grid.updateAttribute (v.channel (), v.step (), v.note (), v.payloadInt (0), v.payloadDouble (8));
            default -> false;
        };
        if (!handled)
            this.notHandled (v, cmd);
    }


    // ------------------------------------------------------------------------------------------
    // Browser

    /**
     * The popup browser, as BrowserImpl of the Java version.
     */
    private void handleBrowser (final Protocol.View v, final Cmd cmd)
    {
        final BitwigBrowser browser = this.model.browser;
        if (browser == null)
            return;
        final boolean handled = switch (cmd)
        {
            case BROWSER_STOP ->
            {
                browser.stopBrowsing (v.boolValue ());
                yield true;
            }
            case BROWSER_PREVIEW ->
            {
                browser.setPreview (v.boolValue ());
                yield true;
            }
            case BROWSER_CONTENT_TYPE ->
            {
                browser.changeContentType (v.dir () == Protocol.Dir.NEXT ? 1 : -1);
                yield true;
            }
            case BROWSER_COLUMN_STEP -> browser.stepColumn (v.column (), v.dir () == Protocol.Dir.NEXT);
            case BROWSER_COLUMN_PAGE -> browser.pageColumn (v.column (), v.dir () == Protocol.Dir.NEXT);
            case BROWSER_COLUMN_RESET -> browser.resetColumn (v.column ());
            case BROWSER_RESULT_STEP ->
            {
                browser.stepResult (v.dir () == Protocol.Dir.NEXT);
                yield true;
            }
            case BROWSER_RESULT_PAGE ->
            {
                browser.pageResult (v.dir () == Protocol.Dir.NEXT);
                yield true;
            }
            case BROWSER_INSERT_BEFORE ->
            {
                final PinnableCursorDevice device = this.model.cursorDevice;
                browser.browse (device.exists ().get () ? device.beforeDeviceInsertionPoint () : this.model.cursorTrack.startOfDeviceChainInsertionPoint ());
                yield true;
            }
            case BROWSER_INSERT_AFTER ->
            {
                final PinnableCursorDevice device = this.model.cursorDevice;
                browser.browse (device.exists ().get () ? device.afterDeviceInsertionPoint () : this.model.cursorTrack.endOfDeviceChainInsertionPoint ());
                yield true;
            }
            // The start of the channel's device chain
            case BROWSER_ADD_TO_MASTER ->
            {
                browser.browse (this.model.masterTrack.startOfDeviceChainInsertionPoint ());
                yield true;
            }
            case BROWSER_ADD_TO_TRACK ->
            {
                final Track track = this.model.getTrack (v.track ());
                if (track != null)
                    browser.browse (track.startOfDeviceChainInsertionPoint ());
                yield true;
            }
            case BROWSER_REPLACE_DEVICE ->
            {
                browser.browse (this.model.cursorDevice.replaceDeviceInsertionPoint ());
                yield true;
            }
            case BROWSER_REPLACE_SLOT ->
            {
                // Only slots of the window can be addressed
                final Track track = this.model.getWindowTrack (v.track ());
                final ClipLauncherSlot slot = track == null ? null : this.model.getSlot (track, v.slot ());
                if (slot != null)
                    browser.browse (slot.replaceInsertionPoint ());
                yield true;
            }
            case BROWSER_REPLACE_PAD ->
            {
                final SpecificDevice device = switch (v.owner ())
                {
                    case Protocol.Processor.DRUM -> this.model.drumDevice;
                    case Protocol.Processor.PRIMARY -> this.model.firstInstrument;
                    default -> null;
                };
                final int pad = v.pad ();
                if (device == null || pad < 0 || pad >= device.numPads)
                    yield false;
                browser.browse (device.drumPadBank.getItemAt (pad).insertionPoint ());
                yield true;
            }
            default -> false;
        };
        if (!handled)
            this.notHandled (v, cmd);
    }


    // ------------------------------------------------------------------------------------------
    // Markers

    /**
     * Move the play position to the closest marker before or after it, as
     * MarkerBankImpl.findAndSelect of the Java version. Without a marker bank (the controller
     * uses no markers) Bitwig's jump to the previous or next cue marker.
     */
    private void findAndSelectMarker (final boolean isForward)
    {
        if (this.model.markerBank == null)
        {
            if (isForward)
                this.model.transport.jumpToNextCueMarker ();
            else
                this.model.transport.jumpToPreviousCueMarker ();
            return;
        }

        final double playPosition = this.model.transport.getPosition ().get ();
        double closest = -1;
        double distance = 0;
        for (int i = 0; i < BitwigModel.NUM_MARKERS; i++)
        {
            final CueMarker marker = this.model.markerBank.getItemAt (i);
            if (!marker.exists ().get ())
                continue;
            final double pos = marker.position ().get ();
            final double dist = isForward ? pos - playPosition : playPosition - pos;
            if (dist > 0 && (distance == 0 || dist < distance))
            {
                distance = dist;
                closest = pos;
            }
        }
        if (closest >= 0)
            this.setPosition (closest);
    }


    // ------------------------------------------------------------------------------------------
    // Note repeat

    /**
     * Note repeat commands, mapped to the arpeggiator of the default note input.
     */
    private void handleNoteRepeat (final Protocol.View v, final Cmd cmd)
    {
        if (this.model.arpeggiator == null)
            return;
        switch (cmd)
        {
            case NOTE_REPEAT_ACTIVE -> this.model.arpeggiator.isEnabled ().set (v.boolValue ());
            // The core sends the inverse of the period in beats
            case NOTE_REPEAT_RATE ->
            {
                if (v.doubleValue () > 0)
                    this.model.arpeggiator.rate ().set (1.0 / v.doubleValue ());
            }
            case NOTE_REPEAT_NOTE_LENGTH -> this.model.arpeggiator.gateLength ().set (v.doubleValue ());
            case NOTE_REPEAT_MODE ->
            {
                final int index = v.intValue ();
                if (index >= 0 && index < BitwigModel.ARPEGGIATOR_MODES.size ())
                    this.model.arpeggiator.mode ().set (BitwigModel.ARPEGGIATOR_MODES.get (index));
            }
            case NOTE_REPEAT_VELOCITY -> this.model.arpeggiator.usePressureToVelocity ().set (v.boolValue ());
            default -> this.notHandled (v, cmd);
        }
    }


    /**
     * Invoke a Bitwig action by its ID, as the Java version does for features without an API.
     */
    private void invokeAction (final String id)
    {
        final Action action = this.model.application.getAction (id);
        if (action == null)
            this.unhandled.accept ("action " + id);
        else
            action.invoke ();
    }
}
