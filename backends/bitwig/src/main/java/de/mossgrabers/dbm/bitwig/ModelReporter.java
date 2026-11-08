// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import java.lang.foreign.MemorySegment;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Locale;
import java.util.Map;


/**
 * Collects the reports for the core (doc/TYPED_PROTOCOL_PLAN.md §2.4): the values of the model by
 * key (the ID of the report and its index slots, see {@link Protocol#key}), written as binary
 * records into one buffer per report for dbm_update_model_bin. Remembers the last value of each
 * key and only reports changes, unless a full dump was requested (the core's refresh command).
 * An unchanged value costs one lookup in an open addressing table and a compare.
 *
 * The protocol ({@link Mode}, the environment variable DBM_HOST_PROTOCOL):
 * <ul>
 * <li>binary (the default): records;</li>
 * <li>check: binary, and every record is compared with the core's conversion of the text line
 * which the text protocol reports (the dual run, TYPED_PROTOCOL_PLAN §3); a difference is
 * logged;</li>
 * <li>text: the lines "address value" of the text protocol, the address of a key from the core
 * (a debug switch until step 5).</li>
 * </ul>
 * Doubles are rounded as the text protocol's "%.6f" (TYPED_PROTOCOL_PLAN §5.1, the transition until
 * step 5), so that both protocols carry the same double; a changed double is always reported.
 */
public final class ModelReporter
{
    /** The protocol of the reports. */
    public enum Mode
    {
        TEXT,
        BINARY,
        CHECK;


        /**
         * The protocol of the environment variable DBM_HOST_PROTOCOL (text, binary, check),
         * binary by default.
         *
         * @return The mode
         */
        public static Mode fromEnvironment ()
        {
            final String value = System.getenv ("DBM_HOST_PROTOCOL");
            if (value == null)
                return BINARY;
            return switch (value.trim ().toLowerCase (Locale.ROOT))
            {
                case "text" -> TEXT;
                case "check" -> CHECK;
                default -> BINARY;
            };
        }
    }


    /** The conversions of the core: the text forms (text and check mode, unusual values). */
    public interface Conversions
    {
        /**
         * The text of a binary buffer (dbm_messages_to_text).
         *
         * @param buffer The buffer
         * @param size Its size
         * @return The lines
         */
        String messagesToText (MemorySegment buffer, long size);


        /**
         * The binary buffer of a text (dbm_messages_from_text).
         *
         * @param text The text
         * @return The buffer with the header
         */
        byte [] messagesFromText (String text);


        /**
         * Log a message.
         *
         * @param message The message
         */
        void log (String message);
    }


    /** The kinds of things reported again after a delay (see {@link #forgetLater}). */
    public enum Family
    {
        /** A channel: the reports with the slots ch_kind, ch_pos (index 0, 1). */
        CHANNEL("ch_kind,ch_pos", 2),
        /** A parameter: owner, param (index 0, 1). */
        PARAM("owner,param", 2),
        /** A drum pad or layer: pad_kind, pad_dev, pad_index. */
        PAD("pad_kind,pad_dev,pad_index", 3),
        /** A parameter of the host (crossfader, cue, groove) of a kind: param_kind (index 0). */
        HOST_PARAM("param_kind,param_param", 1),
        /** Everything of a parameter owner: owner (index 0). */
        OWNER("owner", 1);


        private final String     slots;
        private final int        numIndices;
        private final boolean [] ids = new boolean [0x1000];


        private Family (final String slots, final int numIndices)
        {
            this.slots = slots;
            this.numIndices = numIndices;
        }


        boolean matches (final long key, final int i0, final int i1, final int i2)
        {
            final int id = Protocol.keyId (key);
            if (id >= this.ids.length || !this.ids[id] || Protocol.keyIndex (key, 0) != (i0 & 0xFFFF))
                return false;
            if (this.numIndices > 1 && Protocol.keyIndex (key, 1) != (i1 & 0xFFFF))
                return false;
            return this.numIndices <= 2 || Protocol.keyIndex (key, 2) == (i2 & 0xFFFF);
        }


        static
        {
            for (int id = 0; id < 0x1000; id++)
            {
                final String slots = Protocol.msgSlots (id);
                for (final Family family: values ())
                    family.ids[id] = slots.equals (family.slots) || slots.startsWith (family.slots + ",");
            }
        }
    }


    /**
     * A thing reported again: the family and its indices.
     *
     * @param family The family
     * @param i0 The first index
     * @param i1 The second index (if the family has it)
     * @param i2 The third index (if the family has it)
     */
    public record Entity (Family family, int i0, int i1, int i2)
    {
        boolean matches (final long key)
        {
            return this.family.matches (key, this.i0, this.i1, this.i2);
        }
    }


    /** The kinds of the values as put (a value of another kind is a change). */
    private static final byte          TAG_BOOL      = 1;
    private static final byte          TAG_INT       = 2;
    private static final byte          TAG_DOUBLE    = 3;
    private static final byte          TAG_TEXT      = 4;
    private static final byte          TAG_COLOR     = 5;
    private static final byte          TAG_NOTES     = 6;
    private static final byte          TAG_STEP      = 7;

    /** The state of an entry: free, reported, to be reported again. */
    private static final byte          FREE          = 0;
    private static final byte          REPORTED      = 1;
    private static final byte          FORGOTTEN     = 2;

    /** The step values which are ints in the text form (BitwigNoteClip). */
    private static final boolean []    STEP_INTS     = new boolean [24];
    static
    {
        for (final int i: new int [] { 0, 1, 2, 12, 14, 15, 16, 17, 18, 19, 20 })
            STEP_INTS[i] = true;
    }

    /** The position of the slot "limit" by report ID (2 or 3, -1 if none). */
    private static final int []        LIMIT_SLOTS   = new int [0x1000];
    static
    {
        for (int id = 0; id < LIMIT_SLOTS.length; id++)
            LIMIT_SLOTS[id] = Arrays.asList (Protocol.msgSlots (id).split (",")).indexOf ("limit");
    }

    private final Mode                 mode;
    private final Conversions          conversions;
    private final Protocol.Writer      writer        = new Protocol.Writer (16384);
    private final StringBuilder        lines         = new StringBuilder ();
    private boolean                    dumpAll       = true;
    /** The things to report again and when (System.nanoTime), see forgetLater. */
    private final Map<Entity, Long>    pending       = new HashMap<> ();
    /** The number of records which differed from the text in the check mode. */
    private int                        checkErrors;
    /** The number of records written in the check mode. */
    private long                       checkedRecords;

    // The last values: an open addressing table (linear probing) over the key and the fourth
    // index slot; entries are never removed (the keys of a model are a bounded set)
    private long []                    keys;
    private short []                   slots3;
    private byte []                    states;
    private byte []                    tags;
    private long []                    raws;
    /** The texts (text values; the text form in the text protocol), notes and steps. */
    private Object []                  objects;
    /** The addresses of the keys (text form), resolved by the core when needed. */
    private String []                  addresses;
    private int                        count;


    /**
     * Constructor.
     *
     * @param mode The protocol
     * @param conversions The conversions of the core
     */
    public ModelReporter (final Mode mode, final Conversions conversions)
    {
        this.mode = mode;
        this.conversions = conversions;
        this.allocate (4096);
    }


    /**
     * The protocol.
     *
     * @return The mode
     */
    public Mode getMode ()
    {
        return this.mode;
    }


    /**
     * The number of records which differed from the text line in the check mode.
     *
     * @return The number
     */
    public int getCheckErrors ()
    {
        return this.checkErrors;
    }


    /**
     * The number of records compared in the check mode.
     *
     * @return The number
     */
    public long getCheckedRecords ()
    {
        return this.checkedRecords;
    }


    /**
     * Report all values with the next collection, not only the changed ones.
     */
    public void requestDump ()
    {
        this.dumpAll = true;
    }


    /**
     * Report the value of a key again with the next collection.
     *
     * @param key The key
     * @param i3 The fourth index slot
     */
    public void forget (final long key, final int i3)
    {
        final int slot = this.find (key, i3);
        if (slot >= 0 && this.states[slot] == REPORTED)
            this.states[slot] = FORGOTTEN;
    }


    /**
     * Report the values of a thing again with the next collection, even if they did not change.
     *
     * @param entity The thing
     */
    public void forget (final Entity entity)
    {
        for (int slot = 0; slot < this.keys.length; slot++)
        {
            if (this.states[slot] == REPORTED && entity.matches (this.keys[slot]))
                this.states[slot] = FORGOTTEN;
        }
    }


    /**
     * As forget, but only after the delay, counted from the last call for the thing. Used after a
     * command which sets a value: the core changes its value at once (as for Reaper), and if Bitwig
     * does not take the new value (e.g. because a control has to pick up the current value first)
     * nothing would correct it; while a knob is turned Bitwig might not have applied the last value
     * yet, and reporting the old one would set the core back.
     *
     * @param entity The thing
     * @param delayMs The delay in milliseconds
     */
    public void forgetLater (final Entity entity, final int delayMs)
    {
        this.pending.put (entity, Long.valueOf (System.nanoTime () + delayMs * 1_000_000L));
    }


    /**
     * Start a collection: forget the things whose delay is over, start a new report.
     */
    public void startCollection ()
    {
        this.writer.clear ();
        this.lines.setLength (0);
        if (this.pending.isEmpty ())
            return;
        final long now = System.nanoTime ();
        final Iterator<Map.Entry<Entity, Long>> iterator = this.pending.entrySet ().iterator ();
        while (iterator.hasNext ())
        {
            final Map.Entry<Entity, Long> entry = iterator.next ();
            if (entry.getValue ().longValue () - now <= 0)
            {
                this.forget (entry.getKey ());
                iterator.remove ();
            }
        }
    }


    /**
     * End a collection: the report is in {@link #getWriter()} (binary) or {@link #getLines()}
     * (text) until the next collection.
     */
    public void finishCollection ()
    {
        this.dumpAll = false;
    }


    /**
     * The records of the last collection (binary and check mode).
     *
     * @return The writer
     */
    public Protocol.Writer getWriter ()
    {
        return this.writer;
    }


    /**
     * The lines of the last collection (text mode).
     *
     * @return The lines, empty if nothing changed
     */
    public String getLines ()
    {
        return this.lines.toString ();
    }


    /**
     * The text of the last collection (for the recordings and logs): the lines of the text mode,
     * the core's text of the records else.
     *
     * @return The text
     */
    public String getText ()
    {
        if (this.mode == Mode.TEXT)
            return this.getLines ();
        return this.writer.isEmpty () ? "" : this.conversions.messagesToText (this.writer.segment (), this.writer.size ());
    }


    // ------------------------------------------------------------------------------------------
    // The values

    /**
     * Report a flag.
     *
     * @param key The key (see {@link Protocol#key})
     * @param value The value
     */
    public void put (final long key, final boolean value)
    {
        this.put4 (key, 0, value);
    }


    /**
     * Report a flag of a key with a fourth index slot.
     *
     * @param key The key
     * @param i3 The fourth index slot
     * @param value The value
     */
    public void put4 (final long key, final int i3, final boolean value)
    {
        final long raw = value ? 1 : 0;
        int slot = this.find (key, i3);
        if (this.isUnchanged (slot, TAG_BOOL, raw))
            return;
        slot = this.store (slot, key, i3, TAG_BOOL, raw);
        if (this.mode == Mode.TEXT)
        {
            this.line (slot, value ? "1" : "0");
            return;
        }
        final long start = this.writer.size ();
        switch (Protocol.msgType (Protocol.keyId (key)))
        {
            case Protocol.T_BOOL -> this.writer.putBool (key, i3, value);
            case Protocol.T_I32 -> this.writer.putInt (key, i3, value ? 1 : 0);
            case Protocol.T_F64 -> this.writer.putDouble (key, i3, value ? 1 : 0);
            case Protocol.T_NONE -> this.writer.putNone (key, i3);
            default -> this.convert (slot, value ? "1" : "0");
        }
        if (this.mode == Mode.CHECK)
            this.check (slot, start, value ? "1" : "0");
    }


    /**
     * Report an integer.
     *
     * @param key The key
     * @param value The value
     */
    public void put (final long key, final int value)
    {
        this.put4 (key, 0, value);
    }


    /**
     * Report an integer of a key with a fourth index slot.
     *
     * @param key The key
     * @param i3 The fourth index slot
     * @param value The value
     */
    public void put4 (final long key, final int i3, final int value)
    {
        int slot = this.find (key, i3);
        if (this.isUnchanged (slot, TAG_INT, value))
            return;
        slot = this.store (slot, key, i3, TAG_INT, value);
        if (this.mode == Mode.TEXT)
        {
            this.line (slot, Integer.toString (value));
            return;
        }
        final long start = this.writer.size ();
        switch (Protocol.msgType (Protocol.keyId (key)))
        {
            case Protocol.T_I32 -> this.writer.putInt (key, i3, value);
            case Protocol.T_BOOL -> this.writer.putBool (key, i3, value > 0);
            case Protocol.T_F64 -> this.writer.putDouble (key, i3, value);
            case Protocol.T_NONE -> this.writer.putNone (key, i3);
            default -> this.convert (slot, Integer.toString (value));
        }
        if (this.mode == Mode.CHECK)
            this.check (slot, start, Integer.toString (value));
    }


    /**
     * Report a double.
     *
     * @param key The key
     * @param value The value
     */
    public void put (final long key, final double value)
    {
        this.put4 (key, 0, value);
    }


    /**
     * Report a double of a key with a fourth index slot.
     *
     * @param key The key
     * @param i3 The fourth index slot
     * @param value The value
     */
    public void put4 (final long key, final int i3, final double value)
    {
        final long raw = Double.doubleToRawLongBits (value);
        int slot = this.find (key, i3);
        if (this.mode == Mode.TEXT)
        {
            // As the text protocol: a change of the text
            final String text = formatDouble (value);
            if (slot >= 0 && !this.dumpAll && this.states[slot] == REPORTED && this.tags[slot] == TAG_DOUBLE && text.equals (this.objects[slot]))
                return;
            slot = this.store (slot, key, i3, TAG_DOUBLE, raw);
            this.objects[slot] = text;
            this.line (slot, text);
            return;
        }
        if (this.isUnchanged (slot, TAG_DOUBLE, raw))
            return;
        slot = this.store (slot, key, i3, TAG_DOUBLE, raw);
        final long start = this.writer.size ();
        switch (Protocol.msgType (Protocol.keyId (key)))
        {
            case Protocol.T_F64 -> this.writer.putDouble (key, i3, round6 (value));
            case Protocol.T_NONE -> this.writer.putNone (key, i3);
            default -> this.convert (slot, formatDouble (value));
        }
        if (this.mode == Mode.CHECK)
            this.check (slot, start, formatDouble (value));
    }


    /**
     * Report a text; line breaks are replaced by spaces. The texts of the channel types and of
     * the record quantization are written as their numbers.
     *
     * @param key The key
     * @param value The value
     */
    public void put (final long key, final String value)
    {
        this.put4 (key, 0, value);
    }


    /**
     * Report a text of a key with a fourth index slot.
     *
     * @param key The key
     * @param i3 The fourth index slot
     * @param value The value
     */
    public void put4 (final long key, final int i3, final String value)
    {
        final String text = value == null ? "" : value.replace ('\n', ' ').replace ('\r', ' ');
        int slot = this.find (key, i3);
        if (slot >= 0 && !this.dumpAll && this.states[slot] == REPORTED && this.tags[slot] == TAG_TEXT && text.equals (this.objects[slot]))
            return;
        slot = this.store (slot, key, i3, TAG_TEXT, 0);
        this.objects[slot] = text;
        if (this.mode == Mode.TEXT)
        {
            this.line (slot, text);
            return;
        }
        final long start = this.writer.size ();
        final int id = Protocol.keyId (key);
        switch (Protocol.msgType (id))
        {
            case Protocol.T_TEXT -> this.writer.putText (key, i3, text);
            case Protocol.T_NONE -> this.writer.putNone (key, i3);
            case Protocol.T_I32 ->
            {
                final int number = enumNumber (id, text.trim ());
                if (number >= 0)
                    this.writer.putInt (key, i3, number);
                else
                    this.convert (slot, text);
            }
            default -> this.convert (slot, text);
        }
        if (this.mode == Mode.CHECK)
            this.check (slot, start, text);
    }


    /**
     * Report a color.
     *
     * @param key The key
     * @param red The red part 0..1
     * @param green The green part 0..1
     * @param blue The blue part 0..1
     */
    public void putColor (final long key, final double red, final double green, final double blue)
    {
        final long r = Math.round (red * 255);
        final long g = Math.round (green * 255);
        final long b = Math.round (blue * 255);
        final long raw = (r & 0x1FFFFF) << 42 | (g & 0x1FFFFF) << 21 | b & 0x1FFFFF;
        int slot = this.find (key, 0);
        if (this.isUnchanged (slot, TAG_COLOR, raw))
            return;
        slot = this.store (slot, key, 0, TAG_COLOR, raw);
        final String text = this.mode == Mode.BINARY ? null : r + " " + g + " " + b;
        if (this.mode == Mode.TEXT)
        {
            this.line (slot, text);
            return;
        }
        final long start = this.writer.size ();
        if (Protocol.msgType (Protocol.keyId (key)) == Protocol.T_COLOR)
            this.writer.putColor (key, 0, clampColor (r), clampColor (g), clampColor (b), r >= 0);
        else
            this.convert (slot, r + " " + g + " " + b);
        if (this.mode == Mode.CHECK)
            this.check (slot, start, text);
    }


    /**
     * Report the playing notes of a channel (in the text form "0:0:0:0:0:pitch:velocity;…", no
     * start, end, channel and flags).
     *
     * @param key The key
     * @param notes The pitch and the velocity of each note, in pairs
     * @param numNotes The number of notes
     */
    public void putPlayingNotes (final long key, final int [] notes, final int numNotes)
    {
        int slot = this.find (key, 0);
        if (slot >= 0 && !this.dumpAll && this.states[slot] == REPORTED && this.tags[slot] == TAG_NOTES && this.objects[slot] instanceof final int [] last && Arrays.equals (last, 0, last.length, notes, 0, 2 * numNotes))
            return;
        slot = this.store (slot, key, 0, TAG_NOTES, 0);
        this.objects[slot] = Arrays.copyOf (notes, 2 * numNotes);
        String text = null;
        if (this.mode != Mode.BINARY)
        {
            final StringBuilder builder = new StringBuilder ();
            for (int i = 0; i < numNotes; i++)
            {
                if (i > 0)
                    builder.append (';');
                builder.append ("0:0:0:0:0:").append (notes[2 * i]).append (':').append (notes[2 * i + 1]);
            }
            text = builder.toString ();
        }
        if (this.mode == Mode.TEXT)
        {
            this.line (slot, text);
            return;
        }
        final long start = this.writer.size ();
        final double [] zeros = new double [numNotes];
        final int [] none = new int [numNotes];
        final int [] pitches = new int [numNotes];
        final int [] velocities = new int [numNotes];
        for (int i = 0; i < numNotes; i++)
        {
            pitches[i] = notes[2 * i];
            velocities[i] = notes[2 * i + 1];
        }
        this.writer.putNotes (key, 0, numNotes, zeros, zeros, none, pitches, velocities, none);
        if (this.mode == Mode.CHECK)
            this.check (slot, start, text);
    }


    /**
     * Report a step of a step grid: the 24 values of the core's StepInfo (the ints as doubles),
     * null for an empty step.
     *
     * @param key The key
     * @param i3 The fourth index slot (the step)
     * @param values The values or null
     */
    public void putStep (final long key, final int i3, final double [] values)
    {
        int slot = this.find (key, i3);
        if (slot >= 0 && !this.dumpAll && this.states[slot] == REPORTED && this.tags[slot] == TAG_STEP && Arrays.equals ((double []) this.objects[slot], values))
            return;
        slot = this.store (slot, key, i3, TAG_STEP, 0);
        this.objects[slot] = values == null ? null : values.clone ();
        final String text = this.mode == Mode.BINARY ? null : stepText (values);
        if (this.mode == Mode.TEXT)
        {
            this.line (slot, text);
            return;
        }
        final long start = this.writer.size ();
        this.writer.putDoubles (key, i3, values == null ? new double [0] : values);
        if (this.mode == Mode.CHECK)
            this.check (slot, start, text);
    }


    /**
     * Report a displayed value and its texts shortened by Bitwig (the slot "limit" of the ID: 0,
     * 6 and 8).
     *
     * @param key The key (the slot "limit" 0)
     * @param text The full text
     * @param limited6 The text shortened to 6 characters
     * @param limited8 The text shortened to 8 characters
     */
    public void putDisplayed (final long key, final String text, final String limited6, final String limited8)
    {
        final int limitSlot = LIMIT_SLOTS[Protocol.keyId (key)];
        if (limitSlot == 3)
        {
            this.put4 (key, 0, text);
            this.put4 (key, 6, limited6);
            this.put4 (key, 8, limited8);
            return;
        }
        final int shift = 16 * (limitSlot + 1);
        this.put (key, text);
        this.put (key | 6L << shift, limited6);
        this.put (key | 8L << shift, limited8);
    }


    // ------------------------------------------------------------------------------------------
    // The conversions

    /**
     * Round a double as the text protocol's "%.6f" does (TYPED_PROTOCOL_PLAN §5.1, the transition
     * until step 5): the double which the text gives.
     *
     * @param value The value
     * @return The rounded value
     */
    static double round6 (final double value)
    {
        // A value with at most 6 decimals is its own text
        final double scaled = value * 1e6;
        if (scaled == Math.rint (scaled) && Math.abs (scaled) < 1e15 && scaled / 1e6 == value)
            return value;
        return Double.parseDouble (formatDouble (value));
    }


    /**
     * The text of a double in the text protocol.
     *
     * @param value The value
     * @return The text
     */
    static String formatDouble (final double value)
    {
        return String.format (Locale.ROOT, "%.6f", Double.valueOf (value));
    }


    private static int clampColor (final long value)
    {
        return (int) Math.clamp (value, 0, 255);
    }


    /** The number of the text of a channel type or record quantization, -1 if unknown. */
    private static int enumNumber (final int id, final String text)
    {
        if (id == Protocol.Msg.CHANNEL_TYPE)
            return Arrays.asList (Protocol.ChannelType.TEXTS).indexOf (text);
        if (id == Protocol.Msg.RECORD_QUANTIZATION)
        {
            final int number = Arrays.asList (Protocol.RecordQuantization.TEXTS).indexOf (text);
            return number < 0 ? Protocol.RecordQuantization.DEFAULT : number;
        }
        return -1;
    }


    /** The text form of a step (as StepInfo::to_wire of the core), "0" for an empty step. */
    private static String stepText (final double [] values)
    {
        if (values == null)
            return "0";
        final StringBuilder text = new StringBuilder ();
        for (int i = 0; i < values.length; i++)
        {
            if (i > 0)
                text.append (' ');
            if (i < STEP_INTS.length && STEP_INTS[i])
                text.append ((int) values[i]);
            else
                text.append (Double.toString (values[i]));
        }
        return text.toString ();
    }


    /** The record of a value which has no direct form: the core's conversion of its text line. */
    private void convert (final int slot, final String text)
    {
        final byte [] buffer = this.conversions.messagesFromText (this.address (slot) + " " + text);
        if (buffer.length > Protocol.HEADER_SIZE)
            this.writer.putRecords (Arrays.copyOfRange (buffer, Protocol.HEADER_SIZE, buffer.length));
    }


    /** The dual run: the written record must equal the core's conversion of the text line. */
    private void check (final int slot, final long start, final String text)
    {
        this.checkedRecords++;
        final String line = this.address (slot) + " " + text;
        final byte [] expected = this.conversions.messagesFromText (line);
        final byte [] actual = this.writer.buffer ().asSlice (start).toArray (java.lang.foreign.ValueLayout.JAVA_BYTE);
        final byte [] expectedRecords = expected.length > Protocol.HEADER_SIZE ? Arrays.copyOfRange (expected, Protocol.HEADER_SIZE, expected.length) : new byte [0];
        if (Arrays.equals (actual, expectedRecords))
            return;
        this.checkErrors++;
        if (this.checkErrors <= 100)
            this.conversions.log ("Protocol check: the record of '" + line + "' differs from the text: " + this.recordText (actual) + " != " + this.recordText (expectedRecords));
    }


    private String recordText (final byte [] records)
    {
        if (records.length == 0)
            return "(none)";
        final Protocol.Writer w = new Protocol.Writer (records.length + Protocol.HEADER_SIZE);
        w.putRecords (records);
        return "'" + this.conversions.messagesToText (w.segment (), w.size ()) + "'";
    }


    /** The address of the key of an entry (the core's text form), resolved once. */
    private String address (final int slot)
    {
        String address = this.addresses[slot];
        if (address != null)
            return address;
        final long key = this.keys[slot];
        final int i3 = this.slots3[slot] & 0xFFFF;
        final int type = Protocol.msgType (Protocol.keyId (key));
        final Protocol.Writer w = new Protocol.Writer (64);
        switch (type)
        {
            case Protocol.T_BOOL, Protocol.T_I32, Protocol.T_COLOR -> w.putLong (key, i3, type, 0);
            case Protocol.T_F64 -> w.putDouble (key, i3, 0);
            case Protocol.T_TEXT -> w.putText (key, i3, "");
            case Protocol.T_STRUCT -> w.putDoubles (key, i3, new double [0]);
            case Protocol.T_STRUCT_ARRAY -> w.putPayload (key, i3, Protocol.T_STRUCT_ARRAY, Protocol.OP_SET, 0, new byte [0]);
            default -> w.putNone (key, i3);
        }
        final String text = this.conversions.messagesToText (w.segment (), w.size ());
        final int space = text.indexOf (' ');
        address = space < 0 ? text : text.substring (0, space);
        this.addresses[slot] = address;
        return address;
    }


    private void line (final int slot, final String text)
    {
        this.lines.append (this.address (slot)).append (' ').append (text).append ('\n');
    }


    // ------------------------------------------------------------------------------------------
    // The table of the last values

    private boolean isUnchanged (final int slot, final byte tag, final long raw)
    {
        return slot >= 0 && !this.dumpAll && this.states[slot] == REPORTED && this.tags[slot] == tag && this.raws[slot] == raw;
    }


    private void allocate (final int capacity)
    {
        this.keys = new long [capacity];
        this.slots3 = new short [capacity];
        this.states = new byte [capacity];
        this.tags = new byte [capacity];
        this.raws = new long [capacity];
        this.objects = new Object [capacity];
        this.addresses = new String [capacity];
        this.count = 0;
    }


    private static int hash (final long key, final int i3)
    {
        long h = key * 0x9E3779B97F4A7C15L ^ (i3 & 0xFFFFL) * 0xC2B2AE3D27D4EB4FL;
        h ^= h >>> 29;
        h *= 0xBF58476D1CE4E5B9L;
        return (int) (h ^ h >>> 32);
    }


    /** The entry of a key, or -(the free entry where it would go) - 1. */
    private int find (final long key, final int i3)
    {
        final int mask = this.keys.length - 1;
        final short s3 = (short) i3;
        int slot = hash (key, i3) & mask;
        while (true)
        {
            if (this.states[slot] == FREE)
                return -slot - 1;
            if (this.keys[slot] == key && this.slots3[slot] == s3)
                return slot;
            slot = slot + 1 & mask;
        }
    }


    /** Store a value as reported; returns its entry. */
    private int store (final int found, final long key, final int i3, final byte tag, final long raw)
    {
        int slot = found;
        if (slot < 0)
        {
            if (2 * (this.count + 1) > this.keys.length)
            {
                this.grow ();
                slot = this.find (key, i3);
            }
            slot = -slot - 1;
            this.keys[slot] = key;
            this.slots3[slot] = (short) i3;
            this.count++;
        }
        this.states[slot] = REPORTED;
        this.tags[slot] = tag;
        this.raws[slot] = raw;
        return slot;
    }


    private void grow ()
    {
        final long [] oldKeys = this.keys;
        final short [] oldSlots3 = this.slots3;
        final byte [] oldStates = this.states;
        final byte [] oldTags = this.tags;
        final long [] oldRaws = this.raws;
        final Object [] oldObjects = this.objects;
        final String [] oldAddresses = this.addresses;
        this.allocate (2 * oldKeys.length);
        for (int i = 0; i < oldKeys.length; i++)
        {
            if (oldStates[i] == FREE)
                continue;
            final int slot = -this.find (oldKeys[i], oldSlots3[i] & 0xFFFF) - 1;
            this.keys[slot] = oldKeys[i];
            this.slots3[slot] = oldSlots3[i];
            this.states[slot] = oldStates[i];
            this.tags[slot] = oldTags[i];
            this.raws[slot] = oldRaws[i];
            this.objects[slot] = oldObjects[i];
            this.addresses[slot] = oldAddresses[i];
            this.count++;
        }
    }
}
