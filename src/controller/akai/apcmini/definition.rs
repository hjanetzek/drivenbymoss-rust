// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The definitions of the Akai APC mini models (former `AbstractAPCminiDefinition`,
//! `APCminiMk1ControllerDefinition`, `APCminiMk2ControllerDefinition` and `APCminiButton`).

use crate::controller::{ControllerDefinitionInfo, OperatingSystem};

/// The APC mini models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApcMiniModel {
    Mk1,
    Mk2,
}

impl ApcMiniModel {
    pub const ALL: [ApcMiniModel; 2] = [ApcMiniModel::Mk1, ApcMiniModel::Mk2];
}

/// The buttons of the APC mini which have different MIDI notes on the models (former
/// `APCminiButton`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApcMiniButton {
    Shift,
    Track(usize),
    Scene(usize),
}

/// The definition of an APC mini model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApcMiniDefinition {
    pub model: ApcMiniModel,
}

impl ApcMiniDefinition {
    pub fn new(model: ApcMiniModel) -> Self {
        ApcMiniDefinition { model }
    }

    /// The UUID of the extension.
    pub fn uuid(&self) -> &'static str {
        match self.model {
            ApcMiniModel::Mk1 => "E7E02A80-3657-11E4-8C21-0800200C9A66",
            ApcMiniModel::Mk2 => "3B953D86-4A53-43B1-BB2C-65A33DC778F1",
        }
    }

    pub fn hardware_model(&self) -> &'static str {
        match self.model {
            ApcMiniModel::Mk1 => "APCmini Mk1",
            ApcMiniModel::Mk2 => "APCmini Mk2",
        }
    }

    /// General information about the controller.
    pub fn info(&self) -> ControllerDefinitionInfo {
        ControllerDefinitionInfo {
            uuid: self.uuid().to_string(),
            vendor: "Akai".to_string(),
            hardware_model: self.hardware_model().to_string(),
            num_midi_in_ports: 1,
            num_midi_out_ports: 1,
        }
    }

    /// The port names for the automatic detection of the device.
    pub fn midi_discovery_pairs(&self, _os: OperatingSystem) -> Vec<(Vec<String>, Vec<String>)> {
        super::super::device_discovery_pairs(match self.model {
            ApcMiniModel::Mk1 => "APC MINI",
            ApcMiniModel::Mk2 => "APC mini mk2",
        })
    }

    /// The MIDI note of a button (former `getButtonID`).
    pub fn button_id(&self, button: ApcMiniButton) -> i32 {
        match (self.model, button) {
            (ApcMiniModel::Mk1, ApcMiniButton::Track(i)) => 64 + i as i32,
            (ApcMiniModel::Mk1, ApcMiniButton::Scene(i)) => 82 + i as i32,
            (ApcMiniModel::Mk1, ApcMiniButton::Shift) => 98,
            (ApcMiniModel::Mk2, ApcMiniButton::Track(i)) => 0x64 + i as i32,
            (ApcMiniModel::Mk2, ApcMiniButton::Scene(i)) => 0x70 + i as i32,
            (ApcMiniModel::Mk2, ApcMiniButton::Shift) => 0x7A,
        }
    }

    /// Swap the track button indices for the shifted functions (former
    /// `swapShiftedTrackIndices`). The Mk2 switches the cursor buttons and the fader mode buttons.
    pub fn swap_shifted_track_indices(&self, track_index: usize) -> usize {
        match self.model {
            ApcMiniModel::Mk1 => track_index,
            ApcMiniModel::Mk2 => {
                if track_index < 4 {
                    track_index + 4
                } else {
                    track_index - 4
                }
            }
        }
    }

    /// Swap the scene button indices for the shifted functions (former
    /// `swapShiftedSceneIndices`). The Mk2 switches mute and record arm.
    pub fn swap_shifted_scene_indices(&self, scene_index: usize) -> usize {
        match (self.model, scene_index) {
            (ApcMiniModel::Mk2, 2) => 3,
            (ApcMiniModel::Mk2, 3) => 2,
            _ => scene_index,
        }
    }

    /// True if the pad brightness can be set (former `hasBrightness`).
    pub fn has_brightness(&self) -> bool {
        self.model == ApcMiniModel::Mk2
    }

    /// True if the pads have RGB colors (former `hasRGBColors`).
    pub fn has_rgb_colors(&self) -> bool {
        self.model == ApcMiniModel::Mk2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_ids() {
        let mk1 = ApcMiniDefinition::new(ApcMiniModel::Mk1);
        let mk2 = ApcMiniDefinition::new(ApcMiniModel::Mk2);
        assert_eq!(mk1.button_id(ApcMiniButton::Track(0)), 64);
        assert_eq!(mk1.button_id(ApcMiniButton::Track(7)), 71);
        assert_eq!(mk1.button_id(ApcMiniButton::Scene(7)), 89);
        assert_eq!(mk1.button_id(ApcMiniButton::Shift), 98);
        assert_eq!(mk2.button_id(ApcMiniButton::Track(7)), 0x6B);
        assert_eq!(mk2.button_id(ApcMiniButton::Scene(0)), 0x70);
        assert_eq!(mk2.button_id(ApcMiniButton::Shift), 0x7A);
    }

    #[test]
    fn swapped_indices() {
        let mk1 = ApcMiniDefinition::new(ApcMiniModel::Mk1);
        let mk2 = ApcMiniDefinition::new(ApcMiniModel::Mk2);
        assert_eq!((0..8).map(|i| mk1.swap_shifted_track_indices(i)).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!((0..8).map(|i| mk2.swap_shifted_track_indices(i)).collect::<Vec<_>>(), vec![4, 5, 6, 7, 0, 1, 2, 3]);
        assert_eq!((0..8).map(|i| mk2.swap_shifted_scene_indices(i)).collect::<Vec<_>>(), vec![0, 1, 3, 2, 4, 5, 6, 7]);
        assert_eq!(mk1.swap_shifted_scene_indices(2), 2);
    }

    #[test]
    fn infos() {
        let mk2 = ApcMiniDefinition::new(ApcMiniModel::Mk2);
        assert_eq!(mk2.info().to_string(), "Akai APCmini Mk2");
        assert_eq!(mk2.midi_discovery_pairs(OperatingSystem::Linux)[0].0[0], "APC mini mk2");
        assert!(mk2.has_brightness() && !ApcMiniDefinition::new(ApcMiniModel::Mk1).has_rgb_colors());
    }
}
