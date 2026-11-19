// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The icons of the graphics displays (former `ResourceHandler` with the SVG images and
//! `DeviceTypes`). The SVGs are pre-rendered into alpha masks in [`super::icon_data`] (see
//! `tools/gen_icons.py`), so there is no SVG renderer at runtime.

use super::icon_data;
use crate::daw::constants::ChannelType;

/// An icon as an 8 bit alpha mask (former `IImage` of a SVG loaded at scale 1).
#[derive(Debug)]
pub struct IconData {
    /// The Java resource name, e.g. "track/audio_track.svg".
    pub name: &'static str,
    pub width: i32,
    pub height: i32,
    /// The alpha values, row by row.
    pub alpha: &'static [u8],
}

impl PartialEq for IconData {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// A reference to an embedded icon; compared by name.
pub type Icon = &'static IconData;

/// Get an icon by its Java resource name (former `ResourceHandler.getSVGImage`).
pub fn get_svg_image(name: &str) -> Option<Icon> {
    icon_data::ALL.iter().copied().find(|icon| icon.name == name)
}

/// The icon of a channel type, or the pin if the channel is pinned (former
/// `ChannelSelectComponent.getIcon`).
pub fn channel_icon(channel_type: Option<ChannelType>, is_pinned: bool) -> Option<Icon> {
    if is_pinned {
        return Some(&icon_data::PIN);
    }
    Some(match channel_type? {
        ChannelType::Audio => &icon_data::AUDIO_TRACK,
        ChannelType::Instrument => &icon_data::INSTRUMENT_TRACK,
        ChannelType::Group => &icon_data::GROUP_TRACK,
        ChannelType::GroupOpen => &icon_data::GROUP_TRACK_OPEN,
        ChannelType::Effect => &icon_data::RETURN_TRACK,
        ChannelType::Hybrid => &icon_data::HYBRID_TRACK,
        ChannelType::Master => &icon_data::MASTER_TRACK,
        ChannelType::Layer => &icon_data::MULTI_LAYER,
        ChannelType::Unknown | ChannelType::Cue => return None,
    })
}

/// The icon of a (Bitwig) device by its name; the plugin icon for unknown devices (former
/// `DeviceTypes.getIconId`).
pub fn device_icon(device_name: &str) -> Icon {
    match device_name {
        "Oscilloscope" | "Spectrum" => &icon_data::DEVICE_ANALYSIS,
        "Amp" | "Audio Receiver" | "Bit-8" | "Blur" | "Chorus" | "Chorus+" | "Comb" | "Compressor" | "DC Offset" | "De-Esser" | "Delay-1" | "Delay-2" | "Delay-4" | "Distortion" | "Dual Pan" | "Dynamics" | "EQ+" | "EQ-2" | "EQ-5" | "EQ-DJ" | "Filter" | "Flanger" | "Flanger+" | "Freq Shifter" | "FX Grid" | "Gate" | "Ladder" | "Note Pitch Shifter" | "Peak Limiter" | "Phaser" | "Phaser+" | "Pitch Shifter" | "Resonator Bank" | "Reverb" | "Ring-Mod" | "Rotary" | "Test Tone" | "Time Shift" | "Tool" | "Transient Control" | "Treemonster" | "Tremolo" | "Vocoder" => &icon_data::DEVICE_AUDIO,
        "Audio MOD" | "Chain" | "FX Layer" | "FX Selector" | "Instrument Layer" | "Instrument Selector" | "LFO MOD" | "Mid-Side Split" | "Multiband FX-2" | "Multiband FX-3" | "Note FX Layer" | "Note FX Selector" | "Note MOD" | "Replacer" | "Step MOD" | "Stereo Split" | "XY Effect" | "XY Instrument" => &icon_data::DEVICE_CONTAINER,
        "Drum Machine" => &icon_data::DEVICE_DRUM_MACHINE,
        "E-Clap" | "E-Cowbell" | "E-Hat" | "E-Kick" | "E-Snare" | "E-Tom" => &icon_data::DEVICE_DRUM_MODULE,
        "FM-4" | "Organ" | "Phase-4" | "Poly Grid" | "Polysynth" | "Sampler" => &icon_data::DEVICE_INSTRUMENT,
        "HW Clock Out" | "HW CV Instrument" | "HW CV Out" | "HW FX" | "HW Instrument" => &icon_data::DEVICE_IO,
        "Arpeggiator" | "Channel Filter" | "Channel Map" | "Diatonic Transposer" | "MIDI CC" | "MIDI Program Change" | "MIDI Song Select" | "Multi-Note" | "Note Echo" | "Note Filter" | "Note Harmonizer" | "Note Latch" | "Note Length" | "Note Receiver" | "Note Velocity" | "Transposition Map" => &icon_data::DEVICE_NOTE,
        "PROJECT" => &icon_data::MASTER_TRACK,
        "USER" => &icon_data::USER,
        _ => &icon_data::DEVICE_PLUGIN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup() {
        assert_eq!(get_svg_image("channel/solo.svg").map(|i| (i.width, i.height)), Some((15, 13)));
        assert!(get_svg_image("unknown.svg").is_none());
        for icon in icon_data::ALL {
            assert_eq!(icon.alpha.len(), (icon.width * icon.height) as usize, "{}", icon.name);
        }
        assert_eq!(channel_icon(Some(ChannelType::Audio), false).unwrap().name, "track/audio_track.svg");
        assert_eq!(channel_icon(Some(ChannelType::Audio), true).unwrap().name, "pin.svg");
        assert!(channel_icon(None, false).is_none());
        assert_eq!(device_icon("Polysynth").name, "device/device_instrument.svg");
        assert_eq!(device_icon("PROJECT").name, "track/master_track.svg");
        assert_eq!(device_icon("Some VST").name, "device/device_plugin.svg");
    }
}
