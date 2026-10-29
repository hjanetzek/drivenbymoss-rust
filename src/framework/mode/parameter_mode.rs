// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The base of all modes which bind their knobs/faders to parameters (former
//! `AbstractParameterMode` and the used parts of `AbstractMode`).
//!
//! Usage: a concrete mode contains a [`ParameterModeBase`] and forwards the functions of the
//! [`crate::framework::feature::Mode`] trait which it does not override to it (the base has
//! functions with the same names), e.g.:
//!
//! ```ignore
//! impl Mode for MyMode {
//!     fn name(&self) -> &str { self.base.name() }
//!     fn on_activate(&mut self, ctx: &mut Ctx) { self.base.on_activate(ctx); }
//!     fn on_deactivate(&mut self, ctx: &mut Ctx) { self.base.on_deactivate(ctx); }
//!     fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 { self.base.knob_value(ctx, index) }
//!     ...
//! }
//! ```
//!
//! Differences to Java:
//! - The parameters are [`ParamRef`]s which are resolved when a value is received, therefore the
//!   controls do not need to be re-bound when a bank is scrolled (no parameter observers).
//! - The bank of the mode is a [`ModeBank`] which identifies a bank of the model.
//! - The binding state (controls, providers, active state) is shared with the button event
//!   handlers which re-bind the controls when a modifier button with its own provider is
//!   pressed/released (see [`ParameterModeBase::set_button_parameter_provider`]), therefore it
//!   is kept in the reference counted [`ParameterBinding`].

use std::cell::RefCell;
use std::rc::Rc;

use crate::daw::bank::BankItem;
use crate::daw::events::ParameterBankId;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId};
use crate::framework::parameterprovider::{self, DrumDeviceRef, ParamRef, SharedProvider};
use crate::midi::ButtonEvent;

/// The default knob IDs (former `DEFAULT_KNOB_IDS`).
pub fn default_knob_ids() -> Vec<ContinuousId> {
    sequential_controls(ContinuousId::Knob1, 8)
}

/// Create a list of sequential control IDs (former `ContinuousID.createSequentialList`).
pub fn sequential_controls(first: ContinuousId, count: usize) -> Vec<ContinuousId> {
    (0..count).map(|i| ContinuousId::get(first, i)).collect()
}

/// Returns true if the alternative function of a mode is active (former
/// `isAlternativeFunction` supplier). The default is "Shift is pressed".
pub type AlternativeFunction = fn(&Ctx) -> bool;

fn shift_pressed(ctx: &Ctx) -> bool {
    ctx.surface.is_shift_pressed()
}

// ------------------------------------------------------------------------------------------
// The bank of a mode

/// The bank which is navigated by a parameter mode (former `IBank<B> bank`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeBank {
    /// No bank (all navigation functions do nothing).
    None,
    /// The current track bank of the model.
    TrackBank,
    /// A parameter bank of the model.
    ParameterBank(ParameterBankId),
    /// The drum pad bank (or layer bank) of a device.
    DrumPadBank(DrumDeviceRef),
}

/// The read functions of the paged banks used by [`ModeBank`].
struct BankInfo {
    page_size: usize,
    scroll_position: i32,
    position_of_last_item: i32,
    can_scroll_backwards: bool,
    can_scroll_forwards: bool,
    can_scroll_page_backwards: bool,
    can_scroll_page_forwards: bool,
}

impl ModeBank {
    fn info(self, ctx: &Ctx) -> Option<BankInfo> {
        macro_rules! info {
            ($b:expr) => {{
                let b = $b;
                Some(BankInfo {
                    page_size: b.page_size(),
                    scroll_position: b.scroll_position(),
                    position_of_last_item: b.position_of_last_item(),
                    can_scroll_backwards: b.can_scroll_backwards(),
                    can_scroll_forwards: b.can_scroll_forwards(),
                    can_scroll_page_backwards: b.can_scroll_page_backwards(),
                    can_scroll_page_forwards: b.can_scroll_page_forwards(),
                })
            }};
        }
        match self {
            ModeBank::None => None,
            ModeBank::TrackBank => info!(ctx.model.current_track_bank()),
            ModeBank::ParameterBank(id) => info!(parameterprovider::parameter_bank(&ctx.model, id)?),
            ModeBank::DrumPadBank(device) => {
                let d = specific_device(ctx, device)?;
                if uses_layers(d) {
                    info!(d.layer_bank())
                } else {
                    info!(d.drum_pad_bank())
                }
            }
        }
    }

    pub fn page_size(self, ctx: &Ctx) -> usize {
        self.info(ctx).map_or(0, |i| i.page_size)
    }

    pub fn scroll_position(self, ctx: &Ctx) -> i32 {
        self.info(ctx).map_or(-1, |i| i.scroll_position)
    }

    pub fn position_of_last_item(self, ctx: &Ctx) -> i32 {
        self.info(ctx).map_or(-1, |i| i.position_of_last_item)
    }

    pub fn can_scroll_backwards(self, ctx: &Ctx) -> bool {
        self.info(ctx).is_some_and(|i| i.can_scroll_backwards)
    }

    pub fn can_scroll_forwards(self, ctx: &Ctx) -> bool {
        self.info(ctx).is_some_and(|i| i.can_scroll_forwards)
    }

    pub fn can_scroll_page_backwards(self, ctx: &Ctx) -> bool {
        self.info(ctx).is_some_and(|i| i.can_scroll_page_backwards)
    }

    pub fn can_scroll_page_forwards(self, ctx: &Ctx) -> bool {
        self.info(ctx).is_some_and(|i| i.can_scroll_page_forwards)
    }

    /// Select the item at the index of the page.
    pub fn select_item(self, ctx: &mut Ctx, index: usize) {
        match self {
            ModeBank::None | ModeBank::ParameterBank(_) => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().item_mut(index).select(),
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        BankItem::select(d.layer_bank_mut().item_mut(index));
                    } else {
                        d.drum_pad_bank_mut().select_pad(index);
                    }
                }
            }
        }
    }

    /// Is the item at the index of the page selected?
    pub fn is_item_selected(self, ctx: &Ctx, index: usize) -> bool {
        match self {
            ModeBank::None | ModeBank::ParameterBank(_) => false,
            ModeBank::TrackBank => ctx.model.current_track_bank().item(index).is_selected(),
            ModeBank::DrumPadBank(device) => specific_device(ctx, device).is_some_and(|d| if uses_layers(d) { d.layer_bank().item(index).is_selected() } else { d.drum_pad_bank().item(index).is_selected() }),
        }
    }

    pub fn select_previous_item(self, ctx: &mut Ctx) {
        match self {
            ModeBank::None => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().select_previous_item(),
            ModeBank::ParameterBank(id) => {
                if let Some(bank) = parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                    bank.select_previous_item();
                }
            }
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        d.layer_bank_mut().select_previous_item();
                        return;
                    }
                    let bank = d.drum_pad_bank_mut();
                    let index = bank.selected_index().map_or(0, |i| i as i32 - 1);
                    if index < 0 {
                        bank.scroll_page_backwards();
                    } else {
                        bank.select_pad(index as usize);
                    }
                }
            }
        }
    }

    pub fn select_next_item(self, ctx: &mut Ctx) {
        match self {
            ModeBank::None => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().select_next_item(),
            ModeBank::ParameterBank(id) => {
                if let Some(bank) = parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                    bank.select_next_item();
                }
            }
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        d.layer_bank_mut().select_next_item();
                        return;
                    }
                    let bank = d.drum_pad_bank_mut();
                    let index = bank.selected_index().map_or(0, |i| i + 1);
                    if index >= bank.page_size() {
                        bank.scroll_page_forwards();
                    } else {
                        bank.select_pad(index);
                    }
                }
            }
        }
    }

    pub fn scroll_backwards(self, ctx: &mut Ctx) {
        match self {
            ModeBank::None => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().scroll_backwards(),
            ModeBank::ParameterBank(id) => {
                if let Some(bank) = parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                    bank.scroll_backwards();
                }
            }
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        d.layer_bank_mut().scroll_backwards();
                    } else {
                        d.drum_pad_bank_mut().scroll_backwards();
                    }
                }
            }
        }
    }

    pub fn scroll_forwards(self, ctx: &mut Ctx) {
        match self {
            ModeBank::None => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().scroll_forwards(),
            ModeBank::ParameterBank(id) => {
                if let Some(bank) = parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                    bank.scroll_forwards();
                }
            }
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        d.layer_bank_mut().scroll_forwards();
                    } else {
                        d.drum_pad_bank_mut().scroll_forwards();
                    }
                }
            }
        }
    }

    pub fn select_previous_page(self, ctx: &mut Ctx) {
        match self {
            ModeBank::None => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().select_previous_page(),
            ModeBank::ParameterBank(id) => {
                if let Some(bank) = parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                    bank.select_previous_page();
                }
            }
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        d.layer_bank_mut().scroll_page_backwards();
                    } else {
                        d.drum_pad_bank_mut().scroll_page_backwards();
                    }
                }
            }
        }
    }

    pub fn select_next_page(self, ctx: &mut Ctx) {
        match self {
            ModeBank::None => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().select_next_page(),
            ModeBank::ParameterBank(id) => {
                if let Some(bank) = parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                    bank.select_next_page();
                }
            }
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        d.layer_bank_mut().scroll_page_forwards();
                    } else {
                        d.drum_pad_bank_mut().scroll_page_forwards();
                    }
                }
            }
        }
    }

    pub fn scroll_to(self, ctx: &mut Ctx, position: usize) {
        match self {
            ModeBank::None => {}
            ModeBank::TrackBank => ctx.model.current_track_bank_mut().scroll_to(position, true),
            ModeBank::ParameterBank(id) => {
                if let Some(bank) = parameterprovider::parameter_bank_mut(&mut ctx.model, id) {
                    bank.scroll_to(position as i64, true);
                }
            }
            ModeBank::DrumPadBank(device) => {
                if let Some(d) = specific_device_mut(ctx, device) {
                    if uses_layers(d) {
                        d.layer_bank_mut().scroll_to(position, true);
                    } else {
                        d.drum_pad_bank_mut().scroll_to(position, true);
                    }
                }
            }
        }
    }
}

fn specific_device(ctx: &Ctx, device: DrumDeviceRef) -> Option<&crate::daw::device::SpecificDevice> {
    match device {
        DrumDeviceRef::FirstInstrument => ctx.model.specific_device(crate::daw::constants::DeviceId::FirstInstrument),
        DrumDeviceRef::Drum => ctx.model.drum_device(),
        DrumDeviceRef::Cursor => Some(ctx.model.cursor_device()),
    }
}

fn specific_device_mut(ctx: &mut Ctx, device: DrumDeviceRef) -> Option<&mut crate::daw::device::SpecificDevice> {
    match device {
        DrumDeviceRef::FirstInstrument => ctx.model.specific_device_mut(crate::daw::constants::DeviceId::FirstInstrument),
        DrumDeviceRef::Drum => ctx.model.drum_device_mut(),
        DrumDeviceRef::Cursor => Some(ctx.model.cursor_device_mut()),
    }
}

/// True if the bank of the device are its layers: it has layers but no drum pads (as
/// `DefaultLayerMode.parametersAdjusted` of the Java version). Reaper always has drum pads.
pub(crate) fn uses_layers(device: &crate::daw::device::SpecificDevice) -> bool {
    !device.has_drum_pads() && device.has_layers()
}

// ------------------------------------------------------------------------------------------
// The binding of the controls

struct BindingState {
    controls: Vec<ContinuousId>,
    default_provider: Option<SharedProvider>,
    button_providers: Vec<(ButtonId, SharedProvider)>,
    is_active: bool,
}

/// The controls of a mode and the providers of their parameters. Cloning shares the state.
#[derive(Clone)]
pub struct ParameterBinding(Rc<RefCell<BindingState>>);

impl ParameterBinding {
    pub fn new(controls: Vec<ContinuousId>) -> Self {
        ParameterBinding(Rc::new(RefCell::new(BindingState { controls, default_provider: None, button_providers: Vec::new(), is_active: false })))
    }

    pub fn controls(&self) -> Vec<ContinuousId> {
        self.0.borrow().controls.clone()
    }

    pub fn set_controls(&self, controls: Vec<ContinuousId>) {
        self.0.borrow_mut().controls = controls;
    }

    pub fn is_active(&self) -> bool {
        self.0.borrow().is_active
    }

    pub fn set_active(&self, is_active: bool) {
        self.0.borrow_mut().is_active = is_active;
    }

    pub fn default_provider(&self) -> Option<SharedProvider> {
        self.0.borrow().default_provider.clone()
    }

    /// Get the provider of the first pressed modifier button or the default provider (former
    /// `getParameterProvider`).
    pub fn provider(&self, ctx: &Ctx) -> Option<SharedProvider> {
        let state = self.0.borrow();
        for (button, provider) in &state.button_providers {
            if ctx.surface.is_pressed(*button) {
                return Some(provider.clone());
            }
        }
        state.default_provider.clone()
    }

    /// Bind the controls to the parameters of the current provider, if active (former
    /// `bindControls`).
    pub fn bind_controls(&self, ctx: &mut Ctx) {
        let (controls, is_active, has_default) = {
            let state = self.0.borrow();
            (state.controls.clone(), state.is_active, state.default_provider.is_some())
        };
        if !is_active || !has_default {
            return;
        }
        let Some(provider) = self.provider(ctx) else {
            return;
        };
        for (i, control) in controls.iter().enumerate() {
            let parameter = provider.get(ctx, i);
            ctx.surface.bind_continuous_parameter(*control, Some(parameter));
        }
    }

    /// Remove the parameter bindings of the controls (former `unbindControls`).
    pub fn unbind_controls(&self, ctx: &mut Ctx) {
        let state = self.0.borrow();
        if state.default_provider.is_none() {
            return;
        }
        for control in &state.controls {
            ctx.surface.bind_continuous_parameter(*control, None);
        }
    }

    fn check_size(&self, ctx: &Ctx, provider: &SharedProvider) -> bool {
        let controls_size = self.0.borrow().controls.len();
        let parameter_size = provider.size();
        if controls_size != parameter_size {
            ctx.host.error(&format!("Number of knobs ({controls_size}) must match the number of parameters ({parameter_size})!"));
            return false;
        }
        true
    }

    /// Set the default provider (former `setParameterProvider (provider)`).
    pub fn set_provider(&self, ctx: &Ctx, provider: SharedProvider) {
        if self.check_size(ctx, &provider) {
            self.0.borrow_mut().default_provider = Some(provider);
        }
    }

    /// Set the provider which is used while the button is pressed (former
    /// `setParameterProvider (buttonID, provider)`). Registers handlers on the button which
    /// re-bind the controls when it is pressed or released.
    pub fn set_button_provider(&self, ctx: &mut Ctx, button: ButtonId, provider: SharedProvider) {
        if !self.check_size(ctx, &provider) {
            return;
        }
        if ctx.surface.button(button).is_none() {
            ctx.host.error(&format!("Attempt to set parameters for non-existing button {}!", button.name()));
            return;
        }
        {
            let mut state = self.0.borrow_mut();
            state.button_providers.retain(|(b, _)| *b != button);
            state.button_providers.push((button, provider));
        }
        let binding = self.clone();
        ctx.surface.add_button_event_handler(button, ButtonEvent::Down, Rc::new(move |ctx: &mut Ctx, _| binding.bind_controls(ctx)));
        let binding = self.clone();
        ctx.surface.add_button_event_handler(button, ButtonEvent::Up, Rc::new(move |ctx: &mut Ctx, _| binding.bind_controls(ctx)));
    }
}

// ------------------------------------------------------------------------------------------
// The base of the parameter modes

/// The state and the default functions of a parameter mode (former `AbstractParameterMode`).
pub struct ParameterModeBase {
    pub name: String,
    pub is_absolute: bool,
    /// If true, the bank is scrolled instead of selecting the previous/next item.
    pub scroll_bank: bool,
    pub bank: ModeBank,
    pub binding: ParameterBinding,
    pub is_alternative_function: AlternativeFunction,
    touched: Vec<bool>,
}

impl ParameterModeBase {
    /// Former `AbstractParameterMode (name, surface, model, isAbsolute, bank, controls)`.
    pub fn new(name: &str, is_absolute: bool, bank: ModeBank, controls: Vec<ContinuousId>) -> Self {
        let touched = vec![false; controls.len()];
        ParameterModeBase {
            name: name.to_string(),
            is_absolute,
            scroll_bank: false,
            bank,
            binding: ParameterBinding::new(controls),
            is_alternative_function: shift_pressed,
            touched,
        }
    }

    /// A mode without controls (former `AbstractParameterMode (name, surface, model, isAbsolute)`).
    pub fn without_controls(name: &str, is_absolute: bool, bank: ModeBank) -> Self {
        Self::new(name, is_absolute, bank, Vec::new())
    }

    pub fn with_alternative_function(mut self, is_alternative_function: AlternativeFunction) -> Self {
        self.is_alternative_function = is_alternative_function;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_active(&self) -> bool {
        self.binding.is_active()
    }

    pub fn controls(&self) -> Vec<ContinuousId> {
        self.binding.controls()
    }

    pub fn has_controls(&self) -> bool {
        !self.binding.0.borrow().controls.is_empty()
    }

    /// Former `setControls`.
    pub fn set_controls(&mut self, controls: Vec<ContinuousId>) {
        self.touched = vec![false; controls.len()];
        self.binding.set_controls(controls);
    }

    /// Former `setParameterProvider (provider)`.
    pub fn set_parameter_provider(&mut self, ctx: &Ctx, provider: SharedProvider) {
        self.binding.set_provider(ctx, provider);
    }

    /// Former `setParameterProvider (buttonID, provider)`.
    pub fn set_button_parameter_provider(&mut self, ctx: &mut Ctx, button: ButtonId, provider: SharedProvider) {
        self.binding.set_button_provider(ctx, button, provider);
    }

    /// Former `getParameterProvider`.
    pub fn parameter_provider(&self, ctx: &Ctx) -> Option<SharedProvider> {
        self.binding.provider(ctx)
    }

    /// Get the parameter of the current provider at the index.
    pub fn parameter(&self, ctx: &Ctx, index: usize) -> ParamRef {
        self.parameter_provider(ctx).map(|p| p.get(ctx, index)).unwrap_or_default()
    }

    /// Former `bindControls`.
    pub fn bind_controls(&self, ctx: &mut Ctx) {
        self.binding.bind_controls(ctx);
    }

    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        self.binding.set_active(true);
        self.binding.bind_controls(ctx);
    }

    pub fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.binding.set_active(false);
        self.binding.unbind_controls(ctx);
    }

    /// Former `parametersAdjusted`: re-binds the controls.
    pub fn parameters_adjusted(&self, ctx: &mut Ctx) {
        self.binding.bind_controls(ctx);
    }

    /// Former `switchBanks`.
    pub fn switch_banks(&mut self, bank: ModeBank) {
        self.bank = bank;
    }

    /// The value of the parameter of the knob or -1 if it does not exist (former
    /// `AbstractParameterMode.getKnobValue`).
    pub fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        let Some(provider) = self.parameter_provider(ctx) else {
            return -1;
        };
        let parameter = provider.get(ctx, index);
        if parameter.does_exist(ctx) {
            parameter.value(ctx)
        } else {
            -1
        }
    }

    /// Default knob handling for modes which are not bound (e.g. relative knobs bound to
    /// commands): set or change the parameter of the provider.
    pub fn on_knob_value(&self, ctx: &mut Ctx, index: usize, value: i32) {
        let parameter = self.parameter(ctx, index);
        if self.is_absolute {
            parameter.set_value(ctx, value);
        } else {
            parameter.change_value(ctx, value);
        }
    }

    /// Default touch handling: reset the parameter if Delete is pressed and touch it.
    pub fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.set_touched_knob(index, is_touched);
        let parameter = self.parameter(ctx, index);
        if !parameter.does_exist(ctx) {
            return;
        }
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            parameter.reset_value(ctx);
        }
        parameter.touch_value(ctx, is_touched);
    }

    // Touch states (former `AbstractMode`)

    pub fn set_touched_knob(&mut self, index: usize, is_touched: bool) {
        if let Some(touched) = self.touched.get_mut(index) {
            *touched = is_touched;
        }
    }

    pub fn is_any_knob_touched(&self) -> bool {
        self.touched.iter().any(|t| *t)
    }

    pub fn touched_knob(&self) -> Option<usize> {
        self.touched.iter().position(|t| *t)
    }

    pub fn is_knob_touched(&self, index: usize) -> bool {
        self.touched.get(index).copied().unwrap_or(false)
    }

    // Bank navigation

    pub fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.bank.select_item(ctx, index);
    }

    pub fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if (self.is_alternative_function)(ctx) {
            self.select_previous_item_page(ctx);
            return;
        }
        if self.scroll_bank {
            self.bank.scroll_backwards(ctx);
        } else {
            self.bank.select_previous_item(ctx);
        }
    }

    pub fn select_next_item(&mut self, ctx: &mut Ctx) {
        if (self.is_alternative_function)(ctx) {
            self.select_next_item_page(ctx);
            return;
        }
        if self.scroll_bank {
            self.bank.scroll_forwards(ctx);
        } else {
            self.bank.select_next_item(ctx);
        }
    }

    pub fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.bank.select_previous_page(ctx);
    }

    pub fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.bank.select_next_page(ctx);
    }

    pub fn select_item_page(&mut self, ctx: &mut Ctx, page: usize) {
        let position = page * self.bank.page_size(ctx);
        self.bank.scroll_to(ctx, position);
    }

    pub fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.bank.can_scroll_backwards(ctx)
    }

    pub fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.bank.can_scroll_forwards(ctx)
    }

    pub fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.bank.can_scroll_page_backwards(ctx)
    }

    pub fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.bank.can_scroll_page_forwards(ctx)
    }

    /// Format the range of the page, `{first}` and `{last}` are replaced by the 1-based
    /// positions (former `formatPageRange` with a `%d`-format).
    pub fn format_page_range(&self, ctx: &Ctx, format: &str) -> String {
        let first = self.bank.scroll_position(ctx);
        if first < 0 {
            return String::new();
        }
        let last = self.bank.position_of_last_item(ctx);
        format.replace("{first}", &(first + 1).to_string()).replace("{last}", &(last + 1).to_string())
    }
}

/// Former `AbstractMode.isButtonRow`: the index of the button in the row (0-5) or `None`.
pub fn button_row_index(row: usize, button: ButtonId) -> Option<usize> {
    let first = match row {
        0 => ButtonId::Row11,
        1 => ButtonId::Row21,
        2 => ButtonId::Row31,
        3 => ButtonId::Row41,
        4 => ButtonId::Row51,
        5 => ButtonId::Row61,
        _ => return None,
    };
    let ordinal = button.ordinal();
    (ordinal >= first.ordinal() && ordinal < first.ordinal() + 8).then(|| ordinal - first.ordinal())
}

/// Former `AbstractMode.getButtonRow`: the row (0-5) of a row button or `None`.
pub fn button_row(button: ButtonId) -> Option<usize> {
    (0..6).find(|row| button_row_index(*row, button).is_some())
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use crate::controller::launchpad::definition::LaunchpadModel;
    use crate::controller::launchpad::LaunchpadFactory;
    use crate::controller::{ControllerFactory, MidiPorts};
    use crate::framework::command::FnTrigger;
    use crate::framework::ctx::MAIN_MODE_MANAGER;
    use crate::framework::ids::Modes;
    use crate::framework::mode::dummy::DummyMode;
    use crate::framework::parameterprovider::{
        provider, CombinedParameterProvider, EmptyParameterProvider, FixedParameterProvider, FnParameterProvider, ParameterProvider,
        RangeFilterParameterProvider, VolumeParameterProvider,
    };
    use crate::framework::surface::ContinuousKind;
    use crate::host::{Host, MemoryLog};
    use crate::midi::BindType;
    use crate::parser::Parser;

    fn create_ctx() -> (Ctx, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        let sender = Sender::new(backend.clone());
        let host = Host::new(Rc::new(MemoryLog::default()));
        let factory = LaunchpadFactory::new(LaunchpadModel::X);
        let config = factory.create_configuration();
        let mut ctx = factory.create(host, sender, MidiPorts { input: 3, output: 5 }, config).expect("instance").ctx;
        let mut parser = Parser::new();
        for line in ["/track/count 2", "/track/0/exists 1", "/track/0/select 1", "/track/1/exists 1"] {
            parser.parse_line(&mut ctx.model, line);
        }
        ctx.model.take_events();
        for (i, id) in [ContinuousId::Knob1, ContinuousId::Knob2].iter().enumerate() {
            ctx.surface.create_continuous(*id, "Knob", ContinuousKind::AbsoluteKnob);
            ctx.surface.bind_continuous(*id, BindType::Cc, 0, 20 + i as i32);
        }
        backend.take_calls();
        (ctx, backend)
    }

    fn volume_of(calls: &[BackendCall], track: usize) -> Option<f64> {
        calls.iter().rev().find_map(|c| match c {
            BackendCall::DoubleArg(p, cmd, v) if p == "track" && *cmd == format!("{track}/volume") => Some(*v),
            _ => None,
        })
    }

    #[test]
    fn special_providers() {
        let (ctx, _backend) = create_ctx();
        let combined = CombinedParameterProvider::new(vec![
            provider(FixedParameterProvider::new(vec![ParamRef::MasterVolume, ParamRef::MasterPan])),
            provider(EmptyParameterProvider::new(1)),
            provider(VolumeParameterProvider::new(&ctx)),
        ]);
        assert_eq!(combined.size(), 11);
        assert_eq!(combined.get(&ctx, 1), ParamRef::MasterPan);
        assert_eq!(combined.get(&ctx, 2), ParamRef::Empty);
        assert_eq!(combined.get(&ctx, 4), ParamRef::TrackVolume { index: 1 });
        assert_eq!(combined.get(&ctx, 11), ParamRef::Empty);
        let range = RangeFilterParameterProvider::new(provider(combined), 3, 2);
        assert_eq!(range.size(), 2);
        assert_eq!(range.get(&ctx, 1), ParamRef::TrackVolume { index: 1 });
    }

    #[test]
    fn param_refs_resolve_against_the_model() {
        let (mut ctx, backend) = create_ctx();
        let volume = ParamRef::TrackVolume { index: 0 };
        assert!(volume.does_exist(&ctx));
        volume.set_value(&mut ctx, 127);
        assert_eq!(volume_of(&backend.take_calls(), 0), Some(1.0));
        assert!(!ParamRef::TrackVolume { index: 5 }.does_exist(&ctx));
        assert!(!ParamRef::Empty.does_exist(&ctx));
        assert!(ParamRef::SelectedTrackVolume.does_exist(&ctx));
        ParamRef::SelectedTrackVolume.set_normalized_value(&mut ctx, 0.0);
        assert_eq!(volume_of(&backend.take_calls(), 0), Some(0.0));
    }

    #[test]
    fn mode_binds_the_controls_while_active_and_switches_with_modifier() {
        let (mut ctx, backend) = create_ctx();
        let controls = vec![ContinuousId::Knob1, ContinuousId::Knob2];
        let mut base = ParameterModeBase::new("Test", true, ModeBank::TrackBank, controls);
        base.set_parameter_provider(&ctx, provider(FnParameterProvider::new(2, |_, i| ParamRef::TrackVolume { index: i })));
        ctx.add_button(ButtonId::Shift, "Shift", Some(Rc::new(std::cell::RefCell::new(FnTrigger(|_: &mut Ctx, _, _| {})))), BindType::Cc, 0, 90, None);
        base.set_button_parameter_provider(&mut ctx, ButtonId::Shift, provider(FixedParameterProvider::new(vec![ParamRef::TrackVolume { index: 1 }, ParamRef::Empty])));

        // Not active: not bound
        ctx.handle_midi(0xB0, 20, 127);
        assert_eq!(volume_of(&backend.take_calls(), 0), None);

        base.on_activate(&mut ctx);
        ctx.handle_midi(0xB0, 20, 127);
        assert_eq!(volume_of(&backend.take_calls(), 0), Some(1.0));
        assert_eq!(base.knob_value(&ctx, 1), ctx.model.track_bank().item(1).volume());

        // With Shift the first knob controls the 2nd track
        ctx.handle_midi(0xB0, 90, 127);
        ctx.handle_midi(0xB0, 20, 0);
        assert_eq!(volume_of(&backend.take_calls(), 1), Some(0.0));
        ctx.handle_midi(0xB0, 90, 0);
        ctx.handle_midi(0xB0, 20, 0);
        assert_eq!(volume_of(&backend.take_calls(), 0), Some(0.0));

        base.on_deactivate(&mut ctx);
        assert_eq!(ctx.surface.continuous(ContinuousId::Knob1).unwrap().parameter, None);
    }

    #[test]
    fn additional_mode_managers() {
        let (mut ctx, _backend) = create_ctx();
        let manager = ctx.add_mode_manager();
        assert_ne!(manager, MAIN_MODE_MANAGER);
        let mode = DummyMode::new(&ctx, vec![ContinuousId::Knob1]);
        ctx.register_mode_in(manager, Modes::Dummy, mode);
        ctx.register_mode_in(manager, Modes::Volume, DummyMode::new(&ctx, Vec::new()));
        ctx.set_active_mode_in(manager, Modes::Dummy);
        assert!(ctx.mode_manager(manager).is_active(Modes::Dummy));
        assert!(!ctx.modes.is_active(Modes::Dummy));
        // The dummy mode blocks the knob
        assert_eq!(ctx.surface.continuous(ContinuousId::Knob1).unwrap().parameter, Some(ParamRef::Empty));
        ctx.set_temporary_mode_in(manager, Modes::Volume);
        assert!(ctx.mode_manager(manager).is_active(Modes::Volume));
        assert_eq!(ctx.surface.continuous(ContinuousId::Knob1).unwrap().parameter, None);
        ctx.restore_mode_in(manager);
        assert!(ctx.mode_manager(manager).is_active(Modes::Dummy));
        let name = ctx.active_mode_in(manager).unwrap().borrow().name().to_string();
        assert_eq!(name, "Dummy");
    }

    #[test]
    fn button_rows() {
        assert_eq!(button_row_index(1, ButtonId::Row23), Some(2));
        assert_eq!(button_row_index(0, ButtonId::Row23), None);
        assert_eq!(button_row(ButtonId::Row68), Some(5));
        assert_eq!(button_row(ButtonId::Play), None);
    }
}
