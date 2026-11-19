// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.api.MemoryBlock;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.HardwareDevice;
import com.bitwig.extension.controller.api.UsbDevice;
import com.bitwig.extension.controller.api.UsbOutputPipe;
import com.bitwig.extension.controller.api.UsbPipe;

import java.io.IOException;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.SocketChannel;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BiConsumer;


/**
 * The USB bulk writes of the core (e.g. the frames of the Push 2/3 display) through Bitwig's USB
 * API, as the Java version's PushUsbDisplay: the definition claims the devices
 * ({@link DbmExtensionDefinition#listHardwareDevices}), here {@code host.hardwareDevice (i)} is
 * device index i, its pipe 0 of interface 0 the endpoint (one interface with one endpoint per
 * device). The core's callback only copies the data into the slot of its device (a frame which
 * was not written yet is replaced: dropped, never queued up); a thread of its own writes it with
 * {@code UsbOutputPipe.write}. A Push frame is written as the Java version does: the 16-byte
 * header, then the image.
 *
 * <p>
 * With the environment variable {@code DBM_PUSH_DISPLAY_SOCKET} (a UNIX socket path, as for the
 * Python hosts) the data goes to that socket instead, e.g. to the Push simulator
 * (support/push_simulator): the exact bytes of the bulk writes (a Push frame: header and image),
 * back to back, without the device; the definition then claims no USB device. Without a listener
 * the frames are dropped and the connection is tried again at most once a second.
 */
public final class UsbOutput
{
    /** The environment variable of the socket (the contract with the Push simulator). */
    public static final String   SOCKET_VARIABLE = "DBM_PUSH_DISPLAY_SOCKET";

    /** The timeout of one write in milliseconds (as PushUsbDisplay). */
    private static final int     TIMEOUT         = 1000;
    /** The pause between two connection attempts to the socket in milliseconds. */
    private static final long    RECONNECT_PAUSE = 1000;
    private static final int     HEADER_SIZE     = 16;
    private static final byte [] MAGIC           = { (byte) 0xFF, (byte) 0xCC, (byte) 0xAA, (byte) 0x88 };

    private final ControllerHost host;
    private final Device []      devices;
    private final ExecutorService executor = Executors.newSingleThreadExecutor (runnable -> {
        final Thread thread = new Thread (runnable, "DrivenByMoss USB");
        thread.setDaemon (true);
        return thread;
    });
    private final BiConsumer<String, Throwable> errors;
    private volatile boolean     isShutdown;


    /** Where the data of a device goes. */
    private interface Target
    {
        /**
         * Write the data of one bulk write.
         *
         * @param data The data
         * @throws IOException A socket error
         */
        void write (byte [] data) throws IOException;


        /** Release the target (exit). */
        default void close ()
        {
            // Nothing to release
        }
    }


    /** A claimed device: where its data goes and the frame to write. */
    private final class Device
    {
        private final Target                   target;
        private final AtomicReference<byte []> pending = new AtomicReference<> ();
        private boolean                        hasLoggedError;


        Device (final Target target)
        {
            this.target = target;
        }


        void write ()
        {
            final byte [] data = this.pending.getAndSet (null);
            if (data == null || UsbOutput.this.isShutdown)
                return;
            try
            {
                this.target.write (data);
                this.hasLoggedError = false;
            }
            catch (final IOException | RuntimeException ex)
            {
                // Bitwig throws its internal exceptions as RuntimeException (e.g. unplugged)
                if (!this.hasLoggedError)
                {
                    this.hasLoggedError = true;
                    UsbOutput.this.errors.accept ("Could not send USB data.", ex);
                }
            }
        }
    }


    /** The bulk endpoint of a device claimed through Bitwig. */
    private final class PipeTarget implements Target
    {
        private final UsbOutputPipe pipe;
        private MemoryBlock         header;
        private MemoryBlock         body;


        PipeTarget (final UsbOutputPipe pipe)
        {
            this.pipe = pipe;
        }


        @Override
        public void write (final byte [] data)
        {
            int offset = 0;
            if (data.length > HEADER_SIZE && startsWithMagic (data))
            {
                this.header = this.block (this.header, HEADER_SIZE);
                this.header.createByteBuffer ().put (data, 0, HEADER_SIZE);
                this.pipe.write (this.header, TIMEOUT);
                offset = HEADER_SIZE;
            }
            final int size = data.length - offset;
            this.body = this.block (this.body, size);
            this.body.createByteBuffer ().put (data, offset, size);
            this.pipe.write (this.body, TIMEOUT);
        }


        private MemoryBlock block (final MemoryBlock block, final int size)
        {
            return block != null && block.size () == size ? block : UsbOutput.this.host.allocateMemoryBlock (size);
        }
    }


    /**
     * The UNIX socket of DBM_PUSH_DISPLAY_SOCKET: the exact bytes, back to back. Connects on the
     * first write; after an error (no listener, the simulator quit) it drops the frames and tries
     * again at most once a second, without logging each attempt.
     */
    private static final class SocketTarget implements Target
    {
        private final UnixDomainSocketAddress address;
        private SocketChannel                 channel;
        private long                          nextAttempt;


        SocketTarget (final String path)
        {
            this.address = UnixDomainSocketAddress.of (path);
        }


        @Override
        public void write (final byte [] data) throws IOException
        {
            if (this.channel == null)
            {
                final long now = System.currentTimeMillis ();
                if (now < this.nextAttempt)
                    return;
                this.nextAttempt = now + RECONNECT_PAUSE;
                try
                {
                    this.channel = SocketChannel.open (StandardProtocolFamily.UNIX);
                    this.channel.connect (this.address);
                }
                catch (final IOException ex)
                {
                    this.close ();
                    // No listener yet: not an error worth a log line per second
                    return;
                }
            }
            try
            {
                final ByteBuffer buffer = ByteBuffer.wrap (data);
                while (buffer.hasRemaining ())
                    this.channel.write (buffer);
            }
            catch (final IOException ex)
            {
                this.close ();
                this.nextAttempt = System.currentTimeMillis () + RECONNECT_PAUSE;
                throw ex;
            }
        }


        @Override
        public void close ()
        {
            if (this.channel != null)
            {
                try
                {
                    this.channel.close ();
                }
                catch (final IOException ex)
                {
                    // Closed anyway
                }
                this.channel = null;
            }
        }
    }


    /**
     * The socket path of {@code DBM_PUSH_DISPLAY_SOCKET}, if set.
     *
     * @return The path or null
     */
    public static String socketPath ()
    {
        final String path = System.getenv (SOCKET_VARIABLE);
        return path == null || path.isBlank () ? null : path;
    }


    /**
     * Claim the pipes of the devices of the definition (or connect them to the socket of
     * DBM_PUSH_DISPLAY_SOCKET). Call it during init.
     *
     * @param host The host
     * @param numDevices The number of USB devices of the definition
     * @param errors Logs an error
     */
    public UsbOutput (final ControllerHost host, final int numDevices, final BiConsumer<String, Throwable> errors)
    {
        this.host = host;
        this.errors = errors;
        this.devices = new Device [numDevices];

        final String socket = socketPath ();
        if (socket != null)
        {
            host.println ("USB data to the socket " + socket + " (" + SOCKET_VARIABLE + "), not to the device.");
            for (int i = 0; i < numDevices; i++)
                this.devices[i] = new Device (new SocketTarget (socket));
            return;
        }

        for (int i = 0; i < numDevices; i++)
        {
            try
            {
                final HardwareDevice hardwareDevice = host.hardwareDevice (i);
                if (hardwareDevice instanceof final UsbDevice usbDevice)
                {
                    final UsbPipe pipe = usbDevice.iface (0).pipe (0);
                    if (pipe instanceof final UsbOutputPipe outputPipe)
                        this.devices[i] = new Device (new PipeTarget (outputPipe));
                    else
                        errors.accept ("The USB endpoint of device " + i + " is no output.", null);
                }
                else
                    errors.accept ("USB device " + i + " is not available.", null);
            }
            catch (final RuntimeException ex)
            {
                errors.accept ("Could not open USB device " + i + ".", ex);
            }
        }
    }


    /**
     * Queue a write of the core (the callback of dbm_set_usb_callback): copies the data, a thread
     * writes it.
     *
     * @param deviceIndex The index of the device
     * @param data The data, only valid during the call
     * @return True if the data was queued
     */
    public boolean write (final int deviceIndex, final MemorySegment data)
    {
        if (this.isShutdown || deviceIndex < 0 || deviceIndex >= this.devices.length || this.devices[deviceIndex] == null)
            return false;
        final Device device = this.devices[deviceIndex];
        // Only one write task per device is queued: a newer frame replaces the pending one
        if (device.pending.getAndSet (data.toArray (ValueLayout.JAVA_BYTE)) == null)
        {
            try
            {
                this.executor.execute (device::write);
            }
            catch (final RuntimeException ex)
            {
                return false;
            }
        }
        return true;
    }


    /**
     * Stop the writes (exit). Waits a moment for the running write, then releases the targets.
     */
    public void shutdown ()
    {
        this.isShutdown = true;
        this.executor.shutdown ();
        try
        {
            this.executor.awaitTermination (2 * TIMEOUT, TimeUnit.MILLISECONDS);
        }
        catch (final InterruptedException ex)
        {
            Thread.currentThread ().interrupt ();
        }
        for (final Device device: this.devices)
            if (device != null)
                device.target.close ();
    }


    private static boolean startsWithMagic (final byte [] data)
    {
        for (int i = 0; i < MAGIC.length; i++)
            if (data[i] != MAGIC[i])
                return false;
        return true;
    }

}
