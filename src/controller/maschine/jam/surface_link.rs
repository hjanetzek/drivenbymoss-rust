// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The Maschine JAM as a page surface (doc/SURFACE_LINK.md, doc/HOST_PAGE_PLAN.md §5; not part of
//! Java): the pages of a source (the Electra One over the surface link, the host's pages) on the
//! grid and the touch strips.
//!
//! - **Entry and exit:** Select + SONG cycles: off → the next available source → … → off (the
//!   sources in their order, missing ones skipped; with one source it switches the surface on and
//!   off). Any other view or mode button (SONG, STEP, PAD MODE, MACRO, LEVEL, AUX, CONTROL) leaves
//!   it for its view or mode; a detach of the source leaves it too. The JAM's own modes and views
//!   come back.
//! - **Grid row 1:** the page list (6 pads, the window of the current page), its last two pads the
//!   previous / next page.
//! - **Grid rows 2-7:** the cells, left-aligned, up to 8 columns (the Electra's pages are 6 wide);
//!   in their colors; a press selects a knob cell (and its row) or presses a pad cell; the
//!   selected cell blinks white. With control sets the cells outside the active one are dimmer.
//! - **Grid row 8:** the sections of the source (the host's processors).
//! - **Strips 1-8:** the cells of the active row; with a 6-wide page strip 7 is the selected cell
//!   (fine) and strip 8 the first extra. A swipe turns by the distance (relative, no jumps); with
//!   Shift in fine steps.
//! - **Main encoder:** the selected cell (`selected.turn`), with Shift fine; its push resets it.
//! - **PAGE LEFT/RIGHT:** the previous / next page, **up / down:** the previous / next row,
//!   **left / right:** the previous / next control set (with sets), else the previous / next
//!   value of the selected cell (an envelope's handle).
//! - Without a source the grid shows a red cross.

use std::rc::Rc;

use super::surface::{self as jam, FaderConfig};
use crate::framework::command::{shared, ContinuousCommand, SharedTrigger, TriggerCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::controller::maschine::core::colors;
use crate::framework::page_surface::client::{EncoderFunction, EncoderKind, PadFunction, SharedClient, SurfaceClient, SurfaceMapping, UiColors};
use crate::framework::page_surface::view::{palette_index, state_text, PageSurfaceMode, PageSurfaceView};
use crate::framework::page_surface::host::HostSource;
use crate::framework::page_surface::{ColorDepth, LinkState, PageSource};
use crate::framework::surface_link::{LinkSource, DEFAULT_LINK};
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
    encoders.push(EncoderFunction::RowSlotOrSelected { column: 6, fine: true });
    encoders.push(EncoderFunction::RowSlotOrExtra { column: 7, index: 0 });
    let mut mapping = SurfaceMapping::new(8, 8)
        .page_list_row_at(0, 0, 6)
        .pad(6, 0, PadFunction::PagePrev)
        .pad(7, 0, PadFunction::PageNext)
        .cells_at(0, 1, 8, 6)
        .section_list_row_at(0, 7, 8)
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

/// The sources of the JAM, in the order of Select + SONG: the surface link (the Electra One),
/// the host's pages.
fn sources(mapping: &SurfaceMapping) -> Vec<Box<dyn PageSource>> {
    vec![Box::new(LinkSource::new(DEFAULT_LINK, mapping.capabilities())), Box::new(HostSource::new(mapping.capabilities()))]
}

impl JamSurface {
    pub fn new() -> Self {
        let mapping = mapping();
        let sources = sources(&mapping);
        JamSurface { client: SurfaceClient::shared(mapping, sources), active: false, previous: None }
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
    ctx.register_view(Views::SurfaceLink, PageSurfaceView::new(client.clone()));
    ctx.register_mode(Modes::SurfaceLink, PageSurfaceMode::new(client));
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

/// Switch the surface on, with the first available source (or the current one without any: it
/// shows "no provider").
pub fn enter(ctx: &mut Ctx) {
    if is_active(ctx) {
        return;
    }
    {
        let mut client = surface(ctx).client.borrow_mut();
        if let Some(index) = client.next_available(None) {
            client.select_source(index);
        }
        client.set_shown(true);
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
    surface(ctx).client.borrow_mut().set_shown(false);
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

/// Select + SONG: off → the next available source → … → off.
pub fn toggle(ctx: &mut Ctx) {
    if !is_active(ctx) {
        enter(ctx);
        return;
    }
    let next = {
        let client = surface(ctx).client.borrow();
        client.next_available(Some(client.current_source()))
    };
    match next {
        Some(index) => {
            surface(ctx).client.borrow_mut().select_source(index);
            notify_state(ctx);
        }
        None => {
            leave(ctx);
            ctx.notify("Surface off");
        }
    }
}

/// The tick (in `handle_changes`): the provider's changes, the state, and whether another
/// button took the grid or the strips.
pub fn poll(ctx: &mut Ctx) {
    let client = surface(ctx).client.clone();
    let states = client.borrow_mut().poll(&ctx.model);
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
    /// The previous / next value of the selected slot (e.g. an envelope's handle).
    ValuePrev,
    ValueNext,
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
            SurfaceButton::ValuePrev => client.step_value(-1),
            SurfaceButton::ValueNext => client.step_value(1),
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
