// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Simulates a fader with a column of 8 pads (former `IVirtualFader`, `VirtualFaderImpl` and
//! `IVirtualFaderCallback`).
//!
//! The fader is shared as `Rc<RefCell<VirtualFader>>` since moving it to a destination is done
//! in several steps by scheduled tasks.

use std::cell::RefCell;
use std::rc::Rc;

use super::ctx::Ctx;
use super::grid::PadGrid;

/// Gets and sets the value which is controlled by a virtual fader (former
/// `IVirtualFaderCallback`).
pub trait VirtualFaderCallback {
    /// Get the current value (0-127).
    fn value(&self, ctx: &Ctx) -> i32;

    /// Set a new value (0-127).
    fn set_value(&self, ctx: &mut Ctx, value: i32);
}

/// A view which provides the values for a row of indexed virtual faders, e.g. the Launchpad
/// fader views (former `AbstractFaderView.getFaderValue/onValueKnob`). Views return themselves
/// from [`crate::framework::feature::View::as_virtual_fader_view`].
pub trait VirtualFaderView {
    /// Get the value of the fader with the given index.
    fn fader_value(&self, ctx: &Ctx, index: usize) -> i32;

    /// Set the value of the fader with the given index.
    fn on_value_knob(&self, ctx: &mut Ctx, index: usize, value: i32);
}

/// Adapts a [`VirtualFaderView`] and a fader index to a [`VirtualFaderCallback`].
pub struct VirtualFaderViewAdapter<'a> {
    pub view: &'a dyn VirtualFaderView,
    pub index: usize,
}

impl VirtualFaderCallback for VirtualFaderViewAdapter<'_> {
    fn value(&self, ctx: &Ctx) -> i32 {
        self.view.fader_value(ctx, self.index)
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        self.view.on_value_knob(ctx, self.index, value);
    }
}

const PAD_VALUE_AMOUNT: i32 = 16;

#[rustfmt::skip]
const SPEED_SCALE: [i32; 128] = [
    1,   1,  1,  1,  1,  1,  1,  1,
    1,   1,  1,  1,  1,  1,  1,  1,
    1,   1,  1,  1,  1,  1,  1,  1,
    2,   2,  2,  2,  2,  2,  2,  2,
    2,   2,  2,  2,  2,  2,  2,  2,
    2,   2,  2,  2,  2,  2,  2,  2,
    3,   3,  3,  3,  3,  3,  3,  3,
    3,   3,  3,  3,  3,  3,  3,  3,
    4,   4,  4,  4,  4,  4,  4,  4,
    5,   5,  5,  5,  5,  5,  5,  5,
    6,   6,  6,  6,  7,  7,  7,  7,
    8,   8,  8,  8,  9,  9,  9,  9,
    10, 10, 10, 10, 11, 11, 11, 11,
    12, 12, 12, 12, 13, 13, 13, 13,
    14, 14, 15, 15, 16, 16, 17, 17,
    18, 19, 20, 21, 22, 23, 24, 25,
];

const LOOP_DELAY: i32 = 6;

/// Java's `Math.round`.
fn java_round(value: f64) -> i32 {
    (value + 0.5).floor() as i32
}

/// A fader simulated by 8 pads (former `VirtualFaderImpl`).
pub struct VirtualFader {
    callback: Rc<dyn VirtualFaderCallback>,
    /// The column of the pad grid to draw the fader into, `None` for no drawing.
    grid_column: Option<i32>,
    color: i32,
    is_panning: bool,
    color_states: [i32; 8],
    move_delay: i32,
    move_timer_delay: i32,
    move_destination: i32,
    move_target_value: i32,
    move_direction_is_upwards: bool,
    is_knob_type: bool,
}

impl VirtualFader {
    /// Constructor. If a grid column is given, [`VirtualFader::set_value`] draws the fader
    /// into that column of the pad grid.
    pub fn new(callback: Rc<dyn VirtualFaderCallback>, grid_column: Option<i32>) -> Self {
        VirtualFader {
            callback,
            grid_column,
            color: 0,
            is_panning: false,
            color_states: [0; 8],
            move_delay: 0,
            move_timer_delay: 0,
            move_destination: 0,
            move_target_value: 0,
            move_direction_is_upwards: true,
            is_knob_type: false,
        }
    }

    /// Wrap the fader for sharing.
    pub fn shared(self) -> Rc<RefCell<VirtualFader>> {
        Rc::new(RefCell::new(self))
    }

    /// Get the color of one of the 8 pads (0 = bottom).
    pub fn color_state(&self, index: usize) -> i32 {
        self.color_states[index]
    }

    /// Set the color of the fader and if it is a panorama fader (drawn from the center).
    pub fn setup(&mut self, color: i32, is_pan: bool) {
        self.color = color;
        self.is_panning = is_pan;
    }

    /// Set the value to display (0-127) and draw the fader into the given pad grid (if a grid
    /// column was given to the constructor).
    pub fn set_value(&mut self, grid: Option<&mut PadGrid>, value: i32) {
        self.color_states = [0; 8];
        if self.is_panning {
            self.draw_panning(value);
        } else {
            self.draw_fader(value);
        }
        if let (Some(grid), Some(column)) = (grid, self.grid_column) {
            for i in 0..8 {
                grid.light_ex(column, i as i32, self.color_states[7 - i].max(0));
            }
        }
    }

    /// Compute the new destination for a press on a pad of the fader.
    fn calc_destination(&mut self, current_value: i32, row: i32, velocity: i32) {
        let velocity = velocity.clamp(0, 127) as usize;
        // About 3 seconds on softest velocity
        self.move_delay = SPEED_SCALE[velocity] * LOOP_DELAY;
        self.move_timer_delay = SPEED_SCALE[SPEED_SCALE.len() - 1 - velocity] * LOOP_DELAY;
        // Compensate for parameter type detection delay
        self.move_timer_delay -= 1;
        // Reset parameter type detection flag
        self.is_knob_type = false;

        // Calculate the bounds of the destination pad
        let min = row * PAD_VALUE_AMOUNT;
        let max = 127.min((row + 1) * PAD_VALUE_AMOUNT - 1);

        let mut new_destination = self.smooth_fader_value(current_value, row, max);

        // Support stepping through 4 values on the destination pad
        if min <= self.move_destination && self.move_destination <= max {
            let step = (self.move_destination - min) / 4;
            if self.move_direction_is_upwards {
                new_destination = min + 4 * (step + 2) - 1;
                if new_destination > max {
                    new_destination = min + 4 * step - 1;
                    self.move_direction_is_upwards = false;
                }
            } else {
                new_destination = min + 4 * (step - 1) + 1;
                if new_destination < min {
                    new_destination = min + 4 * (step + 2) - 1;
                    self.move_direction_is_upwards = true;
                }
            }
        } else if row == 0 {
            new_destination = 0;
        }

        self.move_destination = new_destination;
    }

    fn smooth_fader_value(&self, old_value: i32, row: i32, value: i32) -> i32 {
        if self.is_panning && (row == 3 || row == 4) {
            return 64;
        }
        if row == 0 {
            return if old_value == 0 { PAD_VALUE_AMOUNT - 1 } else { 0 };
        }
        value
    }

    fn draw_fader(&mut self, value: i32) {
        let pos = 8.0 * value as f64 / 127.0;
        let num_pads = pos as i32;
        let fine = java_round(3.0 * (pos - num_pads as f64));
        for i in 0..8 {
            // Four velocity colors
            if i < num_pads {
                self.color_states[i as usize] = self.color;
            } else if i == num_pads {
                // First 3 colors are supposed to be shades of grey
                self.color_states[i as usize] = if fine == 3 { self.color } else { fine };
            }
        }
    }

    fn draw_panning(&mut self, value: i32) {
        // Centered
        if value == 64 {
            self.color_states[3] = self.color;
            self.color_states[4] = self.color;
            return;
        }

        // Pan to the left / bottom
        if value < 64 {
            let pos = 4.0 * value as f64 / 64.0 - 1.0;
            let num_pads = pos as i32;
            let fine = 3 - java_round(3.0 * (pos - num_pads as f64)).abs();
            for i in 0..4 {
                if i - 1 > num_pads {
                    self.color_states[i as usize] = self.color;
                } else if i - 1 == num_pads {
                    // First 3 colors are supposed to be shades of grey
                    self.color_states[i as usize] = if fine == 3 { self.color } else { fine };
                }
            }
            return;
        }

        // Pan to the right / top
        let pos = 4.0 * (value - 64) as f64 / 64.0;
        let num_pads = pos as i32;
        let fine = java_round(3.0 * (pos - num_pads as f64));
        for i in 4..8 {
            if i - 4 < num_pads {
                self.color_states[i as usize] = self.color;
            } else if i - 4 == num_pads {
                // First 3 colors are supposed to be shades of grey
                self.color_states[i as usize] = if fine == 3 { self.color } else { fine };
            }
        }
    }
}

/// Move the fader to the value of the given row (0 = bottom). The fader moves in steps, the
/// speed depends on the velocity.
///
/// `direct` replaces the callback of the fader for the first (synchronous) step. It is required
/// if the callback of the fader needs access to an object which is currently borrowed by the
/// caller (e.g. the fader view itself, which calls this function from `on_grid_note`). The
/// following steps are executed by scheduled tasks with the callback of the fader.
pub fn move_to(fader: &Rc<RefCell<VirtualFader>>, ctx: &mut Ctx, row: i32, velocity: i32, direct: Option<&dyn VirtualFaderCallback>) {
    let callback = fader.borrow().callback.clone();
    let callback: &dyn VirtualFaderCallback = direct.unwrap_or(&*callback);
    let current = callback.value(ctx);
    fader.borrow_mut().calc_destination(current, row, velocity);
    move_fader_to_destination(fader, ctx, callback);
}

fn move_fader_to_destination(fader: &Rc<RefCell<VirtualFader>>, ctx: &mut Ctx, callback: &dyn VirtualFaderCallback) {
    let current = callback.value(ctx);
    let target = {
        let mut f = fader.borrow_mut();
        if current == f.move_destination {
            return;
        }
        f.move_direction_is_upwards = current < f.move_destination;
        f.move_target_value = if f.move_direction_is_upwards {
            (current + f.move_delay).min(f.move_destination)
        } else {
            (current - f.move_delay).max(f.move_destination)
        };
        f.move_target_value
    };
    callback.set_value(ctx, target);

    // Delay to allow the parameter value to update properly
    let fader = fader.clone();
    ctx.schedule(LOOP_DELAY as u64, move |ctx| move_fader_to_destination_callback(&fader, ctx));
}

fn move_fader_to_destination_callback(fader: &Rc<RefCell<VirtualFader>>, ctx: &mut Ctx) {
    let callback = fader.borrow().callback.clone();
    let updated_value = callback.value(ctx);

    // Compare updated parameter value to target update value, if it is different the parameter
    // is either a boolean or selection list type and the destination value should be force set
    // It seems that setting USER parameters is slower and LOOP_DELAY needs to be at least 6!
    let (is_knob_type, target, destination, timer_delay) = {
        let f = fader.borrow();
        (f.is_knob_type, f.move_target_value, f.move_destination, f.move_timer_delay)
    };
    if !is_knob_type && updated_value != target {
        ctx.host.println("FORCED!");
        callback.set_value(ctx, destination);
        return;
    }

    fader.borrow_mut().is_knob_type = true;
    let fader = fader.clone();
    ctx.schedule(timer_delay.max(0) as u64, move |ctx| {
        let callback = fader.borrow().callback.clone();
        move_fader_to_destination(&fader, ctx, &*callback);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;

    impl VirtualFaderCallback for Dummy {
        fn value(&self, _ctx: &Ctx) -> i32 {
            0
        }

        fn set_value(&self, _ctx: &mut Ctx, _value: i32) {}
    }

    fn fader() -> VirtualFader {
        let mut fader = VirtualFader::new(Rc::new(Dummy), None);
        fader.setup(5, false);
        fader
    }

    #[test]
    fn draw_fader() {
        let mut f = fader();
        f.set_value(None, 0);
        assert_eq!(f.color_states, [0; 8]);
        f.set_value(None, 127);
        assert_eq!(f.color_states, [5; 8]);
        // 8 * 64 / 127 = 4.03 -> 4 pads, fine 0
        f.set_value(None, 64);
        assert_eq!(f.color_states, [5, 5, 5, 5, 0, 0, 0, 0]);
        // 8 * 40 / 127 = 2.52 -> 2 pads, fine = round (1.56) = 2
        f.set_value(None, 40);
        assert_eq!(f.color_states, [5, 5, 2, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn draw_panning() {
        let mut f = fader();
        f.setup(5, true);
        f.set_value(None, 64);
        assert_eq!(f.color_states, [0, 0, 0, 5, 5, 0, 0, 0]);
        f.set_value(None, 0);
        assert_eq!(f.color_states, [5, 5, 5, 5, 0, 0, 0, 0]);
        f.set_value(None, 127);
        // pos = 3.94 -> 3 pads, fine = round (2.81) = 3
        assert_eq!(f.color_states, [0, 0, 0, 0, 5, 5, 5, 5]);
    }

    #[test]
    fn destination() {
        let mut f = fader();
        f.calc_destination(0, 7, 127);
        assert_eq!(f.move_destination, 127);
        assert_eq!(f.move_delay, 25 * LOOP_DELAY);
        f.calc_destination(127, 0, 127);
        assert_eq!(f.move_destination, 0);
        f.calc_destination(0, 3, 1);
        assert_eq!(f.move_destination, 63);
        // Pressing the same pad again steps through the values of the pad
        f.move_direction_is_upwards = false;
        f.calc_destination(63, 3, 1);
        assert_eq!(f.move_destination, 57);
    }
}
