// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The other modes of the Push (former classes directly in `mode`).
//!
//! Mode switches requested by a mode itself (e.g. `modeManager.restore ()` in a button handler)
//! are deferred with [`Ctx::schedule`]: the mode is borrowed while its function runs and could not
//! be deactivated.

pub mod automation;
pub mod fixed;
pub mod frame;
pub mod groove;
pub mod marker;
pub mod metronome;
pub mod note;
pub mod note_repeat;
pub mod ribbon;
pub mod scales;
pub mod session;
pub mod view_select;

use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::graphics::{Component, GraphicDisplay, LabelLayout, MenuComponent, ParameterComponent};
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::parameter_mode::button_row_index;
use crate::midi::ButtonEvent;

pub use automation::AutomationSelectionMode;
pub use fixed::FixedMode;
pub use frame::FrameMode;
pub use groove::{GrooveMode, QuantizeMode};
pub use marker::MarkerMode;
pub use metronome::{AccentMode, MetronomeMode};
pub use note::NoteMode;
pub use note_repeat::NoteRepeatMode;
pub use ribbon::RibbonMode;
pub use scales::{ScaleLayoutMode, ScalesMode};
pub use session::SessionMode;
pub use view_select::{NoteViewSelectMode, SessionViewSelectMode};

/// Register NOTE, FRAME, GROOVE, REC_ARM (quantize), ACCENT, SCALES, SCALE_LAYOUT, FIXED, RIBBON,
/// VIEW_SELECT, AUTOMATION, TRANSPORT, MARKERS, SESSION, SESSION_VIEW_SELECT, REPEAT_NOTE (in the
/// order of `PushControllerSetup.createModes`).
pub fn register_misc_modes(ctx: &mut Ctx) {
    ctx.register_mode(Modes::Note, NoteMode::new());
    ctx.register_mode(Modes::Frame, FrameMode::new());
    ctx.register_mode(Modes::Groove, GrooveMode::new());
    ctx.register_mode(Modes::RecArm, QuantizeMode::new());
    ctx.register_mode(Modes::Accent, AccentMode::new());
    ctx.register_mode(Modes::Scales, ScalesMode::new());
    ctx.register_mode(Modes::ScaleLayout, ScaleLayoutMode::new());
    ctx.register_mode(Modes::Fixed, FixedMode::new());
    ctx.register_mode(Modes::Ribbon, RibbonMode::new());
    ctx.register_mode(Modes::ViewSelect, NoteViewSelectMode::new());
    ctx.register_mode(Modes::Automation, AutomationSelectionMode::new());
    let metronome = MetronomeMode::new(ctx);
    ctx.register_mode(Modes::Transport, metronome);
    let marker = MarkerMode::new(ctx);
    ctx.register_mode(Modes::Markers, marker);
    let session = SessionMode::new(ctx);
    ctx.register_mode(Modes::Session, session);
    ctx.register_mode(Modes::SessionViewSelect, SessionViewSelectMode::new());
    ctx.register_mode(Modes::RepeatNote, NoteRepeatMode::new());
}

/// The functions of the Push 3 encoder of a mode (former `IPush3Encoder`). Implemented by the
/// [`SessionMode`]; the Java `ClipMode` and `DeviceBrowserMode` implement it as well.
pub trait Push3Encoder {
    /// The encoder was turned.
    fn encoder_turn(&mut self, ctx: &mut Ctx, value: i32);
    /// The encoder was moved to the left.
    fn encoder_left(&mut self, ctx: &mut Ctx, event: ButtonEvent);
    /// The encoder was moved to the right.
    fn encoder_right(&mut self, ctx: &mut Ctx, event: ButtonEvent);
    /// The encoder was pressed.
    fn encoder_press(&mut self, ctx: &mut Ctx, event: ButtonEvent);
    /// The button in the center of the 4 arrows was pressed.
    fn arrow_center(&mut self, ctx: &mut Ctx, event: ButtonEvent);
}

/// The Push 3 encoder functions of a mode of this module, if it has some (former `instanceof
/// IPush3Encoder`).
pub fn as_push3_encoder(mode: &mut dyn crate::framework::feature::Mode) -> Option<&mut dyn Push3Encoder> {
    let any = mode.as_any_mut();
    if any.is::<super::track::clip::ClipMode>() {
        return any.downcast_mut::<super::track::clip::ClipMode>().map(|m| m as &mut dyn Push3Encoder);
    }
    if any.is::<super::device::browser::DeviceBrowserMode>() {
        return any.downcast_mut::<super::device::browser::DeviceBrowserMode>().map(|m| m as &mut dyn Push3Encoder);
    }
    any.downcast_mut::<SessionMode>().map(|m| m as &mut dyn Push3Encoder)
}

// ----------------------------------------------------------------------------------------------
// Helpers shared by the modes

/// Former `isButtonRow (row, buttonID)`.
pub(crate) fn row_index(row: usize, button: ButtonId) -> Option<usize> {
    button_row_index(row, button)
}

/// Former `Push1Display.setCell (row, column, value, format)`: the value range is the upper
/// bound of the value changer.
pub(crate) fn set_cell_value(ctx: &Ctx, display: &mut TextDisplay, row: usize, column: usize, value: i32, format: Format) {
    let upper_bound = ctx.model.value_changer().upper_bound();
    crate::controller::push::display1::set_cell_value(display, row, column, value, format, upper_bound);
}

/// Former `addParameterElementWithPlainMenu` with a nullable bottom menu color.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_plain_parameter(
    display: &mut GraphicDisplay,
    top_menu: &str,
    is_top_menu_on: bool,
    bottom_menu: &str,
    bottom_menu_color: Option<ColorEx>,
    is_bottom_menu_on: bool,
    name: &str,
    value: i32,
    value_str: &str,
    is_touched: bool,
    modulated_value: i32,
) {
    let menu = MenuComponent::new(top_menu, is_top_menu_on, bottom_menu, None, bottom_menu_color, is_bottom_menu_on, true, LabelLayout::Plain);
    add_parameter_component(display, menu, name, value, value_str, is_touched, modulated_value);
}

/// Former `addParameterElement (topMenu, isTopMenuOn, bottomMenu, (ChannelType) null, null,
/// false, name, value, valueStr, isTouched, modulated)`: without icon and bottom menu color.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_parameter(display: &mut GraphicDisplay, top_menu: &str, is_top_menu_on: bool, bottom_menu: &str, name: &str, value: i32, value_str: &str, is_touched: bool, modulated_value: i32) {
    let menu = MenuComponent::new(top_menu, is_top_menu_on, bottom_menu, None, None, false, true, LabelLayout::Colored);
    add_parameter_component(display, menu, name, value, value_str, is_touched, modulated_value);
}

fn add_parameter_component(display: &mut GraphicDisplay, menu: MenuComponent, name: &str, value: i32, value_str: &str, is_touched: bool, modulated_value: i32) {
    display.add_element(Component::Parameter(Box::new(ParameterComponent {
        menu,
        param_name: name.to_string(),
        param_value_text: value_str.to_string(),
        param_value: value,
        modulated_param_value: modulated_value,
        is_touched,
    })));
}

/// Former `modeManager.restore ()` from inside a mode.
pub(crate) fn restore_mode_later(ctx: &mut Ctx) {
    ctx.schedule(0, |ctx| ctx.restore_mode());
}

/// Former `modeManager.setActive (mode)` from inside a mode.
pub(crate) fn set_active_mode_later(ctx: &mut Ctx, mode: Modes) {
    ctx.schedule(0, move |ctx| ctx.set_active_mode(mode));
}

/// Former `modeManager.setTemporary (mode)` from inside a mode.
pub(crate) fn set_temporary_mode_later(ctx: &mut Ctx, mode: Modes) {
    ctx.schedule(0, move |ctx| ctx.set_temporary_mode(mode));
}

/// Former `StringUtils.shortenAndFixASCII`.
pub(crate) fn shorten_and_fix_ascii(text: &str, length: usize) -> String {
    crate::daw::util::optimize_name(&crate::config::values::fix_ascii(text), length)
}

/// True if the host has the capabilities of Bitwig which Reaper lacks (former `host.supports
/// (Capability.QUANTIZE_INPUT_NOTE_LENGTH | QUANTIZE_AMOUNT)`): the host reports its record
/// quantization.
pub(crate) fn has_host_record_quantization(ctx: &Ctx) -> bool {
    ctx.model.ctx().host_record_quantization().is_some()
}

/// Implements the `Mode` functions which forward to the `base` (a `ParameterModeBase`).
macro_rules! forward_base {
    () => {
        fn on_activate(&mut self, ctx: &mut Ctx) {
            self.base.on_activate(ctx);
        }

        fn on_deactivate(&mut self, ctx: &mut Ctx) {
            self.base.on_deactivate(ctx);
        }

        $crate::controller::push::mode::misc::forward_base!(no_activate);
    };
    (no_activate) => {
        fn name(&self) -> &str {
            self.base.name()
        }

        fn update_display(&mut self, ctx: &mut Ctx) {
            crate::controller::push::mode::update_display(self, ctx);
        }

        fn is_any_knob_touched(&self) -> bool {
            self.base.is_any_knob_touched()
        }
    };
}
pub(crate) use forward_base;

/// Former `onKnobTouch` which only stores the touch state.
macro_rules! touch_only {
    () => {
        fn on_knob_touch(&mut self, _ctx: &mut Ctx, index: usize, is_touched: bool) {
            self.base.set_touched_knob(index, is_touched);
        }
    };
}
pub(crate) use touch_only;

/// The parameter mode base of the Push modes without a bank (former `BaseMode (name, surface,
/// model)`). Java creates it with `isAbsolute = true`; the knobs of the Push are relative,
/// therefore the default knob handling of the base changes the values relatively.
pub(crate) fn base(name: &str) -> crate::framework::mode::parameter_mode::ParameterModeBase {
    use crate::framework::mode::parameter_mode::{default_knob_ids, ModeBank, ParameterModeBase};
    ParameterModeBase::new(name, false, ModeBank::None, default_knob_ids())
}
