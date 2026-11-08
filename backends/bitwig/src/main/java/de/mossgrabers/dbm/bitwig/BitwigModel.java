// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Application;
import com.bitwig.extension.controller.api.Arpeggiator;
import com.bitwig.extension.controller.api.Arranger;
import com.bitwig.extension.controller.api.Channel;
import com.bitwig.extension.controller.api.ClipLauncherSlot;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.CueMarker;
import com.bitwig.extension.controller.api.CueMarkerBank;
import com.bitwig.extension.controller.api.CursorDeviceFollowMode;
import com.bitwig.extension.controller.api.CursorRemoteControlsPage;
import com.bitwig.extension.controller.api.CursorTrack;
import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.DeviceBank;
import com.bitwig.extension.controller.api.DeviceMatcher;
import com.bitwig.extension.controller.api.Groove;
import com.bitwig.extension.controller.api.MasterTrack;
import com.bitwig.extension.controller.api.Mixer;
import com.bitwig.extension.controller.api.Parameter;
import com.bitwig.extension.controller.api.PlayingNote;
import com.bitwig.extension.controller.api.Project;
import com.bitwig.extension.controller.api.Scene;
import com.bitwig.extension.controller.api.SceneBank;
import com.bitwig.extension.controller.api.PinnableCursorDevice;
import com.bitwig.extension.controller.api.Send;
import com.bitwig.extension.controller.api.StringValue;
import com.bitwig.extension.controller.api.Track;
import com.bitwig.extension.controller.api.TrackBank;
import com.bitwig.extension.controller.api.Transport;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import de.mossgrabers.dbm.bitwig.Protocol.Msg;


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
    /** The number of drum pads: all pads of a drum machine, the core pages over them. */
    public static final int  NUM_DRUM_PADS       = 128;
    /** The number of layers of the first instrument: all layers, the core pages over them. */
    public static final int  NUM_LAYERS          = 32;
    /** The Bitwig Drum Machine (ModelImpl.INSTRUMENT_DRUM_MACHINE of the Java version). */
    private static final UUID INSTRUMENT_DRUM_MACHINE = UUID.fromString ("8ea97e45-0255-40fd-bc7e-94419741e9d1");
    /** The size of the bank to move the cursor device (CursorDeviceImpl.NUM_DEVICES_LARGE_BANK of the Java version). */
    public static final int  NUM_LARGE_DEVICES   = 100;
    /** The number of cue markers: all markers, the core pages over them (MarkerBankImpl.NUM_LARGE_MARKER_BANK of the Java version). */
    public static final int  NUM_MARKERS         = 100;
    /** The pre-roll lengths of Bitwig by the number of bars (TransportImpl of the Java version). */
    static final String []   PREROLLS            = { "none", "one_bar", "two_bars", null, "four_bars" };
    /** The size of the device sibling window, as in the Reaper backend. */
    public static final int  NUM_SIBLINGS        = 8;
    /** The parameters per remote control page. */
    public static final int  PARAMS_PER_PAGE     = 8;
    /** The Bitwig arpeggiator modes in the order of the core's modes (Down, Up, Down-Up, Up-Down). */
    static final List<String> ARPEGGIATOR_MODES = List.of ("down", "up", "down-up", "up-down");
    /** VU meter resolution, as ChannelImpl of the Java version. */
    private static final int VU_RANGE            = 16384;

    final ControllerHost     host;
    final Transport          transport;
    /** The groove and its parameters in the order of GrooveImpl (GROOVE_PARAMETERS). */
    final Groove             groove;
    final Parameter []       grooveParameters;
    /** The names of the groove parameters for the core. */
    static final String []   GROOVE_PARAMETERS = { "enabled", "shuffleAmount", "shuffleRate", "accentAmount", "accentRate", "accentPhase" };
    /** All command forms of the core's HostFeature (see HOST_BRIDGE_GUIDE.md §2). */
    private static final String HOST_FEATURES = "valueInc valueReset immediately indication soloToggle transportCommands deviceCommands layoutStep";
    /** The feature of the clip launcher navigator, only if the controller wants it. */
    private static final String HOST_FEATURE_NAVIGATOR = "clipLauncherNavigator";
    final Application        application;
    final Arranger           arranger;
    final Mixer              mixer;
    /** The window of tracks, with sends and slots. */
    final TrackBank          trackBank;
    /** All tracks, without sends and slots, for commands to tracks outside of the window. */
    final TrackBank          largeTrackBank;
    /**
     * The offset of the window which the core requested ("bank track") until Bitwig reports it,
     * -1 if none: the core pages at once and sends the commands of the new page right after, but
     * Bitwig updates the scroll position later.
     */
    private int              requestedTrackOffset = -1;
    /** When the offset was requested (System.nanoTime), for Bitwig limiting the scroll position. */
    private long             requestedTrackOffsetTime;
    /** The window of scenes; the slot banks of the window tracks follow it. */
    final SceneBank          sceneBank;
    /** The size of the track window. */
    final int                numTracks;
    /** The size of the scene window. */
    final int                numScenes;
    final Project            project;
    /** The project's remote controls (ProjectImpl: on the root track group), empty if not needed. */
    final CursorRemoteControlsPage [] projectPages;
    /** Follows the project page selected in Bitwig, null if there are no project pages. */
    final CursorRemoteControlsPage projectRemoteControls;
    final CursorTrack        cursorTrack;
    /** Follows the selection, also while the cursor track is pinned (for the selection reports). */
    final CursorTrack        selectionTrack;
    final MasterTrack        masterTrack;
    final PinnableCursorDevice cursorDevice;
    final DeviceBank         siblings;
    /** All devices of the cursor device's chain, to move the cursor device (CursorDeviceImpl.largeDeviceBank). */
    final DeviceBank         largeDeviceBank;
    /** The cue markers of the arranger (MarkerBankImpl), null if the controller has no markers. */
    final CueMarkerBank      markerBank;
    /** The remote controls of the cursor track (CursorTrackImpl.getParameterBank), empty if not needed. */
    final CursorRemoteControlsPage [] trackPages;
    /** The remote controls of the cursor track which follow the page selected in Bitwig. */
    final CursorRemoteControlsPage trackRemoteControls;
    final CursorRemoteControlsPage [] pages;
    /** Follows the page selected in Bitwig (as the remote controls page of the Java version). */
    final CursorRemoteControlsPage remoteControls;
    final int                numSends;
    /** The first drum machine of the cursor track, null if the controller does not need it. */
    final SpecificDevice     drumDevice;
    /** The first instrument of the cursor track, null if the controller does not need it. */
    final SpecificDevice     firstInstrument;
    /** The effect tracks (ModelImpl: createEffectTrackBank with numFxTracks, -1 = numTracks). */
    final TrackBank          effectTrackBank;
    final int                numFxTracks;
    private final boolean [] fxTrackSelected;
    /** The clip launcher navigator, null if the controller does not want it. */
    final ClipLauncherNavigator navigator;
    /** The layers of the cursor device (and its hasLayers / hasDrumPads). */
    final SpecificDevice     cursorDeviceLayers;
    /** The first EQ+ of the cursor track; null if the controller does not use it. */
    final EqualizerDevice    eqDevice;
    /** The popup browser, null if the controller does not need it. */
    final BitwigBrowser      browser;
    /** The step grids of the note clips of the core (model setup "noteClips"). */
    final List<BitwigNoteClip> noteClips = new ArrayList<> ();
    /** The clip for the clip properties and commands: the first note clip or an extra one. */
    final BitwigNoteClip     cursorClip;
    /** The note repeat of the default note input, null if the controller has none. */
    final Arpeggiator        arpeggiator;

    private final int []     trackVuLeft;
    private final int []     trackVuRight;
    private final boolean [] sceneSelected;
    /** The position of the selected track which was reported last, -1 if none. */
    private int              lastSelectedTrack   = -1;
    /** The position of the cursor track (selected or pinned) which was reported last. */
    private int              lastCursorTrack     = -1;
    private boolean          masterSelected;
    private int              masterVuLeft;
    private int              masterVuRight;
    /** The pitch and velocity of the playing notes of the cursor track, in pairs (reused). */
    private int []           playingNotes        = new int [32];


    /**
     * Create all Bitwig objects; must be called during init.
     *
     * @param host The host
     * @param modelSetup The model setup of the core as JSON (see dbm_get_model_setup)
     * @param arpeggiator The arpeggiator of the default note input (note repeat), may be null
     */
    public BitwigModel (final ControllerHost host, final String modelSetup, final Arpeggiator arpeggiator)
    {
        this.host = host;
        this.arpeggiator = arpeggiator;
        this.numSends = Math.max (0, getInt (modelSetup, "numSends", 8));
        this.numTracks = Math.max (1, getInt (modelSetup, "numTracks", 8));
        this.numScenes = Math.max (1, getInt (modelSetup, "numScenes", 8));
        this.trackVuLeft = new int [this.numTracks];
        this.trackVuRight = new int [this.numTracks];
        this.sceneSelected = new boolean [this.numScenes];
        final int numParamPages = Math.max (1, getInt (modelSetup, "numParamPages", 8));

        this.transport = host.createTransport ();
        // As GrooveImpl of the Java version
        this.groove = host.createGroove ();
        this.grooveParameters = new Parameter [] { this.groove.getEnabled (), this.groove.getShuffleAmount (), this.groove.getShuffleRate (), this.groove.getAccentAmount (), this.groove.getAccentRate (), this.groove.getAccentPhase () };
        this.application = host.createApplication ();
        this.arranger = host.createArranger ();
        this.mixer = host.createMixer ();
        // As ModelImpl of the Java version: with effect tracks and the master only for a full
        // flat track list
        if (getBoolean (modelSetup, "hasFullFlatTrackList", false))
            this.trackBank = host.createTrackBank (this.numTracks, this.numSends, this.numScenes, true);
        else
            this.trackBank = host.createMainTrackBank (this.numTracks, this.numSends, this.numScenes);
        this.largeTrackBank = host.createTrackBank (NUM_LARGE_TRACKS, 0, 0, true);
        this.sceneBank = this.trackBank.sceneBank ();
        final int fxTracks = getInt (modelSetup, "numFxTracks", -1);
        this.numFxTracks = fxTracks < 0 ? this.numTracks : fxTracks;
        this.effectTrackBank = host.createEffectTrackBank (this.numFxTracks, this.numSends, 0);
        this.fxTrackSelected = new boolean [this.numFxTracks];
        this.project = host.getProject ();
        // As ProjectImpl: the remote controls of the root track group, in pages as for devices
        final int numProjectPages = getInt (modelSetup, "numParams", 8) > 0 ? numParamPages : 0;
        final Track rootTrackGroup = this.project.getRootTrackGroup ();
        this.projectPages = new CursorRemoteControlsPage [numProjectPages];
        for (int i = 0; i < numProjectPages; i++)
            this.projectPages[i] = rootTrackGroup.createCursorRemoteControlsPage ("DBM project page " + (i + 1), PARAMS_PER_PAGE, "");
        this.projectRemoteControls = numProjectPages > 0 ? rootTrackGroup.createCursorRemoteControlsPage (PARAMS_PER_PAGE) : null;
        this.cursorTrack = host.createCursorTrack ("DBM_CURSOR_TRACK", "Cursor track", this.numSends, this.numScenes, true);
        this.selectionTrack = host.createCursorTrack ("DBM_SELECTION_TRACK", "Selected track", 0, 0, true);
        this.masterTrack = host.createMasterTrack (0);
        this.cursorDevice = this.cursorTrack.createCursorDevice ("DBM_CURSOR_DEVICE", "Cursor device", 0, CursorDeviceFollowMode.FOLLOW_SELECTION);
        this.siblings = this.cursorDevice.createSiblingsDeviceBank (NUM_SIBLINGS);
        // The layers of the cursor device (CursorDeviceImpl: layer bank of numDeviceLayers), with
        // hasLayers / hasDrumPads, which the layer commands of the controllers check
        this.cursorDeviceLayers = new SpecificDevice (this.cursorDevice, Protocol.Dev.DEVICE, 0, 0, Math.max (0, getInt (modelSetup, "numDeviceLayers", 8)));
        this.largeDeviceBank = this.cursorDevice.channel ().createDeviceBank (NUM_LARGE_DEVICES);
        this.remoteControls = this.cursorDevice.createCursorRemoteControlsPage (PARAMS_PER_PAGE);
        // CursorTrackImpl: the remote controls of the cursor track, in pages as for devices
        this.trackPages = new CursorRemoteControlsPage [numProjectPages];
        for (int i = 0; i < numProjectPages; i++)
            this.trackPages[i] = this.cursorTrack.createCursorRemoteControlsPage ("DBM track page " + (i + 1), PARAMS_PER_PAGE, "");
        this.trackRemoteControls = numProjectPages > 0 ? this.cursorTrack.createCursorRemoteControlsPage (PARAMS_PER_PAGE) : null;
        // ClipLauncherNavigatorImpl, if the controller wants it (model setup)
        this.navigator = getBoolean (modelSetup, "wantsClipLauncherNavigator", false) ? new ClipLauncherNavigator (host, this.sceneBank, this.numScenes) : null;
        // MarkerBankImpl: the cue markers of the arranger, if the controller uses markers
        this.markerBank = getInt (modelSetup, "numMarkers", 0) > 0 ? this.arranger.createCueMarkerBank (NUM_MARKERS) : null;

        // The drum machine of the cursor track, as ModelImpl of the Java version
        if (getBoolean (modelSetup, "wantsDrumDevice", true))
            this.drumDevice = new SpecificDevice (this.createDevice (host.createBitwigDeviceMatcher (INSTRUMENT_DRUM_MACHINE)), SpecificDevice.DRUM, 0, NUM_DRUM_PADS, 0);
        else
            this.drumDevice = null;
        // The first instrument of the cursor track (ModelImpl, DeviceID.FIRST_INSTRUMENT)
        if (modelSetup.contains ("\"FIRST_INSTRUMENT\""))
            this.firstInstrument = new SpecificDevice (this.createDevice (host.createInstrumentMatcher ()), Protocol.Dev.PRIMARY, numParamPages, NUM_DRUM_PADS, NUM_LAYERS);
        else
            this.firstInstrument = null;
        // The first EQ+ of the cursor track (ModelImpl, DeviceID.EQ)
        if (modelSetup.contains ("\"EQ\""))
            this.eqDevice = new EqualizerDevice (this.createDevice (host.createBitwigDeviceMatcher (EqualizerDevice.ID_BITWIG_EQ_PLUS)));
        else
            this.eqDevice = null;
        final int numResults = getInt (modelSetup, "numResults", 16);
        this.browser = numResults > 0 ? new BitwigBrowser (host, Math.max (1, getInt (modelSetup, "numFilterColumnEntries", 16)), numResults) : null;
        // The note clips, as ModelImpl.getNoteClip: one launcher cursor clip per size
        final Matcher noteClips = Pattern.compile ("\\[(\\d+),(\\d+)\\]").matcher (getArray (modelSetup, "noteClips"));
        while (noteClips.find ())
            this.noteClips.add (new BitwigNoteClip (host, this.cursorTrack, Integer.parseInt (noteClips.group (1)), Integer.parseInt (noteClips.group (2))));
        this.cursorClip = this.noteClips.isEmpty () ? new BitwigNoteClip (host, this.cursorTrack, 1, 1) : this.noteClips.get (0);

        this.pages = new CursorRemoteControlsPage [numParamPages];
        for (int i = 0; i < numParamPages; i++)
            this.pages[i] = this.cursorDevice.createCursorRemoteControlsPage ("DBM page " + (i + 1), PARAMS_PER_PAGE, "");

        this.markInterested ();
    }


    /** The first device of the cursor track which the matcher accepts, as ModelImpl.createDevice. */
    private Device createDevice (final DeviceMatcher matcher)
    {
        final DeviceBank deviceBank = this.cursorTrack.createDeviceBank (1);
        deviceBank.setDeviceMatcher (matcher);
        return deviceBank.getItemAt (0);
    }


    private void markInterested ()
    {
        this.transport.isPlaying ().markInterested ();
        this.cursorTrack.playingNotes ().markInterested ();
        this.transport.isArrangerRecordEnabled ().markInterested ();
        this.transport.isArrangerLoopEnabled ().markInterested ();
        this.transport.isMetronomeEnabled ().markInterested ();
        this.transport.isArrangerAutomationWriteEnabled ().markInterested ();
        this.transport.isClipLauncherAutomationWriteEnabled ().markInterested ();
        this.transport.automationWriteMode ().markInterested ();
        this.transport.isArrangerOverdubEnabled ().markInterested ();
        this.transport.isClipLauncherOverdubEnabled ().markInterested ();
        this.transport.isMetronomeAudibleDuringPreRoll ().markInterested ();
        this.transport.isPunchInEnabled ().markInterested ();
        this.transport.isPunchOutEnabled ().markInterested ();
        this.transport.isFillModeActive ().markInterested ();
        this.transport.isMetronomeTickPlaybackEnabled ().markInterested ();
        this.arranger.isPlaybackFollowEnabled ().markInterested ();
        this.transport.metronomeVolume ().markInterested ();
        this.transport.metronomeVolume ().displayedValue ().markInterested ();
        this.transport.tempo ().value ().markInterested ();
        this.transport.getPosition ().markInterested ();
        this.transport.playStartPosition ().markInterested ();
        this.transport.arrangerLoopStart ().markInterested ();
        this.transport.arrangerLoopDuration ().markInterested ();
        this.arranger.getHorizontalScrollbarModel ().getContentPerPixel ().markInterested ();
        // TransportImpl: the pre-roll length, the crossfader
        this.transport.preRoll ().markInterested ();
        markParameter (this.transport.crossfade ());
        this.transport.timeSignature ().numerator ().markInterested ();
        this.transport.timeSignature ().denominator ().markInterested ();
        this.application.canUndo ().markInterested ();
        this.application.canRedo ().markInterested ();
        // ApplicationImpl, ProjectImpl, ArrangerImpl, MixerImpl of the Java version
        this.application.hasActiveEngine ().markInterested ();
        this.application.panelLayout ().markInterested ();
        this.application.recordQuantizationGrid ().markInterested ();
        this.application.recordQuantizeNoteLength ().markInterested ();
        this.application.projectName ().markInterested ();
        this.project.isModified ().markInterested ();
        this.project.hasSoloedTracks ().markInterested ();
        this.project.hasMutedTracks ().markInterested ();
        for (final Parameter parameter: this.grooveParameters)
            markParameter (parameter);
        markParameter (this.project.cueVolume ());
        markParameter (this.project.cueMix ());
        this.arranger.areCueMarkersVisible ().markInterested ();
        this.arranger.hasDoubleRowTrackHeight ().markInterested ();
        this.arranger.isClipLauncherVisible ().markInterested ();
        this.arranger.isTimelineVisible ().markInterested ();
        this.arranger.isIoSectionVisible ().markInterested ();
        this.arranger.areEffectTracksVisible ().markInterested ();
        this.mixer.isClipLauncherSectionVisible ().markInterested ();
        this.mixer.isCrossFadeSectionVisible ().markInterested ();
        this.mixer.isDeviceSectionVisible ().markInterested ();
        this.mixer.isIoSectionVisible ().markInterested ();
        this.mixer.isMeterSectionVisible ().markInterested ();
        this.mixer.isSendSectionVisible ().markInterested ();

        this.trackBank.itemCount ().markInterested ();
        this.trackBank.scrollPosition ().addValueObserver (offset -> {
            // Bitwig handled the scroll (possibly limited to the existing tracks)
            this.requestedTrackOffset = -1;
        });
        this.cursorTrack.position ().markInterested ();
        this.selectionTrack.position ().markInterested ();
        this.cursorTrack.exists ().markInterested ();
        // CursorTrackImpl: isPinned (), hasPrevious (), hasNext ()
        this.cursorTrack.isPinned ().markInterested ();
        this.cursorTrack.hasPrevious ().markInterested ();
        this.cursorTrack.hasNext ().markInterested ();
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
            track.isMonitoring ().markInterested ();
            track.canHoldNoteData ().markInterested ();
            track.canHoldAudioData ().markInterested ();
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
                send.sendChannelColor ().markInterested ();
            }
        }

        this.effectTrackBank.itemCount ().markInterested ();
        this.effectTrackBank.scrollPosition ().markInterested ();
        for (int i = 0; i < this.numFxTracks; i++)
        {
            final Track track = this.effectTrackBank.getItemAt (i);
            final int index = i;
            markChannel (track);
            track.trackType ().markInterested ();
            track.isGroup ().markInterested ();
            track.arm ().markInterested ();
            track.monitorMode ().markInterested ();
            track.isMonitoring ().markInterested ();
            track.canHoldNoteData ().markInterested ();
            track.canHoldAudioData ().markInterested ();
            track.addIsSelectedInEditorObserver (selected -> this.fxTrackSelected[index] = selected);
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
        this.cursorDevice.isPinned ().markInterested ();
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
            markPage (page);
        for (final CursorRemoteControlsPage page: this.projectPages)
            markPage (page);
        for (final CursorRemoteControlsPage page: this.trackPages)
            markPage (page);
        if (this.markerBank != null)
        {
            // MarkerImpl
            this.markerBank.itemCount ().markInterested ();
            for (int i = 0; i < NUM_MARKERS; i++)
            {
                final CueMarker marker = this.markerBank.getItemAt (i);
                marker.exists ().markInterested ();
                marker.name ().markInterested ();
                marker.getColor ().markInterested ();
                marker.position ().markInterested ();
            }
        }
        if (this.projectRemoteControls != null)
        {
            this.projectRemoteControls.pageCount ().markInterested ();
            this.projectRemoteControls.pageNames ().markInterested ();
            this.projectRemoteControls.selectedPageIndex ().markInterested ();
        }
        if (this.trackRemoteControls != null)
        {
            this.trackRemoteControls.pageCount ().markInterested ();
            this.trackRemoteControls.pageNames ().markInterested ();
            this.trackRemoteControls.selectedPageIndex ().markInterested ();
        }
    }


    static void markPage (final CursorRemoteControlsPage page)
    {
        page.pageCount ().markInterested ();
        page.selectedPageIndex ().markInterested ();
        for (int i = 0; i < PARAMS_PER_PAGE; i++)
            markParameter (page.getParameter (i));
    }


    static void markChannel (final Channel channel)
    {
        channel.exists ().markInterested ();
        channel.name ().markInterested ();
        channel.color ().markInterested ();
        channel.mute ().markInterested ();
        channel.solo ().markInterested ();
        channel.isActivated ().markInterested ();
        markParameter (channel.volume ());
        markParameter (channel.pan ());
    }


    static void markParameter (final Parameter parameter)
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
        // The command forms this extension handles (HostFeature of the core); reported once and
        // with every dump
        reporter.put (Msg.HOST_FEATURES, this.navigator == null ? HOST_FEATURES : HOST_FEATURES + " " + HOST_FEATURE_NAVIGATOR);
        this.collectTransport (reporter);
        this.collectTracks (reporter);
        this.collectEffectTracks (reporter);
        if (this.trackPages.length > 0)
        {
            collectPages (reporter, Protocol.ParamOwner.TRACK_FX, this.trackPages);
            collectPageNames (reporter, Protocol.PageOwner.TRACK_FX, this.trackRemoteControls, this.trackPages.length);
        }
        this.collectMarkers (reporter);
        this.collectScenes (reporter);
        this.collectMaster (reporter);
        this.collectDevice (reporter);
        this.cursorDeviceLayers.collect (reporter);
        this.collectNoteRepeat (reporter);
        if (this.drumDevice != null)
            this.drumDevice.collect (reporter);
        if (this.firstInstrument != null)
            this.firstInstrument.collect (reporter);
        if (this.eqDevice != null)
            this.eqDevice.collect (reporter);
        if (this.browser != null)
            this.browser.collect (reporter);
        this.cursorClip.collectClip (reporter);
        for (final BitwigNoteClip noteClip: this.noteClips)
            noteClip.collectGrid (reporter);
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
        reporter.put (Msg.NOTE_REPEAT_ACTIVE, this.arpeggiator.isEnabled ().get ());
        if (period > 0)
            reporter.put (Msg.NOTE_REPEAT_PERIOD, 1.0 / period);
        reporter.put (Msg.NOTE_REPEAT_NOTE_LENGTH, this.arpeggiator.gateLength ().get ());
        reporter.put (Msg.NOTE_REPEAT_MODE, Math.max (0, ARPEGGIATOR_MODES.indexOf (this.arpeggiator.mode ().get ())));
        reporter.put (Msg.NOTE_REPEAT_VELOCITY, this.arpeggiator.usePressureToVelocity ().get ());
    }


    /** The index of the automation mode Read of the core. */
    static final int AUTOMATION_READ = 1;
    /** Bitwig's automation write modes by the index of the core's mode. */
    static final String [] AUTOMATION_MODES = { null, null, "touch", "write", "latch" };


    private static int automationModeIndex (final String mode)
    {
        for (int i = 0; i < AUTOMATION_MODES.length; i++)
            if (AUTOMATION_MODES[i] != null && AUTOMATION_MODES[i].equals (mode))
                return i;
        return AUTOMATION_READ;
    }


    private void collectTransport (final ModelReporter reporter)
    {
        final double tempo = this.transport.tempo ().value ().getRaw ();
        final double beats = this.transport.getPosition ().get ();
        final double seconds = tempo > 0 ? beats * 60.0 / tempo : 0;
        final double contentPerPixel = this.arranger.getHorizontalScrollbarModel ().getContentPerPixel ().get ();

        reporter.put (Msg.PLAY, this.transport.isPlaying ().get ());
        reporter.put (Msg.RECORD, this.transport.isArrangerRecordEnabled ().get ());
        reporter.put (Msg.REPEAT, this.transport.isArrangerLoopEnabled ().get ());
        reporter.put (Msg.TEMPO, tempo);
        reporter.put (Msg.NUMERATOR, this.transport.timeSignature ().numerator ().get ());
        reporter.put (Msg.DENOMINATOR, this.transport.timeSignature ().denominator ().get ());
        reporter.put (Msg.TIME, seconds);

        // As the Java version: the position (for moving it) is the play start position, the
        // texts show the play position; the zoom is the inverse of the content per pixel
        reporter.put (Msg.BEATS_POSITION, this.transport.playStartPosition ().get ());
        reporter.put (Msg.BEATS_LOOP_START, this.transport.arrangerLoopStart ().get ());
        reporter.put (Msg.BEATS_LOOP_LENGTH, this.transport.arrangerLoopDuration ().get ());
        reporter.put (Msg.TIME_TEXT, tempo > 0 ? TimeFormat.formatTimeLong (tempo, beats) : "");
        reporter.put (Msg.BEATS_TEXT, this.transport.getPosition ().getFormatted (TimeFormat.BEAT_POSITION_FORMATTER));
        reporter.put (Msg.BEATS_LOOP_START_TEXT, this.transport.arrangerLoopStart ().getFormatted (TimeFormat.BEAT_POSITION_FORMATTER));
        reporter.put (Msg.BEATS_LOOP_LENGTH_TEXT, this.transport.arrangerLoopDuration ().getFormatted (TimeFormat.BEAT_LENGTH_FORMATTER));
        if (contentPerPixel > 0)
            reporter.put (Msg.TIME_H_ZOOM, 1.0 / contentPerPixel);
        reporter.put (Msg.METRONOME, this.transport.isMetronomeEnabled ().get ());
        // The automation write switches, and the mode as the index of the core's mode (Reaper);
        // Read if no switch is on, as TransportImpl.getAutomationWriteMode
        final boolean arrangerAutomation = this.transport.isArrangerAutomationWriteEnabled ().get ();
        final boolean launcherAutomation = this.transport.isClipLauncherAutomationWriteEnabled ().get ();
        // The overdub switches, as TransportImpl.isArrangerOverdub / isLauncherOverdub
        reporter.put (Msg.OVERDUB_ARRANGER, this.transport.isArrangerOverdubEnabled ().get ());
        reporter.put (Msg.OVERDUB_LAUNCHER, this.transport.isClipLauncherOverdubEnabled ().get ());
        reporter.put (Msg.AUTOMATION_ARRANGER, arrangerAutomation);
        reporter.put (Msg.AUTOMATION_LAUNCHER, launcherAutomation);
        reporter.put (Msg.AUTOMATION_MODE, arrangerAutomation || launcherAutomation ? automationModeIndex (this.transport.automationWriteMode ().get ()) : AUTOMATION_READ);
        // As Reaper's pre-roll bit field: 2 = the metronome is audible during the pre-roll
        reporter.put (Msg.METRONOME_PREROLL, this.transport.isMetronomeAudibleDuringPreRoll ().get () ? 2 : 0);
        // TransportImpl.getPrerollMeasures
        reporter.put (Msg.METRONOME_PREROLL_MEASURES, Math.max (0, Arrays.asList (PREROLLS).indexOf (this.transport.preRoll ().get ())));
        // TransportImpl.getCrossfadeParameter
        final Parameter crossfade = this.transport.crossfade ();
        final long crossfadeKey = hostParamKey (Protocol.HostParamKind.CROSSFADE, 0);
        reporter.put (Msg.HOST_PARAM_NAME | crossfadeKey, crossfade.name ().get ());
        reporter.put (Msg.HOST_PARAM_VALUE | crossfadeKey, crossfade.value ().get ());
        putDisplayedValue (reporter, Msg.HOST_PARAM_STR | crossfadeKey, crossfade.displayedValue (), true);
        reporter.put (Msg.FOLLOW_PLAYBACK, this.arranger.isPlaybackFollowEnabled ().get ());
        reporter.put (Msg.METRONOME_VOLUME, this.transport.metronomeVolume ().get ());
        // As TransportImpl.getMetronomeVolumeStr
        reporter.put (Msg.METRONOME_VOLUME_STR, this.transport.metronomeVolume ().displayedValue ().getLimited (6) + " dB");
        reporter.put (Msg.METRONOME_TICKS, this.transport.isMetronomeTickPlaybackEnabled ().get ());
        // Independent punch switches, the fill mode (TransportImpl)
        reporter.put (Msg.PUNCH_IN, this.transport.isPunchInEnabled ().get ());
        reporter.put (Msg.PUNCH_OUT, this.transport.isPunchOutEnabled ().get ());
        reporter.put (Msg.FILL, this.transport.isFillModeActive ().get ());
        reporter.put (Msg.PROJECT_CAN_UNDO, this.application.canUndo ().get ());
        reporter.put (Msg.PROJECT_CAN_REDO, this.application.canRedo ().get ());
        this.collectApplication (reporter);
    }


    /**
     * The engine, the project's name and modified state, the panel layout, the project's solo and
     * mute state, cue volume and mix, the visibility flags of the arranger and the mixer, as
     * ApplicationImpl, ProjectImpl, ArrangerImpl and MixerImpl of the Java version.
     */
    private void collectApplication (final ModelReporter reporter)
    {
        reporter.put (Msg.PROJECT_ENGINE, this.application.hasActiveEngine ().get ());
        // ProjectImpl.getName, isDirty
        reporter.put (Msg.PROJECT_NAME, this.application.projectName ().get ());
        reporter.put (Msg.PROJECT_IS_DIRTY, this.project.isModified ().get ());
        reporter.put (Msg.LAYOUT, this.application.panelLayout ().get ());
        reporter.put (Msg.RECORD_QUANTIZATION, this.application.recordQuantizationGrid ().get ());
        reporter.put (Msg.RECORD_QUANTIZATION_NOTE_LENGTH, this.application.recordQuantizeNoteLength ().get ());
        reporter.put (Msg.PROJECT_HAS_SOLO, this.project.hasSoloedTracks ().get ());
        reporter.put (Msg.PROJECT_HAS_MUTE, this.project.hasMutedTracks ().get ());
        final long cueVolume = hostParamKey (Protocol.HostParamKind.CUE, Protocol.CueParam.VOLUME);
        reporter.put (Msg.HOST_PARAM_VALUE | cueVolume, this.project.cueVolume ().value ().get ());
        putDisplayedValue (reporter, Msg.HOST_PARAM_STR | cueVolume, this.project.cueVolume ().displayedValue (), true);
        final long cueMix = hostParamKey (Protocol.HostParamKind.CUE, Protocol.CueParam.MIX);
        reporter.put (Msg.HOST_PARAM_VALUE | cueMix, this.project.cueMix ().value ().get ());
        putDisplayedValue (reporter, Msg.HOST_PARAM_STR | cueMix, this.project.cueMix ().displayedValue (), true);
        reporter.put (Msg.ARRANGER | flagKey (Protocol.ArrangerFlag.CUE_MARKERS), this.arranger.areCueMarkersVisible ().get ());
        reporter.put (Msg.ARRANGER | flagKey (Protocol.ArrangerFlag.DOUBLE_ROW_TRACK_HEIGHT), this.arranger.hasDoubleRowTrackHeight ().get ());
        reporter.put (Msg.ARRANGER | flagKey (Protocol.ArrangerFlag.CLIP_LAUNCHER), this.arranger.isClipLauncherVisible ().get ());
        reporter.put (Msg.ARRANGER | flagKey (Protocol.ArrangerFlag.TIMELINE), this.arranger.isTimelineVisible ().get ());
        reporter.put (Msg.ARRANGER | flagKey (Protocol.ArrangerFlag.IO_SECTION), this.arranger.isIoSectionVisible ().get ());
        reporter.put (Msg.ARRANGER | flagKey (Protocol.ArrangerFlag.EFFECT_TRACKS), this.arranger.areEffectTracksVisible ().get ());
        reporter.put (Msg.MIXER | flagKey (Protocol.MixerFlag.CLIP_LAUNCHER_SECTION), this.mixer.isClipLauncherSectionVisible ().get ());
        reporter.put (Msg.MIXER | flagKey (Protocol.MixerFlag.CROSS_FADE_SECTION), this.mixer.isCrossFadeSectionVisible ().get ());
        reporter.put (Msg.MIXER | flagKey (Protocol.MixerFlag.DEVICE_SECTION), this.mixer.isDeviceSectionVisible ().get ());
        reporter.put (Msg.MIXER | flagKey (Protocol.MixerFlag.IO_SECTION), this.mixer.isIoSectionVisible ().get ());
        reporter.put (Msg.MIXER | flagKey (Protocol.MixerFlag.METER_SECTION), this.mixer.isMeterSectionVisible ().get ());
        reporter.put (Msg.MIXER | flagKey (Protocol.MixerFlag.SEND_SECTION), this.mixer.isSendSectionVisible ().get ());
        if (this.projectRemoteControls != null)
        {
            collectPages (reporter, Protocol.ParamOwner.PROJECT, this.projectPages);
            collectPageNames (reporter, Protocol.PageOwner.PROJECT, this.projectRemoteControls, this.projectPages.length);
        }
        for (int i = 0; i < GROOVE_PARAMETERS.length; i++)
        {
            final Parameter parameter = this.grooveParameters[i];
            final long key = hostParamKey (Protocol.HostParamKind.GROOVE, i);
            reporter.put (Msg.HOST_PARAM_NAME | key, parameter.name ().get ());
            reporter.put (Msg.HOST_PARAM_VALUE | key, parameter.value ().get ());
            putDisplayedValue (reporter, Msg.HOST_PARAM_STR | key, parameter.displayedValue (), true);
        }
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
        // The selection comes from a cursor track which is never pinned; the pinned cursor track
        // (CursorTrackImpl.isPinned) is reported as "/track/pinned" with its position
        final int selected = this.selectionTrack.position ().get ();
        final int cursor = this.cursorTrack.position ().get ();
        reporter.put (Msg.TRACK_PINNED, this.cursorTrack.isPinned ().get () ? cursor : -1);
        reporter.put (Msg.TRACK_HAS_PREVIOUS, this.cursorTrack.hasPrevious ().get ());
        reporter.put (Msg.TRACK_HAS_NEXT, this.cursorTrack.hasNext ().get ());
        reporter.put (Msg.TRACK_OFFSET, offset);
        if (selected != this.lastSelectedTrack)
        {
            if (this.lastSelectedTrack >= 0)
                reporter.put (Msg.CHANNEL_SELECTED | channelKey (Protocol.ChKind.TRACK, this.lastSelectedTrack), false);
            this.lastSelectedTrack = selected;
        }
        if (cursor != this.lastCursorTrack)
        {
            this.lastCursorTrack = cursor;
            // The core keeps one note cache: report the notes of the new cursor track; the
            // remote controls belong to the track
            reporter.forget (Msg.CHANNEL_PLAYING_NOTES | channelKey (Protocol.ChKind.TRACK, cursor), 0);
            reporter.forget (new ModelReporter.Entity (ModelReporter.Family.OWNER, Protocol.ParamOwner.TRACK_FX, 0, 0));
        }

        final int sceneOffset = this.sceneBank.scrollPosition ().get ();
        final int sceneTotal = this.sceneBank.itemCount ().get ();
        for (int i = 0; i < this.numTracks; i++)
        {
            final int position = offset + i;
            if (position >= total)
                break;
            final Track track = this.trackBank.getItemAt (i);
            final long base = channelKey (Protocol.ChKind.TRACK, position);
            reporter.put (Msg.CHANNEL_EXISTS | base, 1);
            reporter.put (Msg.CHANNEL_NUMBER | base, position);
            reporter.put (Msg.CHANNEL_TYPE | base, trackType (track));
            reporter.put (Msg.CHANNEL_IS_GROUP_EXPANDED | base, track.isGroupExpanded ().get ());
            reporter.put (Msg.CHANNEL_SELECTED | base, position == selected);
            reporter.put (Msg.CHANNEL_REC_ARM | base, track.arm ().get ());
            final String monitorMode = track.monitorMode ().get ();
            // TrackImpl.isMonitor: the effective monitoring (also AUTO while armed)
            reporter.put (Msg.CHANNEL_MONITOR | base, track.isMonitoring ().get ());
            reporter.put (Msg.CHANNEL_AUTO_MONITOR | base, "AUTO".equals (monitorMode));
            reporter.put (Msg.CHANNEL_CAN_HOLD_NOTES | base, track.canHoldNoteData ().get ());
            reporter.put (Msg.CHANNEL_CAN_HOLD_AUDIO | base, track.canHoldAudioData ().get ());
            reportVu (reporter, base, this.trackVuLeft[i], this.trackVuRight[i]);
            collectChannel (reporter, base, track);

            final int sendCount = Math.min (track.sendBank ().itemCount ().get (), this.numSends);
            for (int s = 0; s < sendCount; s++)
            {
                final Send send = track.sendBank ().getItemAt (s);
                final long sendBase = base | (long) s << 48;
                reporter.put (Msg.SEND_NAME | sendBase, send.name ().get ());
                reporter.put (Msg.SEND_VOLUME | sendBase, send.value ().get ());
                putDisplayedValue (reporter, Msg.SEND_VOLUME_STR | sendBase, send.displayedValue (), true);
                reporter.put (Msg.SEND_ACTIVE | sendBase, send.isEnabled ().get ());
                // SendImpl.getColor: sendChannelColor ()
                reporter.putColor (Msg.SEND_COLOR | sendBase, send.sendChannelColor ().red (), send.sendChannelColor ().green (), send.sendChannelColor ().blue ());
            }
            reporter.put (Msg.SEND_COUNT | base, sendCount);
            this.collectSlots (reporter, base, track, sceneOffset, sceneTotal);
        }
        reporter.put (Msg.TRACK_COUNT, total);
        if (selected >= 0 && selected < total)
            reporter.put (Msg.CHANNEL_SELECTED | channelKey (Protocol.ChKind.TRACK, selected), true);
        if (cursor >= 0 && cursor < total)
        {
            // The notes which play on the selected track (live and from clips), e.g. to light
            // the pads of the played notes, as the note observer of TrackImpl
            final PlayingNote [] playingNotes = this.cursorTrack.playingNotes ().get ();
            if (this.playingNotes.length < 2 * playingNotes.length)
                this.playingNotes = new int [2 * playingNotes.length];
            for (int n = 0; n < playingNotes.length; n++)
            {
                this.playingNotes[2 * n] = playingNotes[n].pitch ();
                this.playingNotes[2 * n + 1] = playingNotes[n].velocity ();
            }
            reporter.putPlayingNotes (Msg.CHANNEL_PLAYING_NOTES | channelKey (Protocol.ChKind.TRACK, cursor), this.playingNotes, playingNotes.length);
        }
    }


    /**
     * The clip launcher slots of a track (protocol v2), as SlotImpl of the Java version.
     */
    private void collectSlots (final ModelReporter reporter, final long base, final Track track, final int sceneOffset, final int sceneTotal)
    {
        reporter.put (Msg.CHANNEL_IS_STOPPED | base, track.isStopped ().get ());
        reporter.put (Msg.SLOT_COUNT | base, sceneTotal);
        for (int s = 0; s < this.numScenes; s++)
        {
            final int position = sceneOffset + s;
            if (position >= sceneTotal)
                break;
            final ClipLauncherSlot slot = track.clipLauncherSlotBank ().getItemAt (s);
            final long slotBase = base | (long) position << 48;
            reporter.put (Msg.SLOT_EXISTS | slotBase, slot.exists ().get ());
            reporter.put (Msg.SLOT_HAS_CONTENT | slotBase, slot.hasContent ().get ());
            reporter.put (Msg.SLOT_NAME | slotBase, slot.name ().get ());
            // As the Java version: the slots of group tracks have the color of the track (API)
            if (track.isGroup ().get ())
                reporter.putColor (Msg.SLOT_COLOR | slotBase, track.color ().red (), track.color ().green (), track.color ().blue ());
            else
                reporter.putColor (Msg.SLOT_COLOR | slotBase, slot.color ().red (), slot.color ().green (), slot.color ().blue ());
            reporter.put (Msg.SLOT_SELECTED | slotBase, slot.isSelected ().get ());
            reporter.put (Msg.SLOT_PLAYING | slotBase, slot.isPlaying ().get ());
            reporter.put (Msg.SLOT_RECORDING | slotBase, slot.isRecording ().get ());
            reporter.put (Msg.SLOT_PLAY_QUEUED | slotBase, slot.isPlaybackQueued ().get ());
            reporter.put (Msg.SLOT_REC_QUEUED | slotBase, slot.isRecordingQueued ().get ());
            reporter.put (Msg.SLOT_STOP_QUEUED | slotBase, slot.isStopQueued ().get ());
        }
    }


    /**
     * The cue markers with their absolute positions, as MarkerImpl of the Java version (and the
     * marker data of the Reaper backend); the position is in beats.
     */
    private void collectMarkers (final ModelReporter reporter)
    {
        if (this.markerBank == null)
            return;
        for (int i = 0; i < NUM_MARKERS; i++)
        {
            final CueMarker marker = this.markerBank.getItemAt (i);
            final long base = Protocol.key (0, i, 0, 0);
            final boolean exists = marker.exists ().get ();
            reporter.put (Msg.MARKER_EXISTS | base, exists);
            if (!exists)
                continue;
            reporter.put (Msg.MARKER_NUMBER | base, i);
            reporter.put (Msg.MARKER_NAME | base, marker.name ().get ());
            reporter.putColor (Msg.MARKER_COLOR | base, marker.getColor ().red (), marker.getColor ().green (), marker.getColor ().blue ());
            reporter.put (Msg.MARKER_POSITION | base, marker.position ().get ());
        }
        reporter.put (Msg.MARKER_COUNT, Math.min (this.markerBank.itemCount ().get (), NUM_MARKERS));
    }


    private void collectScenes (final ModelReporter reporter)
    {
        final int offset = this.sceneBank.scrollPosition ().get ();
        final int total = this.sceneBank.itemCount ().get ();
        reporter.put (Msg.SCENE_OFFSET, offset);
        for (int i = 0; i < this.numScenes; i++)
        {
            final int position = offset + i;
            if (position >= total)
                break;
            final Scene scene = this.sceneBank.getItemAt (i);
            final long base = Protocol.key (0, position, 0, 0);
            reporter.put (Msg.SCENE_EXISTS | base, scene.exists ().get ());
            reporter.put (Msg.SCENE_NUMBER | base, position);
            reporter.put (Msg.SCENE_NAME | base, scene.name ().get ());
            reporter.putColor (Msg.SCENE_COLOR | base, scene.color ().red (), scene.color ().green (), scene.color ().blue ());
            reporter.put (Msg.SCENE_SELECTED | base, this.sceneSelected[i]);
        }
        reporter.put (Msg.SCENE_COUNT, total);
    }


    /**
     * Report the effect tracks of the window as "/fxtrack/&lt;position&gt;/…" (the attributes of the
     * tracks without slots and VU), "/fxtrack/count" and "/fxtrack/offset" (the core pages them).
     */
    private void collectEffectTracks (final ModelReporter reporter)
    {
        final int offset = this.effectTrackBank.scrollPosition ().get ();
        final int total = this.effectTrackBank.itemCount ().get ();
        reporter.put (Msg.EFFECT_TRACK_OFFSET, offset);
        for (int i = 0; i < this.numFxTracks; i++)
        {
            final int position = offset + i;
            if (position >= total)
                break;
            final Track track = this.effectTrackBank.getItemAt (i);
            final long base = channelKey (Protocol.ChKind.EFFECT, position);
            reporter.put (Msg.CHANNEL_EXISTS | base, 1);
            reporter.put (Msg.CHANNEL_NUMBER | base, position);
            reporter.put (Msg.CHANNEL_TYPE | base, trackType (track));
            reporter.put (Msg.CHANNEL_SELECTED | base, this.fxTrackSelected[i]);
            reporter.put (Msg.CHANNEL_REC_ARM | base, track.arm ().get ());
            reporter.put (Msg.CHANNEL_MONITOR | base, track.isMonitoring ().get ());
            reporter.put (Msg.CHANNEL_AUTO_MONITOR | base, "AUTO".equals (track.monitorMode ().get ()));
            reporter.put (Msg.CHANNEL_CAN_HOLD_NOTES | base, track.canHoldNoteData ().get ());
            reporter.put (Msg.CHANNEL_CAN_HOLD_AUDIO | base, track.canHoldAudioData ().get ());
            collectChannel (reporter, base, track);
            final int sendCount = Math.min (track.sendBank ().itemCount ().get (), this.numSends);
            for (int s = 0; s < sendCount; s++)
            {
                final Send send = track.sendBank ().getItemAt (s);
                final long sendBase = base | (long) s << 48;
                reporter.put (Msg.SEND_NAME | sendBase, send.name ().get ());
                reporter.put (Msg.SEND_VOLUME | sendBase, send.value ().get ());
                putDisplayedValue (reporter, Msg.SEND_VOLUME_STR | sendBase, send.displayedValue (), true);
                reporter.put (Msg.SEND_ACTIVE | sendBase, send.isEnabled ().get ());
            }
            reporter.put (Msg.SEND_COUNT | base, sendCount);
        }
        reporter.put (Msg.EFFECT_TRACK_COUNT, total);
    }


    /**
     * Get the effect track at the absolute position if it is in the window.
     */
    Track getEffectTrack (final int position)
    {
        final int index = position - this.effectTrackBank.scrollPosition ().get ();
        return index >= 0 && index < this.numFxTracks ? this.effectTrackBank.getItemAt (index) : null;
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
     * Scroll the window of tracks (the core's "bank track").
     *
     * @param offset The position of the first track
     */
    void scrollTrackWindow (final int offset)
    {
        final int position = Math.max (0, offset);
        if (position != this.trackBank.scrollPosition ().get ())
        {
            this.requestedTrackOffset = position;
            this.requestedTrackOffsetTime = System.nanoTime ();
        }
        this.trackBank.scrollPosition ().set (position);
    }


    /**
     * Get the track at the absolute position if it is in the window.
     *
     * @param position The position
     * @return The track or null
     */
    Track getWindowTrack (final int position)
    {
        // The items of the bank are the slots of the window: after the requested scroll they
        // hold the tracks of the new page, the commands go to Bitwig in this order
        // Without a report within a second Bitwig did not change the position
        if (this.requestedTrackOffset >= 0 && System.nanoTime () - this.requestedTrackOffsetTime > 1_000_000_000L)
            this.requestedTrackOffset = -1;
        final int offset = this.requestedTrackOffset >= 0 ? this.requestedTrackOffset : this.trackBank.scrollPosition ().get ();
        final int index = position - offset;
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
        final long base = channelKey (Protocol.ChKind.MASTER, 0);
        reporter.put (Msg.CHANNEL_SELECTED | base, this.masterSelected);
        reportVu (reporter, base, this.masterVuLeft, this.masterVuRight);
        collectChannel (reporter, base, this.masterTrack);
    }


    /**
     * Report the VU meters (0..1): left, right and the mono value as their average (as ChannelImpl
     * of the Java version).
     */
    private static void reportVu (final ModelReporter reporter, final long base, final int left, final int right)
    {
        final double vuLeft = Math.min (left, VU_RANGE - 1) / (double) VU_RANGE;
        final double vuRight = Math.min (right, VU_RANGE - 1) / (double) VU_RANGE;
        reporter.put (Msg.CHANNEL_VU_LEFT | base, vuLeft);
        reporter.put (Msg.CHANNEL_VU_RIGHT | base, vuRight);
        reporter.put (Msg.CHANNEL_VU | base, (vuLeft + vuRight) / 2.0);
    }


    /** The lengths for which the texts shortened by Bitwig are reported (getLimited). */
    static final int [] LIMITS = { 6, 8 };


    /**
     * The key base of a channel (the slots ch_kind, ch_pos), OR'ed with a channel, send or slot ID;
     * a send or slot index goes into the third slot.
     *
     * @param kind The kind (Protocol.ChKind)
     * @param position The position
     * @return The key base
     */
    static long channelKey (final int kind, final int position)
    {
        return Protocol.key (0, kind, position, 0);
    }


    /** The key base of a parameter of the host (crossfader, cue, groove). */
    private static long hostParamKey (final int kind, final int param)
    {
        return Protocol.key (0, kind, param, 0);
    }


    /** The key base of a flag of the arranger or the mixer. */
    private static long flagKey (final int flag)
    {
        return Protocol.key (0, flag, 0, 0);
    }


    /**
     * Report a displayed value and its texts shortened by Bitwig (the slot "limit" of the ID), as
     * getDisplayedValue (limit) of the Java version, which shortens in its own way (e.g. drops
     * the unit).
     */
    static void putDisplayedValue (final ModelReporter reporter, final long key, final StringValue value, final boolean exists)
    {
        if (exists)
            reporter.putDisplayed (key, value.get (), value.getLimited (LIMITS[0]), value.getLimited (LIMITS[1]));
        else
            reporter.putDisplayed (key, "", "", "");
    }


    /**
     * The values of a track or the master, with the texts of volume and pan shortened by Bitwig.
     */
    static void collectChannel (final ModelReporter reporter, final long base, final Channel channel)
    {
        reporter.put (Msg.CHANNEL_NAME | base, channel.name ().get ());
        reporter.putColor (Msg.CHANNEL_COLOR | base, channel.color ().red (), channel.color ().green (), channel.color ().blue ());
        reporter.put (Msg.CHANNEL_MUTE | base, channel.mute ().get ());
        reporter.put (Msg.CHANNEL_SOLO | base, channel.solo ().get ());
        // ChannelImpl.isActivated
        reporter.put (Msg.CHANNEL_ACTIVE | base, channel.isActivated ().get ());
        reporter.put (Msg.CHANNEL_VOLUME | base, channel.volume ().value ().get ());
        putDisplayedValue (reporter, Msg.CHANNEL_VOLUME_STR | base, channel.volume ().displayedValue (), true);
        reporter.put (Msg.CHANNEL_PAN | base, channel.pan ().value ().get ());
        putDisplayedValue (reporter, Msg.CHANNEL_PAN_STR | base, channel.pan ().displayedValue (), true);
    }


    private void collectDevice (final ModelReporter reporter)
    {
        final int position = this.cursorDevice.position ().get ();
        reporter.put (Msg.DEVICE_COUNT, this.siblings.itemCount ().get ());
        reporter.put (Msg.DEVICE_EXISTS, this.cursorDevice.exists ().get ());
        reporter.put (Msg.DEVICE_POSITION, this.cursorDevice.exists ().get () ? position : -1);
        reporter.put (Msg.DEVICE_NAME, this.cursorDevice.name ().get ());
        reporter.put (Msg.DEVICE_BYPASS, !this.cursorDevice.isEnabled ().get ());
        reporter.put (Msg.DEVICE_WINDOW, this.cursorDevice.isWindowOpen ().get ());
        reporter.put (Msg.DEVICE_PINNED, this.cursorDevice.isPinned ().get ());
        reporter.put (Msg.DEVICE_EXPAND, this.cursorDevice.isExpanded ().get ());

        for (int i = 0; i < NUM_SIBLINGS; i++)
        {
            final Device device = this.siblings.getItemAt (i);
            // "/device/sibling/<i + 1>/…": the slot holds the index
            final long base = Protocol.key (0, Protocol.Dev.DEVICE, i, 0);
            final boolean exists = device.exists ().get ();
            reporter.put (Msg.DEVICE_SIBLING_NAME | base, exists ? device.name ().get () : "");
            reporter.put (Msg.DEVICE_SIBLING_BYPASS | base, exists && !device.isEnabled ().get ());
            reporter.put (Msg.DEVICE_SIBLING_POSITION | base, exists ? device.position ().get () : -1);
            reporter.put (Msg.DEVICE_SIBLING_SELECTED | base, exists && device.position ().get () == position);
        }

        collectPages (reporter, Protocol.ParamOwner.DEVICE, this.pages);

        // The page names and the page selected in Bitwig (protocol v2)
        collectPageNames (reporter, Protocol.PageOwner.DEVICE, this.remoteControls, this.pages.length);
    }


    /**
     * Report the parameters of the page cursors as one flat list "&lt;owner&gt;/param/&lt;index&gt;/...";
     * each page cursor is kept on its page.
     *
     * @param reporter Where to report to
     * @param owner The owner of the parameters (Protocol.ParamOwner)
     * @param pages The page cursors
     */
    static void collectPages (final ModelReporter reporter, final int owner, final CursorRemoteControlsPage [] pages)
    {
        // Each remote control page object shows one page of the device: together a flat list
        final int pageCount = pages[0].pageCount ().get ();
        final int numPages = Math.min (pageCount, pages.length);
        for (int p = 0; p < pages.length; p++)
        {
            final CursorRemoteControlsPage page = pages[p];
            final boolean isUsed = p < numPages;
            if (isUsed && page.selectedPageIndex ().get () != p)
                page.selectedPageIndex ().set (p);
            for (int i = 0; i < PARAMS_PER_PAGE; i++)
            {
                final Parameter parameter = page.getParameter (i);
                final long base = Protocol.key (0, owner, p * PARAMS_PER_PAGE + i, 0);
                final boolean exists = isUsed && parameter.exists ().get ();
                collectParameter (reporter, base, parameter, exists);
            }
        }
        reporter.put (Msg.PARAM_COUNT | Protocol.key (0, owner, 0, 0), numPages * PARAMS_PER_PAGE);
    }


    /**
     * Report a parameter: name, value, the displayed value, steps (none) and the modulated value.
     *
     * @param reporter Where to report to
     * @param base The key base (owner and index)
     * @param parameter The parameter
     * @param exists False to report an empty parameter
     */
    static void collectParameter (final ModelReporter reporter, final long base, final Parameter parameter, final boolean exists)
    {
        reporter.put (Msg.PARAM_NAME | base, exists ? parameter.name ().get () : "");
        reporter.put (Msg.PARAM_VALUE | base, exists ? parameter.value ().get () : 0);
        putDisplayedValue (reporter, Msg.PARAM_VALUE_STR | base, parameter.displayedValue (), exists);
        reporter.put (Msg.PARAM_STEPS | base, -1);
        reporter.put (Msg.PARAM_MODULATED | base, exists ? parameter.modulatedValue ().get () : 0);
    }


    /**
     * The names of the pages (as many as there are fixed page cursors) and the page selected in
     * Bitwig, from the cursor which follows Bitwig's selection.
     *
     * @param reporter Where to report to
     * @param owner The owner of the pages (Protocol.PageOwner)
     * @param remoteControls The cursor which follows Bitwig's page selection
     * @param numPages The number of page cursors
     */
    static void collectPageNames (final ModelReporter reporter, final int owner, final CursorRemoteControlsPage remoteControls, final int numPages)
    {
        final String [] pageNames = remoteControls.pageNames ().get ();
        final int namedPages = Math.min (pageNames.length, numPages);
        for (int p = 0; p < namedPages; p++)
            reporter.put (Msg.PAGE_NAME | Protocol.key (0, owner, p, 0), pageNames[p]);
        reporter.put (Msg.PAGE_COUNT | Protocol.key (0, owner, 0, 0), namedPages);
        reporter.put (Msg.PAGE_SELECTED | Protocol.key (0, owner, 0, 0), remoteControls.selectedPageIndex ().get ());
    }


    private static String trackType (final Track track)
    {
        if (track.isGroup ().get ())
            return "GROUP";
        final String type = track.trackType ().get ().toUpperCase (Locale.ROOT);
        return type.isEmpty () ? "HYBRID" : type;
    }




    private static boolean getBoolean (final String json, final String key, final boolean defaultValue)
    {
        final Matcher matcher = Pattern.compile ("\"" + Pattern.quote (key) + "\"\\s*:\\s*(true|false)").matcher (json);
        return matcher.find () ? Boolean.parseBoolean (matcher.group (1)) : defaultValue;
    }


    /** The content of a JSON array value (between the outer brackets), empty if not found. */
    private static String getArray (final String json, final String key)
    {
        final int start = json.indexOf ("\"" + key + "\":[");
        if (start < 0)
            return "";
        int depth = 0;
        final int begin = start + key.length () + 3;
        for (int i = begin; i < json.length (); i++)
        {
            final char c = json.charAt (i);
            if (c == '[')
                depth++;
            else if (c == ']' && --depth == 0)
                return json.substring (begin + 1, i);
        }
        return "";
    }


    private static int getInt (final String json, final String key, final int defaultValue)
    {
        final Matcher matcher = Pattern.compile ("\"" + Pattern.quote (key) + "\"\\s*:\\s*(-?\\d+)").matcher (json);
        return matcher.find () ? Integer.parseInt (matcher.group (1)) : defaultValue;
    }
}
