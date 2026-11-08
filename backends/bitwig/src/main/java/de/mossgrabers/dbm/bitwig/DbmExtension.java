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
import java.lang.foreign.MemorySegment;
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
 *
 * The reports and commands use the binary protocol (doc/TYPED_PROTOCOL_PLAN.md, step 3); the
 * environment variable DBM_HOST_PROTOCOL selects "check" (binary, every report compared with the
 * core's conversion of its text line) or "text" (the text protocol, a debug switch until step 5),
 * see ModelReporter.
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
    /** The note inputs by port and index, see {@link #noteInputKey(int, int)}. */
    private final Map<Integer, NoteInput>    noteInputs         = new HashMap<> ();
    private final Set<String>                unhandledCommands  = new TreeSet<> ();
    private ModelReporter                    reporter;
    private NativeCore                       core;
    private BitwigModel                      model;
    private CommandHandler                   commands;
    private HostSettings                     settings;
    /** The MIDI ports, one pair per surface of the controller (e.g. MCU extenders). */
    private MidiIn []                        midiIns            = new MidiIn [0];
    private MidiOut []                       midiOuts           = new MidiOut [0];
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

        final String mismatch = this.core.getProtocolMismatch ();
        if (mismatch != null)
        {
            this.errorln ("The core (" + this.core.getVersion () + ") does not fit the extension: " + mismatch);
            this.closeCore ();
            return;
        }
        final NativeCore nativeCore = this.core;
        this.reporter = new ModelReporter (ModelReporter.Mode.fromEnvironment (), new ModelReporter.Conversions ()
        {
            @Override
            public String messagesToText (final MemorySegment buffer, final long size)
            {
                return nativeCore.messagesToText (buffer, size);
            }


            @Override
            public byte [] messagesFromText (final String text)
            {
                return nativeCore.messagesFromText (text);
            }


            @Override
            public void log (final String message)
            {
                DbmExtension.this.errorln (message);
            }
        });
        if (this.reporter.getMode () != ModelReporter.Mode.TEXT)
        {
            try
            {
                if (!this.core.setCommandCallback (CommandHandler.handledBitset ()))
                {
                    this.errorln ("The core did not switch to the binary commands.");
                    this.closeCore ();
                    return;
                }
            }
            catch (final ReflectiveOperationException ex)
            {
                this.error ("Could not bind the command callback", ex);
                this.closeCore ();
                return;
            }
        }
        if (this.reporter.getMode () != ModelReporter.Mode.BINARY)
            this.println ("Protocol: " + this.reporter.getMode ().name ().toLowerCase (java.util.Locale.ROOT) + " (DBM_HOST_PROTOCOL).");

        this.recorder = ProtocolRecorder.create (this.definition.getHardwareModel (), this.devLibrary);
        if (this.recorder != null)
        {
            this.recorder.init (this.definition.getCoreId (), appVersion, configFolder);
            this.println ("Recording the protocol to " + this.recorder.getFile ());
        }

        // One device ID per port: the index of the port (several for e.g. MCU extenders)
        this.midiIns = new MidiIn [this.definition.getNumMidiInPorts ()];
        this.midiOuts = new MidiOut [this.definition.getNumMidiOutPorts ()];
        for (int i = 0; i < this.midiOuts.length; i++)
            this.midiOuts[i] = host.getMidiOutPort (i);
        for (int i = 0; i < this.midiIns.length; i++)
        {
            final int port = i;
            final MidiIn midiIn = host.getMidiInPort (port);
            this.midiIns[port] = midiIn;
            midiIn.setMidiCallback ( (status, data1, data2) -> this.receiveMidi (port, new byte []
            {
                (byte) status,
                (byte) data1,
                (byte) data2
            }));
            midiIn.setSysexCallback (data -> this.receiveMidi (port, HexFormat.of ().parseHex (data)));
        }
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
            if (this.recorder != null)
                this.recorder.setting (id.toString (), value);
            this.core.setSetting (id.intValue (), value);
        });
        this.core.takeSettingChanges ();
        // Hidden and disabled settings (e.g. the slot settings of Generic Flexi)
        this.settings.applyStates (this.core.takeSettingStates ());
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
        if (this.reporter != null && this.reporter.getMode () == ModelReporter.Mode.CHECK)
            this.println ("Protocol check: " + this.reporter.getCheckedRecords () + " records compared with the text, " + this.reporter.getCheckErrors () + " differences.");
        this.closeCore ();
        if (this.recorder != null)
            this.recorder.close ();
    }


    private void record (final String kind, final String... fields)
    {
        if (this.recorder != null)
            this.recorder.event (kind, fields);
    }


    /**
     * The kind of a recorded MIDI event: "in" / "out" for the first port, "in1" / "out1" etc. for
     * the others (the recordings of the controllers with one port stay the same).
     *
     * @param direction "in" or "out"
     * @param port The index of the port
     * @return The kind
     */
    private static Integer noteInputKey (final int port, final int noteInputIndex)
    {
        return Integer.valueOf (port * 1000 + noteInputIndex);
    }


    private static String midiKind (final String direction, final int port)
    {
        return port == 0 ? direction : direction + port;
    }


    private void receiveMidi (final int port, final byte [] data)
    {
        if (LOG_MIDI)
            this.logMidi (midiKind ("in", port) + " ", data);
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
            this.recorder.midi (midiKind ("in", port), data);
        }
        if (this.core != null)
            this.core.onMidiEvent (port, data);
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
        this.reporter.startCollection ();
        this.model.collect (this.reporter);
        this.reporter.finishCollection ();
        if (this.reporter.getMode () == ModelReporter.Mode.TEXT)
        {
            final String lines = this.reporter.getLines ();
            if (!lines.isEmpty ())
                this.record ("model", lines);
            this.core.updateModel (lines);
        }
        else
        {
            final Protocol.Writer writer = this.reporter.getWriter ();
            if (!writer.isEmpty ())
            {
                if (this.recorder != null)
                    this.record ("model", this.reporter.getText ());
                this.core.updateModelBin (writer.segment (), writer.size ());
            }
        }
        this.record ("tick");
        this.core.tick ();
        this.settings.apply (this.core.takeSettingChanges ());
        this.settings.applyStates (this.core.takeSettingStates ());
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
    public void processCommands (final MemorySegment buffer)
    {
        if (this.recorder != null)
        {
            for (final String line: this.core.messagesToText (buffer, buffer.byteSize ()).split ("\n"))
            {
                if (!line.isEmpty ())
                    this.recorder.line (line);
            }
        }
        this.dispatch (buffer);
    }


    /**
     * Execute the commands of a buffer. Before the command handler exists (the first commands
     * while the core starts) only the refresh.
     */
    private void dispatch (final MemorySegment buffer)
    {
        if (this.commands != null)
        {
            this.commands.processCommands (buffer);
            return;
        }
        final Protocol.View view = new Protocol.View ();
        view.reset (buffer);
        while (view.next ())
        {
            if (view.cmd () == Protocol.Cmd.REFRESH)
                this.reporter.requestDump ();
        }
    }


    // The text callbacks (the debug switch DBM_HOST_PROTOCOL=text): recorded and converted to
    // the binary form by the core, executed as the binary commands

    /** {@inheritDoc} */
    @Override
    public void processNoArg (final String processor, final String command)
    {
        this.record ("noArg", processor, command);
        this.dispatchText ("noArg", processor, command);
    }


    /** {@inheritDoc} */
    @Override
    public void processStringArg (final String processor, final String command, final String value)
    {
        this.record ("string", processor, command, value);
        this.dispatchText ("string", processor, command, value);
    }


    /** {@inheritDoc} */
    @Override
    public void processStringArgs (final String processor, final String command, final String [] values)
    {
        final String [] fields = new String [values.length + 2];
        fields[0] = processor;
        fields[1] = command;
        System.arraycopy (values, 0, fields, 2, values.length);
        if (this.recorder != null)
            this.recorder.event ("strings", fields);
        this.dispatchText ("strings", fields);
    }


    /** {@inheritDoc} */
    @Override
    public void processIntArg (final String processor, final String command, final int value)
    {
        this.record ("int", processor, command, Integer.toString (value));
        this.dispatchText ("int", processor, command, Integer.toString (value));
    }


    /** {@inheritDoc} */
    @Override
    public void processDoubleArg (final String processor, final String command, final double value)
    {
        this.record ("double", processor, command, Double.toString (value));
        this.dispatchText ("double", processor, command, Double.toString (value));
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


    /**
     * Execute a command of the text callbacks: its legacy line converted by the core.
     */
    private void dispatchText (final String kind, final String... fields)
    {
        final StringBuilder line = new StringBuilder (kind);
        for (final String field: fields)
            line.append ('\t').append (field.replace ("\\", "\\\\").replace ("\n", "\\n").replace ("\t", "\\t"));
        final byte [] buffer = this.core.messagesFromText (line.toString ());
        if (buffer.length > Protocol.HEADER_SIZE)
            this.dispatch (MemorySegment.ofArray (buffer));
        else
            this.unhandled (kind + " " + String.join (" ", fields));
    }


    /** {@inheritDoc} */
    @Override
    public boolean openMidiInput (final int deviceId)
    {
        return deviceId >= 0 && deviceId < this.midiIns.length && this.midiIns[deviceId] != null;
    }


    /** {@inheritDoc} */
    @Override
    public boolean openMidiOutput (final int deviceId)
    {
        return deviceId >= 0 && deviceId < this.midiOuts.length && this.midiOuts[deviceId] != null;
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
        if (deviceId < 0 || deviceId >= this.midiOuts.length || data.length == 0)
            return;
        final MidiOut midiOut = this.midiOuts[deviceId];
        if (midiOut == null)
            return;
        if (LOG_MIDI)
            this.logMidi (midiKind ("out", deviceId), data);
        if (this.recorder != null)
            this.recorder.midi (midiKind ("out", deviceId), data);
        if ((data[0] & 0xFF) == 0xF0)
            midiOut.sendSysex (data);
        else if (data.length >= 3)
            midiOut.sendMidi (data[0] & 0xFF, data[1] & 0xFF, data[2] & 0xFF);
        else if (data.length == 2)
            midiOut.sendMidi (data[0] & 0xFF, data[1] & 0xFF, 0);
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
            this.recorder.event (midiKind ("filters", deviceId), fields);
        }
        if (deviceId < 0 || deviceId >= this.midiIns.length || this.midiIns[deviceId] == null)
            return;
        if (!this.isInitializing)
        {
            this.errorln ("Note input " + noteInputIndex + " can only be created while the extension starts.");
            return;
        }
        // No filters: the controller wants no note input (e.g. Generic Flexi without a keyboard
        // channel, as MidiInputImpl of the Java version with the name null)
        if (filters.length == 0)
            return;
        final String name = this.definition.getHardwareModel () + (deviceId == 0 ? "" : " - Port " + (deviceId + 1)) + (noteInputIndex == 0 ? "" : " " + (noteInputIndex + 1));
        final NoteInput noteInput = this.midiIns[deviceId].createNoteInput (name, toBitwigMasks (filters));
        // The core needs to see the notes as well (e.g. to light the pads)
        noteInput.setShouldConsumeEvents (false);
        this.noteInputs.put (noteInputKey (deviceId, noteInputIndex), noteInput);
    }


    /**
     * A MIDI event as if it was played on the device, e.g. the further notes of a chord: sent to
     * the default note input, as MidiInputImpl.sendRawMidiEvent of the Java version.
     */
    @Override
    public void sendRawMidiEvent (final int deviceId, final int status, final int data1, final int data2)
    {
        if (this.recorder != null)
            this.recorder.midi (midiKind ("raw", deviceId), new byte [] { (byte) status, (byte) data1, (byte) data2 });
        final NoteInput noteInput = this.noteInputs.get (noteInputKey (deviceId, 0));
        if (noteInput != null)
            noteInput.sendRawMidiEvent (status, data1, data2);
    }


    /** {@inheritDoc} */
    @Override
    public void setNoteInputKeyTranslationTable (final int deviceId, final int noteInputIndex, final int [] table)
    {
        if (this.recorder != null)
            this.recorder.ints (midiKind ("keys", deviceId), noteInputIndex, table);
        final NoteInput noteInput = this.noteInputs.get (noteInputKey (deviceId, noteInputIndex));
        if (noteInput != null)
            noteInput.setKeyTranslationTable (toObjects (table));
    }


    /** {@inheritDoc} */
    @Override
    public void setNoteInputVelocityTranslationTable (final int deviceId, final int noteInputIndex, final int [] table)
    {
        if (this.recorder != null)
            this.recorder.ints (midiKind ("velocities", deviceId), noteInputIndex, table);
        final NoteInput noteInput = this.noteInputs.get (noteInputKey (deviceId, noteInputIndex));
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
