// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! A minimal JSON writer for the data the C interface hands to hosts (the controller catalog and
//! the model setup), so that the crate needs no external dependency.

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

#[cfg(test)]
mod tests {
    use super::JsonWriter;

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
}
