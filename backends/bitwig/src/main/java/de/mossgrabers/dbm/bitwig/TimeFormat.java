// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import com.bitwig.extension.controller.api.BeatTimeFormatter;

import java.util.Locale;


/**
 * The time and beat formatting of the Java DrivenByMoss for Bitwig (StringUtils.formatTimeLong
 * and formatMeasuresLong, TransportImpl.formatAsBeats), so that the controllers show the same
 * texts as with the Java version.
 */
public final class TimeFormat
{
    /** Formats a position as measures.quarters.eighths:ticks, starting with 1. */
    public static final BeatTimeFormatter BEAT_POSITION_FORMATTER = formatAsBeats (1);
    /** Formats a length as measures.quarters.eighths:ticks, starting with 0. */
    public static final BeatTimeFormatter BEAT_LENGTH_FORMATTER   = formatAsBeats (0);


    private TimeFormat ()
    {
        // Utility class
    }


    private static BeatTimeFormatter formatAsBeats (final int offset)
    {
        return (beatTime, isAbsolute, timeSignatureNumerator, timeSignatureDenominator, timeSignatureTicks) -> {
            final int quartersPerMeasure = 4 * timeSignatureNumerator / timeSignatureDenominator;
            return formatMeasuresLong (quartersPerMeasure, beatTime, offset);
        };
    }


    /**
     * Format beats as measures.quarters.eighths:ticks.
     *
     * @param quartersPerMeasure The number of quarters in a measure
     * @param beats The beats
     * @param startOffset 1 for a position, 0 for a length
     * @return The formatted text
     */
    public static String formatMeasuresLong (final int quartersPerMeasure, final double beats, final int startOffset)
    {
        final int measure = (int) Math.floor (beats / quartersPerMeasure);
        double t = beats - measure * quartersPerMeasure;
        final int quarters = (int) Math.floor (t);
        t = t - quarters;
        final int eights = (int) Math.floor (t / 0.25);
        t = t - eights * 0.25;
        final int frames = (int) Math.floor (t / 0.25 * 100.0);
        return String.format (Locale.ROOT, "%d.%02d.%02d:%02d", Integer.valueOf (measure + startOffset), Integer.valueOf (quarters + startOffset), Integer.valueOf (eights + startOffset), Integer.valueOf (frames));
    }


    /**
     * Format beats as time hours.minutes.seconds:millis at the given (constant) tempo.
     *
     * @param tempo The tempo
     * @param beats The beats
     * @return The formatted text
     */
    public static String formatTimeLong (final double tempo, final double beats)
    {
        final double time = beats * 60.0 / tempo;
        final int seconds = (int) Math.floor (time % 60);
        double t = (time - seconds) / 60.0;
        final int minutes = (int) Math.floor (t % 60);
        t = (t - minutes) / 60.0;
        final int hours = (int) Math.floor (t);
        final int millis = (int) ((time - ((hours * 60 + minutes) * 60 + seconds)) * 1000);
        return String.format (Locale.ROOT, "%d.%02d.%02d:%03d", Integer.valueOf (hours), Integer.valueOf (minutes), Integer.valueOf (seconds), Integer.valueOf (millis));
    }
}
