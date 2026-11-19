// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device layer modes (former `DeviceLayerMode`, `DeviceLayerVolumeMode`,
//! `DeviceLayerPanMode` and `DeviceLayerSendMode`): one type with the kind of the mode. They
//! work on the layer or drum pad bank of the cursor device.
//!
//! Divergence: the Rust model has neither `ILayer.enter` nor `hasDevices` of a layer (only of a
//! drum pad): selecting a selected layer only switches to the device mode for a drum pad with
//! devices, without entering it.

use crate::config::push::LockState;
use crate::daw::channel::Channel;
use crate::daw::constants::ChannelType;
use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::Mode;
use crate::framework::graphics::{GraphicDisplay, SendData, GRID_ELEMENT_CHANNEL_PAN, GRID_ELEMENT_CHANNEL_VOLUME};
use crate::framework::ids::{ButtonId, Modes};
use crate::framework::mode::parameter_mode::{button_row_index, default_knob_ids, uses_layers, ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{provider, DrumDeviceRef, SharedProvider};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::colors::*;
use super::super::super::display1;
use super::super::super::parameterprovider::{
    cursor_layer, cursor_layer_mut, cursor_layers_exist, cursor_layer_page_size, drum_pad_offset, is_send, select_cursor_layer, selected_cursor_layer_index, toggle_send_enabled,
    PushLayerOrDrumPadParameterProvider, PushSelectedLayerOrDrumPadParameterProvider,
};
use super::super::super::surface as push;
use super::super::track::{color_off, set_cell_value, shorten_and_fix_ascii, trigger_button_later};
use super::super::{check_stop_automation_on_knob_release, update_display, PushDisplayMode};
use super::params::set_show_devices;

/// The kind of a layer mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    /// The selected layer with volume, panorama and sends (former `DeviceLayerMode`).
    Layer,
    /// The volumes of the layers (former `DeviceLayerVolumeMode`).
    Volume,
    /// The panoramas of the layers (former `DeviceLayerPanMode`).
    Pan,
    /// A send of the layers (former `DeviceLayerSendMode`).
    Send(usize),
}

/// Former `Modes.isLayerMode`.
pub fn is_layer_mode(mode: Modes) -> bool {
    (Modes::DeviceLayer.ordinal()..=Modes::DeviceLayerSend8.ordinal()).contains(&mode.ordinal())
}

/// A device layer mode.
pub struct DeviceLayerMode {
    pub base: ParameterModeBase,
    pub kind: LayerKind,
    menu: Vec<(String, bool)>,
}

impl DeviceLayerMode {
    pub fn new(ctx: &Ctx, kind: LayerKind) -> Self {
        let (name, parameter_provider): (&str, SharedProvider) = match kind {
            LayerKind::Layer => ("Layer", provider(PushSelectedLayerOrDrumPadParameterProvider)),
            LayerKind::Volume => ("Layer Volume", provider(PushLayerOrDrumPadParameterProvider::volume())),
            LayerKind::Pan => ("Layer Panning", provider(PushLayerOrDrumPadParameterProvider::pan())),
            LayerKind::Send(index) => ("Layer Sends", provider(PushLayerOrDrumPadParameterProvider::send(index))),
        };
        let mut base = ParameterModeBase::new(name, true, ModeBank::DrumPadBank(DrumDeviceRef::Cursor), default_knob_ids());
        base.set_parameter_provider(ctx, parameter_provider);
        DeviceLayerMode { base, kind, menu: vec![(" ".to_string(), false); 8] }
    }

    fn send_index(ctx: &Ctx, index: usize) -> usize {
        if push::is_modern(ctx) {
            index.wrapping_sub(4)
        } else {
            index.wrapping_sub(2)
        }
    }

    fn on_layer_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        let Some(selected) = selected_cursor_layer_index(&ctx.model) else {
            return;
        };
        self.base.set_touched_knob(index, is_touched);
        let is_modern = push::is_modern(ctx);
        let send_ok = !is_modern || index >= 4;

        if is_touched && ctx.surface.is_delete_pressed() {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
            let send = Self::send_index(ctx, index);
            let channel = cursor_layer_mut(&mut ctx.model, selected);
            match index {
                0 => channel.reset_volume(),
                1 => channel.reset_pan(),
                _ => {
                    if send_ok {
                        channel.send_bank_mut().item_mut(send).reset_value();
                    }
                }
            }
            return;
        }

        let send = Self::send_index(ctx, index);
        let channel = cursor_layer_mut(&mut ctx.model, selected);
        match index {
            0 => channel.touch_volume(is_touched),
            1 => channel.touch_pan(is_touched),
            _ => {
                if send_ok {
                    channel.send_bank_mut().item_mut(send).touch_value(is_touched);
                }
            }
        }
        check_stop_automation_on_knob_release(ctx, is_touched);
        self.toggle_send_on_touch(ctx, index, is_touched);
    }

    /// Toggle send enablement (Shift+Select+touch).
    fn toggle_send_on_touch(&self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        if is_touched && ctx.surface.is_shift_pressed() && ctx.surface.is_select_pressed() {
            let parameter = self.base.parameter(ctx, index);
            if is_send(&parameter) {
                ctx.surface.set_trigger_consumed(ButtonId::Select);
                toggle_send_enabled(ctx, &parameter);
            }
        }
    }

    fn on_channel_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        let layer_index = drum_pad_offset(&ctx.model) + index;
        if !cursor_layer(&ctx.model, layer_index).does_exist() {
            return;
        }
        let delete = is_touched && ctx.surface.is_delete_pressed();
        if delete {
            ctx.surface.set_trigger_consumed(ButtonId::Delete);
        }
        let channel = cursor_layer_mut(&mut ctx.model, layer_index);
        match self.kind {
            LayerKind::Volume => {
                if delete {
                    channel.reset_volume();
                }
                channel.touch_volume(is_touched);
            }
            LayerKind::Pan => {
                if delete {
                    channel.reset_pan();
                }
                channel.touch_pan(is_touched);
            }
            LayerKind::Send(send) => {
                let send = channel.send_bank_mut().item_mut(send);
                if delete {
                    send.reset_value();
                }
                send.touch_value(is_touched);
            }
            LayerKind::Layer => {}
        }
        check_stop_automation_on_knob_release(ctx, is_touched);
        if matches!(self.kind, LayerKind::Send(_)) {
            self.toggle_send_on_touch(ctx, index, is_touched);
        }
    }

    fn on_first_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event == ButtonEvent::Down {
            return;
        }

        if event == ButtonEvent::Up {
            if !ctx.model.cursor_device().does_exist() {
                return;
            }
            let offset = drum_pad_offset(&ctx.model);
            let layer = cursor_layer(&ctx.model, offset + index);
            if !layer.does_exist() {
                return;
            }
            let layer_index = layer.index().max(0) as usize;
            if !layer.is_selected() {
                select_cursor_layer(&mut ctx.model, layer_index);
                return;
            }

            // Only select if it exists otherwise the parent device is selected which is confusing
            // to the user
            let device = ctx.model.cursor_device();
            let has_devices = !uses_layers(device) && device.drum_pad_bank().item(offset + index).has_devices();
            if !has_devices {
                return;
            }
            // The model has no layer.enter ()
            Self::set_mode(ctx, Modes::DeviceParams);
            ctx.schedule(0, |ctx| set_show_devices(ctx, true));
            return;
        }

        // LONG press
        ctx.surface.set_trigger_consumed(ButtonId::get(ButtonId::Row11, index));
        Self::move_up(ctx);
    }

    /// Former `moveUp`.
    fn move_up(ctx: &mut Ctx) {
        // There is no device on the track move upwards to the track view
        if !ctx.model.cursor_device().does_exist() {
            trigger_button_later(ctx, ButtonId::Track);
            return;
        }
        Self::set_mode(ctx, Modes::DeviceParams);
        ctx.model.cursor_device_mut().select_channel();
        set_show_devices(ctx, true);
    }

    /// Former `setMode`.
    fn set_mode(ctx: &mut Ctx, layer_mode: Modes) {
        ctx.schedule(0, move |ctx| ctx.set_active_mode(layer_mode));
        if is_layer_mode(layer_mode) {
            push::set_layer_mix_mode(ctx, Some(layer_mode));
        }
    }

    /// Former `handleSendEffect`.
    fn handle_send_effect(ctx: &mut Ctx, send_index: usize) {
        if !cursor_layer(&ctx.model, 0).send_bank().item(send_index).does_exist() {
            return;
        }
        let si = Modes::get(Modes::DeviceLayerSend1, send_index);
        Self::set_mode(ctx, if ctx.modes.is_active(si) { Modes::DeviceLayer } else { si });
    }

    fn on_second_row(&mut self, ctx: &mut Ctx, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }

        let offset = drum_pad_offset(&ctx.model);
        if push::is_mute_state(ctx, ctx.surface.is_long_pressed(ButtonId::Mute)) {
            cursor_layer_mut(&mut ctx.model, offset + index).toggle_mute();
            return;
        }
        if push::is_solo_state(ctx, ctx.surface.is_long_pressed(ButtonId::Solo)) {
            cursor_layer_mut(&mut ctx.model, offset + index).toggle_solo();
            return;
        }

        match index {
            0 => Self::set_mode(ctx, if ctx.modes.is_active(Modes::DeviceLayerVolume) { Modes::DeviceLayer } else { Modes::DeviceLayerVolume }),
            1 => Self::set_mode(ctx, if ctx.modes.is_active(Modes::DeviceLayerPan) { Modes::DeviceLayer } else { Modes::DeviceLayerPan }),
            2 => {}
            3 => {
                let is_shift = ctx.surface.is_shift_pressed();
                for i in 0..cursor_layer_page_size(&ctx.model) {
                    let send_bank = cursor_layer_mut(&mut ctx.model, offset + i).send_bank_mut();
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
            }
            7 => {
                if ctx.surface.is_shift_pressed() {
                    Self::handle_send_effect(ctx, 3);
                } else {
                    Self::move_up(ctx);
                }
            }
            _ => Self::handle_send_effect(ctx, index - 4),
        }
    }

    /// Former `drawRow4`.
    fn draw_row4(ctx: &Ctx, display: &mut TextDisplay) {
        let offset = drum_pad_offset(&ctx.model);
        for i in 0..8 {
            let layer = cursor_layer(&ctx.model, offset + i);
            let n = shorten_and_fix_ascii(layer.name(), if layer.is_selected() { 7 } else { 8 });
            let text = if layer.is_selected() { format!("{}{n}", display1::SELECT_ARROW) } else { n };
            display.set_cell(3, i, &text);
        }
    }

    /// The message if there are no layers, returns false in that case (Push 1).
    fn check_layers1(ctx: &Ctx, display: &mut TextDisplay) -> bool {
        let cd = ctx.model.cursor_device();
        if !cd.has_layers() {
            display.set_block(1, 1, "    This device  ").set_block(1, 2, "does not have layers.");
            return false;
        }
        if !cursor_layers_exist(&ctx.model) {
            display.set_block(1, 1, "    Please create").set_block(1, 2, if cd.has_drum_pads() { "a Drum Pad..." } else { "a Device Layer..." });
            return false;
        }
        true
    }

    /// Former `checkLayerExistance`.
    fn check_layer_existance(ctx: &Ctx, display: &mut GraphicDisplay) -> bool {
        let cd = ctx.model.cursor_device();
        if !cd.has_layers() {
            for i in 0..8 {
                display.add_option_element(if i == 3 { "This device does not have layers." } else { "" }, if i == 7 { "Up" } else { "" }, true, "", "", false, true);
            }
            return false;
        }
        if cursor_layers_exist(&ctx.model) {
            return true;
        }
        for i in 0..8 {
            let label = if i == 3 { format!("Please create a {}", if cd.has_drum_pads() { "Drum Pad..." } else { "Device Layer..." }) } else { String::new() };
            display.add_option_element(&label, if i == 7 { "Up" } else { "" }, true, "", "", false, true);
        }
        false
    }

    /// Former `updateMenuItems`.
    fn update_menu_items(&mut self, ctx: &Ctx, selected_menu: i32) {
        if push::is_mute_state(ctx, ctx.surface.is_long_pressed(ButtonId::Mute)) {
            for i in 0..8 {
                let layer = cursor_layer(&ctx.model, i);
                self.menu[i] = (if layer.does_exist() { "Mute" } else { "" }.to_string(), layer.is_mute());
            }
        } else if push::is_solo_state(ctx, ctx.surface.is_long_pressed(ButtonId::Solo)) {
            for i in 0..8 {
                let layer = cursor_layer(&ctx.model, i);
                self.menu[i] = (if layer.does_exist() { "Solo" } else { "" }.to_string(), layer.is_solo());
            }
        } else {
            self.update_layer_menu(ctx, selected_menu);
        }
    }

    /// Former `updateLayerMenu`.
    ///
    /// Divergence: the model has no edit send names of the layer bank (former
    /// `ILayerBank.getEditSendName`), the names of the sends of the first layer are shown.
    fn update_layer_menu(&mut self, ctx: &Ctx, selected_menu: i32) {
        self.menu[0] = ("Volume".to_string(), selected_menu - 1 == 0);
        self.menu[1] = ("Pan".to_string(), selected_menu - 1 == 1);
        self.menu[2] = (" ".to_string(), false);

        let first = cursor_layer(&ctx.model, 0);
        let start = first.send_bank().item(0).position().max(0) + 1;
        self.menu[3] = (format!("Sends {}-{}", start, start + 3), false);
        for i in 0..4 {
            let send = first.send_bank().item(i);
            let send_name = crate::daw::util::optimize_name(if send.does_exist() { send.name() } else { "" }, 12);
            self.menu[4 + i] = (if send_name.is_empty() { " ".to_string() } else { send_name }, (4 + i) as i32 == selected_menu - 1);
        }

        if !ctx.surface.is_shift_pressed() && !self.base.is_knob_touched(7) {
            self.menu[7] = ("Up".to_string(), true);
        }
    }

    /// Former `updateDisplayElements` of `DeviceLayerMode`.
    fn update_display_elements(&mut self, ctx: &Ctx, display: &mut GraphicDisplay) {
        let selected = selected_cursor_layer_index(&ctx.model);

        // Drum Pad Bank has size of 16, layers only 8
        let offset = drum_pad_offset(&ctx.model) as i32;

        // Get the index at which to draw the Sends element
        let selected_layer = selected.map(|i| cursor_layer(&ctx.model, i));
        let mut sends_index = selected_layer.map_or(-1, |l| l.index() - offset + 1);
        if sends_index == 8 {
            sends_index = 6;
        }

        self.update_menu_items(ctx, -1);

        let vc = ctx.model.value_changer();
        let enable_vu_meters = ctx.config.is_enable_vu_meters();
        for i in 0..8 {
            let layer = cursor_layer(&ctx.model, offset as usize + i);
            let (top_menu, is_top_menu_on) = self.menu[i].clone();

            // Channel info
            let bottom_menu = if layer.does_exist() { layer.name_limited(12) } else { String::new() };
            let bottom_menu_color = layer.color();
            let is_bottom_menu_on = layer.is_selected();

            if layer.is_selected() {
                let vu_r = vc.to_display_value(if enable_vu_meters { layer.vu_right() } else { 0 });
                let vu_l = vc.to_display_value(if enable_vu_meters { layer.vu_left() } else { 0 });
                display.add_channel_element(
                    &top_menu,
                    is_top_menu_on,
                    &bottom_menu,
                    Some(ChannelType::Layer),
                    bottom_menu_color,
                    is_bottom_menu_on,
                    vc.to_display_value(layer.volume()),
                    vc.to_display_value(layer.modulated_volume()),
                    &if self.base.is_knob_touched(0) { layer.volume_str_limited(8) } else { String::new() },
                    vc.to_display_value(layer.pan()),
                    vc.to_display_value(layer.modulated_pan()),
                    &if self.base.is_knob_touched(1) { layer.pan_str_limited(8) } else { String::new() },
                    vu_l,
                    vu_r,
                    layer.is_mute(),
                    layer.is_solo(),
                    false,
                    layer.is_activated(),
                    0,
                    false,
                );
            } else if let (true, Some(selected_layer)) = (sends_index == i as i32, selected_layer) {
                let send_bank = selected_layer.send_bank();
                let mut send_data = Vec::with_capacity(4);
                for j in 0..4 {
                    let send = send_bank.item(j);
                    let does_exist = send.does_exist();
                    let text = if does_exist && self.base.is_knob_touched(4 + j) { send.displayed_value() } else { String::new() };
                    // As Java: the values are not converted to display values
                    send_data.push(SendData::new(
                        send.is_enabled(),
                        send.name(),
                        &text,
                        if does_exist { send.value() } else { 0 },
                        if does_exist { send.modulated_value() } else { 0 },
                        true,
                    ));
                }
                let name = if layer.does_exist() { layer.name().to_string() } else { String::new() };
                display.add_sends_element(&top_menu, is_top_menu_on, &name, Some(ChannelType::Layer), layer.color(), layer.is_selected(), &send_data, true, selected_layer.is_activated(), layer.is_activated());
            } else {
                display.add_channel_selector_element(&top_menu, is_top_menu_on, &bottom_menu, Some(ChannelType::Layer), bottom_menu_color, is_bottom_menu_on, layer.is_activated());
            }
        }
    }

    /// Former `updateChannelDisplay` (volume and panorama mode).
    fn update_channel_display(&mut self, ctx: &Ctx, display: &mut GraphicDisplay, selected_menu: i32, is_volume: bool, is_pan: bool) {
        self.update_menu_items(ctx, selected_menu);

        let offset = drum_pad_offset(&ctx.model);
        let vc = ctx.model.value_changer();
        let enable_vu_meters = ctx.config.is_enable_vu_meters();
        for i in 0..8 {
            let layer = cursor_layer(&ctx.model, offset + i);
            let (top_menu, is_top_menu_on) = self.menu[i].clone();
            let vu_r = vc.to_display_value(if enable_vu_meters { layer.vu_right() } else { 0 });
            let vu_l = vc.to_display_value(if enable_vu_meters { layer.vu_left() } else { 0 });
            display.add_channel_element_typed(
                selected_menu,
                &top_menu,
                is_top_menu_on,
                &if layer.does_exist() { layer.name().to_string() } else { String::new() },
                Some(ChannelType::Layer),
                layer.color(),
                layer.is_selected(),
                vc.to_display_value(layer.volume()),
                vc.to_display_value(layer.modulated_volume()),
                &if is_volume && self.base.is_knob_touched(i) { layer.volume_str_limited(8) } else { String::new() },
                vc.to_display_value(layer.pan()),
                vc.to_display_value(layer.modulated_pan()),
                &if is_pan && self.base.is_knob_touched(i) { layer.pan_str() } else { String::new() },
                vu_l,
                vu_r,
                layer.is_mute(),
                layer.is_solo(),
                false,
                layer.is_activated(),
                0,
                false,
            );
        }
    }

    /// Former `updateDisplayElements` of `DeviceLayerSendMode`.
    fn update_send_display_elements(&mut self, ctx: &Ctx, display: &mut GraphicDisplay, send_index: usize) {
        self.update_menu_items(ctx, 5 + (send_index % 4) as i32);

        let offset = drum_pad_offset(&ctx.model);
        for i in 0..8 {
            let layer = cursor_layer(&ctx.model, offset + i);
            let (top_menu, is_top_menu_on) = self.menu[i].clone();
            let send_bank = layer.send_bank();
            let mut send_data = Vec::with_capacity(4);
            for j in 0..4 {
                let send = send_bank.item(j);
                let exists = send.does_exist();
                let text = if exists && send_index == j && self.base.is_knob_touched(i) { send.displayed_value() } else { String::new() };
                send_data.push(SendData::new(send.is_enabled(), send.name(), &text, if exists { send.value() } else { 0 }, if exists { send.modulated_value() } else { 0 }, send_index == j));
            }
            let name = if layer.does_exist() { layer.name().to_string() } else { String::new() };
            display.add_sends_element(&top_menu, is_top_menu_on, &name, Some(ChannelType::Layer), layer.color(), layer.is_selected(), &send_data, false, layer.is_activated(), layer.is_activated());
        }
    }

    fn update_layer_display1(ctx: &Ctx, display: &mut TextDisplay) {
        let Some(selected) = selected_cursor_layer_index(&ctx.model) else {
            return;
        };
        let l: &Channel = cursor_layer(&ctx.model, selected);
        display.set_cell(0, 0, "Volume").set_cell(1, 0, &l.volume_str_limited(8));
        set_cell_value(ctx, display, 2, 0, if ctx.config.is_enable_vu_meters() { l.vu() } else { l.volume() }, Format::Value);
        display.set_cell(0, 1, "Pan").set_cell(1, 1, &l.pan_str_limited(8));
        set_cell_value(ctx, display, 2, 1, l.pan(), Format::Pan);
        let send_bank = l.send_bank();
        for i in 0..6 {
            let pos = 2 + i;
            let send = send_bank.item(i);
            display.set_cell(0, pos, send.name()).set_cell(1, pos, &send.displayed_value_limited(8));
            set_cell_value(ctx, display, 2, pos, send.value(), Format::Value);
        }
    }
}

impl PushDisplayMode for DeviceLayerMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let ctx = &*ctx;
        if self.kind == LayerKind::Layer && !ctx.model.cursor_device().does_exist() {
            display.set_block(1, 0, "           Select").set_block(1, 1, "a device or press").set_block(1, 2, "'Add Effect'...  ");
            return;
        }

        if Self::check_layers1(ctx, display) {
            let offset = drum_pad_offset(&ctx.model);
            match self.kind {
                LayerKind::Layer => Self::update_layer_display1(ctx, display),
                LayerKind::Volume => {
                    for i in 0..8 {
                        let layer = cursor_layer(&ctx.model, offset + i);
                        display.set_cell(0, i, if layer.does_exist() { "Volume" } else { "" }).set_cell(1, i, &layer.volume_str_limited(8));
                        if layer.does_exist() {
                            set_cell_value(ctx, display, 2, i, if ctx.config.is_enable_vu_meters() { layer.vu() } else { layer.volume() }, Format::Value);
                        }
                    }
                }
                LayerKind::Pan => {
                    for i in 0..8 {
                        let layer = cursor_layer(&ctx.model, offset + i);
                        display.set_cell(0, i, if layer.does_exist() { "Pan" } else { "" }).set_cell(1, i, &layer.pan_str_limited(8));
                        if layer.does_exist() {
                            set_cell_value(ctx, display, 2, i, layer.pan(), Format::Pan);
                        }
                    }
                }
                LayerKind::Send(send_index) => {
                    for i in 0..8 {
                        let layer = cursor_layer(&ctx.model, offset + i);
                        let exists = layer.does_exist();
                        let send = layer.send_bank().item(send_index);
                        display.set_cell(0, i, if exists { send.name() } else { "" }).set_cell(1, i, &send.displayed_value_limited(8));
                        if exists {
                            set_cell_value(ctx, display, 2, i, send.value(), Format::Value);
                        }
                    }
                }
            }
        }

        Self::draw_row4(ctx, display);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let ctx = &*ctx;
        if !ctx.model.cursor_device().does_exist() {
            for i in 0..8 {
                display.add_option_element(if i == 2 { "Please select a device or press 'Add Device'..." } else { "" }, if i == 7 { "Up" } else { "" }, true, "", "", false, true);
            }
            return;
        }

        if !Self::check_layer_existance(ctx, display) {
            return;
        }
        match self.kind {
            LayerKind::Layer => self.update_display_elements(ctx, display),
            LayerKind::Volume => self.update_channel_display(ctx, display, GRID_ELEMENT_CHANNEL_VOLUME, true, false),
            LayerKind::Pan => self.update_channel_display(ctx, display, GRID_ELEMENT_CHANNEL_PAN, false, true),
            LayerKind::Send(send_index) => self.update_send_display_elements(ctx, display, send_index),
        }
    }
}

impl Mode for DeviceLayerMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        let cd = ctx.model.cursor_device();
        if !cd.has_layers() {
            return color_off(ctx);
        }

        let is_modern = push::is_modern(ctx);
        // Drum Pad Bank has size of 16, layers only 8
        let offset = drum_pad_offset(&ctx.model);

        if let Some(index) = button_row_index(0, button) {
            let dl = cursor_layer(&ctx.model, offset + index);
            if dl.does_exist() && dl.is_activated() {
                if dl.is_selected() {
                    return if is_modern { PUSH2_COLOR_ORANGE_HI } else { PUSH1_COLOR_ORANGE_HI };
                }
                return if is_modern { PUSH2_COLOR_YELLOW_LO } else { PUSH1_COLOR_YELLOW_LO };
            }
            return color_off(ctx);
        }

        if let Some(index) = button_row_index(1, button) {
            let layer = cursor_layer(&ctx.model, offset + index);
            if is_modern {
                let is_mute_state = push::is_mute_state(ctx, ctx.surface.is_long_pressed(ButtonId::Mute));
                if is_mute_state || push::is_solo_state(ctx, ctx.surface.is_long_pressed(ButtonId::Solo)) {
                    if layer.does_exist() {
                        if is_mute_state {
                            if layer.is_mute() {
                                return PUSH2_COLOR2_AMBER_LO;
                            }
                        } else if layer.is_solo() {
                            return PUSH2_COLOR2_YELLOW_HI;
                        }
                    }
                    return PUSH2_COLOR_BLACK;
                }

                let white_if = |mode: Modes| if ctx.modes.is_active(mode) { PUSH2_COLOR2_WHITE } else { PUSH2_COLOR_BLACK };
                return match index {
                    0 => white_if(Modes::DeviceLayerVolume),
                    1 => white_if(Modes::DeviceLayerPan),
                    4..=7 => white_if(Modes::get(Modes::DeviceLayerSend1, index - 4)),
                    _ => PUSH2_COLOR_BLACK,
                };
            }

            if layer.does_exist() {
                if push::lock_state(ctx) == LockState::Mute {
                    return if layer.is_mute() { PUSH1_COLOR_BLACK } else { PUSH1_COLOR2_YELLOW_HI };
                }
                return if layer.is_solo() { PUSH1_COLOR2_BLUE_HI } else { PUSH1_COLOR2_GREY_LO };
            }
        }

        color_off(ctx)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        match self.kind {
            LayerKind::Layer => self.on_layer_knob_touch(ctx, index, is_touched),
            _ => self.on_channel_knob_touch(ctx, index, is_touched),
        }
    }

    fn is_any_knob_touched(&self) -> bool {
        self.base.is_any_knob_touched()
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row == 0 {
            self.on_first_row(ctx, index, event);
        } else {
            self.on_second_row(ctx, index, event);
        }
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.base.select_item(ctx, index);
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_previous_item_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.base.select_next_item_page(ctx);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_previous_item_page(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.base.has_next_item_page(ctx)
    }

    impl_as_any!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_modes() {
        assert!(is_layer_mode(Modes::DeviceLayer));
        assert!(is_layer_mode(Modes::DeviceLayerSend8));
        assert!(!is_layer_mode(Modes::DeviceLayerDetails));
        assert!(!is_layer_mode(Modes::DeviceParams));
    }
}
