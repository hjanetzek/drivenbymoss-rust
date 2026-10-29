// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! String helpers used by the model (subset of the former `StringUtils`).

const REMOVABLE_CHARS: [char; 6] = [' ', 'e', 'a', 'u', 'i', 'o'];

/// Shortens a text to the given length by removing spaces and vowels first, then cutting it.
pub fn optimize_name(text: &str, length: usize) -> String {
    let mut shortened: Vec<char> = text.chars().collect();
    for element in REMOVABLE_CHARS {
        if shortened.len() <= length {
            return shortened.into_iter().collect();
        }
        while let Some(pos) = shortened.iter().position(|c| *c == element) {
            shortened.remove(pos);
            if shortened.len() <= length {
                return shortened.into_iter().collect();
            }
        }
    }
    shortened.into_iter().take(length).collect()
}

/// Limits a text to the given number of characters.
pub fn limit(text: &str, length: usize) -> String {
    text.chars().take(length).collect()
}

/// Removes the plugin type prefix (e.g. "VST: ") and the manufacturer suffix (e.g. " (Cockos)")
/// from a device name (mirrors the regular expression `(.+:)?\s*(.+?)\s*(\(.+\))?`).
pub fn remove_type_and_manufacturer(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    // Greedy prefix: up to the last ':' which still leaves at least one character
    let mut rest = name;
    if let Some(pos) = name.rfind(':') {
        let after = &name[pos + 1..];
        if !after.trim_start().is_empty() {
            rest = after.trim_start();
        } else if !after.is_empty() {
            rest = after;
        }
    }
    let rest_trimmed = rest.trim_end();
    // Lazy name: cut at the first '(' (not at the start) if the remainder ends with ')'
    if rest_trimmed.ends_with(')') {
        for (i, c) in rest_trimmed.char_indices() {
            if c == '(' && i > 0 && i + 1 < rest_trimmed.len() - 1 {
                let candidate = rest_trimmed[..i].trim_end();
                if !candidate.is_empty() {
                    return candidate.to_string();
                }
            }
        }
    }
    rest_trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(optimize_name("Hello World", 8), "HllWorld");
        assert_eq!(optimize_name("Drums", 8), "Drums");
        assert_eq!(remove_type_and_manufacturer("VST: ReaEQ (Cockos)"), "ReaEQ");
        assert_eq!(remove_type_and_manufacturer("VSTi: Foo (bar) (x64)"), "Foo");
        assert_eq!(remove_type_and_manufacturer("ReaComp"), "ReaComp");
        assert_eq!(limit("abcdef", 3), "abc");
    }
}
