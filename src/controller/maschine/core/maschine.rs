// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The supported Maschine models (former `Maschine`).

/// The supported Maschine models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Maschine {
    /// Maschine JAM.
    Jam,
    /// Maschine Mikro Mk3.
    MikroMk3,
    /// Maschine Mk2.
    Mk2,
    /// Maschine Mk3.
    Mk3,
    /// Maschine+.
    Plus,
    /// Maschine Studio.
    Studio,
}

struct MaschineInfo {
    name: &'static str,
    maschine_id: &'static str,
    has_shift: bool,
    has_mcu_display: bool,
    has_group_buttons: bool,
    has_cursor_keys: bool,
    footswitches: usize,
}

const MESSAGE_SHIFT_DOWN: (&str, &str) = ("F0002109", "4D5000014D01F7");
const MESSAGE_SHIFT_UP: (&str, &str) = ("F0002109", "4D5000014D00F7");
const MESSAGE_RETURN_FROM_HOST: (&str, &str) = ("F0002109", "4D5000014601F7");

impl Maschine {
    pub const ALL: [Maschine; 6] = [Maschine::Jam, Maschine::MikroMk3, Maschine::Mk2, Maschine::Mk3, Maschine::Plus, Maschine::Studio];

    fn info(self) -> MaschineInfo {
        let (name, maschine_id, has_shift, has_mcu_display, has_group_buttons, has_cursor_keys, footswitches) = match self {
            Maschine::Jam => ("Maschine JAM", "1500", true, false, true, true, 2),
            Maschine::MikroMk3 => ("Maschine Mikro Mk3", "1700", false, false, false, false, 0),
            Maschine::Mk2 => ("Maschine Mk2", "0000", false, true, true, true, 0),
            Maschine::Mk3 => ("Maschine Mk3", "1600", true, true, true, true, 2),
            Maschine::Plus => ("Maschine+", "1820", true, true, true, true, 2),
            Maschine::Studio => ("Maschine Studio", "1300", false, true, true, true, 4),
        };
        MaschineInfo { name, maschine_id, has_shift, has_mcu_display, has_group_buttons, has_cursor_keys, footswitches }
    }

    /// Get the name of the specific Maschine.
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// Can the Shift button be used? Otherwise emulated with Stop button.
    pub fn has_shift(self) -> bool {
        self.info().has_shift
    }

    /// Does it have a display that supports the MCU protocol?
    pub fn has_mcu_display(self) -> bool {
        self.info().has_mcu_display
    }

    /// Does it have group buttons?
    pub fn has_group_buttons(self) -> bool {
        self.info().has_group_buttons
    }

    /// Does the device have cursor keys?
    pub fn has_cursor_keys(self) -> bool {
        self.info().has_cursor_keys
    }

    /// Get the number of foot switches on the Maschine.
    pub fn footswitches(self) -> usize {
        self.info().footswitches
    }

    fn message(self, message: (&str, &str)) -> String {
        format!("{}{}{}", message.0, self.info().maschine_id, message.1)
    }

    /// Get the system exclusive message which is sent from the device on pressing the Shift
    /// button.
    pub fn message_shift_down(self) -> String {
        self.message(MESSAGE_SHIFT_DOWN)
    }

    /// Get the system exclusive message which is sent from the device on releasing the Shift
    /// button.
    pub fn message_shift_up(self) -> String {
        self.message(MESSAGE_SHIFT_UP)
    }

    /// Get the system exclusive message which is sent from the device when the MIDI mode is
    /// entered.
    pub fn message_return_from_host(self) -> String {
        self.message(MESSAGE_RETURN_FROM_HOST)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages() {
        assert_eq!(Maschine::Jam.message_shift_down(), "F000210915004D5000014D01F7");
        assert_eq!(Maschine::Mk2.message_shift_up(), "F000210900004D5000014D00F7");
        assert_eq!(Maschine::Studio.message_return_from_host(), "F000210913004D5000014601F7");
        assert!(!Maschine::Mk2.has_shift());
        assert!(Maschine::Mk2.has_mcu_display());
        assert_eq!(Maschine::Studio.footswitches(), 4);
    }
}
