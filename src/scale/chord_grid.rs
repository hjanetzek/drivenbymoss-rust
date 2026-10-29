// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A chord grid for a scale (former `ChordGrid`).

use super::Scale;

/// A matrix for a pad grid where each column contains one note of the scale (ascending, wrapping
/// into the next octave), repeated in all rows. Values are semitone offsets from the base note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordGrid {
    matrix: Vec<i32>,
}

impl ChordGrid {
    /// Constructor.
    pub fn new(scale: Scale, rows: usize, cols: usize) -> Self {
        let mut matrix = vec![0; cols * rows];
        let intervals = scale.intervals();
        let len = intervals.len();
        for column in 0..cols {
            let oct = (column / len) as i32;
            let note = oct * 12 + intervals[column % len];
            for row in 0..rows {
                matrix[row * cols + column] = note;
            }
        }
        ChordGrid { matrix }
    }

    /// Get the matrix (`rows * cols` entries, row major).
    pub fn matrix(&self) -> &[i32] {
        &self.matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_grid() {
        let g = ChordGrid::new(Scale::MajorPentatonic, 2, 8);
        assert_eq!(&g.matrix()[0..8], &[0, 2, 4, 7, 9, 12, 14, 16]);
        assert_eq!(&g.matrix()[8..16], &[0, 2, 4, 7, 9, 12, 14, 16]);
    }
}
