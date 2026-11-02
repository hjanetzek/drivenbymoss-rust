// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The settings of the controllers (former `framework.configuration` and
//! `reaper.framework.configuration` packages) without any UI.

pub mod configuration;
pub mod hui;
pub mod launchcontrol;
pub mod launchpad;
pub mod maschine;
pub mod maschine_jam;
pub mod mcu;
pub mod properties;
pub mod reaper;
pub mod setting;
pub mod values;

pub use configuration::{Configuration, NoteRepeatCapabilities};
pub use launchpad::{LaunchpadConfigParams, LaunchpadConfiguration};
pub use properties::Properties;
pub use setting::{Scope, SettingDescriptor, SettingId, SettingKind};
pub use values::{ArpeggiatorMode, RecordFunction, Resolution, TransportBehavior};
