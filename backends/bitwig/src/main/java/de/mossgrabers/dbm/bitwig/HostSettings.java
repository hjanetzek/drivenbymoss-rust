// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.Action;
import com.bitwig.extension.controller.api.ActionCategory;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.SettableBooleanValue;
import com.bitwig.extension.controller.api.SettableEnumValue;
import com.bitwig.extension.controller.api.SettableRangedValue;
import com.bitwig.extension.controller.api.SettableStringValue;
import com.bitwig.extension.controller.api.Setting;
import com.bitwig.extension.controller.api.Settings;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BiConsumer;
import java.util.function.Consumer;


/**
 * The settings of the controller in Bitwig's UI, as SettingsUIImpl of the Java version: the
 * global settings in the controller preferences, the document settings in the document state
 * (stored in the project). Bitwig keeps the values; they are sent to the core on start and on
 * every change, changes of the core (e.g. the scale selected on the controller) are set in
 * Bitwig. The core hides or disables settings (e.g. the slot settings of Generic Flexi while
 * the slot is off), see {@link #applyStates}.
 */
final class HostSettings
{
    /** The maximum length of a text setting (StringSettingImpl users pass similar lengths). */
    private static final int                    MAX_TEXT_LENGTH = 100;

    private final Map<Integer, Consumer<String>> setters        = new HashMap<> ();
    /** The Bitwig settings of a setting of the core (an action setting has several). */
    private final Map<Integer, Runnable []>      showers        = new HashMap<> ();
    private final Map<Integer, List<Setting>>    widgets        = new HashMap<> ();
    private final ControllerHost                 host;
    private Actions                              actions;


    /**
     * Create the settings; must be called during init, after the core started the controller.
     *
     * @param host The host
     * @param settings The settings of the core (NativeCore.getSettings)
     * @param core Called with the ID and the value of a setting changed in Bitwig
     */
    HostSettings (final ControllerHost host, final String settings, final BiConsumer<Integer, String> core)
    {
        this.host = host;
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
                    this.addWidget (id, value);
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
                    this.addWidget (id, value);
                    toCore.accept (Integer.toString ((int) Math.round (value.getRaw ())));
                }
                case "boolean" ->
                {
                    final SettableBooleanValue value = target.getBooleanSetting (label, category, Boolean.parseBoolean (defaultValue));
                    value.addValueObserver (on -> toCore.accept (Boolean.toString (on)));
                    this.setters.put (id, text -> value.set (Boolean.parseBoolean (text)));
                    this.addWidget (id, value);
                    toCore.accept (Boolean.toString (value.get ()));
                }
                case "text" ->
                {
                    final SettableStringValue value = target.getStringSetting (label, category, MAX_TEXT_LENGTH, defaultValue);
                    value.addValueObserver (toCore::accept);
                    this.setters.put (id, value::set);
                    this.addWidget (id, value);
                    toCore.accept (value.get ());
                }
                case "action" -> this.createActionSetting (id, target, category, label, toCore);
                case "signal" ->
                {
                    // A button; the button text follows the fixed fields
                    final String button = fields.length > 7 ? fields[7] : label;
                    final com.bitwig.extension.controller.api.Signal value = target.getSignalSetting (label, category, button);
                    value.addSignalObserver ( () -> toCore.accept (""));
                    this.addWidget (id, value);
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


    /**
     * Show, hide, enable or disable the settings which the core changed.
     *
     * @param states One "ID&lt;tab&gt;visible&lt;tab&gt;enabled" per line, 1 or 0
     *            (NativeCore.takeSettingStates)
     */
    void applyStates (final String states)
    {
        for (final String line: states.split ("\n"))
        {
            final String [] fields = line.split ("\t");
            if (fields.length < 3)
                continue;
            final Integer id = Integer.valueOf (parseInt (fields[0], -1));
            final boolean visible = "1".equals (fields[1]);
            final boolean enabled = "1".equals (fields[2]);
            final Runnable [] shower = this.showers.get (id);
            for (final Setting setting: this.widgets.getOrDefault (id, List.of ()))
            {
                if (enabled)
                    setting.enable ();
                else
                    setting.disable ();
                if (shower == null)
                {
                    if (visible)
                        setting.show ();
                    else
                        setting.hide ();
                }
            }
            // An action setting shows only the action list of its selected category
            if (shower != null)
                shower[visible ? 0 : 1].run ();
        }
    }


    private void addWidget (final Integer id, final Object setting)
    {
        this.widgets.computeIfAbsent (id, key -> new ArrayList<> ()).add ((Setting) setting);
    }


    /**
     * An action of Bitwig as SettingsUIImpl.getActionSetting and ActionSettingImpl of the Java
     * version: a category and, per category with more than one action, the list of its actions,
     * of which only the one of the selected category is shown. The core gets the ID of the
     * selected action.
     */
    private void createActionSetting (final Integer id, final Settings target, final String category, final String label, final Consumer<String> toCore)
    {
        if (this.actions == null)
            this.actions = new Actions (this.host);
        if (this.actions.categoryNames.length == 0)
            return;

        final String cat = category + " - " + label;
        final SettableEnumValue categorySetting = target.getEnumSetting (label + ": Category", cat, this.actions.categoryNames, this.actions.categoryNames[0]);
        final Map<String, SettableEnumValue> categoryActions = new LinkedHashMap<> ();
        for (final String categoryName: this.actions.categoryNames)
        {
            final String [] actionNames = this.actions.categoryActionNames.get (categoryName);
            if (actionNames.length > 1)
                categoryActions.put (categoryName, target.getEnumSetting (label + ": " + categoryName + " Actions", cat, actionNames, actionNames[0]));
        }

        final Runnable showSelected = () -> {
            final SettableEnumValue selected = categoryActions.get (categorySetting.get ());
            for (final SettableEnumValue setting: categoryActions.values ())
            {
                if (setting == selected)
                    ((Setting) setting).show ();
                else
                    ((Setting) setting).hide ();
            }
        };
        final Runnable selectedID = () -> toCore.accept (this.actions.selectedID (categorySetting.get (), categoryActions));

        categorySetting.addValueObserver (value -> {
            showSelected.run ();
            selectedID.run ();
        });
        for (final SettableEnumValue setting: categoryActions.values ())
            setting.addValueObserver (value -> selectedID.run ());

        this.setters.put (id, actionID -> {
            final String actionCategory = this.actions.actionCategories.get (actionID);
            final String actionName = this.actions.actionNames.get (actionID);
            // Could only happen if actions would be removed (or an ID of another host)
            if (actionCategory == null || actionName == null)
                return;
            categorySetting.set (actionCategory);
            final SettableEnumValue setting = categoryActions.get (actionCategory);
            if (setting != null)
                setting.set (actionName);
            showSelected.run ();
        });
        this.addWidget (id, categorySetting);
        for (final SettableEnumValue setting: categoryActions.values ())
            this.addWidget (id, setting);
        this.showers.put (id, new Runnable []
        {
            () -> {
                ((Setting) categorySetting).show ();
                showSelected.run ();
            },
            () -> {
                ((Setting) categorySetting).hide ();
                for (final SettableEnumValue setting: categoryActions.values ())
                    ((Setting) setting).hide ();
            }
        });

        showSelected.run ();
        selectedID.run ();
    }


    /** The actions of Bitwig (SettingsUIImpl.prepareActions); only available during init. */
    private static final class Actions
    {
        private final String []             categoryNames;
        private final Map<String, String []> categoryActionNames = new HashMap<> ();
        private final Map<String, String>    actionNames         = new LinkedHashMap<> ();
        private final Map<String, String>    actionCategories    = new HashMap<> ();


        Actions (final ControllerHost host)
        {
            final ActionCategory [] categories = host.createApplication ().getActionCategories ();
            this.categoryNames = new String [categories.length];
            for (int i = 0; i < categories.length; i++)
            {
                final String categoryName = categories[i].getName ();
                this.categoryNames[i] = categoryName;
                final Action [] categoryActions = categories[i].getActions ();
                final String [] names = new String [categoryActions.length];
                for (int j = 0; j < categoryActions.length; j++)
                {
                    names[j] = categoryActions[j].getName ();
                    this.actionNames.put (categoryActions[j].getId (), names[j]);
                    this.actionCategories.put (categoryActions[j].getId (), categoryName);
                }
                this.categoryActionNames.put (categoryName, names);
            }
        }


        /** As ActionSettingImpl.get: the ID of the action selected in the list of the category. */
        String selectedID (final String category, final Map<String, SettableEnumValue> categoryActions)
        {
            final SettableEnumValue setting = categoryActions.get (category);
            if (setting == null)
                return this.actionNames.keySet ().iterator ().next ();
            final String actionName = setting.get ();
            for (final Map.Entry<String, String> entry: this.actionNames.entrySet ())
            {
                if (entry.getValue ().equals (actionName))
                    return entry.getKey ();
            }
            return this.actionNames.keySet ().iterator ().next ();
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
