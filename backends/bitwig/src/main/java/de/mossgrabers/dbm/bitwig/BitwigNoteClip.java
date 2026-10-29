// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.CursorTrack;
import com.bitwig.extension.controller.api.NoteOccurrence;
import com.bitwig.extension.controller.api.NoteStep;
import com.bitwig.extension.controller.api.PinnableCursorClip;

import java.util.HashMap;
import java.util.Iterator;
import java.util.Locale;
import java.util.Map;


/**
 * The step grid of a note clip of the core (protocol v2), as CursorClipImpl of the Java version:
 * a launcher cursor clip with a window of the given number of steps and rows. The steps are
 * reported as "/clip/grid/&lt;steps&gt;x&lt;rows&gt;/step/&lt;channel&gt;/&lt;x&gt;/&lt;y&gt;" with all
 * values in one line (the format of StepInfo::to_wire of the core), "0" for an empty step.
 */
final class BitwigNoteClip
{
    /** The occurrence types in the order of the core (NoteOccurrenceType::ALL). */
    private static final NoteOccurrence [] OCCURRENCES =
    {
        NoteOccurrence.ALWAYS,
        NoteOccurrence.FIRST,
        NoteOccurrence.NOT_FIRST,
        NoteOccurrence.PREV,
        NoteOccurrence.NOT_PREV,
        NoteOccurrence.PREV_CHANNEL,
        NoteOccurrence.NOT_PREV_CHANNEL,
        NoteOccurrence.PREV_KEY,
        NoteOccurrence.NOT_PREV_KEY,
        NoteOccurrence.FILL,
        NoteOccurrence.NOT_FILL
    };

    private final ControllerHost        host;
    final PinnableCursorClip            clip;
    final int                           numSteps;
    final int                           numRows;
    /** The ID of the grid: "&lt;steps&gt;x&lt;rows&gt;". */
    final String                        id;
    /** The last state of each step which is not empty or still has to be reported as empty. */
    private final Map<Long, NoteStep>   steps = new HashMap<> ();


    /**
     * Create the clip; must be called during init.
     *
     * @param host The host
     * @param cursorTrack The cursor track
     * @param numSteps The number of steps of the window
     * @param numRows The number of rows (keys) of the window
     */
    BitwigNoteClip (final ControllerHost host, final CursorTrack cursorTrack, final int numSteps, final int numRows)
    {
        this.host = host;
        this.numSteps = numSteps;
        this.numRows = numRows;
        this.id = numSteps + "x" + numRows;
        this.clip = cursorTrack.createLauncherCursorClip (numSteps, numRows);
        this.clip.addNoteStepObserver (step -> this.steps.put (Long.valueOf (key (step.channel (), step.x (), step.y ())), step));
        this.clip.exists ().markInterested ();
        this.clip.playingStep ().markInterested ();
        this.clip.getPlayStart ().markInterested ();
        this.clip.getPlayStop ().markInterested ();
        this.clip.getLoopStart ().markInterested ();
        this.clip.getLoopLength ().markInterested ();
        this.clip.isLoopEnabled ().markInterested ();
        this.clip.getShuffle ().markInterested ();
        this.clip.getAccent ().markInterested ();
        this.clip.color ().markInterested ();
        this.clip.isPinned ().markInterested ();
        this.clip.getTrack ().canHoldNoteData ().markInterested ();
    }


    private static long key (final int channel, final int x, final int y)
    {
        return ((long) channel << 40) | ((long) x << 20) | y;
    }


    /**
     * Report the clip properties (only from one of the grids, they all show the same clip).
     *
     * @param reporter Where to report to
     */
    void collectClip (final ModelReporter reporter)
    {
        final boolean exists = this.clip.exists ().get ();
        reporter.put ("/clip/exists", exists);
        reporter.put ("/clip/start", this.clip.getPlayStart ().get ());
        reporter.put ("/clip/end", this.clip.getPlayStop ().get ());
        reporter.put ("/clip/loop", this.clip.isLoopEnabled ().get ());
        reporter.put ("/clip/loopStart", this.clip.getLoopStart ().get ());
        reporter.put ("/clip/loopLength", this.clip.getLoopLength ().get ());
        reporter.put ("/clip/shuffle", this.clip.getShuffle ().get ());
        reporter.put ("/clip/accent", this.clip.getAccent ().get ());
        reporter.put ("/clip/pinned", this.clip.isPinned ().get ());
        reporter.putColor ("/clip/color", this.clip.color ().red (), this.clip.color ().green (), this.clip.color ().blue ());
    }


    /**
     * Report the playing step and the changed steps of the grid.
     *
     * @param reporter Where to report to
     */
    void collectGrid (final ModelReporter reporter)
    {
        final String prefix = "/clip/grid/" + this.id + "/";
        reporter.put (prefix + "playingStep", this.clip.playingStep ().get ());
        final Iterator<NoteStep> iterator = this.steps.values ().iterator ();
        while (iterator.hasNext ())
        {
            final NoteStep step = iterator.next ();
            final String address = prefix + "step/" + step.channel () + "/" + step.x () + "/" + step.y ();
            if (step.state () == NoteStep.State.Empty)
            {
                reporter.put (address, "0");
                iterator.remove ();
            }
            else
                reporter.put (address, format (step));
        }
    }


    /** All values of a step, as StepInfoImpl.updateData of the Java version (gain / 2). */
    private static String format (final NoteStep step)
    {
        final int state = step.state () == NoteStep.State.NoteOn ? 1 : 2;
        int occurrence = 0;
        for (int i = 0; i < OCCURRENCES.length; i++)
            if (OCCURRENCES[i] == step.occurrence ())
                occurrence = i;
        return String.format (Locale.ROOT, "%d %d %d %s %s %s %s %s %s %s %s %s %d %s %d %d %d %d %d %d %d %s %s %s", Integer.valueOf (state), flag (step.isIsSelected ()), flag (step.isMuted ()), num (step.duration ()), num (step.velocity ()), num (step.velocitySpread ()), num (step.releaseVelocity ()), num (step.pressure ()), num (step.timbre ()), num (step.pan ()), num (step.transpose ()), num (step.gain () / 2.0), flag (step.isChanceEnabled ()), num (step.chance ()), flag (step.isOccurrenceEnabled ()), Integer.valueOf (occurrence), flag (step.isRecurrenceEnabled ()), Integer.valueOf (step.recurrenceLength ()), Integer.valueOf (step.recurrenceMask ()), flag (step.isRepeatEnabled ()), Integer.valueOf (step.repeatCount ()), num (step.repeatCurve ()), num (step.repeatVelocityCurve ()), num (step.repeatVelocityEnd ()));
    }


    private static Integer flag (final boolean value)
    {
        return Integer.valueOf (value ? 1 : 0);
    }


    private static String num (final double value)
    {
        return Double.toString (value);
    }


    /**
     * A command of the grid: "stepSize", "scroll", "clearAll", "clearRow/&lt;ch&gt;/&lt;row&gt;",
     * "clearColumn/&lt;ch&gt;/&lt;step&gt;", "step/&lt;ch&gt;/&lt;x&gt;/&lt;y&gt;/toggle|set|setFrom|update|clear|moveY".
     *
     * @param parts The parts of the command after "grid/&lt;id&gt;/"
     * @param value The numeric value
     * @param text The text value or null
     * @return False if the command is unknown
     */
    boolean handle (final String [] parts, final double value, final String text)
    {
        switch (parts[0])
        {
            case "stepSize" -> this.clip.setStepSize (value);
            case "scroll" -> this.clip.scrollToStep ((int) value);
            case "clearAll" -> this.clip.clearSteps ();
            case "clearRow" ->
            {
                if (parts.length != 3)
                    return false;
                this.clip.clearStepsAtY (parse (parts[1]), parse (parts[2]));
            }
            case "clearColumn" ->
            {
                if (parts.length != 3)
                    return false;
                this.clip.clearStepsAtX (parse (parts[1]), parse (parts[2]));
            }
            case "step" ->
            {
                if (parts.length != 5)
                    return false;
                return this.handleStep (parse (parts[1]), parse (parts[2]), parse (parts[3]), parts[4], value, text);
            }
            default ->
            {
                return false;
            }
        }
        return true;
    }


    private boolean handleStep (final int channel, final int x, final int y, final String command, final double value, final String text)
    {
        switch (command)
        {
            case "toggle" -> this.clip.toggleStep (channel, x, y, (int) value);
            case "clear" -> this.clip.clearStep (channel, x, y);
            case "moveY" -> this.clip.moveStep (channel, x, y, 0, (int) value - y);
            case "set" ->
            {
                final String [] values = split (text);
                if (values.length < 2)
                    return false;
                this.clip.setStep (channel, x, y, parse (values[0]), parseDouble (values[1]));
            }
            case "setFrom" ->
            {
                // As CursorClipImpl.setStep (NotePosition, IStepInfo): create the note, set the
                // other values a bit later when Bitwig has created it
                final String [] values = split (text);
                if (values.length < 24)
                    return false;
                this.clip.setStep (channel, x, y, (int) (parseDouble (values[4]) * 127), parseDouble (values[3]));
                this.host.scheduleTask ( () -> {
                    final NoteStep step = this.clip.getStep (channel, x, y);
                    step.setVelocity (parseDouble (values[4]));
                    step.setGain (parseDouble (values[11]));
                    step.setPan (parseDouble (values[9]));
                    step.setPressure (parseDouble (values[7]));
                    step.setReleaseVelocity (parseDouble (values[6]));
                    step.setTimbre (parseDouble (values[8]));
                    step.setTranspose (parseDouble (values[10]));
                }, 100);
            }
            case "update" ->
            {
                final String [] values = split (text);
                if (values.length < 24)
                    return false;
                update (this.clip.getStep (channel, x, y), values);
            }
            default ->
            {
                return false;
            }
        }
        return true;
    }


    /** Set all values of a step, as CursorClipImpl.sendClipData of the Java version. */
    private static void update (final NoteStep step, final String [] values)
    {
        if (step == null || step.state () == NoteStep.State.Empty)
            return;
        step.setIsMuted (parseDouble (values[2]) > 0);
        step.setDuration (parseDouble (values[3]));
        step.setVelocity (parseDouble (values[4]));
        step.setVelocitySpread (parseDouble (values[5]));
        step.setReleaseVelocity (parseDouble (values[6]));
        step.setPressure (parseDouble (values[7]));
        step.setTimbre (parseDouble (values[8]));
        step.setPan (parseDouble (values[9]));
        step.setTranspose (parseDouble (values[10]));
        step.setGain (parseDouble (values[11]));
        step.setIsChanceEnabled (parseDouble (values[12]) > 0);
        step.setChance (parseDouble (values[13]));
        step.setIsOccurrenceEnabled (parseDouble (values[14]) > 0);
        step.setOccurrence (OCCURRENCES[Math.clamp (parse (values[15]), 0, OCCURRENCES.length - 1)]);
        step.setIsRecurrenceEnabled (parseDouble (values[16]) > 0);
        step.setRecurrence (Math.max (1, parse (values[17])), parse (values[18]));
        step.setIsRepeatEnabled (parseDouble (values[19]) > 0);
        step.setRepeatCount (parse (values[20]));
        step.setRepeatCurve (parseDouble (values[21]));
        step.setRepeatVelocityCurve (parseDouble (values[22]));
        step.setRepeatVelocityEnd (parseDouble (values[23]));
    }


    private static String [] split (final String text)
    {
        return text == null ? new String [0] : text.trim ().split ("\\s+");
    }


    private static int parse (final String text)
    {
        return (int) parseDouble (text);
    }


    private static double parseDouble (final String text)
    {
        try
        {
            return Double.parseDouble (text);
        }
        catch (final NumberFormatException ex)
        {
            return 0;
        }
    }
}
