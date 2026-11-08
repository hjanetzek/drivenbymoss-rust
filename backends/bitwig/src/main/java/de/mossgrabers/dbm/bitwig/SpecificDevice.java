// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Channel;
import com.bitwig.extension.controller.api.CursorRemoteControlsPage;
import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.DeviceLayer;
import com.bitwig.extension.controller.api.DeviceLayerBank;
import com.bitwig.extension.controller.api.DrumPad;
import com.bitwig.extension.controller.api.DrumPadBank;

import de.mossgrabers.dbm.bitwig.Protocol.Msg;


/**
 * A device found by a matcher on the cursor track (the drum machine, the first instrument), with
 * its parameters, drum pads and layers, as SpecificDeviceImpl, DrumPadBankImpl and LayerBankImpl
 * of the Java version. The pads and layers are reported completely, the core pages over them.
 */
final class SpecificDevice
{
    /** The drum machine (its own report IDs), see the constructor. */
    static final int                         DRUM = -1;

    /** The device: {@link #DRUM}, Protocol.Dev.DEVICE (the layers of the cursor device) or PRIMARY. */
    private final int                        dev;
    final Device                             device;
    /** One remote control page cursor per page, each fixed to its page; may be empty. */
    final CursorRemoteControlsPage []        pages;
    /** For the page names and the page selected in Bitwig. Null without parameters. */
    final CursorRemoteControlsPage           remoteControls;
    final DrumPadBank                        drumPadBank;
    final int                                numPads;
    /** The first device of each pad: a pad exists if it has a device (LayerImpl). */
    private final Device []                  padFirstDevices;
    private final boolean []                 padSelected;
    /** Null if no layers are needed. */
    final DeviceLayerBank                    layerBank;
    final int                                numLayers;
    /** The first device of each layer: a layer exists if it has a device (LayerImpl). */
    private final Device []                  layerFirstDevices;
    private final boolean []                 layerSelected;


    /**
     * Create all Bitwig objects; must be called during init.
     *
     * @param device The matched device
     * @param dev The device: {@link #DRUM}, Protocol.Dev.DEVICE or Protocol.Dev.PRIMARY
     * @param numParamPages The number of parameter pages, 0 for none
     * @param numPads The number of drum pads
     * @param numLayers The number of layers, 0 for none
     */
    SpecificDevice (final Device device, final int dev, final int numParamPages, final int numPads, final int numLayers)
    {
        this.device = device;
        this.dev = dev;
        final String prefix = switch (dev)
        {
            case DRUM -> "/drum";
            case Protocol.Dev.PRIMARY -> "/primary";
            default -> "/device";
        };

        this.pages = new CursorRemoteControlsPage [numParamPages];
        for (int i = 0; i < numParamPages; i++)
            this.pages[i] = device.createCursorRemoteControlsPage (prefix + " page " + (i + 1), BitwigModel.PARAMS_PER_PAGE, "");
        this.remoteControls = numParamPages > 0 ? device.createCursorRemoteControlsPage (BitwigModel.PARAMS_PER_PAGE) : null;

        this.numPads = numPads;
        // No pads for the layers of the cursor device (see BitwigModel)
        this.drumPadBank = numPads > 0 ? device.createDrumPadBank (numPads) : null;
        this.padFirstDevices = new Device [numPads];
        this.padSelected = new boolean [numPads];
        for (int i = 0; i < numPads; i++)
            this.padFirstDevices[i] = this.drumPadBank.getItemAt (i).createDeviceBank (1).getItemAt (0);

        this.numLayers = numLayers;
        this.layerBank = numLayers > 0 ? device.createLayerBank (numLayers) : null;
        this.layerFirstDevices = new Device [numLayers];
        this.layerSelected = new boolean [numLayers];
        for (int i = 0; i < numLayers; i++)
            this.layerFirstDevices[i] = this.layerBank.getItemAt (i).createDeviceBank (1).getItemAt (0);

        this.markInterested ();
    }


    private void markInterested ()
    {
        this.device.exists ().markInterested ();
        this.device.name ().markInterested ();
        this.device.isEnabled ().markInterested ();
        this.device.isWindowOpen ().markInterested ();
        this.device.isExpanded ().markInterested ();
        this.device.isRemoteControlsSectionVisible ().markInterested ();
        this.device.hasDrumPads ().markInterested ();
        this.device.hasLayers ().markInterested ();

        for (final CursorRemoteControlsPage page: this.pages)
            BitwigModel.markPage (page);
        if (this.remoteControls != null)
        {
            this.remoteControls.pageCount ().markInterested ();
            this.remoteControls.pageNames ().markInterested ();
            this.remoteControls.selectedPageIndex ().markInterested ();
        }

        if (this.drumPadBank != null)
        {
            this.drumPadBank.hasMutedPads ().markInterested ();
            this.drumPadBank.hasSoloedPads ().markInterested ();
        }
        for (int i = 0; i < this.numPads; i++)
        {
            final DrumPad pad = this.drumPadBank.getItemAt (i);
            final int index = i;
            BitwigModel.markChannel (pad);
            pad.isActivated ().markInterested ();
            pad.addIsSelectedInEditorObserver (selected -> this.padSelected[index] = selected);
            this.padFirstDevices[i].exists ().markInterested ();
        }

        for (int i = 0; i < this.numLayers; i++)
        {
            final DeviceLayer layer = this.layerBank.getItemAt (i);
            final int index = i;
            BitwigModel.markChannel (layer);
            layer.isActivated ().markInterested ();
            layer.addIsSelectedInEditorObserver (selected -> this.layerSelected[index] = selected);
            this.layerFirstDevices[i].exists ().markInterested ();
        }
        if (this.layerBank != null)
            this.layerBank.itemCount ().markInterested ();
    }


    /**
     * Report the current state.
     *
     * @param reporter Where to report to
     */
    void collect (final ModelReporter reporter)
    {
        final boolean exists = this.device.exists ().get ();
        final boolean isDrum = this.dev == DRUM;
        // The drum machine has its own IDs without an index, the others the IDs of /device with
        // the Dev index
        final long base = isDrum ? 0 : Protocol.key (0, this.dev, 0, 0);
        reporter.put ((isDrum ? Msg.DRUM_EXISTS : Msg.DEVICE_EXISTS) | base, exists);
        reporter.put ((isDrum ? Msg.DRUM_NAME : Msg.DEVICE_NAME) | base, exists ? this.device.name ().get () : "");
        reporter.put ((isDrum ? Msg.DRUM_BYPASS : Msg.DEVICE_BYPASS) | base, exists && !this.device.isEnabled ().get ());
        reporter.put ((isDrum ? Msg.DRUM_WINDOW : Msg.DEVICE_WINDOW) | base, exists && this.device.isWindowOpen ().get ());
        reporter.put ((isDrum ? Msg.DRUM_EXPAND : Msg.DEVICE_EXPAND) | base, exists && this.device.isExpanded ().get ());
        reporter.put ((isDrum ? Msg.DRUM_PARAMETERS : Msg.DEVICE_PARAMETERS) | base, exists && this.device.isRemoteControlsSectionVisible ().get ());
        reporter.put ((isDrum ? Msg.DRUM_HAS_DRUM_PADS : Msg.DEVICE_HAS_DRUM_PADS) | base, exists && this.device.hasDrumPads ().get ());
        reporter.put ((isDrum ? Msg.DRUM_HAS_LAYERS : Msg.DEVICE_HAS_LAYERS) | base, exists && this.device.hasLayers ().get ());

        if (this.remoteControls != null)
        {
            // The owners of parameters and pages have the numbers of Dev
            BitwigModel.collectPages (reporter, this.dev, this.pages);
            BitwigModel.collectPageNames (reporter, this.dev, this.remoteControls, this.pages.length);
        }

        final int padDev = isDrum ? Protocol.Dev.DEVICE : this.dev;
        if (this.drumPadBank != null)
        {
            reporter.put ((isDrum ? Msg.DRUM_HAS_MUTED_PADS : Msg.DEVICE_HAS_MUTED_PADS) | base, exists && this.drumPadBank.hasMutedPads ().get ());
            reporter.put ((isDrum ? Msg.DRUM_HAS_SOLOED_PADS : Msg.DEVICE_HAS_SOLOED_PADS) | base, exists && this.drumPadBank.hasSoloedPads ().get ());
        }
        final long padBase = Protocol.key (0, isDrum ? Protocol.PadKind.DRUM_PAD : Protocol.PadKind.PAD, padDev, 0);
        for (int i = 0; i < this.numPads; i++)
            collectItem (reporter, padBase | (long) i << 48, this.drumPadBank.getItemAt (i), exists && this.padFirstDevices[i].exists ().get (), this.padSelected[i]);

        if (this.layerBank == null)
            return;
        reporter.put (Msg.DEVICE_LAYER_COUNT | base, exists ? Math.min (this.layerBank.itemCount ().get (), this.numLayers) : 0);
        final long layerBase = Protocol.key (0, Protocol.PadKind.LAYER, padDev, 0);
        for (int i = 0; i < this.numLayers; i++)
            collectItem (reporter, layerBase | (long) i << 48, this.layerBank.getItemAt (i), exists && this.layerFirstDevices[i].exists ().get (), this.layerSelected[i]);
    }


    /**
     * A drum pad or layer: exists, selected, the channel values (the texts not shortened, the
     * core does not read them for layers and pads).
     */
    private static void collectItem (final ModelReporter reporter, final long base, final Channel channel, final boolean exists, final boolean selected)
    {
        reporter.put (Msg.PAD_EXISTS | base, exists);
        reporter.put (Msg.PAD_SELECTED | base, selected);
        reporter.put (Msg.PAD_NAME | base, channel.name ().get ());
        reporter.putColor (Msg.PAD_COLOR | base, channel.color ().red (), channel.color ().green (), channel.color ().blue ());
        reporter.put (Msg.PAD_MUTE | base, channel.mute ().get ());
        reporter.put (Msg.PAD_SOLO | base, channel.solo ().get ());
        reporter.put (Msg.PAD_ACTIVE | base, channel.isActivated ().get ());
        reporter.put (Msg.PAD_VOLUME | base, channel.volume ().value ().get ());
        reporter.put (Msg.PAD_VOLUME_STR | base, channel.volume ().displayedValue ().get ());
        reporter.put (Msg.PAD_PAN | base, channel.pan ().value ().get ());
        reporter.put (Msg.PAD_PAN_STR | base, channel.pan ().displayedValue ().get ());
    }
}
