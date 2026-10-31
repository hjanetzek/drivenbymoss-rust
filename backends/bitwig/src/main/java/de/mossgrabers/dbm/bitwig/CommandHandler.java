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
import com.bitwig.extension.controller.api.SettableBeatTimeValue;
import com.bitwig.extension.controller.api.SettableRangedValue;
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

    /** The delay before the values of a commanded item are reported again, in milliseconds. */
    private static final int     REPORT_AGAIN_DELAY = 300;
    private static final String  IMMEDIATELY = "/immediately";
    private static final String  INCREMENT = "/inc";
    private static final String  RESET = "/reset";
    private static final Pattern COLOR_PATTERN = Pattern.compile ("RGB\\((\\d+),(\\d+),(\\d+)\\)");


    private final BitwigModel    model;
    private final ModelReporter  reporter;
    private final Consumer<String> unhandled;
    /** The current value command ends with "/immediately": set without the takeover mode. */
    private boolean              immediately;
    /** The current value command ends with "/inc": change the value by the (normalized) delta. */
    private boolean              increment;
    /** The current value command ends with "/reset": reset the parameter (ParameterImpl.resetValue). */
    private boolean              reset;


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
        // "<value command>/reset" (see the value commands)
        if (command.endsWith (RESET))
        {
            this.handle (processor, command, 0);
            return;
        }
        switch (processor)
        {
            case "transport" -> this.handleTransport (command, 1);
            case "application" -> this.handleApplication (command, 1);
            case "arranger" -> this.handleArranger (command);
            case "mixer" -> this.handleMixer (command);
            case "project" -> this.handleProject (command, 1);
            case "marker" -> this.handleMarker (command);
            case "automation" -> this.handleAutomation (command);
            case "scene" -> this.handleScene (command, null);
            case "drum" -> this.handleSpecific (this.model.drumDevice, processor, command, 1, null);
            case "primary" -> this.handleSpecific (this.model.firstInstrument, processor, command, 1, null);
            case "eq" -> this.handleEqualizer (command, 1, null);
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
            case "master" -> this.handleChannel (this.model.masterTrack, command, 1, null);
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
    public void handle (final String processor, final String fullCommand, final double value)
    {
        // "…/immediately": set the value without Bitwig's takeover mode (catch, relative
        // scaling), as setValueImmediatly of the Java version, e.g. for pads
        this.immediately = fullCommand.endsWith (IMMEDIATELY);
        String command = this.immediately ? fullCommand.substring (0, fullCommand.length () - IMMEDIATELY.length ()) : fullCommand;
        // "…/inc": a relative change, as RangedValueImpl.inc (rangedValue.inc) of the Java version;
        // the core sends the delta normalized (increment / upper bound)
        this.increment = command.endsWith (INCREMENT);
        if (this.increment)
            command = command.substring (0, command.length () - INCREMENT.length ());
        // "…/reset": ParameterImpl.resetValue (parameter.reset ())
        this.reset = command.endsWith (RESET);
        if (this.reset)
            command = command.substring (0, command.length () - RESET.length ());
        this.reportAgain (processor, command);
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
            case "project" -> this.handleProject (command, value);
            case "drum" -> this.handleSpecific (this.model.drumDevice, processor, command, value, null);
            case "primary" -> this.handleSpecific (this.model.firstInstrument, processor, command, value, null);
            case "eq" -> this.handleEqualizer (command, value, null);
            case "browser" -> this.handleBrowser (command, value);
            case "clip" -> this.handleClip (command, value, null);
            case "automation" -> this.setAutomationWriteMode ((int) value);
            case "groove" -> this.handleGroove (command, value);
            case "inifile" -> this.handleIniFile (command, (int) value);
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
     * The groove parameters, as GrooveImpl of the Java version: "&lt;name&gt;" (with "/inc",
     * "/reset"), "&lt;name&gt;/touch", "&lt;name&gt;/indicate".
     */
    private void handleGroove (final String command, final double value)
    {
        final String [] parts = command.split ("/");
        final int index = Arrays.asList (BitwigModel.GROOVE_PARAMETERS).indexOf (parts[0]);
        if (index < 0 || parts.length > 2)
        {
            this.unhandled.accept ("groove " + command);
            return;
        }
        final Parameter parameter = this.model.grooveParameters[index];
        if (parts.length == 1)
            this.setValue (parameter, value);
        else if ("touch".equals (parts[1]))
            parameter.touch (value > 0);
        else if ("indicate".equals (parts[1]))
            parameter.setIndication (value > 0);
        else
            this.unhandled.accept ("groove " + command);
    }


    /**
     * Execute a command with several text values: "track addTrack &lt;type&gt; &lt;name&gt;
     * &lt;color&gt; &lt;devices…&gt;", as AbstractTrackBankImpl.addChannel of the Java version.
     *
     * @param processor The processor
     * @param command The command
     * @param values The values
     * @return False if the command is not handled
     */
    public boolean handle (final String processor, final String command, final String [] values)
    {
        if (!"track".equals (processor) || !"addTrack".equals (command) || values.length < 1)
            return false;
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
            return true;
        // Name and color of the new track (the cursor track) 300 ms later, as the Java version;
        // the core pages the track bank itself (no scrollIntoView)
        this.model.host.scheduleTask ( () -> {
            if (!this.model.cursorTrack.exists ().get ())
                return;
            if (name != null)
                this.model.cursorTrack.name ().set (name);
            if (color != null)
                setColor (color, this.model.cursorTrack.color ()::set);
        }, 300);
        return true;
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
            // ApplicationImpl.setRecordQuantizationGrid
            case "application" ->
            {
                if ("recordQuantization".equals (command) && value != null)
                    this.model.application.recordQuantizationGrid ().set (value.trim ());
                else
                    this.unhandled.accept (processor + " " + command);
            }
            case "drum" -> this.handleSpecific (this.model.drumDevice, processor, command, 0, value);
            case "primary" -> this.handleSpecific (this.model.firstInstrument, processor, command, 0, value);
            case "eq" -> this.handleEqualizer (command, 0, value);
            case "clip" -> this.handleClip (command, 0, value);
            // ApplicationImpl.invokeAction: a Bitwig action ID; IDs which Bitwig does not know
            // (e.g. Reaper's) are ignored as in the Java version
            case "action" ->
            {
                final Action action = value == null ? null : this.model.application.getAction (value.trim ());
                if (action != null)
                    action.invoke ();
            }
            case "device" ->
            {
                // "add/0 <creation name>": Reaper's device creation names, Bitwig has no such
                // names (ChannelImpl.addDevice of the Java version uses the device metadata)
                if (!"add/0".equals (command))
                    this.unhandled.accept (processor + " " + command);
            }
            default -> this.unhandled.accept (processor + " " + command);
        }
    }


    /**
     * The settings of Reaper's ini file: only the pre-roll length ("reaper/prerollmeas
     * &lt;bars&gt;"), as TransportImpl.setPrerollMeasures of the Java version (none, 1, 2, 4 bars).
     */
    private void handleIniFile (final String command, final int bars)
    {
        if ("reaper/prerollmeas".equals (command) && bars >= 0 && bars < BitwigModel.PREROLLS.length && BitwigModel.PREROLLS[bars] != null)
            this.model.transport.preRoll ().set (BitwigModel.PREROLLS[bars]);
        else
            this.unhandled.accept ("inifile " + command + " " + bars);
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
            case "window/toggle", "expand/toggle", "bypass", "remove", "duplicate", "select" -> handleOwnDevice (device.device, command);
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
                    // DrumPadImpl.select: only selectInEditor; layers as ChannelImpl.select
                    if (value > 0)
                    {
                        if ("pad".equals (parts[0]))
                            channel.selectInEditor ();
                        else
                            selectChannel (channel);
                    }
                }
                else
                    this.handleChannel (channel, parts[2], value, text);
            }
        }
    }


    /**
     * The first EQ+ of the cursor track, as EqualizerDeviceImpl of the Java version, in the layout
     * of the Reaper EQ (see EqualizerDevice): "add" (ChannelImpl.addEqualizerDevice),
     * "param/&lt;index&gt;/value|touch|indicate", "band/&lt;band&gt;" with the Reaper type index as
     * text, "window", "expand".
     */
    private void handleEqualizer (final String command, final double value, final String text)
    {
        final EqualizerDevice eq = this.model.eqDevice;
        if (eq == null)
        {
            this.unhandled.accept ("eq " + command);
            return;
        }
        switch (command)
        {
            case "add" -> this.model.cursorTrack.endOfDeviceChainInsertionPoint ().insertBitwigDevice (EqualizerDevice.ID_BITWIG_EQ_PLUS);
            case "window" -> eq.device.isWindowOpen ().set (value > 0);
            case "expand" -> eq.device.isExpanded ().set (value > 0);
            case "window/toggle", "expand/toggle", "bypass", "remove", "duplicate", "select" -> handleOwnDevice (eq.device, command);
            default ->
            {
                final String [] parts = command.split ("/");
                if (parts.length == 2 && "band".equals (parts[0]) && text != null)
                {
                    try
                    {
                        if (eq.setType (parseIndex (parts[1]), Integer.parseInt (text.trim ())))
                            return;
                    }
                    catch (final NumberFormatException ex)
                    {
                        // Reported below
                    }
                }
                else if (parts.length == 3 && "param".equals (parts[0]))
                {
                    final Parameter parameter = eq.getParameter (parseIndex (parts[1]));
                    if (parameter != null)
                    {
                        switch (parts[2])
                        {
                            case "value" -> this.setValue (parameter, value);
                            case "touch" -> parameter.touch (value > 0);
                            case "indicate" -> parameter.setIndication (value > 0);
                            default -> this.unhandled.accept ("eq " + command);
                        }
                        return;
                    }
                }
                this.unhandled.accept ("eq " + command);
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
            // "…/inc": CursorClipImpl.changePlayStart etc. (getPlayStart ().inc (frac))
            case "start" -> setBeatTime (clip.getPlayStart (), value, this.increment);
            case "end" -> setBeatTime (clip.getPlayStop (), value, this.increment);
            case "loop" -> clip.isLoopEnabled ().set (value > 0);
            case "loopStart" -> setBeatTime (clip.getLoopStart (), value, this.increment);
            case "loopLength" -> setBeatTime (clip.getLoopLength (), value, this.increment);
            case "shuffle" -> clip.getShuffle ().set (value > 0);
            // "accent/immediately": CursorClipImpl.resetAccent (setImmediately)
            case "accent" -> this.setValue (clip.getAccent (), value);
            case "duplicate" -> clip.duplicate ();
            case "duplicateContent" -> clip.duplicateContent ();
            // CursorClipImpl.clearAll (the core's command without a host step grid)
            case "clear" -> clip.clearSteps ();
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
     * A command which sets a value: report the values of the item again a bit later (see
     * ModelReporter.forgetLater), so that the core gets Bitwig's value even if it did not change. Only the item of the command: a track, the master, or a parameter, pad or layer
     * of a device.
     */
    private void reportAgain (final String processor, final String command)
    {
        final String [] parts = command.split ("/");
        switch (processor)
        {
            case "master" -> this.reporter.forgetLater ("/master/", REPORT_AGAIN_DELAY);
            case "track" ->
            {
                if (parseIndex (parts[0]) >= 0)
                    this.reporter.forgetLater ("/track/" + parts[0] + "/", REPORT_AGAIN_DELAY);
                // "[<position>/]param/<index>/…": the remote controls of the cursor track
                final int param = Arrays.asList (parts).indexOf ("param");
                if ((param == 0 || param == 1) && parts.length > param + 1)
                    this.reporter.forgetLater ("/track/fx/param/" + parts[param + 1] + "/", REPORT_AGAIN_DELAY);
            }
            case "transport" ->
            {
                if ("crossfade".equals (parts[0]))
                    this.reporter.forgetLater ("/crossfade", REPORT_AGAIN_DELAY);
            }
            case "device", "primary", "drum", "eq", "project" ->
            {
                if (parts.length >= 2 && ("param".equals (parts[0]) || "pad".equals (parts[0]) || "layer".equals (parts[0])))
                    this.reporter.forgetLater ("/" + processor + "/" + parts[0] + "/" + parts[1] + "/", REPORT_AGAIN_DELAY);
            }
            default ->
            {
                // Nothing to correct
            }
        }
    }


    /**
     * Set a beat time value, or change it by the delta.
     */
    private static void setBeatTime (final SettableBeatTimeValue beatTime, final double value, final boolean increment)
    {
        if (increment)
            beatTime.inc (value);
        else
            beatTime.set (value);
    }


    /**
     * Set the value of a parameter, reset it ("…/reset"), change it ("…/inc"), immediately if the
     * command asked for it.
     */
    private void setValue (final Parameter parameter, final double value)
    {
        if (this.reset)
            parameter.reset ();
        else
            this.setValue (parameter.value (), value);
    }


    /**
     * Set a value (change it with "…/inc"), immediately if the command asked for it.
     */
    private void setValue (final SettableRangedValue rangedValue, final double value)
    {
        if (this.increment)
            rangedValue.inc (value);
        else if (this.immediately)
            rangedValue.setImmediately (value);
        else
            rangedValue.set (value);
    }


    /**
     * The automation write switches, as TransportImpl of the Java version.
     */
    private void handleAutomation (final String command)
    {
        switch (command)
        {
            case "toggleArranger" -> this.model.transport.toggleWriteArrangerAutomation ();
            case "toggleLauncher" -> this.model.transport.toggleWriteClipLauncherAutomation ();
            case "resetOverrides" -> this.model.transport.resetAutomationOverrides ();
            default -> this.unhandled.accept ("automation " + command);
        }
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
            case "overdub/arranger" -> this.model.transport.isArrangerOverdubEnabled ().set (value > 0);
            case "overdub/launcher" -> this.model.transport.isClipLauncherOverdubEnabled ().set (value > 0);
            case "end" -> this.invokeAction ("jump_to_end_of_arrangement");
            // TransportImpl of the Java version for Bitwig
            case "play" -> this.model.transport.play ();
            case "stop" -> this.model.transport.stop ();
            case "restart" -> this.model.transport.restart ();
            case "stopAndRewind" ->
            {
                this.model.transport.stop ();
                // Delay the position movement to make sure that the playback is really stopped
                this.model.host.scheduleTask ( () -> this.model.transport.setPosition (0), 100);
            }
            case "togglePunchIn" -> this.model.transport.isPunchInEnabled ().toggle ();
            case "togglePunchOut" -> this.model.transport.isPunchOutEnabled ().toggle ();
            case "fill" -> this.model.transport.isFillModeActive ().set (value > 0);
            case "toggleFill" -> this.model.transport.isFillModeActive ().toggle ();
            case "metronomeTicks" -> this.model.transport.isMetronomeTickPlaybackEnabled ().set (value > 0);
            case "toggleMetronomeTicks" -> this.model.transport.isMetronomeTickPlaybackEnabled ().toggle ();
            // MetronomeVolumeParameterImpl: set, "/inc"
            case "metronomeVolume" -> this.setValue (this.model.transport.metronomeVolume (), value);
            // TransportImpl.getCrossfadeParameter (a ParameterImpl): set, "/inc", "/reset"
            case "crossfade" -> this.setValue (this.model.transport.crossfade (), value);
            case "crossfade/touch" -> this.model.transport.crossfade ().touch (value > 0);
            case "crossfade/indicate" -> this.model.transport.crossfade ().setIndication (value > 0);
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
            // ApplicationImpl.previousPanelLayout / nextPanelLayout
            // ApplicationImpl.toggleRecordQuantizationNoteLength
            case "recordQuantization/noteLength/toggle" -> this.model.application.recordQuantizeNoteLength ().toggle ();
            case "layout/-" -> this.model.application.previousPanelLayout ();
            case "layout/+" -> this.model.application.nextPanelLayout ();
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


    private void handleProject (final String command, final double value)
    {
        switch (command)
        {
            // ApplicationImpl.setEngineActive
            case "engine" ->
            {
                if (value > 0)
                    this.model.application.activateEngine ();
                else
                    this.model.application.deactivateEngine ();
            }
            // ProjectImpl.clearSolo / clearMute
            case "unsoloAll" -> this.model.project.unsoloAll ();
            case "unmuteAll" -> this.model.project.unmuteAll ();
            // ProjectImpl: the cue volume and mix parameters
            case "cueVolume" -> this.setValue (this.model.project.cueVolume (), value);
            case "cueMix" -> this.setValue (this.model.project.cueMix (), value);
            // ProjectImpl: the remote controls of the root track group
            case "parameterPage" ->
            {
                if (this.model.projectRemoteControls != null)
                {
                    final int count = this.model.projectRemoteControls.pageCount ().get ();
                    if (count > 0)
                        this.model.projectRemoteControls.selectedPageIndex ().set (Math.clamp ((int) value, 0, count - 1));
                }
            }
            case "cueVolume/touch" -> this.model.project.cueVolume ().touch (value > 0);
            case "cueMix/touch" -> this.model.project.cueMix ().touch (value > 0);
            case "createScene" -> this.model.project.createScene ();
            case "createSceneFromPlayingLauncherClips" -> this.model.project.createSceneFromPlayingLauncherClips ();
            case "previous" -> this.model.application.previousProject ();
            case "next" -> this.model.application.nextProject ();
            case "save" -> this.invokeAction ("Save");
            case "load" -> this.invokeAction ("Open");
            default ->
            {
                final String [] parts = command.split ("/");
                if (parts.length != 3 || !"param".equals (parts[0]) || !this.handleParameter (this.model.projectPages, parts, value))
                    this.unhandled.accept ("project " + command);
            }
        }
    }


    /**
     * The cue markers, as MarkerBankImpl and MarkerImpl of the Java version: "add", "previous",
     * "next", "&lt;position&gt;/launch|select|remove" with the absolute position in the marker
     * bank.
     */
    private void handleMarker (final String command)
    {
        switch (command)
        {
            case "add" -> this.model.transport.addCueMarkerAtPlaybackPosition ();
            case "previous" -> this.findAndSelectMarker (false);
            case "next" -> this.findAndSelectMarker (true);
            default ->
            {
                final String [] parts = command.split ("/");
                final int index = parts.length == 2 ? parseIndex (parts[0]) : -1;
                if (this.model.markerBank == null || index < 0 || index >= BitwigModel.NUM_MARKERS)
                {
                    this.unhandled.accept ("marker " + command);
                    return;
                }
                final CueMarker marker = this.model.markerBank.getItemAt (index);
                switch (parts[1])
                {
                    // The controllers of the Java version launch quantized
                    case "launch" -> marker.launch (true);
                    case "select" -> this.setPosition (marker.position ().get ());
                    case "remove" -> marker.deleteObject ();
                    default -> this.unhandled.accept ("marker " + command);
                }
            }
        }
    }


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


    private void changeTempo (final String command)
    {
        // "+" and "-" are the slow changes: 0.01 as TransportImpl.changeTempo (Reaper: 0.1)
        final double delta = switch (command)
        {
            case "+" -> 0.01;
            case "++" -> 1;
            case "-" -> -0.01;
            case "--" -> -1;
            default -> 0;
        };
        final double tempo = this.model.transport.tempo ().value ().getRaw ();
        this.model.transport.tempo ().value ().setRaw (tempo + delta);
    }


    private void handleTime (final String command, final double seconds)
    {
        // "signature <numerator + denominator * 100>": TransportImpl.setTimeSignature
        if ("signature".equals (command))
        {
            final int value = (int) seconds;
            final int numerator = value % 100;
            final int denominator = value / 100;
            if (numerator > 0 && denominator > 0)
            {
                this.model.transport.timeSignature ().numerator ().set (numerator);
                this.model.transport.timeSignature ().denominator ().set (denominator);
            }
            return;
        }
        final double tempo = this.model.transport.tempo ().value ().getRaw ();
        final double beats = seconds * tempo / 60.0;
        switch (command)
        {
            case "", "nosnap" -> this.setPosition (beats);
            // The loop in seconds (the Reaper commands; the core sends "beats loop/…" itself)
            case "loop/start" -> this.model.transport.arrangerLoopStart ().set (beats);
            case "loop/length" -> this.model.transport.arrangerLoopDuration ().set (beats);
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
        switch (command)
        {
            // AbstractTrackBankImpl.setIndication
            case "indicate" ->
            {
                this.model.trackBank.setShouldShowClipLauncherFeedback (value > 0);
                return;
            }
            // CursorTrackImpl.setPinned, togglePinned, selectPrevious, selectNext
            case "pinned" ->
            {
                this.model.cursorTrack.isPinned ().set (value > 0);
                return;
            }
            case "pinned/toggle" ->
            {
                this.model.cursorTrack.isPinned ().toggle ();
                return;
            }
            case "selectPrevious" ->
            {
                this.model.cursorTrack.selectPrevious ();
                return;
            }
            case "selectNext" ->
            {
                this.model.cursorTrack.selectNext ();
                return;
            }
            // Show the page of the cursor track's remote controls in Bitwig as well
            // (ParameterPageBankImpl.scrollTo)
            case "parameterPage" ->
            {
                final CursorRemoteControlsPage remoteControls = this.model.trackRemoteControls;
                if (remoteControls != null)
                {
                    final int count = remoteControls.pageCount ().get ();
                    if (count > 0)
                        remoteControls.selectedPageIndex ().set (Math.clamp ((int) value, 0, count - 1));
                }
                return;
            }
            default ->
            {
                // "param/<index>/…" without a position (touch, indicate, the master)
                if (command.startsWith ("param/"))
                {
                    this.handleTrackParameter (command, value);
                    return;
                }
            }
        }
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
            case "select" ->
            {
                if (value > 0)
                    selectChannel (track);
            }
            // AbstractItemBankImpl.scrollTo: only scrolls (the core pages with "bank track" now)
            case "scrollto" -> this.model.trackBank.scrollPosition ().set (index);
            case "recarm" -> track.arm ().set (value > 0);
            case "monitor" -> track.monitorMode ().set (value > 0 ? "ON" : "OFF");
            case "autoMonitor" -> track.monitorMode ().set (value > 0 ? "AUTO" : "OFF");
            case "isGroupExpanded" -> track.isGroupExpanded ().set (value > 0);
            case "remove" -> track.deleteObject ();
            case "duplicate" -> track.duplicateObject ();
            case "stop" -> track.stop ();
            case "stopAlt" -> track.stopAlt ();
            case "returnToArrangement" -> track.returnToArrangement ();
            case "movePrev" -> this.moveTrack (track, index, -1);
            case "moveNext" -> this.moveTrack (track, index, 1);
            // ChannelImpl.toggleMultiSelect: no multi-selection in the Bitwig API
            case "toggleMultiSelect" ->
            {
                // Intentionally empty, as the Java version
            }
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
                else if (subCommand.startsWith ("param/"))
                    this.handleTrackParameter (subCommand, value);
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
     * "param/&lt;index&gt;/value|touch|indicate": the remote controls of the cursor track, as
     * CursorTrackImpl.getParameterBank of the Java version. The core sends them for its cursor
     * track, which is Bitwig's cursor track, so the position is not needed.
     */
    private void handleTrackParameter (final String command, final double value)
    {
        final String [] parts = command.split ("/");
        if (parts.length != 3 || !this.handleParameter (this.model.trackPages, parts, value))
            this.unhandled.accept ("track " + command);
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
        final int index = parts.length >= 3 ? parseIndex (parts[1]) : -1;
        if (index < 0 || index >= this.model.numSends)
        {
            this.unhandled.accept ("track send " + command);
            return;
        }
        final Parameter send = track.sendBank ().getItemAt (index);
        // "send/<index>/volume/indicate": ParameterImpl.setIndication
        if (parts.length == 4 && "volume".equals (parts[2]) && "indicate".equals (parts[3]))
        {
            send.setIndication (value > 0);
            return;
        }
        switch (parts[2])
        {
            case "volume" -> this.setValue (send, value);
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
            // ChannelImpl.select (the master; tracks are selected in handleTrack)
            case "select" ->
            {
                if (value > 0)
                    selectChannel (channel);
            }
            case "mute" -> channel.mute ().set (value > 0);
            case "active" -> channel.isActivated ().set (value > 0);
            case "solo" -> channel.solo ().set (value > 0);
            // ChannelImpl.toggleSolo
            case "solo/toggle" -> channel.solo ().toggleUsingPreferences (false);
            // ParameterImpl.setIndication of the volume and panorama
            case "volume/indicate" -> channel.volume ().setIndication (value > 0);
            case "pan/indicate" -> channel.pan ().setIndication (value > 0);
            case "volume" -> this.setValue (channel.volume (), value);
            case "pan" -> this.setValue (channel.pan (), value);
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
            case "pinned" -> this.model.cursorDevice.isPinned ().set (value > 0);
            case "expand" -> this.model.cursorDevice.isExpanded ().set (value > 0);
            case "window/toggle", "expand/toggle", "bypass", "remove", "duplicate", "select" -> handleOwnDevice (this.model.cursorDevice, command);
            // "preset <index>": the result of Reaper's preset browser, only sent without the
            // host's browser (which Bitwig always reports)
            case "preset" ->
            {
                // Intentionally empty
            }
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
        // sibling/<index+1>/bypass|remove|duplicate|select: the sibling of the window (DeviceImpl)
        if (parts.length == 3 && "sibling".equals (parts[0]))
        {
            final int index = parseIndex (parts[1]) - 1;
            if (index >= 0 && index < BitwigModel.NUM_SIBLINGS && handleOwnDevice (this.model.siblings.getItemAt (index), parts[2]))
                return;
            this.unhandled.accept ("device " + command);
            return;
        }
        // <position+1>/movePrev|moveNext: the core moves its cursor device, which is Bitwig's
        if (parts.length == 2 && ("movePrev".equals (parts[1]) || "moveNext".equals (parts[1])))
        {
            this.moveCursorDevice ("movePrev".equals (parts[1]) ? -1 : 1);
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
                    // As Reaper's DeviceProcessor: the value is the new bypass state
                    case "bypass" -> device.isEnabled ().set (value <= 0);
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
            case "value" -> this.setValue (parameter, value);
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
     * The commands of a device on the device itself, as SpecificDeviceImpl and DeviceImpl of the
     * Java version: toggle the window and the expanded state, toggle the bypass, remove,
     * duplicate, select.
     *
     * @return False if the command is unknown
     */
    private static boolean handleOwnDevice (final Device device, final String command)
    {
        switch (command)
        {
            case "window/toggle" -> device.isWindowOpen ().toggle ();
            case "expand/toggle" -> device.isExpanded ().toggle ();
            case "bypass" -> device.isEnabled ().toggle ();
            case "remove" -> device.deleteObject ();
            case "duplicate" -> device.afterDeviceInsertionPoint ().copyDevices (device);
            case "select" -> device.selectInEditor ();
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
