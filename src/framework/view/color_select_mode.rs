// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The target of a color view (former `ColorView.ColorSelectMode`): which item gets the color
//! selected on the pad grid. A color view implements
//! [`crate::framework::feature::View::set_color_select_mode`].

/// Which item is colored by the color view (former `ColorSelectMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSelectMode {
    /// The selected track (former `MODE_TRACK`).
    #[default]
    Track,
    /// The selected layer (former `MODE_LAYER`).
    Layer,
    /// The selected clip (former `MODE_CLIP`).
    Clip,
}
