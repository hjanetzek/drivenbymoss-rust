// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The helper types of the Flexi handlers (former `flexihandler.utils`): the command slots, the
//! knob modes, the received MIDI values and the program banks.

use std::collections::HashMap;

use super::command::FlexiCommand;

/// The slot is not used.
pub const TYPE_OFF: i32 = -1;
/// The slot reacts on a MIDI CC.
pub const TYPE_CC: i32 = 0;
/// The slot reacts on a note.
pub const TYPE_NOTE: i32 = 1;
/// The slot reacts on a program change.
pub const TYPE_PROGRAM_CHANGE: i32 = 2;
/// The slot reacts on pitchbend.
pub const TYPE_PITCH_BEND: i32 = 3;
/// The slot reacts on a MIDI machine control (MMC) command.
pub const TYPE_MMC: i32 = 4;

/// How the values of a control are interpreted (former `KnobMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KnobMode {
    #[default]
    Absolute,
    RelativeTwosComplement,
    RelativeSignedBit,
    RelativeSignedBit2,
    RelativeOffsetBinary,
    AbsoluteToggle,
}

impl KnobMode {
    /// All knob modes in the order of the Java enum (the ordinal is stored in the files).
    pub const ALL: [KnobMode; 6] = [
        KnobMode::Absolute,
        KnobMode::RelativeTwosComplement,
        KnobMode::RelativeSignedBit,
        KnobMode::RelativeSignedBit2,
        KnobMode::RelativeOffsetBinary,
        KnobMode::AbsoluteToggle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            KnobMode::Absolute => "Absolute (push button: Button down > 0, button up = 0)",
            KnobMode::RelativeTwosComplement => "Relative: Twos Complement (1-64 increment, 127-65 decrement)",
            KnobMode::RelativeSignedBit => "Relative: Signed Bit (65-127 increment, 1-63 decrement)",
            KnobMode::RelativeSignedBit2 => "Relative: Signed Bit 2 (1-63 increment, 65-127 decrement)",
            KnobMode::RelativeOffsetBinary => "Relative: Offset Binary (65-127 increment, 63-0 decrement)",
            KnobMode::AbsoluteToggle => "Absolute (toggle button: 1st press > 0, 2nd press = 0)",
        }
    }

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// The labels of all knob modes.
    pub fn labels() -> Vec<String> {
        Self::ALL.iter().map(|m| m.label().to_string()).collect()
    }

    /// Lookup the knob mode by its label, `Absolute` if not found.
    pub fn lookup_by_label(label: &str) -> KnobMode {
        Self::ALL.iter().copied().find(|m| m.label() == label).unwrap_or(KnobMode::Absolute)
    }

    /// The knob mode stored in a mapping file (the ordinal), `Absolute` if missing or invalid
    /// (former `GenericFlexiConfiguration.readKnobMode`).
    pub fn from_property(value: Option<&str>) -> KnobMode {
        value.and_then(|v| v.trim().parse::<usize>().ok()).and_then(|i| Self::ALL.get(i).copied()).unwrap_or(KnobMode::Absolute)
    }

    /// True for the absolute knob modes (former `AbstractHandler.isAbsolute`).
    pub fn is_absolute(self) -> bool {
        matches!(self, KnobMode::Absolute | KnobMode::AbsoluteToggle)
    }
}

/// A received MIDI value, 7 or 14 bit (former `MidiValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiValue {
    value: i32,
    is_high_res: bool,
}

impl MidiValue {
    pub fn new(value: i32, is_high_res: bool) -> Self {
        MidiValue { value, is_high_res }
    }

    pub fn is_positive(&self) -> bool {
        self.value > 0
    }

    pub fn is_high_res(&self) -> bool {
        self.is_high_res
    }

    pub fn value(&self) -> i32 {
        self.value
    }

    /// The value in the 14 bit range.
    pub fn upscaled(&self) -> i32 {
        if self.is_high_res {
            self.value
        } else {
            (self.value as f64 * 16383.0 / 127.0).round() as i32
        }
    }
}

/// A MIDI mapping of a command (former `CommandSlot`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSlot {
    pub slot_type: i32,
    pub number: i32,
    /// 0-15, 16 = all channels.
    pub midi_channel: i32,
    pub is_high_res: bool,
    pub knob_mode: KnobMode,
    /// -1 = all layers, otherwise the index of the function layer.
    pub function_layer: i32,
    pub send_value: bool,
    pub send_value_when_received: bool,
    pub command: FlexiCommand,
}

impl Default for CommandSlot {
    fn default() -> Self {
        CommandSlot {
            slot_type: TYPE_OFF,
            number: 0,
            midi_channel: 0,
            is_high_res: false,
            knob_mode: KnobMode::Absolute,
            function_layer: 0,
            send_value: false,
            send_value_when_received: false,
            command: FlexiCommand::Off,
        }
    }
}

/// An error of parsing a programs file (former `ParseException`): the message and the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (line {})", self.message, self.line)
    }
}

/// A bank of programs, read from a programs file (former `ProgramBank`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramBank {
    pub name: String,
    pub msb: i32,
    pub lsb: i32,
    pub channel: i32,
    pub programs: Vec<String>,
    program_mapper: HashMap<String, i32>,
}

impl ProgramBank {
    /// Parse the program banks: each bank starts with a line
    /// `$BANK=<name>$MSB=<0-127>$LSB=<0-127>$CHANNEL=<0-15>` followed by 1 to 128 program names.
    pub fn parse(text: &str) -> Result<Vec<ProgramBank>, ParseError> {
        let lines: Vec<&str> = text.lines().collect();
        let mut banks = Vec::new();
        let mut pos = 0;
        while pos < lines.len() {
            let Some((name, msb, lsb, channel)) = Self::match_bank_line(lines[pos]) else {
                break;
            };
            let error = |message: &str| ParseError { message: message.to_string(), line: pos };
            if name.trim().is_empty() {
                return Err(error("Missing BANK name."));
            }
            let msb = msb.parse::<i32>().ok().filter(|v| (0..=127).contains(v)).ok_or_else(|| error("MSB value must be a number in the range of 0 to 127."))?;
            let lsb = lsb.parse::<i32>().ok().filter(|v| (0..=127).contains(v)).ok_or_else(|| error("LSB value must be a number in the range of 0 to 127."))?;
            // The message is the one of the Java version
            let channel = channel.parse::<i32>().ok().filter(|v| (0..=15).contains(v)).ok_or_else(|| error("LSB value must be a number in the range of 0 to 127."))?;

            let mut bank = ProgramBank { name: name.to_string(), msb, lsb, channel, programs: Vec::new(), program_mapper: HashMap::new() };
            pos += 1;
            while pos < lines.len() && !lines[pos].starts_with("$BANK=") {
                bank.program_mapper.insert(lines[pos].to_string(), bank.programs.len() as i32);
                bank.programs.push(lines[pos].to_string());
                pos += 1;
            }
            if bank.programs.is_empty() || bank.programs.len() > 128 {
                return Err(ParseError { message: "There must be at least 1 program and not more than 128.".to_string(), line: pos });
            }
            banks.push(bank);
        }
        if banks.is_empty() {
            return Err(ParseError { message: "Could not parse any banks. Check file format.".to_string(), line: pos });
        }
        Ok(banks)
    }

    /// Match `\$BANK=(.+)\$MSB=(\d+)\$LSB=(\d+)\$CHANNEL=(\d+)` (the name is greedy, so the last
    /// `$MSB=` is used).
    fn match_bank_line(line: &str) -> Option<(&str, &str, &str, &str)> {
        let rest = line.strip_prefix("$BANK=")?;
        let channel_pos = rest.rfind("$CHANNEL=")?;
        let (rest, channel) = (&rest[..channel_pos], &rest[channel_pos + 9..]);
        let lsb_pos = rest.rfind("$LSB=")?;
        let (rest, lsb) = (&rest[..lsb_pos], &rest[lsb_pos + 5..]);
        let msb_pos = rest.rfind("$MSB=")?;
        let (name, msb) = (&rest[..msb_pos], &rest[msb_pos + 5..]);
        let is_number = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        (!name.is_empty() && is_number(msb) && is_number(lsb) && is_number(channel)).then_some((name, msb, lsb, channel))
    }

    /// The program number of the program with the given name, -1 if not found.
    pub fn lookup_program(&self, program_name: &str) -> i32 {
        self.program_mapper.get(program_name).copied().unwrap_or(-1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knob_modes() {
        assert_eq!(KnobMode::lookup_by_label(KnobMode::RelativeSignedBit.label()), KnobMode::RelativeSignedBit);
        assert_eq!(KnobMode::lookup_by_label("x"), KnobMode::Absolute);
        assert_eq!(KnobMode::from_property(Some("5")), KnobMode::AbsoluteToggle);
        assert_eq!(KnobMode::from_property(Some("6")), KnobMode::Absolute);
        assert_eq!(KnobMode::from_property(Some("x")), KnobMode::Absolute);
        assert_eq!(KnobMode::from_property(None), KnobMode::Absolute);
        assert!(KnobMode::AbsoluteToggle.is_absolute());
        assert!(!KnobMode::RelativeOffsetBinary.is_absolute());
        assert_eq!(KnobMode::labels().len(), 6);
    }

    #[test]
    fn midi_values() {
        assert_eq!(MidiValue::new(127, false).upscaled(), 16383);
        assert_eq!(MidiValue::new(64, false).upscaled(), 8256);
        assert_eq!(MidiValue::new(8192, true).upscaled(), 8192);
        assert!(!MidiValue::new(0, false).is_positive());
    }

    #[test]
    fn program_banks() {
        let text = "$BANK=Piano$MSB=0$LSB=1$CHANNEL=2\nGrand\nUpright\n$BANK=Organ$MSB=3$LSB=4$CHANNEL=2\nB3\n";
        let banks = ProgramBank::parse(text).unwrap();
        assert_eq!(banks.len(), 2);
        assert_eq!((banks[0].name.as_str(), banks[0].msb, banks[0].lsb, banks[0].channel), ("Piano", 0, 1, 2));
        assert_eq!(banks[0].lookup_program("Upright"), 1);
        assert_eq!(banks[0].lookup_program("B3"), -1);
        assert_eq!(banks[1].programs, vec!["B3".to_string()]);

        assert!(ProgramBank::parse("nothing").is_err());
        assert!(ProgramBank::parse("$BANK=A$MSB=0$LSB=0$CHANNEL=0\n").is_err());
        assert_eq!(ProgramBank::parse("$BANK=A$MSB=200$LSB=0$CHANNEL=0\nX").unwrap_err().message, "MSB value must be a number in the range of 0 to 127.");
        assert!(ProgramBank::parse("$BANK=A$MSB=0$LSB=0$CHANNEL=16\nX").is_err());
    }
}
