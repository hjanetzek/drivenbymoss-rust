// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.api.MemoryBlock;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.HardwareDevice;
import com.bitwig.extension.controller.api.UsbDevice;
import com.bitwig.extension.controller.api.UsbOutputPipe;
import com.bitwig.extension.controller.api.UsbPipe;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
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
 */
public final class UsbOutput
{
    /** The timeout of one write in milliseconds (as PushUsbDisplay). */
    private static final int     TIMEOUT     = 1000;
    private static final int     HEADER_SIZE = 16;
    private static final byte [] MAGIC       = { (byte) 0xFF, (byte) 0xCC, (byte) 0xAA, (byte) 0x88 };

    private final ControllerHost host;
    private final Device []      devices;
    private final ExecutorService executor = Executors.newSingleThreadExecutor (runnable -> {
        final Thread thread = new Thread (runnable, "DrivenByMoss USB");
        thread.setDaemon (true);
        return thread;
    });
    private final BiConsumer<String, Throwable> errors;
    private volatile boolean     isShutdown;


    /** A claimed device: its pipe, the frame to write and the memory blocks of the writes. */
    private final class Device
    {
        private final UsbOutputPipe           pipe;
        private final AtomicReference<byte []> pending = new AtomicReference<> ();
        private MemoryBlock                   header;
        private MemoryBlock                   body;
        private boolean                       hasLoggedError;


        Device (final UsbOutputPipe pipe)
        {
            this.pipe = pipe;
        }


        void write ()
        {
            final byte [] data = this.pending.getAndSet (null);
            if (data == null || UsbOutput.this.isShutdown)
                return;
            try
            {
                int offset = 0;
                if (data.length > HEADER_SIZE && startsWithMagic (data))
                {
                    this.header = block (this.header, HEADER_SIZE);
                    this.header.createByteBuffer ().put (data, 0, HEADER_SIZE);
                    this.pipe.write (this.header, TIMEOUT);
                    offset = HEADER_SIZE;
                }
                final int size = data.length - offset;
                this.body = block (this.body, size);
                this.body.createByteBuffer ().put (data, offset, size);
                this.pipe.write (this.body, TIMEOUT);
                this.hasLoggedError = false;
            }
            catch (final RuntimeException ex)
            {
                // Bitwig throws its internal exceptions as RuntimeException (e.g. unplugged)
                if (!this.hasLoggedError)
                {
                    this.hasLoggedError = true;
                    UsbOutput.this.errors.accept ("Could not send USB data.", ex);
                }
            }
        }


        private MemoryBlock block (final MemoryBlock block, final int size)
        {
            return block != null && block.size () == size ? block : UsbOutput.this.host.allocateMemoryBlock (size);
        }
    }


    /**
     * Claim the pipes of the devices of the definition. Call it during init.
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
        for (int i = 0; i < numDevices; i++)
        {
            try
            {
                final HardwareDevice hardwareDevice = host.hardwareDevice (i);
                if (hardwareDevice instanceof final UsbDevice usbDevice)
                {
                    final UsbPipe pipe = usbDevice.iface (0).pipe (0);
                    if (pipe instanceof final UsbOutputPipe outputPipe)
                        this.devices[i] = new Device (outputPipe);
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
     * Stop the writes (exit). Waits a moment for the running write.
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
    }


    private static boolean startsWithMagic (final byte [] data)
    {
        for (int i = 0; i < MAGIC.length; i++)
            if (data[i] != MAGIC[i])
                return false;
        return true;
    }

}
