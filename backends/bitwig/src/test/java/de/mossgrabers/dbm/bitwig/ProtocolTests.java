// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.BrowserResultsItemBank;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.CursorBrowserFilterItem;
import com.bitwig.extension.controller.api.CursorBrowserResultItem;

import java.lang.foreign.MemorySegment;
import java.lang.reflect.Array;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.ParameterizedType;
import java.lang.reflect.Proxy;
import java.lang.reflect.Type;
import java.lang.reflect.TypeVariable;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.EnumSet;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.TreeSet;

import de.mossgrabers.dbm.bitwig.Protocol.Cmd;


/**
 * The tests of the binary protocol of the extension (doc/TYPED_PROTOCOL_PLAN.md, step 3), run by
 * Maven in the test phase (no test framework: the main method fails with exit code 1).
 * <ul>
 * <li>the command coverage (§2.5): a record of every command with plausible indices and values
 * goes through the command handler on a stand-in of the Bitwig API; the commands which end as not
 * handled must be exactly {@link CommandHandler#UNHANDLED};</li>
 * <li>the writer and the view: the records of every command read back;</li>
 * <li>the rounding of the doubles as the text protocol's "%.6f";</li>
 * <li>with the native library (system property dbm.library, skipped if it does not exist): the
 * dual run of the reports (every record equals the core's conversion of the text line) for
 * every report ID and the value forms of the extension; the addresses of the keys.</li>
 * </ul>
 */
public final class ProtocolTests
{
    private static final List<String> FAILURES = new ArrayList<> ();
    private static int                checks;


    private ProtocolTests ()
    {
        // Tests
    }


    /**
     * Run the tests.
     *
     * @param args Not used
     */
    public static void main (final String [] args)
    {
        testRound6 ();
        testWriterAndView ();
        testCommandCoverage ();
        testHandledBitset ();
        final String library = System.getProperty ("dbm.library", "");
        if (!library.isEmpty () && Files.isRegularFile (Path.of (library)))
            testReportsWithTheCore (Path.of (library));
        else
            System.out.println ("SKIP the tests with the core: no library (" + library + ")");

        System.out.println ("Protocol tests: " + checks + " checks, " + FAILURES.size () + " failures");
        for (final String failure: FAILURES)
            System.out.println ("FAIL " + failure);
        if (!FAILURES.isEmpty ())
            System.exit (1);
    }


    private static void check (final boolean condition, final String message)
    {
        checks++;
        if (!condition)
            FAILURES.add (message);
    }


    // ------------------------------------------------------------------------------------------
    // Rounding

    /** The fast path of round6 gives the same double as the text. */
    private static void testRound6 ()
    {
        final Random random = new Random (42);
        final double [] special = { 0, -0.0, 0.5, 0.25, 1, -1, 0.1, 0.123456, 0.0078125, 1.0000005, 0.0000005, -0.0000004, 1e-9, -1e-9, 123456.1234565, 0.7086614173228346, 1e15, 3.0 / 16384 };
        for (final double value: special)
            checkRound6 (value);
        for (int i = 0; i < 100_000; i++)
        {
            checkRound6 (random.nextDouble ());
            checkRound6 (random.nextInt (2_000_001) / 1e6 - 1);
            checkRound6 (random.nextInt (16384) / 16384.0);
        }
    }


    private static void checkRound6 (final double value)
    {
        final double expected = Double.parseDouble (ModelReporter.formatDouble (value));
        check (Double.doubleToRawLongBits (ModelReporter.round6 (value)) == Double.doubleToRawLongBits (expected), "round6 (" + value + ") = " + ModelReporter.round6 (value) + ", the text gives " + expected);
    }


    // ------------------------------------------------------------------------------------------
    // Writer and view

    /** The plausible index of an index field of a command (for the samples). */
    private static int sampleIndex (final Cmd cmd, final String name)
    {
        if ((cmd == Cmd.DRUM_CLEAR_MUTE || cmd == Cmd.DRUM_CLEAR_SOLO) && "dev".equals (name))
            return Protocol.Processor.DRUM;
        return switch (name)
        {
            case "ch_kind" -> Protocol.ChanKind.TRACK;
            case "ch_processor", "bank", "area" -> Protocol.Processor.TRACK;
            case "dev" -> Protocol.Processor.DEVICE;
            case "owner" -> Protocol.Processor.DRUM;
            case "grid_steps" -> 16;
            case "grid_rows" -> 8;
            default -> 0;
        };
    }


    /** A buffer with one record of the command: plausible indices and a value of its kind. */
    private static byte [] commandRecord (final Cmd cmd)
    {
        final int [] idx = new int [4];
        final String [] slots = cmd.slots.isEmpty () ? new String [0] : cmd.slots.split (",");
        for (int i = 0; i < slots.length; i++)
        {
            for (final String part: slots[i].split ("\\|"))
                idx[i] |= sampleIndex (cmd, part);
        }
        final long key = Protocol.key (cmd.id, idx[0], idx[1], idx[2]);
        final Protocol.Writer w = new Protocol.Writer (256);
        switch (cmd.kind)
        {
            case "bool" -> w.putInline (key, idx[3], Protocol.T_BOOL, Protocol.OP_SET, 1);
            case "i32" -> w.putInline (key, idx[3], Protocol.T_I32, Protocol.OP_SET, 1);
            case "f64", "value_op" -> w.putInline (key, idx[3], Protocol.T_F64, Protocol.OP_SET, Double.doubleToRawLongBits (0.5));
            case "text" ->
            {
                final String text = cmd == Cmd.GRID_STEP_SET_FROM || cmd == Cmd.GRID_STEP_UPDATE ? String.join (" ", java.util.Collections.nCopies (24, "0.5")) : "x";
                w.putPayload (key, idx[3], Protocol.T_TEXT, Protocol.OP_SET, 0, text.getBytes (StandardCharsets.UTF_8));
            }
            case "text_list" -> w.putPayload (key, idx[3], Protocol.T_TEXT_LIST, Protocol.OP_SET, 0, new byte [] { 1, 0, 0, 0, 1, 0, 0, 0, 'x' });
            case "color" -> w.putColor (key, idx[3], 1, 2, 3, true);
            case "pair" -> w.putInline (key, idx[3], Protocol.T_STRUCT, Protocol.OP_SET, 1L | 1L << 32);
            case "int_double" ->
            {
                final byte [] payload = new byte [16];
                payload[0] = 1;
                final long half = Double.doubleToRawLongBits (0.5);
                for (int i = 0; i < 8; i++)
                    payload[8 + i] = (byte) (half >>> 8 * i);
                w.putPayload (key, idx[3], Protocol.T_STRUCT, Protocol.OP_SET, 0, payload);
            }
            default -> w.putNone (key, idx[3]);
        }
        return w.toByteArray ();
    }


    /** Every command reads back from its record: the ID, the type, the indices by name. */
    private static void testWriterAndView ()
    {
        final Protocol.View view = new Protocol.View ();
        for (final Cmd cmd: Cmd.values ())
        {
            final byte [] buffer = commandRecord (cmd);
            check (buffer.length % 8 == 0, cmd + ": the buffer is padded");
            check (view.reset (MemorySegment.ofArray (buffer)) && view.next (), cmd + ": one record");
            check (view.cmd () == cmd, cmd + ": the command reads back");
            check (!view.next (), cmd + ": only one record");
            view.reset (MemorySegment.ofArray (buffer));
            view.next ();
            if (cmd.slots.startsWith ("ch_kind"))
                check (view.chKind () == Protocol.ChanKind.TRACK && view.chProcessor () == Protocol.Processor.TRACK && view.chIndex () == 0, cmd + ": the channel fields");
            if (cmd.slots.startsWith ("grid_steps"))
                check (view.gridSteps () == 16 && view.gridRows () == 8, cmd + ": the grid fields");
        }
        // Packed and signed fields, the text list, the color
        final Protocol.Writer w = new Protocol.Writer (64);
        w.putNone (Protocol.key (Cmd.GRID_STEP_CLEAR.id, 16, 8, 3 << 7 | 100), -5);
        w.putPayload (Protocol.key (Cmd.TRACK_ADD.id, 0, 0, 0), 0, Protocol.T_TEXT_LIST, Protocol.OP_SET, 0, new byte [] { 2, 0, 0, 0, 1, 0, 0, 0, 'a', 2, 0, 0, 0, (byte) 0xC3, (byte) 0xBC });
        w.putColor (Protocol.key (Cmd.SCENE_COLOR.id, 2, 0, 0), 0, 10, 20, 30, true);
        view.reset (w.buffer ());
        check (view.next () && view.channel () == 3 && view.note () == 100 && view.step () == -5, "the packed and signed fields of a grid step");
        check (view.next () && List.of (view.textList ()).equals (List.of ("a", "ü")), "a text list");
        check (view.next () && view.scene () == 2 && view.colorPart (0) == 10 && view.colorPart (1) == 20 && view.colorPart (2) == 30 && view.colorPart (3) == 1, "a color");
        check (!view.next (), "the end of the buffer");
        // A wrong header
        final byte [] wrong = w.toByteArray ();
        wrong[0] = 'X';
        check (!view.reset (MemorySegment.ofArray (wrong)) && !view.next (), "a wrong header has no records");
    }


    // ------------------------------------------------------------------------------------------
    // Command coverage

    /** The model setup of the coverage: every part of the model which a controller can ask for. */
    private static final String MODEL_SETUP = "{\"numTracks\":8,\"numScenes\":8,\"numSends\":2,\"numParamPages\":2,\"numParams\":8,\"numMarkers\":1,\"numResults\":4,\"numFilterColumnEntries\":4,\"numDeviceLayers\":4,\"wantsClipLauncherNavigator\":true,\"deviceIds\":[\"FIRST_INSTRUMENT\",\"EQ\"],\"noteClips\":[[16,8]]}";


    /** The commands which a fresh handler does not handle (as CommandHandler.UNHANDLED). */
    private static void testCommandCoverage ()
    {
        final ModelReporter reporter = new ModelReporter (ModelReporter.Mode.BINARY, null);
        final Set<Cmd> unhandled = EnumSet.noneOf (Cmd.class);
        final List<String> messages = new ArrayList<> ();
        for (final Cmd cmd: Cmd.values ())
        {
            // A fresh model: a command changes its state (e.g. the window of the tracks)
            final BitwigModel model = new BitwigModel (stub (ControllerHost.class), MODEL_SETUP, null);
            final List<String> calls = new ArrayList<> ();
            final CommandHandler handler = new CommandHandler (model, reporter, calls::add);
            try
            {
                handler.processCommands (MemorySegment.ofArray (commandRecord (cmd)));
            }
            catch (final RuntimeException ex)
            {
                calls.add ("exception " + ex);
            }
            if (!calls.isEmpty ())
            {
                unhandled.add (cmd);
                messages.add (cmd + ": " + calls);
            }
        }
        final Set<Cmd> missing = EnumSet.copyOf (unhandled);
        missing.removeAll (CommandHandler.UNHANDLED);
        final Set<Cmd> extra = EnumSet.copyOf (CommandHandler.UNHANDLED);
        extra.removeAll (unhandled);
        check (missing.isEmpty (), "commands not handled but declared handled: " + missing + " " + messages);
        check (extra.isEmpty (), "commands handled but declared unhandled: " + extra);
    }


    /** The bitset of the core: every command except UNHANDLED. */
    private static void testHandledBitset ()
    {
        final long [] words = CommandHandler.handledBitset ();
        for (final Cmd cmd: Cmd.values ())
        {
            final int index = cmd.id & 0x7FFF;
            final boolean bit = index / 64 < words.length && (words[index / 64] >>> index % 64 & 1) != 0;
            check (bit == !CommandHandler.UNHANDLED.contains (cmd), cmd + ": the bit of the handled commands");
        }
    }


    /** The return type of a method of an interface, with the type arguments of the interface. */
    private static Class<?> returnType (final Class<?> type, final Method method)
    {
        final Type generic = method.getGenericReturnType ();
        if (generic instanceof final TypeVariable<?> variable)
        {
            final Type actual = resolve (type, variable, new HashMap<> ());
            if (actual instanceof final Class<?> c)
                return c;
            if (actual instanceof final ParameterizedType parameterized)
                return (Class<?>) parameterized.getRawType ();
        }
        return method.getReturnType ();
    }


    /** The type argument of a type variable in the interfaces of a type (null if unknown). */
    private static Type resolve (final Class<?> type, final TypeVariable<?> variable, final Map<TypeVariable<?>, Type> bindings)
    {
        for (final Type parent: type.getGenericInterfaces ())
        {
            final Map<TypeVariable<?>, Type> parentBindings = new HashMap<> ();
            final Class<?> raw;
            if (parent instanceof final ParameterizedType parameterized)
            {
                raw = (Class<?>) parameterized.getRawType ();
                final TypeVariable<?> [] parameters = raw.getTypeParameters ();
                final Type [] arguments = parameterized.getActualTypeArguments ();
                for (int i = 0; i < parameters.length; i++)
                    parentBindings.put (parameters[i], arguments[i] instanceof final TypeVariable<?> v && bindings.containsKey (v) ? bindings.get (v) : arguments[i]);
            }
            else
                raw = (Class<?>) parent;
            if (parentBindings.containsKey (variable))
                return parentBindings.get (variable);
            final Type found = resolve (raw, variable, parentBindings);
            if (found != null)
                return found;
        }
        return null;
    }


    /** The interfaces a stand-in implements in addition to the declared return type (casts). */
    private static final Class<?> [] EXTRA_INTERFACES = { CursorBrowserFilterItem.class, CursorBrowserResultItem.class, BrowserResultsItemBank.class };


    /**
     * A stand-in of an interface of the Bitwig API: every method returns a stand-in of its
     * interface type (the same for each method), true, 0, an empty text or an empty array.
     */
    @SuppressWarnings("unchecked")
    static <T> T stub (final Class<T> type)
    {
        final List<Class<?>> interfaces = new ArrayList<> ();
        for (final Class<?> extra: EXTRA_INTERFACES)
        {
            if (type.isAssignableFrom (extra))
                interfaces.add (extra);
        }
        if (interfaces.isEmpty ())
            interfaces.add (type);
        final Map<Method, Object> results = new HashMap<> ();
        final InvocationHandler handler = (proxy, method, args) -> {
            if (method.getDeclaringClass () == Object.class)
            {
                return switch (method.getName ())
                {
                    case "equals" -> Boolean.valueOf (proxy == args[0]);
                    case "hashCode" -> Integer.valueOf (System.identityHashCode (proxy));
                    default -> type.getSimpleName () + " stand-in";
                };
            }
            final Class<?> result = returnType (type, method);
            if (result == void.class)
                return null;
            if (result == boolean.class)
                return Boolean.TRUE;
            if (result == int.class)
                return Integer.valueOf (0);
            if (result == long.class)
                return Long.valueOf (0);
            if (result == double.class)
                return Double.valueOf (0);
            if (result == float.class)
                return Float.valueOf (0);
            if (result == String.class)
                return "";
            if (result.isArray ())
                return Array.newInstance (result.getComponentType (), 0);
            if (result.isInterface ())
                return results.computeIfAbsent (method, m -> stub (result));
            return null;
        };
        return (T) Proxy.newProxyInstance (ProtocolTests.class.getClassLoader (), interfaces.toArray (new Class<?> [0]), handler);
    }


    // ------------------------------------------------------------------------------------------
    // The reports with the core

    /** A sample index of a report slot (an enum-like slot 0, a position 1). */
    private static int reportIndex (final String name)
    {
        return switch (name)
        {
            case "ch_kind", "pad_kind", "pad_dev", "dev", "owner", "param_kind", "param_param", "item_kind", "flag", "limit" -> 0;
            case "grid_steps" -> 16;
            case "grid_rows" -> 8;
            case "channel" -> 1 << 7;
            case "row" -> 60;
            default -> 1;
        };
    }


    /** The dual run of the reports: every record equals the core's conversion of the text line. */
    private static void testReportsWithTheCore (final Path library)
    {
        final NativeCore core;
        try
        {
            core = new NativeCore (library, stub (NativeCore.Callbacks.class));
        }
        catch (final ReflectiveOperationException | RuntimeException ex)
        {
            check (false, "load the library " + library + ": " + ex);
            return;
        }
        check (core.getProtocolMismatch () == null, "the protocol of the library: " + core.getProtocolMismatch ());
        final List<String> log = new ArrayList<> ();
        final ModelReporter.Conversions conversions = new ModelReporter.Conversions ()
        {
            @Override
            public String messagesToText (final MemorySegment buffer, final long size)
            {
                return core.messagesToText (buffer, size);
            }


            @Override
            public byte [] messagesFromText (final String text)
            {
                return core.messagesFromText (text);
            }


            @Override
            public void log (final String message)
            {
                log.add (message);
            }
        };
        final ModelReporter reporter = new ModelReporter (ModelReporter.Mode.CHECK, conversions);
        final ModelReporter text = new ModelReporter (ModelReporter.Mode.TEXT, conversions);
        final Set<String> unknown = new TreeSet<> ();
        int ids = 0;
        for (int id = 0; id < 0x1000; id++)
        {
            final int type = Protocol.msgType (id);
            if (type < 0)
                continue;
            ids++;
            final String [] slots = Protocol.msgSlots (id).isEmpty () ? new String [0] : Protocol.msgSlots (id).split (",");
            final int [] idx = new int [4];
            for (int i = 0; i < slots.length; i++)
            {
                for (final String part: slots[i].split ("\\|"))
                    idx[i] |= reportIndex (part);
            }
            final long key = Protocol.key (id, idx[0], idx[1], idx[2]);
            final int i3 = idx[3];
            putSamples (reporter, id, type, key, i3);
            // The address of the key in the text form is the template of the ID
            text.startCollection ();
            text.requestDump ();
            text.put4 (key, i3, type == Protocol.T_TEXT ? "x" : "0");
            final String line = text.getLines ();
            if (line.isEmpty () || line.startsWith ("#"))
                unknown.add (Protocol.msgName (id) + ": " + line);
        }
        check (ids > 200, "the report IDs: " + ids);
        check (unknown.isEmpty (), "keys without an address: " + unknown);
        check (reporter.getCheckErrors () == 0, "the records differ from the text: " + log);
        check (reporter.getCheckedRecords () > 1000, "records compared: " + reporter.getCheckedRecords ());
        System.out.println ("Dual run: " + reporter.getCheckedRecords () + " records of " + ids + " report IDs compared with the text");

        testForget (conversions);
        core.close ();
    }


    private static void put (final ModelReporter reporter, final Runnable put)
    {
        reporter.startCollection ();
        reporter.requestDump ();
        put.run ();
        reporter.finishCollection ();
    }


    /** The value forms of the extension for a report ID. */
    private static void putSamples (final ModelReporter r, final int id, final int type, final long key, final int i3)
    {
        if (id == Protocol.Msg.CHANNEL_TYPE)
        {
            for (final String name: new String [] { "AUDIO", "INSTRUMENT", "HYBRID", "GROUP", "EFFECT", "MASTER", "BOGUS" })
                put (r, () -> r.put4 (key, i3, name));
            return;
        }
        if (id == Protocol.Msg.RECORD_QUANTIZATION)
        {
            for (final String name: new String [] { "OFF", "1/32", "1/16", "1/8", "1/4", "bogus", " 1/8 " })
                put (r, () -> r.put4 (key, i3, name));
            return;
        }
        switch (type)
        {
            case Protocol.T_BOOL ->
            {
                for (final boolean value: new boolean [] { true, false })
                    put (r, () -> r.put4 (key, i3, value));
                for (final int value: new int [] { 0, 1, 2 })
                    put (r, () -> r.put4 (key, i3, value));
            }
            case Protocol.T_I32 ->
            {
                for (final int value: new int [] { 0, 7, -1, 100000 })
                    put (r, () -> r.put4 (key, i3, value));
                for (final boolean value: new boolean [] { true, false })
                    put (r, () -> r.put4 (key, i3, value));
            }
            case Protocol.T_F64 ->
            {
                for (final double value: new double [] { 0, 0.5, -0.0, 0.123456789, 0.0000005, -0.0000004, 1e-9, -1e-9, 0.7086614173228346, 1.0000005, 0.0078125, 123456.1234565, 1.0 / 3, 2.0 / 3 })
                    put (r, () -> r.put4 (key, i3, value));
                for (final int value: new int [] { 0, 3 })
                    put (r, () -> r.put4 (key, i3, value));
                put (r, () -> r.put4 (key, i3, true));
            }
            case Protocol.T_TEXT ->
            {
                for (final String value: new String [] { "abc", "", "two\nlines", "cr\rhere", "ümlaut ♪", " spaces ", "tab\there" })
                    put (r, () -> r.put4 (key, i3, value));
            }
            case Protocol.T_COLOR ->
            {
                put (r, () -> r.putColor (key, 0.5, 0.25, 1));
                put (r, () -> r.putColor (key, 0, 0, 0));
                put (r, () -> r.putColor (key, 1, 1, 1));
                put (r, () -> r.putColor (key, 0.2, 0.4, 0.6));
            }
            case Protocol.T_STRUCT_ARRAY ->
            {
                put (r, () -> r.putPlayingNotes (key, new int [] { 60, 100, 64, 90 }, 2));
                put (r, () -> r.putPlayingNotes (key, new int [0], 0));
            }
            case Protocol.T_STRUCT ->
            {
                final double [] step = { 1, 0, 1, 0.25, 0.7874015748031497, 0, 0.5, 0, 0, 0.1, -12, 0.5, 1, 0.8, 0, 3, 1, 4, 5, 1, 2, 0.5, -0.5, 0.25 };
                put (r, () -> r.putStep (key, i3, step));
                put (r, () -> r.putStep (key, i3, null));
            }
            case Protocol.T_NONE ->
            {
                put (r, () -> r.put4 (key, i3, 3));
                put (r, () -> r.put4 (key, i3, 1.5));
                put (r, () -> r.put4 (key, i3, "x"));
            }
            default -> check (false, Protocol.msgName (id) + ": no samples for the type " + type);
        }
    }


    /** Report again: a forgotten thing is reported again, the others not. */
    private static void testForget (final ModelReporter.Conversions conversions)
    {
        final ModelReporter r = new ModelReporter (ModelReporter.Mode.BINARY, conversions);
        final long track3 = BitwigModel.channelKey (Protocol.ChKind.TRACK, 3);
        final long track4 = BitwigModel.channelKey (Protocol.ChKind.TRACK, 4);
        final Runnable collect = () -> {
            r.put (Protocol.Msg.CHANNEL_VOLUME | track3, 0.5);
            r.put (Protocol.Msg.CHANNEL_NAME | track3, "Bass");
            r.put4 (Protocol.Msg.SEND_VOLUME_STR | track3, 6, "-6dB");
            r.put (Protocol.Msg.CHANNEL_VOLUME | track4, 0.5);
            r.put (Protocol.Msg.PARAM_VALUE | Protocol.key (0, Protocol.ParamOwner.TRACK_FX, 2, 0), 0.25);
            r.put (Protocol.Msg.PAGE_COUNT | Protocol.key (0, Protocol.PageOwner.TRACK_FX, 0, 0), 2);
        };
        final java.util.function.Supplier<String> report = () -> {
            r.startCollection ();
            collect.run ();
            r.finishCollection ();
            return r.getText ();
        };
        check (report.get ().lines ().count () == 6, "the first report is a dump");
        check (report.get ().isEmpty (), "nothing changed");
        r.forget (new ModelReporter.Entity (ModelReporter.Family.CHANNEL, Protocol.ChKind.TRACK, 3, 0));
        final String again = report.get ();
        check (again.equals ("/track/3/volume 0.5\n/track/3/name Bass\n/track/3/send/0/volume/str/6 -6dB"), "the channel is reported again: " + again);
        r.forget (new ModelReporter.Entity (ModelReporter.Family.OWNER, Protocol.ParamOwner.TRACK_FX, 0, 0));
        final String owner = report.get ();
        check (owner.equals ("/track/fx/param/2/value 0.25\n/track/fx/page/count 2"), "the owner is reported again: " + owner);
        r.forget (Protocol.Msg.CHANNEL_VOLUME | track4, 0);
        check (report.get ().equals ("/track/4/volume 0.5"), "one key is reported again");
        r.forgetLater (new ModelReporter.Entity (ModelReporter.Family.CHANNEL, Protocol.ChKind.TRACK, 4, 0), 100_000);
        check (report.get ().isEmpty (), "the delay is not over");
        r.requestDump ();
        check (report.get ().lines ().count () == 6, "a dump");
        // A changed double is reported even if the text is the same
        r.startCollection ();
        r.put (Protocol.Msg.CHANNEL_VOLUME | track3, 0.5000000001);
        r.finishCollection ();
        check (r.getText ().equals ("/track/3/volume 0.5"), "a changed double: " + r.getText ());
    }
}
