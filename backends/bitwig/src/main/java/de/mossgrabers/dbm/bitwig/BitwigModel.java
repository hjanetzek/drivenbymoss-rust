// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Application;
import com.bitwig.extension.controller.api.Arpeggiator;
import com.bitwig.extension.controller.api.Arranger;
import com.bitwig.extension.controller.api.Channel;
import com.bitwig.extension.controller.api.ClipLauncherSlot;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.CursorDeviceFollowMode;
import com.bitwig.extension.controller.api.CursorRemoteControlsPage;
import com.bitwig.extension.controller.api.CursorTrack;
import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.DeviceBank;
import com.bitwig.extension.controller.api.MasterTrack;
import com.bitwig.extension.controller.api.Mixer;
import com.bitwig.extension.controller.api.Parameter;
import com.bitwig.extension.controller.api.Project;
import com.bitwig.extension.controller.api.Scene;
import com.bitwig.extension.controller.api.SceneBank;
import com.bitwig.extension.controller.api.PinnableCursorDevice;
import com.bitwig.extension.controller.api.Send;
import com.bitwig.extension.controller.api.Track;
import com.bitwig.extension.controller.api.TrackBank;
import com.bitwig.extension.controller.api.Transport;

import java.util.List;
import java.util.Locale;
import java.util.regex.Matcher;
import java.util.regex.Pattern;


/**
 * The Bitwig objects the core needs, created during init from the core's model setup, and the
 * collection of their state as model update lines. The track and scene banks are windows of the
 * size the controller needs, scrolled by Bitwig on the core's command (host paging, protocol v2);
 * their items are reported with their absolute positions.
 */
public final class BitwigModel
{
    /** The size of the bank for commands to tracks outside of the window (as CursorTrackImpl of the Java version). */
    public static final int  NUM_LARGE_TRACKS    = 400;
    /** The size of the device sibling window, as in the Reaper backend. */
    public static final int  NUM_SIBLINGS        = 8;
    /** The parameters per remote control page. */
    public static final int  PARAMS_PER_PAGE     = 8;
    /** The Bitwig arpeggiator modes in the order of the core's modes (Down, Up, Down-Up, Up-Down). */
    static final List<String> ARPEGGIATOR_MODES = List.of ("down", "up", "down-up", "up-down");
    /** VU meter resolution, as ChannelImpl of the Java version. */
    private static final int VU_RANGE            = 16384;

    final Transport          transport;
    final Application        application;
    final Arranger           arranger;
    final Mixer              mixer;
    /** The window of tracks, with sends and slots. */
    final TrackBank          trackBank;
    /** All tracks, without sends and slots, for commands to tracks outside of the window. */
    final TrackBank          largeTrackBank;
    /** The window of scenes; the slot banks of the window tracks follow it. */
    final SceneBank          sceneBank;
    /** The size of the track window. */
    final int                numTracks;
    /** The size of the scene window. */
    final int                numScenes;
    final Project            project;
    final CursorTrack        cursorTrack;
    final MasterTrack        masterTrack;
    final PinnableCursorDevice cursorDevice;
    final DeviceBank         siblings;
    final CursorRemoteControlsPage [] pages;
    /** Follows the page selected in Bitwig (as the remote controls page of the Java version). */
    final CursorRemoteControlsPage remoteControls;
    final int                numSends;
    /** The note repeat of the default note input, null if the controller has none. */
    final Arpeggiator        arpeggiator;

    private final int []     trackVuLeft;
    private final int []     trackVuRight;
    private final boolean [] sceneSelected;
    /** The position of the selected track which was reported last, -1 if none. */
    private int              lastSelectedTrack   = -1;
    private boolean          masterSelected;
    private int              masterVuLeft;
    private int              masterVuRight;


    /**
     * Create all Bitwig objects; must be called during init.
     *
     * @param host The host
     * @param modelSetup The model setup of the core as JSON (see dbm_get_model_setup)
     * @param arpeggiator The arpeggiator of the default note input (note repeat), may be null
     */
    public BitwigModel (final ControllerHost host, final String modelSetup, final Arpeggiator arpeggiator)
    {
        this.arpeggiator = arpeggiator;
        this.numSends = Math.max (0, getInt (modelSetup, "numSends", 8));
        this.numTracks = Math.max (1, getInt (modelSetup, "numTracks", 8));
        this.numScenes = Math.max (1, getInt (modelSetup, "numScenes", 8));
        this.trackVuLeft = new int [this.numTracks];
        this.trackVuRight = new int [this.numTracks];
        this.sceneSelected = new boolean [this.numScenes];
        final int numParamPages = Math.max (1, getInt (modelSetup, "numParamPages", 8));

        this.transport = host.createTransport ();
        this.application = host.createApplication ();
        this.arranger = host.createArranger ();
        this.mixer = host.createMixer ();
        this.trackBank = host.createTrackBank (this.numTracks, this.numSends, this.numScenes, true);
        this.largeTrackBank = host.createTrackBank (NUM_LARGE_TRACKS, 0, 0, true);
        this.sceneBank = this.trackBank.sceneBank ();
        this.project = host.getProject ();
        this.cursorTrack = host.createCursorTrack ("DBM_CURSOR_TRACK", "Cursor track", this.numSends, this.numScenes, true);
        this.masterTrack = host.createMasterTrack (0);
        this.cursorDevice = this.cursorTrack.createCursorDevice ("DBM_CURSOR_DEVICE", "Cursor device", 0, CursorDeviceFollowMode.FOLLOW_SELECTION);
        this.siblings = this.cursorDevice.createSiblingsDeviceBank (NUM_SIBLINGS);
        this.remoteControls = this.cursorDevice.createCursorRemoteControlsPage (PARAMS_PER_PAGE);
        this.pages = new CursorRemoteControlsPage [numParamPages];
        for (int i = 0; i < numParamPages; i++)
            this.pages[i] = this.cursorDevice.createCursorRemoteControlsPage ("DBM page " + (i + 1), PARAMS_PER_PAGE, "");

        this.markInterested ();
    }


    private void markInterested ()
    {
        this.transport.isPlaying ().markInterested ();
        this.transport.isArrangerRecordEnabled ().markInterested ();
        this.transport.isArrangerLoopEnabled ().markInterested ();
        this.transport.isMetronomeEnabled ().markInterested ();
        this.transport.isMetronomeAudibleDuringPreRoll ().markInterested ();
        this.transport.isPunchInEnabled ().markInterested ();
        this.transport.isPunchOutEnabled ().markInterested ();
        this.arranger.isPlaybackFollowEnabled ().markInterested ();
        this.transport.metronomeVolume ().markInterested ();
        this.transport.metronomeVolume ().displayedValue ().markInterested ();
        this.transport.tempo ().value ().markInterested ();
        this.transport.getPosition ().markInterested ();
        this.transport.playStartPosition ().markInterested ();
        this.transport.arrangerLoopStart ().markInterested ();
        this.transport.arrangerLoopDuration ().markInterested ();
        this.arranger.getHorizontalScrollbarModel ().getContentPerPixel ().markInterested ();
        this.transport.timeSignature ().numerator ().markInterested ();
        this.transport.timeSignature ().denominator ().markInterested ();
        this.application.canUndo ().markInterested ();
        this.application.canRedo ().markInterested ();

        this.trackBank.itemCount ().markInterested ();
        this.trackBank.scrollPosition ().markInterested ();
        this.cursorTrack.position ().markInterested ();
        for (int i = 0; i < this.numTracks; i++)
        {
            final Track track = this.trackBank.getItemAt (i);
            final int index = i;
            this.markChannel (track);
            track.trackType ().markInterested ();
            track.isGroup ().markInterested ();
            track.isGroupExpanded ().markInterested ();
            track.arm ().markInterested ();
            track.monitorMode ().markInterested ();
            track.addVuMeterObserver (VU_RANGE, 0, true, value -> this.trackVuLeft[index] = value);
            track.addVuMeterObserver (VU_RANGE, 1, true, value -> this.trackVuRight[index] = value);
            track.isStopped ().markInterested ();
            for (int s = 0; s < this.numScenes; s++)
            {
                final ClipLauncherSlot slot = track.clipLauncherSlotBank ().getItemAt (s);
                slot.exists ().markInterested ();
                slot.hasContent ().markInterested ();
                slot.name ().markInterested ();
                slot.color ().markInterested ();
                slot.isSelected ().markInterested ();
                slot.isPlaying ().markInterested ();
                slot.isPlaybackQueued ().markInterested ();
                slot.isRecording ().markInterested ();
                slot.isRecordingQueued ().markInterested ();
                slot.isStopQueued ().markInterested ();
            }
            track.sendBank ().itemCount ().markInterested ();
            for (int s = 0; s < this.numSends; s++)
            {
                final Send send = track.sendBank ().getItemAt (s);
                send.exists ().markInterested ();
                send.name ().markInterested ();
                send.value ().markInterested ();
                send.displayedValue ().markInterested ();
                send.isEnabled ().markInterested ();
            }
        }

        this.sceneBank.itemCount ().markInterested ();
        this.sceneBank.scrollPosition ().markInterested ();
        for (int i = 0; i < this.numScenes; i++)
        {
            final Scene scene = this.sceneBank.getItemAt (i);
            final int index = i;
            scene.exists ().markInterested ();
            scene.name ().markInterested ();
            scene.color ().markInterested ();
            scene.addIsSelectedInEditorObserver (selected -> this.sceneSelected[index] = selected);
        }

        this.markChannel (this.masterTrack);
        this.masterTrack.addIsSelectedInEditorObserver (selected -> this.masterSelected = selected);
        this.masterTrack.addVuMeterObserver (VU_RANGE, 0, true, value -> this.masterVuLeft = value);
        this.masterTrack.addVuMeterObserver (VU_RANGE, 1, true, value -> this.masterVuRight = value);

        this.cursorDevice.exists ().markInterested ();
        this.cursorDevice.position ().markInterested ();
        this.cursorDevice.name ().markInterested ();
        this.cursorDevice.isEnabled ().markInterested ();
        this.cursorDevice.isWindowOpen ().markInterested ();
        this.cursorDevice.isExpanded ().markInterested ();
        this.siblings.itemCount ().markInterested ();
        for (int i = 0; i < NUM_SIBLINGS; i++)
        {
            final Device device = this.siblings.getItemAt (i);
            device.exists ().markInterested ();
            device.name ().markInterested ();
            device.isEnabled ().markInterested ();
            device.position ().markInterested ();
        }
        if (this.arpeggiator != null)
        {
            this.arpeggiator.isEnabled ().markInterested ();
            this.arpeggiator.rate ().markInterested ();
            this.arpeggiator.gateLength ().markInterested ();
            this.arpeggiator.mode ().markInterested ();
            this.arpeggiator.usePressureToVelocity ().markInterested ();
        }

        this.remoteControls.pageCount ().markInterested ();
        this.remoteControls.pageNames ().markInterested ();
        this.remoteControls.selectedPageIndex ().markInterested ();

        for (final CursorRemoteControlsPage page: this.pages)
        {
            page.pageCount ().markInterested ();
            page.selectedPageIndex ().markInterested ();
            for (int i = 0; i < PARAMS_PER_PAGE; i++)
                markParameter (page.getParameter (i));
        }
    }


    private void markChannel (final Channel channel)
    {
        channel.exists ().markInterested ();
        channel.name ().markInterested ();
        channel.color ().markInterested ();
        channel.mute ().markInterested ();
        channel.solo ().markInterested ();
        markParameter (channel.volume ());
        markParameter (channel.pan ());
    }


    private static void markParameter (final Parameter parameter)
    {
        parameter.exists ().markInterested ();
        parameter.name ().markInterested ();
        parameter.value ().markInterested ();
        parameter.displayedValue ().markInterested ();
        parameter.modulatedValue ().markInterested ();
    }


    /**
     * Report the current state.
     *
     * @param reporter Where to report to
     */
    public void collect (final ModelReporter reporter)
    {
        this.collectTransport (reporter);
        this.collectTracks (reporter);
        this.collectScenes (reporter);
        this.collectMaster (reporter);
        this.collectDevice (reporter);
        this.collectNoteRepeat (reporter);
    }


    /**
     * The note repeat as in the Reaper backend: "period" is the inverse of the period in beats
     * (the parser inverts it again), "mode" the index of the core's arpeggiator modes.
     */
    private void collectNoteRepeat (final ModelReporter reporter)
    {
        if (this.arpeggiator == null)
            return;
        final double period = this.arpeggiator.rate ().get ();
        reporter.put ("/noterepeat/active", this.arpeggiator.isEnabled ().get ());
        if (period > 0)
            reporter.put ("/noterepeat/period", 1.0 / period);
        reporter.put ("/noterepeat/notelength", this.arpeggiator.gateLength ().get ());
        reporter.put ("/noterepeat/mode", Math.max (0, ARPEGGIATOR_MODES.indexOf (this.arpeggiator.mode ().get ())));
        reporter.put ("/noterepeat/velocity", this.arpeggiator.usePressureToVelocity ().get ());
    }


    private void collectTransport (final ModelReporter reporter)
    {
        final double tempo = this.transport.tempo ().value ().getRaw ();
        final double beats = this.transport.getPosition ().get ();
        final double seconds = tempo > 0 ? beats * 60.0 / tempo : 0;
        final double contentPerPixel = this.arranger.getHorizontalScrollbarModel ().getContentPerPixel ().get ();

        reporter.put ("/play", this.transport.isPlaying ().get ());
        reporter.put ("/record", this.transport.isArrangerRecordEnabled ().get ());
        reporter.put ("/repeat", this.transport.isArrangerLoopEnabled ().get ());
        reporter.put ("/tempo", tempo);
        reporter.put ("/numerator", this.transport.timeSignature ().numerator ().get ());
        reporter.put ("/denominator", this.transport.timeSignature ().denominator ().get ());
        reporter.put ("/time", seconds);

        // As the Java version: the position (for moving it) is the play start position, the
        // texts show the play position; the zoom is the inverse of the content per pixel
        reporter.put ("/beats/position", this.transport.playStartPosition ().get ());
        reporter.put ("/beats/loop/start", this.transport.arrangerLoopStart ().get ());
        reporter.put ("/beats/loop/length", this.transport.arrangerLoopDuration ().get ());
        reporter.put ("/time/text", tempo > 0 ? TimeFormat.formatTimeLong (tempo, beats) : "");
        reporter.put ("/beats/text", this.transport.getPosition ().getFormatted (TimeFormat.BEAT_POSITION_FORMATTER));
        reporter.put ("/beats/loop/start/text", this.transport.arrangerLoopStart ().getFormatted (TimeFormat.BEAT_POSITION_FORMATTER));
        reporter.put ("/beats/loop/length/text", this.transport.arrangerLoopDuration ().getFormatted (TimeFormat.BEAT_LENGTH_FORMATTER));
        if (contentPerPixel > 0)
            reporter.put ("/time/hzoom", 1.0 / contentPerPixel);
        reporter.put ("/click", this.transport.isMetronomeEnabled ().get ());
        // As Reaper's pre-roll bit field: 2 = the metronome is audible during the pre-roll
        reporter.put ("/click/preroll", this.transport.isMetronomeAudibleDuringPreRoll ().get () ? 2 : 0);
        reporter.put ("/followPlayback", this.arranger.isPlaybackFollowEnabled ().get ());
        reporter.put ("/click/volume", this.transport.metronomeVolume ().get ());
        reporter.put ("/click/volumeStr", this.transport.metronomeVolume ().displayedValue ().get ());
        reporter.put ("/project/canUndo", this.application.canUndo ().get ());
        reporter.put ("/project/canRedo", this.application.canRedo ().get ());
    }


    /**
     * The tracks of the window with their absolute positions, the window offset and the number of
     * all tracks. The selection comes from the cursor track, so that tracks outside of the window
     * are deselected as well.
     */
    private void collectTracks (final ModelReporter reporter)
    {
        final int offset = this.trackBank.scrollPosition ().get ();
        final int total = this.trackBank.itemCount ().get ();
        final int selected = this.cursorTrack.position ().get ();
        reporter.put ("/track/offset", offset);
        if (selected != this.lastSelectedTrack)
        {
            if (this.lastSelectedTrack >= 0)
                reporter.put ("/track/" + this.lastSelectedTrack + "/select", false);
            this.lastSelectedTrack = selected;
        }

        final int sceneOffset = this.sceneBank.scrollPosition ().get ();
        final int sceneTotal = this.sceneBank.itemCount ().get ();
        for (int i = 0; i < this.numTracks; i++)
        {
            final int position = offset + i;
            if (position >= total)
                break;
            final Track track = this.trackBank.getItemAt (i);
            final String prefix = "/track/" + position + "/";
            reporter.put (prefix + "exists", 1);
            reporter.put (prefix + "number", position);
            reporter.put (prefix + "type", trackType (track));
            reporter.put (prefix + "isGroupExpanded", track.isGroupExpanded ().get ());
            reporter.put (prefix + "select", position == selected);
            reporter.put (prefix + "recarm", track.arm ().get ());
            final String monitorMode = track.monitorMode ().get ();
            reporter.put (prefix + "monitor", "ON".equals (monitorMode));
            reporter.put (prefix + "autoMonitor", "AUTO".equals (monitorMode));
            reportVu (reporter, prefix, this.trackVuLeft[i], this.trackVuRight[i]);
            this.collectChannel (reporter, prefix, track);

            final int sendCount = Math.min (track.sendBank ().itemCount ().get (), this.numSends);
            for (int s = 0; s < sendCount; s++)
            {
                final Send send = track.sendBank ().getItemAt (s);
                final String sendPrefix = prefix + "send/" + s + "/";
                reporter.put (sendPrefix + "name", send.name ().get ());
                reporter.put (sendPrefix + "volume", send.value ().get ());
                reporter.put (sendPrefix + "volume/str", send.displayedValue ().get ());
                reporter.put (sendPrefix + "active", send.isEnabled ().get ());
            }
            reporter.put (prefix + "send/count", sendCount);
            this.collectSlots (reporter, prefix, track, sceneOffset, sceneTotal);
        }
        reporter.put ("/track/count", total);
        if (selected >= 0 && selected < total)
            reporter.put ("/track/" + selected + "/select", true);
    }


    /**
     * The clip launcher slots of a track (protocol v2), as SlotImpl of the Java version.
     */
    private void collectSlots (final ModelReporter reporter, final String prefix, final Track track, final int sceneOffset, final int sceneTotal)
    {
        reporter.put (prefix + "isStopped", track.isStopped ().get ());
        reporter.put (prefix + "slot/count", sceneTotal);
        for (int s = 0; s < this.numScenes; s++)
        {
            final int position = sceneOffset + s;
            if (position >= sceneTotal)
                break;
            final ClipLauncherSlot slot = track.clipLauncherSlotBank ().getItemAt (s);
            final String slotPrefix = prefix + "slot/" + position + "/";
            reporter.put (slotPrefix + "exists", slot.exists ().get ());
            reporter.put (slotPrefix + "hasContent", slot.hasContent ().get ());
            reporter.put (slotPrefix + "name", slot.name ().get ());
            // As the Java version: the slots of group tracks have the color of the track (API)
            if (track.isGroup ().get ())
                reporter.putColor (slotPrefix + "color", track.color ().red (), track.color ().green (), track.color ().blue ());
            else
                reporter.putColor (slotPrefix + "color", slot.color ().red (), slot.color ().green (), slot.color ().blue ());
            reporter.put (slotPrefix + "selected", slot.isSelected ().get ());
            reporter.put (slotPrefix + "playing", slot.isPlaying ().get ());
            reporter.put (slotPrefix + "recording", slot.isRecording ().get ());
            reporter.put (slotPrefix + "playQueued", slot.isPlaybackQueued ().get ());
            reporter.put (slotPrefix + "recQueued", slot.isRecordingQueued ().get ());
            reporter.put (slotPrefix + "stopQueued", slot.isStopQueued ().get ());
        }
    }


    private void collectScenes (final ModelReporter reporter)
    {
        final int offset = this.sceneBank.scrollPosition ().get ();
        final int total = this.sceneBank.itemCount ().get ();
        reporter.put ("/scene/offset", offset);
        for (int i = 0; i < this.numScenes; i++)
        {
            final int position = offset + i;
            if (position >= total)
                break;
            final Scene scene = this.sceneBank.getItemAt (i);
            final String prefix = "/scene/" + position + "/";
            reporter.put (prefix + "exists", scene.exists ().get ());
            reporter.put (prefix + "number", position);
            reporter.put (prefix + "name", scene.name ().get ());
            reporter.putColor (prefix + "color", scene.color ().red (), scene.color ().green (), scene.color ().blue ());
            reporter.put (prefix + "select", this.sceneSelected[i]);
        }
        reporter.put ("/scene/count", total);
    }


    /**
     * Get the track at the absolute position: from the window if it is visible (with sends and
     * slots), otherwise from the large bank.
     *
     * @param position The position
     * @return The track or null
     */
    Track getTrack (final int position)
    {
        final Track track = this.getWindowTrack (position);
        if (track != null)
            return track;
        return position >= 0 && position < NUM_LARGE_TRACKS ? this.largeTrackBank.getItemAt (position) : null;
    }


    /**
     * Get the track at the absolute position if it is in the window.
     *
     * @param position The position
     * @return The track or null
     */
    Track getWindowTrack (final int position)
    {
        final int index = position - this.trackBank.scrollPosition ().get ();
        return index >= 0 && index < this.numTracks ? this.trackBank.getItemAt (index) : null;
    }


    /**
     * Get the slot of a window track at the absolute scene position if it is in the window.
     *
     * @param track A track of the window
     * @param position The scene position
     * @return The slot or null
     */
    ClipLauncherSlot getSlot (final Track track, final int position)
    {
        final int index = position - this.sceneBank.scrollPosition ().get ();
        return index >= 0 && index < this.numScenes ? track.clipLauncherSlotBank ().getItemAt (index) : null;
    }


    /**
     * Get the scene at the absolute position if it is in the window.
     *
     * @param position The position
     * @return The scene or null
     */
    Scene getScene (final int position)
    {
        final int index = position - this.sceneBank.scrollPosition ().get ();
        return index >= 0 && index < this.numScenes ? this.sceneBank.getItemAt (index) : null;
    }


    private void collectMaster (final ModelReporter reporter)
    {
        reporter.put ("/master/select", this.masterSelected);
        reportVu (reporter, "/master/", this.masterVuLeft, this.masterVuRight);
        this.collectChannel (reporter, "/master/", this.masterTrack);
    }


    /**
     * Report the VU meters (0..1): left, right and the mono value as their average (as ChannelImpl
     * of the Java version).
     */
    private static void reportVu (final ModelReporter reporter, final String prefix, final int left, final int right)
    {
        final double vuLeft = Math.min (left, VU_RANGE - 1) / (double) VU_RANGE;
        final double vuRight = Math.min (right, VU_RANGE - 1) / (double) VU_RANGE;
        reporter.put (prefix + "vuleft", vuLeft);
        reporter.put (prefix + "vuright", vuRight);
        reporter.put (prefix + "vu", (vuLeft + vuRight) / 2.0);
    }


    private void collectChannel (final ModelReporter reporter, final String prefix, final Channel channel)
    {
        reporter.put (prefix + "name", channel.name ().get ());
        reporter.putColor (prefix + "color", channel.color ().red (), channel.color ().green (), channel.color ().blue ());
        reporter.put (prefix + "mute", channel.mute ().get ());
        reporter.put (prefix + "solo", channel.solo ().get ());
        reporter.put (prefix + "volume", channel.volume ().value ().get ());
        reporter.put (prefix + "volume/str", channel.volume ().displayedValue ().get ());
        reporter.put (prefix + "pan", channel.pan ().value ().get ());
        reporter.put (prefix + "pan/str", channel.pan ().displayedValue ().get ());
    }


    private void collectDevice (final ModelReporter reporter)
    {
        final int position = this.cursorDevice.position ().get ();
        reporter.put ("/device/count", this.siblings.itemCount ().get ());
        reporter.put ("/device/exists", this.cursorDevice.exists ().get ());
        reporter.put ("/device/position", this.cursorDevice.exists ().get () ? position : -1);
        reporter.put ("/device/name", this.cursorDevice.name ().get ());
        reporter.put ("/device/bypass", !this.cursorDevice.isEnabled ().get ());
        reporter.put ("/device/window", this.cursorDevice.isWindowOpen ().get ());
        reporter.put ("/device/expand", this.cursorDevice.isExpanded ().get ());

        for (int i = 0; i < NUM_SIBLINGS; i++)
        {
            final Device device = this.siblings.getItemAt (i);
            final String prefix = "/device/sibling/" + (i + 1) + "/";
            final boolean exists = device.exists ().get ();
            reporter.put (prefix + "name", exists ? device.name ().get () : "");
            reporter.put (prefix + "bypass", exists && !device.isEnabled ().get ());
            reporter.put (prefix + "position", exists ? device.position ().get () : -1);
            reporter.put (prefix + "selected", exists && device.position ().get () == position);
        }

        // Each remote control page object shows one page of the device: together a flat list
        final int pageCount = this.pages[0].pageCount ().get ();
        final int numPages = Math.min (pageCount, this.pages.length);
        for (int p = 0; p < this.pages.length; p++)
        {
            final CursorRemoteControlsPage page = this.pages[p];
            final boolean isUsed = p < numPages;
            if (isUsed && page.selectedPageIndex ().get () != p)
                page.selectedPageIndex ().set (p);
            for (int i = 0; i < PARAMS_PER_PAGE; i++)
            {
                final Parameter parameter = page.getParameter (i);
                final String prefix = "/device/param/" + (p * PARAMS_PER_PAGE + i) + "/";
                final boolean exists = isUsed && parameter.exists ().get ();
                reporter.put (prefix + "name", exists ? parameter.name ().get () : "");
                reporter.put (prefix + "value", exists ? parameter.value ().get () : 0);
                reporter.put (prefix + "value/str", exists ? parameter.displayedValue ().get () : "");
                reporter.put (prefix + "steps", -1);
                reporter.put (prefix + "modulated", exists ? parameter.modulatedValue ().get () : 0);
            }
        }
        reporter.put ("/device/param/count", numPages * PARAMS_PER_PAGE);

        // The page names and the page selected in Bitwig (protocol v2)
        final String [] pageNames = this.remoteControls.pageNames ().get ();
        final int namedPages = Math.min (pageNames.length, this.pages.length);
        for (int p = 0; p < namedPages; p++)
            reporter.put ("/device/page/" + p + "/name", pageNames[p]);
        reporter.put ("/device/page/count", namedPages);
        reporter.put ("/device/page/selected", this.remoteControls.selectedPageIndex ().get ());
    }


    private static String trackType (final Track track)
    {
        if (track.isGroup ().get ())
            return "GROUP";
        final String type = track.trackType ().get ().toUpperCase (Locale.ROOT);
        return type.isEmpty () ? "HYBRID" : type;
    }




    private static int getInt (final String json, final String key, final int defaultValue)
    {
        final Matcher matcher = Pattern.compile ("\"" + Pattern.quote (key) + "\"\\s*:\\s*(-?\\d+)").matcher (json);
        return matcher.find () ? Integer.parseInt (matcher.group (1)) : defaultValue;
    }
}
