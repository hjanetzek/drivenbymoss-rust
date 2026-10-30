// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.SettableBooleanValue;
import com.bitwig.extension.controller.api.SettableEnumValue;
import com.bitwig.extension.controller.api.SettableRangedValue;
import com.bitwig.extension.controller.api.SettableStringValue;
import com.bitwig.extension.controller.api.Settings;

import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.function.BiConsumer;
import java.util.function.Consumer;


/**
 * The settings of the controller in Bitwig's UI, as SettingsUIImpl of the Java version: the
 * global settings in the controller preferences, the document settings in the document state
 * (stored in the project). Bitwig keeps the values; they are sent to the core on start and on
 * every change, changes of the core (e.g. the scale selected on the controller) are set in
 * Bitwig.
 */
final class HostSettings
{
    /** The maximum length of a text setting (StringSettingImpl users pass similar lengths). */
    private static final int                    MAX_TEXT_LENGTH = 100;

    private final Map<Integer, Consumer<String>> setters        = new HashMap<> ();


    /**
     * Create the settings; must be called during init, after the core started the controller.
     *
     * @param host The host
     * @param settings The settings of the core (NativeCore.getSettings)
     * @param core Called with the ID and the value of a setting changed in Bitwig
     */
    HostSettings (final ControllerHost host, final String settings, final BiConsumer<Integer, String> core)
    {
        final Settings preferences = host.getPreferences ();
        final Settings documentState = host.getDocumentState ();

        for (final String line: settings.split ("\n"))
        {
            final String [] fields = line.split ("\t", -1);
            if (fields.length < 7)
                continue;
            final Integer id;
            try
            {
                id = Integer.valueOf (fields[0]);
            }
            catch (final NumberFormatException ex)
            {
                continue;
            }
            final Settings target = "document".equals (fields[1]) ? documentState : preferences;
            final String category = fields[3];
            final String label = fields[4];
            final String defaultValue = fields[5];
            final Consumer<String> toCore = value -> core.accept (id, value);

            switch (fields[2])
            {
                case "enum" ->
                {
                    final String [] options = Arrays.copyOfRange (fields, 7, fields.length);
                    if (options.length == 0)
                        continue;
                    final String initial = Arrays.asList (options).contains (defaultValue) ? defaultValue : options[0];
                    final SettableEnumValue value = target.getEnumSetting (label, category, options, initial);
                    value.addValueObserver (toCore::accept);
                    this.setters.put (id, value::set);
                    toCore.accept (value.get ());
                }
                case "integer" ->
                {
                    if (fields.length < 9)
                        continue;
                    final int min = parseInt (fields[7], 0);
                    final int max = parseInt (fields[8], min);
                    final SettableRangedValue value = target.getNumberSetting (label, category, min, max, 1, "", parseInt (defaultValue, min));
                    value.addRawValueObserver (raw -> toCore.accept (Integer.toString ((int) Math.round (raw))));
                    this.setters.put (id, text -> value.setRaw (parseInt (text, min)));
                    toCore.accept (Integer.toString ((int) Math.round (value.getRaw ())));
                }
                case "boolean" ->
                {
                    final SettableBooleanValue value = target.getBooleanSetting (label, category, Boolean.parseBoolean (defaultValue));
                    value.addValueObserver (on -> toCore.accept (Boolean.toString (on)));
                    this.setters.put (id, text -> value.set (Boolean.parseBoolean (text)));
                    toCore.accept (Boolean.toString (value.get ()));
                }
                case "text" ->
                {
                    final SettableStringValue value = target.getStringSetting (label, category, MAX_TEXT_LENGTH, defaultValue);
                    value.addValueObserver (toCore::accept);
                    this.setters.put (id, value::set);
                    toCore.accept (value.get ());
                }
                default ->
                {
                    // Unknown kind
                }
            }
        }
    }


    /**
     * Set the settings which the core changed in Bitwig.
     *
     * @param changes One "ID&lt;tab&gt;value" per line (NativeCore.takeSettingChanges)
     */
    void apply (final String changes)
    {
        for (final String line: changes.split ("\n"))
        {
            final int tab = line.indexOf ('\t');
            if (tab <= 0)
                continue;
            final Consumer<String> setter = this.setters.get (Integer.valueOf (parseInt (line.substring (0, tab), -1)));
            if (setter != null)
                setter.accept (line.substring (tab + 1));
        }
    }


    private static int parseInt (final String text, final int defaultValue)
    {
        try
        {
            return Integer.parseInt (text.trim ());
        }
        catch (final NumberFormatException ex)
        {
            return defaultValue;
        }
    }
}
