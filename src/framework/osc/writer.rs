// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Collects the outgoing OSC messages and sends only changed values (former
//! `AbstractOpenSoundControlWriter` and `IOpenSoundControlWriter`).
//!
//! Every `send_*` call compares the value with the last one sent to the address and queues a
//! message only if it changed or if `dump` is set (a full update). [`OscWriter::take_messages`]
//! returns the queued messages, framed by the update address (`/update 1` ... `/update 0`) as
//! the Java `flush (updateAddress)` does.
//!
//! Differences to Java: the cache stores the value which is sent (a boolean as the integer 1 or
//! 0, a string after `fixASCII`) instead of the Java object; this only differs for an address
//! which is sent with different Java types, which no module does.

use std::collections::HashMap;

use super::message::{OscArg, OscMessage};
use crate::config::values::fix_ascii;

/// Collects the changed values (former `AbstractOpenSoundControlWriter`).
#[derive(Default)]
pub struct OscWriter {
    old_values: HashMap<String, Vec<OscArg>>,
    messages: Vec<OscMessage>,
}

impl OscWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send a boolean as the integer 1 or 0.
    pub fn send_bool(&mut self, address: &str, value: bool, dump: bool) {
        self.send(address, OscArg::Int(i32::from(value)), dump);
    }

    /// Send an integer.
    pub fn send_int(&mut self, address: &str, value: i32, dump: bool) {
        self.send(address, OscArg::Int(value), dump);
    }

    /// Send a double as a float ("Using float here since Double seems to be always received as 0
    /// in Max.").
    pub fn send_double(&mut self, address: &str, value: f64, dump: bool) {
        self.send(address, OscArg::Float(value as f32), dump);
    }

    /// Send a string, non-ASCII characters replaced (`StringUtils.fixASCII`).
    pub fn send_str(&mut self, address: &str, value: &str, dump: bool) {
        self.send(address, OscArg::Str(fix_ascii(value)), dump);
    }

    /// Send a color (components in the range of [0..1]) as the string `rgb(r,g,b)` with the
    /// components in the range of [0..255].
    pub fn send_color(&mut self, address: &str, red: f64, green: f64, blue: f64, dump: bool) {
        let component = |c: f64| java_round(c * 255.0);
        let text = format!("rgb({},{},{})", component(red), component(green), component(blue));
        self.send(address, OscArg::Str(text), dump);
    }

    /// Send a message with the given integers immediately (former `fastSendOSC`): queues it
    /// even if it did not change; the caller sends the queued messages right away.
    pub fn fast_send(&mut self, address: &str, numbers: &[i32]) {
        self.queue(address, address, numbers.iter().map(|&n| OscArg::Int(n)).collect(), true);
    }

    /// Queue a value if it changed since the last call for the address, or if `dump` is set.
    pub fn send(&mut self, address: &str, value: OscArg, dump: bool) {
        self.queue(address, address, vec![value], dump);
    }

    /// Queue a message: the cache address may differ from the address of the message (former
    /// `sendOSC (cacheAddress, address, testValue, value, dump)`).
    pub fn queue(&mut self, cache_address: &str, address: &str, values: Vec<OscArg>, dump: bool) {
        match self.old_values.get_mut(cache_address) {
            Some(old) => {
                if !dump && *old == values {
                    return;
                }
                old.clone_from(&values);
            }
            None => {
                self.old_values.insert(cache_address.to_string(), values.clone());
            }
        }
        self.messages.push(OscMessage::new(address, values));
    }

    /// True if there are queued messages.
    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }

    /// Take the queued messages. If there are any and an update address is given, they are
    /// framed by the update address with the values 1 and 0 (former `flush (updateAddress)`).
    pub fn take_messages(&mut self, update_address: Option<&str>) -> Vec<OscMessage> {
        if self.messages.is_empty() {
            return Vec::new();
        }
        let mut messages = std::mem::take(&mut self.messages);
        if let Some(address) = update_address {
            messages.insert(0, OscMessage::new(address, vec![OscArg::Int(1)]));
            messages.push(OscMessage::new(address, vec![OscArg::Int(0)]));
        }
        messages
    }

    /// Forget all sent values, the next call of every address is sent.
    pub fn clear_cache(&mut self) {
        self.old_values.clear();
    }

    /// The last value sent to the address.
    pub fn last_value(&self, address: &str) -> Option<&[OscArg]> {
        self.old_values.get(address).map(Vec::as_slice)
    }
}

/// Java `Math.round` of a double to an int: rounds half up (towards positive infinity).
pub fn java_round(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    (value + 0.5).floor() as i64
}

/// The log text of the sent messages (former `logMessages`): one `Sending: /address [ values ]`
/// line per message, without the heartbeat messages if they are filtered. None if there is
/// nothing to log.
pub fn log_text(messages: &[OscMessage], is_heartbeat: impl Fn(&str) -> bool, filter_heartbeat: bool) -> Option<String> {
    let lines: Vec<String> = messages
        .iter()
        .filter(|m| !(filter_heartbeat && is_heartbeat(&m.address)))
        .map(|m| format!("Sending: {}", m.log_text()))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_only_changes() {
        let mut writer = OscWriter::new();
        writer.send_bool("/play", true, false);
        writer.send_int("/track/1/volume", 64, false);
        assert_eq!(writer.take_messages(None).len(), 2);
        writer.send_bool("/play", true, false);
        writer.send_int("/track/1/volume", 64, false);
        assert!(!writer.has_messages());
        assert!(writer.take_messages(Some("/update")).is_empty(), "nothing to frame");
        writer.send_int("/track/1/volume", 65, false);
        writer.send_bool("/play", true, true);
        let messages = writer.take_messages(Some("/update"));
        let texts: Vec<String> = messages.iter().map(OscMessage::log_text).collect();
        assert_eq!(texts, ["/update [ 1 ]", "/track/1/volume [ 65 ]", "/play [ 1 ]", "/update [ 0 ]"]);
    }

    #[test]
    fn value_conversions() {
        let mut writer = OscWriter::new();
        writer.send_double("/tempo/raw", 120.5, false);
        writer.send_str("/track/1/name", "Bässe", false);
        writer.send_color("/track/1/color", 1.0, 0.5, 0.0, false);
        writer.send_color("/track/2/color", 0.85098, 0.18039, 0.14118, false);
        writer.fast_send("/vkb_midi/1/note", &[60, 127]);
        let messages = writer.take_messages(None);
        assert_eq!(messages[0].args, vec![OscArg::Float(120.5)]);
        assert_eq!(messages[1].args, vec![OscArg::Str("Baesse".into())]);
        assert_eq!(messages[2].args, vec![OscArg::Str("rgb(255,128,0)".into())]);
        assert_eq!(messages[3].args, vec![OscArg::Str("rgb(217,46,36)".into())]);
        assert_eq!(messages[4].args, vec![OscArg::Int(60), OscArg::Int(127)]);
        // The cache compares the converted value
        writer.send_str("/track/1/name", "Bässe", false);
        assert!(!writer.has_messages());
        assert_eq!(writer.last_value("/track/1/name"), Some([OscArg::Str("Baesse".into())].as_slice()));
        writer.clear_cache();
        writer.send_str("/track/1/name", "Bässe", false);
        assert!(writer.has_messages());
    }

    #[test]
    fn rounding_and_log() {
        assert_eq!(java_round(127.5), 128);
        assert_eq!(java_round(-0.5), 0);
        assert_eq!(java_round(-1.5), -1);
        assert_eq!(java_round(f64::NAN), 0);
        let messages = vec![OscMessage::new("/update", vec![OscArg::Int(1)]), OscMessage::new("/play", vec![OscArg::Int(0)])];
        assert_eq!(log_text(&messages, |a| a == "/update", false).as_deref(), Some("Sending: /update [ 1 ]\nSending: /play [ 0 ]"));
        assert_eq!(log_text(&messages, |a| a == "/update", true).as_deref(), Some("Sending: /play [ 0 ]"));
        assert_eq!(log_text(&messages[..1], |a| a == "/update", true), None);
    }
}
