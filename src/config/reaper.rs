// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Reaper specific storage of the settings (from `AbstractControllerInstance`,
//! `GlobalSettingsUI`, `DocumentSettingsUI` and `MainApp`).
//!
//! * Global settings: one properties file per controller in the Reaper resource (INI) folder,
//!   named `DrivenByMoss4Reaper-<hardware model>.config` ([`config_file_name`]). Besides the
//!   settings it contains the enabled state and the selected MIDI ports ([`ControllerConfigFile`]).
//! * Document settings: stored in the Reaper project. The string exchanged with Reaper
//!   (`getFormattedDocumentSettings`/`setFormattedDocumentSettings`) contains one line per
//!   controller instance: `TAG="<Base64 of the properties text>"` with the tag created by
//!   [`document_tag`].

use std::io;
use std::path::{Path, PathBuf};

use super::configuration::Configuration;
use super::properties::{base64_decode, base64_encode, Properties};
use super::setting::Scope;

const TAG_IS_ENABLED: &str = "TAG_IS_ENABLED";
const TAG_MIDI_INPUT: &str = "MIDI_INPUT";
const TAG_MIDI_OUTPUT: &str = "MIDI_OUTPUT";

/// The file name of the controller configuration, e.g. "DrivenByMoss4Reaper-Launchpad-X.config".
pub fn config_file_name(hardware_model: &str) -> String {
    format!("DrivenByMoss4Reaper-{}.config", hardware_model.replace([' ', '/'], "-"))
}

/// The tag of a controller in the document settings, e.g. "LAUNCHPAD_PRO_MK3".
pub fn document_tag(hardware_model: &str) -> String {
    hardware_model.replace([' ', '/'], "_").to_uppercase()
}

/// Create the document settings string for Reaper from the (tag, properties text) pairs of all
/// controller instances (`MainApp.getFormattedDocumentSettings`). Use [`document_tag`] and
/// [`Configuration::store_document_settings`] to create the pairs.
pub fn format_document_settings<T: AsRef<str>, P: AsRef<str>>(instances: &[(T, P)]) -> String {
    let mut data = String::new();
    for (tag, properties_text) in instances {
        data.push_str(tag.as_ref());
        data.push_str("=\"");
        data.push_str(&base64_encode(properties_text.as_ref().as_bytes()));
        data.push_str("\"\n");
    }
    data
}

/// Parse the document settings string from Reaper into (tag, properties text) pairs
/// (`MainApp.setFormattedDocumentSettings`). Entries which cannot be decoded are skipped.
pub fn parse_document_settings(data: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for line in data.lines() {
        let mut rest = line;
        // Mirrors the Java pattern (.*?)="(.*?)"\s* applied with find ()
        while let Some(start) = rest.find("=\"") {
            let tag = rest[..start].trim();
            let after = &rest[start + 2..];
            let Some(end) = after.find('"') else {
                break;
            };
            if let Some(bytes) = base64_decode(&after[..end]) {
                result.push((tag.to_string(), String::from_utf8_lossy(&bytes).into_owned()));
            }
            rest = &after[end + 1..];
        }
    }
    result
}

/// Apply the matching entry of a parsed document settings string to a configuration. Returns
/// true if an entry for the controller was found.
pub fn apply_document_settings(configuration: &mut Configuration, hardware_model: &str, parsed: &[(String, String)]) -> bool {
    let tag = document_tag(hardware_model);
    match parsed.iter().find(|(t, _)| *t == tag) {
        Some((_, text)) => {
            configuration.load_document_settings(text);
            true
        }
        None => false,
    }
}

/// The global configuration file of a controller instance. Keeps all entries of the file
/// (including the ones written by the Java version, e.g. window placements).
#[derive(Debug, Clone, Default)]
pub struct ControllerConfigFile {
    pub properties: Properties,
}

impl ControllerConfigFile {
    /// Path of the configuration file in the given folder (the Reaper INI path).
    pub fn path(ini_path: &Path, hardware_model: &str) -> PathBuf {
        ini_path.join(config_file_name(hardware_model))
    }

    /// Load the file, a missing file results in an empty configuration.
    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(ControllerConfigFile { properties: Properties::load_file(path)? })
    }

    /// Store the global settings of the configuration into the properties and write the file
    /// (`AbstractControllerInstance.storeConfiguration`).
    pub fn save(&mut self, path: &Path, configuration: &Configuration) -> io::Result<()> {
        configuration.store(Scope::Global, &mut self.properties);
        self.properties.save_file(path)
    }

    /// Load the global settings of the configuration from the properties.
    pub fn apply_to(&self, configuration: &mut Configuration) {
        configuration.load(Scope::Global, &self.properties);
    }

    /// Is the controller instance enabled (default true)?
    pub fn is_enabled(&self) -> bool {
        self.properties.get_bool(TAG_IS_ENABLED, true)
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.properties.put_bool(TAG_IS_ENABLED, enabled);
    }

    /// The name of the selected MIDI input port with the given index, `None` if not configured
    /// (the Java version then tries to auto-detect the device).
    pub fn midi_input(&self, index: usize) -> Option<&str> {
        self.properties.get_string(&format!("{TAG_MIDI_INPUT}{index}"))
    }

    pub fn set_midi_input(&mut self, index: usize, name: &str) {
        self.properties.put(&format!("{TAG_MIDI_INPUT}{index}"), name);
    }

    /// The name of the selected MIDI output port with the given index.
    pub fn midi_output(&self, index: usize) -> Option<&str> {
        self.properties.get_string(&format!("{TAG_MIDI_OUTPUT}{index}"))
    }

    pub fn set_midi_output(&mut self, index: usize, name: &str) {
        self.properties.put(&format!("{TAG_MIDI_OUTPUT}{index}"), name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::configuration::NoteRepeatCapabilities;
    use crate::config::values::REAPER_ARP_MODES;

    #[test]
    fn names() {
        assert_eq!(config_file_name("Launchpad Pro Mk3"), "DrivenByMoss4Reaper-Launchpad-Pro-Mk3.config");
        assert_eq!(document_tag("Launchpad Pro Mk3"), "LAUNCHPAD_PRO_MK3");
        assert_eq!(document_tag("MPC/Force"), "MPC_FORCE");
    }

    #[test]
    fn document_string_round_trip() {
        let mut c = Configuration::new(&REAPER_ARP_MODES);
        c.activate_scale_setting(Scope::Document);
        c.activate_note_repeat_setting(Scope::Document, NoteRepeatCapabilities::REAPER);
        c.set_scale("Kumoi");
        c.set_note_repeat_active(true);

        let data = format_document_settings(&[
            (document_tag("Launchpad X"), c.store_document_settings()),
            (document_tag("Push 3"), "A=1\n".to_string()),
        ]);
        assert!(data.starts_with("LAUNCHPAD_X=\""));
        assert_eq!(data.lines().count(), 2);

        let parsed = parse_document_settings(&data);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1], ("PUSH_3".to_string(), "A=1\n".to_string()));

        let mut d = Configuration::new(&REAPER_ARP_MODES);
        d.activate_scale_setting(Scope::Document);
        d.activate_note_repeat_setting(Scope::Document, NoteRepeatCapabilities::REAPER);
        assert!(apply_document_settings(&mut d, "Launchpad X", &parsed));
        assert!(!apply_document_settings(&mut d, "Launchpad Pro", &parsed));
        assert_eq!(d.scale(), "Kumoi");
        assert!(d.is_note_repeat_active());
    }

    #[test]
    fn parse_java_document_string() {
        // Created with the Java code: Properties with SCALES_SCALE=Minor
        let text = "#\n#Thu Oct 29 12:00:00 CET 2026\nSCALES_SCALE=Minor\n";
        let data = format!("LAUNCHPAD_MINI_MKIII=\"{}\"\n", base64_encode(text.as_bytes()));
        let parsed = parse_document_settings(&data);
        assert_eq!(parsed, vec![("LAUNCHPAD_MINI_MKIII".to_string(), text.to_string())]);
        assert!(parse_document_settings("").is_empty());
        assert!(parse_document_settings("garbage").is_empty());
    }

    #[test]
    fn config_file() {
        let dir = std::env::temp_dir().join(format!("dbm-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = ControllerConfigFile::path(&dir, "Launchpad X");

        let mut file = ControllerConfigFile::load(&path).unwrap();
        assert!(file.is_enabled());
        assert_eq!(file.midi_input(0), None);
        file.set_midi_input(0, "LPX MIDI");
        file.set_midi_output(0, "LPX MIDI Out");
        file.set_enabled(false);
        file.properties.put("MainWindowX", "10.0");

        let mut c = Configuration::new(&REAPER_ARP_MODES);
        c.activate_accent_active_setting(Scope::Global);
        c.set_accent_enabled(true);
        file.save(&path, &c).unwrap();

        let loaded = ControllerConfigFile::load(&path).unwrap();
        assert!(!loaded.is_enabled());
        assert_eq!(loaded.midi_input(0), Some("LPX MIDI"));
        assert_eq!(loaded.midi_output(0), Some("LPX MIDI Out"));
        assert_eq!(loaded.properties.get("MainWindowX"), Some("10.0"));
        let mut d = Configuration::new(&REAPER_ARP_MODES);
        d.activate_accent_active_setting(Scope::Global);
        loaded.apply_to(&mut d);
        assert!(d.is_accent_active());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
