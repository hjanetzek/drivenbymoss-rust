// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The scale and chromatic matrices of a scale for a pad grid (former `ScaleGrid`).

use super::{Scale, ScaleLayout};

/// The orientation of a scale layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// The rows are shifted upwards.
    OrientUp,
    /// The columns are shifted to the right.
    OrientRight,
}

/// Contains the scale matrix and the chromatic matrix of a scale for a pad grid. The values are
/// semitone offsets relative to the base note (may be negative for centered layouts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaleGrid {
    scale: Scale,
    matrix: Vec<i32>,
    chromatic: Vec<i32>,
}

impl ScaleGrid {
    /// Constructor.
    ///
    /// * `shift` - the number of scale steps the notes in the next row (column) are shifted
    /// * `semitone_shift` - the number of semitones the notes in the next row (column) are shifted in
    ///   chromatic mode
    pub fn new(scale: Scale, layout: ScaleLayout, orientation: Orientation, rows: usize, cols: usize, shift: i32, semitone_shift: i32) -> Self {
        let size = cols * rows;
        let mut matrix = vec![0; size];
        let mut chromatic = vec![0; size];

        let intervals = scale.intervals();
        let len = intervals.len() as i32;
        let is_up = orientation == Orientation::OrientUp;

        let center_offset = if layout == ScaleLayout::EightUpCenter || layout == ScaleLayout::EightRightCenter { -3 } else { 0 };

        // Axis deltas, in scale steps
        let mut dx = 1;
        let mut dy = shift;

        // Fix 8th layout for scales which do not have 7 steps
        if shift == 7 {
            dy = len;
        }

        if layout == ScaleLayout::StaggeredUp || layout == ScaleLayout::StaggeredRight {
            dx = 2;
            // Staggered layout steps right by + 2, so on the row above a nice symmetry is produced
            // by octave - 2. Divide until shift is odd to evenly distribute the full scale and
            // ensure all steps are included.
            dy = len - 2;
            while dy > 1 && (dy & 1) == 0 {
                dy /= 2;
            }
        }

        for row in 0..rows {
            for column in 0..cols {
                let y = if is_up { row } else { column } as i32;
                let x = if is_up { column } else { row } as i32;
                let index = row * cols + column;

                let mut offset = y * dy + x * dx + center_offset;
                let mut oct = offset / len;

                // Fix negative values introduced by centerOffset
                if offset < 0 {
                    offset += len;
                    oct = offset / len - 1;
                }

                matrix[index] = oct * 12 + intervals[(offset % len) as usize];
                chromatic[index] = y * semitone_shift + x;
            }
        }

        ScaleGrid { scale, matrix, chromatic }
    }

    /// The name of the scale.
    pub fn name(&self) -> &'static str {
        self.scale.name()
    }

    /// The scale matrix (`rows * cols` entries, row major).
    pub fn matrix(&self) -> &[i32] {
        &self.matrix
    }

    /// The chromatic matrix (`rows * cols` entries, row major).
    pub fn chromatic(&self) -> &[i32] {
        &self.chromatic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourth_up() {
        let g = ScaleGrid::new(Scale::Major, ScaleLayout::FourthUp, Orientation::OrientUp, 8, 8, 3, 5);
        assert_eq!(&g.matrix()[0..8], &[0, 2, 4, 5, 7, 9, 11, 12]);
        assert_eq!(&g.matrix()[8..16], &[5, 7, 9, 11, 12, 14, 16, 17]);
        assert_eq!(g.matrix()[63], 48);
        assert_eq!(&g.chromatic()[8..11], &[5, 6, 7]);
        assert_eq!(g.name(), "Major");
    }

    #[test]
    fn staggered() {
        let g = ScaleGrid::new(Scale::Major, ScaleLayout::StaggeredUp, Orientation::OrientUp, 2, 4, 0, 5);
        // dx = 2, dy = 5
        assert_eq!(g.matrix(), &[0, 4, 7, 11, 9, 12, 16, 19]);
        // Whole tone: len 6 -> dy = 4 -> 2 -> 1
        let g = ScaleGrid::new(Scale::WholeTone, ScaleLayout::StaggeredUp, Orientation::OrientUp, 2, 2, 0, 5);
        assert_eq!(g.matrix(), &[0, 4, 2, 6]);
    }

    #[test]
    fn eight_centered() {
        let g = ScaleGrid::new(Scale::Major, ScaleLayout::EightUpCenter, Orientation::OrientUp, 2, 8, 7, 12);
        assert_eq!(&g.matrix()[0..8], &[-5, -3, -1, 0, 2, 4, 5, 7]);
        assert_eq!(&g.matrix()[8..16], &[7, 9, 11, 12, 14, 16, 17, 19]);
        // Pentatonic: dy = len = 5 -> next row is one octave up
        let g = ScaleGrid::new(Scale::MajorPentatonic, ScaleLayout::EightUp, Orientation::OrientUp, 2, 2, 7, 12);
        assert_eq!(g.matrix(), &[0, 2, 12, 14]);
        assert_eq!(g.chromatic(), &[0, 1, 12, 13]);
    }
}
