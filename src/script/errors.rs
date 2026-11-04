// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The error policy of the script calls: an error is logged with the script and the line, the
//! call has no effect, and a function which fails three times in a row is disabled, so that a
//! broken script cannot flood the log.

use std::collections::{HashMap, HashSet};

/// The number of errors in a row after which a function is disabled.
pub const MAX_ERRORS_IN_A_ROW: u32 = 3;

/// The error counts of the functions of a script (identified by a key, e.g. the address of the
/// function in the engine).
#[derive(Debug, Default)]
pub struct ErrorPolicy {
    in_a_row: HashMap<usize, u32>,
    disabled: HashSet<usize>,
}

impl ErrorPolicy {
    pub fn is_disabled(&self, key: usize) -> bool {
        self.disabled.contains(&key)
    }

    /// The function succeeded: its count starts again.
    pub fn succeeded(&mut self, key: usize) {
        if !self.in_a_row.is_empty() {
            self.in_a_row.remove(&key);
        }
    }

    /// The function failed. Returns true if it is disabled now.
    pub fn failed(&mut self, key: usize) -> bool {
        let count = self.in_a_row.entry(key).or_insert(0);
        *count += 1;
        if *count >= MAX_ERRORS_IN_A_ROW {
            self.in_a_row.remove(&key);
            self.disabled.insert(key);
            return true;
        }
        false
    }
}

/// The first line of an engine error message without the stack traceback, e.g.
/// `launch_control_xl.lua:12: attempt to index a nil value`.
pub fn short_message(message: &str) -> String {
    let message = message.split("\nstack traceback:").next().unwrap_or(message);
    message.trim().replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_in_a_row_disable() {
        let mut policy = ErrorPolicy::default();
        assert!(!policy.failed(1));
        assert!(!policy.failed(1));
        policy.succeeded(1);
        assert!(!policy.failed(1));
        assert!(!policy.failed(1));
        assert!(policy.failed(1));
        assert!(policy.is_disabled(1));
        assert!(!policy.is_disabled(2));
        assert_eq!(short_message("a.lua:3: x\nstack traceback:\n\t[C]: in ?"), "a.lua:3: x");
    }
}
