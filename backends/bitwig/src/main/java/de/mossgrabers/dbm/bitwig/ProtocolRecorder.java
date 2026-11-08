// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import java.io.BufferedWriter;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;
import java.util.HexFormat;
import java.util.stream.Stream;


/**
 * Records everything which goes into and out of the core, for the replay tests of the
 * conformance tests (doc/CONFORMANCE_TESTING_PLAN.md §5): the model lines, ticks, MIDI in and
 * out, the commands and the note input tables, with the milliseconds (3 decimals) since the
 * start of the controller.
 *
 * <p>
 * Enabled with DBM_RECORD_DIR, or in dev mode (DBM_DEV_LIBRARY) into target/conformance/recordings
 * of the crate. A recording starts with the controller. The scenario runner marks a run with a
 * system exclusive message, which is not passed to the core: F0 7D 44 42 4D 01 &lt;name&gt; F7 at the
 * start, F0 7D 44 42 4D 02 &lt;name&gt; F7 at the end; the end saves a copy as &lt;name&gt;.rec.
 *
 * <p>
 * Format: one event per line, "&lt;ms&gt;\t&lt;kind&gt;\t&lt;field&gt;...", fields with "\\", "\n" and
 * "\t" escaped. Kinds: init (core ID, application version), config (file name, content),
 * documentSettings (older recordings), setting (ID, value from Bitwig), file (path, content: a
 * properties file which a setting names, e.g. the mapping of Generic Flexi, recorded before the
 * setting so that a replay has it), start, model (lines),
 * tick, in, out (hex bytes), noArg, string, int, double
 * (processor, command, value), strings (processor, command, values...), delay, enable
 * (processor, 0/1), filters, keys, velocities (note input, values...), mark (begin/end, name).
 */
final class ProtocolRecorder
{
    private static final byte [] MARKER = { (byte) 0xF0, 0x7D, 0x44, 0x42, 0x4D };

    private final Path           folder;
    private final Path           file;
    private final BufferedWriter writer;
    private final long           start = System.nanoTime ();


    private ProtocolRecorder (final Path folder, final Path file, final BufferedWriter writer)
    {
        this.folder = folder;
        this.file = file;
        this.writer = writer;
    }


    /**
     * Start a recording if enabled.
     *
     * @param controller The name of the controller (for the file name)
     * @param devLibrary The library of the dev mode or null
     * @return The recorder or null if not enabled or the file could not be created
     */
    static ProtocolRecorder create (final String controller, final Path devLibrary)
    {
        final String dir = System.getenv ("DBM_RECORD_DIR");
        final Path folder;
        if (dir != null && !dir.isBlank ())
            folder = Path.of (dir.trim ());
        else if (devLibrary != null && devLibrary.getParent () != null && devLibrary.getParent ().getParent () != null)
            folder = devLibrary.getParent ().getParent ().resolve ("conformance").resolve ("recordings");
        else
            return null;

        try
        {
            Files.createDirectories (folder);
            final String name = controller.replaceAll ("[^A-Za-z0-9]+", "_") + "-" + LocalDateTime.now ().format (DateTimeFormatter.ofPattern ("yyyyMMdd-HHmmss")) + ".rec";
            final Path file = folder.resolve (name);
            return new ProtocolRecorder (folder, file, Files.newBufferedWriter (file, StandardCharsets.UTF_8));
        }
        catch (final IOException ex)
        {
            return null;
        }
    }


    /**
     * The file of the recording.
     *
     * @return The file
     */
    Path getFile ()
    {
        return this.file;
    }


    /**
     * Record the header: the controller and its configuration files.
     *
     * @param coreId The ID of the controller in the core
     * @param appVersion The application version passed to the core
     * @param configFolder The configuration folder of the core
     */
    void init (final String coreId, final String appVersion, final Path configFolder)
    {
        this.event ("init", coreId, appVersion);
        try (Stream<Path> files = Files.list (configFolder))
        {
            for (final Path config: files.filter (p -> p.toString ().endsWith (".config")).sorted ().toList ())
                this.event ("config", config.getFileName ().toString (), Files.readString (config, StandardCharsets.UTF_8));
        }
        catch (final IOException ex)
        {
            this.event ("error", "Could not read the configuration: " + ex.getMessage ());
        }
    }


    /**
     * Record a setting from Bitwig. If its value is the path of a properties file (the mapping file
     * of Generic Flexi), the content of the file is recorded first: the replay uses it in place of
     * the file, which the next scenario file of the runner overwrites.
     *
     * @param id The ID of the setting
     * @param value The value
     */
    void setting (final String id, final String value)
    {
        if (value.endsWith (".properties"))
        {
            try
            {
                final Path path = Path.of (value);
                if (Files.isRegularFile (path))
                    this.event ("file", value, Files.readString (path, StandardCharsets.UTF_8));
            }
            catch (final IOException | java.nio.file.InvalidPathException ex)
            {
                this.event ("error", "Could not read " + value + ": " + ex.getMessage ());
            }
        }
        this.event ("setting", id, value);
    }


    /**
     * Handle a marker of the scenario runner.
     *
     * @param data The received MIDI data
     * @return True if it was a marker (not to be passed to the core)
     */
    boolean handleMarker (final byte [] data)
    {
        if (data.length < MARKER.length + 2 || data[data.length - 1] != (byte) 0xF7)
            return false;
        for (int i = 0; i < MARKER.length; i++)
            if (data[i] != MARKER[i])
                return false;
        // Only 01 and 02, the other messages with the header are for the core (e.g. the Shift
        // button of the Mk2 from dbm-nihost)
        if (data[MARKER.length] != 1 && data[MARKER.length] != 2)
            return false;
        final boolean isBegin = data[MARKER.length] == 1;
        final String name = new String (data, MARKER.length + 1, data.length - MARKER.length - 2, StandardCharsets.US_ASCII).replaceAll ("[^A-Za-z0-9._-]+", "_");
        this.event ("mark", isBegin ? "begin" : "end", name);
        this.flush ();
        if (!isBegin && !name.isEmpty ())
        {
            try
            {
                Files.copy (this.file, this.folder.resolve (name + ".rec"), StandardCopyOption.REPLACE_EXISTING);
            }
            catch (final IOException ex)
            {
                // The recording itself is still complete
            }
        }
        return true;
    }


    void midi (final String kind, final byte [] data)
    {
        this.event (kind, HexFormat.ofDelimiter (" ").withUpperCase ().formatHex (data));
    }


    void ints (final String kind, final int noteInput, final int [] values)
    {
        final String [] fields = new String [values.length + 1];
        fields[0] = Integer.toString (noteInput);
        for (int i = 0; i < values.length; i++)
            fields[i + 1] = Integer.toString (values[i]);
        this.event (kind, fields);
    }


    /**
     * Record an event: the time, the kind and the fields (escaped).
     *
     * @param kind The kind
     * @param fields The fields
     */
    synchronized void event (final String kind, final String... fields)
    {
        final StringBuilder line = new StringBuilder (kind);
        for (final String field: fields)
            line.append ('\t').append (escape (field));
        this.line (line.toString ());
    }


    /**
     * Record a line which is already in the form of the recordings: the kind and the escaped
     * fields, separated by tabs (e.g. a command line of dbm_messages_to_text).
     *
     * @param kindAndFields The kind and the fields
     */
    synchronized void line (final String kindAndFields)
    {
        final StringBuilder line = new StringBuilder ();
        // Milliseconds with 3 decimals: a replay at whole milliseconds can end a timed state (e.g.
        // a notification) one tick earlier than the run
        final long micros = (System.nanoTime () - this.start) / 1_000L;
        line.append (micros / 1000L).append ('.').append (String.format ("%03d", Long.valueOf (micros % 1000L))).append ('\t').append (kindAndFields);
        try
        {
            this.writer.write (line.append ('\n').toString ());
        }
        catch (final IOException ex)
        {
            // Recording is best effort
        }
    }


    synchronized void flush ()
    {
        try
        {
            this.writer.flush ();
        }
        catch (final IOException ex)
        {
            // Recording is best effort
        }
    }


    synchronized void close ()
    {
        try
        {
            this.writer.close ();
        }
        catch (final IOException ex)
        {
            // Recording is best effort
        }
    }


    private static String escape (final String text)
    {
        if (text == null)
            return "";
        return text.replace ("\\", "\\\\").replace ("\n", "\\n").replace ("\t", "\\t").replace ("\r", "");
    }
}
