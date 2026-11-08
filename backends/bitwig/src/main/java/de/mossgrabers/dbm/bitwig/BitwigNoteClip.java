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
import java.util.Map;

import de.mossgrabers.dbm.bitwig.Protocol.Msg;


/**
 * The step grid of a note clip of the core (protocol v2), as CursorClipImpl of the Java version:
 * a launcher cursor clip with a window of the given number of steps and rows. The steps are
 * reported as GRID_STEP ("/clip/grid/&lt;steps&gt;x&lt;rows&gt;/step/&lt;channel&gt;/&lt;x&gt;/&lt;y&gt;")
 * with the 24 values of StepInfo::to_values of the core, no values for an empty step.
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
    /**
     * The changed steps, read when Bitwig reports them (as StepInfoImpl.updateData of the Java
     * version): a NoteStep object changes locally with a setter (setGain in its own scale) before
     * Bitwig confirms the value, so it must not be read later. A step which is not empty or still
     * has to be reported as empty (values null).
     */
    private final Map<Long, StepState>  steps = new HashMap<> ();


    /** The state of a step: its position and its values (null: empty). */
    private record StepState (int channel, int x, int y, double [] values)
    {
    }


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
        this.clip = cursorTrack.createLauncherCursorClip (numSteps, numRows);
        this.clip.addNoteStepObserver (step -> this.steps.put (Long.valueOf (key (step.channel (), step.x (), step.y ())), new StepState (step.channel (), step.x (), step.y (), step.state () == NoteStep.State.Empty ? null : values (step))));
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
        reporter.put (Msg.CLIP_EXISTS, exists);
        reporter.put (Msg.CLIP_START, this.clip.getPlayStart ().get ());
        reporter.put (Msg.CLIP_END, this.clip.getPlayStop ().get ());
        reporter.put (Msg.CLIP_LOOP, this.clip.isLoopEnabled ().get ());
        reporter.put (Msg.CLIP_LOOP_START, this.clip.getLoopStart ().get ());
        reporter.put (Msg.CLIP_LOOP_LENGTH, this.clip.getLoopLength ().get ());
        reporter.put (Msg.CLIP_SHUFFLE, this.clip.getShuffle ().get ());
        reporter.put (Msg.CLIP_ACCENT, this.clip.getAccent ().get ());
        reporter.put (Msg.CLIP_PINNED, this.clip.isPinned ().get ());
        reporter.putColor (Msg.CLIP_COLOR, this.clip.color ().red (), this.clip.color ().green (), this.clip.color ().blue ());
    }


    /**
     * Report the playing step and the changed steps of the grid.
     *
     * @param reporter Where to report to
     */
    void collectGrid (final ModelReporter reporter)
    {
        final long grid = Protocol.key (0, this.numSteps, this.numRows, 0);
        reporter.put (Msg.GRID_PLAYING_STEP | grid, this.clip.playingStep ().get ());
        final Iterator<StepState> iterator = this.steps.values ().iterator ();
        while (iterator.hasNext ())
        {
            final StepState step = iterator.next ();
            // The channel and the row share the third slot, the step is the fourth
            final long key = Msg.GRID_STEP | grid | (long) ((step.channel & 15) << 7 | step.y & 127) << 48;
            reporter.putStep (key, step.x, step.values);
            if (step.values == null)
                iterator.remove ();
        }
    }


    /** All values of a step, as StepInfoImpl.updateData of the Java version (gain / 2). */
    private static double [] values (final NoteStep step)
    {
        final int state = step.state () == NoteStep.State.NoteOn ? 1 : 2;
        int occurrence = 0;
        for (int i = 0; i < OCCURRENCES.length; i++)
            if (OCCURRENCES[i] == step.occurrence ())
                occurrence = i;
        return new double []
        {
            state,
            flag (step.isIsSelected ()),
            flag (step.isMuted ()),
            step.duration (),
            step.velocity (),
            step.velocitySpread (),
            step.releaseVelocity (),
            step.pressure (),
            step.timbre (),
            step.pan (),
            step.transpose (),
            step.gain () / 2.0,
            flag (step.isChanceEnabled ()),
            step.chance (),
            flag (step.isOccurrenceEnabled ()),
            occurrence,
            flag (step.isRecurrenceEnabled ()),
            step.recurrenceLength (),
            step.recurrenceMask (),
            flag (step.isRepeatEnabled ()),
            step.repeatCount (),
            step.repeatCurve (),
            step.repeatVelocityCurve (),
            step.repeatVelocityEnd ()
        };
    }


    private static int flag (final boolean value)
    {
        return value ? 1 : 0;
    }


    /**
     * Set the step size in beats (CursorClipImpl.setStepLength).
     *
     * @param stepSize The step size
     */
    void setStepSize (final double stepSize)
    {
        this.clip.setStepSize (stepSize);
    }


    /**
     * Scroll the grid to a step.
     *
     * @param step The step
     */
    void scrollToStep (final int step)
    {
        this.clip.scrollToStep (step);
    }


    /**
     * Scroll the grid by a page, as CursorClipImpl.scrollStepsPageForward / Backwards.
     *
     * @param next True for the next page
     */
    void page (final boolean next)
    {
        if (next)
            this.clip.scrollStepsPageForward ();
        else
            this.clip.scrollStepsPageBackwards ();
    }


    /** Clear all steps. */
    void clearAll ()
    {
        this.clip.clearSteps ();
    }


    /**
     * Clear the steps of a row.
     *
     * @param channel The MIDI channel
     * @param row The row
     */
    void clearRow (final int channel, final int row)
    {
        this.clip.clearStepsAtY (channel, row);
    }


    /**
     * Clear the steps of a column.
     *
     * @param channel The MIDI channel
     * @param column The step
     */
    void clearColumn (final int channel, final int column)
    {
        this.clip.clearStepsAtX (channel, column);
    }


    /**
     * Toggle a note.
     *
     * @param channel The MIDI channel
     * @param x The step
     * @param y The row
     * @param velocity The MIDI velocity
     */
    void toggle (final int channel, final int x, final int y, final int velocity)
    {
        this.clip.toggleStep (channel, x, y, velocity);
    }


    /**
     * Clear a note.
     *
     * @param channel The MIDI channel
     * @param x The step
     * @param y The row
     */
    void clear (final int channel, final int x, final int y)
    {
        this.clip.clearStep (channel, x, y);
    }


    /**
     * Move a note to another row.
     *
     * @param channel The MIDI channel
     * @param x The step
     * @param y The row
     * @param row The new row
     */
    void moveY (final int channel, final int x, final int y, final int row)
    {
        this.clip.moveStep (channel, x, y, 0, row - y);
    }


    /**
     * Set a note.
     *
     * @param channel The MIDI channel
     * @param x The step
     * @param y The row
     * @param velocity The MIDI velocity
     * @param duration The duration in beats
     */
    void set (final int channel, final int x, final int y, final int velocity, final double duration)
    {
        this.clip.setStep (channel, x, y, velocity, duration);
    }


    /**
     * Create a note and set the other values of the step a bit later when Bitwig has created it,
     * as CursorClipImpl.setStep (NotePosition, IStepInfo).
     *
     * @param channel The MIDI channel
     * @param x The step
     * @param y The row
     * @param text The values of the step (StepInfo::to_wire)
     * @return False if the text has not all values
     */
    boolean setFrom (final int channel, final int x, final int y, final String text)
    {
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
        return true;
    }


    /**
     * Update all values of a note, as CursorClipImpl.sendClipData of the Java version.
     *
     * @param channel The MIDI channel
     * @param x The step
     * @param y The row
     * @param text The values of the step (StepInfo::to_wire)
     * @return False if the text has not all values
     */
    boolean update (final int channel, final int x, final int y, final String text)
    {
        final String [] values = split (text);
        if (values.length < 24)
            return false;
        update (this.clip.getStep (channel, x, y), values);
        return true;
    }


    /**
     * Set one attribute of a step, as CursorClipImpl.updateStep* of the Java version outside of an
     * edit (getNoteStep (notePosition).setVelocity (v)).
     *
     * @param channel The MIDI channel
     * @param x The step
     * @param y The row
     * @param attribute The attribute (Protocol.StepAttr)
     * @param value The value
     * @return False if the attribute is unknown
     */
    boolean updateAttribute (final int channel, final int x, final int y, final int attribute, final double value)
    {
        final NoteStep step = this.clip.getStep (channel, x, y);
        if (step == null)
            return true;
        final boolean on = value > 0;
        switch (attribute)
        {
            case Protocol.StepAttr.MUTED -> step.setIsMuted (on);
            case Protocol.StepAttr.DURATION -> step.setDuration (value);
            case Protocol.StepAttr.VELOCITY -> step.setVelocity (value);
            case Protocol.StepAttr.VELOCITY_SPREAD -> step.setVelocitySpread (value);
            case Protocol.StepAttr.RELEASE_VELOCITY -> step.setReleaseVelocity (value);
            case Protocol.StepAttr.PRESSURE -> step.setPressure (value);
            case Protocol.StepAttr.TIMBRE -> step.setTimbre (value);
            case Protocol.StepAttr.PAN -> step.setPan (value);
            case Protocol.StepAttr.TRANSPOSE -> step.setTranspose (value);
            case Protocol.StepAttr.GAIN -> step.setGain (value);
            case Protocol.StepAttr.CHANCE_ENABLED -> step.setIsChanceEnabled (on);
            case Protocol.StepAttr.CHANCE -> step.setChance (value);
            case Protocol.StepAttr.OCCURRENCE_ENABLED -> step.setIsOccurrenceEnabled (on);
            case Protocol.StepAttr.OCCURRENCE -> step.setOccurrence (OCCURRENCES[Math.clamp ((int) value, 0, OCCURRENCES.length - 1)]);
            case Protocol.StepAttr.RECURRENCE_ENABLED -> step.setIsRecurrenceEnabled (on);
            case Protocol.StepAttr.RECURRENCE_LENGTH -> step.setRecurrence ((int) value, step.recurrenceMask ());
            case Protocol.StepAttr.RECURRENCE_MASK -> step.setRecurrence (step.recurrenceLength (), (int) value);
            case Protocol.StepAttr.REPEAT_ENABLED -> step.setIsRepeatEnabled (on);
            case Protocol.StepAttr.REPEAT_COUNT -> step.setRepeatCount ((int) value);
            case Protocol.StepAttr.REPEAT_CURVE -> step.setRepeatCurve (value);
            case Protocol.StepAttr.REPEAT_VELOCITY_CURVE -> step.setRepeatVelocityCurve (value);
            case Protocol.StepAttr.REPEAT_VELOCITY_END -> step.setRepeatVelocityEnd (value);
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
