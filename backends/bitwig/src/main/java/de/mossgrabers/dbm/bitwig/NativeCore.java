// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_INT;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.nio.file.Path;


/**
 * The binding of the C interface of the Rust core (include/drivenbymoss.h) with the Foreign
 * Function and Memory API. One instance loads the library, creates one controller instance of
 * the core and owns the callback stubs. {@link #close()} destroys the core instance and unloads
 * the library (where the platform allows it).
 *
 * All calls must come from the same thread (Bitwig's controller thread). The callbacks are
 * called on that thread while a call into the core is running; they must not call the core.
 */
public final class NativeCore implements AutoCloseable
{
    /** Do not start the core's own timer thread, the host calls {@link #tick()}. */
    private static final int DBM_FLAG_HOST_TICK = 1;

    /** The number of pointers in the struct DbmCallbacks (user + 19 functions). */
    private static final int NUM_CALLBACK_FIELDS = 20;


    /**
     * The functions the core calls. Implementations must not throw; exceptions are caught and
     * logged, since an exception in an upcall would end the JVM.
     */
    public interface Callbacks
    {
        void processNoArg (String processor, String command);

        void processStringArg (String processor, String command, String value);

        void processStringArgs (String processor, String command, String [] values);

        void processIntArg (String processor, String command, int value);

        void processDoubleArg (String processor, String command, double value);

        void delayUpdates (String processor);

        void enableUpdates (String processor, boolean enable);

        boolean openMidiInput (int deviceId);

        boolean openMidiOutput (int deviceId);

        void closeMidiInput (int deviceId);

        void closeMidiOutput (int deviceId);

        void sendMidiData (int deviceId, byte [] data);

        void sendRawMidiEvent (int deviceId, int status, int data1, int data2);

        void setNoteInputFilters (int deviceId, int noteInputIndex, String [] filters);

        void setNoteInputKeyTranslationTable (int deviceId, int noteInputIndex, int [] table);

        void setNoteInputVelocityTranslationTable (int deviceId, int noteInputIndex, int [] table);

        void log (String message);

        void error (String message, Throwable throwable);
    }


    private static final Linker LINKER = Linker.nativeLinker ();

    private final Arena         arena = Arena.ofShared ();
    private final Callbacks     callbacks;
    private final MethodHandle  createController;
    private final MethodHandle  destroy;
    private final MethodHandle  startInfrastructure;
    private final MethodHandle  tick;
    private final MethodHandle  updateModel;
    private final MethodHandle  onMidiEvent;
    private final MethodHandle  getModelSetup;
    private final MethodHandle  getSettings;
    private final MethodHandle  setSetting;
    private final MethodHandle  takeSettingChanges;
    /** Null for a core before 2026-11-02 (dev mode can load an older library). */
    private final MethodHandle  takeSettingStates;
    private final MethodHandle  takeNotifications;
    private final MethodHandle  freeString;
    private final MemorySegment callbackTable;
    private final String        version;
    private MemorySegment       app = MemorySegment.NULL;


    /**
     * Load the library and bind its functions.
     *
     * @param library The library file
     * @param callbacks The functions the core calls
     * @throws ReflectiveOperationException The binding of the callbacks failed
     */
    public NativeCore (final Path library, final Callbacks callbacks) throws ReflectiveOperationException
    {
        this.callbacks = callbacks;

        final SymbolLookup lookup = SymbolLookup.libraryLookup (library, this.arena);
        this.createController = downcall (lookup, "dbm_create_controller", FunctionDescriptor.of (ADDRESS, ADDRESS, ADDRESS, ADDRESS, ADDRESS, JAVA_INT));
        this.destroy = downcall (lookup, "dbm_destroy", FunctionDescriptor.ofVoid (ADDRESS));
        this.startInfrastructure = downcall (lookup, "dbm_start_infrastructure", FunctionDescriptor.ofVoid (ADDRESS));
        this.tick = downcall (lookup, "dbm_tick", FunctionDescriptor.ofVoid (ADDRESS));
        this.updateModel = downcall (lookup, "dbm_update_model", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS));
        this.onMidiEvent = downcall (lookup, "dbm_on_midi_event", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT, ADDRESS, JAVA_INT));
        this.getModelSetup = downcall (lookup, "dbm_get_model_setup", FunctionDescriptor.of (ADDRESS, ADDRESS));
        this.getSettings = downcall (lookup, "dbm_get_settings", FunctionDescriptor.of (ADDRESS, ADDRESS));
        this.setSetting = downcall (lookup, "dbm_set_setting", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT, ADDRESS));
        this.takeSettingChanges = downcall (lookup, "dbm_take_setting_changes", FunctionDescriptor.of (ADDRESS, ADDRESS));
        this.takeSettingStates = lookup.find ("dbm_take_setting_states").map (symbol -> LINKER.downcallHandle (symbol, FunctionDescriptor.of (ADDRESS, ADDRESS))).orElse (null);
        this.takeNotifications = downcall (lookup, "dbm_take_notifications", FunctionDescriptor.of (ADDRESS, ADDRESS));
        this.freeString = downcall (lookup, "dbm_free_string", FunctionDescriptor.ofVoid (ADDRESS));

        final MethodHandle versionHandle = downcall (lookup, "dbm_version", FunctionDescriptor.of (ADDRESS));
        this.version = readString ((MemorySegment) invoke (versionHandle));

        // The struct DbmCallbacks, in the order of the header; the user pointer is not needed
        // since the stubs are bound to this object
        final MemorySegment [] fields =
        {
            MemorySegment.NULL,
            this.upcall ("upcallNoArg", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS, ADDRESS)),
            this.upcall ("upcallStringArg", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS, ADDRESS, ADDRESS)),
            this.upcall ("upcallStringArgs", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS, ADDRESS, ADDRESS, JAVA_INT)),
            this.upcall ("upcallIntArg", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS, ADDRESS, JAVA_INT)),
            this.upcall ("upcallDoubleArg", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS, ADDRESS, JAVA_DOUBLE)),
            this.upcall ("upcallDelayUpdates", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS)),
            this.upcall ("upcallEnableUpdates", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS, JAVA_INT)),
            this.upcall ("upcallProcessMidiArg", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT, JAVA_INT, JAVA_INT, JAVA_INT)),
            // get_midi_inputs, get_midi_outputs: not used for a single controller
            MemorySegment.NULL,
            MemorySegment.NULL,
            this.upcall ("upcallOpenMidiInput", FunctionDescriptor.of (JAVA_INT, ADDRESS, JAVA_INT)),
            this.upcall ("upcallOpenMidiOutput", FunctionDescriptor.of (JAVA_INT, ADDRESS, JAVA_INT)),
            this.upcall ("upcallCloseMidiInput", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT)),
            this.upcall ("upcallCloseMidiOutput", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT)),
            this.upcall ("upcallSendMidiData", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT, ADDRESS, JAVA_INT)),
            this.upcall ("upcallNoteInputFilters", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT, JAVA_INT, ADDRESS, JAVA_INT)),
            this.upcall ("upcallKeyTranslationTable", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT, JAVA_INT, ADDRESS, JAVA_INT)),
            this.upcall ("upcallVelocityTranslationTable", FunctionDescriptor.ofVoid (ADDRESS, JAVA_INT, JAVA_INT, ADDRESS, JAVA_INT)),
            this.upcall ("upcallLog", FunctionDescriptor.ofVoid (ADDRESS, ADDRESS))
        };
        if (fields.length != NUM_CALLBACK_FIELDS)
            throw new IllegalStateException ("The callback table does not match the header.");
        this.callbackTable = this.arena.allocate (ADDRESS, NUM_CALLBACK_FIELDS);
        for (int i = 0; i < fields.length; i++)
            this.callbackTable.setAtIndex (ADDRESS, i, fields[i]);
    }


    /**
     * Get the version of the library.
     *
     * @return The version
     */
    public String getVersion ()
    {
        return this.version;
    }


    /**
     * Create the core instance for one controller. Its MIDI ports are the device IDs 0.
     *
     * @param controllerId The UUID of the controller in the core
     * @param configPath The folder for the controller's configuration file
     * @param appVersion The name and version of the host
     * @return True if the controller is supported
     */
    public boolean createController (final String controllerId, final String configPath, final String appVersion)
    {
        try (final Arena call = Arena.ofConfined ())
        {
            this.app = (MemorySegment) invoke (this.createController, this.callbackTable, call.allocateFrom (controllerId), call.allocateFrom (configPath), call.allocateFrom (appVersion), Integer.valueOf (DBM_FLAG_HOST_TICK));
        }
        return !this.app.equals (MemorySegment.NULL);
    }


    /**
     * Start the controller.
     */
    public void startInfrastructure ()
    {
        if (this.isValid ())
            invoke (this.startInfrastructure, this.app);
    }


    /**
     * Run the scheduled tasks of the core and update the controller. Call every 30ms.
     */
    public void tick ()
    {
        if (this.isValid ())
            invoke (this.tick, this.app);
    }


    /**
     * Send model data to the core: lines of "address value".
     *
     * @param data The data
     */
    public void updateModel (final String data)
    {
        if (!this.isValid () || data.isEmpty ())
            return;
        try (final Arena call = Arena.ofConfined ())
        {
            invoke (this.updateModel, this.app, call.allocateFrom (data));
        }
    }


    /**
     * Hand a MIDI message received from the controller to the core.
     *
     * @param deviceId The ID of the MIDI input
     * @param data The message
     */
    public void onMidiEvent (final int deviceId, final byte [] data)
    {
        if (!this.isValid () || data.length == 0)
            return;
        try (final Arena call = Arena.ofConfined ())
        {
            invoke (this.onMidiEvent, this.app, Integer.valueOf (deviceId), call.allocateFrom (JAVA_BYTE, data), Integer.valueOf (data.length));
        }
    }


    /**
     * Get the model setup of the running controller (bank sizes etc.) as JSON.
     *
     * @return The JSON text, empty if the controller is not running
     */
    public String getModelSetup ()
    {
        return this.isValid () ? this.takeString ((MemorySegment) invoke (this.getModelSetup, this.app)) : "";
    }


    /**
     * Get all settings of the controller, one line per setting with tab separated fields: ID,
     * scope (global, document), kind (enum, integer, boolean, text), category, label, default,
     * value, then the options of an enumeration or the minimum and maximum of an integer.
     *
     * @return The settings, empty until the controller was started
     */
    public String getSettings ()
    {
        return this.isValid () ? this.takeString ((MemorySegment) invoke (this.getSettings, this.app)) : "";
    }


    /**
     * Set a setting from Bitwig's settings UI.
     *
     * @param id The ID of the setting
     * @param value The value as text
     */
    public void setSetting (final int id, final String value)
    {
        if (!this.isValid ())
            return;
        try (final Arena call = Arena.ofConfined ())
        {
            invoke (this.setSetting, this.app, Integer.valueOf (id), call.allocateFrom (value));
        }
    }


    /**
     * Get the settings which the controller changed since the last call.
     *
     * @return One "ID&lt;tab&gt;value" per line
     */
    public String takeSettingChanges ()
    {
        return this.isValid () ? this.takeString ((MemorySegment) invoke (this.takeSettingChanges, this.app)) : "";
    }


    /**
     * Get the settings whose visibility or enabled state changed since the last call.
     *
     * @return One "ID&lt;tab&gt;visible&lt;tab&gt;enabled" per line (1 or 0), empty for an older
     *         core
     */
    public String takeSettingStates ()
    {
        return this.isValid () && this.takeSettingStates != null ? this.takeString ((MemorySegment) invoke (this.takeSettingStates, this.app)) : "";
    }


    /**
     * Get the notifications since the last call.
     *
     * @return One notification per line
     */
    public String takeNotifications ()
    {
        return this.isValid () ? this.takeString ((MemorySegment) invoke (this.takeNotifications, this.app)) : "";
    }


    /**
     * Destroy the core instance (the controller is shut down) and unload the library.
     */
    @Override
    public void close ()
    {
        if (this.isValid ())
        {
            invoke (this.destroy, this.app);
            this.app = MemorySegment.NULL;
        }
        this.arena.close ();
    }


    private boolean isValid ()
    {
        return !this.app.equals (MemorySegment.NULL);
    }


    private String takeString (final MemorySegment text)
    {
        final String result = readString (text);
        invoke (this.freeString, text);
        return result;
    }


    private static MethodHandle downcall (final SymbolLookup lookup, final String name, final FunctionDescriptor descriptor)
    {
        final MemorySegment symbol = lookup.find (name).orElseThrow ( () -> new UnsatisfiedLinkError ("Function not found in the native library: " + name));
        return LINKER.downcallHandle (symbol, descriptor);
    }


    private static Object invoke (final MethodHandle handle, final Object... arguments)
    {
        try
        {
            return handle.invokeWithArguments (arguments);
        }
        catch (final RuntimeException | Error ex)
        {
            throw ex;
        }
        catch (final Throwable ex)
        {
            throw new IllegalStateException (ex);
        }
    }


    private MemorySegment upcall (final String method, final FunctionDescriptor descriptor) throws ReflectiveOperationException
    {
        final MethodHandle handle = MethodHandles.lookup ().findVirtual (NativeCore.class, method, descriptor.toMethodType ()).bindTo (this);
        return LINKER.upcallStub (handle, descriptor, this.arena);
    }


    private static String readString (final MemorySegment text)
    {
        return text.equals (MemorySegment.NULL) ? "" : text.reinterpret (Long.MAX_VALUE).getString (0);
    }


    private static String [] readStrings (final MemorySegment values, final int count)
    {
        if (values.equals (MemorySegment.NULL) || count <= 0)
            return new String [0];
        final MemorySegment array = values.reinterpret (ADDRESS.byteSize () * count);
        final String [] result = new String [count];
        for (int i = 0; i < count; i++)
            result[i] = readString (array.getAtIndex (ADDRESS, i));
        return result;
    }


    private static int [] readInts (final MemorySegment values, final int count)
    {
        if (values.equals (MemorySegment.NULL) || count <= 0)
            return new int [0];
        return values.reinterpret (JAVA_INT.byteSize () * count).toArray (JAVA_INT);
    }


    // The upcalls, called by the core; the first argument is the (unused) user pointer

    @SuppressWarnings("unused")
    private void upcallNoArg (final MemorySegment user, final MemorySegment processor, final MemorySegment command)
    {
        try
        {
            this.callbacks.processNoArg (readString (processor), readString (command));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("processNoArg", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallStringArg (final MemorySegment user, final MemorySegment processor, final MemorySegment command, final MemorySegment value)
    {
        try
        {
            this.callbacks.processStringArg (readString (processor), readString (command), readString (value));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("processStringArg", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallStringArgs (final MemorySegment user, final MemorySegment processor, final MemorySegment command, final MemorySegment values, final int count)
    {
        try
        {
            this.callbacks.processStringArgs (readString (processor), readString (command), readStrings (values, count));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("processStringArgs", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallIntArg (final MemorySegment user, final MemorySegment processor, final MemorySegment command, final int value)
    {
        try
        {
            this.callbacks.processIntArg (readString (processor), readString (command), value);
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("processIntArg", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallDoubleArg (final MemorySegment user, final MemorySegment processor, final MemorySegment command, final double value)
    {
        try
        {
            this.callbacks.processDoubleArg (readString (processor), readString (command), value);
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("processDoubleArg", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallDelayUpdates (final MemorySegment user, final MemorySegment processor)
    {
        try
        {
            this.callbacks.delayUpdates (readString (processor));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("delayUpdates", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallEnableUpdates (final MemorySegment user, final MemorySegment processor, final int enable)
    {
        try
        {
            this.callbacks.enableUpdates (readString (processor), enable != 0);
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("enableUpdates", ex);
        }
    }


    @SuppressWarnings("unused")
    private int upcallOpenMidiInput (final MemorySegment user, final int deviceId)
    {
        try
        {
            return this.callbacks.openMidiInput (deviceId) ? 1 : 0;
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("openMidiInput", ex);
            return 0;
        }
    }


    @SuppressWarnings("unused")
    private int upcallOpenMidiOutput (final MemorySegment user, final int deviceId)
    {
        try
        {
            return this.callbacks.openMidiOutput (deviceId) ? 1 : 0;
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("openMidiOutput", ex);
            return 0;
        }
    }


    @SuppressWarnings("unused")
    private void upcallCloseMidiInput (final MemorySegment user, final int deviceId)
    {
        try
        {
            this.callbacks.closeMidiInput (deviceId);
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("closeMidiInput", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallCloseMidiOutput (final MemorySegment user, final int deviceId)
    {
        try
        {
            this.callbacks.closeMidiOutput (deviceId);
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("closeMidiOutput", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallProcessMidiArg (final MemorySegment user, final int deviceId, final int status, final int data1, final int data2)
    {
        try
        {
            this.callbacks.sendRawMidiEvent (deviceId, status, data1, data2);
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("sendRawMidiEvent", ex);
        }
    }


    private void upcallSendMidiData (final MemorySegment user, final int deviceId, final MemorySegment data, final int length)
    {
        try
        {
            if (!data.equals (MemorySegment.NULL) && length > 0)
                this.callbacks.sendMidiData (deviceId, data.reinterpret (length).toArray (JAVA_BYTE));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("sendMidiData", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallNoteInputFilters (final MemorySegment user, final int deviceId, final int noteInputIndex, final MemorySegment filters, final int count)
    {
        try
        {
            this.callbacks.setNoteInputFilters (deviceId, noteInputIndex, readStrings (filters, count));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("setNoteInputFilters", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallKeyTranslationTable (final MemorySegment user, final int deviceId, final int noteInputIndex, final MemorySegment table, final int count)
    {
        try
        {
            this.callbacks.setNoteInputKeyTranslationTable (deviceId, noteInputIndex, readInts (table, count));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("setNoteInputKeyTranslationTable", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallVelocityTranslationTable (final MemorySegment user, final int deviceId, final int noteInputIndex, final MemorySegment table, final int count)
    {
        try
        {
            this.callbacks.setNoteInputVelocityTranslationTable (deviceId, noteInputIndex, readInts (table, count));
        }
        catch (final Throwable ex)
        {
            this.callbacks.error ("setNoteInputVelocityTranslationTable", ex);
        }
    }


    @SuppressWarnings("unused")
    private void upcallLog (final MemorySegment user, final MemorySegment message)
    {
        try
        {
            this.callbacks.log (readString (message));
        }
        catch (final Throwable ex)
        {
            // Nothing left to report to
        }
    }
}
