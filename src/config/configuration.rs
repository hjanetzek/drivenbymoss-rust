// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The controller independent settings (former `Configuration` / `AbstractConfiguration`).
//!
//! # Usage
//!
//! 1. Create a [`Configuration`] (or a controller specific wrapper like
//!    [`super::launchpad::LaunchpadConfiguration`]) and activate the settings the controller
//!    supports with the `activate_*` functions (this replaces `init (globalSettings,
//!    documentSettings)`; the [`Scope`] parameter replaces the two `ISettingsUI` instances).
//! 2. Load the stored values with [`Configuration::load`] (global settings from the controller
//!    configuration file, document settings from the project data, see [`super::reaper`]).
//! 3. Call [`Configuration::take_changed_settings`] regularly (e.g. after handling input) and
//!    react on the changed IDs (replaces `addSettingObserver`). Call
//!    [`Configuration::mark_all_changed`] once after startup to replace `notifyAllObservers`.
//! 4. Store the values with [`Configuration::store`] before shutdown or when the project is saved.
//!
//! Setters of settings which were not activated are ignored (in Java they either did nothing or
//! threw a `NullPointerException`). Getters of non-activated settings return the Java field
//! defaults.
//!
//! # Deactivated items
//!
//! `registerDeactivatedItemsHandler (model)` is not part of the configuration anymore. If
//! [`SettingId::EXCLUDE_DEACTIVATED_ITEMS`] is reported as changed (also after
//! [`Configuration::mark_all_changed`]), the caller must call
//! `set_skip_disabled_items (config.are_deactivated_items_excluded ())` on: the track bank, the
//! send bank of every track in the track bank page, the effect track bank (if present), the
//! device bank of the cursor device, the layer bank of the cursor device and the drum pad bank of
//! the cursor device (if present).

use std::collections::{HashMap, HashSet};

use super::properties::Properties;
use super::setting::{Scope, SettingDescriptor, SettingId, SettingKind};
use super::values::*;
use crate::framework::ids::{Modes, Views};
use crate::framework::value_changer::ValueChanger;

pub const CATEGORY_DRUMS: &str = "Drum Sequencer";
pub const CATEGORY_SCALES: &str = "Scales";
pub const CATEGORY_SESSION: &str = "Session";
pub const CATEGORY_TRANSPORT: &str = "Transport";
pub const CATEGORY_WORKFLOW: &str = "Workflow";
pub const CATEGORY_PADS: &str = "Pads";
pub const CATEGORY_PLAY_AND_SEQUENCE: &str = "Play and Sequence";
pub const CATEGORY_HARDWARE_SETUP: &str = "Hardware Setup";
pub const CATEGORY_DEBUG: &str = "Debug";
pub const CATEGORY_NOTEREPEAT: &str = "Note Repeat";

const NUMBER_OF_FOOTSWITCHES: usize = 4;

/// Which note repeat features the host supports (`Capability.NOTE_REPEAT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteRepeatCapabilities {
    pub length: bool,
    pub mode: bool,
    pub octaves: bool,
}

impl NoteRepeatCapabilities {
    /// The capabilities of the Reaper implementation (`HostImpl.CAPABILITIES`).
    pub const REAPER: NoteRepeatCapabilities = NoteRepeatCapabilities { length: true, mode: true, octaves: false };
}

fn on_off(enabled: bool) -> &'static str {
    ON_OFF_OPTIONS[usize::from(enabled)]
}

fn is_on(value: &str) -> bool {
    value == ON_OFF_OPTIONS[1]
}

/// All settings of a controller instance.
#[derive(Debug, Clone)]
pub struct Configuration {
    descriptors: Vec<SettingDescriptor>,
    descriptor_index: HashMap<SettingId, usize>,
    changed: Vec<SettingId>,
    dont_notify_all: HashSet<SettingId>,
    dirty: bool,
    /// Values of controller specific settings (IDs >= NEXT_SETTING_ID), stored as text.
    custom_values: HashMap<SettingId, String>,

    arpeggiator_modes: Vec<ArpeggiatorMode>,
    startup_views: Vec<Views>,
    startup_modes: Vec<Modes>,

    scale: String,
    scale_base: String,
    scale_in_key: bool,
    scale_layout: String,
    enable_vu_meters: bool,
    behavior_on_stop: TransportBehavior,
    behavior_on_pause: TransportBehavior,
    flip_session: bool,
    select_clip_on_launch: bool,
    draw_record_stripe: bool,
    convert_aftertouch: i32,
    accent_active: bool,
    fixed_accent_value: i32,
    quantize_amount: i32,
    flip_record: bool,
    new_clip_length: usize,
    auto_select_drum: bool,
    action_for_rec_armed_pad: usize,
    turn_off_empty_drum_pads: bool,
    turn_off_scale_pads: bool,
    footswitch: [i32; NUMBER_OF_FOOTSWITCHES],
    knob_speed_default: i32,
    knob_speed_slow: i32,
    encoder_knob_slow_down: i32,

    note_repeat_active: bool,
    note_repeat_period: Resolution,
    note_repeat_length: Resolution,
    note_repeat_mode: Option<ArpeggiatorMode>,
    note_repeat_octave: i32,
    midi_edit_channel: i32,

    include_master: bool,
    exclude_deactivated_items: bool,
    is_track_navigation_flat: bool,

    is_delete_active: bool,
    is_duplicate_active: bool,

    record_button_function: RecordFunction,
    shifted_record_button_function: RecordFunction,
    startup_view: Views,
    startup_mode: Modes,
    preferred_audio_view: Views,
    start_with_session_view: bool,
    use_combination_button_to_sound_drum_pads: bool,

    is_mpe_enabled: bool,
    mpe_pitch_bend_range: i32,
    show_played_chords: bool,
    color_track_states: bool,

    enable_double_click_play_button: bool,
    enable_double_click_stop_button: bool,

    cursor_keys_track_option: i32,
    cursor_keys_track_shifted_option: i32,
    cursor_keys_scene_option: i32,
    cursor_keys_scene_shifted_option: i32,
}

impl Configuration {
    /// Constructor. `arpeggiator_modes` are the modes supported by the host (for Reaper
    /// [`REAPER_ARP_MODES`]).
    pub fn new(arpeggiator_modes: &[ArpeggiatorMode]) -> Self {
        Configuration {
            descriptors: Vec::new(),
            descriptor_index: HashMap::new(),
            changed: Vec::new(),
            dont_notify_all: HashSet::new(),
            dirty: false,
            custom_values: HashMap::new(),

            arpeggiator_modes: arpeggiator_modes.to_vec(),
            startup_views: Vec::new(),
            startup_modes: Vec::new(),

            scale: "Major".to_string(),
            scale_base: "C".to_string(),
            scale_in_key: true,
            scale_layout: "4th ^".to_string(),
            enable_vu_meters: false,
            behavior_on_stop: TransportBehavior::Stop,
            behavior_on_pause: TransportBehavior::Pause,
            flip_session: false,
            select_clip_on_launch: true,
            draw_record_stripe: true,
            convert_aftertouch: 0,
            accent_active: false,
            fixed_accent_value: 127,
            quantize_amount: 100,
            flip_record: false,
            new_clip_length: 2,
            auto_select_drum: false,
            action_for_rec_armed_pad: 0,
            turn_off_empty_drum_pads: false,
            turn_off_scale_pads: false,
            footswitch: [FOOTSWITCH_NEW_BUTTON; NUMBER_OF_FOOTSWITCHES],
            knob_speed_default: 0,
            knob_speed_slow: -40,
            encoder_knob_slow_down: 0,

            note_repeat_active: false,
            note_repeat_period: Resolution::Res1_8,
            note_repeat_length: Resolution::Res1_8,
            note_repeat_mode: arpeggiator_modes.first().copied(),
            note_repeat_octave: 0,
            midi_edit_channel: 0,

            include_master: true,
            exclude_deactivated_items: false,
            is_track_navigation_flat: true,

            is_delete_active: false,
            is_duplicate_active: false,

            record_button_function: RecordFunction::RecordArranger,
            shifted_record_button_function: RecordFunction::NewClip,
            startup_view: Views::Play,
            startup_mode: Modes::Volume,
            preferred_audio_view: Views::Play,
            start_with_session_view: false,
            use_combination_button_to_sound_drum_pads: false,

            is_mpe_enabled: false,
            mpe_pitch_bend_range: 48,
            show_played_chords: true,
            color_track_states: true,

            enable_double_click_play_button: true,
            enable_double_click_stop_button: true,

            cursor_keys_track_option: 0,
            cursor_keys_track_shifted_option: 2,
            cursor_keys_scene_option: 1,
            cursor_keys_scene_shifted_option: 0,
        }
    }

    // ------------------------------------------------------------------------------------------
    // Setting registry, persistence and change tracking

    /// Activate (register) a setting. The value is set to the default of the setting (without
    /// recording a change). Use this for controller specific settings with IDs >=
    /// [`SettingId::NEXT_SETTING_ID`], their values can be accessed with [`Self::value`],
    /// [`Self::custom_index`], [`Self::custom_int`] and [`Self::custom_bool`].
    pub fn activate_setting(&mut self, descriptor: SettingDescriptor) {
        let id = descriptor.id;
        let default_value = descriptor.default_value.clone();
        match self.descriptor_index.get(&id) {
            Some(&index) => self.descriptors[index] = descriptor,
            None => {
                self.descriptor_index.insert(id, self.descriptors.len());
                self.descriptors.push(descriptor);
            }
        }
        self.write_field(id, &default_value);
    }

    /// Activate an enumeration setting.
    #[allow(clippy::too_many_arguments)]
    pub fn activate_enum_setting(
        &mut self,
        id: SettingId,
        scope: Scope,
        category: &str,
        label: &str,
        options: Vec<String>,
        default_value: &str,
        observable: bool,
    ) {
        self.activate_setting(SettingDescriptor {
            id,
            scope,
            category: category.to_string(),
            label: label.to_string(),
            kind: SettingKind::Enum { options },
            default_value: default_value.to_string(),
            observable,
        });
    }

    /// Activate an integer range setting.
    #[allow(clippy::too_many_arguments)]
    pub fn activate_range_setting(
        &mut self,
        id: SettingId,
        scope: Scope,
        category: &str,
        label: &str,
        min: i32,
        max: i32,
        default_value: i32,
        observable: bool,
    ) {
        self.activate_setting(SettingDescriptor {
            id,
            scope,
            category: category.to_string(),
            label: label.to_string(),
            kind: SettingKind::Integer { min, max },
            default_value: default_value.to_string(),
            observable,
        });
    }

    fn activate_on_off(&mut self, id: SettingId, scope: Scope, category: &str, label: &str, default_on: bool, observable: bool) {
        self.activate_enum_setting(id, scope, category, label, to_options(&ON_OFF_OPTIONS), on_off(default_on), observable);
    }

    /// All activated settings in the order of their activation.
    pub fn descriptors(&self) -> &[SettingDescriptor] {
        &self.descriptors
    }

    pub fn descriptor(&self, id: SettingId) -> Option<&SettingDescriptor> {
        self.descriptor_index.get(&id).map(|&index| &self.descriptors[index])
    }

    pub fn is_setting_active(&self, id: SettingId) -> bool {
        self.descriptor_index.contains_key(&id)
    }

    /// True if the setting is supported by the controller and notifies changes (Java semantics:
    /// only the settings which were added to `isSettingActive`).
    pub fn can_setting_be_observed(&self, id: SettingId) -> bool {
        self.descriptor(id).is_some_and(|d| d.observable)
    }

    /// The current value of an activated setting as it is stored in the properties.
    pub fn value(&self, id: SettingId) -> Option<String> {
        if !self.is_setting_active(id) {
            return None;
        }
        Some(self.read_field(id))
    }

    /// Set the value of an activated setting from its textual (stored) representation. Invalid
    /// values (not one of the options, out of range) are ignored. Returns true if the value
    /// changed.
    pub fn set_value(&mut self, id: SettingId, value: &str) -> bool {
        let Some(normalized) = self.descriptor(id).and_then(|d| d.normalize(value)) else {
            return false;
        };
        self.apply(id, &normalized)
    }

    fn apply(&mut self, id: SettingId, normalized: &str) -> bool {
        let old = self.read_field(id);
        if old == normalized {
            return false;
        }
        self.write_field(id, normalized);
        self.mark_changed(id);
        if self.descriptor(id).is_some_and(|d| d.scope == Scope::Global) {
            self.dirty = true;
        }
        true
    }

    /// Reset a setting to its default value (`IfxSetting.reset`).
    pub fn reset(&mut self, id: SettingId) {
        if let Some(default_value) = self.descriptor(id).map(|d| d.default_value.clone()) {
            self.apply(id, &default_value);
        }
    }

    /// Reset all settings of a scope to their defaults (`MainApp.setDefaultDocumentSettings`
    /// uses this for the document settings when a new project is created).
    pub fn reset_scope(&mut self, scope: Scope) {
        let ids: Vec<SettingId> = self.descriptors.iter().filter(|d| d.scope == scope).map(|d| d.id).collect();
        for id in ids {
            self.reset(id);
        }
    }

    /// Load the values of all activated settings of the given scope. Missing or invalid values
    /// are set to the default. Changed values are recorded.
    pub fn load(&mut self, scope: Scope, properties: &Properties) {
        let entries: Vec<(SettingId, String)> = self
            .descriptors
            .iter()
            .filter(|d| d.scope == scope)
            .map(|d| {
                let value = properties.get_string(&d.key()).and_then(|v| d.normalize(v)).unwrap_or_else(|| d.default_value.clone());
                (d.id, value)
            })
            .collect();
        for (id, value) in entries {
            self.apply(id, &value);
        }
        if scope == Scope::Global {
            self.dirty = false;
        }
    }

    /// Store the values of all activated settings of the given scope. Other entries of the
    /// properties are kept (e.g. the MIDI port names).
    pub fn store(&self, scope: Scope, properties: &mut Properties) {
        for descriptor in self.descriptors.iter().filter(|d| d.scope == scope) {
            properties.put(&descriptor.key(), &self.read_field(descriptor.id));
        }
    }

    /// Format the document settings as properties text (the text which is Base64 encoded and
    /// exchanged with Reaper, see [`super::reaper::format_document_settings`]).
    pub fn store_document_settings(&self) -> String {
        let mut properties = Properties::new();
        self.store(Scope::Document, &mut properties);
        properties.to_text()
    }

    /// Load the document settings from properties text (`DocumentSettingsUI.parse`).
    pub fn load_document_settings(&mut self, properties_text: &str) {
        self.load(Scope::Document, &Properties::parse(properties_text));
    }

    /// True if a global setting was changed since the last load (`GlobalSettingsUI.isDirty`).
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Get (and clear) the IDs of all settings which changed since the last call, in the order of
    /// their first change. Replaces the setting observers.
    pub fn take_changed_settings(&mut self) -> Vec<SettingId> {
        std::mem::take(&mut self.changed)
    }

    /// Check (without clearing) if a setting was changed since the last
    /// [`Self::take_changed_settings`].
    pub fn has_changed(&self, id: SettingId) -> bool {
        self.changed.contains(&id)
    }

    /// Record a change of a setting.
    pub fn mark_changed(&mut self, id: SettingId) {
        if !self.changed.contains(&id) {
            self.changed.push(id);
        }
    }

    /// Mark all observable activated settings as changed, except those registered with
    /// [`Self::add_dont_notify_all`] (replaces `notifyAllObservers`).
    pub fn mark_all_changed(&mut self) {
        let ids: Vec<SettingId> =
            self.descriptors.iter().filter(|d| d.observable && !self.dont_notify_all.contains(&d.id)).map(|d| d.id).collect();
        for id in ids {
            self.mark_changed(id);
        }
    }

    /// Exclude a setting from [`Self::mark_all_changed`] (the Java `dontNotifyAll` set).
    pub fn add_dont_notify_all(&mut self, id: SettingId) {
        self.dont_notify_all.insert(id);
    }

    // ------------------------------------------------------------------------------------------
    // Controller specific settings

    /// The index of the current value of a controller specific enumeration setting in its
    /// options (0 if not active).
    pub fn custom_index(&self, id: SettingId) -> usize {
        match (self.descriptor(id), self.custom_values.get(&id)) {
            (Some(descriptor), Some(value)) => lookup_index(descriptor.options(), value),
            _ => 0,
        }
    }

    /// The value of a controller specific integer setting (0 if not active or not a number).
    pub fn custom_int(&self, id: SettingId) -> i32 {
        self.custom_values.get(&id).and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// The value of a controller specific boolean ("true") or on/off ("On") setting.
    pub fn custom_bool(&self, id: SettingId) -> bool {
        self.custom_values.get(&id).is_some_and(|v| v == "true" || is_on(v))
    }

    /// Set a controller specific enumeration setting by the index of the option.
    pub fn set_custom_index(&mut self, id: SettingId, index: usize) -> bool {
        match self.descriptor(id).and_then(|d| d.options().get(index)).cloned() {
            Some(value) => self.apply(id, &value),
            None => false,
        }
    }

    // ------------------------------------------------------------------------------------------
    // Mapping between the typed fields and the stored values

    fn option_index(&self, id: SettingId, value: &str) -> usize {
        self.descriptor(id).map_or(0, |d| lookup_index(d.options(), value))
    }

    fn option_at(&self, id: SettingId, index: usize) -> String {
        self.descriptor(id).and_then(|d| d.options().get(index).cloned()).unwrap_or_default()
    }

    fn read_field(&self, id: SettingId) -> String {
        let text = |s: &str| s.to_string();
        match id {
            SettingId::SCALES_SCALE => self.scale.clone(),
            SettingId::SCALES_BASE => self.scale_base.clone(),
            SettingId::SCALES_IN_KEY => text(if self.scale_in_key { SCALE_IN_KEY } else { SCALE_CHROMATIC }),
            SettingId::SCALES_LAYOUT => self.scale_layout.clone(),
            SettingId::VU_METER_TYPE => text(on_off(self.enable_vu_meters)),
            SettingId::BEHAVIOUR_ON_STOP => text(self.behavior_on_stop.name()),
            SettingId::BEHAVIOUR_ON_PAUSE => text(self.behavior_on_pause.name()),
            SettingId::FLIP_SESSION => text(on_off(self.flip_session)),
            SettingId::SELECT_CLIP_ON_LAUNCH => text(on_off(self.select_clip_on_launch)),
            SettingId::DRAW_RECORD_STRIPE => text(on_off(self.draw_record_stripe)),
            SettingId::ACTION_FOR_REC_ARMED_PAD => self.option_at(id, self.action_for_rec_armed_pad),
            SettingId::CONVERT_AFTERTOUCH => self.option_at(id, (self.convert_aftertouch + 3).max(0) as usize),
            SettingId::ACTIVATE_FIXED_ACCENT => text(on_off(self.accent_active)),
            SettingId::FIXED_ACCENT_VALUE => self.fixed_accent_value.to_string(),
            SettingId::QUANTIZE_AMOUNT => self.quantize_amount.to_string(),
            SettingId::FLIP_RECORD => text(on_off(self.flip_record)),
            SettingId::INCLUDE_MASTER => text(on_off(self.include_master)),
            SettingId::EXCLUDE_DEACTIVATED_ITEMS => text(on_off(self.exclude_deactivated_items)),
            SettingId::TRACK_NAVIGATION => text(TRACK_NAVIGATION_OPTIONS[usize::from(!self.is_track_navigation_flat)]),
            SettingId::NEW_CLIP_LENGTH => text(NEW_CLIP_LENGTH_VALUES[self.new_clip_length]),
            SettingId::MIDI_EDIT_CHANNEL => (self.midi_edit_channel + 1).to_string(),
            SettingId::AUTO_SELECT_DRUM => text(AUTO_SELECT_DRUM_OPTIONS[usize::from(self.auto_select_drum)]),
            SettingId::TURN_OFF_EMPTY_DRUM_PADS => text(on_off(self.turn_off_empty_drum_pads)),
            SettingId::TURN_OFF_SCALE_PADS => text(on_off(self.turn_off_scale_pads)),
            SettingId::SOUND_DRUM_PADS => text(on_off(self.use_combination_button_to_sound_drum_pads)),
            SettingId::FOOTSWITCH_1 | SettingId::FOOTSWITCH_2 | SettingId::FOOTSWITCH_3 | SettingId::FOOTSWITCH_4 => {
                self.option_at(id, self.footswitch[usize::from(id.0 - SettingId::FOOTSWITCH_1.0)].max(0) as usize)
            }
            SettingId::KNOB_SENSITIVITY_DEFAULT => self.option_at(id, (self.knob_speed_default + 100).max(0) as usize),
            SettingId::KNOB_SENSITIVITY_SLOW => self.option_at(id, (self.knob_speed_slow + 100).max(0) as usize),
            SettingId::ENCODER_KNOB_SLOW_DOWN => self.option_at(id, self.encoder_knob_slow_down.max(0) as usize),
            SettingId::NOTEREPEAT_ACTIVE => text(on_off(self.note_repeat_active)),
            SettingId::NOTEREPEAT_PERIOD => text(self.note_repeat_period.name()),
            SettingId::NOTEREPEAT_LENGTH => text(self.note_repeat_length.name()),
            SettingId::NOTEREPEAT_MODE => self.note_repeat_mode.map(|m| text(m.name())).unwrap_or_default(),
            SettingId::NOTEREPEAT_OCTAVE => self.note_repeat_octave.to_string(),
            SettingId::RECORD_BUTTON_FUNCTION => text(self.record_button_function.name()),
            SettingId::SHIFTED_RECORD_BUTTON_FUNCTION => text(self.shifted_record_button_function.name()),
            SettingId::STARTUP_VIEW => {
                let index = self.startup_views.iter().position(|v| *v == self.startup_view).unwrap_or(0);
                self.option_at(id, index)
            }
            SettingId::STARTUP_MODE => {
                let index = self.startup_modes.iter().position(|m| *m == self.startup_mode).unwrap_or(0);
                self.option_at(id, index)
            }
            SettingId::START_WITH_SESSION_VIEW => text(on_off(self.start_with_session_view)),
            SettingId::ENABLED_MPE_ZONES => text(on_off(self.is_mpe_enabled)),
            SettingId::MPE_PITCHBEND_RANGE => self.mpe_pitch_bend_range.to_string(),
            SettingId::SHOW_PLAYED_CHORDS => text(on_off(self.show_played_chords)),
            SettingId::COLOR_TRACK_STATES => text(on_off(self.color_track_states)),
            SettingId::DOUBLE_CLICK_PLAY_BUTTON => text(on_off(self.enable_double_click_play_button)),
            SettingId::DOUBLE_CLICK_STOP_BUTTON => text(on_off(self.enable_double_click_stop_button)),
            SettingId::CURSOR_KEYS_TRACK_OPTION => self.option_at(id, self.cursor_keys_track_option.max(0) as usize),
            SettingId::CURSOR_KEYS_TRACK_SHIFTED_OPTION => self.option_at(id, self.cursor_keys_track_shifted_option.max(0) as usize),
            SettingId::CURSOR_KEYS_SCENE_OPTION => self.option_at(id, self.cursor_keys_scene_option.max(0) as usize),
            SettingId::CURSOR_KEYS_SCENE_SHIFTED_OPTION => self.option_at(id, self.cursor_keys_scene_shifted_option.max(0) as usize),
            _ => self.custom_values.get(&id).cloned().unwrap_or_default(),
        }
    }

    /// Set the typed field from an already normalized value.
    fn write_field(&mut self, id: SettingId, value: &str) {
        let index = self.option_index(id, value);
        let number = value.parse::<i32>().unwrap_or(0);
        match id {
            SettingId::SCALES_SCALE => self.scale = value.to_string(),
            SettingId::SCALES_BASE => self.scale_base = value.to_string(),
            SettingId::SCALES_IN_KEY => self.scale_in_key = value == SCALE_IN_KEY,
            SettingId::SCALES_LAYOUT => self.scale_layout = value.to_string(),
            SettingId::VU_METER_TYPE => self.enable_vu_meters = is_on(value),
            SettingId::BEHAVIOUR_ON_STOP => self.behavior_on_stop = TransportBehavior::ALL[lookup_index(&TRANSPORT_BEHAVIOUR_VALUES, value)],
            SettingId::BEHAVIOUR_ON_PAUSE => self.behavior_on_pause = TransportBehavior::ALL[lookup_index(&TRANSPORT_BEHAVIOUR_VALUES, value)],
            SettingId::FLIP_SESSION => self.flip_session = is_on(value),
            SettingId::SELECT_CLIP_ON_LAUNCH => self.select_clip_on_launch = is_on(value),
            SettingId::DRAW_RECORD_STRIPE => self.draw_record_stripe = is_on(value),
            SettingId::ACTION_FOR_REC_ARMED_PAD => self.action_for_rec_armed_pad = index,
            SettingId::CONVERT_AFTERTOUCH => self.convert_aftertouch = index as i32 - 3,
            SettingId::ACTIVATE_FIXED_ACCENT => self.accent_active = is_on(value),
            SettingId::FIXED_ACCENT_VALUE => self.fixed_accent_value = number,
            SettingId::QUANTIZE_AMOUNT => self.quantize_amount = number,
            SettingId::FLIP_RECORD => self.flip_record = is_on(value),
            SettingId::INCLUDE_MASTER => self.include_master = is_on(value),
            SettingId::EXCLUDE_DEACTIVATED_ITEMS => self.exclude_deactivated_items = is_on(value),
            SettingId::TRACK_NAVIGATION => self.is_track_navigation_flat = value == TRACK_NAVIGATION_OPTIONS[0],
            SettingId::NEW_CLIP_LENGTH => self.new_clip_length = lookup_index(&NEW_CLIP_LENGTH_VALUES, value),
            SettingId::MIDI_EDIT_CHANNEL => self.midi_edit_channel = (number - 1).clamp(0, 15),
            SettingId::AUTO_SELECT_DRUM => self.auto_select_drum = value == AUTO_SELECT_DRUM_OPTIONS[1],
            SettingId::TURN_OFF_EMPTY_DRUM_PADS => self.turn_off_empty_drum_pads = is_on(value),
            SettingId::TURN_OFF_SCALE_PADS => self.turn_off_scale_pads = is_on(value),
            SettingId::SOUND_DRUM_PADS => self.use_combination_button_to_sound_drum_pads = is_on(value),
            SettingId::FOOTSWITCH_1 | SettingId::FOOTSWITCH_2 | SettingId::FOOTSWITCH_3 | SettingId::FOOTSWITCH_4 => {
                self.footswitch[usize::from(id.0 - SettingId::FOOTSWITCH_1.0)] = index as i32
            }
            SettingId::KNOB_SENSITIVITY_DEFAULT => self.knob_speed_default = index as i32 - 100,
            SettingId::KNOB_SENSITIVITY_SLOW => self.knob_speed_slow = index as i32 - 100,
            SettingId::ENCODER_KNOB_SLOW_DOWN => self.encoder_knob_slow_down = index as i32,
            SettingId::NOTEREPEAT_ACTIVE => self.note_repeat_active = is_on(value),
            SettingId::NOTEREPEAT_PERIOD => self.note_repeat_period = Resolution::by_name(value).unwrap_or(Resolution::Res1_16),
            SettingId::NOTEREPEAT_LENGTH => self.note_repeat_length = Resolution::by_name(value).unwrap_or(Resolution::Res1_16),
            SettingId::NOTEREPEAT_MODE => self.note_repeat_mode = ArpeggiatorMode::by_name(value).or(self.note_repeat_mode),
            SettingId::NOTEREPEAT_OCTAVE => self.note_repeat_octave = number.clamp(0, 8),
            SettingId::RECORD_BUTTON_FUNCTION => {
                if let Some(i) = RECORD_OPTIONS.iter().position(|o| *o == value) {
                    self.record_button_function = RecordFunction::ALL[i];
                }
            }
            SettingId::SHIFTED_RECORD_BUTTON_FUNCTION => {
                if let Some(i) = RECORD_OPTIONS.iter().position(|o| *o == value) {
                    self.shifted_record_button_function = RecordFunction::ALL[i];
                }
            }
            SettingId::STARTUP_VIEW => {
                if let Some(view) = self.startup_views.get(index) {
                    self.startup_view = *view;
                }
            }
            SettingId::STARTUP_MODE => {
                if let Some(mode) = self.startup_modes.get(index) {
                    self.startup_mode = *mode;
                }
            }
            SettingId::START_WITH_SESSION_VIEW => self.start_with_session_view = is_on(value),
            SettingId::ENABLED_MPE_ZONES => self.is_mpe_enabled = is_on(value),
            SettingId::MPE_PITCHBEND_RANGE => self.mpe_pitch_bend_range = number,
            SettingId::SHOW_PLAYED_CHORDS => self.show_played_chords = is_on(value),
            SettingId::COLOR_TRACK_STATES => self.color_track_states = is_on(value),
            SettingId::DOUBLE_CLICK_PLAY_BUTTON => self.enable_double_click_play_button = is_on(value),
            SettingId::DOUBLE_CLICK_STOP_BUTTON => self.enable_double_click_stop_button = is_on(value),
            SettingId::CURSOR_KEYS_TRACK_OPTION => self.cursor_keys_track_option = index as i32,
            SettingId::CURSOR_KEYS_TRACK_SHIFTED_OPTION => self.cursor_keys_track_shifted_option = index as i32,
            SettingId::CURSOR_KEYS_SCENE_OPTION => self.cursor_keys_scene_option = index as i32,
            SettingId::CURSOR_KEYS_SCENE_SHIFTED_OPTION => self.cursor_keys_scene_shifted_option = index as i32,
            _ => {
                self.custom_values.insert(id, value.to_string());
            }
        }
    }

    fn set_on_off(&mut self, id: SettingId, enabled: bool) {
        self.set_value(id, on_off(enabled));
    }

    // ------------------------------------------------------------------------------------------
    // Activation of the settings (the former `activate*Setting (ISettingsUI)` functions)

    pub fn activate_scale_setting(&mut self, scope: Scope) {
        self.activate_scale_setting_with(scope, &to_options(&SCALE_NAMES), SCALE_NAMES[0]);
    }

    pub fn activate_scale_setting_with(&mut self, scope: Scope, scale_names: &[String], default_scale: &str) {
        self.activate_enum_setting(SettingId::SCALES_SCALE, scope, CATEGORY_SCALES, "Scale", scale_names.to_vec(), default_scale, true);
    }

    pub fn activate_scale_base_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(SettingId::SCALES_BASE, scope, CATEGORY_SCALES, "Base", to_options(&SCALE_BASES), SCALE_BASES[0], true);
    }

    pub fn activate_scale_in_scale_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::SCALES_IN_KEY,
            scope,
            CATEGORY_SCALES,
            SCALE_IN_KEY,
            to_options(&SCALE_IN_KEY_OPTIONS),
            SCALE_IN_KEY,
            true,
        );
    }

    pub fn activate_scale_layout_setting(&mut self, scope: Scope) {
        self.activate_scale_layout_setting_with(scope, SCALE_LAYOUT_NAMES[0]);
    }

    pub fn activate_scale_layout_setting_with(&mut self, scope: Scope, default_layout: &str) {
        self.activate_enum_setting(
            SettingId::SCALES_LAYOUT,
            scope,
            CATEGORY_SCALES,
            "Layout",
            to_options(&SCALE_LAYOUT_NAMES),
            default_layout,
            true,
        );
    }

    pub fn activate_enable_vu_meters_setting(&mut self, scope: Scope, category: &str) {
        self.activate_on_off(SettingId::VU_METER_TYPE, scope, category, "VU Meters", true, true);
    }

    pub fn activate_behaviour_on_stop_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::BEHAVIOUR_ON_STOP,
            scope,
            CATEGORY_TRANSPORT,
            "Behaviour on Stop",
            to_options(&TRANSPORT_BEHAVIOUR_VALUES),
            TRANSPORT_BEHAVIOUR_VALUES[0],
            true,
        );
    }

    pub fn activate_behaviour_on_pause_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::BEHAVIOUR_ON_PAUSE,
            scope,
            CATEGORY_TRANSPORT,
            "Behaviour on Pause",
            to_options(&TRANSPORT_BEHAVIOUR_VALUES),
            TRANSPORT_BEHAVIOUR_VALUES[2],
            true,
        );
    }

    pub fn activate_double_click_play_button(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::DOUBLE_CLICK_PLAY_BUTTON, scope, CATEGORY_TRANSPORT, "Double click PLAY returns to zero", true, false);
    }

    pub fn activate_double_click_stop_button(&mut self, scope: Scope) {
        self.activate_on_off(
            SettingId::DOUBLE_CLICK_STOP_BUTTON,
            scope,
            CATEGORY_TRANSPORT,
            "Double click STOP jumps to end of arranger",
            true,
            false,
        );
    }

    pub fn activate_flip_session_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::FLIP_SESSION, scope, CATEGORY_SESSION, "Flip Session", false, true);
    }

    pub fn activate_select_clip_on_launch_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::SELECT_CLIP_ON_LAUNCH, scope, CATEGORY_SESSION, "Select clip/scene on launch", true, true);
    }

    pub fn activate_draw_record_stripe_setting(&mut self, scope: Scope) {
        self.activate_on_off(
            SettingId::DRAW_RECORD_STRIPE,
            scope,
            CATEGORY_SESSION,
            "Display clips of record enabled tracks in red",
            true,
            true,
        );
    }

    pub fn activate_action_for_rec_armed_pad(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::ACTION_FOR_REC_ARMED_PAD,
            scope,
            CATEGORY_SESSION,
            "Action for pressing rec armed empty clip",
            to_options(&ACTIONS_REC_ARMED_PADS),
            ACTIONS_REC_ARMED_PADS[0],
            true,
        );
    }

    pub fn activate_convert_aftertouch_setting(&mut self, scope: Scope) {
        let values = aftertouch_conversion_values();
        let default_value = values[1].clone();
        self.activate_enum_setting(SettingId::CONVERT_AFTERTOUCH, scope, CATEGORY_PADS, "Convert Poly Aftertouch to", values, &default_value, true);
    }

    pub fn activate_accent_active_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::ACTIVATE_FIXED_ACCENT, scope, CATEGORY_PLAY_AND_SEQUENCE, "Activate Fixed Accent", false, true);
    }

    pub fn activate_accent_value_setting(&mut self, scope: Scope) {
        self.activate_range_setting(SettingId::FIXED_ACCENT_VALUE, scope, CATEGORY_PLAY_AND_SEQUENCE, "Fixed Accent Value", 1, 127, 127, true);
    }

    pub fn activate_flip_record_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::FLIP_RECORD, scope, CATEGORY_TRANSPORT, "Flip arranger and clip record / automation", false, true);
    }

    /// Requires a restart: the value is only read once at startup in Java.
    pub fn activate_include_master_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::INCLUDE_MASTER, scope, CATEGORY_WORKFLOW, "Include (Group-)Mastertrack (requires restart)", true, false);
    }

    pub fn activate_exclude_deactivated_items_setting(&mut self, scope: Scope) {
        self.activate_on_off(
            SettingId::EXCLUDE_DEACTIVATED_ITEMS,
            scope,
            CATEGORY_WORKFLOW,
            "Exclude deactivated items (tracks, sends, devices, layers)",
            false,
            true,
        );
    }

    /// Requires a restart: the value is only read once at startup in Java.
    pub fn activate_track_navigation_setting(&mut self, scope: Scope, category: Option<&str>, flat_is_default: bool) {
        let default_value = TRACK_NAVIGATION_OPTIONS[usize::from(!flat_is_default)];
        self.activate_enum_setting(
            SettingId::TRACK_NAVIGATION,
            scope,
            category.unwrap_or(CATEGORY_WORKFLOW),
            "Track Navigation (requires restart)",
            to_options(&TRACK_NAVIGATION_OPTIONS),
            default_value,
            false,
        );
    }

    pub fn activate_new_clip_length_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::NEW_CLIP_LENGTH,
            scope,
            CATEGORY_WORKFLOW,
            "New Clip Length",
            to_options(&NEW_CLIP_LENGTH_VALUES),
            NEW_CLIP_LENGTH_VALUES[2],
            true,
        );
    }

    pub fn activate_quantize_amount_setting(&mut self, scope: Scope) {
        self.activate_range_setting(SettingId::QUANTIZE_AMOUNT, scope, CATEGORY_PLAY_AND_SEQUENCE, "Quantize Amount", 1, 100, 100, true);
    }

    pub fn activate_midi_edit_channel_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::MIDI_EDIT_CHANNEL,
            scope,
            CATEGORY_PLAY_AND_SEQUENCE,
            "MIDI Edit/Insert note channel",
            midi_channel_options(),
            "1",
            true,
        );
    }

    pub fn activate_auto_select_drum_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::AUTO_SELECT_DRUM,
            scope,
            CATEGORY_DRUMS,
            "Auto-select drum settings",
            to_options(&AUTO_SELECT_DRUM_OPTIONS),
            AUTO_SELECT_DRUM_OPTIONS[0],
            true,
        );
    }

    pub fn activate_turn_off_empty_drum_pads_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::TURN_OFF_EMPTY_DRUM_PADS, scope, CATEGORY_DRUMS, "Turn off empty drum pads", false, true);
    }

    pub fn activate_turn_off_scale_pads_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::TURN_OFF_SCALE_PADS, scope, CATEGORY_PLAY_AND_SEQUENCE, "Turn off scale pads", false, true);
    }

    pub fn activate_use_combination_button_to_sound_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::SOUND_DRUM_PADS, scope, CATEGORY_DRUMS, "Use combination button to sound drum pads", false, true);
    }

    /// Activate the footswitch setting with the given index (0-3). Controllers which support
    /// more footswitch functions can pass an extended option list (the first 16 entries must be
    /// [`FOOTSWITCH_VALUES`]).
    pub fn activate_footswitch_setting(&mut self, scope: Scope, index: usize, label: &str, options: Option<Vec<String>>) {
        let options = options.unwrap_or_else(|| to_options(&FOOTSWITCH_VALUES));
        let default_value = options[FOOTSWITCH_DEFAULTS[index]].clone();
        self.activate_enum_setting(SettingId::FOOTSWITCH_1.offset(index as u16), scope, CATEGORY_WORKFLOW, label, options, &default_value, true);
    }

    /// Activate the knob sensitivity settings. The Java defaults are `sensitivity = 100` and
    /// `slow_sensitivity = 60` (indices into [`knob_sensitivity_options`]).
    pub fn activate_knob_speed_setting(&mut self, scope: Scope, sensitivity: usize, slow_sensitivity: usize, only_normal_setting: bool) {
        let options = knob_sensitivity_options();
        let default_value = options[sensitivity].clone();
        self.activate_enum_setting(
            SettingId::KNOB_SENSITIVITY_DEFAULT,
            scope,
            CATEGORY_WORKFLOW,
            "Knob Sensitivity Default",
            options.clone(),
            &default_value,
            true,
        );
        if !only_normal_setting {
            let default_value = options[slow_sensitivity].clone();
            self.activate_enum_setting(SettingId::KNOB_SENSITIVITY_SLOW, scope, CATEGORY_WORKFLOW, "Knob Sensitivity Slow", options, &default_value, true);
        }
    }

    pub fn activate_encoder_knob_speed_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::ENCODER_KNOB_SLOW_DOWN,
            scope,
            CATEGORY_WORKFLOW,
            "Encoder Knob Slow Down",
            encoder_knob_slow_options(),
            "Normal",
            true,
        );
    }

    pub fn activate_note_repeat_setting(&mut self, scope: Scope, capabilities: NoteRepeatCapabilities) {
        self.activate_on_off(SettingId::NOTEREPEAT_ACTIVE, scope, CATEGORY_NOTEREPEAT, "Active", false, true);
        let names = resolution_names();
        let default_value = names[4].clone();
        self.activate_enum_setting(SettingId::NOTEREPEAT_PERIOD, scope, CATEGORY_NOTEREPEAT, "Period", names.clone(), &default_value, true);
        if capabilities.length {
            self.activate_enum_setting(SettingId::NOTEREPEAT_LENGTH, scope, CATEGORY_NOTEREPEAT, "Length", names, &default_value, true);
        }
        if capabilities.mode && !self.arpeggiator_modes.is_empty() {
            let mode_names: Vec<String> = self.arpeggiator_modes.iter().map(|m| m.name().to_string()).collect();
            let default_value = mode_names[1.min(mode_names.len() - 1)].clone();
            self.activate_enum_setting(SettingId::NOTEREPEAT_MODE, scope, CATEGORY_NOTEREPEAT, "Mode", mode_names, &default_value, true);
        }
        if capabilities.octaves {
            self.activate_enum_setting(
                SettingId::NOTEREPEAT_OCTAVE,
                scope,
                CATEGORY_NOTEREPEAT,
                "Octave",
                to_options(&NOTE_REPEAT_OCTAVES),
                NOTE_REPEAT_OCTAVES[1],
                true,
            );
        }
    }

    pub fn activate_record_button_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::RECORD_BUTTON_FUNCTION,
            scope,
            CATEGORY_TRANSPORT,
            "Record button",
            to_options(&RECORD_OPTIONS),
            RECORD_OPTIONS[1],
            true,
        );
    }

    pub fn activate_shifted_record_button_setting(&mut self, scope: Scope) {
        self.activate_enum_setting(
            SettingId::SHIFTED_RECORD_BUTTON_FUNCTION,
            scope,
            CATEGORY_TRANSPORT,
            "Shift + Record button",
            to_options(&RECORD_OPTIONS),
            RECORD_OPTIONS[5],
            true,
        );
    }

    /// Activate the startup view setting. The labels are the view names ([`view_name`]); views
    /// without a name are skipped. The first view is the default.
    pub fn activate_startup_view_setting(&mut self, scope: Scope, views: &[Views]) {
        let named: Vec<(Views, &str)> = views.iter().filter_map(|v| view_name(*v).map(|n| (*v, n))).collect();
        if named.is_empty() {
            return;
        }
        self.startup_views = named.iter().map(|(v, _)| *v).collect();
        let labels: Vec<String> = named.iter().map(|(_, n)| n.to_string()).collect();
        let default_value = labels[0].clone();
        self.activate_enum_setting(SettingId::STARTUP_VIEW, scope, CATEGORY_PLAY_AND_SEQUENCE, "Startup view", labels, &default_value, true);
    }

    /// Activate the startup mode setting. Since there is no mode name table in Rust (yet), the
    /// caller provides the names (`Modes.getModeName` in Java). The first mode is the default.
    pub fn activate_startup_mode_setting(&mut self, scope: Scope, modes: &[(Modes, &str)]) {
        if modes.is_empty() {
            return;
        }
        self.startup_modes = modes.iter().map(|(m, _)| *m).collect();
        let labels: Vec<String> = modes.iter().map(|(_, n)| n.to_string()).collect();
        let default_value = labels[0].clone();
        self.activate_enum_setting(SettingId::STARTUP_MODE, scope, CATEGORY_WORKFLOW, "Startup mode", labels, &default_value, true);
    }

    pub fn activate_start_with_session_view_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::START_WITH_SESSION_VIEW, scope, CATEGORY_PLAY_AND_SEQUENCE, "Start with session view", false, true);
    }

    /// Activates the MPE on/off and the pitch bend range settings. Both are not observable in
    /// Java (but changes are tracked).
    pub fn activate_mpe_setting(&mut self, scope: Scope, category: &str, enable_mpe: bool) {
        self.activate_on_off(SettingId::ENABLED_MPE_ZONES, scope, category, "MIDI Polyphonic Expression (MPE)", enable_mpe, false);
        self.activate_mpe_bend_range(scope, category);
    }

    pub fn activate_mpe_bend_range(&mut self, scope: Scope, category: &str) {
        self.activate_range_setting(SettingId::MPE_PITCHBEND_RANGE, scope, category, "MPE Pitch Bend Sensitivity", 1, 96, 48, false);
    }

    pub fn activate_show_played_chords_setting(&mut self, scope: Scope) {
        self.activate_on_off(SettingId::SHOW_PLAYED_CHORDS, scope, CATEGORY_PLAY_AND_SEQUENCE, "Notify played chords", false, false);
    }

    pub fn activate_color_track_states(&mut self, scope: Scope) {
        self.activate_on_off(
            SettingId::COLOR_TRACK_STATES,
            scope,
            CATEGORY_WORKFLOW,
            "Color all track states (mute, solo, rec arm)",
            true,
            true,
        );
    }

    pub fn activate_cursor_keys_settings(&mut self, scope: Scope) {
        let track = to_options(&CURSOR_KEYS_TRACK_OPTIONS);
        let scene = to_options(&CURSOR_KEYS_SCENE_OPTIONS);
        self.activate_enum_setting(
            SettingId::CURSOR_KEYS_TRACK_OPTION,
            scope,
            CATEGORY_WORKFLOW,
            "Cursor Keys Track Option",
            track.clone(),
            CURSOR_KEYS_TRACK_OPTIONS[0],
            false,
        );
        self.activate_enum_setting(
            SettingId::CURSOR_KEYS_TRACK_SHIFTED_OPTION,
            scope,
            CATEGORY_WORKFLOW,
            "Shifted Cursor Keys Track Option",
            track,
            CURSOR_KEYS_TRACK_OPTIONS[2],
            false,
        );
        self.activate_enum_setting(
            SettingId::CURSOR_KEYS_SCENE_OPTION,
            scope,
            CATEGORY_WORKFLOW,
            "Cursor Keys Scene Option",
            scene.clone(),
            CURSOR_KEYS_SCENE_OPTIONS[1],
            false,
        );
        self.activate_enum_setting(
            SettingId::CURSOR_KEYS_SCENE_SHIFTED_OPTION,
            scope,
            CATEGORY_WORKFLOW,
            "Shifted Cursor Keys Scene Option",
            scene,
            CURSOR_KEYS_SCENE_OPTIONS[0],
            false,
        );
    }

    // ------------------------------------------------------------------------------------------
    // Scales

    pub fn scale(&self) -> &str {
        &self.scale
    }

    pub fn set_scale(&mut self, scale: &str) {
        self.set_value(SettingId::SCALES_SCALE, scale);
    }

    pub fn scale_base(&self) -> &str {
        &self.scale_base
    }

    pub fn set_scale_base(&mut self, scale_base: &str) {
        self.set_value(SettingId::SCALES_BASE, scale_base);
    }

    pub fn is_scale_in_key(&self) -> bool {
        self.scale_in_key
    }

    pub fn set_scale_in_key(&mut self, in_key: bool) {
        self.set_value(SettingId::SCALES_IN_KEY, if in_key { SCALE_IN_KEY } else { SCALE_CHROMATIC });
    }

    pub fn scale_layout(&self) -> &str {
        &self.scale_layout
    }

    pub fn set_scale_layout(&mut self, scale_layout: &str) {
        self.set_value(SettingId::SCALES_LAYOUT, scale_layout);
    }

    // ------------------------------------------------------------------------------------------
    // Transport

    pub fn is_enable_vu_meters(&self) -> bool {
        self.enable_vu_meters
    }

    pub fn set_vu_meters_enabled(&mut self, enabled: bool) {
        self.set_on_off(SettingId::VU_METER_TYPE, enabled);
    }

    pub fn behaviour_on_stop(&self) -> TransportBehavior {
        self.behavior_on_stop
    }

    pub fn behaviour_on_pause(&self) -> TransportBehavior {
        self.behavior_on_pause
    }

    pub fn is_flip_record(&self) -> bool {
        self.flip_record
    }

    pub fn record_button_function(&self) -> RecordFunction {
        self.record_button_function
    }

    pub fn shifted_record_button_function(&self) -> RecordFunction {
        self.shifted_record_button_function
    }

    /// Java: `isEnableDoubleClickPlayButton`.
    pub fn is_enable_double_click_play_button(&self) -> bool {
        self.enable_double_click_play_button
    }

    /// True if a double click on PLAY returns the play cursor to zero.
    pub fn is_play_double_click_return_to_zero(&self) -> bool {
        self.enable_double_click_play_button
    }

    /// Java: `isEnableDoubleClickStopButton`.
    pub fn is_enable_double_click_stop_button(&self) -> bool {
        self.enable_double_click_stop_button
    }

    /// True if a double click on STOP jumps to the end of the arranger.
    pub fn is_stop_double_click_jump_to_end(&self) -> bool {
        self.enable_double_click_stop_button
    }

    // ------------------------------------------------------------------------------------------
    // Play and sequence

    pub fn is_accent_active(&self) -> bool {
        self.accent_active
    }

    pub fn set_accent_enabled(&mut self, enabled: bool) {
        self.set_on_off(SettingId::ACTIVATE_FIXED_ACCENT, enabled);
    }

    pub fn fixed_accent_value(&self) -> i32 {
        self.fixed_accent_value
    }

    pub fn set_fixed_accent_value(&mut self, value: i32) {
        self.set_value(SettingId::FIXED_ACCENT_VALUE, &value.to_string());
    }

    pub fn quantize_amount(&self) -> i32 {
        self.quantize_amount
    }

    /// Set the quantize amount (1-100), other values are ignored.
    pub fn set_quantize_amount(&mut self, value: i32) {
        self.set_value(SettingId::QUANTIZE_AMOUNT, &value.to_string());
    }

    /// Change the quantize amount with a relative knob value.
    pub fn change_quantize_amount(&mut self, value_changer: &ValueChanger, control: i32) {
        let value = value_changer.change_value_with(control, self.quantize_amount, -100.0, 101);
        self.set_quantize_amount(value);
    }

    pub fn reset_quantize_amount(&mut self) {
        self.set_quantize_amount(100);
    }

    pub fn convert_aftertouch(&self) -> i32 {
        self.convert_aftertouch
    }

    pub fn midi_edit_channel(&self) -> i32 {
        self.midi_edit_channel
    }

    pub fn set_midi_edit_channel(&mut self, midi_channel: i32) {
        self.set_value(SettingId::MIDI_EDIT_CHANNEL, &(midi_channel.clamp(0, 15) + 1).to_string());
    }

    pub fn is_turn_off_scale_pads(&self) -> bool {
        self.turn_off_scale_pads
    }

    pub fn is_show_played_chords(&self) -> bool {
        self.show_played_chords
    }

    /// The preferred view for tracks which can hold notes (Java: `getStartupView`).
    pub fn startup_view(&self) -> Views {
        self.startup_view
    }

    /// Same as [`Self::startup_view`].
    pub fn preferred_note_view(&self) -> Views {
        self.startup_view
    }

    /// The preferred view for audio tracks (not a setting, set by the controller configuration).
    pub fn preferred_audio_view(&self) -> Views {
        self.preferred_audio_view
    }

    pub fn set_preferred_audio_view(&mut self, view: Views) {
        self.preferred_audio_view = view;
    }

    pub fn startup_mode(&self) -> Modes {
        self.startup_mode
    }

    pub fn should_start_with_session_view(&self) -> bool {
        self.start_with_session_view
    }

    // ------------------------------------------------------------------------------------------
    // Session

    pub fn is_flip_session(&self) -> bool {
        self.flip_session
    }

    pub fn set_flip_session(&mut self, enabled: bool) {
        self.set_on_off(SettingId::FLIP_SESSION, enabled);
    }

    pub fn is_select_clip_on_launch(&self) -> bool {
        self.select_clip_on_launch
    }

    pub fn is_draw_record_stripe(&self) -> bool {
        self.draw_record_stripe
    }

    /// 0 = start recording, 1 = create new clip, 2 = do nothing.
    pub fn action_for_rec_armed_pad(&self) -> usize {
        self.action_for_rec_armed_pad
    }

    // ------------------------------------------------------------------------------------------
    // Drums

    pub fn is_auto_select_drum(&self) -> bool {
        self.auto_select_drum
    }

    pub fn is_turn_off_empty_drum_pads(&self) -> bool {
        self.turn_off_empty_drum_pads
    }

    pub fn is_combination_button_to_sound_drum_pads(&self) -> bool {
        self.use_combination_button_to_sound_drum_pads
    }

    // ------------------------------------------------------------------------------------------
    // Workflow

    /// The index (0-7) of the length for new clips, see [`NEW_CLIP_LENGTH_VALUES`].
    pub fn new_clip_length(&self) -> usize {
        self.new_clip_length
    }

    /// The length for new clips in beats (Java: `getNewClipLenghthInBeats`).
    pub fn new_clip_length_in_beats(&self, quarters_per_measure: i32) -> i32 {
        if self.new_clip_length < 2 {
            1 << self.new_clip_length
        } else {
            (1 << (self.new_clip_length - 2)) * quarters_per_measure
        }
    }

    /// Set the index (0-7) of the length for new clips.
    pub fn set_new_clip_length(&mut self, index: usize) {
        if let Some(value) = NEW_CLIP_LENGTH_VALUES.get(index) {
            self.set_value(SettingId::NEW_CLIP_LENGTH, value);
        }
    }

    pub fn next_new_clip_length(&mut self) {
        let index = (self.new_clip_length + 1) % NEW_CLIP_LENGTH_VALUES.len();
        self.set_new_clip_length(index);
    }

    pub fn footswitch(&self, index: usize) -> i32 {
        self.footswitch[index]
    }

    /// In the range of [-100, 100], 0 is normal, negative is slower, positive faster.
    pub fn knob_sensitivity_default(&self) -> i32 {
        self.knob_speed_default
    }

    pub fn knob_sensitivity_slow(&self) -> i32 {
        self.knob_speed_slow
    }

    pub fn encoder_knob_slow_down(&self) -> i32 {
        self.encoder_knob_slow_down
    }

    pub fn are_master_tracks_included(&self) -> bool {
        self.include_master
    }

    pub fn are_deactivated_items_excluded(&self) -> bool {
        self.exclude_deactivated_items
    }

    /// Same as [`Self::are_deactivated_items_excluded`].
    pub fn are_deactivated_items_hidden(&self) -> bool {
        self.exclude_deactivated_items
    }

    pub fn is_track_navigation_flat(&self) -> bool {
        self.is_track_navigation_flat
    }

    pub fn is_color_track_states(&self) -> bool {
        self.color_track_states
    }

    pub fn cursor_keys_track_option(&self) -> i32 {
        self.cursor_keys_track_option
    }

    pub fn cursor_keys_track_shifted_option(&self) -> i32 {
        self.cursor_keys_track_shifted_option
    }

    pub fn cursor_keys_scene_option(&self) -> i32 {
        self.cursor_keys_scene_option
    }

    pub fn cursor_keys_scene_shifted_option(&self) -> i32 {
        self.cursor_keys_scene_shifted_option
    }

    pub fn is_delete_mode_active(&self) -> bool {
        self.is_delete_active
    }

    pub fn toggle_delete_mode_active(&mut self) {
        self.is_delete_active = !self.is_delete_active;
        if self.is_delete_active {
            self.is_duplicate_active = false;
        }
    }

    pub fn is_duplicate_mode_active(&self) -> bool {
        self.is_duplicate_active
    }

    pub fn toggle_duplicate_mode_active(&mut self) {
        self.is_duplicate_active = !self.is_duplicate_active;
        if self.is_duplicate_active {
            self.is_delete_active = false;
        }
    }

    // ------------------------------------------------------------------------------------------
    // Note repeat

    pub fn is_note_repeat_active(&self) -> bool {
        self.note_repeat_active
    }

    pub fn set_note_repeat_active(&mut self, active: bool) {
        self.set_on_off(SettingId::NOTEREPEAT_ACTIVE, active);
    }

    pub fn toggle_note_repeat_active(&mut self) {
        self.set_note_repeat_active(!self.note_repeat_active);
    }

    pub fn note_repeat_period(&self) -> Resolution {
        self.note_repeat_period
    }

    pub fn set_note_repeat_period(&mut self, period: Resolution) {
        self.set_value(SettingId::NOTEREPEAT_PERIOD, period.name());
    }

    pub fn note_repeat_length(&self) -> Resolution {
        self.note_repeat_length
    }

    pub fn set_note_repeat_length(&mut self, length: Resolution) {
        self.set_value(SettingId::NOTEREPEAT_LENGTH, length.name());
    }

    pub fn note_repeat_mode(&self) -> Option<ArpeggiatorMode> {
        self.note_repeat_mode
    }

    pub fn set_note_repeat_mode(&mut self, mode: ArpeggiatorMode) {
        self.set_value(SettingId::NOTEREPEAT_MODE, mode.name());
    }

    /// Select the next/previous note repeat mode (clamped, `setPrevNextNoteRepeatMode`).
    pub fn set_prev_next_note_repeat_mode(&mut self, increase: bool) {
        if self.arpeggiator_modes.is_empty() {
            return;
        }
        let index = self.lookup_arpeggiator_mode_index(self.note_repeat_mode);
        let new_index = if increase { (index + 1).min(self.arpeggiator_modes.len() - 1) } else { index.saturating_sub(1) };
        self.set_note_repeat_mode(self.arpeggiator_modes[new_index]);
    }

    pub fn note_repeat_octave(&self) -> i32 {
        self.note_repeat_octave
    }

    pub fn set_note_repeat_octave(&mut self, octave: i32) {
        self.set_value(SettingId::NOTEREPEAT_OCTAVE, &octave.clamp(0, 8).to_string());
    }

    pub fn arpeggiator_modes(&self) -> &[ArpeggiatorMode] {
        &self.arpeggiator_modes
    }

    /// The index of the mode in the available arpeggiator modes (0 if not found).
    pub fn lookup_arpeggiator_mode_index(&self, mode: Option<ArpeggiatorMode>) -> usize {
        mode.and_then(|m| self.arpeggiator_modes.iter().position(|a| *a == m)).unwrap_or(0)
    }

    /// The mode after the current note repeat mode (wraps around).
    pub fn next_arpeggiator_mode(&self) -> Option<ArpeggiatorMode> {
        if self.arpeggiator_modes.is_empty() {
            return None;
        }
        let index = self.lookup_arpeggiator_mode_index(self.note_repeat_mode) + 1;
        Some(self.arpeggiator_modes[if index < self.arpeggiator_modes.len() { index } else { 0 }])
    }

    /// The mode before the current note repeat mode (wraps around).
    pub fn prev_arpeggiator_mode(&self) -> Option<ArpeggiatorMode> {
        let index = self.lookup_arpeggiator_mode_index(self.note_repeat_mode);
        let len = self.arpeggiator_modes.len();
        self.arpeggiator_modes.get(if index == 0 { len.checked_sub(1)? } else { index - 1 }).copied()
    }

    // ------------------------------------------------------------------------------------------
    // MPE

    pub fn is_mpe_enabled(&self) -> bool {
        self.is_mpe_enabled
    }

    pub fn set_mpe_enabled(&mut self, enable: bool) {
        self.set_on_off(SettingId::ENABLED_MPE_ZONES, enable);
    }

    pub fn change_mpe_enabled(&mut self, value_changer: &ValueChanger, control: i32) {
        self.set_mpe_enabled(value_changer.is_increase(control));
    }

    pub fn mpe_pitch_bend_range(&self) -> i32 {
        self.mpe_pitch_bend_range
    }

    pub fn set_mpe_pitchbend_range(&mut self, value: i32) {
        self.set_value(SettingId::MPE_PITCHBEND_RANGE, &value.clamp(1, 96).to_string());
    }

    pub fn change_mpe_pitchbend_range(&mut self, value_changer: &ValueChanger, control: i32) {
        // Out of range values are ignored by the integer setting
        let value = value_changer.change_value_with(control, self.mpe_pitch_bend_range, -100.0, 97);
        self.set_value(SettingId::MPE_PITCHBEND_RANGE, &value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Configuration {
        let mut c = Configuration::new(&REAPER_ARP_MODES);
        c.activate_scale_setting(Scope::Document);
        c.activate_scale_base_setting(Scope::Document);
        c.activate_scale_in_scale_setting(Scope::Document);
        c.activate_scale_layout_setting(Scope::Document);
        c.activate_note_repeat_setting(Scope::Document, NoteRepeatCapabilities::REAPER);
        c.activate_accent_active_setting(Scope::Global);
        c.activate_accent_value_setting(Scope::Global);
        c.activate_quantize_amount_setting(Scope::Global);
        c.activate_startup_view_setting(Scope::Global, &[Views::Play, Views::Chords, Views::Drum64]);
        c.activate_show_played_chords_setting(Scope::Global);
        c.activate_convert_aftertouch_setting(Scope::Global);
        c.activate_knob_speed_setting(Scope::Global, 100, 60, false);
        c.activate_footswitch_setting(Scope::Global, 0, "Footswitch 1", None);
        c.activate_new_clip_length_setting(Scope::Global);
        c.activate_double_click_play_button(Scope::Global);
        c.activate_mpe_setting(Scope::Global, CATEGORY_PADS, false);
        c
    }

    #[test]
    fn defaults() {
        let c = config();
        assert_eq!(c.scale(), "Major");
        assert_eq!(c.scale_base(), "C");
        assert!(c.is_scale_in_key());
        assert_eq!(c.scale_layout(), "4th ^");
        assert!(!c.is_note_repeat_active());
        // The setting default (1/16) overrides the field default (1/8)
        assert_eq!(c.note_repeat_period(), Resolution::Res1_16);
        assert_eq!(c.note_repeat_length(), Resolution::Res1_16);
        assert_eq!(c.note_repeat_mode(), Some(ArpeggiatorMode::Up));
        assert!(!c.is_accent_active());
        assert_eq!(c.fixed_accent_value(), 127);
        assert_eq!(c.quantize_amount(), 100);
        assert_eq!(c.startup_view(), Views::Play);
        assert!(!c.is_show_played_chords());
        assert_eq!(c.convert_aftertouch(), AFTERTOUCH_CONVERT_POLY);
        assert_eq!(c.knob_sensitivity_default(), 0);
        assert_eq!(c.knob_sensitivity_slow(), -40);
        assert_eq!(c.footswitch(0), FOOTSWITCH_SUSTAIN_PEDAL);
        assert_eq!(c.new_clip_length(), 2);
        assert_eq!(c.new_clip_length_in_beats(4), 4);
        assert!(c.is_play_double_click_return_to_zero());
        assert_eq!(c.mpe_pitch_bend_range(), 48);
        // Not activated: Java field defaults
        assert!(c.is_select_clip_on_launch());
        assert_eq!(c.record_button_function(), RecordFunction::RecordArranger);
        assert!(c.is_show_played_chords() == c.value(SettingId::SHOW_PLAYED_CHORDS).is_some_and(|v| v == "On"));
        assert!(c.can_setting_be_observed(SettingId::SCALES_SCALE));
        assert!(!c.can_setting_be_observed(SettingId::SHOW_PLAYED_CHORDS));
        assert!(!c.can_setting_be_observed(SettingId::ENABLED_MPE_ZONES));
        assert!(!c.can_setting_be_observed(SettingId::FLIP_SESSION));
        assert!(!c.is_dirty());
    }

    #[test]
    fn change_tracking() {
        let mut c = config();
        assert!(c.take_changed_settings().is_empty());
        c.set_accent_enabled(true);
        c.set_accent_enabled(true);
        c.set_scale("Dorian");
        c.set_quantize_amount(0); // Out of range: ignored
        c.set_fixed_accent_value(127); // Unchanged
        c.set_flip_session(true); // Not active: ignored
        assert!(c.has_changed(SettingId::SCALES_SCALE));
        assert_eq!(c.take_changed_settings(), vec![SettingId::ACTIVATE_FIXED_ACCENT, SettingId::SCALES_SCALE]);
        assert!(c.take_changed_settings().is_empty());
        assert!(c.is_accent_active());
        assert_eq!(c.quantize_amount(), 100);
        assert!(!c.is_flip_session());
        assert!(c.is_dirty());

        c.toggle_note_repeat_active();
        c.set_note_repeat_period(Resolution::Res1_32T);
        c.set_prev_next_note_repeat_mode(true);
        c.next_new_clip_length();
        assert_eq!(
            c.take_changed_settings(),
            vec![SettingId::NOTEREPEAT_ACTIVE, SettingId::NOTEREPEAT_PERIOD, SettingId::NOTEREPEAT_MODE, SettingId::NEW_CLIP_LENGTH]
        );
        assert_eq!(c.note_repeat_mode(), Some(ArpeggiatorMode::DownUp));
        assert_eq!(c.new_clip_length(), 3);

        c.mark_all_changed();
        let all = c.take_changed_settings();
        assert!(all.contains(&SettingId::SCALES_LAYOUT));
        assert!(!all.contains(&SettingId::SHOW_PLAYED_CHORDS));

        c.add_dont_notify_all(SettingId::SCALES_LAYOUT);
        c.mark_all_changed();
        assert!(!c.take_changed_settings().contains(&SettingId::SCALES_LAYOUT));
    }

    #[test]
    fn quantize_with_value_changer() {
        let mut c = config();
        let vc = ValueChanger::twos_complement(1024, 10);
        c.change_quantize_amount(&vc, 127); // -1 step
        assert_eq!(c.quantize_amount(), 99);
        c.change_quantize_amount(&vc, 1);
        c.change_quantize_amount(&vc, 1); // 101 is out of range
        assert_eq!(c.quantize_amount(), 100);
        c.reset_quantize_amount();
        assert_eq!(c.quantize_amount(), 100);
    }

    #[test]
    fn global_properties_round_trip() {
        let mut c = config();
        c.set_accent_enabled(true);
        c.set_fixed_accent_value(99);
        c.set_value(SettingId::STARTUP_VIEW, "Drum 64");
        c.set_value(SettingId::CONVERT_AFTERTOUCH, "CC 7");
        c.set_value(SettingId::KNOB_SENSITIVITY_SLOW, "+5");
        c.set_value(SettingId::FOOTSWITCH_1, "Undo");
        let mut props = Properties::new();
        props.put("MIDI_INPUT0", "Launchpad X");
        c.store(Scope::Global, &mut props);
        assert_eq!(props.get("PLAY_AND_SEQUENCE_ACTIVATE_FIXED_ACCENT"), Some("On"));
        assert_eq!(props.get("PLAY_AND_SEQUENCE_FIXED_ACCENT_VALUE"), Some("99"));
        assert_eq!(props.get("PLAY_AND_SEQUENCE_STARTUP_VIEW"), Some("Drum 64"));
        assert_eq!(props.get("PADS_CONVERT_POLY_AFTERTOUCH_TO"), Some("CC 7"));
        assert_eq!(props.get("WORKFLOW_FOOTSWITCH_1"), Some("Undo"));
        // Document settings are not part of the global settings
        assert_eq!(props.get("SCALES_SCALE"), None);

        let text = props.to_text();
        let mut d = config();
        d.load(Scope::Global, &Properties::parse(&text));
        assert!(d.is_accent_active());
        assert_eq!(d.fixed_accent_value(), 99);
        assert_eq!(d.startup_view(), Views::Drum64);
        assert_eq!(d.convert_aftertouch(), 7);
        assert_eq!(d.knob_sensitivity_slow(), 5);
        assert_eq!(d.footswitch(0), FOOTSWITCH_UNDO);
        assert!(d.take_changed_settings().contains(&SettingId::FIXED_ACCENT_VALUE));
        assert!(!d.is_dirty());

        let mut again = Properties::parse(&text);
        d.store(Scope::Global, &mut again);
        assert_eq!(again, Properties::parse(&text));
    }

    #[test]
    fn invalid_values_load_defaults() {
        let mut c = config();
        let mut props = Properties::new();
        props.put("PLAY_AND_SEQUENCE_FIXED_ACCENT_VALUE", "500");
        props.put("PLAY_AND_SEQUENCE_STARTUP_VIEW", "Unknown");
        props.put("PLAY_AND_SEQUENCE_QUANTIZE_AMOUNT", "42");
        c.load(Scope::Global, &props);
        assert_eq!(c.fixed_accent_value(), 127);
        assert_eq!(c.startup_view(), Views::Play);
        assert_eq!(c.quantize_amount(), 42);
    }

    #[test]
    fn document_settings_round_trip() {
        let mut c = config();
        c.set_scale("Minor Blues");
        c.set_scale_base("Eb");
        c.set_scale_in_key(false);
        c.set_scale_layout("8th > centered");
        c.set_note_repeat_active(true);
        c.set_note_repeat_length(Resolution::Res1_4T);
        c.set_note_repeat_mode(ArpeggiatorMode::Down);
        assert!(!c.is_dirty());
        let text = c.store_document_settings();
        assert!(text.contains("SCALES_SCALE=Minor Blues\n"));
        assert!(text.contains("SCALES_IN_KEY=Chromatic\n"));
        assert!(text.contains("NOTE_REPEAT_ACTIVE=On\n"));
        assert!(text.contains("NOTE_REPEAT_LENGTH=1/4t\n"));

        let mut d = config();
        d.load_document_settings(&text);
        assert_eq!(d.scale(), "Minor Blues");
        assert_eq!(d.scale_base(), "Eb");
        assert!(!d.is_scale_in_key());
        assert_eq!(d.scale_layout(), "8th > centered");
        assert!(d.is_note_repeat_active());
        assert_eq!(d.note_repeat_length(), Resolution::Res1_4T);
        assert_eq!(d.note_repeat_mode(), Some(ArpeggiatorMode::Down));

        d.reset_scope(Scope::Document);
        assert_eq!(d.scale(), "Major");
        assert!(!d.is_note_repeat_active());
    }

    #[test]
    fn arpeggiator_navigation() {
        let c = config();
        // Up is at index 1 of [Down, Up, DownUp, UpDown]
        assert_eq!(c.next_arpeggiator_mode(), Some(ArpeggiatorMode::DownUp));
        assert_eq!(c.prev_arpeggiator_mode(), Some(ArpeggiatorMode::Down));
        let empty = Configuration::new(&[]);
        assert_eq!(empty.next_arpeggiator_mode(), None);
        assert_eq!(empty.prev_arpeggiator_mode(), None);
    }
}
