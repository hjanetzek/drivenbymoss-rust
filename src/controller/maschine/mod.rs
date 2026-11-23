// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Native Instruments Maschine controllers (former packages
//! `controller.ni.maschine.*`).
//!
//! Not in the Java version: the setting "LED Brightness" of all models (Normal as Java, Bright),
//! which raises the palette levels and single-colour values where the LEDs are sent, see
//! [`core::brightness`].

pub mod core;
pub mod mk;
pub mod jam;

use crate::controller::ControllerFactory;

/// The factories of all supported Maschine models.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    let mut factories: Vec<Box<dyn ControllerFactory>> = Vec::new();
    factories.push(Box::new(jam::MaschineJamFactory));
    factories.extend(mk::factories());
    factories
}
