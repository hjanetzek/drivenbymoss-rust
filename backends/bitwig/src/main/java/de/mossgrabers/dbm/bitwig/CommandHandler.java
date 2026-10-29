// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Channel;
import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.Parameter;
import com.bitwig.extension.controller.api.Track;

import java.util.function.Consumer;
import java.util.regex.Matcher;
import java.util.regex.Pattern;


/**
 * Executes the commands of the core (processor, command, value; see doc/BACKEND_PROTOCOL.md §2)
 * with the Bitwig API. Stage 1: transport, tracks, master, cursor device and the Reaper action
 * IDs the core uses for features without a command.
 */
public final class CommandHandler
{
    private static final Pattern COLOR_PATTERN = Pattern.compile ("RGB\\((\\d+),(\\d+),(\\d+)\\)");

    // Reaper action IDs (src/daw/actions.rs) which have a Bitwig equivalent
    private static final int     TRANSPORT_TAP_TEMPO   = 1134;
    private static final int     TOGGLE_METRONOME      = 40364;
    private static final int     ENABLE_METRONOME      = 41745;
    private static final int     DISABLE_METRONOME     = 41746;
    private static final int     DUPLICATE_TRACKS      = 40062;
    private static final int     TOGGLE_MIXER_VISIBLE  = 40078;
    private static final int     TOGGLE_FULLSCREEN     = 40346;
    private static final int     GO_TO_NEXT_TRACK      = 40285;
    private static final int     GO_TO_PREV_TRACK      = 40286;

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
            case "repeat" -> this.model.transport.isArrangerLoopEnabled ().toggle ();
            case "tempo" -> this.model.transport.tempo ().value ().setRaw (value);
            case "time" -> this.handleTime (command, value);
            case "action" -> this.handleAction ((int) value);
            case "track" -> this.handleTrack (command, value, null);
            case "master" -> this.handleChannel (this.model.masterTrack, command, value, null);
            case "device" -> this.handleDevice (command, value);
            default -> this.unhandled.accept (processor + " " + command);
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
            default -> this.unhandled.accept (processor + " " + command);
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
            case "", "nosnap" -> this.model.transport.getPosition ().set (beats);
            default -> this.unhandled.accept ("time " + command);
        }
    }


    private void handleAction (final int id)
    {
        switch (id)
        {
            case TRANSPORT_TAP_TEMPO -> this.model.transport.tapTempo ();
            case TOGGLE_METRONOME -> this.model.transport.isMetronomeEnabled ().toggle ();
            case ENABLE_METRONOME -> this.model.transport.isMetronomeEnabled ().set (true);
            case DISABLE_METRONOME -> this.model.transport.isMetronomeEnabled ().set (false);
            case DUPLICATE_TRACKS -> this.model.cursorTrack.duplicateObject ();
            case TOGGLE_MIXER_VISIBLE -> this.model.application.toggleMixer ();
            case TOGGLE_FULLSCREEN -> this.model.application.toggleFullScreen ();
            case GO_TO_NEXT_TRACK -> this.model.cursorTrack.selectNext ();
            case GO_TO_PREV_TRACK -> this.model.cursorTrack.selectPrevious ();
            default -> this.unhandled.accept ("action " + id);
        }
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
            case "name" ->
            {
                if (text != null)
                    track.name ().set (text);
            }
            default ->
            {
                if (subCommand.startsWith ("send/"))
                    this.handleSend (track, subCommand, value);
                else
                    this.handleChannel (track, subCommand, value, text);
            }
        }
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
            case "window" -> this.model.cursorDevice.isWindowOpen ().set (value > 0);
            case "expand" -> this.model.cursorDevice.isExpanded ().set (value > 0);
            default -> this.handleDeviceIndexed (command, value);
        }
    }


    private void handleDeviceIndexed (final String command, final double value)
    {
        final String [] parts = command.split ("/");
        // param/<index>/value
        if (parts.length == 3 && "param".equals (parts[0]) && "value".equals (parts[2]))
        {
            final int index = parseIndex (parts[1]);
            final int page = index / BitwigModel.PARAMS_PER_PAGE;
            if (index >= 0 && page < this.model.pages.length)
                this.model.pages[page].getParameter (index % BitwigModel.PARAMS_PER_PAGE).value ().set (value);
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
