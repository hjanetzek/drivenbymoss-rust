// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Action;
import com.bitwig.extension.controller.api.Channel;
import com.bitwig.extension.controller.api.ClipLauncherSlot;
import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.Parameter;
import com.bitwig.extension.controller.api.Scene;
import com.bitwig.extension.controller.api.Track;

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
            case "repeat" -> this.model.transport.isArrangerLoopEnabled ().toggle ();
            case "tempo" -> this.model.transport.tempo ().value ().setRaw (value);
            case "time" -> this.handleTime (command, value);
            case "beats" -> this.handleBeats (command, value);
            case "track" -> this.handleTrack (command, value, null);
            case "master" -> this.handleChannel (this.model.masterTrack, command, value, null);
            case "device" -> this.handleDevice (command, value);
            case "noterepeat" -> this.handleNoteRepeat (command, value);
            case "project" -> this.handleProject (command);
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
            default -> this.unhandled.accept (processor + " " + command);
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
     * Track commands: "&lt;index&gt;/&lt;command&gt;", the index of the flat track list.
     */
    private void handleTrack (final String command, final double value, final String text)
    {
        final int slash = command.indexOf ('/');
        final int index = slash > 0 ? parseIndex (command.substring (0, slash)) : -1;
        if (index < 0 || index >= BitwigModel.NUM_TRACKS)
        {
            this.unhandled.accept ("track " + command);
            return;
        }
        final Track track = this.model.trackBank.getItemAt (index);
        final String subCommand = command.substring (slash + 1);
        switch (subCommand)
        {
            case "select", "scrollto" ->
            {
                if (value > 0)
                {
                    track.selectInEditor ();
                    track.makeVisibleInArranger ();
                    track.makeVisibleInMixer ();
                }
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
                if (subCommand.startsWith ("send/"))
                    this.handleSend (track, subCommand, value);
                else if (subCommand.startsWith ("clip/"))
                    this.handleSlot (track, subCommand, text);
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
        final int index = parts.length == 3 ? parseIndex (parts[1]) : -1;
        if (index < 0 || index >= BitwigModel.NUM_SCENES)
        {
            this.unhandled.accept ("track " + command);
            return;
        }
        final ClipLauncherSlot slot = track.clipLauncherSlotBank ().getItemAt (index);
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
                if (sourceTrack >= 0 && sourceTrack < BitwigModel.NUM_TRACKS && sourceSlot >= 0 && sourceSlot < BitwigModel.NUM_SCENES)
                    slot.replaceInsertionPoint ().copySlotsOrScenes (this.model.trackBank.getItemAt (sourceTrack).clipLauncherSlotBank ().getItemAt (sourceSlot));
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
                final int index = parts.length == 2 ? parseIndex (parts[0]) : -1;
                if (index < 0 || index >= BitwigModel.NUM_SCENES)
                {
                    this.unhandled.accept ("scene " + command);
                    return;
                }
                final Scene scene = this.model.sceneBank.getItemAt (index);
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
            final int index = parseIndex (parts[1]);
            final int page = index / BitwigModel.PARAMS_PER_PAGE;
            if (index < 0 || page >= this.model.pages.length)
                return;
            final Parameter parameter = this.model.pages[page].getParameter (index % BitwigModel.PARAMS_PER_PAGE);
            switch (parts[2])
            {
                case "value" -> parameter.value ().set (value);
                case "touch" -> parameter.touch (value > 0);
                case "indicate" -> parameter.setIndication (value > 0);
                default -> this.unhandled.accept ("device " + command);
            }
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
