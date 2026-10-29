// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The option lists and value types of the settings (from `AbstractConfiguration`, `Resolution`,
//! `ArpeggiatorMode`, `Scale`, `ScaleLayout`, `Scales` and `Views`).
//!
//! The option texts are stored in the configuration files, therefore they must stay identical to
//! the Java version.

use crate::framework::ids::Views;

/// The Off/On option.
pub const ON_OFF_OPTIONS: [&str; 2] = ["Off", "On"];

/// The Flat/Hierarchical tracks option.
pub const TRACK_NAVIGATION_OPTIONS: [&str; 2] = ["Flat", "Hierarchical"];

pub const SCALE_IN_KEY: &str = "In Key";
pub const SCALE_CHROMATIC: &str = "Chromatic";
pub const SCALE_IN_KEY_OPTIONS: [&str; 2] = [SCALE_IN_KEY, SCALE_CHROMATIC];

/// The names of all scales (`Scale.getNames ()`).
pub const SCALE_NAMES: [&str; 37] = [
    "Major",
    "Minor",
    "Dorian",
    "Mixolydian",
    "Lydian",
    "Phrygian",
    "Locrian",
    "Whole Tone",
    "Half-whole Dim.",
    "Whole-half Dim.",
    "Minor Blues",
    "Minor Pentatonic",
    "Major Pentatonic",
    "Harmonic Minor",
    "Harmonic Major",
    "Dorian #4",
    "Phrygian Dominant",
    "Melodic Minor",
    "Lydian Augmented",
    "Lydian Dominant",
    "Super Locrian",
    "Spanish",
    "8-Tone Spanish",
    "Bhairav",
    "Hungarian Minor",
    "Hirojoshi",
    "In-Sen",
    "Iwato",
    "Kumoi",
    "Pelog",
    "Pelog Selisir",
    "Pelog Tembung",
    "Messiaen 3",
    "Messiaen 4",
    "Messiaen 5",
    "Messiaen 6",
    "Messiaen 7",
];

/// The base notes of the scales in the order of the circle of fifths (`Scales.BASES`).
pub const SCALE_BASES: [&str; 12] = ["C", "G", "D", "A", "E", "B", "F", "Bb", "Eb", "Ab", "Db", "Gb"];

/// The names of the scale layouts (`ScaleLayout.getNames ()`, the typo 'Seqent' is original).
pub const SCALE_LAYOUT_NAMES: [&str; 12] = [
    "4th ^",
    "4th >",
    "3rd ^",
    "3rd >",
    "Seqent ^",
    "Seqent >",
    "8th ^",
    "8th >",
    "8th ^ centered",
    "8th > centered",
    "Staggered ^",
    "Staggered >",
];

/// The names for clip lengths.
pub const NEW_CLIP_LENGTH_VALUES: [&str; 8] = ["1 Beat", "2 Beat", "1 Bar", "2 Bars", "4 Bars", "8 Bars", "16 Bars", "32 Bars"];

pub const TRANSPORT_BEHAVIOUR_VALUES: [&str; 3] = ["Stop", "Return to Zero", "Pause"];

pub const ACTIONS_REC_ARMED_PADS: [&str; 3] = ["Start recording", "Create new clip", "Do nothing"];

pub const FOOTSWITCH_VALUES: [&str; 16] = [
    "Toggle Play",
    "Toggle Record",
    "Stop All Clips",
    "Toggle Clip Overdub",
    "Undo",
    "Tap Tempo",
    "New Button",
    "Clip Based Looper",
    "Panel layout arrange",
    "Panel layout mix",
    "Panel layout edit",
    "Add instrument track",
    "Add audio track",
    "Add effect track",
    "Quantize",
    "Sustain Pedal",
];

/// The default footswitch functions (indices into [`FOOTSWITCH_VALUES`]).
pub const FOOTSWITCH_DEFAULTS: [usize; 4] = [15, 6, 4, 14];

pub const FOOTSWITCH_TOGGLE_PLAY: i32 = 0;
pub const FOOTSWITCH_TOGGLE_RECORD: i32 = 1;
pub const FOOTSWITCH_STOP_ALL_CLIPS: i32 = 2;
pub const FOOTSWITCH_TOGGLE_CLIP_OVERDUB: i32 = 3;
pub const FOOTSWITCH_UNDO: i32 = 4;
pub const FOOTSWITCH_TAP_TEMPO: i32 = 5;
pub const FOOTSWITCH_NEW_BUTTON: i32 = 6;
pub const FOOTSWITCH_CLIP_BASED_LOOPER: i32 = 7;
pub const FOOTSWITCH_PANEL_LAYOUT_ARRANGE: i32 = 8;
pub const FOOTSWITCH_PANEL_LAYOUT_MIX: i32 = 9;
pub const FOOTSWITCH_PANEL_LAYOUT_EDIT: i32 = 10;
pub const FOOTSWITCH_ADD_INSTRUMENT_TRACK: i32 = 11;
pub const FOOTSWITCH_ADD_AUDIO_TRACK: i32 = 12;
pub const FOOTSWITCH_ADD_EFFECT_TRACK: i32 = 13;
pub const FOOTSWITCH_QUANTIZE: i32 = 14;
pub const FOOTSWITCH_SUSTAIN_PEDAL: i32 = 15;

pub const RECORD_OPTIONS: [&str; 9] = [
    "Record arranger",
    "Record arranger + enable automation",
    "Record clip",
    "Record clip + enable automation",
    "New clip",
    "New clip + enable automation",
    "Toggle arranger overdub",
    "Toggle clip overdub",
    "Toggle rec arm",
];

pub const CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE: i32 = 0;
pub const CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_1: i32 = 1;
pub const CURSOR_KEYS_TRACK_OPTION_SWAP: i32 = 2;
pub const CURSOR_KEYS_TRACK_OPTIONS: [&str; 3] = ["Scroll track bank by page", "Scroll track bank by 1", "Swap tracks"];

pub const CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_PAGE: i32 = 0;
pub const CURSOR_KEYS_SCENE_OPTION_MOVE_BANK_BY_1: i32 = 1;
pub const CURSOR_KEYS_SCENE_OPTIONS: [&str; 2] = ["Scroll scene bank by page", "Scroll scene bank by 1"];

pub const AUTO_SELECT_DRUM_OPTIONS: [&str; 2] = ["Off", "Channel"];

pub const NOTE_REPEAT_OCTAVES: [&str; 9] = ["0", "1", "2", "3", "4", "5", "6", "7", "8"];

/// Aftertouch conversion is set to off.
pub const AFTERTOUCH_CONVERT_OFF: i32 = -3;
/// Aftertouch conversion is set to poly aftertouch.
pub const AFTERTOUCH_CONVERT_POLY: i32 = -2;
/// Aftertouch conversion is set to channel aftertouch.
pub const AFTERTOUCH_CONVERT_CHANNEL: i32 = -1;

/// "Off", "Poly Aftertouch", "Channel Aftertouch", "CC 0" .. "CC 127".
pub fn aftertouch_conversion_values() -> Vec<String> {
    let mut values = vec!["Off".to_string(), "Poly Aftertouch".to_string(), "Channel Aftertouch".to_string()];
    values.extend((0..128).map(|i| format!("CC {i}")));
    values
}

/// "1" .. "16".
pub fn midi_channel_options() -> Vec<String> {
    (1..=16).map(|i| i.to_string()).collect()
}

/// "-100" .. "-1", "Normal", "+1" .. "+100" (index 100 is "Normal").
pub fn knob_sensitivity_options() -> Vec<String> {
    let mut values: Vec<String> = (0..100).map(|i| format!("-{}", 100 - i)).collect();
    values.push("Normal".to_string());
    values.extend((1..=100).map(|i| format!("+{i}")));
    values
}

/// "Normal", "1" .. "100".
pub fn encoder_knob_slow_options() -> Vec<String> {
    let mut values = vec!["Normal".to_string()];
    values.extend((1..=100).map(|i| i.to_string()));
    values
}

/// Returns the index of the value in the options or 0 if not found (`lookupIndex`).
pub fn lookup_index<S: AsRef<str>>(options: &[S], value: &str) -> usize {
    options.iter().position(|o| o.as_ref() == value).unwrap_or(0)
}

/// Converts a list of static strings into owned option strings.
pub fn to_options(options: &[&str]) -> Vec<String> {
    options.iter().map(|o| o.to_string()).collect()
}

/// The behavior when the stop/pause button is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportBehavior {
    /// Keep the play cursor at the current position on stop.
    Stop,
    /// Move the cursor back to zero on stop.
    ReturnToZero,
    /// Only pause on stop.
    Pause,
}

impl TransportBehavior {
    pub const ALL: [TransportBehavior; 3] = [TransportBehavior::Stop, TransportBehavior::ReturnToZero, TransportBehavior::Pause];

    pub fn name(self) -> &'static str {
        TRANSPORT_BEHAVIOUR_VALUES[self as usize]
    }
}

/// Different options for the record button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordFunction {
    /// Record in arranger.
    RecordArranger,
    /// Record in arranger and enable arranger automation writing.
    RecordArrangerAndEnableAutomation,
    /// Record in clip.
    RecordClip,
    /// Record in clip and enable clip automation writing.
    RecordClipAndEnableAutomation,
    /// Create a new clip, enable overdub and start playback.
    NewClip,
    /// ... and enable clip automation writing.
    NewClipAndEnableAutomation,
    /// Toggle arranger overdub.
    ToggleArrangerOverdub,
    /// Toggle clip overdub.
    ToggleClipOverdub,
    /// Toggle record arm.
    ToggleRecArm,
}

impl RecordFunction {
    pub const ALL: [RecordFunction; 9] = [
        RecordFunction::RecordArranger,
        RecordFunction::RecordArrangerAndEnableAutomation,
        RecordFunction::RecordClip,
        RecordFunction::RecordClipAndEnableAutomation,
        RecordFunction::NewClip,
        RecordFunction::NewClipAndEnableAutomation,
        RecordFunction::ToggleArrangerOverdub,
        RecordFunction::ToggleClipOverdub,
        RecordFunction::ToggleRecArm,
    ];

    pub fn name(self) -> &'static str {
        RECORD_OPTIONS[self as usize]
    }
}

/// Note resolutions (former `de.mossgrabers.framework.daw.constants.Resolution`), shared with the
/// DAW model.
pub use crate::daw::constants::Resolution;

/// The names of all resolutions (`Resolution.getNames ()`).
pub fn resolution_names() -> Vec<String> {
    Resolution::ALL.iter().map(|r| r.name().to_string()).collect()
}

/// Arpeggiator modes (former `de.mossgrabers.framework.daw.midi.ArpeggiatorMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArpeggiatorMode {
    All,
    Up,
    UpDown,
    UpThenDown,
    Down,
    DownUp,
    DownThenUp,
    Flow,
    Random,
    ConvergeUp,
    ConvergeDown,
    DivergeUp,
    DivergeDown,
    ThumbUp,
    ThumbDown,
    PinkyUp,
    PinkyDown,
}

impl ArpeggiatorMode {
    pub const ALL: [ArpeggiatorMode; 17] = [
        ArpeggiatorMode::All,
        ArpeggiatorMode::Up,
        ArpeggiatorMode::UpDown,
        ArpeggiatorMode::UpThenDown,
        ArpeggiatorMode::Down,
        ArpeggiatorMode::DownUp,
        ArpeggiatorMode::DownThenUp,
        ArpeggiatorMode::Flow,
        ArpeggiatorMode::Random,
        ArpeggiatorMode::ConvergeUp,
        ArpeggiatorMode::ConvergeDown,
        ArpeggiatorMode::DivergeUp,
        ArpeggiatorMode::DivergeDown,
        ArpeggiatorMode::ThumbUp,
        ArpeggiatorMode::ThumbDown,
        ArpeggiatorMode::PinkyUp,
        ArpeggiatorMode::PinkyDown,
    ];

    pub const NAMES: [&'static str; 17] = [
        "All",
        "Up",
        "Up/Down",
        "Up then Down",
        "Down",
        "Down/Up",
        "Down then Up",
        "Flow",
        "Random",
        "Converge Up",
        "Converge Down",
        "Diverge Up",
        "Diverge Down",
        "Thumb Up",
        "Thumb Down",
        "Pinky UP",
        "Pinky Down",
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn by_name(name: &str) -> Option<ArpeggiatorMode> {
        Self::NAMES.iter().position(|n| *n == name).map(|i| Self::ALL[i])
    }
}

/// The arpeggiator modes supported by the Reaper note repeat (`NoteRepeatImpl.ARP_MODES`).
pub const REAPER_ARP_MODES: [ArpeggiatorMode; 4] = [ArpeggiatorMode::Down, ArpeggiatorMode::Up, ArpeggiatorMode::DownUp, ArpeggiatorMode::UpDown];

/// The name of a view as used in the startup view setting (`Views.getViewName`).
pub fn view_name(view: Views) -> Option<&'static str> {
    Some(match view {
        Views::Play => "Play",
        Views::Chords => "Chords",
        Views::Piano => "Piano",
        Views::Drum => "Drum",
        Views::DrumXox => "Drum XoX",
        Views::Drum4 => "Drum 4",
        Views::Drum8 => "Drum 8",
        Views::Drum64 => "Drum 64",
        Views::ScenePlay => "Scene Play",
        Views::Sequencer => "Sequencer",
        Views::Raindrops => "Raindrop",
        Views::PolySequencer => "Poly Seq.",
        Views::Browser => "Browser",
        Views::Mix => "Mix",
        Views::Session => "Session",
        Views::ClipLength => "Clip Length",
        _ => return None,
    })
}

/// The inverse of [`view_name`].
pub fn view_by_name(name: &str) -> Option<Views> {
    Views::ALL.iter().copied().find(|v| view_name(*v) == Some(name))
}

/// Replace umlauts and other non-ASCII characters with alternative writing
/// (`StringUtils.fixASCII`).
pub fn fix_ascii(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if (c as u32) <= 127 {
            result.push(c);
            continue;
        }
        result.push_str(match c {
            'Ä' => "Ae",
            'ä' => "ae",
            'Ö' | '\u{0152}' => "Oe",
            'ö' | '\u{0153}' => "oe",
            'Ü' => "Ue",
            'ü' => "ue",
            'ß' => "ss",
            'é' | 'ê' => "e",
            'ī' | 'ï' => "i",
            'ā' => "a",
            '→' => "->",
            '♯' => "#",
            '\u{2013}' => "-",
            '¼' => "1/4",
            '⅕' => "1/5",
            '⅙' => "1/6",
            '’' => "'",
            '±' => "+-",
            '\u{1d3e}' => "p",
            _ => "?",
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_lists() {
        let k = knob_sensitivity_options();
        assert_eq!(k.len(), 201);
        assert_eq!(k[0], "-100");
        assert_eq!(k[99], "-1");
        assert_eq!(k[100], "Normal");
        assert_eq!(k[200], "+100");
        assert_eq!(aftertouch_conversion_values().len(), 131);
        assert_eq!(aftertouch_conversion_values()[130], "CC 127");
        assert_eq!(encoder_knob_slow_options().len(), 101);
        assert_eq!(midi_channel_options()[15], "16");
    }

    #[test]
    fn lookups() {
        assert_eq!(lookup_index(&NEW_CLIP_LENGTH_VALUES, "4 Bars"), 4);
        assert_eq!(lookup_index(&NEW_CLIP_LENGTH_VALUES, "unknown"), 0);
        assert_eq!(Resolution::by_name("1/16"), Some(Resolution::Res1_16));
        assert_eq!(Resolution::get_match(0.25), 4);
        assert_eq!(ArpeggiatorMode::by_name("Pinky UP"), Some(ArpeggiatorMode::PinkyUp));
        assert_eq!(view_by_name("Poly Seq."), Some(Views::PolySequencer));
        assert_eq!(view_name(Views::Device), None);
        assert_eq!(fix_ascii("GRÖSSE ÄNDERN"), "GROeSSE AeNDERN");
    }
}
