// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Interface for views which support octave transposition (former `TransposeView`).

use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// A view which can be transposed by octaves. Views return themselves from
/// [`crate::framework::feature::View::as_transpose_view`] /
/// [`crate::framework::feature::View::as_transpose_view_mut`].
pub trait TransposeView {
    fn on_octave_down(&mut self, ctx: &mut Ctx, event: ButtonEvent);

    fn on_octave_up(&mut self, ctx: &mut Ctx, event: ButtonEvent);

    fn is_octave_up_button_on(&self, ctx: &Ctx) -> bool;

    fn is_octave_down_button_on(&self, ctx: &Ctx) -> bool;

    fn reset_octave(&mut self, ctx: &mut Ctx);
}
