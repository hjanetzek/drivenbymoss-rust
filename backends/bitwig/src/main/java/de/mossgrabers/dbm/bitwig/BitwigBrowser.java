// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.BrowserFilterColumn;
import com.bitwig.extension.controller.api.BrowserFilterItem;
import com.bitwig.extension.controller.api.BrowserItem;
import com.bitwig.extension.controller.api.BrowserItemBank;
import com.bitwig.extension.controller.api.BrowserResultsItemBank;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.CursorBrowserFilterItem;
import com.bitwig.extension.controller.api.CursorBrowserItem;
import com.bitwig.extension.controller.api.CursorBrowserResultItem;
import com.bitwig.extension.controller.api.InsertionPoint;
import com.bitwig.extension.controller.api.PopupBrowser;


/**
 * The popup browser of Bitwig (protocol v2), as BrowserImpl, BrowserColumnImpl and
 * BrowserColumnItemImpl of the Java version: the content types, 7 filter columns with a page of
 * items each, a page of results, preview. Reported under "/browser/…" while it is open; the
 * commands of the "browser" processor open it on an insertion point and change the selection.
 */
final class BitwigBrowser
{
    private final ControllerHost              host;
    private final PopupBrowser                browser;
    private final BrowserFilterColumn []      columns;
    private final BrowserItemBank<?> []       columnItemBanks;
    private final CursorBrowserFilterItem []  columnCursors;
    private final CursorBrowserResultItem     cursorResult;
    private final BrowserResultsItemBank      resultsItemBank;
    private final int                         numFilterColumnEntries;
    private final int                         numResults;


    /**
     * Create all Bitwig objects; must be called during init.
     *
     * @param host The host
     * @param numFilterColumnEntries The number of items per page of a filter column
     * @param numResults The number of results per page
     */
    BitwigBrowser (final ControllerHost host, final int numFilterColumnEntries, final int numResults)
    {
        this.host = host;
        this.numFilterColumnEntries = numFilterColumnEntries;
        this.numResults = numResults;
        this.browser = host.createPopupBrowser ();
        this.browser.exists ().markInterested ();
        this.browser.selectedContentTypeIndex ().markInterested ();
        this.browser.selectedContentTypeName ().markInterested ();
        this.browser.contentTypeNames ().markInterested ();
        this.browser.shouldAudition ().markInterested ();

        // As BrowserImpl: without the smart collection column, which needs an API extension
        this.columns = new BrowserFilterColumn []
        {
            this.browser.locationColumn (),
            this.browser.fileTypeColumn (),
            this.browser.categoryColumn (),
            this.browser.tagColumn (),
            this.browser.creatorColumn (),
            this.browser.deviceTypeColumn (),
            this.browser.deviceColumn ()
        };
        this.columnItemBanks = new BrowserItemBank<?> [this.columns.length];
        this.columnCursors = new CursorBrowserFilterItem [this.columns.length];
        for (int c = 0; c < this.columns.length; c++)
        {
            final BrowserFilterColumn column = this.columns[c];
            column.exists ().markInterested ();
            column.name ().markInterested ();
            column.getWildcardItem ().name ().markInterested ();
            this.columnItemBanks[c] = ((CursorBrowserItem) column.createCursorItem ()).createSiblingsBank (numFilterColumnEntries);
            this.columnItemBanks[c].cursorIndex ().markInterested ();
            for (int i = 0; i < numFilterColumnEntries; i++)
                markItem (this.columnItemBanks[c].getItemAt (i));
            this.columnCursors[c] = (CursorBrowserFilterItem) column.createCursorItem ();
            this.columnCursors[c].exists ().markInterested ();
            this.columnCursors[c].name ().markInterested ();
        }

        this.cursorResult = (CursorBrowserResultItem) this.browser.resultsColumn ().createCursorItem ();
        this.cursorResult.name ().markInterested ();
        this.resultsItemBank = (BrowserResultsItemBank) this.cursorResult.createSiblingsBank (numResults);
        for (int i = 0; i < numResults; i++)
            markItem (this.resultsItemBank.getItemAt (i));
    }


    private static void markItem (final BrowserItem item)
    {
        item.exists ().markInterested ();
        item.name ().markInterested ();
        item.isSelected ().markInterested ();
        if (item instanceof final BrowserFilterItem filterItem)
            filterItem.hitCount ().markInterested ();
    }


    /**
     * Report the current state. Only "/browser/exists" while the browser is closed.
     *
     * @param reporter Where to report to
     */
    void collect (final ModelReporter reporter)
    {
        final boolean exists = this.browser.exists ().get ();
        reporter.put ("/browser/exists", exists);
        if (!exists)
            return;

        final String [] contentTypes = this.browser.contentTypeNames ().get ();
        for (int i = 0; i < contentTypes.length; i++)
            reporter.put ("/browser/contentType/" + i + "/name", contentTypes[i]);
        reporter.put ("/browser/contentType/count", contentTypes.length);
        reporter.put ("/browser/contentType/selected", this.browser.selectedContentTypeIndex ().get ());
        // As BrowserImpl.getSelectedContentType: the name is not usable since Bitwig 5
        reporter.put ("/browser/contentType/selected/name", "Result");
        reporter.put ("/browser/preview", this.browser.shouldAudition ().get ());

        reporter.put ("/browser/column/count", this.columns.length);
        for (int c = 0; c < this.columns.length; c++)
        {
            final BrowserFilterColumn column = this.columns[c];
            final String prefix = "/browser/column/" + c + "/";
            reporter.put (prefix + "exists", column.exists ().get ());
            reporter.put (prefix + "name", column.name ().get ());
            reporter.put (prefix + "wildcard", column.getWildcardItem ().name ().get ());
            reporter.put (prefix + "cursor/exists", this.columnCursors[c].exists ().get ());
            reporter.put (prefix + "cursor/name", this.columnCursors[c].name ().get ());
            reporter.put (prefix + "cursorIndex", this.columnItemBanks[c].cursorIndex ().get ());
            for (int i = 0; i < this.numFilterColumnEntries; i++)
                collectItem (reporter, prefix + "item/" + i + "/", this.columnItemBanks[c].getItemAt (i));
        }

        for (int i = 0; i < this.numResults; i++)
            collectItem (reporter, "/browser/result/" + i + "/", this.resultsItemBank.getItemAt (i));
        reporter.put ("/browser/result/cursor/name", this.cursorResult.name ().get ());
    }


    private static void collectItem (final ModelReporter reporter, final String prefix, final BrowserItem item)
    {
        reporter.put (prefix + "exists", item.exists ().get ());
        reporter.put (prefix + "name", item.name ().get ());
        reporter.put (prefix + "select", item.isSelected ().get ());
        if (item instanceof final BrowserFilterItem filterItem)
            reporter.put (prefix + "hits", filterItem.hitCount ().get ());
    }


    /**
     * Open the browser on an insertion point, as BrowserImpl.browse: the previous browser is
     * cancelled and gets a moment to shut down.
     *
     * @param insertionPoint The insertion point, may be null
     */
    void browse (final InsertionPoint insertionPoint)
    {
        this.stopBrowsing (false);
        if (insertionPoint != null)
            this.host.scheduleTask (insertionPoint::browse, 100);
    }


    void stopBrowsing (final boolean commit)
    {
        if (commit)
            this.browser.commit ();
        else
            this.browser.cancel ();
    }


    void setPreview (final boolean enable)
    {
        this.browser.shouldAudition ().set (enable);
    }


    void changeContentType (final int delta)
    {
        this.browser.selectedContentTypeIndex ().inc (delta);
    }


    /**
     * Change the selection of a filter column: "+", "-", "page/+", "page/-", "reset".
     *
     * @return False if the column or command is unknown
     */
    boolean handleColumn (final int column, final String command)
    {
        if (column < 0 || column >= this.columns.length)
            return false;
        switch (command)
        {
            case "+" -> this.columnCursors[column].selectNext ();
            case "-" -> this.columnCursors[column].selectPrevious ();
            case "page/+" -> this.columnItemBanks[column].scrollPageForwards ();
            case "page/-" -> this.columnItemBanks[column].scrollPageBackwards ();
            case "reset" -> this.columnCursors[column].selectFirst ();
            default ->
            {
                return false;
            }
        }
        return true;
    }


    /**
     * Change the selected result: "+", "-", "page/+", "page/-".
     *
     * @return False if the command is unknown
     */
    boolean handleResult (final String command)
    {
        switch (command)
        {
            case "+" -> this.cursorResult.selectNext ();
            case "-" -> this.cursorResult.selectPrevious ();
            case "page/+" -> this.resultsItemBank.scrollPageForwards ();
            case "page/-" -> this.resultsItemBank.scrollPageBackwards ();
            default ->
            {
                return false;
            }
        }
        return true;
    }
}
