// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Maschine JAM as a surface of the surface link (doc/SURFACE_LINK.md; not part of Java):
//! the pages of a provider (e.g. the Electra One) on the grid and the touch strips.
//!
//! - **Entry and exit:** Select + SONG switches the surface on and off. Any other view or mode
//!   button (SONG, STEP, PAD MODE, MACRO, LEVEL, AUX, CONTROL) leaves it for its view or mode;
//!   a detach of the provider leaves it too. The JAM's own modes and views come back.
//! - **Grid:** the centre 6x6 are the slots (in their colors; a press selects a knob slot or
//!   presses a pad slot; the selected slot blinks white). Column 1, rows 2-7: the row
//!   selectors (the active row white, the control sets in two colors). Column 8: the page list
//!   (up to 8 pages). The top and the bottom pad of column 1: the previous and next page.
//! - **Strips 1-6:** the slots of the active row; their LEDs show the values. A swipe turns by
//!   the distance (relative, no jumps); with Shift in fine steps.
//! - **Strip 7:** the selected slot, in fine steps; **strip 8:** the first extra of the provider.
//! - **Main encoder:** the selected slot (`selected.turn`), with Shift fine; its push resets it.
//! - **PAGE LEFT/RIGHT:** the previous / next page, **up / down:** the previous / next row.
//! - Without a provider the grid shows a red cross.

use std::rc::Rc;

use super::surface::{self as jam, FaderConfig};
use crate::framework::command::{shared, ContinuousCommand, SharedTrigger, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::controller::maschine::core::colors;
use crate::framework::surface_link::client::{EncoderFunction, EncoderKind, PadFunction, SharedClient, SurfaceLinkClient, SurfaceMapping, UiColors};
use crate::framework::surface_link::view::{palette_index, state_text, SurfaceLinkMode, SurfaceLinkView};
use crate::framework::surface_link::{ColorDepth, LinkState, DEFAULT_LINK};
use crate::framework::value_changer::RelativeEncoding;
use crate::midi::ButtonEvent;

/// The state of the surface in the JAM's state.
pub struct JamSurface {
    pub client: SharedClient,
    /// True while the JAM is the surface.
    pub active: bool,
    /// The view and the mode before the surface was switched on.
    previous: Option<(Option<Views>, Option<Modes>)>,
}

/// The mapping of the JAM: the grid, its 8 touch strips and the encoder.
pub fn mapping() -> SurfaceMapping {
    let mut encoders: Vec<EncoderFunction> = (0..6).map(EncoderFunction::RowSlot).collect();
    encoders.push(EncoderFunction::Selected { fine: true });
    encoders.push(EncoderFunction::Extra(0));
    let mut mapping = SurfaceMapping::new(8, 8)
        .slots_at(1, 1)
        .row_selectors_at(0, 1)
        .page_list_at(7, 0, 8)
        .pad(0, 0, PadFunction::PagePrev)
        .pad(0, 7, PadFunction::PageNext)
        .encoders(EncoderKind::Absolute, encoders)
        .main_encoder(RelativeEncoding::TwosComplement);
    mapping.touch_encoders = true;
    mapping.color_depth = ColorDepth::Palette;
    mapping.ui_colors = UiColors {
        row_active: Some(colors::COLOR_WHITE),
        row_set_a: Some(colors::COLOR_BLUE_LO),
        row_set_b: Some(colors::COLOR_TURQUOISE_LO),
        page_active: Some(colors::COLOR_ORANGE),
        page: Some(colors::COLOR_AMBER_LO),
        navigation: Some(colors::COLOR_GREY),
        selected: Some(colors::COLOR_WHITE),
        no_provider: Some(colors::COLOR_RED_LO),
        incompatible: Some(colors::COLOR_ORANGE),
        black_slot: Some(colors::COLOR_DARK_GREY),
    };
    mapping
}

impl JamSurface {
    pub fn new() -> Self {
        JamSurface { client: SurfaceLinkClient::shared(DEFAULT_LINK, mapping()), active: false, previous: None }
    }
}

impl Default for JamSurface {
    fn default() -> Self {
        Self::new()
    }
}

fn surface(ctx: &Ctx) -> &JamSurface {
    &jam::ext(ctx).surface
}

fn surface_mut(ctx: &mut Ctx) -> &mut JamSurface {
    &mut jam::ext_mut(ctx).surface
}

/// True while the JAM is the surface.
pub fn is_active(ctx: &Ctx) -> bool {
    surface(ctx).active
}

/// Register the view and the mode (in `init`).
pub fn register(ctx: &mut Ctx) {
    let client = surface(ctx).client.clone();
    ctx.register_view(Views::SurfaceLink, SurfaceLinkView::new(client.clone()));
    ctx.register_mode(Modes::SurfaceLink, SurfaceLinkMode::new(client));
    // The touches of the strips (Java binds the no-op command): only for the surface
    for index in 0..8 {
        let id = ContinuousId::get(ContinuousId::Fader1, index);
        let command: SharedTrigger = shared(StripTouchCommand { index });
        if let Some(control) = ctx.surface.continuous_mut(id) {
            control.touch_command = Some(command);
        }
    }
}

fn notify_state(ctx: &mut Ctx) {
    let text = {
        let client = surface(ctx).client.borrow();
        state_text(client.state(), client.provider_name(), client.model().page_name())
    };
    ctx.notify(&text);
}

/// Switch the surface on.
pub fn enter(ctx: &mut Ctx) {
    if is_active(ctx) {
        return;
    }
    let previous = (ctx.views.active_id_ignore_temporary(), ctx.modes.active_id());
    let state = surface_mut(ctx);
    state.previous = Some(previous);
    state.active = true;
    ctx.set_active_view(Views::SurfaceLink);
    ctx.set_active_mode(Modes::SurfaceLink);
    notify_state(ctx);
}

/// Switch the surface off: back to the view and the mode before (or keep the one which was
/// selected instead).
pub fn leave(ctx: &mut Ctx) {
    if !is_active(ctx) {
        return;
    }
    let state = surface_mut(ctx);
    state.active = false;
    let (view, mode) = state.previous.take().unwrap_or((None, None));
    if ctx.views.active_id_ignore_temporary() == Some(Views::SurfaceLink) {
        ctx.set_active_view(view.unwrap_or(Views::Play));
    }
    if ctx.modes.active_id_ignore_temporary() == Some(Modes::SurfaceLink) {
        ctx.set_active_mode(mode.unwrap_or(Modes::Volume));
    }
}

/// Select + SONG.
pub fn toggle(ctx: &mut Ctx) {
    if is_active(ctx) {
        leave(ctx);
        ctx.notify("Surface off");
    } else {
        enter(ctx);
    }
}

/// The tick (in `handle_changes`): the provider's changes, the state, and whether another
/// button took the grid or the strips.
pub fn poll(ctx: &mut Ctx) {
    let states = surface(ctx).client.borrow_mut().poll();
    if !is_active(ctx) {
        return;
    }
    for state in states {
        if state == LinkState::Detached || state == LinkState::Incompatible {
            leave(ctx);
        }
        notify_state(ctx);
    }
    if !is_active(ctx) {
        return;
    }
    // Another view or mode button: leave for it
    // (a temporary view, e.g. the Shift view while Shift is held, keeps the surface)
    if ctx.views.active_id_ignore_temporary() != Some(Views::SurfaceLink) || ctx.modes.active_id_ignore_temporary() != Some(Modes::SurfaceLink) {
        leave(ctx);
    }
}

/// The strips of the surface (in `updateFaders`), `None` if the surface mode is not active.
pub fn fader_configs(ctx: &Ctx) -> Option<[FaderConfig; 8]> {
    if !is_active(ctx) || !ctx.modes.is_active(Modes::SurfaceLink) {
        return None;
    }
    let client = surface(ctx).client.borrow();
    Some(std::array::from_fn(|index| match client.encoder_value(index) {
        Some((value, color)) => FaderConfig::new(FaderConfig::TYPE_SINGLE, palette_index(ctx, color), value),
        None => FaderConfig::new(FaderConfig::TYPE_SINGLE, 0, 0),
    }))
}

/// The touch of a strip.
struct StripTouchCommand {
    index: usize,
}

impl TriggerCommand for StripTouchCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if !is_active(ctx) || event == ButtonEvent::Long {
            return;
        }
        let client = surface(ctx).client.clone();
        client.borrow_mut().on_encoder_touch(self.index, event == ButtonEvent::Down);
    }
}

/// What a button does on the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceButton {
    PagePrev,
    PageNext,
    RowUp,
    RowDown,
    Reset,
}

/// A button command which does something else on the surface.
pub struct SurfaceOr<C: TriggerCommand> {
    inner: C,
    button: SurfaceButton,
}

impl<C: TriggerCommand> SurfaceOr<C> {
    pub fn new(inner: C, button: SurfaceButton) -> Self {
        SurfaceOr { inner, button }
    }
}

impl<C: TriggerCommand> TriggerCommand for SurfaceOr<C> {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        if !is_active(ctx) {
            self.inner.execute(ctx, event, velocity);
            return;
        }
        if event != ButtonEvent::Down {
            return;
        }
        let client = surface(ctx).client.clone();
        let mut client = client.borrow_mut();
        match self.button {
            SurfaceButton::PagePrev => client.send(crate::framework::surface_link::Input::PagePrev),
            SurfaceButton::PageNext => client.send(crate::framework::surface_link::Input::PageNext),
            SurfaceButton::RowUp => client.step_row(-1),
            SurfaceButton::RowDown => client.step_row(1),
            SurfaceButton::Reset => client.send(crate::framework::surface_link::Input::SelectedReset),
        }
    }
}

/// The main encoder: the selected slot on the surface, otherwise the encoder modes.
pub struct SurfaceOrEncoder<C: ContinuousCommand> {
    inner: C,
}

impl<C: ContinuousCommand> SurfaceOrEncoder<C> {
    pub fn new(inner: C) -> Self {
        SurfaceOrEncoder { inner }
    }
}

impl<C: ContinuousCommand> ContinuousCommand for SurfaceOrEncoder<C> {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        if !is_active(ctx) {
            self.inner.execute(ctx, value);
            return;
        }
        let fine = ctx.surface.is_shift_pressed();
        let client = surface(ctx).client.clone();
        client.borrow_mut().on_main_encoder(value, fine);
        if fine {
            ctx.surface.set_trigger_consumed(ButtonId::Shift);
        }
    }
}

/// Keep the surface client reachable for the tests.
pub fn client(ctx: &Ctx) -> SharedClient {
    Rc::clone(&surface(ctx).client)
}
