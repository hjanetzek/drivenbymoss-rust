// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The track (mix) modes of the Push (former `mode.track`) and the helpers which the Push modes
//! share.

pub mod add_track;
pub mod clip;
pub mod master;
pub mod mix;
pub mod track_details;

use crate::config::push as config;
use crate::config::values::{fix_ascii, CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_1, CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE, CURSOR_KEYS_TRACK_OPTION_SWAP};
use crate::daw::constants::{Capability, ChannelType};
use crate::daw::context::HostFeature;
use crate::daw::track::Track;
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::BUTTON_COLOR_OFF;
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::mode::parameter_mode::{button_row_index, default_knob_ids, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::SharedProvider;
use crate::framework::view::color_select_mode::ColorSelectMode;
use crate::midi::ButtonEvent;

use super::super::colors::*;
use super::super::display1;
use super::super::surface as push;
use super::{check_stop_automation_on_knob_release, BaseModeState};

/// Register the track modes: TRACK, TRACK_DETAILS, VOLUME, PAN, CROSSFADER, SEND1-8, MASTER, MASTER_TEMP, CLIP, ADD_TRACK.
pub fn register_track_modes(ctx: &mut Ctx) {
    let track = mix::MixMode::new(ctx, mix::MixKind::Track);
    ctx.register_mode(Modes::Track, track);
    ctx.register_mode(Modes::TrackDetails, track_details::TrackDetailsMode::new());
    let volume = mix::MixMode::new(ctx, mix::MixKind::Volume);
    ctx.register_mode(Modes::Volume, volume);
    let pan = mix::MixMode::new(ctx, mix::MixKind::Pan);
    ctx.register_mode(Modes::Pan, pan);
    let crossfade = mix::MixMode::new(ctx, mix::MixKind::Crossfade);
    ctx.register_mode(Modes::Crossfader, crossfade);
    for i in 0..8 {
        let send = mix::MixMode::new(ctx, mix::MixKind::Send(i));
        ctx.register_mode(Modes::get(Modes::Send1, i), send);
    }
    let master = master::MasterMode::new(ctx, false);
    ctx.register_mode(Modes::Master, master);
    let master_temp = master::MasterMode::new(ctx, true);
    ctx.register_mode(Modes::MasterTemp, master_temp);
    ctx.register_mode(Modes::Clip, clip::ClipMode::new());
    ctx.register_mode(Modes::AddTrack, add_track::AddTrackMode::new());
}

// ------------------------------------------------------------------------------------------
// Shared helpers

/// The functions of the encoder of the Push 3 (former `IPush3Encoder`), implemented by the clip
/// and the browser mode; see `misc::as_push3_encoder`.
pub use super::misc::Push3Encoder;

/// Former `host.supports (Capability)`. The host does not report its capabilities (yet): Bitwig
/// supports all of them, Reaper only the pinning. A host with the device commands (protocol
/// v2) is treated as Bitwig.
pub fn host_supports(ctx: &Ctx, capability: Capability) -> bool {
    if capability == Capability::HasPinning {
        return true;
    }
    ctx.model.ctx().host_supports(HostFeature::DeviceCommands)
}

/// Former `StringUtils.shortenAndFixASCII`.
pub fn shorten_and_fix_ascii(text: &str, length: usize) -> String {
    optimize_name(&fix_ascii(text), length)
}

/// Former `StringUtils.limit`.
pub fn limit(text: &str, length: usize) -> String {
    text.chars().take(length).collect()
}

/// Former `Push1Display.setCell (row, cell, value, format)`.
pub fn set_cell_value(ctx: &Ctx, display: &mut TextDisplay, row: usize, cell: usize, value: i32, format: Format) {
    display1::set_cell_value(display, row, cell, value, format, ctx.model.value_changer().upper_bound());
}

/// The volume of a channel as a bar, with the VU if enabled (former `Push1Display.formatValue
/// (volume, vu, upperBound)` / `formatValue (volume, upperBound)`).
pub fn format_volume(ctx: &Ctx, volume: i32, vu: i32) -> String {
    let upper_bound = ctx.model.value_changer().upper_bound();
    if ctx.config.is_enable_vu_meters() {
        display1::format_value_modulated(vu, volume, upper_bound)
    } else {
        display1::format_value(volume, upper_bound)
    }
}

/// Select the item to color and activate the color view (former `((ColorView) viewManager.get
/// (Views.COLOR)).setMode (mode); viewManager.setActive (Views.COLOR)`).
pub fn select_color_view(ctx: &mut Ctx, mode: ColorSelectMode) {
    if let Some(view) = ctx.views.get(Views::Color) {
        if let Ok(mut view) = view.try_borrow_mut() {
            view.set_color_select_mode(mode);
        }
    }
    ctx.set_active_view(Views::Color);
}

/// Activate a mode after the current mode has finished (modes are borrowed while they run,
/// former `modeManager.setActive (id)`).
pub fn set_mode_later(ctx: &mut Ctx, id: Modes) {
    ctx.schedule(0, move |ctx| ctx.set_active_mode(id));
}

/// Press a button as if it was pressed on the device, after the current mode has finished
/// (former `surface.getButton (id).trigger (ButtonEvent.DOWN)`: as in Java the button is not
/// released).
pub fn trigger_button_later(ctx: &mut Ctx, id: ButtonId) {
    ctx.schedule(0, move |ctx| ctx.trigger_button(id, ButtonEvent::Down, 1.0));
}

/// The color index of a color ID of the color manager.
pub fn color(ctx: &Ctx, id: &str) -> i32 {
    ctx.colors.color_index(id)
}

/// The default button color (former `AbstractFeatureGroup.getButtonColor`).
pub fn color_off(ctx: &Ctx) -> i32 {
    ctx.colors.color_index(BUTTON_COLOR_OFF)
}

/// The type of a track, `GroupOpen` for an expanded group (former `updateType`).
pub fn update_type(track: &Track) -> ChannelType {
    let channel_type = track.channel_type();
    if channel_type == ChannelType::Group && track.is_group_expanded() {
        ChannelType::GroupOpen
    } else {
        channel_type
    }
}

// ------------------------------------------------------------------------------------------
// AbstractTrackMode

/// The base of the track modes (former `AbstractTrackMode`). Embedded by the mix modes, the
/// clip mode and the session mode.
pub struct AbstractTrackMode {
    pub base: ParameterModeBase,
    pub state: BaseModeState,
    /// The top menu of the Push 2/3 (former `menu`).
    pub menu: Vec<(String, bool)>,
}

impl AbstractTrackMode {
    /// Former `AbstractTrackMode (name, surface, model)`; `provider` is the parameter provider
    /// which the sub-class sets.
    pub fn new(ctx: &Ctx, name: &str, provider: Option<SharedProvider>) -> Self {
        let mut base = ParameterModeBase::new(name, true, ModeBank::TrackBank, default_knob_ids());
        if let Some(provider) = provider {
            base.set_parameter_provider(ctx, provider);
        }
        AbstractTrackMode { base, state: BaseModeState::default(), menu: vec![(" ".to_string(), false); 8] }
    }

    /// Former `onKnobTouch`.
    pub fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        let parameter = self.base.parameter(ctx, index);
        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            parameter.reset_value(ctx);
        }
        parameter.touch_value(ctx, is_touched);
        check_stop_automation_on_knob_release(ctx, is_touched);
    }

    /// Former `BaseMode.onButton`.
    pub fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    /// Former `onFirstRow`.
    pub fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            return;
        }

        if event == ButtonEvent::Up {
            if ctx.surface.is_pressed(ButtonId::Duplicate) {
                ctx.surface.set_trigger_consumed(ButtonId::Duplicate);
                ctx.model.current_track_bank_mut().item_mut(index).duplicate();
                return;
            }
            if ctx.surface.is_pressed(ButtonId::Delete) {
                ctx.surface.set_trigger_consumed(ButtonId::Delete);
                ctx.model.current_track_bank_mut().item_mut(index).remove();
                return;
            }
            if ctx.surface.is_pressed(ButtonId::StopClip) {
                ctx.surface.set_trigger_consumed(ButtonId::StopClip);
                ctx.model.current_track_bank_mut().item_mut(index).stop(true);
                return;
            }
            if ctx.surface.is_pressed(ButtonId::Record) {
                ctx.surface.set_trigger_consumed(ButtonId::Record);
                ctx.model.current_track_bank_mut().item_mut(index).toggle_rec_arm();
                return;
            }
            if ctx.surface.is_select_pressed() {
                ctx.surface.set_trigger_consumed(ButtonId::Select);
                ctx.model.current_track_bank_mut().item_mut(index).toggle_multi_select();
                return;
            }
            let track = ctx.model.current_track_bank().item(index);
            if !track.is_selected() {
                ctx.model.current_track_bank_mut().item_mut(index).select();
                return;
            }

            // If it is a group display child channels of group, otherwise jump into device
            // mode
            if track.is_group() {
                if ctx.surface.is_shift_pressed() || ctx.config.is_track_navigation_flat() {
                    ctx.model.current_track_bank_mut().item_mut(index).toggle_group_expanded();
                } else {
                    ctx.model.current_track_bank_mut().item_mut(index).set_group_expanded(true);
                    ctx.model.current_track_bank_mut().enter(index);
                }
            } else {
                trigger_button_later(ctx, ButtonId::Device);
            }
            return;
        }

        // LONG press, go out of group
        ctx.model.track_bank_mut().select_parent();
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Row11, index));
    }

    /// Former `onSecondRow`.
    pub fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }

        if push::is_mute_state(ctx, ctx.surface.is_long_pressed(ButtonId::Mute)) {
            ctx.surface.set_trigger_consumed(ButtonId::Mute);
            ctx.model.current_track_bank_mut().item_mut(index).toggle_mute();
            return;
        }
        if push::is_solo_state(ctx, ctx.surface.is_long_pressed(ButtonId::Solo)) {
            ctx.surface.set_trigger_consumed(ButtonId::Solo);
            ctx.model.current_track_bank_mut().item_mut(index).toggle_solo();
            return;
        }
        if push::is_clip_stop_state(ctx, ctx.surface.is_long_pressed(ButtonId::StopClip)) {
            ctx.surface.set_trigger_consumed(ButtonId::StopClip);
            let is_shift = ctx.surface.is_shift_pressed();
            let track = ctx.model.current_track_bank_mut().item_mut(index);
            if is_shift {
                track.return_to_arrangement();
            } else {
                track.stop(is_shift);
            }
            return;
        }

        let toggle = |ctx: &Ctx, mode: Modes| if ctx.modes.is_active(mode) { Modes::Track } else { mode };
        let new_mode = match index {
            0 => Some(toggle(ctx, Modes::Volume)),
            1 => Some(toggle(ctx, Modes::Pan)),
            2 => Some(toggle(ctx, Modes::Crossfader)),
            3 => {
                let is_shift = ctx.surface.is_shift_pressed();
                let tb = ctx.model.current_track_bank_mut();
                for i in 0..tb.page_size() {
                    let send_bank = tb.item_mut(i).send_bank_mut();
                    if is_shift {
                        if send_bank.can_scroll_page_backwards() {
                            send_bank.select_previous_page();
                        } else {
                            let position = send_bank.item_count() / 4 * 4;
                            send_bank.scroll_to(position, false);
                        }
                    } else if send_bank.can_scroll_page_forwards() {
                        send_bank.select_next_page();
                    } else {
                        send_bank.scroll_to(0, false);
                    }
                }
                self.base.bind_controls(ctx);
                None
            }
            7 => {
                if self.last_send_is_accessible(ctx) {
                    self.send_effect_mode(ctx, 3)
                } else {
                    ctx.model.track_bank_mut().select_parent();
                    None
                }
            }
            _ => self.send_effect_mode(ctx, index - 4),
        };

        // Former configuration.setMixerMode (modeManager.getActiveID ()) after the mode change
        ctx.schedule(0, move |ctx| {
            if let Some(mode) = new_mode {
                ctx.set_active_mode(mode);
            }
            if let Some(active) = ctx.modes.active_id() {
                config::set_mixer_mode(&mut ctx.config, active);
            }
        });
    }

    /// The mode to activate for the selection of a send effect (former `handleSendEffect`).
    fn send_effect_mode(&self, ctx: &Ctx, send_index: usize) -> Option<Modes> {
        if !ctx.model.current_track_bank().can_edit_send(send_index) {
            return None;
        }
        let si = Modes::get(Modes::Send1, send_index);
        Some(if ctx.modes.is_active(si) { Modes::Track } else { si })
    }

    /// Former `handleSendEffect`.
    pub fn handle_send_effect(&self, ctx: &mut Ctx, send_index: usize) {
        if let Some(mode) = self.send_effect_mode(ctx, send_index) {
            set_mode_later(ctx, mode);
        }
    }

    fn cursor_keys_option(ctx: &Ctx) -> i32 {
        if ctx.surface.is_shift_pressed() {
            ctx.config.cursor_keys_track_shifted_option()
        } else {
            ctx.config.cursor_keys_track_option()
        }
    }

    /// Former `selectPreviousItemPage`.
    pub fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        match Self::cursor_keys_option(ctx) {
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE => self.base.select_previous_item_page(ctx),
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_1 => ctx.model.current_track_bank_mut().scroll_backwards(),
            CURSOR_KEYS_TRACK_OPTION_SWAP => ctx.model.cursor_track_mut().swap_with_previous(),
            _ => {}
        }
    }

    /// Former `selectNextItemPage`.
    pub fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        match Self::cursor_keys_option(ctx) {
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE => self.base.select_next_item_page(ctx),
            CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_1 => ctx.model.current_track_bank_mut().scroll_forwards(),
            CURSOR_KEYS_TRACK_OPTION_SWAP => ctx.model.cursor_track_mut().swap_with_next(),
            _ => {}
        }
    }

    /// Former `hasPreviousItem`.
    pub fn has_previous_item(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_shifted_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_previous_item_page(ctx)
        } else {
            self.base.has_previous_item(ctx)
        }
    }

    /// Former `hasNextItem`.
    pub fn has_next_item(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_shifted_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_next_item_page(ctx)
        } else {
            self.base.has_next_item(ctx)
        }
    }

    /// Former `hasPreviousItemPage`.
    pub fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_previous_item_page(ctx)
        } else {
            self.base.has_previous_item(ctx)
        }
    }

    /// Former `hasNextItemPage`.
    pub fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        if ctx.config.cursor_keys_track_option() == CURSOR_KEYS_TRACK_OPTION_MOVE_BANK_BY_PAGE {
            self.base.has_next_item_page(ctx)
        } else {
            self.base.has_next_item(ctx)
        }
    }

    /// Former `getButtonColor`.
    pub fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let tb = ctx.model.current_track_bank();

        if let Some(index) = button_row_index(0, button) {
            let track = tb.item(index);
            if !track.does_exist() || !track.is_activated() {
                return color(ctx, PUSH_BLACK);
            }
            let cursor_track = ctx.model.cursor_track();
            let sel_index = if cursor_track.does_exist() { cursor_track.index() } else { -1 };
            let is_sel = track.index() == sel_index;
            if track.is_rec_arm() {
                return color(ctx, if is_sel { PUSH_RED_HI } else { PUSH_RED_LO });
            }
            return color(ctx, if is_sel { PUSH_ORANGE_HI } else { PUSH_YELLOW_LO });
        }

        if let Some(index) = button_row_index(1, button) {
            let track = tb.item(index);

            if push::is_modern(ctx) {
                if push::is_solo_state(ctx, ctx.surface.is_long_pressed(ButtonId::Solo)) {
                    return if track.does_exist() && track.is_solo() { PUSH2_COLOR2_YELLOW_HI } else { PUSH2_COLOR_BLACK };
                }
                if push::is_mute_state(ctx, ctx.surface.is_long_pressed(ButtonId::Mute)) {
                    return if track.does_exist() && track.is_mute() { PUSH2_COLOR2_AMBER_LO } else { PUSH2_COLOR_BLACK };
                }
                if push::is_clip_stop_state(ctx, ctx.surface.is_long_pressed(ButtonId::StopClip)) {
                    return if track.does_exist() && track.is_playing() { PUSH2_COLOR_RED_HI } else { PUSH2_COLOR_BLACK };
                }
                let white_if = |mode: Modes| if ctx.modes.is_active(mode) { PUSH2_COLOR2_WHITE } else { PUSH2_COLOR_BLACK };
                return match index {
                    0 => white_if(Modes::Volume),
                    1 => white_if(Modes::Pan),
                    2 => white_if(Modes::Crossfader),
                    4 => white_if(Modes::Send1),
                    5 => white_if(Modes::Send2),
                    6 => white_if(Modes::Send3),
                    7 => {
                        if self.last_send_is_accessible(ctx) {
                            white_if(Modes::Send4)
                        } else if tb.has_parent() {
                            PUSH2_COLOR2_WHITE
                        } else {
                            PUSH2_COLOR_BLACK
                        }
                    }
                    _ => PUSH2_COLOR_BLACK,
                };
            }

            if !track.does_exist() {
                return PUSH1_COLOR_BLACK;
            }

            match push::lock_state(ctx) {
                config::LockState::Mute => return if track.is_mute() { PUSH1_COLOR_BLACK } else { PUSH1_COLOR2_YELLOW_HI },
                config::LockState::Solo => return if track.is_solo() { PUSH1_COLOR2_BLUE_HI } else { PUSH1_COLOR2_GREY_LO },
                config::LockState::ClipStop => return PUSH1_COLOR2_ROSE,
                _ => {}
            }
        }

        color_off(ctx)
    }

    /// Former `drawRow4`: the track names.
    pub fn draw_row4(&self, ctx: &Ctx, display: &mut TextDisplay) {
        let tb = ctx.model.current_track_bank();
        let sel_index = tb.selected_item().map_or(-1, |t| t.index());
        for i in 0..8 {
            let is_sel = i as i32 == sel_index;
            let t = tb.item(i);
            let mut track_name = t.name().to_string();
            if t.does_exist() && t.is_group() {
                let prefix = if t.is_group_expanded() { display1::THREE_ROWS } else { display1::FOLDER };
                track_name = format!("{prefix}{track_name}");
            }
            let n = shorten_and_fix_ascii(&track_name, if is_sel { 7 } else { 8 });
            let text = if is_sel { format!("{}{n}", display1::SELECT_ARROW) } else { n };
            display.set_cell(3, i, &text);
        }
    }

    /// Former `updateChannelDisplay` (called from the sub-classes).
    pub fn update_channel_display(&mut self, ctx: &Ctx, display: &mut GraphicDisplay, selected_menu: i32, is_volume: bool, is_pan: bool) {
        self.update_menu_items(ctx, selected_menu);

        let vc = ctx.model.value_changer();
        let tb = ctx.model.current_track_bank();
        let enable_vu_meters = ctx.config.is_enable_vu_meters();
        let is_pinned = ctx.model.is_cursor_track_pinned();
        for i in 0..8 {
            let t = tb.item(i);
            let (top_menu, is_top_menu_on) = self.menu[i].clone();
            let crossfade_mode = self.crossfade_mode_as_number(ctx, t);
            let vu_r = vc.to_display_value(if enable_vu_meters { t.vu_right() } else { 0 });
            let vu_l = vc.to_display_value(if enable_vu_meters { t.vu_left() } else { 0 });
            display.add_channel_element_typed(
                selected_menu,
                &top_menu,
                is_top_menu_on,
                &if t.does_exist() { t.name_limited(12) } else { String::new() },
                Some(update_type(t)),
                t.color(),
                t.is_selected(),
                vc.to_display_value(t.volume()),
                vc.to_display_value(t.modulated_volume()),
                &if is_volume && self.base.is_knob_touched(i) { t.volume_str_limited(8) } else { String::new() },
                vc.to_display_value(t.pan()),
                vc.to_display_value(t.modulated_pan()),
                &if is_pan && self.base.is_knob_touched(i) { t.pan_str_limited(8) } else { String::new() },
                vu_l,
                vu_r,
                t.is_mute(),
                t.is_solo(),
                t.is_rec_arm(),
                t.is_activated(),
                crossfade_mode,
                t.is_selected() && is_pinned,
            );
        }
    }

    /// Former `updateMenuItems`.
    pub fn update_menu_items(&mut self, ctx: &Ctx, selected_menu: i32) {
        if push::is_mute_state(ctx, ctx.surface.is_long_pressed(ButtonId::Mute)) {
            self.update_mute_menu(ctx);
        } else if push::is_solo_state(ctx, ctx.surface.is_long_pressed(ButtonId::Solo)) {
            self.update_solo_menu(ctx);
        } else if push::is_clip_stop_state(ctx, ctx.surface.is_long_pressed(ButtonId::StopClip)) {
            self.update_stop_menu(ctx);
        } else {
            self.update_track_menu(ctx, selected_menu);
        }
    }

    /// Former `updateStopMenu`.
    pub fn update_stop_menu(&mut self, ctx: &Ctx) {
        let tb = ctx.model.current_track_bank();
        let label = if ctx.surface.is_shift_pressed() { "Return to Arr." } else { "Stop Clip" };
        for i in 0..8 {
            let t = tb.item(i);
            self.menu[i] = (if t.does_exist() { label.to_string() } else { String::new() }, t.is_playing());
        }
    }

    /// Former `updateMuteMenu`.
    pub fn update_mute_menu(&mut self, ctx: &Ctx) {
        let tb = ctx.model.current_track_bank();
        for i in 0..8 {
            let t = tb.item(i);
            self.menu[i] = (if t.does_exist() { "Mute".to_string() } else { String::new() }, t.is_mute());
        }
    }

    /// Former `updateSoloMenu`.
    pub fn update_solo_menu(&mut self, ctx: &Ctx) {
        let tb = ctx.model.current_track_bank();
        for i in 0..8 {
            let t = tb.item(i);
            self.menu[i] = (if t.does_exist() { "Solo".to_string() } else { String::new() }, t.is_solo());
        }
    }

    /// Former `updateTrackMenu`.
    pub fn update_track_menu(&mut self, ctx: &Ctx, selected_menu: i32) {
        self.menu[0] = ("Volume".to_string(), selected_menu - 1 == 0);
        self.menu[1] = ("Pan".to_string(), selected_menu - 1 == 1);
        let crossfader = if host_supports(ctx, Capability::HasCrossfader) { "Crossfader" } else { " " };
        self.menu[2] = (crossfader.to_string(), selected_menu - 1 == 2);

        let tb = ctx.model.current_track_bank();
        let send_bank = tb.selected_item().unwrap_or_else(|| tb.item(0)).send_bank();
        let start = send_bank.scroll_position().max(0) + 1;
        self.menu[3] = (format!("Sends {}-{}", start, start + 3), false);

        for i in 0..4 {
            let send_name = tb.edit_send_name(i);
            let exists = !send_name.is_empty();
            self.menu[4 + i] = (if exists { send_name } else { " ".to_string() }, exists && (4 + i) as i32 == selected_menu - 1);
        }

        if self.last_send_is_accessible(ctx) {
            return;
        }

        let is_up_available = tb.has_parent();
        self.menu[7] = (if is_up_available { "Up" } else { " " }.to_string(), is_up_available);
    }

    /// Check if the 4th/8th send is accessible. This is the case if the current tracks are not
    /// inside a group (hence no need to go up), Shift is pressed or the 8th knob is touched
    /// (former `lastSendIsAccessible`).
    pub fn last_send_is_accessible(&self, ctx: &Ctx) -> bool {
        ctx.surface.is_shift_pressed() || !ctx.model.current_track_bank().has_parent() || self.base.is_knob_touched(7)
    }

    /// Former `getCrossfadeModeAsNumber`: 0 = A, 1 = AB, 2 = B, -1 = no crossfader.
    ///
    /// Divergence: the Rust model has no crossfade mode parameter of a track; without it the
    /// mode is shown as AB.
    pub fn crossfade_mode_as_number(&self, ctx: &Ctx, track: &Track) -> i32 {
        if !host_supports(ctx, Capability::HasCrossfader) {
            return -1;
        }
        match track.crossfade_parameter() {
            Some(parameter) => (ctx.model.value_changer().to_normalized_value(parameter.value() as f64) * 2.0).round() as i32,
            None => 1,
        }
    }
}
