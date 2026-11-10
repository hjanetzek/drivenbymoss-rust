// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Support for the Arturia controllers (former `controller.arturia`): Beatstep and Beatstep
//! Pro.

pub mod beatstep;

use crate::controller::ControllerFactory;

/// The factories of all Arturia controllers.
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    beatstep::factories()
}
