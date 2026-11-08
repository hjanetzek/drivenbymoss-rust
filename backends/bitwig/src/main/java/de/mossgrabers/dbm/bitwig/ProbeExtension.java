// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.ControllerExtension;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.PlayingNote;

import java.io.IOException;
import java.lang.foreign.MemorySegment;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.SocketException;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.stream.Collectors;


/**
 * The probe of the conformance tests (doc/CONFORMANCE_TESTING_PLAN.md): reports what the OSC
 * protocol of the Java DrivenByMoss (the main observer) does not, as text lines over UDP,
 * independent of the controller extension under test: the steps of a fixed grid of the cursor
 * clip ("/clip/grid/64x128/step/...", the addresses of doc/BACKEND_PROTOCOL.md), the notes which
 * play on the cursor track ("/probe/playingNotes") and the Bitwig version ("/probe/version").
 *
 * <p>
 * The lines go to 127.0.0.1:9100 (DBM_PROBE_PORT), only changes after the first dump. A datagram
 * "dump" to port 9101 (DBM_PROBE_PORT + 1) requests all values again.
 */
public class ProbeExtension extends ControllerExtension
{
    /** The model setup: only the step grid of the cursor clip, the rest comes from OSC. */
    private static final String MODEL_SETUP     = "{\"numTracks\":1,\"numScenes\":1,\"numSends\":0,\"wantsDrumDevice\":false,\"numParamPages\":1,\"numResults\":0,\"deviceIds\":[],\"noteClips\":[[64,128]]}";
    private static final int    TICK_INTERVAL   = 30;
    /** Keep the datagrams below the usual limit of the loopback interface. */
    private static final int    MAX_PACKET_SIZE = 60000;

    private BitwigModel         model;
    private ModelReporter       reporter;
    private DatagramSocket      socket;
    private DatagramSocket      requests;
    private InetAddress         address;
    private int                 port;
    private volatile boolean    dumpRequested   = true;
    private volatile boolean    isRunning;
    /** The last values of the probe's own lines. */
    private final Map<String, String> probeValues = new HashMap<> ();


    /**
     * Constructor.
     *
     * @param definition The definition
     * @param host The host
     */
    protected ProbeExtension (final ProbeExtensionDefinition definition, final ControllerHost host)
    {
        super (definition, host);
    }


    @Override
    public void init ()
    {
        final ControllerHost host = this.getHost ();
        this.model = new BitwigModel (host, MODEL_SETUP, null);
        // The records of the clip and the grid, formatted as text lines by the probe itself (it
        // has no core): probeLines
        this.reporter = new ModelReporter (new ModelReporter.Conversions ()
        {
            @Override
            public String messagesToText (final MemorySegment buffer, final long size)
            {
                return probeLines (buffer.asSlice (0, size), null, true);
            }


            @Override
            public void log (final String message)
            {
                host.errorln (message);
            }
        });
        // A fixed grid: 64 steps of 1/16 from the start of the clip, all keys
        final BitwigNoteClip noteClip = this.model.noteClips.get (0);
        noteClip.clip.setStepSize (0.25);
        noteClip.clip.scrollToStep (0);
        this.model.cursorTrack.playingNotes ().markInterested ();

        final String portText = System.getenv ("DBM_PROBE_PORT");
        this.port = portText == null || portText.isBlank () ? 9100 : Integer.parseInt (portText.trim ());
        try
        {
            this.address = InetAddress.getLoopbackAddress ();
            this.socket = new DatagramSocket ();
            this.requests = new DatagramSocket (this.port + 1, this.address);
        }
        catch (final SocketException ex)
        {
            host.errorln ("Probe: could not open the UDP ports: " + ex.getMessage ());
            return;
        }

        this.isRunning = true;
        final Thread listener = new Thread (this::listen, "Conformance probe requests");
        listener.setDaemon (true);
        listener.start ();
        host.println ("Conformance probe: reporting to UDP " + this.port + ", requests on " + (this.port + 1));
        host.scheduleTask (this::tick, TICK_INTERVAL);
    }


    @Override
    public void exit ()
    {
        this.isRunning = false;
        if (this.requests != null)
            this.requests.close ();
        if (this.socket != null)
            this.socket.close ();
    }


    @Override
    public void flush ()
    {
        // Everything is sent from the timer
    }


    private void listen ()
    {
        final byte [] buffer = new byte [256];
        while (this.isRunning)
        {
            try
            {
                final DatagramPacket packet = new DatagramPacket (buffer, buffer.length);
                this.requests.receive (packet);
                if ("dump".equals (new String (packet.getData (), 0, packet.getLength (), StandardCharsets.UTF_8).trim ()))
                    this.dumpRequested = true;
            }
            catch (final IOException ex)
            {
                // Closed on exit
            }
        }
    }


    private void tick ()
    {
        if (!this.isRunning)
            return;
        final boolean dump = this.dumpRequested;
        if (dump)
        {
            this.dumpRequested = false;
            this.reporter.requestDump ();
        }
        this.reporter.startCollection ();
        final BitwigNoteClip noteClip = this.model.noteClips.get (0);
        noteClip.collectClip (this.reporter);
        noteClip.collectGrid (this.reporter);
        this.reporter.finishCollection ();
        final Protocol.Writer writer = this.reporter.getWriter ();
        final StringBuilder lines = new StringBuilder (writer.isEmpty () ? "" : probeLines (writer.segment ().asSlice (0, writer.size ()), this.probeValues, dump));
        final PlayingNote [] notes = this.model.cursorTrack.playingNotes ().get ();
        this.putProbe (lines, "/probe/version", this.getHost ().getHostProduct () + " " + this.getHost ().getHostVersion (), dump);
        this.putProbe (lines, "/probe/playingNotes", Arrays.stream (notes).sorted (Comparator.comparingInt (PlayingNote::pitch)).map (n -> n.pitch () + ":" + n.velocity ()).collect (Collectors.joining (" ")), dump);
        this.send (lines.toString ());
        this.getHost ().scheduleTask (this::tick, TICK_INTERVAL);
    }


    /** A line of the probe itself, if the value changed (or with a dump). */
    private void putProbe (final StringBuilder lines, final String address, final String value, final boolean dump)
    {
        if (!dump && value.equals (this.probeValues.get (address)))
            return;
        this.probeValues.put (address, value);
        lines.append (address).append (' ').append (value).append ('\n');
    }


    /**
     * The text lines of the records of a buffer: the clip properties and the grid of the cursor
     * clip as the text protocol had them (the addresses of doc/BACKEND_PROTOCOL.md, doubles with
     * 6 decimals, the steps as StepInfo::to_wire), so that the lines stay those of the Java
     * reference runs. The probe has no core, so it formats them itself.
     *
     * @param buffer The buffer (with the header)
     * @param last The last text of each address: only the lines whose text changed (unless
     *            dump), null for all
     * @param dump All lines
     * @return The lines
     */
    static String probeLines (final MemorySegment buffer, final Map<String, String> last, final boolean dump)
    {
        final StringBuilder lines = new StringBuilder ();
        final Protocol.View view = new Protocol.View ();
        if (!view.reset (buffer))
            return "";
        while (view.next ())
        {
            final String address = probeAddress (view);
            final String value = probeValue (view);
            if (last != null && !dump && value.equals (last.get (address)))
                continue;
            if (last != null)
                last.put (address, value);
            lines.append (address).append (' ').append (value).append ('\n');
        }
        return lines.toString ();
    }


    /** The address of a record (the core's text form of the clip and the grid). */
    private static String probeAddress (final Protocol.View view)
    {
        final int id = view.id ();
        if (id == Protocol.Msg.GRID_PLAYING_STEP)
            return "/clip/grid/" + view.idx (0) + "x" + view.idx (1) + "/playingStep";
        if (id == Protocol.Msg.GRID_STEP)
            return "/clip/grid/" + view.idx (0) + "x" + view.idx (1) + "/step/" + (view.idx (2) >> 7) + "/" + view.idx (3) + "/" + (view.idx (2) & 127);
        return Protocol.msgTemplate (id);
    }


    /** The value of a record as text: flags 1/0, doubles with 6 decimals, colors "r g b", steps. */
    private static String probeValue (final Protocol.View view)
    {
        return switch (view.type ())
        {
            case Protocol.T_BOOL -> view.boolValue () ? "1" : "0";
            case Protocol.T_I32 -> Integer.toString (view.intValue ());
            case Protocol.T_F64 -> String.format (Locale.ROOT, "%.6f", Double.valueOf (view.doubleValue ()));
            case Protocol.T_TEXT -> view.text ();
            case Protocol.T_COLOR -> view.colorPart (0) + " " + view.colorPart (1) + " " + view.colorPart (2);
            case Protocol.T_STRUCT -> stepText (view);
            default -> "";
        };
    }


    /** The step values which are ints in the text form (StepInfo::to_wire). */
    private static final boolean [] STEP_INTS = new boolean [24];
    static
    {
        for (final int i: new int [] { 0, 1, 2, 12, 14, 15, 16, 17, 18, 19, 20 })
            STEP_INTS[i] = true;
    }


    /** The text form of a step (as StepInfo::to_wire of the core), "0" for an empty step. */
    private static String stepText (final Protocol.View view)
    {
        final int count = view.payloadSize () / 8;
        if (count == 0)
            return "0";
        final StringBuilder text = new StringBuilder ();
        for (int i = 0; i < count; i++)
        {
            if (i > 0)
                text.append (' ');
            final double value = view.payloadDouble (8 * i);
            if (i < STEP_INTS.length && STEP_INTS[i])
                text.append ((int) value);
            else
                text.append (Double.toString (value));
        }
        return text.toString ();
    }


    /** Send the lines in datagrams which end at a line break. */
    private void send (final String lines)
    {
        int start = 0;
        while (start < lines.length ())
        {
            int end = Math.min (lines.length (), start + MAX_PACKET_SIZE);
            if (end < lines.length ())
            {
                final int lineEnd = lines.lastIndexOf ('\n', end - 1);
                if (lineEnd > start)
                    end = lineEnd + 1;
            }
            final byte [] data = lines.substring (start, end).getBytes (StandardCharsets.UTF_8);
            try
            {
                this.socket.send (new DatagramPacket (data, data.length, this.address, this.port));
            }
            catch (final IOException ex)
            {
                // Nobody listens, the runner is not running
                return;
            }
            start = end;
        }
    }
}
