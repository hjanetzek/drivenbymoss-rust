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


/**
 * A device found by a matcher on the cursor track (the drum machine, the first instrument), with
 * its parameters, drum pads and layers, as SpecificDeviceImpl, DrumPadBankImpl and LayerBankImpl
 * of the Java version. The pads and layers are reported completely, the core pages over them.
 */
final class SpecificDevice
{
    /** The address prefix, e.g. "/drum". */
    private final String                     prefix;
    final Device                             device;
    /** One remote control page cursor per page, each fixed to its page; may be empty. */
    final CursorRemoteControlsPage []        pages;
    /** For the page names. Null without parameters. */
    private final CursorRemoteControlsPage   remoteControls;
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
     * @param prefix The address prefix of the reported values
     * @param numParamPages The number of parameter pages, 0 for none
     * @param numPads The number of drum pads
     * @param numLayers The number of layers, 0 for none
     */
    SpecificDevice (final Device device, final String prefix, final int numParamPages, final int numPads, final int numLayers)
    {
        this.device = device;
        this.prefix = prefix;

        this.pages = new CursorRemoteControlsPage [numParamPages];
        for (int i = 0; i < numParamPages; i++)
            this.pages[i] = device.createCursorRemoteControlsPage (prefix + " page " + (i + 1), BitwigModel.PARAMS_PER_PAGE, "");
        this.remoteControls = numParamPages > 0 ? device.createCursorRemoteControlsPage (BitwigModel.PARAMS_PER_PAGE) : null;

        this.numPads = numPads;
        this.drumPadBank = device.createDrumPadBank (numPads);
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
        this.device.hasDrumPads ().markInterested ();
        this.device.hasLayers ().markInterested ();

        for (final CursorRemoteControlsPage page: this.pages)
            BitwigModel.markPage (page);
        if (this.remoteControls != null)
        {
            this.remoteControls.pageCount ().markInterested ();
            this.remoteControls.pageNames ().markInterested ();
        }

        this.drumPadBank.hasMutedPads ().markInterested ();
        this.drumPadBank.hasSoloedPads ().markInterested ();
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
        reporter.put (this.prefix + "/exists", exists);
        reporter.put (this.prefix + "/name", exists ? this.device.name ().get () : "");
        reporter.put (this.prefix + "/bypass", exists && !this.device.isEnabled ().get ());
        reporter.put (this.prefix + "/window", exists && this.device.isWindowOpen ().get ());
        reporter.put (this.prefix + "/expand", exists && this.device.isExpanded ().get ());
        reporter.put (this.prefix + "/hasDrumPads", exists && this.device.hasDrumPads ().get ());
        reporter.put (this.prefix + "/hasLayers", exists && this.device.hasLayers ().get ());

        if (this.remoteControls != null)
        {
            BitwigModel.collectPages (reporter, this.prefix, this.pages);
            final String [] pageNames = this.remoteControls.pageNames ().get ();
            final int namedPages = Math.min (pageNames.length, this.pages.length);
            for (int p = 0; p < namedPages; p++)
                reporter.put (this.prefix + "/page/" + p + "/name", pageNames[p]);
            reporter.put (this.prefix + "/page/count", namedPages);
        }

        reporter.put (this.prefix + "/hasMutedPads", exists && this.drumPadBank.hasMutedPads ().get ());
        reporter.put (this.prefix + "/hasSoloedPads", exists && this.drumPadBank.hasSoloedPads ().get ());
        for (int i = 0; i < this.numPads; i++)
            this.collectItem (reporter, this.prefix + "/pad/" + i + "/", this.drumPadBank.getItemAt (i), exists && this.padFirstDevices[i].exists ().get (), this.padSelected[i]);

        if (this.layerBank == null)
            return;
        reporter.put (this.prefix + "/layer/count", exists ? Math.min (this.layerBank.itemCount ().get (), this.numLayers) : 0);
        for (int i = 0; i < this.numLayers; i++)
            this.collectItem (reporter, this.prefix + "/layer/" + i + "/", this.layerBank.getItemAt (i), exists && this.layerFirstDevices[i].exists ().get (), this.layerSelected[i]);
    }


    private void collectItem (final ModelReporter reporter, final String itemPrefix, final Channel channel, final boolean exists, final boolean selected)
    {
        reporter.put (itemPrefix + "exists", exists);
        reporter.put (itemPrefix + "select", selected);
        reporter.put (itemPrefix + "active", channel.isActivated ().get ());
        BitwigModel.collectChannel (reporter, itemPrefix, channel);
    }
}
