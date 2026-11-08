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
        // The text protocol without the core: the addresses of the clip and the grid
        this.reporter = new ModelReporter (ModelReporter.Mode.TEXT, new ModelReporter.Conversions ()
        {
            @Override
            public String messagesToText (final MemorySegment buffer, final long size)
            {
                return probeAddress (buffer);
            }


            @Override
            public byte [] messagesFromText (final String text)
            {
                return new byte [0];
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
        final StringBuilder lines = new StringBuilder (this.reporter.getLines ());
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
     * The address of the record of a buffer (the clip properties and the grid of the cursor clip,
     * as the core's text form; the probe has no core).
     */
    static String probeAddress (final MemorySegment buffer)
    {
        final Protocol.View view = new Protocol.View ();
        if (!view.reset (buffer) || !view.next ())
            return "";
        final int id = view.id ();
        if (id == Protocol.Msg.GRID_PLAYING_STEP)
            return "/clip/grid/" + view.idx (0) + "x" + view.idx (1) + "/playingStep";
        if (id == Protocol.Msg.GRID_STEP)
            return "/clip/grid/" + view.idx (0) + "x" + view.idx (1) + "/step/" + (view.idx (2) >> 7) + "/" + view.idx (3) + "/" + (view.idx (2) & 127);
        return Protocol.msgTemplate (id);
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
