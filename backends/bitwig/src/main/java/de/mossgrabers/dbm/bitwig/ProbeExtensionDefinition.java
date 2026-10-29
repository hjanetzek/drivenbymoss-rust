// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.api.PlatformType;
import com.bitwig.extension.controller.AutoDetectionMidiPortNamesList;
import com.bitwig.extension.controller.ControllerExtension;
import com.bitwig.extension.controller.ControllerExtensionDefinition;
import com.bitwig.extension.controller.api.ControllerHost;

import java.util.UUID;


/**
 * The definition of the probe of the conformance tests (doc/CONFORMANCE_TESTING_PLAN.md): an
 * extension without a controller, which reports the state of Bitwig to the scenario runner.
 */
public class ProbeExtensionDefinition extends ControllerExtensionDefinition
{
    private static final UUID ID = UUID.fromString ("24ada252-eb6c-5cbd-b26a-e444412dcc77");


    @Override
    public String getName ()
    {
        return "Conformance Probe (Rust)";
    }


    @Override
    public String getAuthor ()
    {
        return "DrivenByMoss (Rust)";
    }


    @Override
    public String getVersion ()
    {
        return "0.1.0";
    }


    @Override
    public UUID getId ()
    {
        return ID;
    }


    @Override
    public int getRequiredAPIVersion ()
    {
        return 21;
    }


    @Override
    public String getHardwareVendor ()
    {
        return "DrivenByMoss";
    }


    @Override
    public String getHardwareModel ()
    {
        return "Conformance Probe (Rust)";
    }


    @Override
    public int getNumMidiInPorts ()
    {
        return 0;
    }


    @Override
    public int getNumMidiOutPorts ()
    {
        return 0;
    }


    @Override
    public void listAutoDetectionMidiPortNames (final AutoDetectionMidiPortNamesList list, final PlatformType platformType)
    {
        // No ports
    }


    @Override
    public ControllerExtension createInstance (final ControllerHost host)
    {
        return new ProbeExtension (this, host);
    }
}
