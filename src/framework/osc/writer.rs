// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Collects the outgoing OSC messages and sends only changed values (former
//! `AbstractOpenSoundControlWriter` and `IOpenSoundControlWriter`).
//!
//! Every address has a slot in the writer, which keeps the last value sent to it. A module
//! creates the [`Key`]s of its addresses once ([`OscWriter::key`] interns the address, a
//! [`KeyGroup`] all addresses of a prefix, see `osc_keys!`) and sends its values by key
//! ([`OscWriter::put`]): the value is compared with the last one of the slot and a message is
//! queued only if it changed or if `dump` is set (a full update), so no address is formatted
//! and nothing is allocated for an unchanged value. The `send_*` functions with an address
//! string are the same for a single address (they look the slot up by its address).
//! [`OscWriter::take_messages`] returns the queued messages, framed by the update address
//! (`/update 1` ... `/update 0`) as the Java `flush (updateAddress)` does.
//!
//! Differences to Java: the cache stores the value which is sent (a boolean as the integer 1 or
//! 0, a string after `fixASCII`, a color as its rounded components) instead of the Java object;
//! this only differs for an address which is sent with different Java types, which no module
//! does.

use std::collections::HashMap;
use std::marker::PhantomData;

use super::message::{OscArg, OscMessage};
use crate::config::values::fix_ascii;
use crate::daw::color::ColorEx;

/// The key of an address in the writer; `T` is the type of its value (`bool`, `i32`, `f64`,
/// `str` or `ColorEx`).
pub struct Key<T: ?Sized> {
    slot: u32,
    kind: PhantomData<fn(&T)>,
}

impl<T: ?Sized> Clone for Key<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Key<T> {}

impl<T: ?Sized> std::fmt::Debug for Key<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Key({})", self.slot)
    }
}

/// A value type of a key: how it is compared and sent.
pub trait KeyValue {
    /// The value as it is passed to [`OscWriter::put`].
    type In<'a>;

    fn put(writer: &mut OscWriter, slot: usize, value: Self::In<'_>, dump: bool);
}

/// A boolean, sent as the integer 1 or 0.
impl KeyValue for bool {
    type In<'a> = bool;

    fn put(writer: &mut OscWriter, slot: usize, value: bool, dump: bool) {
        writer.put_int(slot, i32::from(value), dump);
    }
}

impl KeyValue for i32 {
    type In<'a> = i32;

    fn put(writer: &mut OscWriter, slot: usize, value: i32, dump: bool) {
        writer.put_int(slot, value, dump);
    }
}

/// A double, sent as a float ("Using float here since Double seems to be always received as 0
/// in Max.").
impl KeyValue for f64 {
    type In<'a> = f64;

    fn put(writer: &mut OscWriter, slot: usize, value: f64, dump: bool) {
        writer.put_float(slot, value as f32, dump);
    }
}

/// A string, non-ASCII characters replaced (`StringUtils.fixASCII`).
impl KeyValue for str {
    type In<'a> = &'a str;

    fn put(writer: &mut OscWriter, slot: usize, value: &str, dump: bool) {
        writer.put_str(slot, value, dump);
    }
}

/// A color, sent as the string `rgb(r,g,b)` with the components in the range of [0..255].
impl KeyValue for ColorEx {
    type In<'a> = ColorEx;

    fn put(writer: &mut OscWriter, slot: usize, value: ColorEx, dump: bool) {
        writer.put_color(slot, [value.red(), value.green(), value.blue()], dump);
    }
}

/// The keys of the addresses with a common prefix (created by `osc_keys!`).
pub trait KeyGroup {
    /// Create (intern) the keys of the addresses `<prefix><tag>`.
    fn new(writer: &mut OscWriter, prefix: &str) -> Self;
}

/// The key groups of a numbered list of items (`<prefix><n>/...`, n 1-based by default): the
/// keys of an item are created when it is used first.
pub struct KeyList<K> {
    prefix: String,
    first: usize,
    items: Vec<K>,
}

impl<K: KeyGroup> KeyList<K> {
    /// A list numbered from 1.
    pub fn new(prefix: String) -> Self {
        Self::with_first(prefix, 1)
    }

    /// A list numbered from `first`.
    pub fn with_first(prefix: String, first: usize) -> Self {
        KeyList { prefix, first, items: Vec::new() }
    }

    /// The keys of the item with the 0-based index.
    pub fn at(&mut self, writer: &mut OscWriter, index: usize) -> &mut K {
        while self.items.len() <= index {
            let prefix = format!("{}{}/", self.prefix, self.items.len() + self.first);
            self.items.push(K::new(writer, &prefix));
        }
        &mut self.items[index]
    }
}

/// A group of keys: a struct with a key per address `<prefix><tag>` (a value type `bool`,
/// `i32`, `f64`, `str`, `ColorEx`), a [`KeyList`] (`[Group]`) or a nested group (`Group`).
///
/// ```ignore
/// osc_keys! {
///     pub struct SendKeys {
///         name: str = "name",       // <prefix>name
///         volume: i32 = "volume",
///     }
/// }
/// osc_keys! {
///     pub struct TrackKeys {
///         sends: [SendKeys] = "send/",  // <prefix>send/<n>/name, ...
///     }
/// }
/// ```
macro_rules! osc_keys {
    ($(#[$meta:meta])* $vis:vis struct $name:ident { $($field:ident : $kind:tt = $tag:literal),* $(,)? }) => {
        $(#[$meta])*
        $vis struct $name {
            $(pub $field: $crate::framework::osc::writer::osc_keys!(@type $kind),)*
        }

        impl $crate::framework::osc::writer::KeyGroup for $name {
            fn new(writer: &mut $crate::framework::osc::OscWriter, prefix: &str) -> Self {
                $name {
                    $($field: $crate::framework::osc::writer::osc_keys!(@new writer prefix $kind $tag),)*
                }
            }
        }

        impl $name {
            /// The tags of the addresses, in the order of the table.
            #[allow(dead_code)]
            pub const TAGS: &'static [&'static str] = &[$($tag),*];
        }
    };
    (@type bool) => { $crate::framework::osc::writer::Key<bool> };
    (@type i32) => { $crate::framework::osc::writer::Key<i32> };
    (@type f64) => { $crate::framework::osc::writer::Key<f64> };
    (@type str) => { $crate::framework::osc::writer::Key<str> };
    (@type ColorEx) => { $crate::framework::osc::writer::Key<$crate::daw::color::ColorEx> };
    (@type [$group:ident]) => { $crate::framework::osc::writer::KeyList<$group> };
    (@type $group:ident) => { $group };
    (@new $writer:ident $prefix:ident [$group:ident] $tag:literal) => {
        $crate::framework::osc::writer::KeyList::new(format!("{}{}", $prefix, $tag))
    };
    (@new $writer:ident $prefix:ident bool $tag:literal) => { $writer.key(&format!("{}{}", $prefix, $tag)) };
    (@new $writer:ident $prefix:ident i32 $tag:literal) => { $writer.key(&format!("{}{}", $prefix, $tag)) };
    (@new $writer:ident $prefix:ident f64 $tag:literal) => { $writer.key(&format!("{}{}", $prefix, $tag)) };
    (@new $writer:ident $prefix:ident str $tag:literal) => { $writer.key(&format!("{}{}", $prefix, $tag)) };
    (@new $writer:ident $prefix:ident ColorEx $tag:literal) => { $writer.key(&format!("{}{}", $prefix, $tag)) };
    (@new $writer:ident $prefix:ident $group:ident $tag:literal) => {
        <$group as $crate::framework::osc::writer::KeyGroup>::new($writer, &format!("{}{}", $prefix, $tag))
    };
}

pub(crate) use osc_keys;

/// The last value sent to an address.
#[derive(Debug, Clone, PartialEq)]
enum Last {
    Unset,
    Int(i32),
    /// The bits of a float (Java `Float.equals`).
    Float(u32),
    Str(String),
    /// The rounded components of a color.
    Color([i64; 3]),
    Args(Vec<OscArg>),
}

impl Last {
    fn args(&self) -> Option<Vec<OscArg>> {
        Some(match self {
            Last::Unset => return None,
            Last::Int(v) => vec![OscArg::Int(*v)],
            Last::Float(bits) => vec![OscArg::Float(f32::from_bits(*bits))],
            Last::Str(v) => vec![OscArg::Str(v.clone())],
            Last::Color(rgb) => vec![OscArg::Str(color_text(*rgb))],
            Last::Args(args) => args.clone(),
        })
    }
}

fn color_text([red, green, blue]: [i64; 3]) -> String {
    format!("rgb({red},{green},{blue})")
}

struct Slot {
    address: String,
    last: Last,
}

/// Collects the changed values (former `AbstractOpenSoundControlWriter`).
#[derive(Default)]
pub struct OscWriter {
    slots: Vec<Slot>,
    /// The slot of each address.
    index: HashMap<String, u32>,
    messages: Vec<OscMessage>,
}

impl OscWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The key of an address (the same for the same address).
    pub fn key<T: ?Sized>(&mut self, address: &str) -> Key<T> {
        Key { slot: self.slot(address) as u32, kind: PhantomData }
    }

    fn slot(&mut self, address: &str) -> usize {
        if let Some(&slot) = self.index.get(address) {
            return slot as usize;
        }
        let slot = self.slots.len();
        self.slots.push(Slot { address: address.to_string(), last: Last::Unset });
        self.index.insert(address.to_string(), slot as u32);
        slot
    }

    /// The address of a key.
    pub fn address<T: ?Sized>(&self, key: Key<T>) -> &str {
        &self.slots[key.slot as usize].address
    }

    /// Queue the value of a key if it changed since it was sent last, or if `dump` is set.
    pub fn put<T: KeyValue + ?Sized>(&mut self, key: Key<T>, value: T::In<'_>, dump: bool) {
        T::put(self, key.slot as usize, value, dump);
    }

    fn push(&mut self, slot: usize, arg: OscArg) {
        self.messages.push(OscMessage::new(self.slots[slot].address.clone(), vec![arg]));
    }

    fn put_int(&mut self, slot: usize, value: i32, dump: bool) {
        let last = &mut self.slots[slot].last;
        if !dump && *last == Last::Int(value) {
            return;
        }
        *last = Last::Int(value);
        self.push(slot, OscArg::Int(value));
    }

    fn put_float(&mut self, slot: usize, value: f32, dump: bool) {
        let last = &mut self.slots[slot].last;
        if !dump && *last == Last::Float(value.to_bits()) {
            return;
        }
        *last = Last::Float(value.to_bits());
        self.push(slot, OscArg::Float(value));
    }

    fn put_str(&mut self, slot: usize, value: &str, dump: bool) {
        // fixASCII does not change an ASCII text
        let fixed;
        let value = if value.is_ascii() {
            value
        } else {
            fixed = fix_ascii(value);
            fixed.as_str()
        };
        let last = &mut self.slots[slot].last;
        match last {
            Last::Str(old) if old == value => {
                if !dump {
                    return;
                }
            }
            Last::Str(old) => {
                old.clear();
                old.push_str(value);
            }
            _ => *last = Last::Str(value.to_string()),
        }
        self.push(slot, OscArg::Str(value.to_string()));
    }

    fn put_color(&mut self, slot: usize, components: [f64; 3], dump: bool) {
        let rgb = components.map(|c| java_round(c * 255.0));
        let last = &mut self.slots[slot].last;
        if !dump && *last == Last::Color(rgb) {
            return;
        }
        *last = Last::Color(rgb);
        self.push(slot, OscArg::Str(color_text(rgb)));
    }

    fn put_args(&mut self, slot: usize, values: Vec<OscArg>, dump: bool) {
        match values.as_slice() {
            [OscArg::Int(v)] => return self.put_int(slot, *v, dump),
            [OscArg::Float(v)] => return self.put_float(slot, *v, dump),
            [OscArg::Str(_)] => {
                let last = &mut self.slots[slot].last;
                if !dump && matches!((&*last, &values[0]), (Last::Str(old), OscArg::Str(new)) if old == new) {
                    return;
                }
                if let OscArg::Str(new) = &values[0] {
                    *last = Last::Str(new.clone());
                }
            }
            _ => {
                let last = &mut self.slots[slot].last;
                if !dump && matches!(&*last, Last::Args(old) if *old == values) {
                    return;
                }
                *last = Last::Args(values.clone());
            }
        }
        self.messages.push(OscMessage::new(self.slots[slot].address.clone(), values));
    }

    /// Send a boolean as the integer 1 or 0.
    pub fn send_bool(&mut self, address: &str, value: bool, dump: bool) {
        let slot = self.slot(address);
        self.put_int(slot, i32::from(value), dump);
    }

    /// Send an integer.
    pub fn send_int(&mut self, address: &str, value: i32, dump: bool) {
        let slot = self.slot(address);
        self.put_int(slot, value, dump);
    }

    /// Send a double as a float.
    pub fn send_double(&mut self, address: &str, value: f64, dump: bool) {
        let slot = self.slot(address);
        self.put_float(slot, value as f32, dump);
    }

    /// Send a string, non-ASCII characters replaced (`StringUtils.fixASCII`).
    pub fn send_str(&mut self, address: &str, value: &str, dump: bool) {
        let slot = self.slot(address);
        self.put_str(slot, value, dump);
    }

    /// Send a color (components in the range of [0..1]) as the string `rgb(r,g,b)` with the
    /// components in the range of [0..255].
    pub fn send_color(&mut self, address: &str, red: f64, green: f64, blue: f64, dump: bool) {
        let slot = self.slot(address);
        self.put_color(slot, [red, green, blue], dump);
    }

    /// Send a message with the given integers immediately (former `fastSendOSC`): queues it
    /// even if it did not change; the caller sends the queued messages right away.
    pub fn fast_send(&mut self, address: &str, numbers: &[i32]) {
        let slot = self.slot(address);
        self.put_args(slot, numbers.iter().map(|&n| OscArg::Int(n)).collect(), true);
    }

    /// Queue a value if it changed since the last call for the address, or if `dump` is set.
    pub fn send(&mut self, address: &str, value: OscArg, dump: bool) {
        let slot = self.slot(address);
        self.put_args(slot, vec![value], dump);
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
        for slot in &mut self.slots {
            slot.last = Last::Unset;
        }
    }

    /// The last value sent to the address.
    pub fn last_value(&self, address: &str) -> Option<Vec<OscArg>> {
        self.index.get(address).and_then(|&slot| self.slots[slot as usize].last.args())
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
        assert_eq!(writer.last_value("/track/1/name"), Some(vec![OscArg::Str("Baesse".into())]));
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

    osc_keys! {
        struct ItemKeys {
            name: str = "name",
            volume: i32 = "volume",
        }
    }

    osc_keys! {
        struct GroupKeys {
            exists: bool = "exists",
            color: ColorEx = "color",
            level: f64 = "level",
            items: [ItemKeys] = "item/",
            nested: ItemKeys = "nested/",
        }
    }

    #[test]
    fn keys() {
        let mut writer = OscWriter::new();
        let mut keys = GroupKeys::new(&mut writer, "/group/");
        assert_eq!(writer.address(keys.exists), "/group/exists");
        assert_eq!(writer.address(keys.nested.name), "/group/nested/name");
        assert_eq!(GroupKeys::TAGS, ["exists", "color", "level", "item/", "nested/"]);
        writer.put(keys.exists, true, false);
        writer.put(keys.color, ColorEx::new(1.0, 0.5, 0.0), false);
        writer.put(keys.level, 0.25, false);
        let item = keys.items.at(&mut writer, 1);
        assert_eq!(writer.address(item.name), "/group/item/2/name");
        writer.put(item.name, "Bässe", false);
        let texts: Vec<String> = writer.take_messages(None).iter().map(OscMessage::log_text).collect();
        assert_eq!(texts, ["/group/exists [ 1 ]", "/group/color [ rgb(255,128,0) ]", "/group/level [ 0.25 ]", "/group/item/2/name [ Baesse ]"]);

        // Only changes, by key and by address (the same slot)
        writer.put(keys.exists, true, false);
        writer.send_bool("/group/exists", true, false);
        writer.put(keys.color, ColorEx::new(1.0, 0.501, 0.0), false);
        let item = keys.items.at(&mut writer, 1);
        writer.put(item.name, "Bässe", false);
        assert!(!writer.has_messages());
        assert_eq!(writer.last_value("/group/color"), Some(vec![OscArg::Str("rgb(255,128,0)".into())]));
        writer.put(item.name, "Bass", false);
        writer.put(keys.exists, true, true);
        let texts: Vec<String> = writer.take_messages(None).iter().map(OscMessage::log_text).collect();
        assert_eq!(texts, ["/group/item/2/name [ Bass ]", "/group/exists [ 1 ]"]);

        // A list numbered from 0
        let mut notes: KeyList<ItemKeys> = KeyList::with_first("/note/".into(), 0);
        let note = notes.at(&mut writer, 0);
        assert_eq!(writer.address(note.volume), "/note/0/volume");
        assert_eq!(writer.key::<i32>("/note/0/volume").slot, note.volume.slot, "the same address has the same key");
    }
}
