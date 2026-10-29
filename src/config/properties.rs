// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A Java compatible properties store (former `java.util.Properties` / `PropertiesEx`).
//!
//! Reads and writes the format of `Properties.load(Reader)` and `Properties.store(Writer, "")`
//! (no unicode escaping, UTF-8 text) so that the configuration files written by the Java version
//! can still be read and vice versa.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Key/value store with the semantics of the Java `PropertiesEx` class.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    entries: BTreeMap<String, String>,
}

impl Properties {
    pub fn new() -> Self {
        Properties::default()
    }

    /// Parse properties text (the result of `Properties.store`).
    pub fn parse(text: &str) -> Self {
        let mut properties = Properties::new();
        properties.load_str(text);
        properties
    }

    /// Load a properties file. A missing file results in empty properties.
    pub fn load_file(path: &Path) -> io::Result<Self> {
        match fs::read(path) {
            Ok(bytes) => Ok(Self::parse(&String::from_utf8_lossy(&bytes))),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Properties::new()),
            Err(err) => Err(err),
        }
    }

    /// Store all properties to a file (including the Java style header comments).
    pub fn save_file(&self, path: &Path) -> io::Result<()> {
        fs::write(path, self.to_text())
    }

    /// Add the properties from the given text, existing keys are overwritten.
    pub fn load_str(&mut self, text: &str) {
        for line in logical_lines(text) {
            let (key, value) = split_key_value(&line);
            self.entries.insert(unescape(key), unescape(value));
        }
    }

    /// Format all properties like `Properties.store (writer, "")`: an empty comment line, a date
    /// comment line and one `key=value` line per entry (sorted by key).
    pub fn to_text(&self) -> String {
        let mut text = String::from("#\n#");
        text.push_str(&java_date_string(SystemTime::now()));
        text.push('\n');
        for (key, value) in &self.entries {
            escape_into(&mut text, key, true);
            text.push('=');
            escape_into(&mut text, value, false);
            text.push('\n');
        }
        text
    }

    /// Get the raw value of a key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// Get a value, empty values are treated as missing (like `PropertiesEx.getString`).
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.get(key).filter(|value| !value.is_empty())
    }

    /// Set a value. An empty value removes the key (like `PropertiesEx.put`).
    pub fn put(&mut self, key: &str, value: &str) {
        if value.is_empty() {
            self.entries.remove(key);
        } else {
            self.entries.insert(key.to_string(), value.to_string());
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.entries.remove(key)
    }

    pub fn get_bool(&self, key: &str, default_value: bool) -> bool {
        // Java Boolean.parseBoolean: only "true" (ignoring case) is true
        self.get(key).map_or(default_value, |value| value.eq_ignore_ascii_case("true"))
    }

    pub fn put_bool(&mut self, key: &str, value: bool) {
        self.put(key, if value { "true" } else { "false" });
    }

    pub fn get_int(&self, key: &str, default_value: i32) -> i32 {
        self.get(key).and_then(|value| value.trim().parse().ok()).unwrap_or(default_value)
    }

    pub fn put_int(&mut self, key: &str, value: i32) {
        self.put(key, &value.to_string());
    }

    pub fn get_double(&self, key: &str, default_value: f64) -> f64 {
        self.get(key).and_then(|value| value.trim().parse().ok()).unwrap_or(default_value)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

fn is_whitespace(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\x0c'
}

/// Split the text into logical lines: comments and blank lines are removed, continuation lines
/// (ending with an odd number of backslashes) are joined.
fn logical_lines(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current: Option<String> = None;
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let line = raw.trim_start_matches(is_whitespace);
        let mut part = match current.take() {
            Some(previous) => {
                let mut joined = previous;
                joined.push_str(line);
                joined
            }
            None => {
                if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
                    continue;
                }
                line.to_string()
            }
        };
        let trailing = part.chars().rev().take_while(|&c| c == '\\').count();
        if trailing % 2 == 1 {
            part.pop();
            current = Some(part);
        } else {
            result.push(part);
        }
    }
    if let Some(rest) = current {
        result.push(rest);
    }
    result
}

/// Split a logical line into the (still escaped) key and value.
fn split_key_value(line: &str) -> (&str, &str) {
    let mut escaped = false;
    let mut key_end = line.len();
    let mut has_separator = false;
    for (i, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
        } else if c == '=' || c == ':' {
            key_end = i;
            has_separator = true;
            break;
        } else if is_whitespace(c) {
            key_end = i;
            break;
        }
    }
    let key = &line[..key_end];
    let mut rest = &line[key_end..];
    if has_separator {
        rest = &rest[1..];
    } else {
        rest = rest.trim_start_matches(is_whitespace);
        if rest.starts_with('=') || rest.starts_with(':') {
            rest = &rest[1..];
        }
    }
    (key, rest.trim_start_matches(is_whitespace))
}

fn unescape(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => result.push('\t'),
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('f') => result.push('\x0c'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(decoded) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    result.push(decoded);
                }
            }
            Some(other) => result.push(other),
            None => {}
        }
    }
    result
}

/// Mirrors `Properties.saveConvert` with `escUnicode = false`.
fn escape_into(out: &mut String, text: &str, is_key: bool) {
    for (i, c) in text.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            ' ' if i == 0 || is_key => out.push_str("\\ "),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\x0c' => out.push_str("\\f"),
            '=' | ':' | '#' | '!' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
}

/// Format a time like Java's `Date.toString ()` (in UTC), e.g. "Thu Oct 29 13:01:02 UTC 2026".
fn java_date_string(time: SystemTime) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    let secs = time.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86400);
    let secs_of_day = secs.rem_euclid(86400);

    // Civil from days (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);

    format!(
        "{} {} {:02} {:02}:{:02}:{:02} UTC {}",
        DAYS[days.rem_euclid(7) as usize],
        MONTHS[(month - 1) as usize],
        day,
        secs_of_day / 3600,
        secs_of_day % 3600 / 60,
        secs_of_day % 60,
        year
    )
}

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard Base64 encoding with padding (like `Base64.getEncoder ()`).
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(BASE64_CHARS[(n >> 18) as usize & 63] as char);
        out.push(BASE64_CHARS[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { BASE64_CHARS[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64_CHARS[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Standard Base64 decoding. Returns `None` if the text contains invalid characters.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_java_format() {
        let text = "#\n#Wed Oct 29 12:00:00 CET 2026\r\nSCALES_SCALE=Minor\n  ! comment\nKEY\\ WITH\\ SPACE = a\\=b\\:c\nWORKFLOW_INCLUDE_(GROUP-)MASTERTRACK_(REQUIRES_RESTART)=On\nMULTI=one\\\n    two\nUNI=\\u00e4\nCOLON:value\nSPACE value 2\nEMPTY=\n";
        let p = Properties::parse(text);
        assert_eq!(p.get("SCALES_SCALE"), Some("Minor"));
        assert_eq!(p.get("KEY WITH SPACE"), Some("a=b:c"));
        assert_eq!(p.get("WORKFLOW_INCLUDE_(GROUP-)MASTERTRACK_(REQUIRES_RESTART)"), Some("On"));
        assert_eq!(p.get("MULTI"), Some("onetwo"));
        assert_eq!(p.get("UNI"), Some("ä"));
        assert_eq!(p.get("COLON"), Some("value"));
        assert_eq!(p.get("SPACE"), Some("value 2"));
        assert_eq!(p.get("EMPTY"), Some(""));
        assert_eq!(p.get_string("EMPTY"), None);
        assert_eq!(p.len(), 8);
    }

    #[test]
    fn round_trip() {
        let mut p = Properties::new();
        p.put("A KEY", " leading space=#!:\\ \t");
        p.put("MIDI_INPUT0", "Launchpad X 1");
        p.put("UMLAUT", "Größe");
        p.put_bool("TAG_IS_ENABLED", true);
        p.put_int("NUM", -5);
        let text = p.to_text();
        assert!(text.starts_with("#\n#"));
        assert!(text.contains("MIDI_INPUT0=Launchpad X 1\n"));
        assert!(text.contains("A\\ KEY=\\ leading space\\=\\#\\!\\:\\\\ \\t\n"));
        let q = Properties::parse(&text);
        assert_eq!(p, q);
        assert!(q.get_bool("TAG_IS_ENABLED", false));
        assert_eq!(q.get_int("NUM", 0), -5);
    }

    #[test]
    fn put_empty_removes() {
        let mut p = Properties::new();
        p.put("X", "1");
        p.put("X", "");
        assert!(p.is_empty());
    }

    #[test]
    fn base64() {
        for sample in ["", "f", "fo", "foo", "foob", "fooba", "foobar", "Größe=\n"] {
            let encoded = base64_encode(sample.as_bytes());
            assert_eq!(base64_decode(&encoded).unwrap(), sample.as_bytes());
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_decode("Zm8"), Some(b"fo".to_vec()));
        assert_eq!(base64_decode("Z*"), None);
    }

    #[test]
    fn date_format() {
        // 2026-10-29 13:01:02 UTC is a Thursday
        let time = UNIX_EPOCH + std::time::Duration::from_secs(1_793_278_862);
        assert_eq!(java_date_string(time), "Thu Oct 29 13:01:02 UTC 2026");
        assert_eq!(java_date_string(UNIX_EPOCH), "Thu Jan 01 00:00:00 UTC 1970");
    }
}
