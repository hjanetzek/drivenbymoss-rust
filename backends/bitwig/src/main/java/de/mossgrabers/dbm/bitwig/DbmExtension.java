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
import java.util.HashMap;
import java.util.HexFormat;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;
import java.util.concurrent.ConcurrentHashMap;


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

    /** Document settings kept across restarts (dev reloads), per extension ID. */
    private static final Map<String, String> KEPT_SETTINGS      = new ConcurrentHashMap<> ();

    private final DbmExtensionDefinition     definition;
    private final Map<Integer, NoteInput>    noteInputs         = new HashMap<> ();
    private final Set<String>                unhandledCommands  = new TreeSet<> ();
    private NativeCore                       core;
    private MidiIn                           midiIn;
    private MidiOut                          midiOut;
    private boolean                          isInitializing;
    private boolean                          isRunning;

    // Dev mode: the state of the library file when it was loaded, and at the last check
    private Path                             devLibrary;
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
            host.println ("Dev mode: using " + this.devLibrary + ", reloading when it changes.");
            host.scheduleTask (this::checkDevLibrary, DEV_CHECK_INTERVAL);
        }

        try
        {
            this.core = new NativeCore (NativeLibraryLoader.provide (), this);
        }
        catch (final IOException | ReflectiveOperationException | RuntimeException | UnsatisfiedLinkError ex)
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
            host.errorln ("The core (" + this.core.getVersion () + ") does not support " + this.definition.getHardwareVendor () + " " + this.definition.getHardwareModel ());
            this.closeCore ();
            return;
        }

        this.midiIn = host.getMidiInPort (0);
        this.midiOut = host.getMidiOutPort (0);
        this.midiIn.setMidiCallback ( (status, data1, data2) -> this.core.onMidiEvent (0, new byte []
        {
            (byte) status,
            (byte) data1,
            (byte) data2
        }));
        this.midiIn.setSysexCallback (data -> this.core.onMidiEvent (0, HexFormat.of ().parseHex (data)));

        final String settings = KEPT_SETTINGS.remove (this.definition.getId ().toString ());
        if (settings != null)
            this.core.setFormattedDocumentSettings (settings);

        // Note inputs can only be created during init, the core creates them on start
        this.isInitializing = true;
        this.core.startInfrastructure ();
        this.isInitializing = false;

        host.println ("DrivenByMoss core " + this.core.getVersion () + " started. Model setup: " + this.core.getModelSetup ());
        this.isRunning = true;
        host.scheduleTask (this::tickLoop, TICK_INTERVAL);
    }


    /** {@inheritDoc} */
    @Override
    public void flush ()
    {
        // The model updates will be sent from here (stage 1)
    }


    /** {@inheritDoc} */
    @Override
    public void exit ()
    {
        this.isRunning = false;
        if (this.core != null)
            KEPT_SETTINGS.put (this.definition.getId ().toString (), this.core.getFormattedDocumentSettings ());
        this.closeCore ();
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
        this.core.tick ();
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
            this.getHost ().println ("Dev mode: " + this.devLibrary + " changed, restarting.");
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
        this.unhandled (processor, command);
    }


    /** {@inheritDoc} */
    @Override
    public void processStringArg (final String processor, final String command, final String value)
    {
        this.unhandled (processor, command);
    }


    /** {@inheritDoc} */
    @Override
    public void processStringArgs (final String processor, final String command, final String [] values)
    {
        this.unhandled (processor, command);
    }


    /** {@inheritDoc} */
    @Override
    public void processIntArg (final String processor, final String command, final int value)
    {
        this.unhandled (processor, command);
    }


    /** {@inheritDoc} */
    @Override
    public void processDoubleArg (final String processor, final String command, final double value)
    {
        this.unhandled (processor, command);
    }


    /** {@inheritDoc} */
    @Override
    public void delayUpdates (final String processor)
    {
        // Not needed yet
    }


    /** {@inheritDoc} */
    @Override
    public void enableUpdates (final String processor, final boolean enable)
    {
        // Not needed yet
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
        if (deviceId != 0 || this.midiIn == null)
            return;
        if (!this.isInitializing)
        {
            this.getHost ().errorln ("Note input " + noteInputIndex + " can only be created while the extension starts.");
            return;
        }
        final NoteInput noteInput = this.midiIn.createNoteInput (this.definition.getHardwareModel () + (noteInputIndex == 0 ? "" : " " + (noteInputIndex + 1)), filters);
        // The core needs to see the notes as well (e.g. to light the pads)
        noteInput.setShouldConsumeEvents (false);
        this.noteInputs.put (Integer.valueOf (noteInputIndex), noteInput);
    }


    /** {@inheritDoc} */
    @Override
    public void setNoteInputKeyTranslationTable (final int deviceId, final int noteInputIndex, final int [] table)
    {
        final NoteInput noteInput = this.noteInputs.get (Integer.valueOf (noteInputIndex));
        if (noteInput != null)
            noteInput.setKeyTranslationTable (toObjects (table));
    }


    /** {@inheritDoc} */
    @Override
    public void setNoteInputVelocityTranslationTable (final int deviceId, final int noteInputIndex, final int [] table)
    {
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
        this.getHost ().errorln (message + ": " + trace);
    }


    /**
     * Log each command which is not handled yet once.
     */
    private void unhandled (final String processor, final String command)
    {
        final String key = processor + " " + command.replaceAll ("\\d+", "<n>");
        if (this.unhandledCommands.add (key))
            this.getHost ().println ("Not handled yet: " + key);
    }


    private static Object [] toObjects (final int [] table)
    {
        final Object [] result = new Object [table.length];
        for (int i = 0; i < table.length; i++)
            result[i] = Integer.valueOf (table[i]);
        return result;
    }
}
