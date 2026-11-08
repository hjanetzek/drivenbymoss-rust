// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import java.lang.foreign.MemorySegment;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;


/**
 * Collects the reports for the core (doc/TYPED_PROTOCOL_PLAN.md §2.4): the values of the model by
 * key (the ID of the report and its index slots, see {@link Protocol#key}), written as binary
 * records into one buffer per report for dbm_update_model_bin. Remembers the last value of each
 * key and only reports changes, unless a full dump was requested (the core's refresh command).
 * An unchanged value costs one lookup in an open addressing table and a compare.
 *
 * Doubles are reported exactly (no rounding, TYPED_PROTOCOL_PLAN §6.6); a changed double is always
 * reported. A value whose type does not fit the report is logged once per ID and not reported.
 */
public final class ModelReporter
{
    /** The conversions of the core (the text of the reports for the recordings and logs). */
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

    /** The position of the slot "limit" by report ID (2 or 3, -1 if none). */
    private static final int []        LIMIT_SLOTS   = new int [0x1000];
    static
    {
        for (int id = 0; id < LIMIT_SLOTS.length; id++)
            LIMIT_SLOTS[id] = Arrays.asList (Protocol.msgSlots (id).split (",")).indexOf ("limit");
    }

    private final Conversions          conversions;
    private final Protocol.Writer      writer        = new Protocol.Writer (16384);
    private boolean                    dumpAll       = true;
    /** The things to report again and when (System.nanoTime), see forgetLater. */
    private final Map<Entity, Long>    pending       = new HashMap<> ();
    /** The report IDs logged with a value of a type which does not fit. */
    private final boolean []           wrongTypes    = new boolean [0x1000];

    // The last values: an open addressing table (linear probing) over the key and the fourth
    // index slot; entries are never removed (the keys of a model are a bounded set)
    private long []                    keys;
    private short []                   slots3;
    private byte []                    states;
    private byte []                    tags;
    private long []                    raws;
    /** The texts, notes and steps. */
    private Object []                  objects;
    private int                        count;


    /**
     * Constructor.
     *
     * @param conversions The conversions of the core
     */
    public ModelReporter (final Conversions conversions)
    {
        this.conversions = conversions;
        this.allocate (4096);
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
     * End a collection: the report is in {@link #getWriter()} until the next collection.
     */
    public void finishCollection ()
    {
        this.dumpAll = false;
    }


    /**
     * The records of the last collection.
     *
     * @return The writer
     */
    public Protocol.Writer getWriter ()
    {
        return this.writer;
    }


    /**
     * The text of the last collection (for the recordings and logs): the core's text of the
     * records.
     *
     * @return The text
     */
    public String getText ()
    {
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
        final int slot = this.find (key, i3);
        if (this.isUnchanged (slot, TAG_BOOL, raw))
            return;
        this.store (slot, key, i3, TAG_BOOL, raw);
        switch (Protocol.msgType (Protocol.keyId (key)))
        {
            case Protocol.T_BOOL -> this.writer.putBool (key, i3, value);
            case Protocol.T_I32 -> this.writer.putInt (key, i3, value ? 1 : 0);
            case Protocol.T_F64 -> this.writer.putDouble (key, i3, value ? 1 : 0);
            case Protocol.T_NONE -> this.writer.putNone (key, i3);
            default -> this.wrongType (key, "a flag");
        }
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
        final int slot = this.find (key, i3);
        if (this.isUnchanged (slot, TAG_INT, value))
            return;
        this.store (slot, key, i3, TAG_INT, value);
        switch (Protocol.msgType (Protocol.keyId (key)))
        {
            case Protocol.T_I32 -> this.writer.putInt (key, i3, value);
            case Protocol.T_BOOL -> this.writer.putBool (key, i3, value > 0);
            case Protocol.T_F64 -> this.writer.putDouble (key, i3, value);
            case Protocol.T_NONE -> this.writer.putNone (key, i3);
            default -> this.wrongType (key, "an integer");
        }
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
        final int slot = this.find (key, i3);
        if (this.isUnchanged (slot, TAG_DOUBLE, raw))
            return;
        this.store (slot, key, i3, TAG_DOUBLE, raw);
        switch (Protocol.msgType (Protocol.keyId (key)))
        {
            case Protocol.T_F64 -> this.writer.putDouble (key, i3, value);
            case Protocol.T_NONE -> this.writer.putNone (key, i3);
            default -> this.wrongType (key, "a double");
        }
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
                    this.wrongType (key, "the text '" + text + "'");
            }
            default -> this.wrongType (key, "a text");
        }
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
        final int slot = this.find (key, 0);
        if (this.isUnchanged (slot, TAG_COLOR, raw))
            return;
        this.store (slot, key, 0, TAG_COLOR, raw);
        if (Protocol.msgType (Protocol.keyId (key)) == Protocol.T_COLOR)
            this.writer.putColor (key, 0, clampColor (r), clampColor (g), clampColor (b), r >= 0);
        else
            this.wrongType (key, "a color");
    }


    /**
     * Report the playing notes of a channel (pitch and velocity; no start, end, channel and
     * flags).
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
        this.writer.putDoubles (key, i3, values == null ? new double [0] : values);
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
    // Helpers

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


    /** A value of a type which does not fit the report: logged once per ID. */
    private void wrongType (final long key, final String what)
    {
        final int id = Protocol.keyId (key);
        if (this.wrongTypes[id])
            return;
        this.wrongTypes[id] = true;
        if (this.conversions != null)
            this.conversions.log ("The report " + Protocol.msgName (id) + " does not take " + what + ".");
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
            this.count++;
        }
    }
}
