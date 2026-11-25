// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A minimal JSON writer for the data the C interface hands to hosts (the controller catalog and
//! the model setup), and a minimal reader for the JSON which devices send (the Electra One's
//! information replies, former Jackson `ObjectMapper.readValue (content, JsonNode.class)`), so
//! that the crate needs no external dependency.

/// Builds a JSON text. Commas are inserted automatically; the caller has to balance `begin_*`
/// and `end_*`.
#[derive(Default)]
pub struct JsonWriter {
    text: String,
    /// True if the next value of the current object or array needs a leading comma.
    needs_comma: bool,
}

impl JsonWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finish(self) -> String {
        self.text
    }

    fn separate(&mut self) {
        if self.needs_comma {
            self.text.push(',');
        }
        self.needs_comma = true;
    }

    pub fn begin_object(&mut self) -> &mut Self {
        self.separate();
        self.text.push('{');
        self.needs_comma = false;
        self
    }

    pub fn end_object(&mut self) -> &mut Self {
        self.text.push('}');
        self.needs_comma = true;
        self
    }

    pub fn begin_array(&mut self) -> &mut Self {
        self.separate();
        self.text.push('[');
        self.needs_comma = false;
        self
    }

    pub fn end_array(&mut self) -> &mut Self {
        self.text.push(']');
        self.needs_comma = true;
        self
    }

    /// Write the key of the next object member.
    pub fn key(&mut self, key: &str) -> &mut Self {
        self.separate();
        self.write_string(key);
        self.text.push(':');
        self.needs_comma = false;
        self
    }

    pub fn string(&mut self, value: &str) -> &mut Self {
        self.separate();
        self.write_string(value);
        self
    }

    pub fn number(&mut self, value: i64) -> &mut Self {
        self.separate();
        self.text.push_str(&value.to_string());
        self
    }

    pub fn null(&mut self) -> &mut Self {
        self.separate();
        self.text.push_str("null");
        self
    }

    pub fn boolean(&mut self, value: bool) -> &mut Self {
        self.separate();
        self.text.push_str(if value { "true" } else { "false" });
        self
    }

    pub fn string_array(&mut self, values: &[String]) -> &mut Self {
        self.begin_array();
        for value in values {
            self.string(value);
        }
        self.end_array()
    }

    fn write_string(&mut self, value: &str) {
        self.text.push('"');
        for c in value.chars() {
            match c {
                '"' => self.text.push_str("\\\""),
                '\\' => self.text.push_str("\\\\"),
                '\n' => self.text.push_str("\\n"),
                '\r' => self.text.push_str("\\r"),
                '\t' => self.text.push_str("\\t"),
                c if (c as u32) < 0x20 => self.text.push_str(&format!("\\u{:04x}", c as u32)),
                c => self.text.push(c),
            }
        }
        self.text.push('"');
    }
}

/// A parsed JSON value (former Jackson `JsonNode`). Object members keep their order.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// Parse a JSON text. Only whitespace may follow the value.
    pub fn parse(text: &str) -> Result<JsonValue, String> {
        let mut parser = JsonParser { chars: text.chars().collect(), pos: 0 };
        let value = parser.value(0)?;
        parser.skip_whitespace();
        if parser.pos < parser.chars.len() {
            return Err(format!("Unexpected character '{}' at {}", parser.chars[parser.pos], parser.pos));
        }
        Ok(value)
    }

    /// The member of an object (former `JsonNode.get (name)`), `None` for a missing member or if
    /// this is not an object.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The elements of an array (none if this is not an array).
    pub fn elements(&self) -> &[JsonValue] {
        match self {
            JsonValue::Array(elements) => elements,
            _ => &[],
        }
    }

    /// The value as text (former `JsonNode.asText`): a string as it is, a number or boolean in
    /// its JSON form, "null" for null, "" for an array or object.
    pub fn as_text(&self) -> String {
        match self {
            JsonValue::Null => "null".to_string(),
            JsonValue::Bool(value) => value.to_string(),
            JsonValue::Number(value) => {
                if value.fract() == 0.0 && value.abs() < 1e15 {
                    format!("{}", *value as i64)
                } else {
                    value.to_string()
                }
            }
            JsonValue::String(value) => value.clone(),
            JsonValue::Array(_) | JsonValue::Object(_) => String::new(),
        }
    }

    /// The value as an integer (former `JsonNode.asInt`): a number truncated, a string parsed
    /// (0 if it is not a number), true 1, everything else 0.
    pub fn as_int(&self) -> i64 {
        match self {
            JsonValue::Number(value) => *value as i64,
            JsonValue::String(value) => {
                let trimmed = value.trim();
                trimmed.parse::<i64>().ok().or_else(|| trimmed.parse::<f64>().ok().map(|v| v as i64)).unwrap_or(0)
            }
            JsonValue::Bool(value) => *value as i64,
            _ => 0,
        }
    }
}

/// The nesting depth up to which the reader descends (protects the stack against bad input).
const MAX_DEPTH: usize = 64;

struct JsonParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsonParser {
    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && matches!(self.chars[self.pos], ' ' | '\t' | '\n' | '\r') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn error<T>(&self, message: &str) -> Result<T, String> {
        Err(format!("{message} at {}", self.pos))
    }

    fn expect_word(&mut self, word: &str, value: JsonValue) -> Result<JsonValue, String> {
        for c in word.chars() {
            if self.peek() != Some(c) {
                return self.error(&format!("Expected '{word}'"));
            }
            self.pos += 1;
        }
        Ok(value)
    }

    fn value(&mut self, depth: usize) -> Result<JsonValue, String> {
        if depth > MAX_DEPTH {
            return self.error("Nested too deep");
        }
        self.skip_whitespace();
        match self.peek() {
            None => self.error("Unexpected end"),
            Some('{') => self.object(depth),
            Some('[') => self.array(depth),
            Some('"') => self.string().map(JsonValue::String),
            Some('t') => self.expect_word("true", JsonValue::Bool(true)),
            Some('f') => self.expect_word("false", JsonValue::Bool(false)),
            Some('n') => self.expect_word("null", JsonValue::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => self.error(&format!("Unexpected character '{c}'")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(JsonValue::Object(members));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some('"') {
                return self.error("Expected a member name");
            }
            let key = self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(':') {
                return self.error("Expected ':'");
            }
            self.pos += 1;
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(',') => self.pos += 1,
                Some('}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(members));
                }
                _ => return self.error("Expected ',' or '}'"),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut elements = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Ok(JsonValue::Array(elements));
        }
        loop {
            elements.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => self.pos += 1,
                Some(']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(elements));
                }
                _ => return self.error("Expected ',' or ']'"),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut code = 0;
        for _ in 0..4 {
            let Some(digit) = self.peek().and_then(|c| c.to_digit(16)) else {
                return self.error("Expected 4 hex digits");
            };
            code = code * 16 + digit;
            self.pos += 1;
        }
        Ok(code)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut text = String::new();
        loop {
            let Some(c) = self.peek() else {
                return self.error("Unterminated string");
            };
            self.pos += 1;
            match c {
                '"' => return Ok(text),
                '\\' => {
                    let Some(escaped) = self.peek() else {
                        return self.error("Unterminated escape");
                    };
                    self.pos += 1;
                    match escaped {
                        '"' => text.push('"'),
                        '\\' => text.push('\\'),
                        '/' => text.push('/'),
                        'b' => text.push('\u{8}'),
                        'f' => text.push('\u{c}'),
                        'n' => text.push('\n'),
                        'r' => text.push('\r'),
                        't' => text.push('\t'),
                        'u' => {
                            let mut code = self.hex4()?;
                            // A surrogate pair
                            if (0xD800..0xDC00).contains(&code) && self.chars.get(self.pos) == Some(&'\\') && self.chars.get(self.pos + 1) == Some(&'u') {
                                self.pos += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                }
                            }
                            text.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        other => return self.error(&format!("Unknown escape '\\{other}'")),
                    }
                }
                c => text.push(c),
            }
        }
    }

    fn number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>().map(JsonValue::Number).map_err(|_| format!("Not a number '{text}' at {start}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonValue, JsonWriter};

    #[test]
    fn nesting_and_escaping() {
        let mut json = JsonWriter::new();
        json.begin_object();
        json.key("name").string("Launch \"XL\"\n");
        json.key("ports").number(1);
        json.key("list").begin_array().string("a").number(-2).boolean(true).end_array();
        json.key("empty").begin_object().end_object();
        json.end_object();
        assert_eq!(json.finish(), r#"{"name":"Launch \"XL\"\n","ports":1,"list":["a",-2,true],"empty":{}}"#);
    }

    #[test]
    fn reads_the_electra_replies() {
        let info = JsonValue::parse(r#"{"versionText":"v5.0.6","versionSeq":500000600,"serial":"EO-00000","hwRevision":"2.00","model":"mk2","modelNum":2}"#).unwrap();
        assert_eq!(info.get("versionSeq").unwrap().as_int(), 500000600);
        assert_eq!(info.get("versionText").unwrap().as_text(), "v5.0.6");
        assert_eq!(info.get("model").unwrap().as_text(), "mk2");
        assert_eq!(info.get("modelNum").unwrap().as_text(), "2");
        assert!(info.get("missing").is_none());

        let list = JsonValue::parse(" {\"version\":1,\"presets\":[{\"slot\":0,\"bankNumber\":2,\"name\":\"DrivenByMoss\"},{\"slot\":\"5\",\"name\":\"x\"}]} \n").unwrap();
        let presets = list.get("presets").unwrap().elements();
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].get("bankNumber").unwrap().as_int(), 2);
        assert_eq!(presets[1].get("slot").unwrap().as_int(), 5);
        assert!(presets[1].get("bankNumber").is_none());
    }

    #[test]
    fn reads_all_value_kinds() {
        let value = JsonValue::parse(r#"[null, true, false, -1.5e1, "a\"\\\/\n\u00e4\ud83d\ude00", [], {}]"#).unwrap();
        assert_eq!(
            value,
            JsonValue::Array(vec![
                JsonValue::Null,
                JsonValue::Bool(true),
                JsonValue::Bool(false),
                JsonValue::Number(-15.0),
                JsonValue::String("a\"\\/\n\u{e4}\u{1F600}".to_string()),
                JsonValue::Array(vec![]),
                JsonValue::Object(vec![]),
            ])
        );
        assert_eq!(JsonValue::Number(2.5).as_text(), "2.5");
        assert_eq!(JsonValue::String("12".into()).as_int(), 12);
        assert_eq!(JsonValue::String("x".into()).as_int(), 0);
        // The writer's output reads back
        let mut json = JsonWriter::new();
        json.begin_object().key("name").string("Tab\t\"q\"").key("n").number(-3).end_object();
        let read = JsonValue::parse(&json.finish()).unwrap();
        assert_eq!(read.get("name").unwrap().as_text(), "Tab\t\"q\"");
        assert_eq!(read.get("n").unwrap().as_int(), -3);
    }

    #[test]
    fn rejects_bad_input() {
        for text in ["", "{", "[1,]", "{\"a\" 1}", "\"open", "tru", "{} x", "[1 2]", "\"\\x\"", "-"] {
            assert!(JsonValue::parse(text).is_err(), "{text:?}");
        }
        assert!(JsonValue::parse(&"[".repeat(100)).is_err());
    }
}
