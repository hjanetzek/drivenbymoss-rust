// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.ClipLauncherSlot;
import com.bitwig.extension.controller.api.ClipLauncherSlotBank;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.CursorTrack;
import com.bitwig.extension.controller.api.SceneBank;
import com.bitwig.extension.controller.api.Track;
import com.bitwig.extension.controller.api.TrackBank;


/**
 * The clip launcher navigator of the host, as ClipLauncherNavigatorImpl of the Java version for
 * Bitwig: a bank of one track which follows its own cursor track, with one scene. Selecting its
 * slot lets Bitwig keep the selected slot when another track is selected. The core sends
 * "navigator selectTrack &lt;position&gt;" and "navigator tracks|clips|scenes/-|+" (host feature
 * clipLauncherNavigator); without it the core moves the slot itself (the Reaper version).
 */
final class ClipLauncherNavigator
{
    private final ControllerHost   host;
    private final TrackBank        singleTrackBank;
    private final Track            theTrack;
    private final ClipLauncherSlot theClip;
    private final SceneBank        sceneBank;


    /**
     * Create all Bitwig objects; must be called during init.
     *
     * @param host The host
     * @param modelSceneBank The scene bank of the controller (the window of the core), which
     *            follows the scene of the navigator
     * @param pageSize The number of scenes of the window
     */
    ClipLauncherNavigator (final ControllerHost host, final SceneBank modelSceneBank, final int pageSize)
    {
        this.host = host;

        this.singleTrackBank = host.createTrackBank (1, 0, 1);
        this.singleTrackBank.scrollPosition ().markInterested ();
        final CursorTrack cursorTrack = host.createCursorTrack (1, 1);
        this.singleTrackBank.followCursorTrack (cursorTrack);
        this.theTrack = this.singleTrackBank.getItemAt (0);

        final ClipLauncherSlotBank slotBank = this.theTrack.clipLauncherSlotBank ();
        this.singleTrackBank.setShouldShowClipLauncherFeedback (true);
        this.theClip = slotBank.getItemAt (0);

        // The scene window of the controller follows the scene of the navigator's slot
        this.theClip.sceneIndex ().addValueObserver (position -> {
            final int scrollPosition = modelSceneBank.scrollPosition ().get ();
            final int newPosition = position / pageSize * pageSize;
            if (scrollPosition != newPosition)
                modelSceneBank.scrollPosition ().set (newPosition);
            modelSceneBank.getItemAt (position % pageSize).selectInEditor ();
        });

        this.sceneBank = this.singleTrackBank.sceneBank ();
        this.sceneBank.cursorIndex ().markInterested ();
        this.sceneBank.setIndication (false);
    }


    /**
     * Select the track at the position (ClipLauncherNavigatorImpl.selectTrack: the core selects
     * the track afterwards).
     *
     * @param position The position of the track
     */
    void selectTrack (final int position)
    {
        this.singleTrackBank.scrollPosition ().set (position);
    }


    /**
     * Move the navigator.
     *
     * @param target The target (Protocol.NavigatorTarget: tracks, clips, scenes)
     * @param next True to move to the next item
     */
    void step (final int target, final boolean next)
    {
        switch (target)
        {
            case Protocol.NavigatorTarget.TRACKS -> this.navigateTracks (!next);
            case Protocol.NavigatorTarget.SCENES ->
            {
                this.navigateClips (!next);
                this.host.scheduleTask ( () -> this.sceneBank.getItemAt (0).selectInEditor (), 100);
            }
            default -> this.navigateClips (!next);
        }
    }


    private void navigateClips (final boolean isLeft)
    {
        if (isLeft)
            this.sceneBank.scrollBackwards ();
        else
            this.sceneBank.scrollForwards ();
        this.theClip.select ();
    }


    private void navigateTracks (final boolean isLeft)
    {
        if (isLeft)
            this.singleTrackBank.scrollBackwards ();
        else
            this.singleTrackBank.scrollForwards ();
        this.theClip.select ();

        this.theTrack.selectInEditor ();
        this.theTrack.selectInMixer ();
        this.theTrack.makeVisibleInArranger ();
        this.theTrack.makeVisibleInMixer ();
    }
}
