// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Fire which control the 4 knobs and draw the display (former
//! `FireTrackMode`, `FireTrackMixerMode`, `FireLayerMode`, `FireLayerMixerMode`,
//! `FireParameterMode`, `FireUserMode`, `FireNoteMode` and `BrowserMode`).
//!
//! All knob modes use 4 knobs; with ALT pressed the knobs control the parameters 5 to 8 (former
//! `FourKnobProvider`). The parameters of the selected drum pad (layer modes) are taken from the
//! drum device with 16 pads.
//!
//! Differences to Java: the displayed values of the note attributes are formatted as
//! percentages (the Java `NoteParameter.getDisplayedValue` formats them per attribute).

use std::rc::Rc;

use super::surface::{self as fire, add_element, is_alt_pressed};
use crate::daw::events::ParameterBankId;
use crate::daw::model::Model;
use crate::daw::parameter::Parameter;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::graphics::Component;
use crate::framework::ids::{ContinuousId, Modes};
use crate::framework::mode::note_editor::NoteEditor;
use crate::framework::mode::note_parameter::{change_note_value, note_value, set_note_value, NoteAttribute};
use crate::framework::mode::parameter_mode::{sequential_controls, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, CustomParameter, FnParameterProvider, ParamRef, ParameterProvider};
use crate::framework::string_utils::format_percentage;
use crate::impl_as_any;
use crate::scale::Scales;

/// The parameters of the knobs in the track and layer modes (former `MODES`).
const MODES: [Modes; 4] = [Modes::Volume, Modes::Pan, Modes::Send1, Modes::Send2];
/// The parameters of the knobs with ALT (former `ALT_MODES`).
const ALT_MODES: [Modes; 4] = [Modes::Send3, Modes::Send4, Modes::Send5, Modes::Send6];

/// The drum device with 16 pads (former `model.getDrumDevice (16)`).
pub const DRUM_PADS: usize = 16;

fn knob_ids() -> Vec<ContinuousId> {
    sequential_controls(ContinuousId::Knob1, 4)
}

/// The index of the parameter of a knob: the parameters 5 to 8 with ALT (former
/// `FourKnobProvider`).
fn four_knob_index(ctx: &Ctx, index: usize) -> usize {
    if is_alt_pressed(ctx) {
        index + 4
    } else {
        index
    }
}

/// Wraps a provider of 8 parameters into one for 4 knobs (former `FourKnobProvider`).
fn four_knob_provider(inner: Rc<dyn ParameterProvider>) -> Rc<dyn ParameterProvider> {
    provider(FnParameterProvider::new(4, move |ctx: &Ctx, index| inner.get(ctx, four_knob_index(ctx, index))))
}

/// The parameter of the selected drum pad: 0 is the volume, 1 the panorama, 2+ the sends
/// (former `SelectedLayerOrDrumPadParameterProvider`).
struct SelectedPadParameter {
    index: usize,
}

fn selected_pad_parameter(model: &Model, index: usize) -> Option<&Parameter> {
    let pad = model.drum_device_sized(DRUM_PADS)?.drum_pad_bank().selected_item()?;
    match index {
        0 => Some(pad.volume_parameter()),
        1 => Some(pad.pan_parameter()),
        n => (n - 2 < pad.send_bank().page_size()).then(|| &**pad.send_bank().item(n - 2)),
    }
}

fn selected_pad_parameter_mut(model: &mut Model, index: usize) -> Option<&mut Parameter> {
    let index_of_pad = model.drum_device_sized(DRUM_PADS)?.drum_pad_bank().selected_index()?;
    let pad = model.drum_device_sized_mut(DRUM_PADS)?.drum_pad_bank_mut().item_mut(index_of_pad);
    match index {
        0 => Some(pad.volume_parameter_mut()),
        1 => Some(pad.pan_parameter_mut()),
        n => {
            if n - 2 >= pad.send_bank().page_size() {
                return None;
            }
            Some(&mut **pad.send_bank_mut().item_mut(n - 2))
        }
    }
}

impl CustomParameter for SelectedPadParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool {
        selected_pad_parameter(&ctx.model, self.index).is_some_and(|p| p.does_exist())
    }

    fn name(&self, ctx: &Ctx) -> String {
        selected_pad_parameter(&ctx.model, self.index).map(|p| p.name().to_string()).unwrap_or_default()
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        selected_pad_parameter(&ctx.model, self.index).map(|p| p.value()).unwrap_or(0)
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        selected_pad_parameter(&ctx.model, self.index).map(|p| p.displayed_value()).unwrap_or_default()
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        if let Some(p) = selected_pad_parameter_mut(&mut ctx.model, self.index) {
            p.set_value(value);
        }
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        if let Some(p) = selected_pad_parameter_mut(&mut ctx.model, self.index) {
            p.set_normalized_value(value);
        }
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        if let Some(p) = selected_pad_parameter_mut(&mut ctx.model, self.index) {
            p.change_value(control);
        }
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        if let Some(p) = selected_pad_parameter_mut(&mut ctx.model, self.index) {
            p.reset_value();
        }
    }

    fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        if let Some(p) = selected_pad_parameter_mut(&mut ctx.model, self.index) {
            p.touch_value(is_touched);
        }
    }
}

/// The parameters of the selected track (former `SelectedTrackParameterProvider`).
fn selected_track_parameter(ctx: &Ctx, index: usize) -> ParamRef {
    if ctx.model.current_track_bank().selected_item().is_none() {
        return ParamRef::Empty;
    }
    match index {
        0 => ParamRef::SelectedTrackVolume,
        1 => ParamRef::SelectedTrackPan,
        _ => ParamRef::SelectedTrackSend { send: index - 2 },
    }
}

/// The index of a mode in the parameter list: volume, panorama, send 1-6.
fn parameter_index(mode: Modes) -> usize {
    match mode {
        Modes::Volume => 0,
        Modes::Pan => 1,
        _ => 2 + (mode.ordinal() - Modes::Send1.ordinal()),
    }
}

/// The values of a channel for the display.
struct ChannelInfo {
    position: i32,
    name: String,
    volume: i32,
    volume_str: String,
    pan: i32,
    pan_str: String,
    vu: i32,
    vu_left: i32,
    vu_right: i32,
    is_selected: bool,
    /// The sends: does exist, name, value, displayed value.
    sends: Vec<(bool, String, i32, String)>,
}

impl ChannelInfo {
    fn of(channel: &crate::daw::channel::Channel) -> Self {
        let sends = (0..channel.send_bank().page_size())
            .map(|i| {
                let send = channel.send_bank().item(i);
                (send.does_exist(), send.name_limited(4), send.value(), send.displayed_value())
            })
            .collect();
        ChannelInfo {
            position: channel.position(),
            name: channel.name_limited(9),
            volume: channel.volume(),
            volume_str: channel.volume_str(),
            pan: channel.pan(),
            pan_str: channel.pan_str(),
            vu: channel.vu(),
            vu_left: channel.vu_left(),
            vu_right: channel.vu_right(),
            is_selected: channel.is_selected(),
            sends,
        }
    }

    /// Former `getSendLabel`.
    fn send_label(&self, index: usize) -> String {
        match self.sends.get(index) {
            Some((true, name, _, value)) => format!("{name}: {value}"),
            _ => format!("No FX {}", index + 1),
        }
    }

    /// Former `getSendValue`.
    fn send_value(&self, index: usize) -> i32 {
        match self.sends.get(index) {
            Some((true, _, value, _)) => *value,
            _ => 0,
        }
    }
}

/// The track and layer modes, with one channel or all channels (mixer) on the display (former
/// `FireTrackMode`, `FireTrackMixerMode`, `FireLayerMode` and `FireLayerMixerMode`).
pub struct FireChannelMode {
    pub base: ParameterModeBase,
    /// The parameter which is shown on the display (former `selectedParameter`).
    pub selected_parameter: Modes,
    is_layer: bool,
    is_mixer: bool,
}

impl FireChannelMode {
    fn new(ctx: &Ctx, name: &str, is_layer: bool, is_mixer: bool) -> Self {
        let bank = if is_layer { ModeBank::None } else { ModeBank::TrackBank };
        let mut base = ParameterModeBase::new(name, false, bank, knob_ids());
        let inner: Rc<dyn ParameterProvider> = if is_layer {
            let parameters: Vec<ParamRef> = (0..8).map(|index| ParamRef::Custom(Rc::new(SelectedPadParameter { index }))).collect();
            provider(FnParameterProvider::new(8, move |_ctx: &Ctx, index| parameters[index].clone()))
        } else {
            provider(FnParameterProvider::new(8, selected_track_parameter))
        };
        base.set_parameter_provider(ctx, four_knob_provider(inner));
        FireChannelMode { base, selected_parameter: Modes::Volume, is_layer, is_mixer }
    }

    /// Former `FireTrackMode`.
    pub fn track(ctx: &Ctx) -> Self {
        Self::new(ctx, "Track", false, false)
    }

    /// Former `FireTrackMixerMode`.
    pub fn track_mixer(ctx: &Ctx) -> Self {
        Self::new(ctx, "Mixer", false, true)
    }

    /// Former `FireLayerMode`.
    pub fn layer(ctx: &Ctx) -> Self {
        Self::new(ctx, "Channel", true, false)
    }

    /// Former `FireLayerMixerMode`.
    pub fn layer_mixer(ctx: &Ctx) -> Self {
        Self::new(ctx, "Layer Mixer", true, true)
    }

    /// The selected channel.
    fn selected_channel(&self, ctx: &Ctx) -> Option<ChannelInfo> {
        if self.is_layer {
            let pad = ctx.model.drum_device_sized(DRUM_PADS)?.drum_pad_bank().selected_item()?;
            Some(ChannelInfo::of(pad))
        } else {
            ctx.model.current_track_bank().selected_item().map(|track| ChannelInfo::of(track))
        }
    }

    /// All channels of the page.
    fn channels(&self, ctx: &Ctx) -> Vec<ChannelInfo> {
        if self.is_layer {
            let Some(device) = ctx.model.drum_device_sized(DRUM_PADS) else {
                return Vec::new();
            };
            let bank = device.drum_pad_bank();
            (0..bank.page_size()).map(|i| ChannelInfo::of(bank.item(i))).collect()
        } else {
            let bank = ctx.model.current_track_bank();
            (0..bank.page_size()).map(|i| ChannelInfo::of(bank.item(i))).collect()
        }
    }

    /// Switch the selected parameter if ALT changed (former `updateMode`).
    fn update_mode(&mut self, ctx: &Ctx) {
        let is_alt = is_alt_pressed(ctx);
        let ms = if is_alt { &MODES } else { &ALT_MODES };
        if let Some(index) = ms.iter().position(|m| *m == self.selected_parameter) {
            self.selected_parameter = if is_alt { ALT_MODES[index] } else { MODES[index] };
        }
    }

    fn draw_single(&self, ctx: &mut Ctx) {
        let (mut desc, mut label) = ("Select".to_string(), if self.is_layer { "a drum pad" } else { "a track" }.to_string());
        let (mut value, mut vu_left, mut vu_right, mut is_pan) = (-1, -1, -1, false);
        if let Some(channel) = self.selected_channel(ctx) {
            vu_left = channel.vu_left;
            vu_right = channel.vu_right;
            desc = format!("{}: {}", channel.position + 1, channel.name);
            match self.selected_parameter {
                Modes::Volume => {
                    label = format!("Vol: {}", channel.volume_str);
                    value = channel.volume;
                }
                Modes::Pan => {
                    label = format!("Pan: {}", channel.pan_str);
                    value = channel.pan;
                    is_pan = true;
                }
                mode => {
                    let send_index = parameter_index(mode) - 2;
                    label = channel.send_label(send_index);
                    value = channel.send_value(send_index);
                }
            }
        }
        add_element(ctx, Component::title_value_vu(&desc, &label, value, vu_left, vu_right, is_pan));
    }

    fn draw_mixer(&self, ctx: &mut Ctx) {
        let label = match self.selected_channel(ctx) {
            Some(channel) => format!("{}: {}", channel.position + 1, channel.name),
            None => "None".to_string(),
        };
        let channels = self.channels(ctx);
        let is_mode = self.base.is_any_knob_touched() || !ctx.model.transport().is_playing();
        let selected = channels.iter().map(|c| c.is_selected).collect();
        let values = channels
            .iter()
            .map(|c| {
                if !is_mode {
                    return c.vu;
                }
                match self.selected_parameter {
                    Modes::Volume => c.volume,
                    Modes::Pan => c.pan,
                    mode => c.send_value(parameter_index(mode) - 2),
                }
            })
            .collect();
        add_element(ctx, Component::title_channels(&label, selected, values, self.selected_parameter == Modes::Pan && is_mode));
    }
}

impl Mode for FireChannelMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    crate::forward_mode!(base => on_activate, on_deactivate, knob_value, on_knob_value);

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        let index = index.min(3);
        self.selected_parameter = if is_alt_pressed(ctx) { ALT_MODES[index] } else { MODES[index] };
        self.base.set_touched_knob(index, is_touched);
        let parameter = if self.is_layer {
            ParamRef::Custom(Rc::new(SelectedPadParameter { index: parameter_index(self.selected_parameter) }))
        } else {
            selected_track_parameter(ctx, parameter_index(self.selected_parameter))
        };
        if !parameter.does_exist(ctx) && parameter_index(self.selected_parameter) < 2 {
            return;
        }
        if is_touched && fire::is_delete_pressed(ctx) {
            parameter.reset_value(ctx);
        }
        parameter.touch_value(ctx, is_touched);
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        self.update_mode(ctx);
        if self.is_mixer {
            self.draw_mixer(ctx);
        } else {
            self.draw_single(ctx);
        }
    }

    impl_as_any!();
}

/// The index of the touched knob, with ALT the knobs 5 to 8.
fn touched_parameter(base: &ParameterModeBase, ctx: &Ctx) -> Option<usize> {
    base.touched_knob().map(|knob| four_knob_index(ctx, knob))
}

/// The device parameter mode (former `FireParameterMode`).
pub struct FireParameterMode {
    pub base: ParameterModeBase,
}

impl FireParameterMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = ParameterModeBase::new("Parameters", false, ModeBank::ParameterBank(ParameterBankId::CursorDevice), knob_ids());
        let inner = provider(FnParameterProvider::new(8, |_ctx: &Ctx, index| ParamRef::BankItem { bank: ParameterBankId::CursorDevice, index }));
        base.set_parameter_provider(ctx, four_knob_provider(inner));
        FireParameterMode { base }
    }
}

impl Mode for FireParameterMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    crate::forward_mode!(base => on_activate, on_deactivate, knob_value, on_knob_value, on_knob_touch, is_any_knob_touched);

    fn update_display(&mut self, ctx: &mut Ctx) {
        let (mut desc, mut param_line, mut value) = ("Select".to_string(), "a device".to_string(), -1);
        if ctx.model.has_selected_device() {
            let device = ctx.model.cursor_device();
            let page = device.parameter_bank().map(|bank| bank.page_bank().selected_item()).unwrap_or_default();
            desc = if page.is_empty() { "None".to_string() } else { page };
            match touched_parameter(&self.base, ctx) {
                Some(index) => {
                    let parameter = ParamRef::BankItem { bank: ParameterBankId::CursorDevice, index };
                    param_line = limit(&parameter.name(ctx), 5);
                    if !param_line.is_empty() {
                        value = parameter.value(ctx);
                        param_line = format!("{param_line}: {}", limit(&parameter.displayed_value(ctx), 6));
                    }
                }
                None => param_line = device.name().to_string(),
            }
        }
        add_element(ctx, Component::title_value(&desc, &param_line, value, false));
    }

    impl_as_any!();
}

fn limit(text: &str, length: usize) -> String {
    text.chars().take(length).collect()
}

/// The user mode, which controls the project or the track parameters (former `FireUserMode`).
pub struct FireUserMode {
    pub base: ParameterModeBase,
    pub is_project_mode: bool,
}

impl FireUserMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut mode = FireUserMode { base: ParameterModeBase::new("User", false, ModeBank::ParameterBank(ParameterBankId::Project), knob_ids()), is_project_mode: true };
        mode.set_provider(ctx);
        mode
    }

    fn bank_id(&self) -> ParameterBankId {
        if self.is_project_mode {
            ParameterBankId::Project
        } else {
            ParameterBankId::CursorTrack
        }
    }

    fn set_provider(&mut self, ctx: &Ctx) {
        let bank = self.bank_id();
        let inner = provider(FnParameterProvider::new(8, move |_ctx: &Ctx, index| ParamRef::BankItem { bank, index }));
        self.base.switch_banks(ModeBank::ParameterBank(bank));
        self.base.set_parameter_provider(ctx, four_knob_provider(inner));
    }

    /// Switch between the project and the track parameters (former `setMode`).
    pub fn set_mode(&mut self, ctx: &mut Ctx, is_project_mode: bool) {
        self.is_project_mode = is_project_mode;
        self.set_provider(ctx);
        self.base.bind_controls(ctx);
    }
}

impl Mode for FireUserMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    crate::forward_mode!(base => on_activate, on_deactivate, knob_value, on_knob_value, on_knob_touch, is_any_knob_touched);

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.bank.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.bank.select_next_item(ctx);
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let bank = self.bank_id();
        let page = crate::framework::parameterprovider::parameter_bank(&ctx.model, bank).map(|b| b.page_bank().selected_item()).unwrap_or_default();
        let desc = if page.is_empty() { "None".to_string() } else { page };
        let mut value = -1;
        let param_line = match touched_parameter(&self.base, ctx) {
            Some(index) => {
                let parameter = ParamRef::BankItem { bank, index };
                let name = limit(&parameter.name(ctx), 5);
                if name.is_empty() {
                    "Not mapped".to_string()
                } else {
                    value = parameter.value(ctx);
                    format!("{name}: {}", limit(&parameter.displayed_value(ctx), 6))
                }
            }
            None => if self.is_project_mode { "Project" } else { "Track" }.to_string(),
        };
        add_element(ctx, Component::title_value(&desc, &param_line, value, false));
    }

    impl_as_any!();
}

/// The note attributes of the knobs and the select knob (index 4).
const NOTE_ATTRIBUTES: [NoteAttribute; 5] = [NoteAttribute::Gain, NoteAttribute::Panning, NoteAttribute::Duration, NoteAttribute::Velocity, NoteAttribute::Transpose];
/// The note attributes with ALT.
const ALT_NOTE_ATTRIBUTES: [NoteAttribute; 5] = [NoteAttribute::Pressure, NoteAttribute::Timbre, NoteAttribute::Chance, NoteAttribute::VelocitySpread, NoteAttribute::Repeat];

/// Edits the attributes of the selected notes (former `FireNoteMode`). The knobs 1 to 4 and the
/// select knob (index 4, transpose) control the attributes.
pub struct FireNoteMode {
    pub base: ParameterModeBase,
    pub note_editor: NoteEditor,
}

impl FireNoteMode {
    pub fn new() -> Self {
        let mut knobs = knob_ids();
        knobs.push(ContinuousId::ViewSelection);
        FireNoteMode { base: ParameterModeBase::new("Note Edit", false, ModeBank::None, knobs), note_editor: NoteEditor::new() }
    }

    fn attribute(ctx: &Ctx, index: usize) -> NoteAttribute {
        let index = index.min(4);
        if is_alt_pressed(ctx) {
            ALT_NOTE_ATTRIBUTES[index]
        } else {
            NOTE_ATTRIBUTES[index]
        }
    }

    /// Reset the transposition of the edited notes (former `resetTranspose`).
    pub fn reset_transpose(&self, ctx: &mut Ctx) {
        let editor = self.note_editor.clone();
        let upper = ctx.model.value_changer().upper_bound();
        set_note_value(ctx, &editor, NoteAttribute::Transpose, upper / 2);
    }
}

impl Default for FireNoteMode {
    fn default() -> Self {
        Self::new()
    }
}

impl Mode for FireNoteMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let editor = self.note_editor.clone();
        change_note_value(ctx, &editor, Self::attribute(ctx, index), value);
        // Note was modified, prevent deletion of note on button up
        super::view::set_active_sequencer_note_edited(ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        note_value(ctx, &self.note_editor, Self::attribute(ctx, index))
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        let Some(clip) = self.note_editor.clip() else {
            return;
        };
        if self.base.is_knob_touched(index) == is_touched {
            return;
        }
        self.base.set_touched_knob(index, is_touched);
        if is_touched {
            let notes = self.note_editor.notes().to_vec();
            ctx.model.note_clip(clip.0, clip.1).start_edit(&notes);
            super::view::set_active_sequencer_note_edited(ctx);
        } else {
            ctx.model.note_clip(clip.0, clip.1).stop_edit();
        }
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let notes = self.note_editor.notes();
        let desc = match notes.len() {
            0 => "Select a note".to_string(),
            1 => format!("Step: {} - {}", notes[0].step + 1, Scales::format_note_and_octave(notes[0].note, -3)),
            n => format!("{n} notes sel."),
        };
        let (mut param_line, mut value) = (String::new(), -1);
        if let Some(knob) = self.base.touched_knob() {
            let attribute = Self::attribute(ctx, knob);
            value = note_value(ctx, &self.note_editor, attribute);
            let normalized = ctx.model.value_changer().to_normalized_value(value as f64);
            param_line = format!("{}: {}", limit(attribute.name(), 5), format_percentage(normalized));
        }
        add_element(ctx, Component::title_value(&desc, &param_line, value, false));
    }

    fn note_editor(&self) -> Option<&NoteEditor> {
        Some(&self.note_editor)
    }

    fn note_editor_mut(&mut self) -> Option<&mut NoteEditor> {
        Some(&mut self.note_editor)
    }

    impl_as_any!();
}

/// The browser mode (former `BrowserMode`): the knobs 1 to 4 (5 to 8 with ALT) select the
/// filter items, the select knob (index 8) the result.
pub struct FireBrowserMode {
    pub base: ParameterModeBase,
}

impl FireBrowserMode {
    pub fn new() -> Self {
        FireBrowserMode { base: ParameterModeBase::without_controls("Browser", false, ModeBank::None) }
    }

    fn knob_index(ctx: &Ctx, index: usize) -> usize {
        if index < 8 {
            four_knob_index(ctx, index)
        } else {
            8
        }
    }

    fn select(ctx: &mut Ctx, index: usize, count: i32) {
        let Some(browser) = ctx.model.browser_mut() else {
            return;
        };
        let filter_column_count = browser.filter_column_count().min(7);
        for _ in 0..count.abs() {
            if index < filter_column_count {
                if !browser.filter_column(index).is_some_and(|c| c.exists) {
                    return;
                }
                if count > 0 {
                    browser.select_next_filter_item(index);
                } else {
                    browser.select_previous_filter_item(index);
                }
            } else if count > 0 {
                browser.select_next_result();
            } else {
                browser.select_previous_result();
            }
        }
    }
}

impl Default for FireBrowserMode {
    fn default() -> Self {
        Self::new()
    }
}

impl Mode for FireBrowserMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
        if let Some(browser) = ctx.model.browser_mut() {
            if browser.is_active() {
                browser.stop_browsing(true);
            }
        }
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        let idx = Self::knob_index(ctx, index);
        let mut speed = ctx.model.value_changer().calc_stepped_knob_change(value);
        if index == 8 && ctx.surface.is_select_pressed() {
            ctx.surface.set_trigger_consumed(crate::framework::ids::ButtonId::Select);
            speed *= 4;
        }
        Self::select(ctx, idx, speed);
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        let idx = Self::knob_index(ctx, index);
        // Make sure that only 1 knob gets changed in browse mode to prevent weird behavior
        if (0..8).any(|i| self.base.is_knob_touched(i) && i != idx) {
            return;
        }
        self.base.set_touched_knob(idx, is_touched);
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.previous_content_type();
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        if let Some(browser) = ctx.model.browser_mut() {
            browser.next_content_type();
        }
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        ctx.model.browser().is_some_and(|b| b.has_previous_content_type())
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        ctx.model.browser().is_some_and(|b| b.has_next_content_type())
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let Some(browser) = ctx.model.browser() else {
            return;
        };
        if !browser.is_active() {
            return;
        }
        let mut rows = vec![String::new(), String::new(), String::new()];
        let mut sels = vec![false; 3];
        match self.base.touched_knob() {
            None => {
                rows[0] = browser.selected_content_type().to_uppercase();
                rows[1] = "Selection: ".to_string();
                rows[2] = match browser.selected_result() {
                    Some(result) if !result.trim().is_empty() => result.to_string(),
                    _ => "None".to_string(),
                };
            }
            Some(8) => {
                let results = browser.result_items();
                if results.first().is_some_and(|r| r.exists) {
                    for item in 0..3 {
                        if let Some(result) = results.get(item) {
                            rows[item] = result.name.clone();
                            sels[item] = result.is_selected;
                        }
                    }
                } else {
                    rows = vec!["No".to_string(), "results ".to_string(), "available...".to_string()];
                }
            }
            Some(idx) => {
                if let Some(column) = browser.filter_column(idx).filter(|c| c.exists && c.cursor_exists) {
                    for (i, item) in column.items.iter().take(3).enumerate() {
                        let mut text = crate::daw::util::optimize_name(&item.name, 10);
                        if !text.is_empty() {
                            text = format!("{text} ({})", item.hit_count);
                        }
                        rows[i] = text;
                        sels[i] = item.is_selected;
                    }
                }
            }
        }
        if let Some(display) = ctx.graphics_display.as_mut() {
            display.add_list_element(&rows, &sels);
        }
    }

    impl_as_any!();
}
