// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Color IDs used by the generic views. In the Java version these constants are defined in the
//! view classes; they are collected here so that color managers of controllers do not depend on
//! the view implementations.

/// From `AbstractSequencerView`.
pub mod abstract_sequencer_view {
    pub const COLOR_STEP_HILITE_NO_CONTENT: &str = "COLOR_STEP_HILITE_NO_CONTENT";
    pub const COLOR_STEP_HILITE_CONTENT: &str = "COLOR_STEP_HILITE_CONTENT";
    pub const COLOR_STEP_MUTED: &str = "COLOR_STEP_MUTED";
    pub const COLOR_STEP_MUTED_CONT: &str = "COLOR_STEP_MUTED_CONT";
    pub const COLOR_STEP_SELECTED: &str = "COLOR_STEP_SELECTED";
    pub const COLOR_NO_CONTENT: &str = "COLOR_NO_CONTENT";
    pub const COLOR_NO_CONTENT_4: &str = "COLOR_NO_CONTENT_4";
    pub const COLOR_CONTENT: &str = "COLOR_CONTENT";
    pub const COLOR_CONTENT_CONT: &str = "COLOR_CONTENT_CONT";
    pub const COLOR_PAGE: &str = "COLOR_PAGE";
    pub const COLOR_ACTIVE_PAGE: &str = "COLOR_ACTIVE_PAGE";
    pub const COLOR_SELECTED_PAGE: &str = "COLOR_SELECTED_PAGE";
    pub const COLOR_RESOLUTION_OFF: &str = "COLOR_RESOLUTION_OFF";
    pub const COLOR_RESOLUTION: &str = "COLOR_RESOLUTION";
    pub const COLOR_RESOLUTION_SELECTED: &str = "COLOR_RESOLUTION_SELECTED";
    pub const COLOR_TRANSPOSE: &str = "COLOR_TRANSPOSE";
    pub const COLOR_TRANSPOSE_SELECTED: &str = "COLOR_TRANSPOSE_SELECTED";
}

/// From `AbstractDrumView`.
pub mod abstract_drum_view {
    pub const COLOR_PAD_RECORD: &str = "COLOR_PAD_RECORD";
    pub const COLOR_PAD_PLAY: &str = "COLOR_PAD_PLAY";
    pub const COLOR_PAD_SELECTED: &str = "COLOR_PAD_SELECTED";
    pub const COLOR_PAD_MUTED: &str = "COLOR_PAD_MUTED";
    pub const COLOR_PAD_HAS_CONTENT: &str = "COLOR_PAD_HAS_CONTENT";
    pub const COLOR_PAD_NO_CONTENT: &str = "COLOR_PAD_NO_CONTENT";
    pub const COLOR_PAD_OFF: &str = "COLOR_PAD_OFF";
}

/// From `AbstractDrumExView`.
pub mod abstract_drum_ex_view {
    pub const COLOR_EX_SELECT_ON: &str = "COLOR_EX_SELECT_ON";
    pub const COLOR_EX_SELECT_OFF: &str = "COLOR_EX_SELECT_OFF";
    pub const COLOR_EX_MUTE_ON: &str = "COLOR_EX_MUTE_ON";
    pub const COLOR_EX_MUTE_OFF: &str = "COLOR_EX_MUTE_OFF";
    pub const COLOR_EX_SOLO_ON: &str = "COLOR_EX_SOLO_ON";
    pub const COLOR_EX_SOLO_OFF: &str = "COLOR_EX_SOLO_OFF";
    pub const COLOR_EX_BROWSE_ON: &str = "COLOR_EX_BROWSE_ON";
    pub const COLOR_EX_BROWSE_OFF: &str = "COLOR_EX_BROWSE_OFF";
    pub const COLOR_EX_NOTE_REPEAT_ON: &str = "COLOR_EX_NOTE_REPEAT_ON";
    pub const COLOR_EX_NOTE_REPEAT_OFF: &str = "COLOR_EX_NOTE_REPEAT_OFF";
    pub const COLOR_EX_NOTE_REPEAT_PERIOD_ON: &str = "COLOR_EX_NOTE_REPEAT_PERIOD_ON";
    pub const COLOR_EX_NOTE_REPEAT_PERIOD_OFF: &str = "COLOR_EX_NOTE_REPEAT_PERIOD_OFF";
    pub const COLOR_EX_NOTE_REPEAT_LENGTH_ON: &str = "COLOR_EX_NOTE_REPEAT_LENGTH_ON";
    pub const COLOR_EX_NOTE_REPEAT_LENGTH_OFF: &str = "COLOR_EX_NOTE_REPEAT_LENGTH_OFF";
    pub const COLOR_EX_TOGGLE_ON: &str = "COLOR_EX_TOGGLE_ON";
    pub const COLOR_EX_TOGGLE_OFF: &str = "COLOR_EX_TOGGLE_OFF";
}

/// From `AbstractPlayView`.
pub mod abstract_play_view {
    pub const COLOR_PLAY: &str = "PLAY_VIEW_COLOR_PLAY";
    pub const COLOR_RECORD: &str = "PLAY_VIEW_COLOR_RECORD";
    pub const COLOR_OFF: &str = "PLAY_VIEW_COLOR_OFF";
}

/// From `ClipLengthView`.
pub mod clip_length_view {
    pub const COLOR_OUTSIDE: &str = "COLOR_OUTSIDE";
    pub const COLOR_PART: &str = "COLOR_PART";
}

/// From `ScenePlayView`.
pub mod scene_play_view {
    pub const COLOR_SELECTED_PLAY_SCENE: &str = "COLOR_SELECTED_PLAY_SCENE";
}

/// From `BrowserView`.
pub mod browser_view {
    pub const OFF: &str = "BROWSER_OFF";
    pub const DISCARD: &str = "BROWSER_DISCARD";
    pub const CONFIRM: &str = "BROWSER_CONFIRM";
    pub const PLAY: &str = "BROWSER_PLAY";
    pub const COLUMN1: &str = "BROWSER_COLUMN1";
    pub const COLUMN2: &str = "BROWSER_COLUMN2";
    pub const COLUMN3: &str = "BROWSER_COLUMN3";
    pub const COLUMN4: &str = "BROWSER_COLUMN4";
    pub const COLUMN5: &str = "BROWSER_COLUMN5";
    pub const COLUMN6: &str = "BROWSER_COLUMN6";
    pub const COLUMN7: &str = "BROWSER_COLUMN7";
    pub const COLUMN8: &str = "BROWSER_COLUMN8";
}

/// From `AbstractSessionView`.
pub mod abstract_session_view {
    pub const COLOR_SCENE: &str = "COLOR_SCENE";
    pub const COLOR_SELECTED_SCENE: &str = "COLOR_SELECTED_SCENE";
    pub const COLOR_SCENE_OFF: &str = "COLOR_SELECTED_OFF";
}

