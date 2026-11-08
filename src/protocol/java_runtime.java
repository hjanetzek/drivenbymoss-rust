    // ------------------------------------------------------------------------------------------
    // The runtime (src/protocol/java_runtime.java, the same for every table)

    /** Little-endian layouts without alignment (the records are 8-byte aligned, the fields not all). */
    public static final ValueLayout.OfShort  LE_SHORT  = ValueLayout.JAVA_SHORT_UNALIGNED.withOrder (ByteOrder.LITTLE_ENDIAN);
    public static final ValueLayout.OfInt    LE_INT    = ValueLayout.JAVA_INT_UNALIGNED.withOrder (ByteOrder.LITTLE_ENDIAN);
    public static final ValueLayout.OfLong   LE_LONG   = ValueLayout.JAVA_LONG_UNALIGNED.withOrder (ByteOrder.LITTLE_ENDIAN);
    public static final ValueLayout.OfDouble LE_DOUBLE = ValueLayout.JAVA_DOUBLE_UNALIGNED.withOrder (ByteOrder.LITTLE_ENDIAN);

    /** The magic of the header ("DBM1" as a little-endian int). */
    public static final int MAGIC = 0x314D4244;


    /**
     * The key of a report for the change tracking (TYPED_PROTOCOL_PLAN §2.4): the ID and the
     * first three index slots. The fourth slot (only a few IDs use it) is kept next to the key.
     *
     * @param id The ID
     * @param i0 The first index slot
     * @param i1 The second index slot
     * @param i2 The third index slot
     * @return The key
     */
    public static long key (final int id, final int i0, final int i1, final int i2)
    {
        return (id & 0xFFFFL) | (i0 & 0xFFFFL) << 16 | (i1 & 0xFFFFL) << 32 | (i2 & 0xFFFFL) << 48;
    }


    /**
     * The ID of a key.
     *
     * @param key The key
     * @return The ID
     */
    public static int keyId (final long key)
    {
        return (int) (key & 0xFFFF);
    }


    /**
     * An index slot of a key.
     *
     * @param key The key
     * @param slot The slot 0..2
     * @return The index (unsigned)
     */
    public static int keyIndex (final long key, final int slot)
    {
        return (int) (key >>> 16 * (slot + 1) & 0xFFFF);
    }


    /**
     * The value type of a report ID.
     *
     * @param id The ID
     * @return The type (T_*), -1 for an unknown ID
     */
    public static int msgType (final int id)
    {
        return (id & 0x8000) == 0 && (id & 0xFFFF) < MSG_TYPES.length ? MSG_TYPES[id] : -1;
    }


    /**
     * The index slots of a report ID: the names of the parts, the slots separated by commas, the
     * parts of a packed slot by '|' (e.g. "ch_kind,ch_pos,send,limit").
     *
     * @param id The ID
     * @return The slots, empty for none or an unknown ID
     */
    public static String msgSlots (final int id)
    {
        final String slots = (id & 0x8000) == 0 && (id & 0xFFFF) < MSG_SLOTS.length ? MSG_SLOTS[id] : null;
        return slots == null ? "" : slots;
    }


    /**
     * The name of a report ID (e.g. "ChannelVolume").
     *
     * @param id The ID
     * @return The name, the ID in hex if unknown
     */
    public static String msgName (final int id)
    {
        final String name = (id & 0x8000) == 0 && (id & 0xFFFF) < MSG_NAMES.length ? MSG_NAMES[id] : null;
        return name == null ? String.format ("0x%04X", Integer.valueOf (id)) : name;
    }


    /**
     * The text template of a report ID (e.g. "/{ch}/volume"; the address itself for an ID without
     * index slots).
     *
     * @param id The ID
     * @return The template, empty if unknown
     */
    public static String msgTemplate (final int id)
    {
        final String template = (id & 0x8000) == 0 && (id & 0xFFFF) < MSG_TEMPLATES.length ? MSG_TEMPLATES[id] : null;
        return template == null ? "" : template;
    }


    /**
     * Check the header of a buffer.
     *
     * @param buffer The buffer
     * @return True if it is a buffer of this major version
     */
    public static boolean checkHeader (final MemorySegment buffer)
    {
        return buffer.byteSize () >= HEADER_SIZE && buffer.get (LE_INT, 0) == MAGIC && (buffer.get (LE_SHORT, 4) & 0xFFFF) == VERSION_MAJOR;
    }


    /**
     * The records of a buffer: a flyweight view, moved from record to record with {@link #next()},
     * which reads the fields from the buffer when they are accessed (TYPED_PROTOCOL_PLAN §2.7).
     * The buffer of a command callback belongs to the core and is only valid during the
     * callback: neither the view nor its segment may be kept, the values are copied out
     * (texts become Strings).
     */
    public static class ViewBase
    {
        protected MemorySegment buffer = MemorySegment.NULL;
        protected long          pos;
        private long            nextPos;
        private long            end;


        /**
         * Start reading a buffer.
         *
         * @param buffer The buffer (with the header)
         * @return False if the header is wrong (the view has no records)
         */
        public boolean reset (final MemorySegment buffer)
        {
            this.buffer = buffer;
            this.pos = -1;
            final boolean valid = checkHeader (buffer);
            this.nextPos = HEADER_SIZE;
            this.end = valid ? buffer.byteSize () : 0;
            return valid;
        }


        /**
         * Move to the next record.
         *
         * @return False at the end (or at a truncated record)
         */
        public boolean next ()
        {
            if (this.nextPos + RECORD_SIZE > this.end)
                return false;
            final long length = this.buffer.get (LE_INT, this.nextPos + 12) & 0xFFFFFFFFL;
            if (this.nextPos + RECORD_SIZE + length > this.end)
                return false;
            this.pos = this.nextPos;
            this.nextPos = this.pos + RECORD_SIZE + (length + 7 & ~7L);
            return true;
        }


        /** @return The position of the record in the buffer */
        public long position ()
        {
            return this.pos;
        }


        /** @return The size of the record with its padded payload */
        public long size ()
        {
            return Math.min (this.nextPos, this.end) - this.pos;
        }


        /** @return The ID */
        public int id ()
        {
            return this.buffer.get (LE_SHORT, this.pos) & 0xFFFF;
        }


        /** @return The value type (T_*) */
        public int type ()
        {
            return this.buffer.get (ValueLayout.JAVA_BYTE, this.pos + 2) & 0xFF;
        }


        /** @return The operation of a value command (OP_*) */
        public int op ()
        {
            return this.buffer.get (ValueLayout.JAVA_BYTE, this.pos + 3) & 0xFF;
        }


        /**
         * An index slot.
         *
         * @param slot The slot 0..3
         * @return The index, unsigned
         */
        public int idx (final int slot)
        {
            return this.buffer.get (LE_SHORT, this.pos + 4 + 2L * slot) & 0xFFFF;
        }


        /**
         * An index slot as a signed index.
         *
         * @param slot The slot 0..3
         * @return The index
         */
        public int signedIdx (final int slot)
        {
            return this.buffer.get (LE_SHORT, this.pos + 4 + 2L * slot);
        }


        /** @return The key of the record (ID and the first three slots, see {@link Protocol#key}) */
        public long key ()
        {
            return (this.buffer.get (LE_LONG, this.pos + 4) << 16) | this.id ();
        }


        /** @return The payload size */
        public int payloadSize ()
        {
            return this.buffer.get (LE_INT, this.pos + 12);
        }


        /** @return The inline value as a long */
        public long i64 ()
        {
            return this.buffer.get (LE_LONG, this.pos + 16);
        }


        /** @return The inline value as a flag */
        public boolean boolValue ()
        {
            return this.i64 () != 0;
        }


        /** @return The inline value as an int */
        public int intValue ()
        {
            return (int) this.i64 ();
        }


        /** @return The inline value as a double */
        public double doubleValue ()
        {
            return this.buffer.get (LE_DOUBLE, this.pos + 16);
        }


        /**
         * The value of a value command (ValueOp): the double of set, inc and immediately, 1 or 0
         * for touch and indicate, 0 for reset.
         *
         * @return The value
         */
        public double opValue ()
        {
            return switch (this.type ())
            {
                case T_F64 -> this.doubleValue ();
                case T_BOOL -> this.boolValue () ? 1 : 0;
                default -> 0;
            };
        }


        /** @return The payload as a text (a copy) */
        public String text ()
        {
            final int length = this.payloadSize ();
            final byte [] bytes = new byte [length];
            MemorySegment.copy (this.buffer, ValueLayout.JAVA_BYTE, this.pos + RECORD_SIZE, bytes, 0, length);
            return new String (bytes, StandardCharsets.UTF_8);
        }


        /** @return The payload as a list of texts (copies) */
        public String [] textList ()
        {
            final int length = this.payloadSize ();
            if (length < 4)
                return new String [0];
            final long start = this.pos + RECORD_SIZE;
            final int count = this.buffer.get (LE_INT, start);
            final java.util.List<String> texts = new java.util.ArrayList<> ();
            long p = 4;
            for (int i = 0; i < count && p + 4 <= length; i++)
            {
                final int size = this.buffer.get (LE_INT, start + p);
                p += 4;
                if (size < 0 || p + size > length)
                    break;
                final byte [] bytes = new byte [size];
                MemorySegment.copy (this.buffer, ValueLayout.JAVA_BYTE, start + p, bytes, 0, size);
                texts.add (new String (bytes, StandardCharsets.UTF_8));
                p += size;
            }
            return texts.toArray (new String [0]);
        }


        /**
         * A part of a color (the inline value: red, green, blue, valid).
         *
         * @param part 0 red, 1 green, 2 blue, 3 valid
         * @return The value 0..255
         */
        public int colorPart (final int part)
        {
            return this.buffer.get (ValueLayout.JAVA_BYTE, this.pos + 16 + part) & 0xFF;
        }


        /**
         * An int of a pair (an inline struct of two i32, e.g. the signature).
         *
         * @param index 0 for the first, 1 for the second
         * @return The value
         */
        public int pairValue (final int index)
        {
            return this.buffer.get (LE_INT, this.pos + 16 + 4L * index);
        }


        /**
         * An int of the payload.
         *
         * @param offset The byte offset in the payload
         * @return The value
         */
        public int payloadInt (final int offset)
        {
            return this.buffer.get (LE_INT, this.pos + RECORD_SIZE + offset);
        }


        /**
         * A double of the payload.
         *
         * @param offset The byte offset in the payload
         * @return The value
         */
        public double payloadDouble (final int offset)
        {
            return this.buffer.get (LE_DOUBLE, this.pos + RECORD_SIZE + offset);
        }


        /**
         * Copy the record (with its padded payload) out of the buffer.
         *
         * @return The bytes
         */
        public byte [] copy ()
        {
            return this.buffer.asSlice (this.pos, this.size ()).toArray (ValueLayout.JAVA_BYTE);
        }


        /** @return The command of the record, null if the ID is not a command of this table */
        public Cmd cmd ()
        {
            return Cmd.of (this.id ());
        }
    }


    /**
     * A buffer of records (one report): the header and the records on a reusable native segment,
     * which grows as needed. {@link #clear()} keeps the memory.
     */
    public static final class Writer
    {
        private final Arena   arena;
        private MemorySegment segment;
        private long          size;


        /**
         * Constructor.
         *
         * @param capacity The initial capacity in bytes
         */
        public Writer (final int capacity)
        {
            this.arena = Arena.ofAuto ();
            this.segment = this.arena.allocate (Math.max (64, capacity), 8);
            this.clear ();
        }


        /** Start a new buffer (the header only). */
        public void clear ()
        {
            this.segment.set (LE_INT, 0, MAGIC);
            this.segment.set (LE_SHORT, 4, (short) VERSION_MAJOR);
            this.segment.set (LE_SHORT, 6, (short) 0);
            this.size = HEADER_SIZE;
        }


        /** @return True if there are no records */
        public boolean isEmpty ()
        {
            return this.size <= HEADER_SIZE;
        }


        /** @return The size of the buffer with the header */
        public long size ()
        {
            return this.size;
        }


        /** @return The segment of the buffer (its size is the capacity, see {@link #size()}) */
        public MemorySegment segment ()
        {
            return this.segment;
        }


        /** @return The used part of the buffer */
        public MemorySegment buffer ()
        {
            return this.segment.asSlice (0, this.size);
        }


        /** @return A copy of the buffer */
        public byte [] toByteArray ()
        {
            return this.buffer ().toArray (ValueLayout.JAVA_BYTE);
        }


        /**
         * Shorten the buffer to a size it had before (drop the records after it).
         *
         * @param newSize The size
         */
        public void truncate (final long newSize)
        {
            if (newSize >= HEADER_SIZE && newSize <= this.size)
                this.size = newSize;
        }


        /**
         * Reserve a record and write its head: the ID and the indices of the key, the fourth
         * index, the type, the operation and the payload size.
         *
         * @return The position of the record
         */
        private long head (final long key, final int i3, final int type, final int op, final int payload)
        {
            final long padded = payload + 7L & ~7L;
            final long p = this.size;
            final long needed = p + RECORD_SIZE + padded;
            if (needed > this.segment.byteSize ())
            {
                final MemorySegment larger = this.arena.allocate (Math.max (needed, 2 * this.segment.byteSize ()), 8);
                MemorySegment.copy (this.segment, 0, larger, 0, p);
                this.segment = larger;
            }
            this.segment.set (LE_SHORT, p, (short) key);
            this.segment.set (ValueLayout.JAVA_BYTE, p + 2, (byte) type);
            this.segment.set (ValueLayout.JAVA_BYTE, p + 3, (byte) op);
            this.segment.set (LE_LONG, p + 4, key >>> 16 | (i3 & 0xFFFFL) << 48);
            this.segment.set (LE_INT, p + 12, payload);
            if (padded > payload)
                this.segment.set (LE_LONG, p + RECORD_SIZE + padded - 8, 0);
            this.size = needed;
            return p;
        }


        /**
         * A record without a value.
         *
         * @param key The key (ID and three slots)
         * @param i3 The fourth slot
         */
        public void putNone (final long key, final int i3)
        {
            this.segment.set (LE_LONG, this.head (key, i3, T_NONE, OP_SET, 0) + 16, 0);
        }


        /**
         * A record with an inline integer value (T_BOOL, T_I32, T_FEATURES).
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param type The value type
         * @param value The value
         */
        public void putLong (final long key, final int i3, final int type, final long value)
        {
            this.segment.set (LE_LONG, this.head (key, i3, type, OP_SET, 0) + 16, value);
        }


        /**
         * A flag.
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param value The value
         */
        public void putBool (final long key, final int i3, final boolean value)
        {
            this.putLong (key, i3, T_BOOL, value ? 1 : 0);
        }


        /**
         * An int.
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param value The value
         */
        public void putInt (final long key, final int i3, final int value)
        {
            this.putLong (key, i3, T_I32, value);
        }


        /**
         * A double.
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param value The value
         */
        public void putDouble (final long key, final int i3, final double value)
        {
            this.segment.set (LE_DOUBLE, this.head (key, i3, T_F64, OP_SET, 0) + 16, value);
        }


        /**
         * A text (UTF-8).
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param value The text
         */
        public void putText (final long key, final int i3, final String value)
        {
            this.putPayload (key, i3, T_TEXT, OP_SET, 0, value == null ? new byte [0] : value.getBytes (StandardCharsets.UTF_8));
        }


        /**
         * A color.
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param red Red 0..255
         * @param green Green 0..255
         * @param blue Blue 0..255
         * @param valid False for "no color"
         */
        public void putColor (final long key, final int i3, final int red, final int green, final int blue, final boolean valid)
        {
            final long value = valid ? (red & 0xFFL) | (green & 0xFFL) << 8 | (blue & 0xFFL) << 16 | 1L << 24 : 0;
            this.putLong (key, i3, T_COLOR, value);
        }


        /**
         * A record with an inline value and an operation (a command).
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param type The value type
         * @param op The operation
         * @param value The 8 bytes of the inline value
         */
        public void putInline (final long key, final int i3, final int type, final int op, final long value)
        {
            this.segment.set (LE_LONG, this.head (key, i3, type, op, 0) + 16, value);
        }


        /**
         * A record with a payload.
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param type The value type
         * @param op The operation
         * @param inline The 8 bytes of the inline value (0 for payload types)
         * @param payload The payload
         */
        public void putPayload (final long key, final int i3, final int type, final int op, final long inline, final byte [] payload)
        {
            final long p = this.head (key, i3, type, op, payload.length);
            this.segment.set (LE_LONG, p + 16, inline);
            MemorySegment.copy (payload, 0, this.segment, ValueLayout.JAVA_BYTE, p + RECORD_SIZE, payload.length);
        }


        /**
         * A struct array of doubles (e.g. the 24 values of a step; empty: no payload).
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param values The values
         */
        public void putDoubles (final long key, final int i3, final double [] values)
        {
            final long p = this.head (key, i3, T_STRUCT, OP_SET, 8 * values.length);
            this.segment.set (LE_LONG, p + 16, 0);
            for (int i = 0; i < values.length; i++)
                this.segment.set (LE_DOUBLE, p + RECORD_SIZE + 8L * i, values[i]);
        }


        /**
         * Notes (a struct array of f64 start, f64 end, i32 channel, i32 pitch, i32 velocity, u32
         * flags: bit 0 selected, bit 1 muted).
         *
         * @param key The key
         * @param i3 The fourth slot
         * @param count The number of notes
         * @param start The starts
         * @param end The ends
         * @param channel The channels
         * @param pitch The pitches
         * @param velocity The velocities
         * @param flags The flags
         */
        public void putNotes (final long key, final int i3, final int count, final double [] start, final double [] end, final int [] channel, final int [] pitch, final int [] velocity, final int [] flags)
        {
            final long p = this.head (key, i3, T_STRUCT_ARRAY, OP_SET, 32 * count);
            this.segment.set (LE_LONG, p + 16, 0);
            for (int i = 0; i < count; i++)
            {
                final long n = p + RECORD_SIZE + 32L * i;
                this.segment.set (LE_DOUBLE, n, start[i]);
                this.segment.set (LE_DOUBLE, n + 8, end[i]);
                this.segment.set (LE_INT, n + 16, channel[i]);
                this.segment.set (LE_INT, n + 20, pitch[i]);
                this.segment.set (LE_INT, n + 24, velocity[i]);
                this.segment.set (LE_INT, n + 28, flags[i]);
            }
        }


        /**
         * Append records (e.g. a record converted by the core).
         *
         * @param records The records without the header
         */
        public void putRecords (final byte [] records)
        {
            final long p = this.size;
            final long needed = p + records.length;
            if (needed > this.segment.byteSize ())
            {
                final MemorySegment larger = this.arena.allocate (Math.max (needed, 2 * this.segment.byteSize ()), 8);
                MemorySegment.copy (this.segment, 0, larger, 0, p);
                this.segment = larger;
            }
            MemorySegment.copy (records, 0, this.segment, ValueLayout.JAVA_BYTE, p, records.length);
            this.size = needed;
        }
    }
