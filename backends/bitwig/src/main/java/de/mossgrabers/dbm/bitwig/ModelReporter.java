// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import java.util.HashMap;
import java.util.Locale;
import java.util.Map;


/**
 * Collects the model update lines for the core ("address value", see doc/BACKEND_PROTOCOL.md
 * §3). Remembers the last value of each address and only reports changes, unless a full dump
 * was requested (the core's refresh command).
 */
public final class ModelReporter
{
    private final Map<String, String> lastValues = new HashMap<> ();
    private final StringBuilder       lines      = new StringBuilder ();
    private boolean                   dumpAll    = true;
    /** Prefixes to report again and when (System.nanoTime), see forgetLater. */
    private final Map<String, Long>   pending    = new HashMap<> ();


    /**
     * Report all values with the next collection, not only the changed ones.
     */
    public void requestDump ()
    {
        this.dumpAll = true;
    }


    /**
     * Report the values under the address prefix again with the next collection, even if they
     * did not change. Used after a command which sets a value: the core changes its value at once
     * (as for Reaper), and if Bitwig does not take the new value (e.g. because a control has to
     * pick up the current value first) nothing would correct it.
     *
     * @param prefix The address prefix, e.g. "/track/3/"
     */
    public void forget (final String prefix)
    {
        this.lastValues.keySet ().removeIf (address -> address.startsWith (prefix));
    }


    /**
     * As forget, but only after the delay, counted from the last call for the prefix: while a
     * knob is turned Bitwig might not have applied the last value yet, and reporting the old one
     * would set the core back.
     *
     * @param prefix The address prefix
     * @param delayMs The delay in milliseconds
     */
    public void forgetLater (final String prefix, final int delayMs)
    {
        this.pending.put (prefix, Long.valueOf (System.nanoTime () + delayMs * 1_000_000L));
    }


    /**
     * Forget the prefixes whose delay is over (call before collecting).
     */
    public void applyPendingForgets ()
    {
        if (this.pending.isEmpty ())
            return;
        final long now = System.nanoTime ();
        final var iterator = this.pending.entrySet ().iterator ();
        while (iterator.hasNext ())
        {
            final var entry = iterator.next ();
            if (entry.getValue ().longValue () - now <= 0)
            {
                this.forget (entry.getKey ());
                iterator.remove ();
            }
        }
    }


    /**
     * Get the collected lines and start a new collection.
     *
     * @return The lines, empty if nothing changed
     */
    public String take ()
    {
        final String result = this.lines.toString ();
        this.lines.setLength (0);
        this.dumpAll = false;
        return result;
    }


    /**
     * Report a text value.
     *
     * @param address The address, e.g. "/track/0/name"
     * @param value The value, line breaks are replaced by spaces
     */
    public void put (final String address, final String value)
    {
        final String text = value == null ? "" : value.replace ('\n', ' ').replace ('\r', ' ');
        final String last = this.lastValues.put (address, text);
        if (this.dumpAll || !text.equals (last))
            this.lines.append (address).append (' ').append (text).append ('\n');
    }


    /**
     * Report an integer value.
     *
     * @param address The address
     * @param value The value
     */
    public void put (final String address, final int value)
    {
        this.put (address, Integer.toString (value));
    }


    /**
     * Report a boolean value as 1 or 0.
     *
     * @param address The address
     * @param value The value
     */
    public void put (final String address, final boolean value)
    {
        this.put (address, value ? "1" : "0");
    }


    /**
     * Report a floating point value.
     *
     * @param address The address
     * @param value The value
     */
    public void put (final String address, final double value)
    {
        this.put (address, String.format (Locale.ROOT, "%.6f", Double.valueOf (value)));
    }


    /**
     * Report a color as "red green blue" (0..255).
     *
     * @param address The address
     * @param red The red part 0..1
     * @param green The green part 0..1
     * @param blue The blue part 0..1
     */
    public void putColor (final String address, final double red, final double green, final double blue)
    {
        this.put (address, Math.round (red * 255) + " " + Math.round (green * 255) + " " + Math.round (blue * 255));
    }
}
