// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Track modes (former `framework.mode.track`: `DefaultTrackMode`, `TrackSelectMode`,
//! `TrackMuteMode`, `TrackSoloMode`, `TrackRecArmMode`, `TrackStopClipMode`, `TrackVolumeMode`,
//! `TrackPanMode` and `TrackSendMode`).
//!
//! The Java class hierarchy (`AbstractParameterMode` -> `DefaultTrackMode` -> concrete mode) is
//! replaced by the generic [`DefaultTrackMode`] which delegates the mode specific parts to a
//! [`TrackModeBehavior`]. The concrete modes are type aliases with their own constructors, e.g.
//! `TrackMuteMode::new(ctx)`.
//!
//! Simplifications compared to Java:
//! - Modes created with controls (e.g. `TrackVolumeMode::new_with_controls`) bind the controls
//!   to the parameters of their provider (`VolumeParameterProvider` etc.) while active. Without
//!   controls (e.g. the Launchpad, which uses the modes for its button rows) the knob functions
//!   (`on_knob_value`, `knob_value`, `on_knob_touch`) work on the tracks directly.
//! - The bank is always the current track bank of the model (Java switches it with a track bank
//!   observer).
//! - The clip launcher navigator (former `model.getClipLauncherNavigator ()`, only created on
//!   request of the model setup) is given to the mode with [`DefaultTrackMode::set_navigator`].

use std::marker::PhantomData;

use crate::daw::bank::BankItem;
use crate::framework::clip_launcher_navigator::{navigate_tracks, select_track, SharedNavigator};
use crate::daw::track::Track;
use crate::framework::commands::clip::NewCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::ids::{ButtonId, ContinuousId};
use crate::framework::mode::parameter_mode::ParameterBinding;
use crate::framework::parameterprovider::{
    provider, PanParameterProvider, SelectedTrackParameterProvider, SendParameterProvider, SharedProvider, VolumeParameterProvider,
};
use crate::midi::ButtonEvent;

/// The mode specific functions of a track mode.
pub trait TrackModeBehavior: 'static {
    /// The function which is executed when a track button is pressed (former `executeMethod`).
    /// The default selects the track or expands/collapses the group.
    fn execute_method(ctx: &mut Ctx, track_index: usize) {
        track_mut(ctx, track_index).select_or_expand_group();
    }

    fn on_knob_value(_ctx: &mut Ctx, _track_index: usize, _value: i32, _is_absolute: bool, _send_index: i32) {}

    fn knob_value(_ctx: &Ctx, _track_index: usize, _send_index: i32) -> i32 {
        -1
    }

    fn on_knob_touch(_ctx: &mut Ctx, _track_index: usize, _is_touched: bool, _send_index: i32) {}

    /// The provider for the parameters which are bound to the controls of the mode.
    fn parameter_provider(_ctx: &Ctx, _send_index: i32) -> Option<SharedProvider> {
        None
    }
}

fn track_mut(ctx: &mut Ctx, index: usize) -> &mut Track {
    ctx.model.current_track_bank_mut().item_mut(index)
}

fn track(ctx: &Ctx, index: usize) -> &Track {
    ctx.model.current_track_bank().item(index)
}

/// The base of all track modes (former `DefaultTrackMode` and the used parts of
/// `AbstractParameterMode`).
pub struct DefaultTrackMode<B: TrackModeBehavior> {
    name: String,
    is_absolute: bool,
    /// The index of the send of a send mode, -1 otherwise.
    send_index: i32,
    /// If true, the bank is scrolled instead of selecting the previous/next item.
    pub scroll_bank: bool,
    /// The controls which are bound to the parameters of the provider of the behavior.
    pub binding: ParameterBinding,
    /// The clip launcher navigator, if the controller uses one.
    navigator: Option<SharedNavigator>,
    behavior: PhantomData<B>,
}

impl<B: TrackModeBehavior> DefaultTrackMode<B> {
    pub fn with_name(name: &str, is_absolute: bool, send_index: i32) -> Self {
        DefaultTrackMode { name: name.to_string(), is_absolute, send_index, scroll_bank: false, binding: ParameterBinding::new(Vec::new()), navigator: None, behavior: PhantomData }
    }

    /// A mode which binds the controls to the parameters of the provider of the behavior while
    /// it is active (former constructors with a list of controls).
    pub fn with_name_and_controls(ctx: &Ctx, name: &str, is_absolute: bool, send_index: i32, controls: Vec<ContinuousId>) -> Self {
        let mode = Self::with_name(name, is_absolute, send_index);
        if !controls.is_empty() {
            mode.binding.set_controls(controls);
            if let Some(provider) = B::parameter_provider(ctx, send_index) {
                mode.binding.set_provider(ctx, provider);
            }
        }
        mode
    }

    pub fn is_absolute(&self) -> bool {
        self.is_absolute
    }

    /// Select the tracks with the clip launcher navigator, which moves the selected slot as well.
    pub fn set_navigator(&mut self, navigator: SharedNavigator) {
        self.navigator = Some(navigator);
    }

    /// Former `AbstractParameterMode.setAbsolute` (used by Generic Flexi).
    pub fn set_absolute(&mut self, is_absolute: bool) {
        self.is_absolute = is_absolute;
    }

    /// Get the index of the track in the page; a negative index returns the selected track
    /// (former `getTrack`).
    pub fn track_index(ctx: &Ctx, index: i32) -> Option<usize> {
        if index < 0 {
            ctx.model.current_track_bank().selected_index()
        } else {
            Some(index as usize)
        }
    }

    /// The alternative function is active if Shift is pressed.
    fn is_alternative_function(ctx: &Ctx) -> bool {
        ctx.surface.is_shift_pressed()
    }

    /// Handle the Delete, Duplicate and New button combinations. Returns true if one was
    /// executed.
    fn is_button_combination(ctx: &mut Ctx, track_index: usize) -> bool {
        if ctx.is_button_combination(ButtonId::Delete) {
            track_mut(ctx, track_index).remove();
            return true;
        }

        if ctx.is_button_combination(ButtonId::Duplicate) {
            track_mut(ctx, track_index).duplicate();
            return true;
        }

        if ctx.is_button_combination(ButtonId::New) {
            let clip_length = track(ctx, track_index).index();
            NewCommand::with_fixed_length(clip_length).execute_now(ctx);
            return true;
        }

        false
    }
}

impl<B: TrackModeBehavior> Mode for DefaultTrackMode<B> {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.binding.set_active(true);
        self.binding.bind_controls(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.binding.set_active(false);
        self.binding.unbind_controls(ctx);
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down || row != 0 {
            return;
        }
        if !Self::is_button_combination(ctx, index) {
            B::execute_method(ctx, index);
        }
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, index: usize, value: i32) {
        B::on_knob_value(ctx, index, value, self.is_absolute, self.send_index);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        B::knob_value(ctx, index, self.send_index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        B::on_knob_touch(ctx, index, is_touched, self.send_index);
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        if let Some(navigator) = &self.navigator {
            select_track(navigator, ctx, index);
            return;
        }
        track_mut(ctx, index).select();
    }

    fn selected_item_name(&self, ctx: &Ctx) -> Option<String> {
        let track = ctx.model.current_track_bank().selected_item()?;
        track.does_exist().then(|| format!("{}: {}", track.position() + 1, track.name()))
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        if Self::is_alternative_function(ctx) {
            self.select_previous_item_page(ctx);
            return;
        }
        if let Some(navigator) = &self.navigator {
            navigate_tracks(navigator, ctx, true);
            return;
        }
        let bank = ctx.model.current_track_bank_mut();
        if self.scroll_bank {
            bank.scroll_backwards();
        } else {
            bank.select_previous_item();
        }
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        if Self::is_alternative_function(ctx) {
            self.select_next_item_page(ctx);
            return;
        }
        if let Some(navigator) = &self.navigator {
            navigate_tracks(navigator, ctx, false);
            return;
        }
        let bank = ctx.model.current_track_bank_mut();
        if self.scroll_bank {
            bank.scroll_forwards();
        } else {
            bank.select_next_item();
        }
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        ctx.model.current_track_bank_mut().select_previous_page();
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        ctx.model.current_track_bank_mut().select_next_page();
    }

    fn select_item_page(&mut self, ctx: &mut Ctx, page: usize) {
        let bank = ctx.model.current_track_bank_mut();
        let position = page * bank.page_size();
        bank.scroll_to(position, true);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        ctx.model.current_track_bank().can_scroll_backwards()
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        ctx.model.current_track_bank().can_scroll_forwards()
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        ctx.model.current_track_bank().can_scroll_page_backwards()
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        ctx.model.current_track_bank().can_scroll_page_forwards()
    }

    crate::impl_as_any!();
}

// ------------------------------------------------------------------------------------------
// Behaviors

/// Selects the track (former `TrackSelectMode`).
pub struct SelectBehavior;
impl TrackModeBehavior for SelectBehavior {}

/// Toggles mute (former `TrackMuteMode`).
pub struct MuteBehavior;
impl TrackModeBehavior for MuteBehavior {
    fn execute_method(ctx: &mut Ctx, track_index: usize) {
        track_mut(ctx, track_index).toggle_mute();
    }
}

/// Toggles solo (former `TrackSoloMode`).
pub struct SoloBehavior;
impl TrackModeBehavior for SoloBehavior {
    fn execute_method(ctx: &mut Ctx, track_index: usize) {
        track_mut(ctx, track_index).toggle_solo();
    }
}

/// Toggles record arm (former `TrackRecArmMode`).
pub struct RecArmBehavior;
impl TrackModeBehavior for RecArmBehavior {
    fn execute_method(ctx: &mut Ctx, track_index: usize) {
        track_mut(ctx, track_index).toggle_rec_arm();
    }
}

/// Stops the playing clip of the track, the alternative stop with Shift (former
/// `TrackStopClipMode`).
pub struct StopClipBehavior;
impl TrackModeBehavior for StopClipBehavior {
    fn execute_method(ctx: &mut Ctx, track_index: usize) {
        let is_alternative = ctx.surface.is_shift_pressed();
        track_mut(ctx, track_index).stop(is_alternative);
    }
}

/// Edits the volume with the knobs (former `TrackVolumeMode`).
pub struct VolumeBehavior;
impl TrackModeBehavior for VolumeBehavior {
    fn on_knob_value(ctx: &mut Ctx, track_index: usize, value: i32, is_absolute: bool, _send_index: i32) {
        let track = track_mut(ctx, track_index);
        if is_absolute {
            track.set_volume(value);
        } else {
            track.change_volume(value);
        }
    }

    fn knob_value(ctx: &Ctx, track_index: usize, _send_index: i32) -> i32 {
        track(ctx, track_index).volume()
    }

    fn parameter_provider(ctx: &Ctx, _send_index: i32) -> Option<SharedProvider> {
        Some(provider(VolumeParameterProvider::new(ctx)))
    }

    fn on_knob_touch(ctx: &mut Ctx, track_index: usize, is_touched: bool, _send_index: i32) {
        let is_delete = ctx.surface.is_delete_pressed();
        let track = track_mut(ctx, track_index);
        if is_touched && is_delete {
            track.reset_volume();
        }
        track.touch_volume(is_touched);
    }
}

/// Edits the panorama with the knobs (former `TrackPanMode`).
pub struct PanBehavior;
impl TrackModeBehavior for PanBehavior {
    fn on_knob_value(ctx: &mut Ctx, track_index: usize, value: i32, is_absolute: bool, _send_index: i32) {
        let track = track_mut(ctx, track_index);
        if is_absolute {
            track.set_pan(value);
        } else {
            track.change_pan(value);
        }
    }

    fn knob_value(ctx: &Ctx, track_index: usize, _send_index: i32) -> i32 {
        track(ctx, track_index).pan()
    }

    fn parameter_provider(ctx: &Ctx, _send_index: i32) -> Option<SharedProvider> {
        Some(provider(PanParameterProvider::new(ctx)))
    }

    fn on_knob_touch(ctx: &mut Ctx, track_index: usize, is_touched: bool, _send_index: i32) {
        if !track(ctx, track_index).does_exist() {
            return;
        }
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            track_mut(ctx, track_index).reset_pan();
        }
        track_mut(ctx, track_index).touch_pan(is_touched);
    }
}

/// Edits a send with the knobs (former `TrackSendMode`).
pub struct SendBehavior;
impl TrackModeBehavior for SendBehavior {
    fn on_knob_value(ctx: &mut Ctx, track_index: usize, value: i32, is_absolute: bool, send_index: i32) {
        let Ok(send_index) = usize::try_from(send_index) else {
            return;
        };
        let send = track_mut(ctx, track_index).send_bank_mut().item_mut(send_index);
        if is_absolute {
            send.set_value(value);
        } else {
            send.change_value(value);
        }
    }

    fn knob_value(ctx: &Ctx, track_index: usize, send_index: i32) -> i32 {
        match usize::try_from(send_index) {
            Ok(send_index) => track(ctx, track_index).send_bank().item(send_index).value(),
            Err(_) => -1,
        }
    }

    fn parameter_provider(ctx: &Ctx, send_index: i32) -> Option<SharedProvider> {
        (send_index >= 0).then(|| provider(SendParameterProvider::new(ctx, send_index, 0)))
    }

    fn on_knob_touch(ctx: &mut Ctx, track_index: usize, is_touched: bool, send_index: i32) {
        let Ok(send_index) = usize::try_from(send_index) else {
            return;
        };
        if !track(ctx, track_index).send_bank().item(send_index).does_exist() {
            return;
        }
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            track_mut(ctx, track_index).send_bank_mut().item_mut(send_index).reset_value();
        }
        track_mut(ctx, track_index).send_bank_mut().item_mut(send_index).touch_value(is_touched);
    }
}

/// Edits volume, panorama and the sends of the selected track with the knobs (former
/// `TrackMode`). The index of the knob functions is the parameter index: 0 = volume, 1 = pan,
/// 2.. = sends.
pub struct TrackBehavior;
impl TrackModeBehavior for TrackBehavior {
    fn on_knob_value(ctx: &mut Ctx, index: usize, value: i32, is_absolute: bool, _send_index: i32) {
        let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() else {
            return;
        };
        match index {
            0 => {
                if is_absolute {
                    track.set_volume(value);
                } else {
                    track.change_volume(value);
                }
            }
            1 => {
                if is_absolute {
                    track.set_pan(value);
                } else {
                    track.change_pan(value);
                }
            }
            _ => {
                let send_bank = track.send_bank_mut();
                if !send_bank.has_existing_items() {
                    return;
                }
                let send = send_bank.item_mut(index - 2);
                if is_absolute {
                    send.set_value(value);
                } else {
                    send.change_value(value);
                }
            }
        }
    }

    fn knob_value(ctx: &Ctx, index: usize, _send_index: i32) -> i32 {
        let Some(track) = ctx.model.current_track_bank().selected_item() else {
            return -1;
        };
        match index {
            0 => track.volume(),
            1 => track.pan(),
            _ => {
                let send_bank = track.send_bank();
                if send_bank.has_existing_items() {
                    send_bank.item(index - 2).value()
                } else {
                    0
                }
            }
        }
    }

    fn on_knob_touch(ctx: &mut Ctx, index: usize, is_touched: bool, _send_index: i32) {
        let is_delete = ctx.surface.is_delete_pressed();
        let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() else {
            return;
        };
        match index {
            0 => {
                if is_touched && is_delete {
                    track.reset_volume();
                }
                track.touch_volume(is_touched);
            }
            1 => {
                if is_touched && is_delete {
                    track.reset_pan();
                }
                track.touch_pan(is_touched);
            }
            _ => {
                let send_bank = track.send_bank_mut();
                if !send_bank.has_existing_items() {
                    return;
                }
                let item = send_bank.item_mut(index - 2);
                if is_touched && is_delete {
                    item.reset_value();
                }
                item.touch_value(is_touched);
            }
        }
    }

    fn parameter_provider(ctx: &Ctx, _send_index: i32) -> Option<SharedProvider> {
        Some(provider(SelectedTrackParameterProvider::new(ctx)))
    }
}

// ------------------------------------------------------------------------------------------
// The concrete modes

pub type TrackSelectMode = DefaultTrackMode<SelectBehavior>;
pub type TrackMuteMode = DefaultTrackMode<MuteBehavior>;
pub type TrackSoloMode = DefaultTrackMode<SoloBehavior>;
pub type TrackRecArmMode = DefaultTrackMode<RecArmBehavior>;
pub type TrackStopClipMode = DefaultTrackMode<StopClipBehavior>;
pub type TrackVolumeMode = DefaultTrackMode<VolumeBehavior>;
pub type TrackPanMode = DefaultTrackMode<PanBehavior>;
pub type TrackSendMode = DefaultTrackMode<SendBehavior>;
pub type TrackMode = DefaultTrackMode<TrackBehavior>;

impl DefaultTrackMode<TrackBehavior> {
    pub fn new(_ctx: &Ctx, is_absolute: bool) -> Self {
        Self::with_name("Track", is_absolute, -1)
    }

    pub fn new_with_controls(ctx: &Ctx, is_absolute: bool, controls: Vec<ContinuousId>) -> Self {
        Self::with_name_and_controls(ctx, "Track", is_absolute, -1, controls)
    }
}

impl DefaultTrackMode<SelectBehavior> {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name("Track Select", true, -1)
    }
}

impl DefaultTrackMode<MuteBehavior> {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name("Track Mute", true, -1)
    }
}

impl DefaultTrackMode<SoloBehavior> {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name("Track Solo", true, -1)
    }
}

impl DefaultTrackMode<RecArmBehavior> {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name("Track Rec Arm", true, -1)
    }
}

impl DefaultTrackMode<StopClipBehavior> {
    pub fn new(_ctx: &Ctx) -> Self {
        Self::with_name("Track Stop Clip", true, -1)
    }
}

impl DefaultTrackMode<VolumeBehavior> {
    pub fn new(_ctx: &Ctx, is_absolute: bool) -> Self {
        Self::with_name("Volume", is_absolute, -1)
    }

    pub fn new_with_controls(ctx: &Ctx, is_absolute: bool, controls: Vec<ContinuousId>) -> Self {
        Self::with_name_and_controls(ctx, "Volume", is_absolute, -1, controls)
    }
}

impl DefaultTrackMode<PanBehavior> {
    pub fn new(_ctx: &Ctx, is_absolute: bool) -> Self {
        Self::with_name("Panning", is_absolute, -1)
    }

    pub fn new_with_controls(ctx: &Ctx, is_absolute: bool, controls: Vec<ContinuousId>) -> Self {
        Self::with_name_and_controls(ctx, "Panning", is_absolute, -1, controls)
    }
}

impl DefaultTrackMode<SendBehavior> {
    /// A negative send index (as used by the Launchpad) creates a mode without a send.
    pub fn new(send_index: i32, _ctx: &Ctx, is_absolute: bool) -> Self {
        Self::with_name(&format!("Send {}", send_index + 1), is_absolute, send_index)
    }

    pub fn new_with_controls(send_index: i32, ctx: &Ctx, is_absolute: bool, controls: Vec<ContinuousId>) -> Self {
        Self::with_name_and_controls(ctx, &format!("Send {}", send_index + 1), is_absolute, send_index, controls)
    }

    pub fn send_index(&self) -> i32 {
        self.send_index
    }
}
