// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Java's formatting of numbers (`Float.toString`, `Double.toString`), for the values which the
//! OSC modules convert to text (`value.toString ()`) and for the log.

/// Java `Float.toString`: the shortest digits which identify the value (as Rust's), in decimal
/// notation with at least one fractional digit for 10^-3 <= |x| < 10^7, otherwise in Java's
/// scientific notation (`1.0E10`, `1.234E-5`).
pub fn float_to_string(value: f32) -> String {
    format_java(f64::from(value), &format!("{value:e}"))
}

/// Java `Double.toString` (see [`float_to_string`]).
pub fn double_to_string(value: f64) -> String {
    format_java(value, &format!("{value:e}"))
}

/// Format a value from its shortest scientific notation (Rust `{:e}`, e.g. `-1.25e-3`).
fn format_java(value: f64, scientific: &str) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0.0" } else { "0.0" }.to_string();
    }
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    let magnitude = value.abs();
    if (1e-3..1e7).contains(&magnitude) {
        // Decimal notation: the point is after (exponent + 1) digits
        let point = exponent + 1;
        let text = if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else {
            let point = point as usize;
            if digits.len() > point {
                format!("{}.{}", &digits[..point], &digits[point..])
            } else {
                format!("{digits}{}.0", "0".repeat(point - digits.len()))
            }
        };
        return format!("{sign}{text}");
    }
    let fraction = if digits.len() > 1 { &digits[1..] } else { "0" };
    format!("{sign}{}.{fraction}E{exponent}", &digits[..1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats() {
        assert_eq!(float_to_string(1.0), "1.0");
        assert_eq!(float_to_string(0.5), "0.5");
        assert_eq!(float_to_string(-2.5), "-2.5");
        assert_eq!(float_to_string(120.0), "120.0");
        assert_eq!(float_to_string(0.1), "0.1");
        assert_eq!(float_to_string(0.001), "0.001");
        assert_eq!(float_to_string(0.0001), "1.0E-4");
        assert_eq!(float_to_string(1.234e-5), "1.234E-5");
        assert_eq!(float_to_string(1234567.0), "1234567.0");
        assert_eq!(float_to_string(1.0e7), "1.0E7");
        assert_eq!(float_to_string(1.5e10), "1.5E10");
        assert_eq!(float_to_string(0.0), "0.0");
        assert_eq!(float_to_string(-0.0), "-0.0");
        assert_eq!(float_to_string(f32::NAN), "NaN");
        assert_eq!(float_to_string(f32::INFINITY), "Infinity");
        assert_eq!(float_to_string(f32::NEG_INFINITY), "-Infinity");
        assert_eq!(float_to_string(1.1), "1.1");
        assert_eq!(float_to_string(3.4028235e38), "3.4028235E38");
    }

    #[test]
    fn doubles() {
        assert_eq!(double_to_string(1.0), "1.0");
        assert_eq!(double_to_string(0.1), "0.1");
        assert_eq!(double_to_string(123.456), "123.456");
        assert_eq!(double_to_string(1e-10), "1.0E-10");
        assert_eq!(double_to_string(1e21), "1.0E21");
        assert_eq!(double_to_string(9999999.0), "9999999.0");
        assert_eq!(double_to_string(-1e-3), "-0.001");
    }
}
