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

import de.mossgrabers.dbm.bitwig.Protocol.Msg;


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
        reporter.put (Msg.BROWSER_EXISTS, exists);
        if (!exists)
            return;

        final String [] contentTypes = this.browser.contentTypeNames ().get ();
        for (int i = 0; i < contentTypes.length; i++)
            reporter.put (Msg.CONTENT_TYPE_NAME | Protocol.key (0, i, 0, 0), contentTypes[i]);
        reporter.put (Msg.CONTENT_TYPE_COUNT, contentTypes.length);
        reporter.put (Msg.CONTENT_TYPE_SELECTED, this.browser.selectedContentTypeIndex ().get ());
        // As BrowserImpl.getSelectedContentType: the name is not usable since Bitwig 5
        reporter.put (Msg.CONTENT_TYPE_SELECTED_NAME, "Result");
        reporter.put (Msg.BROWSER_PREVIEW, this.browser.shouldAudition ().get ());

        reporter.put (Msg.COLUMN_COUNT, this.columns.length);
        for (int c = 0; c < this.columns.length; c++)
        {
            final BrowserFilterColumn column = this.columns[c];
            final long base = Protocol.key (0, c, 0, 0);
            reporter.put (Msg.COLUMN_EXISTS | base, column.exists ().get ());
            reporter.put (Msg.COLUMN_NAME | base, column.name ().get ());
            reporter.put (Msg.COLUMN_WILDCARD | base, column.getWildcardItem ().name ().get ());
            reporter.put (Msg.COLUMN_CURSOR_EXISTS | base, this.columnCursors[c].exists ().get ());
            reporter.put (Msg.COLUMN_CURSOR_NAME | base, this.columnCursors[c].name ().get ());
            reporter.put (Msg.COLUMN_CURSOR_INDEX | base, this.columnItemBanks[c].cursorIndex ().get ());
            for (int i = 0; i < this.numFilterColumnEntries; i++)
                collectItem (reporter, Protocol.key (0, Protocol.BrowserItemKind.COLUMN, c, i), this.columnItemBanks[c].getItemAt (i));
        }

        for (int i = 0; i < this.numResults; i++)
            collectItem (reporter, Protocol.key (0, Protocol.BrowserItemKind.RESULT, 0, i), this.resultsItemBank.getItemAt (i));
        reporter.put (Msg.RESULT_CURSOR_NAME, this.cursorResult.name ().get ());
    }


    private static void collectItem (final ModelReporter reporter, final long base, final BrowserItem item)
    {
        reporter.put (Msg.ITEM_EXISTS | base, item.exists ().get ());
        reporter.put (Msg.ITEM_NAME | base, item.name ().get ());
        reporter.put (Msg.ITEM_SELECTED | base, item.isSelected ().get ());
        if (item instanceof final BrowserFilterItem filterItem)
            reporter.put (Msg.ITEM_HITS | base, filterItem.hitCount ().get ());
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
     * Select the previous or next item of a filter column ("column/&lt;column&gt;/-|+").
     *
     * @param column The column
     * @param next True for the next item
     * @return False if the column is unknown
     */
    boolean stepColumn (final int column, final boolean next)
    {
        if (column < 0 || column >= this.columns.length)
            return false;
        if (next)
            this.columnCursors[column].selectNext ();
        else
            this.columnCursors[column].selectPrevious ();
        return true;
    }


    /**
     * Scroll the items of a filter column by a page ("column/&lt;column&gt;/page/-|+").
     *
     * @param column The column
     * @param next True for the next page
     * @return False if the column is unknown
     */
    boolean pageColumn (final int column, final boolean next)
    {
        if (column < 0 || column >= this.columns.length)
            return false;
        if (next)
            this.columnItemBanks[column].scrollPageForwards ();
        else
            this.columnItemBanks[column].scrollPageBackwards ();
        return true;
    }


    /**
     * Select the first item of a filter column ("column/&lt;column&gt;/reset").
     *
     * @param column The column
     * @return False if the column is unknown
     */
    boolean resetColumn (final int column)
    {
        if (column < 0 || column >= this.columns.length)
            return false;
        this.columnCursors[column].selectFirst ();
        return true;
    }


    /**
     * Select the previous or next result ("result/-|+").
     *
     * @param next True for the next result
     */
    void stepResult (final boolean next)
    {
        if (next)
            this.cursorResult.selectNext ();
        else
            this.cursorResult.selectPrevious ();
    }


    /**
     * Scroll the results by a page ("result/page/-|+").
     *
     * @param next True for the next page
     */
    void pageResult (final boolean next)
    {
        if (next)
            this.resultsItemBank.scrollPageForwards ();
        else
            this.resultsItemBank.scrollPageBackwards ();
    }
}
