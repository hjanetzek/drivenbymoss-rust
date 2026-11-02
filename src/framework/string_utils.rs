// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Text formatting helpers (parts of the former `StringUtils`).

/// Format a value (0..1) as a percentage with 1 fraction digit (former
/// `StringUtils.formatPercentage`).
pub fn format_percentage(value: f64) -> String {
    let percent = (value * 1000.0).round() / 10.0;
    if percent.fract() == 0.0 {
        format!("{}%", percent as i64)
    } else {
        format!("{percent:.1}%")
    }
}

/// Format the given time as measure.quarters.eights(:ticks) (former
/// `StringUtils.formatMeasures`).
pub fn format_measures(quarters_per_measure: i32, beats: f64, start_offset: i32, include_frames: bool) -> String {
    let quarters_per_measure = quarters_per_measure.max(1) as f64;
    let measure = (beats / quarters_per_measure).floor() as i32;
    let mut t = beats - measure as f64 * quarters_per_measure;
    let quarters = t.floor() as i32;
    t -= quarters as f64;
    let eights = (t / 0.25).floor() as i32;
    if !include_frames {
        return format!("{}.{}.{}", measure + start_offset, quarters + start_offset, eights + start_offset);
    }
    t -= eights as f64 * 0.25;
    let frames = (t / 0.25 * 100.0).floor() as i32;
    format!("{}.{}.{}:{frames:03}", measure + start_offset, quarters + start_offset, eights + start_offset)
}

/// Pad a text with spaces on the right or cut it to the given length (former
/// `StringUtils.pad`).
pub fn pad(text: &str, length: usize) -> String {
    let mut result: String = text.chars().take(length).collect();
    let count = result.chars().count();
    result.extend(std::iter::repeat_n(' ', length - count));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(format_percentage(0.5), "50%");
        assert_eq!(format_percentage(0.1234), "12.3%");
        assert_eq!(format_measures(4, 0.0, 0, true), "0.0.0:000");
        assert_eq!(format_measures(4, 5.5, 0, true), "1.1.2:000");
        assert_eq!(format_measures(4, 0.3, 0, true), "0.0.1:019");
        assert_eq!(format_measures(4, 5.5, 1, false), "2.2.3");
        assert_eq!(pad("abc", 5), "abc  ");
        assert_eq!(pad("abcdef", 3), "abc");
    }
}
