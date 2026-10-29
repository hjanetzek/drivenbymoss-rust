// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Action;
import com.bitwig.extension.controller.api.Channel;
import com.bitwig.extension.controller.api.ClipLauncherSlot;
import com.bitwig.extension.controller.api.CursorRemoteControlsPage;
import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.Parameter;
import com.bitwig.extension.controller.api.PinnableCursorClip;
import com.bitwig.extension.controller.api.PinnableCursorDevice;
import com.bitwig.extension.controller.api.Scene;
import com.bitwig.extension.controller.api.Track;

import java.util.Arrays;
import java.util.function.Consumer;
import java.util.regex.Matcher;
import java.util.regex.Pattern;


/**
 * Executes the commands of the core (processor, command, value; see doc/BACKEND_PROTOCOL.md §2)
 * with the Bitwig API: transport, tracks, master, cursor device, note repeat, and the commands
 * of protocol v2 (application, arranger, mixer, project, marker).
 */
public final class CommandHandler
{
    /** The panel layouts in the order of the core's layout index. */
    private static final String [] PANEL_LAYOUTS = { "ARRANGE", "MIX", "EDIT", "PLAY" };

    private static final Pattern COLOR_PATTERN = Pattern.compile ("RGB\\((\\d+),(\\d+),(\\d+)\\)");


    private final BitwigModel    model;
    private final ModelReporter  reporter;
    private final Consumer<String> unhandled;


    /**
     * Constructor.
     *
     * @param model The Bitwig objects
     * @param reporter The reporter, for the refresh command
     * @param unhandled Called with a description of each command which is not supported
     */
    public CommandHandler (final BitwigModel model, final ModelReporter reporter, final Consumer<String> unhandled)
    {
        this.model = model;
        this.reporter = reporter;
        this.unhandled = unhandled;
    }


    /**
     * Execute a command without a value.
     *
     * @param processor The processor
     * @param command The command
     */
    public void handle (final String processor, final String command)
    {
        switch (processor)
        {
            case "transport" -> this.handleTransport (command, 1);
            case "application" -> this.handleApplication (command, 1);
            case "arranger" -> this.handleArranger (command);
            case "mixer" -> this.handleMixer (command);
            case "project" -> this.handleProject (command);
            case "marker" -> this.handleMarker (command);
            case "scene" -> this.handleScene (command, null);
            case "drum" -> this.handleSpecific (this.model.drumDevice, processor, command, 1, null);
            case "primary" -> this.handleSpecific (this.model.firstInstrument, processor, command, 1, null);
            case "browser" -> this.handleBrowser (command, 1);
            case "clip" -> this.handleClip (command, 1, null);
            case "refresh" -> this.reporter.requestDump ();
            case "play" -> this.model.transport.togglePlay ();
            case "stop" -> this.model.transport.stop ();
            case "record" -> this.model.transport.record ();
            case "undo" -> this.model.application.undo ();
            case "redo" -> this.model.application.redo ();
            case "tempo" -> this.changeTempo (command);
            case "track" -> this.handleTrack (command, 1, null);
            case "device" -> this.handleDevice (command, 1);
            default -> this.unhandled.accept (processor + " " + command);
        }
    }


    /**
     * Execute a command with a number value (int and double values of the core).
     *
     * @param processor The processor
     * @param command The command
     * @param value The value
     */
    public void handle (final String processor, final String command, final double value)
    {
        switch (processor)
        {
            case "transport" -> this.handleTransport (command, value);
            case "application" -> this.handleApplication (command, value);
            case "bank" -> this.handleBank (command, (int) value);
            case "repeat" -> this.model.transport.isArrangerLoopEnabled ().toggle ();
            case "tempo" -> this.model.transport.tempo ().value ().setRaw (value);
            case "time" -> this.handleTime (command, value);
            case "beats" -> this.handleBeats (command, value);
            case "track" -> this.handleTrack (command, value, null);
            case "master" -> this.handleChannel (this.model.masterTrack, command, value, null);
            case "device" -> this.handleDevice (command, value);
            case "noterepeat" -> this.handleNoteRepeat (command, value);
            case "project" -> this.handleProject (command);
            case "drum" -> this.handleSpecific (this.model.drumDevice, processor, command, value, null);
            case "primary" -> this.handleSpecific (this.model.firstInstrument, processor, command, value, null);
            case "browser" -> this.handleBrowser (command, value);
            case "clip" -> this.handleClip (command, value, null);
            case "quantize" ->
            {
                // As CursorClipImpl.quantize
                if (value >= 0.000001 && value <= 1)
                    this.model.cursorClip.clip.quantize (value);
            }
            default -> this.unhandled.accept (processor + " " + command);
        }
    }


    /**
     * Note repeat commands, mapped to the arpeggiator of the default note input.
     */
    private void handleNoteRepeat (final String command, final double value)
    {
        if (this.model.arpeggiator == null)
            return;
        switch (command)
        {
            case "active" -> this.model.arpeggiator.isEnabled ().set (value > 0);
            // The core sends the inverse of the period in beats
            case "rate" ->
            {
                if (value > 0)
                    this.model.arpeggiator.rate ().set (1.0 / value);
            }
            case "notelength" -> this.model.arpeggiator.gateLength ().set (value);
            case "mode" ->
            {
                final int index = (int) value;
                if (index >= 0 && index < BitwigModel.ARPEGGIATOR_MODES.size ())
                    this.model.arpeggiator.mode ().set (BitwigModel.ARPEGGIATOR_MODES.get (index));
            }
            case "velocity" -> this.model.arpeggiator.usePressureToVelocity ().set (value > 0);
            default -> this.unhandled.accept ("noterepeat " + command);
        }
    }


    /**
     * Execute a command with a text value.
     *
     * @param processor The processor
     * @param command The command
     * @param value The value
     */
    public void handle (final String processor, final String command, final String value)
    {
        switch (processor)
        {
            case "track" -> this.handleTrack (command, 0, value);
            case "master" -> this.handleChannel (this.model.masterTrack, command, 0, value);
            case "scene" -> this.handleScene (command, value);
            case "drum" -> this.handleSpecific (this.model.drumDevice, processor, command, 0, value);
            case "primary" -> this.handleSpecific (this.model.firstInstrument, processor, command, 0, value);
            case "clip" -> this.handleClip (command, 0, value);
            default -> this.unhandled.accept (processor + " " + command);
        }
    }


    /**
     * A device found by a matcher (drum machine, first instrument): "param/&lt;index&gt;/...",
     * "pad/&lt;pad&gt;/&lt;command&gt;", "layer/&lt;layer&gt;/&lt;command&gt;", "clearMute",
     * "clearSolo", "indicate", "window", "expand", as SpecificDeviceImpl, DrumPadBankImpl,
     * LayerBankImpl and ChannelImpl of the Java version.
     */
    private void handleSpecific (final SpecificDevice device, final String processor, final String command, final double value, final String text)
    {
        if (device == null)
            return;
        switch (command)
        {
            case "clearMute" -> device.drumPadBank.clearMutedPads ();
            case "clearSolo" -> device.drumPadBank.clearSoloedPads ();
            case "indicate" -> device.drumPadBank.setIndication (value > 0);
            case "window" -> device.device.isWindowOpen ().set (value > 0);
            case "expand" -> device.device.isExpanded ().set (value > 0);
            case "parameterPage" ->
            {
                // Show the page in Bitwig as well (ParameterPageBankImpl.scrollTo)
                if (device.remoteControls == null)
                    this.unhandled.accept (processor + " " + command);
                else
                {
                    final int count = device.remoteControls.pageCount ().get ();
                    if (count > 0)
                        device.remoteControls.selectedPageIndex ().set (Math.clamp ((int) value, 0, count - 1));
                }
            }
            default ->
            {
                final String [] parts = command.split ("/", 3);
                if (parts.length == 3 && "param".equals (parts[0]))
                {
                    if (!this.handleParameter (device.pages, parts, value))
                        this.unhandled.accept (processor + " " + command);
                    return;
                }
                final int index = parts.length == 3 ? parseIndex (parts[1]) : -1;
                final Channel channel;
                if ("pad".equals (parts[0]) && index >= 0 && index < device.numPads)
                    channel = device.drumPadBank.getItemAt (index);
                else if ("layer".equals (parts[0]) && index >= 0 && index < device.numLayers)
                    channel = device.layerBank.getItemAt (index);
                else
                {
                    this.unhandled.accept (processor + " " + command);
                    return;
                }
                if ("select".equals (parts[2]))
                {
                    if (value > 0)
                        selectChannel (channel);
                }
                else
                    this.handleChannel (channel, parts[2], value, text);
            }
        }
    }


    /**
     * The popup browser, as BrowserImpl of the Java version: "replace/device",
     * "replace/slot/&lt;track&gt;/&lt;slot&gt;", "replace/&lt;drum|primary&gt;/pad/&lt;pad&gt;",
     * "add/track/&lt;track&gt;", "add/master", "insertBefore", "insertAfter", "stop" (1 =
     * commit), "preview", "contentType/+|-", "column/&lt;column&gt;/…", "result/…".
     */
    private void handleBrowser (final String command, final double value)
    {
        final BitwigBrowser browser = this.model.browser;
        if (browser == null)
            return;
        final String [] parts = command.split ("/");
        final boolean handled = switch (parts[0])
        {
            case "stop" ->
            {
                browser.stopBrowsing (value > 0);
                yield true;
            }
            case "preview" ->
            {
                browser.setPreview (value > 0);
                yield true;
            }
            case "contentType" ->
            {
                if (parts.length == 2)
                    browser.changeContentType ("-".equals (parts[1]) ? -1 : 1);
                yield parts.length == 2;
            }
            case "column" -> parts.length >= 3 && browser.handleColumn (parseIndex (parts[1]), command.substring (parts[0].length () + parts[1].length () + 2));
            case "result" -> parts.length >= 2 && browser.handleResult (command.substring (parts[0].length () + 1));
            case "insertBefore" ->
            {
                final PinnableCursorDevice device = this.model.cursorDevice;
                browser.browse (device.exists ().get () ? device.beforeDeviceInsertionPoint () : this.model.cursorTrack.startOfDeviceChainInsertionPoint ());
                yield true;
            }
            case "insertAfter" ->
            {
                final PinnableCursorDevice device = this.model.cursorDevice;
                browser.browse (device.exists ().get () ? device.afterDeviceInsertionPoint () : this.model.cursorTrack.endOfDeviceChainInsertionPoint ());
                yield true;
            }
            case "add" -> this.browseAdd (browser, parts);
            case "replace" -> this.browseReplace (browser, parts);
            default -> false;
        };
        if (!handled)
            this.unhandled.accept ("browser " + command);
    }


    /** "add/track/&lt;track&gt;", "add/master": the start of the channel's device chain. */
    private boolean browseAdd (final BitwigBrowser browser, final String [] parts)
    {
        if (parts.length == 2 && "master".equals (parts[1]))
        {
            browser.browse (this.model.masterTrack.startOfDeviceChainInsertionPoint ());
            return true;
        }
        if (parts.length != 3 || !"track".equals (parts[1]))
            return false;
        final Track track = this.model.getTrack (parseIndex (parts[2]));
        if (track != null)
            browser.browse (track.startOfDeviceChainInsertionPoint ());
        return true;
    }


    /** "replace/device", "replace/slot/&lt;track&gt;/&lt;slot&gt;", "replace/&lt;device&gt;/pad/&lt;pad&gt;". */
    private boolean browseReplace (final BitwigBrowser browser, final String [] parts)
    {
        if (parts.length == 2 && "device".equals (parts[1]))
        {
            browser.browse (this.model.cursorDevice.replaceDeviceInsertionPoint ());
            return true;
        }
        if (parts.length == 4 && "slot".equals (parts[1]))
        {
            // Only slots of the window can be addressed
            final Track track = this.model.getWindowTrack (parseIndex (parts[2]));
            final ClipLauncherSlot slot = track == null ? null : this.model.getSlot (track, parseIndex (parts[3]));
            if (slot != null)
                browser.browse (slot.replaceInsertionPoint ());
            return true;
        }
        if (parts.length == 4 && "pad".equals (parts[2]))
        {
            final SpecificDevice device = switch (parts[1])
            {
                case "drum" -> this.model.drumDevice;
                case "primary" -> this.model.firstInstrument;
                default -> null;
            };
            final int pad = parseIndex (parts[3]);
            if (device == null || pad < 0 || pad >= device.numPads)
                return false;
            browser.browse (device.drumPadBank.getItemAt (pad).insertionPoint ());
            return true;
        }
        return false;
    }


    /**
     * The cursor clip, as CursorClipImpl of the Java version: the clip properties on the first
     * note clip (all show the same clip), "grid/&lt;steps&gt;x&lt;rows&gt;/…" on the step grid of a
     * note clip.
     */
    private void handleClip (final String command, final double value, final String text)
    {
        final PinnableCursorClip clip = this.model.cursorClip.clip;
        switch (command)
        {
            case "name" -> clip.setName (text == null ? "" : text);
            case "color" -> setColor (text, clip.color ()::set);
            case "start" -> clip.getPlayStart ().set (value);
            case "end" -> clip.getPlayStop ().set (value);
            case "loop" -> clip.isLoopEnabled ().set (value > 0);
            case "loopStart" -> clip.getLoopStart ().set (value);
            case "loopLength" -> clip.getLoopLength ().set (value);
            case "shuffle" -> clip.getShuffle ().set (value > 0);
            case "accent" -> clip.getAccent ().set (value);
            case "duplicate" -> clip.duplicate ();
            case "duplicateContent" -> clip.duplicateContent ();
            case "pinned" ->
            {
                for (final BitwigNoteClip noteClip: this.model.noteClips)
                    noteClip.clip.isPinned ().set (value > 0);
            }
            case "transpose" ->
            {
                // Workaround Bitwig crashing when it is not a MIDI clip (CursorClipImpl)
                if (clip.getTrack ().canHoldNoteData ().get ())
                    clip.transpose ((int) value);
            }
            default ->
            {
                final String [] parts = command.split ("/");
                if (parts.length >= 3 && "grid".equals (parts[0]))
                {
                    for (final BitwigNoteClip noteClip: this.model.noteClips)
                    {
                        if (noteClip.id.equals (parts[1]))
                        {
                            if (!noteClip.handle (Arrays.copyOfRange (parts, 2, parts.length), value, text))
                                this.unhandled.accept ("clip " + command);
                            return;
                        }
                    }
                }
                this.unhandled.accept ("clip " + command);
            }
        }
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
     * Scroll the track or scene window to the core's bank offset (host paging).
     */
    private void handleBank (final String command, final int offset)
    {
        switch (command)
        {
            case "track" -> this.model.trackBank.scrollPosition ().set (Math.max (0, offset));
            case "scene" -> this.model.sceneBank.scrollPosition ().set (Math.max (0, offset));
            default -> this.unhandled.accept ("bank " + command);
        }
    }


    private void handleTransport (final String command, final double value)
    {
        switch (command)
        {
            case "metronome" -> this.model.transport.isMetronomeEnabled ().set (value > 0);
            case "toggleMetronome" -> this.model.transport.isMetronomeEnabled ().toggle ();
            case "togglePrerollMetronome" -> this.model.transport.isMetronomeAudibleDuringPreRoll ().toggle ();
            case "punchIn" -> this.model.transport.isPunchInEnabled ().set (value > 0);
            case "punchOut" -> this.model.transport.isPunchOutEnabled ().set (value > 0);
            case "tapTempo" -> this.model.transport.tapTempo ();
            case "end" -> this.invokeAction ("jump_to_end_of_arrangement");
            default -> this.unhandled.accept ("transport " + command);
        }
    }


    private void handleApplication (final String command, final double value)
    {
        switch (command)
        {
            case "layout" ->
            {
                final int index = (int) value;
                if (index >= 0 && index < PANEL_LAYOUTS.length)
                    this.model.application.setPanelLayout (PANEL_LAYOUTS[index]);
            }
            case "toggleNoteEditor" -> this.model.application.toggleNoteEditor ();
            case "toggleAutomationEditor" -> this.model.application.toggleAutomationEditor ();
            case "toggleDevices" -> this.model.application.toggleDevices ();
            case "toggleInspector" -> this.model.application.toggleInspector ();
            case "toggleMixer" -> this.model.application.toggleMixer ();
            case "toggleFullScreen" -> this.model.application.toggleFullScreen ();
            case "toggleBrowser" -> this.model.application.toggleBrowserVisibility ();
            case "duplicate" -> this.model.application.duplicate ();
            case "deleteSelection" -> this.model.application.remove ();
            case "arrowLeft" -> this.model.application.arrowKeyLeft ();
            case "arrowRight" -> this.model.application.arrowKeyRight ();
            case "arrowUp" -> this.model.application.arrowKeyUp ();
            case "arrowDown" -> this.model.application.arrowKeyDown ();
            case "zoomIn" -> this.model.application.zoomIn ();
            case "zoomOut" -> this.model.application.zoomOut ();
            case "incTrackHeight" -> this.model.arranger.zoomInLaneHeightsAll ();
            case "decTrackHeight" -> this.model.arranger.zoomOutLaneHeightsAll ();
            case "sliceToSampler" -> this.invokeAction ("slice_to_multi_sampler_track");
            case "sliceToDrumMachine" -> this.invokeAction ("slice_to_drum_track");
            case "enter" -> this.model.application.enter ();
            case "escape" -> this.model.application.escape ();
            default -> this.unhandled.accept ("application " + command);
        }
    }


    private void handleArranger (final String command)
    {
        switch (command)
        {
            case "toggleCueMarkers" -> this.model.arranger.areCueMarkersVisible ().toggle ();
            case "togglePlaybackFollow" -> this.model.arranger.isPlaybackFollowEnabled ().toggle ();
            case "toggleTrackRowHeight" -> this.model.arranger.hasDoubleRowTrackHeight ().toggle ();
            case "toggleClipLauncher" -> this.model.arranger.isClipLauncherVisible ().toggle ();
            case "toggleTimeLine" -> this.model.arranger.isTimelineVisible ().toggle ();
            case "toggleIoSection" -> this.model.arranger.isIoSectionVisible ().toggle ();
            case "toggleEffectTracks" -> this.model.arranger.areEffectTracksVisible ().toggle ();
            default -> this.unhandled.accept ("arranger " + command);
        }
    }


    private void handleMixer (final String command)
    {
        switch (command)
        {
            case "toggleClipLauncherSection" -> this.model.mixer.isClipLauncherSectionVisible ().toggle ();
            case "toggleCrossFadeSection" -> this.model.mixer.isCrossFadeSectionVisible ().toggle ();
            case "toggleDeviceSection" -> this.model.mixer.isDeviceSectionVisible ().toggle ();
            case "toggleIoSection" -> this.model.mixer.isIoSectionVisible ().toggle ();
            case "toggleMeterSection" -> this.model.mixer.isMeterSectionVisible ().toggle ();
            case "toggleSendsSection" -> this.model.mixer.isSendSectionVisible ().toggle ();
            default -> this.unhandled.accept ("mixer " + command);
        }
    }


    private void handleProject (final String command)
    {
        switch (command)
        {
            case "createScene" -> this.model.project.createScene ();
            case "createSceneFromPlayingLauncherClips" -> this.model.project.createSceneFromPlayingLauncherClips ();
            case "previous" -> this.model.application.previousProject ();
            case "next" -> this.model.application.nextProject ();
            case "save" -> this.invokeAction ("Save");
            case "load" -> this.invokeAction ("Open");
            default -> this.unhandled.accept ("project " + command);
        }
    }


    private void handleMarker (final String command)
    {
        switch (command)
        {
            case "previous" -> this.model.transport.jumpToPreviousCueMarker ();
            case "next" -> this.model.transport.jumpToNextCueMarker ();
            default -> this.unhandled.accept ("marker " + command);
        }
    }


    private void changeTempo (final String command)
    {
        final double delta = switch (command)
        {
            case "+" -> 0.1;
            case "++" -> 1;
            case "-" -> -0.1;
            case "--" -> -1;
            default -> 0;
        };
        final double tempo = this.model.transport.tempo ().value ().getRaw ();
        this.model.transport.tempo ().value ().setRaw (tempo + delta);
    }


    private void handleTime (final String command, final double seconds)
    {
        final double tempo = this.model.transport.tempo ().value ().getRaw ();
        final double beats = seconds * tempo / 60.0;
        switch (command)
        {
            case "", "nosnap" -> this.setPosition (beats);
            default -> this.unhandled.accept ("time " + command);
        }
    }


    /**
     * The play position and the loop in quarter notes, as TransportImpl of the Java version:
     * there is no snapping, the loop start keeps the duration.
     */
    private void handleBeats (final String command, final double beats)
    {
        switch (command)
        {
            case "position", "position/nosnap" -> this.setPosition (beats);
            case "loop/start" -> this.model.transport.arrangerLoopStart ().set (beats);
            case "loop/length" -> this.model.transport.arrangerLoopDuration ().set (beats);
            default -> this.unhandled.accept ("beats " + command);
        }
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
     * Track commands: "&lt;position&gt;/&lt;command&gt;" with the absolute track position. Tracks
     * outside of the window come from the large bank, without sends and slots.
     */
    private void handleTrack (final String command, final double value, final String text)
    {
        final int slash = command.indexOf ('/');
        final int index = slash > 0 ? parseIndex (command.substring (0, slash)) : -1;
        final Track track = this.model.getTrack (index);
        if (track == null)
        {
            this.unhandled.accept ("track " + command);
            return;
        }
        final String subCommand = command.substring (slash + 1);
        switch (subCommand)
        {
            case "select", "scrollto" ->
            {
                if (value > 0)
                    selectChannel (track);
            }
            case "recarm" -> track.arm ().set (value > 0);
            case "monitor" -> track.monitorMode ().set (value > 0 ? "ON" : "OFF");
            case "autoMonitor" -> track.monitorMode ().set (value > 0 ? "AUTO" : "OFF");
            case "isGroupExpanded" -> track.isGroupExpanded ().set (value > 0);
            case "remove" -> track.deleteObject ();
            case "duplicate" -> track.duplicateObject ();
            case "stop" -> track.stop ();
            case "stopAlt" -> track.stopAlt ();
            case "returnToArrangement" -> track.returnToArrangement ();
            case "name" ->
            {
                if (text != null)
                    track.name ().set (text);
            }
            default ->
            {
                if (subCommand.startsWith ("send/") || subCommand.startsWith ("clip/"))
                {
                    // Sends and slots only exist for the tracks of the window
                    final Track windowTrack = this.model.getWindowTrack (index);
                    if (windowTrack == null)
                        this.unhandled.accept ("track outside of the window " + command);
                    else if (subCommand.startsWith ("send/"))
                        this.handleSend (windowTrack, subCommand, value);
                    else
                        this.handleSlot (windowTrack, subCommand, text);
                }
                else if (subCommand.startsWith ("createClip/"))
                {
                    // createNewLauncherClip needs the absolute slot (as TrackImpl.createClip)
                    final int slot = parseIndex (subCommand.substring ("createClip/".length ()));
                    if (slot >= 0)
                        track.createNewLauncherClip (slot, (int) value);
                }
                else
                    this.handleChannel (track, subCommand, value, text);
            }
        }
    }


    /**
     * Slot commands: "clip/&lt;slot&gt;/&lt;command&gt;" with the absolute slot (scene) index, as
     * SlotImpl of the Java version.
     */
    private void handleSlot (final Track track, final String command, final String text)
    {
        final String [] parts = command.split ("/");
        final ClipLauncherSlot slot = parts.length == 3 ? this.model.getSlot (track, parseIndex (parts[1])) : null;
        if (slot == null)
        {
            this.unhandled.accept ("track " + command);
            return;
        }
        switch (parts[2])
        {
            case "launch" -> slot.launch ();
            case "launchAlt" -> slot.launchAlt ();
            case "launchRelease" -> slot.launchRelease ();
            case "launchReleaseAlt" -> slot.launchReleaseAlt ();
            case "record" -> slot.record ();
            case "select" -> slot.select ();
            case "remove" -> slot.deleteObject ();
            case "duplicate" -> slot.duplicateClip ();
            case "insertFile" ->
            {
                if (text != null)
                    slot.replaceInsertionPoint ().insertFile (text);
            }
            case "color" -> setColor (text, (red, green, blue) -> slot.color ().set (red, green, blue));
            case "paste" ->
            {
                // The source as "<track>/<slot>"
                final String [] source = text == null ? new String [0] : text.split ("/");
                final int sourceTrack = source.length == 2 ? parseIndex (source[0]) : -1;
                final int sourceSlot = source.length == 2 ? parseIndex (source[1]) : -1;
                final Track sourceWindowTrack = this.model.getWindowTrack (sourceTrack);
                final ClipLauncherSlot sourceSlotObject = sourceWindowTrack == null ? null : this.model.getSlot (sourceWindowTrack, sourceSlot);
                if (sourceSlotObject != null)
                    slot.replaceInsertionPoint ().copySlotsOrScenes (sourceSlotObject);
            }
            default -> this.unhandled.accept ("track clip " + parts[2]);
        }
    }


    /**
     * Scene commands: "&lt;index&gt;/&lt;command&gt;", or "stop"/"stopAlt" for all clips, as
     * SceneImpl and SceneBankImpl of the Java version.
     */
    private void handleScene (final String command, final String text)
    {
        switch (command)
        {
            case "stop" -> this.model.sceneBank.stop ();
            case "stopAlt" -> this.model.sceneBank.stopAlt ();
            default ->
            {
                final String [] parts = command.split ("/");
                final Scene scene = parts.length == 2 ? this.model.getScene (parseIndex (parts[0])) : null;
                if (scene == null)
                {
                    this.unhandled.accept ("scene " + command);
                    return;
                }
                switch (parts[1])
                {
                    case "launch" -> scene.launch ();
                    case "launchAlt" -> scene.launchAlt ();
                    case "launchRelease" -> scene.launchRelease ();
                    case "launchReleaseAlt" -> scene.launchReleaseAlt ();
                    case "select" -> scene.selectInEditor ();
                    case "remove" -> scene.deleteObject ();
                    case "duplicate" -> scene.nextSceneInsertionPoint ().copySlotsOrScenes (scene);
                    case "name" ->
                    {
                        if (text != null)
                            scene.name ().set (text);
                    }
                    case "color" -> setColor (text, (red, green, blue) -> scene.color ().set (red, green, blue));
                    default -> this.unhandled.accept ("scene " + parts[1]);
                }
            }
        }
    }


    @FunctionalInterface
    private interface ColorSetter
    {
        void set (float red, float green, float blue);
    }


    /**
     * Parse "RGB(r,g,b)" and set the color.
     */
    private static void setColor (final String text, final ColorSetter setter)
    {
        final Matcher matcher = COLOR_PATTERN.matcher (text == null ? "" : text);
        if (matcher.matches ())
            setter.set (Integer.parseInt (matcher.group (1)) / 255f, Integer.parseInt (matcher.group (2)) / 255f, Integer.parseInt (matcher.group (3)) / 255f);
    }


    /**
     * Send commands: "send/&lt;index&gt;/volume" or "send/&lt;index&gt;/active".
     */
    private void handleSend (final Track track, final String command, final double value)
    {
        final String [] parts = command.split ("/");
        final int index = parts.length == 3 ? parseIndex (parts[1]) : -1;
        if (index < 0 || index >= this.model.numSends)
        {
            this.unhandled.accept ("track send " + command);
            return;
        }
        final Parameter send = track.sendBank ().getItemAt (index);
        switch (parts[2])
        {
            case "volume" -> send.value ().set (value);
            case "active" -> track.sendBank ().getItemAt (index).isEnabled ().toggle ();
            default -> this.unhandled.accept ("track send " + parts[2]);
        }
    }


    /**
     * The commands of tracks and the master track.
     */
    private void handleChannel (final Channel channel, final String command, final double value, final String text)
    {
        switch (command)
        {
            case "select" ->
            {
                if (value > 0)
                    channel.selectInEditor ();
            }
            case "mute" -> channel.mute ().set (value > 0);
            case "active" -> channel.isActivated ().set (value > 0);
            case "solo" -> channel.solo ().set (value > 0);
            case "volume" -> channel.volume ().value ().set (value);
            case "pan" -> channel.pan ().value ().set (value);
            case "volume/touch" -> channel.volume ().touch (value > 0);
            case "pan/touch" -> channel.pan ().touch (value > 0);
            case "color" ->
            {
                final Matcher matcher = COLOR_PATTERN.matcher (text == null ? "" : text);
                if (matcher.matches ())
                    channel.color ().set (Integer.parseInt (matcher.group (1)) / 255f, Integer.parseInt (matcher.group (2)) / 255f, Integer.parseInt (matcher.group (3)) / 255f);
            }
            default -> this.unhandled.accept ("channel " + command);
        }
    }


    /**
     * The commands of the cursor device. Device indices are 1-based, as in the Reaper backend.
     */
    private void handleDevice (final String command, final double value)
    {
        switch (command)
        {
            case "+" -> this.model.cursorDevice.selectNext ();
            case "-" -> this.model.cursorDevice.selectPrevious ();
            case "page/+" -> this.model.siblings.scrollPageForwards ();
            case "page/-" -> this.model.siblings.scrollPageBackwards ();
            case "selected" ->
            {
                final int index = (int) value - 1;
                if (index >= 0 && index < BitwigModel.NUM_SIBLINGS)
                    this.model.siblings.getItemAt (index).selectInEditor ();
            }
            case "parameterPage" ->
            {
                // Show the page in Bitwig as well (ParameterPageBankImpl.scrollTo)
                final int count = this.model.remoteControls.pageCount ().get ();
                if (count > 0)
                    this.model.remoteControls.selectedPageIndex ().set (Math.clamp ((int) value, 0, count - 1));
            }
            case "window" -> this.model.cursorDevice.isWindowOpen ().set (value > 0);
            case "expand" -> this.model.cursorDevice.isExpanded ().set (value > 0);
            default -> this.handleDeviceIndexed (command, value);
        }
    }


    private void handleDeviceIndexed (final String command, final double value)
    {
        final String [] parts = command.split ("/");
        // param/<index>/value, touch, indicate
        if (parts.length == 3 && "param".equals (parts[0]))
        {
            if (!this.handleParameter (this.model.pages, parts, value))
                this.unhandled.accept ("device " + command);
            return;
        }
        // <position+1>/bypass, remove, duplicate
        if (parts.length == 2)
        {
            final Device device = this.findSibling (parseIndex (parts[0]) - 1);
            if (device != null)
            {
                switch (parts[1])
                {
                    case "bypass" -> device.isEnabled ().toggle ();
                    case "remove" -> device.deleteObject ();
                    case "duplicate" -> device.duplicateObject ();
                    default -> this.unhandled.accept ("device " + command);
                }
                return;
            }
        }
        this.unhandled.accept ("device " + command);
    }


    /**
     * "param/&lt;index&gt;/value|touch|indicate" on the flat parameter list of the page cursors.
     *
     * @return False if the command is unknown
     */
    private boolean handleParameter (final CursorRemoteControlsPage [] pages, final String [] parts, final double value)
    {
        final int index = parseIndex (parts[1]);
        final int page = index / BitwigModel.PARAMS_PER_PAGE;
        if (index < 0 || page >= pages.length)
            return true;
        final Parameter parameter = pages[page].getParameter (index % BitwigModel.PARAMS_PER_PAGE);
        switch (parts[2])
        {
            case "value" -> parameter.value ().set (value);
            case "touch" -> parameter.touch (value > 0);
            case "indicate" -> parameter.setIndication (value > 0);
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


    private static int parseIndex (final String text)
    {
        try
        {
            return Integer.parseInt (text);
        }
        catch (final NumberFormatException ex)
        {
            return -1;
        }
    }
}
