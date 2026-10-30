// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Device;
import com.bitwig.extension.controller.api.Parameter;
import com.bitwig.extension.controller.api.SpecificBitwigDevice;

import java.util.UUID;


/**
 * The first EQ+ of the cursor track, as EqualizerDeviceImpl and EqTypeParameter of the Java
 * version. Reported in the layout of the Reaper EQ which the core uses: "/eq/param/&lt;3 * band +
 * 0|1|2&gt;/…" are the frequency, gain and Q of the band, "/eq/band/&lt;band&gt;/value" the type of
 * the band as the index of the Reaper EQ.
 */
final class EqualizerDevice
{
    /** The ID of the Bitwig EQ+ device. */
    static final UUID          ID_BITWIG_EQ_PLUS = UUID.fromString ("e4815188-ba6f-4d14-bcfc-2dcb8f778ccb");

    private static final int   NUMBER_OF_BANDS   = 8;
    private static final String PREFIX           = "/eq";

    /**
     * The types: the start of the displayed value of the EQ+ type parameter (EqualizerDeviceImpl),
     * the index of the type in the EQ+ type parameter (EqTypeParameter), the index of the Reaper
     * EQ (EqualizerBandType::reaper_index of the core).
     */
    private static final String [] TYPE_NAMES    = { "Off", "Low-c", "Low-s", "Bell", "High-c", "High-s", "Notch" };
    private static final int []    TYPE_INDICES  = { 0, 3, 6, 7, 10, 13, 14 };
    private static final int []    REAPER_TYPES  = { -1, 4, 0, 8, 3, 1, 6 };

    final Device               device;
    private final Parameter [] types       = new Parameter [NUMBER_OF_BANDS];
    /** Frequency, gain and Q of each band, in this order. */
    private final Parameter [] parameters  = new Parameter [3 * NUMBER_OF_BANDS];


    /**
     * Create all Bitwig objects; must be called during init.
     *
     * @param device The matched device
     */
    EqualizerDevice (final Device device)
    {
        this.device = device;

        final SpecificBitwigDevice eqDevice = device.createSpecificBitwigDevice (ID_BITWIG_EQ_PLUS);
        for (int i = 0; i < NUMBER_OF_BANDS; i++)
        {
            final String index = Integer.toString (i + 1);
            this.types[i] = eqDevice.createParameter ("TYPE" + index);
            this.parameters[3 * i] = eqDevice.createParameter ("FREQ" + index);
            this.parameters[3 * i + 1] = eqDevice.createParameter ("GAIN" + index);
            this.parameters[3 * i + 2] = eqDevice.createParameter ("Q" + index);
        }

        device.exists ().markInterested ();
        device.name ().markInterested ();
        device.isEnabled ().markInterested ();
        device.isWindowOpen ().markInterested ();
        device.isExpanded ().markInterested ();
        for (final Parameter type: this.types)
            type.displayedValue ().markInterested ();
        for (final Parameter parameter: this.parameters)
            BitwigModel.markParameter (parameter);
    }


    /**
     * The parameter with the given index (frequency, gain and Q of each band).
     *
     * @param index The index
     * @return The parameter or null
     */
    Parameter getParameter (final int index)
    {
        return index >= 0 && index < this.parameters.length ? this.parameters[index] : null;
    }


    /**
     * Set the type of a band (EqTypeParameter.set).
     *
     * @param band The index of the band
     * @param reaperType The type as the index of the Reaper EQ
     * @return False if the band or type is unknown
     */
    boolean setType (final int band, final int reaperType)
    {
        if (band < 0 || band >= NUMBER_OF_BANDS)
            return false;
        for (int t = 0; t < REAPER_TYPES.length; t++)
        {
            if (REAPER_TYPES[t] == reaperType)
            {
                this.types[band].value ().set (Integer.valueOf (TYPE_INDICES[t]), Integer.valueOf (15));
                return true;
            }
        }
        return false;
    }


    /**
     * Report the current state.
     *
     * @param reporter Where to report to
     */
    void collect (final ModelReporter reporter)
    {
        final boolean exists = this.device.exists ().get ();
        reporter.put (PREFIX + "/exists", exists);
        reporter.put (PREFIX + "/name", exists ? this.device.name ().get () : "");
        reporter.put (PREFIX + "/bypass", exists && !this.device.isEnabled ().get ());
        reporter.put (PREFIX + "/window", exists && this.device.isWindowOpen ().get ());
        reporter.put (PREFIX + "/expand", exists && this.device.isExpanded ().get ());

        for (int i = 0; i < this.parameters.length; i++)
        {
            final Parameter parameter = this.parameters[i];
            final String paramPrefix = PREFIX + "/param/" + i + "/";
            final boolean paramExists = exists && parameter.exists ().get ();
            reporter.put (paramPrefix + "name", paramExists ? parameter.name ().get () : "");
            reporter.put (paramPrefix + "value", paramExists ? parameter.value ().get () : 0);
            BitwigModel.putDisplayedValue (reporter, paramPrefix + "value/str", parameter.displayedValue (), paramExists);
            reporter.put (paramPrefix + "steps", -1);
            reporter.put (paramPrefix + "modulated", paramExists ? parameter.modulatedValue ().get () : 0);
        }
        reporter.put (PREFIX + "/param/count", exists ? this.parameters.length : 0);

        for (int i = 0; i < NUMBER_OF_BANDS; i++)
            reporter.put (PREFIX + "/band/" + i + "/value", exists ? reaperType (this.types[i].displayedValue ().get ()) : -1);
    }


    /** The type of a band as the index of the Reaper EQ (EqualizerDeviceImpl.getTypeID). */
    private static int reaperType (final String displayedValue)
    {
        for (int t = 0; t < TYPE_NAMES.length; t++)
        {
            if (displayedValue.startsWith (TYPE_NAMES[t]))
                return REAPER_TYPES[t];
        }
        return -1;
    }
}
