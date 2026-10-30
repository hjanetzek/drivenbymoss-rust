// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.ControllerExtension;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.MidiIn;
import com.bitwig.extension.controller.api.MidiOut;
import com.bitwig.extension.controller.api.NoteInput;

import java.io.IOException;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.FileTime;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;


/**
 * One controller of the Rust core in Bitwig: loads the core, connects the MIDI ports and ticks
 * the core. In dev mode (environment variable DBM_DEV_LIBRARY) it watches the library and
 * restarts the extension when it was rebuilt.
 */
public class DbmExtension extends ControllerExtension implements NativeCore.Callbacks
{
    /** The interval of the core's update tick in milliseconds (as in the Java version). */
    private static final int                 TICK_INTERVAL      = 30;
    /** The interval to check the development library for changes in milliseconds. */
    private static final int                 DEV_CHECK_INTERVAL = 500;

    /** Log all MIDI from and to the controller to stderr if the environment variable is set. */
    private static final boolean             LOG_MIDI           = System.getenv ("DBM_LOG_MIDI") != null && !System.getenv ("DBM_LOG_MIDI").isBlank ();
    /**
     * The conformance runner restarts the controller with this system exclusive message (see
     * ProtocolRecorder for its markers); only if the environment variable DBM_OSC_CONFORMANCE is
     * set, as the conformance module of the Java version.
     */
    private static final byte []             RESTART            = { (byte) 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x03, (byte) 0xF7 };
    private static final boolean             RESTARTABLE        = System.getenv ("DBM_OSC_CONFORMANCE") != null && !System.getenv ("DBM_OSC_CONFORMANCE").isBlank ();

    private final DbmExtensionDefinition     definition;
    private final Map<Integer, NoteInput>    noteInputs         = new HashMap<> ();
    private final Set<String>                unhandledCommands  = new TreeSet<> ();
    private final ModelReporter              reporter           = new ModelReporter ();
    private NativeCore                       core;
    private BitwigModel                      model;
    private CommandHandler                   commands;
    private HostSettings                     settings;
    private MidiIn                           midiIn;
    private MidiOut                          midiOut;
    private boolean                          isInitializing;
    private boolean                          isRunning;

    // Dev mode: the state of the library file when it was loaded, and at the last check
    private Path                             devLibrary;
    /** Records the protocol for the replay tests, null if not enabled. */
    private ProtocolRecorder                 recorder;
    private FileState                        loadedState;
    private FileState                        lastState;

    private record FileState (long size, FileTime modified)
    {
        static FileState of (final Path file)
        {
            try
            {
                return new FileState (Files.size (file), Files.getLastModifiedTime (file));
            }
            catch (final IOException ex)
            {
                // Missing while the build writes it
                return null;
            }
        }
    }


    /**
     * Constructor.
     *
     * @param definition The definition of the controller
     * @param host The Bitwig host
     */
    protected DbmExtension (final DbmExtensionDefinition definition, final ControllerHost host)
    {
        super (definition, host);
        this.definition = definition;
    }


    /** {@inheritDoc} */
    @Override
    public void init ()
    {
        final ControllerHost host = this.getHost ();

        this.devLibrary = NativeLibraryLoader.getDevLibrary ();
        if (this.devLibrary != null)
        {
            this.loadedState = FileState.of (this.devLibrary);
            this.lastState = this.loadedState;
            this.println ("Dev mode: using " + this.devLibrary + ", reloading when it changes.");
            host.scheduleTask (this::checkDevLibrary, DEV_CHECK_INTERVAL);
        }

        try
        {
            this.core = new NativeCore (NativeLibraryLoader.provide (), this);
        }
        catch (final IOException ex)
        {
            this.errorln ("Could not load the native library: " + ex.getMessage ());
            return;
        }
        catch (final ReflectiveOperationException | RuntimeException | UnsatisfiedLinkError ex)
        {
            this.error ("Could not load the native library", ex);
            return;
        }

        final Path configFolder = NativeLibraryLoader.getConfigFolder ();
        try
        {
            Files.createDirectories (configFolder);
        }
        catch (final IOException ex)
        {
            this.error ("Could not create the configuration folder " + configFolder, ex);
        }

        final String appVersion = host.getHostProduct () + " " + host.getHostVersion ();
        if (!this.core.createController (this.definition.getCoreId (), configFolder.toString (), appVersion))
        {
            this.errorln ("The core (" + this.core.getVersion () + ") does not support " + this.definition.getHardwareVendor () + " " + this.definition.getHardwareModel ());
            this.closeCore ();
            return;
        }

        this.recorder = ProtocolRecorder.create (this.definition.getHardwareModel (), this.devLibrary);
        if (this.recorder != null)
        {
            this.recorder.init (this.definition.getCoreId (), appVersion, configFolder);
            this.println ("Recording the protocol to " + this.recorder.getFile ());
        }

        this.midiIn = host.getMidiInPort (0);
        this.midiOut = host.getMidiOutPort (0);
        this.midiIn.setMidiCallback ( (status, data1, data2) -> this.receiveMidi (new byte []
        {
            (byte) status,
            (byte) data1,
            (byte) data2
        }));
        this.midiIn.setSysexCallback (data -> this.receiveMidi (HexFormat.of ().parseHex (data)));
        if (LOG_MIDI)
            this.println ("Logging MIDI to stderr (DBM_LOG_MIDI).");

        // Note inputs can only be created during init, the core creates them on start
        this.isInitializing = true;
        this.record ("start");
        this.core.startInfrastructure ();
        this.isInitializing = false;

        // The Bitwig objects the controller needs; they can only be created during init
        final String modelSetup = this.core.getModelSetup ();
        final NoteInput defaultNoteInput = this.noteInputs.get (Integer.valueOf (0));
        this.model = new BitwigModel (host, modelSetup, defaultNoteInput == null ? null : defaultNoteInput.arpeggiator ());
        this.commands = new CommandHandler (this.model, this.reporter, command -> this.unhandled (command));
        // The settings in Bitwig's UI; Bitwig keeps their values (the document settings in the
        // project), they replace the values the core loaded
        this.settings = new HostSettings (host, this.core.getSettings (), (id, value) -> {
            this.record ("setting", id.toString (), value);
            this.core.setSetting (id.intValue (), value);
        });
        this.core.takeSettingChanges ();
        this.println ("DrivenByMoss core " + this.core.getVersion () + " started. Model setup: " + modelSetup);
        this.isRunning = true;
        host.scheduleTask (this::tickLoop, TICK_INTERVAL);
    }


    /** {@inheritDoc} */
    @Override
    public void flush ()
    {
        // The state is collected in the tick loop
    }


    /** {@inheritDoc} */
    @Override
    public void exit ()
    {
        this.isRunning = false;
        this.closeCore ();
        if (this.recorder != null)
            this.recorder.close ();
    }


    private void record (final String kind, final String... fields)
    {
        if (this.recorder != null)
            this.recorder.event (kind, fields);
    }


    private void receiveMidi (final byte [] data)
    {
        if (LOG_MIDI)
            this.logMidi ("in ", data);
        // The conformance runner restarts the controller for a run from a fresh state
        if (RESTARTABLE && Arrays.equals (data, RESTART))
        {
            this.println ("Restart requested by the conformance runner");
            this.getHost ().restart ();
            return;
        }
        if (this.recorder != null)
        {
            // A marker of the conformance runner is not for the core
            if (this.recorder.handleMarker (data))
                return;
            this.recorder.midi ("in", data);
        }
        if (this.core != null)
            this.core.onMidiEvent (0, data);
    }


    private void logMidi (final String direction, final byte [] data)
    {
        System.err.println ("[DrivenByMoss-Bitwig] " + this.definition.getHardwareModel () + ": MIDI " + direction + " " + HexFormat.ofDelimiter (" ").withUpperCase ().formatHex (data));
    }


    private void closeCore ()
    {
        if (this.core == null)
            return;
        try
        {
            this.core.close ();
        }
        catch (final RuntimeException ex)
        {
            this.error ("Could not shut down the core", ex);
        }
        this.core = null;
    }


    private void tickLoop ()
    {
        if (!this.isRunning || this.core == null)
            return;
        // Send the changed state first, so that the controller is updated with fresh values
        this.model.collect (this.reporter);
        final String lines = this.reporter.take ();
        if (!lines.isEmpty ())
            this.record ("model", lines);
        this.core.updateModel (lines);
        this.record ("tick");
        this.core.tick ();
        this.settings.apply (this.core.takeSettingChanges ());
        for (final String notification: this.core.takeNotifications ().split ("\n"))
        {
            if (!notification.isEmpty ())
                this.getHost ().showPopupNotification (notification);
        }
        this.getHost ().scheduleTask (this::tickLoop, TICK_INTERVAL);
    }


    /**
     * Dev mode: restart the extension when the library changed and its size and time stayed the
     * same for one check (the build has finished writing it).
     */
    private void checkDevLibrary ()
    {
        if (this.devLibrary == null)
            return;
        final FileState state = FileState.of (this.devLibrary);
        final boolean isStable = state != null && state.equals (this.lastState);
        this.lastState = state;
        if (isStable && !state.equals (this.loadedState))
        {
            this.println ("Dev mode: " + this.devLibrary + " changed, restarting.");
            this.devLibrary = null;
            this.getHost ().restart ();
            return;
        }
        this.getHost ().scheduleTask (this::checkDevLibrary, DEV_CHECK_INTERVAL);
    }


    // NativeCore.Callbacks

    /** {@inheritDoc} */
    @Override
    public void processNoArg (final String processor, final String command)
    {
        this.record ("noArg", processor, command);
        if ("refresh".equals (processor))
            this.reporter.requestDump ();
        else if (this.commands != null)
            this.commands.handle (processor, command);
    }


    /** {@inheritDoc} */
    @Override
    public void processStringArg (final String processor, final String command, final String value)
    {
        this.record ("string", processor, command, value);
        if (this.commands != null)
            this.commands.handle (processor, command, value);
    }


    /** {@inheritDoc} */
    @Override
    public void processStringArgs (final String processor, final String command, final String [] values)
    {
        if (this.recorder != null)
        {
            final String [] fields = new String [values.length + 2];
            fields[0] = processor;
            fields[1] = command;
            System.arraycopy (values, 0, fields, 2, values.length);
            this.recorder.event ("strings", fields);
        }
        this.unhandled (processor + " " + command);
    }


    /** {@inheritDoc} */
    @Override
    public void processIntArg (final String processor, final String command, final int value)
    {
        this.record ("int", processor, command, Integer.toString (value));
        if (this.commands != null)
            this.commands.handle (processor, command, value);
    }


    /** {@inheritDoc} */
    @Override
    public void processDoubleArg (final String processor, final String command, final double value)
    {
        this.record ("double", processor, command, Double.toString (value));
        if (this.commands != null)
            this.commands.handle (processor, command, value);
    }


    /** {@inheritDoc} */
    @Override
    public void delayUpdates (final String processor)
    {
        this.record ("delay", processor);
    }


    /** {@inheritDoc} */
    @Override
    public void enableUpdates (final String processor, final boolean enable)
    {
        this.record ("enable", processor, enable ? "1" : "0");
    }


    /** {@inheritDoc} */
    @Override
    public boolean openMidiInput (final int deviceId)
    {
        return deviceId == 0 && this.midiIn != null;
    }


    /** {@inheritDoc} */
    @Override
    public boolean openMidiOutput (final int deviceId)
    {
        return deviceId == 0 && this.midiOut != null;
    }


    /** {@inheritDoc} */
    @Override
    public void closeMidiInput (final int deviceId)
    {
        // The ports belong to Bitwig
    }


    /** {@inheritDoc} */
    @Override
    public void closeMidiOutput (final int deviceId)
    {
        // The ports belong to Bitwig
    }


    /** {@inheritDoc} */
    @Override
    public void sendMidiData (final int deviceId, final byte [] data)
    {
        if (deviceId != 0 || this.midiOut == null || data.length == 0)
            return;
        if (LOG_MIDI)
            this.logMidi ("out", data);
        if (this.recorder != null)
            this.recorder.midi ("out", data);
        if ((data[0] & 0xFF) == 0xF0)
            this.midiOut.sendSysex (data);
        else if (data.length >= 3)
            this.midiOut.sendMidi (data[0] & 0xFF, data[1] & 0xFF, data[2] & 0xFF);
        else if (data.length == 2)
            this.midiOut.sendMidi (data[0] & 0xFF, data[1] & 0xFF, 0);
    }


    /** {@inheritDoc} */
    @Override
    public void setNoteInputFilters (final int deviceId, final int noteInputIndex, final String [] filters)
    {
        if (this.recorder != null)
        {
            final String [] fields = new String [filters.length + 1];
            fields[0] = Integer.toString (noteInputIndex);
            System.arraycopy (filters, 0, fields, 1, filters.length);
            this.recorder.event ("filters", fields);
        }
        if (deviceId != 0 || this.midiIn == null)
            return;
        if (!this.isInitializing)
        {
            this.errorln ("Note input " + noteInputIndex + " can only be created while the extension starts.");
            return;
        }
        final NoteInput noteInput = this.midiIn.createNoteInput (this.definition.getHardwareModel () + (noteInputIndex == 0 ? "" : " " + (noteInputIndex + 1)), toBitwigMasks (filters));
        // The core needs to see the notes as well (e.g. to light the pads)
        noteInput.setShouldConsumeEvents (false);
        this.noteInputs.put (Integer.valueOf (noteInputIndex), noteInput);
    }


    /** {@inheritDoc} */
    @Override
    public void setNoteInputKeyTranslationTable (final int deviceId, final int noteInputIndex, final int [] table)
    {
        if (this.recorder != null)
            this.recorder.ints ("keys", noteInputIndex, table);
        final NoteInput noteInput = this.noteInputs.get (Integer.valueOf (noteInputIndex));
        if (noteInput != null)
            noteInput.setKeyTranslationTable (toObjects (table));
    }


    /** {@inheritDoc} */
    @Override
    public void setNoteInputVelocityTranslationTable (final int deviceId, final int noteInputIndex, final int [] table)
    {
        if (this.recorder != null)
            this.recorder.ints ("velocities", noteInputIndex, table);
        final NoteInput noteInput = this.noteInputs.get (Integer.valueOf (noteInputIndex));
        if (noteInput != null)
            noteInput.setVelocityTranslationTable (toObjects (table));
    }


    /** {@inheritDoc} */
    @Override
    public void log (final String message)
    {
        this.getHost ().println (message);
    }


    /** {@inheritDoc} */
    @Override
    public void error (final String message, final Throwable throwable)
    {
        final StringWriter trace = new StringWriter ();
        throwable.printStackTrace (new PrintWriter (trace));
        this.errorln (message + ": " + trace);
    }


    /**
     * Print a message to the script console and to stderr (Bitwig's log and terminal).
     */
    private void println (final String message)
    {
        this.getHost ().println (message);
        System.err.println ("[DrivenByMoss-Bitwig] " + this.definition.getHardwareModel () + ": " + message);
    }


    /**
     * Print an error to the script console and to stderr.
     */
    private void errorln (final String message)
    {
        this.getHost ().errorln (message);
        System.err.println ("[DrivenByMoss-Bitwig] " + this.definition.getHardwareModel () + ": ERROR: " + message);
    }


    /**
     * Log each command which is not handled yet once.
     */
    private void unhandled (final String command)
    {
        final String key = command.replaceAll ("\\d+", "<n>");
        if (this.unhandledCommands.add (key))
            this.println ("Not handled yet: " + key);
    }


    /**
     * The core sends the filters as prefixes for the Reaper backend (e.g. "B6" or "B040", the
     * wildcards of the channel already expanded). Bitwig needs masks of 3 bytes: "B6????".
     */
    private static String [] toBitwigMasks (final String [] filters)
    {
        final String [] masks = new String [filters.length];
        for (int i = 0; i < filters.length; i++)
            masks[i] = (filters[i] + "??????").substring (0, Math.max (6, filters[i].length ()));
        return masks;
    }


    private static Object [] toObjects (final int [] table)
    {
        final Object [] result = new Object [table.length];
        for (int i = 0; i < table.length; i++)
            result[i] = Integer.valueOf (table[i]);
        return result;
    }
}
