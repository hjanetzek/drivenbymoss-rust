// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Displays a 3 digit number in the upper part of the grid (former `AbstractNumberDisplayView`,
//! `TempoView` and `ShuffleView`).

use crate::daw::constants::GrooveParameterId;
use crate::framework::ctx::Ctx;
use crate::framework::view::base::ViewBase;

#[rustfmt::skip]
const TWO_COLS: [[[bool; 2]; 5]; 10] = [
    [[false, false], [false, false], [false, false], [false, false], [false, false]],
    [[true, false], [true, false], [true, false], [true, false], [true, false]],
    [[true, true], [false, true], [true, true], [true, false], [true, true]],
    [[true, true], [false, true], [true, true], [false, true], [true, true]],
    [[true, false], [true, false], [true, true], [false, true], [false, true]],
    [[true, true], [true, false], [true, true], [false, true], [true, true]],
    [[true, true], [true, false], [true, true], [true, true], [true, true]],
    [[true, true], [false, true], [false, true], [false, true], [false, true]],
    [[true, true], [true, true], [true, true], [true, true], [true, true]],
    [[true, true], [true, true], [true, true], [false, true], [true, true]],
];

/// The index of the empty digit in [`THREE_COLS`].
const BLANK: usize = 10;

#[rustfmt::skip]
const THREE_COLS: [[[bool; 3]; 5]; 11] = [
    [[true, true, true], [true, false, true], [true, false, true], [true, false, true], [true, true, true]],
    [[false, true, false], [false, true, false], [false, true, false], [false, true, false], [false, true, false]],
    [[true, true, true], [false, false, true], [true, true, true], [true, false, false], [true, true, true]],
    [[true, true, true], [false, false, true], [true, true, true], [false, false, true], [true, true, true]],
    [[true, false, false], [true, false, false], [true, true, true], [false, true, false], [false, true, false]],
    [[true, true, true], [true, false, false], [true, true, true], [false, false, true], [true, true, true]],
    [[true, true, true], [true, false, false], [true, true, true], [true, false, true], [true, true, true]],
    [[true, true, true], [false, false, true], [false, false, true], [false, false, true], [false, false, true]],
    [[true, true, true], [true, false, true], [true, true, true], [true, false, true], [true, true, true]],
    [[true, true, true], [true, false, true], [true, true, true], [false, false, true], [true, true, true]],
    [[false, false, false], [false, false, false], [false, false, false], [false, false, false], [false, false, false]],
];

/// The state of a number display view.
pub struct NumberDisplayViewBase {
    pub base: ViewBase,
    text_color1: i32,
    text_color2: i32,
    background_color: i32,
}

impl NumberDisplayViewBase {
    pub fn new(name: &str, text_color1: i32, text_color2: i32, background_color: i32) -> Self {
        NumberDisplayViewBase { base: ViewBase::new(name), text_color1, text_color2, background_color }
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.clear_note_mapping(ctx);
    }

    /// Draw the 3 digit number in the upper part of the grid. Call [`Self::fill_bottom`]
    /// afterwards (former template method).
    pub fn draw_number(&self, ctx: &mut Ctx, number: i32) {
        let value = number.clamp(0, 999);
        let grid = ctx.surface.grid();
        let mut draw_digit = |x_offset: i32, rows: &[&[bool]], color: i32| {
            for (y, row) in rows.iter().enumerate() {
                for (x, &on) in row.iter().enumerate() {
                    grid.light_ex(x_offset + x as i32, y as i32, if on { color } else { self.background_color });
                }
            }
        };

        // 1st digit (two columns) in color 1
        let digit1 = (value / 100) as usize;
        let first: Vec<&[bool]> = TWO_COLS[digit1].iter().map(|r| &r[..]).collect();
        draw_digit(0, &first, self.text_color1);

        // 2nd digit (three columns) in color 2
        let digit2 = (value % 100 / 10) as usize;
        let show_2nd_digit = digit1 > 0 || digit2 > 0;
        let second: Vec<&[bool]> = THREE_COLS[if show_2nd_digit { digit2 } else { BLANK }].iter().map(|r| &r[..]).collect();
        draw_digit(2, &second, self.text_color2);

        // 3rd digit (three columns) in color 1
        let third: Vec<&[bool]> = THREE_COLS[(value % 10) as usize].iter().map(|r| &r[..]).collect();
        draw_digit(5, &third, self.text_color1);
    }

    /// Fill the rows below the number with the background color.
    pub fn fill_bottom(&self, ctx: &mut Ctx) {
        let grid = ctx.surface.grid();
        for x in 0..grid.cols() {
            for y in 5..grid.rows() {
                grid.light_ex(x, y, self.background_color);
            }
        }
    }

    /// Releasing a pad returns to the previous view.
    pub fn on_grid_note(&mut self, ctx: &mut Ctx, _note: i32, velocity: i32) {
        if velocity == 0 {
            crate::framework::view::deferred::restore_view_later(ctx);
        }
    }
}

/// The number of the tempo view (former `TempoView.getNumber`).
pub fn tempo_number(ctx: &Ctx) -> i32 {
    ctx.model.transport().tempo().round() as i32
}

/// The number of the shuffle view (former `ShuffleView.getNumber`).
pub fn shuffle_number(ctx: &Ctx) -> i32 {
    let max = ctx.model.value_changer().upper_bound() - 1;
    let parameter = ctx.model.groove().parameter(GrooveParameterId::ShuffleAmount);
    if !parameter.does_exist() || max <= 0 {
        return 0;
    }
    (parameter.value() as f64 * 100.0 / max as f64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits() {
        assert_eq!(TWO_COLS[1][0], [true, false]);
        assert_eq!(THREE_COLS[0][1], [true, false, true]);
        assert_eq!(THREE_COLS[7][4], [false, false, true]);
    }
}
