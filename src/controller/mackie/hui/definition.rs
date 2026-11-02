// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definition of the Mackie HUI protocol (former `HUIControllerDefinition` and the
//! constants of `HUIControlSurface`). The HUI buttons are addressed by a zone (8 buttons each)
//! and a port; the constants are `zone * 8 + port`.

use crate::controller::ControllerDefinitionInfo;

/// The UUIDs of the models with 0-3 extenders (there are Reaper instances for 0-2).
pub const EXTENSION_ID: [&str; 4] =
    ["AD354AAA-E499-4B89-B86C-3E1F74647FF5", "A9517933-B7AD-408A-B987-39560B0057CD", "415E6966-B016-4224-A667-47248F945CE2", "53566C82-7C54-4A5A-A574-12EDEA44CF33"];

/// The names of the models with 0-3 extenders.
pub const HARDWARE_MODEL: [&str; 4] =
    ["HUI - Human User Interface", "HUI - Human User Interface + 1 Extender", "HUI - Human User Interface + 2 Extenders", "HUI - Human User Interface + 3 Extenders"];

/// General information about the model with the given number of extenders.
pub fn info(num_extenders: usize) -> ControllerDefinitionInfo {
    ControllerDefinitionInfo {
        uuid: EXTENSION_ID[num_extenders].to_string(),
        vendor: "Mackie".to_string(),
        hardware_model: HARDWARE_MODEL[num_extenders].to_string(),
        num_midi_in_ports: num_extenders + 1,
        num_midi_out_ports: num_extenders + 1,
    }
}

// The channel strips: zone = channel, the port of the control
pub const HUI_FADER1: i32 = 0;
pub const HUI_SELECT1: i32 = 1;
pub const HUI_MUTE1: i32 = 2;
pub const HUI_SOLO1: i32 = 3;
pub const HUI_AUTO1: i32 = 4;
pub const HUI_VSELECT1: i32 = 5;
pub const HUI_INSERT1: i32 = 6;
pub const HUI_ARM1: i32 = 7;

pub const HUI_KEY_CTRL_CLT: i32 = 64;
pub const HUI_KEY_SHIFT_AD: i32 = 65;
pub const HUI_KEY_UNDO: i32 = 67;
pub const HUI_KEY_ALT_FINE: i32 = 68;
pub const HUI_KEY_OPTION_A: i32 = 69;
pub const HUI_KEY_SAVE: i32 = 71;

pub const HUI_WINDOW_MIX: i32 = 72;
pub const HUI_WINDOW_EDIT: i32 = 73;
pub const HUI_WINDOW_TRANSPRT: i32 = 74;
pub const HUI_WINDOW_STATUS: i32 = 76;

pub const HUI_CHANL_LEFT: i32 = 80;
pub const HUI_BANK_LEFT: i32 = 81;
pub const HUI_CHANL_RIGHT: i32 = 82;
pub const HUI_BANK_RIGHT: i32 = 83;

pub const HUI_ASSIGN1_OUTPUT: i32 = 88;
pub const HUI_ASSIGN1_INPUT: i32 = 89;
pub const HUI_ASSIGN1_PAN: i32 = 90;
pub const HUI_ASSIGN1_SEND_E: i32 = 91;
pub const HUI_ASSIGN1_SEND_D: i32 = 92;
pub const HUI_ASSIGN1_SEND_C: i32 = 93;
pub const HUI_ASSIGN1_SEND_B: i32 = 94;
pub const HUI_ASSIGN1_SEND_A: i32 = 95;

pub const HUI_ASSIGN2_BYPASS: i32 = 101;
pub const HUI_ASSIGN2_RECRDYAL: i32 = 102;

pub const HUI_CURSOR_DOWN: i32 = 104;
pub const HUI_CURSOR_LEFT: i32 = 105;
pub const HUI_CURSOR_MODE: i32 = 106;
pub const HUI_CURSOR_RIGHT: i32 = 107;
pub const HUI_CURSOR_UP: i32 = 108;

pub const HUI_TRANSPORT_REWIND: i32 = 113;
pub const HUI_TRANSPORT_FAST_FWD: i32 = 114;
pub const HUI_TRANSPORT_STOP: i32 = 115;
pub const HUI_TRANSPORT_PLAY: i32 = 116;
pub const HUI_TRANSPORT_RECORD: i32 = 117;
pub const HUI_TRANSPORT_RETURN_TO_ZERO: i32 = 120;
pub const HUI_TRANSPORT_ON_LINE: i32 = 122;
pub const HUI_TRANSPORT_LOOP: i32 = 123;
pub const HUI_TRANSPORT_QICK_PUNCH: i32 = 124;
pub const HUI_TRANSPORT_PRE: i32 = 129;
pub const HUI_TRANSPORT_IN: i32 = 130;
pub const HUI_TRANSPORT_OUT: i32 = 131;
pub const HUI_TRANSPORT_POST: i32 = 132;

pub const HUI_AUTO_MODE_TRIM: i32 = 192;
pub const HUI_AUTO_MODE_LATCH: i32 = 193;
pub const HUI_AUTO_MODE_READ: i32 = 194;
pub const HUI_AUTO_MODE_OFF: i32 = 195;
pub const HUI_AUTO_MODE_WRITE: i32 = 196;
pub const HUI_AUTO_MODE_TOUCH: i32 = 197;

pub const HUI_EDIT_PASTE: i32 = 208;
pub const HUI_EDIT_COPY: i32 = 212;

pub const HUI_F1: i32 = 216;
pub const HUI_F8_ESC: i32 = 223;

pub const HUI_FS_RLAY1: i32 = 232;
pub const HUI_FS_RLAY2: i32 = 233;

pub const KNOB_LED_MODE_OFF: i32 = -1;
pub const KNOB_LED_MODE_SINGLE_DOT: i32 = 0;
pub const KNOB_LED_MODE_BOOST_CUT: i32 = 1;
pub const KNOB_LED_MODE_WRAP: i32 = 2;
pub const KNOB_LED_MODE_SPREAD: i32 = 3;

/// State for button LED on.
pub const HUI_BUTTON_STATE_ON: i32 = 127;
/// State for button LED off.
pub const HUI_BUTTON_STATE_OFF: i32 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models() {
        assert_eq!(info(0).to_string(), "Mackie HUI - Human User Interface");
        assert_eq!(info(2).num_midi_out_ports, 3);
        assert_eq!(info(1).uuid, "A9517933-B7AD-408A-B987-39560B0057CD");
        // Zone and port of the controls
        assert_eq!((HUI_TRANSPORT_PLAY / 8, HUI_TRANSPORT_PLAY % 8), (14, 4));
    }
}
