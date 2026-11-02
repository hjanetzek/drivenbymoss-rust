// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Converts between hardware, DAW and display value ranges (former `IValueChanger`).

/// The encoding of relative knob values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeEncoding {
    /// Positive values 1..63 increase, 65..127 (= -63..-1) decrease.
    TwosComplement,
    /// 64 is zero, above increases, below decreases (former `OffsetBinaryRelativeValueChanger`).
    OffsetBinary,
    /// 65..127 increase by 1..63, 1..63 decrease by 1..63 (former
    /// `SignedBitRelativeValueChanger`).
    SignedBit,
    /// 1..63 increase by 1..63, 65..127 decrease by 1..63 (former
    /// `SignedBit2RelativeValueChanger`).
    SignedBit2,
}

impl RelativeEncoding {
    /// All encodings, in the order of the Java enum.
    pub const ALL: [RelativeEncoding; 4] = [RelativeEncoding::TwosComplement, RelativeEncoding::OffsetBinary, RelativeEncoding::SignedBit, RelativeEncoding::SignedBit2];
}

/// Value changer with a configurable upper bound (the DAW value range is [0..upper_bound-1]).
#[derive(Debug, Clone)]
pub struct ValueChanger {
    upper_bound: i32,
    step_size: i32,
    sensitivity: f64,
    encoding: RelativeEncoding,
}

impl ValueChanger {
    pub fn twos_complement(upper_bound: i32, step_size: i32) -> Self {
        Self::new(RelativeEncoding::TwosComplement, upper_bound, step_size)
    }

    /// A value changer with the given relative encoding (former `TwosComplementValueChanger`,
    /// `OffsetBinaryRelativeValueChanger`, `SignedBitRelativeValueChanger`,
    /// `SignedBit2RelativeValueChanger`).
    pub fn new(encoding: RelativeEncoding, upper_bound: i32, step_size: i32) -> Self {
        ValueChanger { upper_bound, step_size, sensitivity: 1.0, encoding }
    }

    /// The value changers of `RelativeValueChangers.get`: the range 0..127, step size 1.
    pub fn relative(encoding: RelativeEncoding) -> Self {
        Self::new(encoding, 128, 1)
    }

    pub fn encoding(&self) -> RelativeEncoding {
        self.encoding
    }

    pub fn upper_bound(&self) -> i32 {
        self.upper_bound
    }

    pub fn set_upper_bound(&mut self, upper_bound: i32) {
        self.upper_bound = upper_bound;
    }

    pub fn step_size(&self) -> i32 {
        self.step_size
    }

    pub fn set_step_size(&mut self, step_size: i32) {
        self.step_size = step_size;
    }

    pub fn set_sensitivity(&mut self, sensitivity: f64) {
        self.sensitivity = sensitivity;
    }

    pub fn calc_knob_change(&self, control: i32) -> f64 {
        self.calc_knob_change_with(control, self.sensitivity)
    }

    pub fn calc_knob_change_with(&self, control: i32, sensitivity: f64) -> f64 {
        self.decode(control) as f64 * self.step_size as f64 * Self::rescale(sensitivity)
    }

    pub fn calc_stepped_knob_change(&self, control: i32) -> i32 {
        let result = self.calc_knob_change_with(control, -100.0);
        if result >= 0.0 {
            1.max(result.round() as i32)
        } else {
            (-1).min(result.round() as i32)
        }
    }

    pub fn decode(&self, control: i32) -> i32 {
        match self.encoding {
            RelativeEncoding::TwosComplement => {
                if control < 64 {
                    control
                } else {
                    control - 128
                }
            }
            RelativeEncoding::OffsetBinary => control - 64,
            RelativeEncoding::SignedBit => {
                if control == 64 {
                    0
                } else if control > 64 {
                    control - 64
                } else {
                    -control
                }
            }
            RelativeEncoding::SignedBit2 => {
                if control < 64 {
                    control
                } else {
                    64 - control
                }
            }
        }
    }

    pub fn encode(&self, speed: i32) -> i32 {
        match self.encoding {
            RelativeEncoding::TwosComplement => {
                if speed < 0 {
                    speed + 128
                } else {
                    speed
                }
            }
            RelativeEncoding::OffsetBinary => speed + 64,
            RelativeEncoding::SignedBit => {
                if speed < 0 {
                    -speed
                } else {
                    speed + 64
                }
            }
            RelativeEncoding::SignedBit2 => {
                if speed <= 0 {
                    64 + speed.abs()
                } else {
                    speed
                }
            }
        }
    }

    pub fn is_increase(&self, control: i32) -> bool {
        self.calc_knob_change(control) > 0.0
    }

    pub fn change_value(&self, control: i32, value: i32) -> i32 {
        self.change_value_with(control, value, self.sensitivity, self.upper_bound)
    }

    pub fn change_value_with(&self, control: i32, value: i32, sensitivity: f64, upper_bound: i32) -> i32 {
        let speed = self.calc_knob_change_with(control, sensitivity);
        (value as f64 + speed).clamp(0.0, upper_bound as f64 - 1.0) as i32
    }

    pub fn to_midi_value(&self, value: i32) -> i32 {
        (value * 127 / (self.upper_bound - 1)).min(127)
    }

    pub fn to_daw_value(&self, value: i32) -> i32 {
        value * (self.upper_bound - 1) / 127
    }

    pub fn to_display_value(&self, value: i32) -> i32 {
        value
    }

    pub fn to_normalized_value(&self, value: f64) -> f64 {
        (value / (self.upper_bound - 1) as f64).min(1.0)
    }

    pub fn from_normalized_value(&self, value: f64) -> i32 {
        (value * (self.upper_bound - 1) as f64).round() as i32
    }

    pub fn rescale(sensitivity: f64) -> f64 {
        if sensitivity < 0.0 {
            0.1f64.max((100.0 + sensitivity) / 100.0)
        } else {
            1.0 + sensitivity / 100.0 * 9.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        let vc = ValueChanger::twos_complement(128, 1);
        assert_eq!(vc.decode(127), -1);
        assert_eq!(vc.encode(-1), 127);
        assert_eq!(vc.to_midi_value(127), 127);
        assert_eq!(vc.from_normalized_value(1.0), 127);
        assert_eq!(vc.change_value(1, 126), 127);
        assert_eq!(vc.change_value(1, 127), 127);
        assert_eq!(vc.change_value(127, 0), 0);
    }

    #[test]
    fn relative_encodings() {
        let ob = ValueChanger::relative(RelativeEncoding::OffsetBinary);
        assert_eq!((ob.decode(65), ob.decode(63), ob.encode(-1)), (1, -1, 63));
        let sb = ValueChanger::relative(RelativeEncoding::SignedBit);
        assert_eq!((sb.decode(65), sb.decode(1), sb.decode(64), sb.encode(-1), sb.encode(1)), (1, -1, 0, 1, 65));
        let sb2 = ValueChanger::relative(RelativeEncoding::SignedBit2);
        assert_eq!((sb2.decode(1), sb2.decode(65), sb2.encode(-1), sb2.encode(0), sb2.encode(1)), (1, -1, 65, 64, 1));
    }
}
